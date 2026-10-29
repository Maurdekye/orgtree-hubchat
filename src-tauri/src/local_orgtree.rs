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

/// Orgtree's org slugs.
pub fn valid_slug(_slug: &str) -> bool {
    false
}

/// The orgs in a local-orgs.json.
pub fn parse(_bytes: &[u8]) -> Vec<LocalOrg> {
    Vec::new()
}

/// The registered Orgtree.exe.
pub fn orgtree_exe(_entries: &[Uninstall], _exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    None
}

/// The arguments Orgtree is started with for an org.
pub fn launch_args(_slug: &str) -> Option<Vec<String>> {
    None
}

/// Start `exe` on an org.
pub fn launch(_exe: &Path, _slug: &str) -> Result<(), String> {
    Err("not yet".into())
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
