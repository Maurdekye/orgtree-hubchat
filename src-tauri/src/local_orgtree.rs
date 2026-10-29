//! The Orgtree install on this PC (user 2026-10-29: Contact info's "Orgtree
//! org" badge opens a local org in Orgtree).
//!
//! Orgtree's engine keeps `local-orgs.json` in its data root,
//! `%APPDATA%\Orgtree v2\data`: `{"v":1,"orgs":[{"slug","name","address"}]}`,
//! the orgs it hosts that have a hub address, nothing secret. A contact is
//! local only when its address is listed there AND an Orgtree install is
//! registered. Opening starts that registered `Orgtree.exe` with exactly one
//! argument, `--orgtree-taskbar=org:<slug>`, Orgtree's own taskbar entry
//! point: a running Orgtree opens or focuses that org through its single
//! instance, a stopped one starts and opens it. No network, no Orgtree token,
//! no shell; the program comes from the registry, never from the file.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// One org the Orgtree install on this PC hosts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalOrg {
    pub slug: String,
    pub name: String,
    pub address: String,
}

/// The fields of one Uninstall registry entry that name a program.
#[derive(Debug, Clone, Default)]
pub struct Uninstall {
    pub display_name: String,
    pub display_icon: String,
    pub install_location: String,
}

/// The listing's name in Orgtree's data root.
pub const FILE: &str = "local-orgs.json";
/// Far more than any real listing; a larger file is not read.
const MAX_BYTES: u64 = 256 * 1024;
/// Orgtree's own cap for an org in a taskbar target.
const MAX_SLUG: usize = 128;

/// Orgtree's org slugs: lowercase letters, digits and single hyphens, none
/// at either end (Orgtree's `org_slug`), at most 128 characters.
pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= MAX_SLUG
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The orgs in a local-orgs.json: only entries with an Orgtree slug and an
/// address; nothing from another version or a damaged file.
pub fn parse(bytes: &[u8]) -> Vec<LocalOrg> {
    if bytes.len() as u64 > MAX_BYTES {
        return Vec::new();
    }
    let Ok(listing) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Vec::new();
    };
    if listing["v"].as_i64() != Some(1) {
        return Vec::new();
    }
    listing["orgs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|org| {
            let slug = org["slug"].as_str().filter(|s| valid_slug(s))?;
            let address = org["address"].as_str().filter(|a| !a.is_empty() && a.len() <= 256)?;
            let name = org["name"].as_str().unwrap_or_default();
            Some(LocalOrg { slug: slug.into(), name: name.into(), address: address.into() })
        })
        .collect()
}

/// "Orgtree", or "Orgtree <version>" as electron-builder names a release or
/// alpha. A development channel ("Orgtree Dev ...") keeps its own data root.
fn is_orgtree(display_name: &str) -> bool {
    let Some(rest) = display_name.trim().strip_prefix("Orgtree") else { return false };
    let Some(version) = rest.strip_prefix(' ') else { return rest.is_empty() };
    version.starts_with(|c: char| c.is_ascii_digit())
        && version.split('.').count() >= 3
        && version.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// The program an Uninstall entry names: its DisplayIcon ("path,index",
/// perhaps quoted), else Orgtree.exe in its InstallLocation.
fn program(entry: &Uninstall) -> Option<PathBuf> {
    let icon = entry.display_icon.trim();
    let icon = match icon.rsplit_once(',') {
        Some((path, index)) if index.trim().parse::<i32>().is_ok() => path,
        _ => icon,
    };
    let icon = icon.trim().trim_matches('"');
    if !icon.is_empty() {
        return Some(PathBuf::from(icon));
    }
    let dir = entry.install_location.trim().trim_matches('"');
    (!dir.is_empty()).then(|| Path::new(dir).join("Orgtree.exe"))
}

/// The registered Orgtree.exe: the first Orgtree Uninstall entry whose
/// program is an absolute path to an existing file named Orgtree.exe.
pub fn orgtree_exe(entries: &[Uninstall], exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    entries
        .iter()
        .filter(|entry| is_orgtree(&entry.display_name))
        .filter_map(program)
        .find(|exe| {
            exe.is_absolute()
                && exe.file_name().is_some_and(|name| name.eq_ignore_ascii_case("Orgtree.exe"))
                && exists(exe)
        })
}

/// The arguments Orgtree is started with for an org: only its taskbar
/// target (Orgtree's main/jumplist.ts takes exactly one).
pub fn launch_args(slug: &str) -> Option<Vec<String>> {
    valid_slug(slug).then(|| vec![format!("--orgtree-taskbar=org:{slug}")])
}

/// Start `exe` on an org and let it go: no shell, no waiting, no pipes.
/// Orgtree outlives Hubchat; its single instance takes the request over
/// when it is already running.
pub fn launch(exe: &Path, slug: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    let args = launch_args(slug).ok_or_else(|| "not an Orgtree org".to_string())?;
    let mut command = Command::new(exe);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command.spawn().map(drop).map_err(|_| "Orgtree could not be started".to_string())
}

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

/// Orgtree's Uninstall entries for this user and this machine (both views).
fn uninstall_entries() -> Vec<Uninstall> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    use winreg::RegKey;
    let roots = [
        (HKEY_CURRENT_USER, KEY_READ),
        (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_64KEY),
        (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_32KEY),
    ];
    let mut entries = Vec::new();
    for (root, flags) in roots {
        let Ok(base) = RegKey::predef(root).open_subkey_with_flags(UNINSTALL, flags) else { continue };
        for name in base.enum_keys().flatten() {
            let Ok(key) = base.open_subkey_with_flags(&name, flags) else { continue };
            let value = |v: &str| key.get_value::<String, _>(v).unwrap_or_default();
            let display_name = value("DisplayName");
            if is_orgtree(&display_name) {
                entries.push(Uninstall { display_name, display_icon: value("DisplayIcon"),
                                         install_location: value("InstallLocation") });
            }
        }
    }
    entries
}

/// local-orgs.json of a release Orgtree, under the roaming app data folder.
fn listing(roaming: &Path) -> Vec<LocalOrg> {
    let path = roaming.join("Orgtree v2").join("data").join(FILE);
    match std::fs::metadata(&path) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_BYTES => std::fs::read(&path).map(|b| parse(&b)).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The orgs this PC's Orgtree hosts, when an Orgtree install is registered;
/// none otherwise (a listing an uninstalled Orgtree left behind included).
pub fn local_orgs(roaming: &Path) -> Vec<LocalOrg> {
    if orgtree_exe(&uninstall_entries(), Path::is_file).is_none() {
        return Vec::new();
    }
    listing(roaming)
}

/// Start the registered Orgtree on an org its listing holds now.
pub fn open(roaming: &Path, slug: &str) -> Result<(), String> {
    if !listing(roaming).iter().any(|org| org.slug == slug) {
        return Err("That org isn't on this PC's Orgtree".into());
    }
    let exe = orgtree_exe(&uninstall_entries(), Path::is_file).ok_or_else(|| "Orgtree isn't installed".to_string())?;
    let started = launch(&exe, slug);
    match &started {
        Ok(()) => tracing::info!("started Orgtree on a local org"),
        Err(_) => tracing::warn!("Orgtree could not be started"),
    }
    started
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = r#"{"v":1,"orgs":[
        {"slug":"alpha-works","name":"Alpha Works","address":"alpha-works.someone.1a2b3c"},
        {"slug":"gamma","name":"Gamma Lab","address":"old-name.someone.4d5e6f"}]}"#;

    fn org(slug: &str, name: &str, address: &str) -> LocalOrg {
        LocalOrg { slug: slug.into(), name: name.into(), address: address.into() }
    }

    #[test]
    fn org_slugs_are_lowercase_letters_digits_and_hyphens() {
        for ok in ["alpha", "alpha-works", "a1", "x"] {
            assert!(valid_slug(ok), "{ok}");
        }
        for bad in ["", "Alpha", "alpha works", "alpha/../x", "a@b", "a\"b", "--x", "alpha:org", &"a".repeat(129), "alpha-"] {
            assert!(!valid_slug(bad), "{bad}");
        }
    }

    #[test]
    fn parse_reads_the_listing() {
        assert_eq!(parse(LISTING.as_bytes()), vec![
            org("alpha-works", "Alpha Works", "alpha-works.someone.1a2b3c"),
            org("gamma", "Gamma Lab", "old-name.someone.4d5e6f"),
        ]);
    }

    #[test]
    fn parse_keeps_only_entries_it_can_open_and_match() {
        let mixed = r#"{"v":1,"orgs":[
            {"slug":"Bad Slug","name":"x","address":"x.someone.111111"},
            {"slug":"no-address","name":"x","address":""},
            {"slug":"ok","name":"Ok","address":"ok.someone.222222"},
            {"slug":"no-name","address":"no-name.someone.333333"},
            "not an object"]}"#;
        assert_eq!(parse(mixed.as_bytes()), vec![
            org("ok", "Ok", "ok.someone.222222"),
            org("no-name", "", "no-name.someone.333333"),
        ]);
    }

    #[test]
    fn parse_refuses_another_version_or_a_damaged_file() {
        assert!(parse(br#"{"v":2,"orgs":[{"slug":"a","name":"A","address":"a.b.c"}]}"#).is_empty());
        assert!(parse(br#"{"orgs":[{"slug":"a","name":"A","address":"a.b.c"}]}"#).is_empty());
        assert!(parse(b"{\"v\":1,\"orgs\":[").is_empty());
        assert!(parse(b"").is_empty());
    }

    fn entry(name: &str, icon: &str, location: &str) -> Uninstall {
        Uninstall { display_name: name.into(), display_icon: icon.into(), install_location: location.into() }
    }

    #[test]
    fn the_registered_orgtree_is_found_by_its_uninstall_entry() {
        let exe = PathBuf::from(r"C:\Program Files\Orgtree\Orgtree.exe");
        let there = |p: &Path| p == exe;
        // as electron-builder registers it: "Orgtree <version>", the icon is the exe
        let release = entry("Orgtree 4.5.1", r"C:\Program Files\Orgtree\Orgtree.exe,0", "");
        assert_eq!(orgtree_exe(&[release.clone()], there), Some(exe.clone()));
        assert_eq!(orgtree_exe(&[entry("Orgtree 4.5.0-alpha.8", r#""C:\Program Files\Orgtree\Orgtree.exe",0"#, "")], there),
                   Some(exe.clone()));
        assert_eq!(orgtree_exe(&[entry("Orgtree", "", r"C:\Program Files\Orgtree")], there), Some(exe.clone()));
        // other programs, a development channel and look-alikes are not Orgtree
        for other in [entry("Hubchat 1.2.5", r"C:\Program Files\Orgtree\Orgtree.exe", ""),
                      entry("Orgtree Dev 4.5.1", r"C:\Program Files\Orgtree\Orgtree.exe,0", ""),
                      entry("Orgtree Helper", r"C:\Program Files\Orgtree\Orgtree.exe", ""),
                      entry("Orgtree 4.5.1", r"C:\Program Files\Orgtree\Other.exe,0", ""),
                      entry("Orgtree 4.5.1", r"Orgtree.exe", "")] {
            assert_eq!(orgtree_exe(&[other.clone()], |_| true), None, "{other:?}");
        }
        // an entry whose program is gone is skipped for the next one
        let gone = entry("Orgtree 4.4.0", r"D:\Old\Orgtree.exe,0", "");
        assert_eq!(orgtree_exe(&[gone, release], there), Some(exe));
    }

    #[test]
    fn orgtree_is_started_with_only_its_taskbar_target() {
        assert_eq!(launch_args("alpha-works"), Some(vec!["--orgtree-taskbar=org:alpha-works".to_string()]));
        assert_eq!(launch_args("alpha works"), None);
        assert_eq!(launch_args("alpha\" --inject"), None);
        assert_eq!(launch_args(""), None);
    }

    /// A stand-in Orgtree.exe, compiled here, records its arguments; `launch`
    /// starts it as Hubchat starts the registered Orgtree.
    #[test]
    fn launch_hands_orgtree_exactly_the_taskbar_target() {
        let dir = std::env::temp_dir().join(format!("hubchat-local-orgtree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("fake.rs");
        std::fs::write(&source, r#"fn main() {
            let args: Vec<String> = std::env::args().skip(1).collect();
            let out = std::env::current_exe().unwrap().with_file_name("argv.txt");
            std::fs::write(out, format!("{args:?}")).unwrap();
        }"#).unwrap();
        let exe = dir.join("Orgtree.exe");
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let built = std::process::Command::new(rustc).arg(&source).arg("-o").arg(&exe).status().unwrap();
        assert!(built.success(), "the stand-in compiles");
        launch(&exe, "alpha-works").unwrap();
        let recorded = dir.join("argv.txt");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !recorded.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let argv = std::fs::read_to_string(&recorded).expect("the stand-in ran");
        println!("recorded argv: {argv}");
        assert_eq!(argv, r#"["--orgtree-taskbar=org:alpha-works"]"#);
        assert!(launch(&exe, "not a slug").is_err(), "an invalid slug starts nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
