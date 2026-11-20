//! The Orgtree install on this PC (user 2026-10-29: Contact info's "Orgtree
//! org" badge opens a local org in Orgtree).
//!
//! Orgtree's engine keeps `local-orgs.json` in its data root,
//! `%APPDATA%\Orgtree v2\data` unless Orgtree's Move data put it elsewhere
//! (data-location.json, `data_root`): `{"v":1,"orgs":[{"slug","name","address"}]}`,
//! the orgs it hosts that have a hub address, nothing secret. A contact is
//! local only when its address is listed there AND an Orgtree install is
//! registered. Opening starts that registered `Orgtree.exe` with exactly one
//! argument, `--orgtree-taskbar=org:<slug>`, Orgtree's own taskbar entry
//! point: a running Orgtree opens or focuses that org through its single
//! instance, a stopped one starts and opens it. No network, no Orgtree token,
//! no shell; the program comes from the registry, never from the file.
//!
//! Linux (`linux` below): the install is the orgtree:// handler xdg-open
//! would use, and the badge opens the org through an orgtree:// link.
#![cfg_attr(not(windows), allow(dead_code))]

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
    pub display_version: String,
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
#[cfg(windows)]
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

#[cfg(windows)]
const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

/// Orgtree's Uninstall entries for this user and this machine (both views).
#[cfg(windows)]
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
                                         install_location: value("InstallLocation"),
                                         display_version: value("DisplayVersion") });
            }
        }
    }
    entries
}

/// Orgtree's pointer to a moved data root (its Move data setting), next to
/// the default root: `{"version":1,"dataRoot":"<absolute path>",...}`.
const LOCATION: &str = "data-location.json";
/// Far more than a real pointer file; a larger one is not read.
const MAX_LOCATION_BYTES: u64 = 64 * 1024;

/// A release Orgtree's data root: the absolute `dataRoot` of a version-1
/// data-location.json, else the default `%APPDATA%\Orgtree v2\data`
/// (user 2026-11-18: after a move the default root has no listing).
pub fn data_root(roaming: &Path) -> PathBuf {
    let app = roaming.join("Orgtree v2");
    let moved = std::fs::metadata(app.join(LOCATION))
        .ok()
        .filter(|meta| meta.is_file() && meta.len() <= MAX_LOCATION_BYTES)
        .and_then(|_| std::fs::read(app.join(LOCATION)).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .filter(|v| v.get("version").and_then(|n| n.as_u64()) == Some(1))
        .and_then(|v| v.get("dataRoot").and_then(|r| r.as_str()).map(PathBuf::from))
        .filter(|root| root.is_absolute());
    moved.unwrap_or_else(|| app.join("data"))
}

/// local-orgs.json of a release Orgtree, in its data root.
fn listing(roaming: &Path) -> Vec<LocalOrg> {
    let path = data_root(roaming).join(FILE);
    match std::fs::metadata(&path) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_BYTES => std::fs::read(&path).map(|b| parse(&b)).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The orgs this PC's Orgtree hosts, when an Orgtree install is registered;
/// none otherwise (a listing an uninstalled Orgtree left behind included).
#[cfg(windows)]
pub fn local_orgs(roaming: &Path) -> Vec<LocalOrg> {
    if orgtree_exe(&uninstall_entries(), Path::is_file).is_none() {
        return Vec::new();
    }
    listing(roaming)
}

/// Start the registered Orgtree on an org its listing holds now.
#[cfg(windows)]
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

// ---------------------------------------------------------------- orgtree:// links
//
// Hubchat draws Orgtree's mentions as chips (user 2026-11-08, docket
// hubchat-render-orgtree-mentions-as-chips-linking). A chip of an org this PC
// runs opens its target through an orgtree:// link, the mirror of the
// hubchat:// links Orgtree opens (user 2026-11-08 mb3e5b66f1e00). The contract
// is Orgtree's docs/rust-engine/orgtree-links.md: one exact shape per kind,
// whose path is the canonical token. The link is built here from checked
// parts, never passed through as text, and handed to the system opener.

/// The registered Orgtree's version: its Uninstall entry's DisplayVersion,
/// else the version in its name ("Orgtree <version>"), for the same entry
/// `orgtree_exe` picks.
pub fn orgtree_version(entries: &[Uninstall], exists: impl Fn(&Path) -> bool) -> Option<String> {
    let exe = orgtree_exe(entries, &exists)?;
    let entry = entries.iter().filter(|e| is_orgtree(&e.display_name)).find(|e| program(e).as_deref() == Some(exe.as_path()))?;
    let version = entry.display_version.trim();
    if !version.is_empty() {
        return Some(version.to_string());
    }
    entry.display_name.trim().strip_prefix("Orgtree ").map(str::to_string)
}

/// Orgtree 4.5.9 is the first that takes orgtree:// links; its prereleases
/// (4.5.9-alpha.N) do too.
pub fn takes_links(version: &str) -> bool {
    let core = version.split(['-', '+']).next().unwrap_or("");
    let Ok(parts) = core.split('.').map(str::parse::<u64>).collect::<Result<Vec<_>, _>>() else { return false };
    parts.len() == 3 && (parts[0], parts[1], parts[2]) >= (4, 5, 9)
}

fn seg(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A node id: a segment, perhaps with `@<generation>` (Orgtree's grammar).
fn node(s: &str) -> bool {
    match s.split_once('@') {
        Some((name, generation)) => seg(name) && !generation.is_empty() && generation.bytes().all(|b| b.is_ascii_digit()),
        None => seg(s),
    }
}

/// The orgtree:// link for one canonical token of `org`, or for the org
/// alone; None for anything that is not exactly one documented shape.
pub fn link_url(org: &str, token: Option<&str>) -> Option<String> {
    if !valid_slug(org) {
        return None;
    }
    let Some(token) = token else { return Some(format!("orgtree://org/{org}")) };
    let (kind, path) = token.strip_prefix('@')?.split_once(':')?;
    let parts: Vec<&str> = path.split('/').collect();
    if parts.first() != Some(&org) {
        return None;
    }
    let shaped = match (kind, &parts[1..]) {
        ("item" | "doc" | "app" | "panel", [id]) => seg(id),
        ("agent" | "project", [name]) => node(name),
        ("mail", [mailbox, id]) => (*mailbox == "user" || *mailbox == "org") && seg(id),
        ("mail", ["node", name, id]) => node(name) && seg(id),
        _ => false,
    };
    shaped.then(|| format!("orgtree://{kind}/{path}"))
}

/// The link to open, when this PC's Orgtree takes links and hosts the org.
pub fn link_for(listing: &[LocalOrg], takes: bool, org: &str, token: Option<&str>) -> Result<String, String> {
    if !takes {
        return Err("This PC's Orgtree can't open links yet".into());
    }
    if !listing.iter().any(|o| o.slug == org) {
        return Err("That org isn't on this PC's Orgtree".into());
    }
    link_url(org, token).ok_or_else(|| "That isn't an Orgtree link".to_string())
}

/// orgtree:// is registered for this user (Orgtree claims it at start).
/// Without it Windows would offer to look for an app instead of opening one.
#[cfg(windows)]
fn scheme_registered() -> bool {
    use winreg::enums::HKEY_CLASSES_ROOT;
    use winreg::RegKey;
    RegKey::predef(HKEY_CLASSES_ROOT)
        .open_subkey(r"orgtree\shell\open\command")
        .and_then(|k| k.get_value::<String, _>(""))
        .is_ok_and(|command| !command.trim().is_empty())
}

/// This PC's Orgtree opens orgtree:// links: installed, 4.5.9 or later, and
/// the scheme registered.
#[cfg(windows)]
pub fn links_supported() -> bool {
    orgtree_version(&uninstall_entries(), Path::is_file).is_some_and(|v| takes_links(&v)) && scheme_registered()
}

/// The link for a chip, checked against this PC now.
#[cfg(windows)]
pub fn link_here(roaming: &Path, org: &str, token: Option<&str>) -> Result<String, String> {
    link_for(&listing(roaming), links_supported(), org, token)
}

// ---------------------------------------------------------------- Linux
//
// Linux has no Uninstall registry (user 2026-11-20 m5b5128333307: chips were
// grey on a Steam Frame). There the Orgtree install is found the way
// xdg-open would find it: the default handler of x-scheme-handler/orgtree in
// mimeapps.list, a .desktop entry whose Exec program exists. Orgtree 4.6.6
// writes both (its docs/rust-engine/orgtree-links.md): the entry
// dev.orgtree.link.desktop with X-Orgtree-Version. Any other handler the
// user set counts too; a version is checked only when the entry gives one.
// Its data root is `Orgtree v2` under the config folder, not the data one.
// Everything here is path logic, built on Windows too so its tests run there.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub mod linux {
    use std::path::{Path, PathBuf};

    pub const SCHEME_MIME: &str = "x-scheme-handler/orgtree";
    /// Far more than a real mimeapps.list or .desktop entry.
    const MAX_BYTES: u64 = 256 * 1024;

    /// The XDG folders a handler is looked up in.
    #[derive(Debug, Clone)]
    pub struct Dirs {
        /// `$XDG_CONFIG_HOME`, else `~/.config`: mimeapps.list and `Orgtree v2`.
        pub config: PathBuf,
        /// `$XDG_DATA_HOME`, else `~/.local/share`, then `$XDG_DATA_DIRS`.
        pub data: Vec<PathBuf>,
    }

    /// The folders for `home`, from the XDG variables `var` gives (absolute
    /// values only, as the specification says).
    pub fn dirs(home: &Path, var: impl Fn(&str) -> Option<String>) -> Dirs {
        let abs = |name: &str| var(name).map(PathBuf::from).filter(|p| p.is_absolute());
        let mut data = vec![abs("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local").join("share"))];
        let system = var("XDG_DATA_DIRS").filter(|s| !s.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        data.extend(system.split(':').map(PathBuf::from).filter(|p| p.is_absolute()));
        Dirs { config: abs("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")), data }
    }

    fn read_small(path: &Path) -> Option<String> {
        let meta = std::fs::metadata(path).ok()?;
        (meta.is_file() && meta.len() <= MAX_BYTES).then(|| std::fs::read_to_string(path).ok()).flatten()
    }

    /// The value of `key` in `[section]` of a desktop-style file.
    fn value<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
        let mut inside = false;
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') {
                inside = line == section;
            } else if inside {
                if let Some((k, v)) = line.split_once('=') {
                    if k.trim() == key {
                        return Some(v.trim());
                    }
                }
            }
        }
        None
    }

    /// The scheme's default handler in a mimeapps.list: the first desktop id
    /// of its `[Default Applications]` value, a plain file name.
    pub fn default_handler(mimeapps: &str) -> Option<String> {
        let id = value(mimeapps, "[Default Applications]", SCHEME_MIME)?.split(';').next()?.trim();
        (id.ends_with(".desktop") && !id.contains(['/', '\\']) && !id.starts_with('.')).then(|| id.to_string())
    }

    /// The program an Exec value starts: its first argument, after an `env`
    /// with its assignments. Quoting and escapes as the Desktop Entry
    /// specification gives them; None for anything unterminated or empty.
    pub fn exec_program(exec: &str) -> Option<PathBuf> {
        // the string-value escapes first (\\ is one backslash), then the arguments
        let mut text = String::new();
        let mut chars = exec.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                text.push(c);
                continue;
            }
            match chars.next()? {
                's' => text.push(' '),
                'n' | 'r' | 't' => return None,
                other => text.push(other),
            }
        }
        let mut args: Vec<String> = Vec::new();
        let mut chars = text.chars().peekable();
        while args.len() < 16 {
            while chars.peek().is_some_and(|c| *c == ' ') {
                chars.next();
            }
            let Some(first) = chars.peek().copied() else { break };
            let mut arg = String::new();
            if first == '"' {
                chars.next();
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => arg.push(chars.next()?),
                        c => arg.push(c),
                    }
                }
            } else {
                while let Some(c) = chars.peek().copied().filter(|c| *c != ' ') {
                    arg.push(c);
                    chars.next();
                }
            }
            args.push(arg.replace("%%", "%"));
        }
        let mut args = args.into_iter().peekable();
        if args.peek().is_some_and(|a| a == "env" || a == "/usr/bin/env" || a == "/bin/env") {
            args.next();
            while args.peek().is_some_and(|a| !a.starts_with('/') && a.contains('=')) {
                args.next();
            }
        }
        args.next().filter(|p| !p.is_empty() && !p.starts_with('%')).map(PathBuf::from)
    }

    /// What opens orgtree:// links on this computer.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Handler {
        /// Checked to exist; the system opener starts it, never Hubchat.
        #[allow(dead_code)]
        pub program: PathBuf,
        /// `X-Orgtree-Version`, when the entry is one Orgtree wrote.
        pub version: Option<String>,
    }

    /// The handler xdg-open would use: the default named in the user's
    /// mimeapps.list (the config folder's, then the old place beside the
    /// user's entries), found in the first applications folder that has it.
    /// It must be an entry for the scheme whose program is an absolute path
    /// to something that exists.
    pub fn handler(dirs: &Dirs, exists: impl Fn(&Path) -> bool) -> Option<Handler> {
        let lists = [dirs.config.join("mimeapps.list"), dirs.data.first()?.join("applications").join("mimeapps.list")];
        let id = lists.iter().find_map(|list| read_small(list).and_then(|text| default_handler(&text)))?;
        let entry = dirs.data.iter().find_map(|d| read_small(&d.join("applications").join(&id)))?;
        const ENTRY: &str = "[Desktop Entry]";
        if !value(&entry, ENTRY, "MimeType")?.split(';').any(|m| m.trim() == SCHEME_MIME) {
            return None;
        }
        let program = exec_program(value(&entry, ENTRY, "Exec")?)?;
        if !program.is_absolute() || !exists(&program) {
            return None;
        }
        let version = value(&entry, ENTRY, "X-Orgtree-Version").filter(|v| !v.is_empty()).map(str::to_string);
        Some(Handler { program, version })
    }

    /// The handler opens links: any version it names is 4.5.9 or later.
    pub fn takes(handler: &Handler) -> bool {
        handler.version.as_deref().is_none_or(super::takes_links)
    }

    #[cfg(target_os = "linux")]
    fn here() -> Option<Handler> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        handler(&dirs(&home, |name| std::env::var(name).ok()), |p| p.exists())
    }

    #[cfg(target_os = "linux")]
    pub fn links_supported() -> bool {
        here().is_some_and(|h| takes(&h))
    }

    /// The orgs this computer's Orgtree hosts, when a handler opens them.
    #[cfg(target_os = "linux")]
    pub fn local_orgs(config: &Path) -> Vec<super::LocalOrg> {
        if links_supported() { super::listing(config) } else { Vec::new() }
    }

    /// The link for a chip, or for an org alone, checked against this computer now.
    #[cfg(target_os = "linux")]
    pub fn link_here(config: &Path, org: &str, token: Option<&str>) -> Result<String, String> {
        super::link_for(&super::listing(config), links_supported(), org, token)
    }
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

    /// A scratch roaming folder: `Orgtree v2` with an optional
    /// data-location.json, and the listing written under `listed_in`.
    fn roaming_with(name: &str, location: Option<&str>, listed_in: &[&str]) -> PathBuf {
        let roaming = std::env::temp_dir().join(format!("hubchat-local-orgs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&roaming);
        let app = roaming.join("Orgtree v2");
        std::fs::create_dir_all(&app).unwrap();
        if let Some(text) = location {
            std::fs::write(app.join("data-location.json"), text.replace("{root}", &roaming.join("moved").display().to_string().replace('\\', "\\\\"))).unwrap();
        }
        for dir in listed_in {
            let d = roaming.join(dir);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(FILE), LISTING).unwrap();
        }
        roaming
    }

    /// Orgtree's Move data (data-location.json, version 1) puts local-orgs.json
    /// in the new data root; the old one keeps no copy (user 2026-11-18: an
    /// item link to an org on this PC was grey). Contact info's Orgtree org
    /// badge (`local_orgs`, `open`) and every chip (`link_here`) read this one
    /// listing.
    #[test]
    fn a_moved_data_root_is_followed() {
        let r = roaming_with("moved", Some(r#"{"version":1,"dataRoot":"{root}","previousRoot":"x"}"#), &["moved"]);
        assert_eq!(listing(&r).len(), 2, "the listing in the moved data root");
        let _ = std::fs::remove_dir_all(&r);
    }

    /// Without a usable data-location.json the default data root is read.
    #[test]
    fn the_default_data_root_is_read_without_a_usable_location() {
        for (name, location) in [
            ("none", None),
            ("damaged", Some("{not json")),
            ("v2", Some(r#"{"version":2,"dataRoot":"{root}"}"#)),
            ("relative", Some(r#"{"version":1,"dataRoot":"moved"}"#)),
            ("number", Some(r#"{"version":1,"dataRoot":7}"#)),
        ] {
            let r = roaming_with(name, location, &["Orgtree v2/data"]);
            assert_eq!(listing(&r).len(), 2, "{name}");
            let _ = std::fs::remove_dir_all(&r);
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
        Uninstall { display_name: name.into(), display_icon: icon.into(), install_location: location.into(), ..Default::default() }
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
    #[cfg(windows)]
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

    // orgtree:// links (user 2026-11-08, Orgtree docs/rust-engine/orgtree-links.md)

    #[test]
    fn orgtree_takes_links_from_4_5_9_prereleases_included() {
        for v in ["4.5.9", "4.5.9-alpha.1", "4.5.10", "4.6.0", "5.0.0", "10.0.0-rc.1"] {
            assert!(takes_links(v), "{v}");
        }
        for v in ["4.5.8", "4.5.8-alpha.9", "4.4.12", "3.9.9", "", "4.5", "x.y.z", "4.5.9.1x", " 4.5.9"] {
            assert!(!takes_links(v), "{v}");
        }
    }

    #[test]
    fn the_version_is_the_registered_orgtrees_own() {
        let exe = PathBuf::from(r"C:\Program Files\Orgtree\Orgtree.exe");
        let there = |p: &Path| p == exe;
        let mut now = entry("Orgtree 4.5.9-alpha.1", r"C:\Program Files\Orgtree\Orgtree.exe,0", "");
        now.display_version = "4.5.9-alpha.1".into();
        let mut gone = entry("Orgtree 4.6.0", r"D:\Old\Orgtree.exe,0", "");
        gone.display_version = "4.6.0".into();
        assert_eq!(orgtree_version(&[gone, now.clone()], there).as_deref(), Some("4.5.9-alpha.1"));
        // electron-builder's entry name carries it too, when DisplayVersion is missing
        let mut named = now.clone();
        named.display_version = String::new();
        assert_eq!(orgtree_version(&[named], there).as_deref(), Some("4.5.9-alpha.1"));
        assert_eq!(orgtree_version(&[entry("Orgtree", "", r"C:\Program Files\Orgtree")], there), None);
        assert_eq!(orgtree_version(&[], there), None);
    }

    #[test]
    fn links_mirror_orgtrees_contract_exactly() {
        for (token, url) in [
            ("@item:alpha/fix-login", "orgtree://item/alpha/fix-login"),
            ("@doc:alpha/d1a2", "orgtree://doc/alpha/d1a2"),
            ("@app:alpha/caption-app", "orgtree://app/alpha/caption-app"),
            ("@panel:alpha/p12", "orgtree://panel/alpha/p12"),
            ("@agent:alpha/luna-reserve", "orgtree://agent/alpha/luna-reserve"),
            ("@agent:alpha/codex-checklist@4", "orgtree://agent/alpha/codex-checklist@4"),
            ("@project:alpha/web", "orgtree://project/alpha/web"),
            ("@mail:alpha/user/ab12", "orgtree://mail/alpha/user/ab12"),
            ("@mail:alpha/org/ab12", "orgtree://mail/alpha/org/ab12"),
            ("@mail:alpha/node/luna@4/ab12", "orgtree://mail/alpha/node/luna@4/ab12"),
        ] {
            assert_eq!(link_url("alpha", Some(token)).as_deref(), Some(url), "{token}");
        }
        assert_eq!(link_url("alpha", None).as_deref(), Some("orgtree://org/alpha"));
        for (org, token) in [
            ("alpha", Some("@item:beta/x")), ("alpha", Some("@item:alpha/x/y")), ("alpha", Some("@item:Alpha/x")),
            ("alpha", Some("@item:alpha/x@4")), ("alpha", Some("@agent:alpha/x@b")), ("alpha", Some("@mail:alpha/box/x")),
            ("alpha", Some("@mail:alpha/node/x")), ("alpha", Some("@note:alpha/x")), ("alpha", Some("@item:alpha/x\" --inspect")),
            ("alpha", Some(" @item:alpha/x")), ("alpha", Some("@item:alpha/")), ("alpha", Some("")), ("alpha", Some("item:alpha/x")),
            ("Alpha", None), ("al pha", None), ("-alpha", None), ("", None), ("alpha\" --inspect", None),
        ] {
            assert_eq!(link_url(org, token), None, "{org:?} {token:?}");
        }
    }

    #[test]
    fn a_link_opens_only_an_org_of_this_pc_in_an_orgtree_that_takes_links() {
        let listing = parse(LISTING.as_bytes());
        assert_eq!(link_for(&listing, true, "alpha-works", Some("@item:alpha-works/x")).as_deref(), Ok("orgtree://item/alpha-works/x"));
        assert_eq!(link_for(&listing, true, "alpha-works", None).as_deref(), Ok("orgtree://org/alpha-works"));
        assert!(link_for(&listing, true, "beta", Some("@item:beta/x")).is_err(), "not an org of this PC");
        assert!(link_for(&listing, false, "alpha-works", None).is_err(), "an Orgtree before 4.5.9");
        assert!(link_for(&listing, true, "alpha-works", Some("@item:gamma/x")).is_err(), "the token names another org");
    }

    // ------------------------------------------------------------ Linux

    /// A scratch home: mimeapps.list in .config and entries in
    /// .local/share/applications, as given, plus a program that exists.
    /// Returns the home, its folders, and the program spelled with forward
    /// slashes (an absolute path on the machine running the test).
    fn linux_home(name: &str, mimeapps: Option<&str>, entries: &[(&str, &str)]) -> (PathBuf, linux::Dirs, String) {
        let home = std::env::temp_dir().join(format!("hubchat-linux-orgtree-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let apps = home.join(".local").join("share").join("applications");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::create_dir_all(home.join(".config")).unwrap();
        let program = home.join("Applications").join("Orgtree.AppImage");
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::fs::write(&program, b"x").unwrap();
        let program = program.display().to_string().replace('\\', "/");
        if let Some(text) = mimeapps {
            std::fs::write(home.join(".config").join("mimeapps.list"), text).unwrap();
        }
        for (id, text) in entries {
            std::fs::write(apps.join(id), text.replace("{program}", &program)).unwrap();
        }
        let dirs = linux::dirs(&home, |_| None);
        (home, dirs, program)
    }

    const OURS: &str = "[Default Applications]\nx-scheme-handler/orgtree=dev.orgtree.link.desktop;\n";
    const LINK: &str = "dev.orgtree.link.desktop";

    fn desktop_entry(exec: &str, version: Option<&str>) -> String {
        let version = version.map(|v| format!("X-Orgtree-Version={v}\n")).unwrap_or_default();
        format!("[Desktop Entry]\nType=Application\nName=Orgtree link\nNoDisplay=true\nExec={exec}\nMimeType=x-scheme-handler/orgtree;\n{version}")
    }

    #[test]
    fn linux_folders_follow_the_xdg_variables() {
        let home = Path::new("/home/u");
        let plain = linux::dirs(home, |_| None);
        assert_eq!(plain.config, home.join(".config"));
        assert_eq!(plain.data[0], home.join(".local").join("share"));
        let abs = std::env::temp_dir();
        let moved = linux::dirs(home, |name| match name {
            "XDG_CONFIG_HOME" => Some(abs.join("cfg").display().to_string()),
            "XDG_DATA_HOME" => Some("relative/ignored".into()),
            _ => None,
        });
        assert_eq!(moved.config, abs.join("cfg"));
        assert_eq!(moved.data[0], home.join(".local").join("share"), "a relative value is ignored");
    }

    #[test]
    fn linux_default_handler_is_the_first_id_of_the_default_applications_value() {
        assert_eq!(linux::default_handler(OURS).as_deref(), Some(LINK));
        let mixed = "[Added Associations]\nx-scheme-handler/orgtree=added.desktop;\n\n[Default Applications]\ntext/html=firefox.desktop\nx-scheme-handler/orgtree = orgtree.desktop;other.desktop;\n";
        assert_eq!(linux::default_handler(mixed).as_deref(), Some("orgtree.desktop"));
        for none in ["", "[Default Applications]\nx-scheme-handler/orgtree=\n", "[Added Associations]\nx-scheme-handler/orgtree=a.desktop\n",
                     "[Default Applications]\nx-scheme-handler/orgtree=../evil.desktop\n", "[Default Applications]\nx-scheme-handler/orgtree=sub/a.desktop\n",
                     "[Default Applications]\nx-scheme-handler/orgtree=notadesktopfile\n"] {
            assert_eq!(linux::default_handler(none), None, "{none:?}");
        }
    }

    #[test]
    fn linux_exec_program_is_the_first_argument_after_env() {
        for (exec, program) in [
            (r#""/home/u/Applications/Orgtree.AppImage" %u"#, "/home/u/Applications/Orgtree.AppImage"),
            (r#"env APPIMAGE_EXTRACT_AND_RUN=1 "/home/u/My Apps/Orgtree.AppImage" %u"#, "/home/u/My Apps/Orgtree.AppImage"),
            ("env APPIMAGE_EXTRACT_AND_RUN=1 /home/steamos/Applications/Orgtree.AppImage %U", "/home/steamos/Applications/Orgtree.AppImage"),
            ("/usr/bin/env A=1 B=2 /opt/Orgtree/orgtree %u", "/opt/Orgtree/orgtree"),
            ("/opt/Orgtree/orgtree", "/opt/Orgtree/orgtree"),
            // Orgtree's own quoting of " ` $ \ and % (its desktopExecArg)
            (r#""/a/\\$x\\`y\\"z\\\\w/50%%.AppImage" %u"#, r#"/a/$x`y"z\w/50%.AppImage"#),
        ] {
            assert_eq!(linux::exec_program(exec), Some(PathBuf::from(program)), "{exec}");
        }
        for none in ["", "   ", "env A=1", r#""/unterminated %u"#, "%u", r"/a/b\"] {
            assert_eq!(linux::exec_program(none), None, "{none:?}");
        }
    }

    /// The entry Orgtree 4.6.6 writes, as the default: links open.
    #[test]
    fn linux_orgtrees_own_entry_as_the_default_is_the_handler() {
        let ours = desktop_entry(r#"env APPIMAGE_EXTRACT_AND_RUN=1 "{program}" %u"#, Some("4.6.6"));
        let (home, dirs, program) = linux_home("ours", Some(OURS), &[(LINK, &ours)]);
        let handler = linux::handler(&dirs, |p| p.exists()).expect("the handler");
        assert_eq!(handler, linux::Handler { program: PathBuf::from(&program), version: Some("4.6.6".into()) });
        assert!(linux::takes(&handler));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A launcher the user made the default counts too: it names no version.
    #[test]
    fn linux_another_default_handler_counts_without_a_version() {
        let list = "[Default Applications]\nx-scheme-handler/orgtree=orgtree.desktop\n";
        let theirs = desktop_entry("env APPIMAGE_EXTRACT_AND_RUN=1 {program} %U", None);
        let (home, dirs, _) = linux_home("theirs", Some(list), &[("orgtree.desktop", &theirs)]);
        let handler = linux::handler(&dirs, |p| p.exists()).expect("the handler");
        assert_eq!(handler.version, None);
        assert!(linux::takes(&handler));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn linux_without_a_usable_default_nothing_opens_links() {
        let good = desktop_entry(r#""{program}" %u"#, Some("4.6.6"));
        let no_mime = good.replace("MimeType=x-scheme-handler/orgtree;\n", "MimeType=text/html;\n");
        let relative = desktop_entry("orgtree %u", Some("4.6.6"));
        let cases: [(&str, Option<&str>, Vec<(&str, &str)>, bool); 6] = [
            ("no mimeapps.list", None, vec![(LINK, &good)], true),
            ("no default for the scheme", Some("[Default Applications]\ntext/html=firefox.desktop\n"), vec![(LINK, &good)], true),
            ("the default entry is missing", Some(OURS), vec![], true),
            ("the entry is not for the scheme", Some(OURS), vec![(LINK, &no_mime)], true),
            ("the program is gone", Some(OURS), vec![(LINK, &good)], false),
            ("a relative program", Some(OURS), vec![(LINK, &relative)], true),
        ];
        for (n, (why, list, entries, program_exists)) in cases.into_iter().enumerate() {
            let (home, dirs, _) = linux_home(&format!("none{n}"), list, &entries);
            assert_eq!(linux::handler(&dirs, |p| program_exists && p.exists()), None, "{why}");
            let _ = std::fs::remove_dir_all(&home);
        }
    }

    /// An entry that names an Orgtree older than 4.5.9 does not take links.
    #[test]
    fn linux_a_named_version_must_take_links() {
        let h = |version: Option<&str>| linux::Handler { program: PathBuf::from("/x"), version: version.map(str::to_string) };
        assert!(linux::takes(&h(Some("4.6.6"))) && linux::takes(&h(Some("4.6.6-frame.1"))) && linux::takes(&h(None)));
        assert!(!linux::takes(&h(Some("4.5.8"))) && !linux::takes(&h(Some("garbage"))));
    }

    /// The old place for the user's defaults, beside the user's entries.
    #[test]
    fn linux_the_old_mimeapps_place_is_read_after_the_config_one() {
        let good = desktop_entry(r#""{program}" %u"#, None);
        let (home, dirs, _) = linux_home("old", None, &[(LINK, &good), ("mimeapps.list", OURS)]);
        assert!(linux::handler(&dirs, |p| p.exists()).is_some());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// On Linux the listing is under the config folder: the same reader.
    #[test]
    fn linux_listing_is_read_under_the_config_folder() {
        let (home, dirs, _) = linux_home("listing", None, &[]);
        let data = dirs.config.join("Orgtree v2").join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join(FILE), LISTING).unwrap();
        assert_eq!(listing(&dirs.config).len(), 2);
        let _ = std::fs::remove_dir_all(&home);
    }
}
