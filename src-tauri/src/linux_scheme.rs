//! Linux `hubchat://` scheme registration (docket
//! `frame-linux-arm64-orgtree-4-6-7-doesn-t-detect-h`; mirror of Orgtree's
//! `apps/desktop/main/orgtree-link-linux.ts`).
//!
//! Windows registers `hubchat://` in the per-user registry (`lib.rs`
//! `register_scheme`). Linux has no equivalent call for an AppImage, so at each
//! start Hubchat writes two per-user files:
//!
//!   `$XDG_DATA_HOME/applications/dev.orgtree.hubchat.link.desktop`
//!   `$XDG_CONFIG_HOME/mimeapps.list` (default for `x-scheme-handler/hubchat`)
//!
//! It never reads, changes or removes any other `.desktop` file (such as a
//! user's hand-written `dev.orgtree.hubchat.desktop`), and claims the default
//! only when no default exists or it is already ours.
//!
//! Everything except the Linux `getpwuid_r` / `AppHandle` wrapper is pure path
//! logic built on Windows tests too so its unit tests run on every dev host.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::path::Path;

pub const LINK_ENTRY: &str = "dev.orgtree.hubchat.link.desktop";
pub const LINK_MIME: &str = "x-scheme-handler/hubchat";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryStatus {
    Written,
    Current,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultStatus {
    Set,
    Ours,
    Other(String),
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterResult {
    pub entry: EntryStatus,
    pub default: DefaultStatus,
}

#[derive(Debug, Clone)]
pub struct Inputs<'a> {
    pub identifier: &'a str,
    pub version: &'a str,
    /// The account's real home (from `getpwuid_r` on Linux), not `$HOME`.
    pub account_home: Option<&'a Path>,
    /// The running executable path (`std::env::current_exe()`).
    pub exec_path: &'a str,
    pub env_home: Option<&'a str>,
    pub xdg_data_home: Option<&'a str>,
    pub xdg_config_home: Option<&'a str>,
    pub appimage: Option<&'a str>,
    pub appdir: Option<&'a str>,
}

pub fn desktop_exec_arg(_program: &str) -> Option<String> {
    None
}

pub fn appimage_extracted(_appimage: Option<&str>, _appdir: Option<&str>) -> bool {
    false
}

pub fn link_entry_text(_exec: &str, _version: &str) -> String {
    String::new()
}

pub fn register_in(_inputs: &Inputs<'_>) -> std::io::Result<RegisterResult> {
    Ok(RegisterResult { entry: EntryStatus::Skipped, default: DefaultStatus::Skipped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestHome {
        dir: PathBuf,
        apps: PathBuf,
        entry: PathBuf,
        mime: PathBuf,
        dir_str: String,
    }

    impl TestHome {
        fn new(name: &str) -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "hubchat-linux-scheme-{name}-{}-{n}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let apps = dir.join(".local").join("share").join("applications");
            let entry = apps.join(LINK_ENTRY);
            let mime = dir.join(".config").join("mimeapps.list");
            let dir_str = dir.to_string_lossy().into_owned();
            Self { dir, apps, entry, mime, dir_str }
        }

        fn inputs<'a>(&'a self) -> Inputs<'a> {
            Inputs {
                identifier: "dev.orgtree.hubchat",
                version: "1.3.11",
                account_home: Some(&self.dir),
                exec_path: "/tmp/.mount_HubchAbCdEf/usr/bin/hubchat",
                env_home: Some(&self.dir_str),
                xdg_data_home: None,
                xdg_config_home: None,
                appimage: Some("/home/u/Applications/Hubchat.AppImage"),
                appdir: Some("/tmp/.mount_HubchAbCdEf"),
            }
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn bare_and_quoted_exec_args_follow_desktop_entry_rules() {
        // Plain paths stay bare: SteamOS gamescope xdg-open keeps quotes as part of the name.
        assert_eq!(
            desktop_exec_arg("/home/steamos/.local/opt/hubchat/hubchat").as_deref(),
            Some("/home/steamos/.local/opt/hubchat/hubchat")
        );
        assert_eq!(
            desktop_exec_arg("/opt/Hubchat-1.3.11_arm64+x/hubchat").as_deref(),
            Some("/opt/Hubchat-1.3.11_arm64+x/hubchat")
        );
        // Spaces and special characters are quoted and escaped per Desktop Entry spec.
        assert_eq!(
            desktop_exec_arg("/home/u/My Apps/Hubchat.AppImage").as_deref(),
            Some(r#""/home/u/My Apps/Hubchat.AppImage""#)
        );
        assert_eq!(
            desktop_exec_arg(r#"/a/$x`y"z\w/50%.AppImage"#).as_deref(),
            Some(r#""/a/\\$x\\`y\\"z\\\\w/50%%.AppImage""#)
        );
        // Relative paths and control characters are refused.
        for bad in [
            "",
            "relative/Hubchat.AppImage",
            "/a/line\nbreak",
            "/a/tab\there",
            "/a/\x01ctrl",
            "/a/\x7fdel",
        ] {
            assert_eq!(desktop_exec_arg(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn entry_text_carries_required_fields_and_both_exec_modes() {
        let mounted = link_entry_text("/home/u/Hubchat.AppImage %u", "1.3.11");
        assert_eq!(
            mounted,
            "[Desktop Entry]\nType=Application\nName=Hubchat link\nNoDisplay=true\n\
             Exec=/home/u/Hubchat.AppImage %u\nMimeType=x-scheme-handler/hubchat;\n\
             X-Hubchat-Version=1.3.11\n"
        );
        let extracted = link_entry_text(
            "env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/u/Hubchat.AppImage %u",
            "1.3.11-alpha.1",
        );
        assert!(extracted.contains(
            "\nExec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/u/Hubchat.AppImage %u\n"
        ));
        assert!(extracted.contains("\nX-Hubchat-Version=1.3.11-alpha.1\n"));
        assert!(appimage_extracted(
            Some("/home/u/Hubchat.AppImage"),
            Some("/tmp/appimage_extracted_0123abcd")
        ));
        assert!(!appimage_extracted(
            Some("/home/u/Hubchat.AppImage"),
            Some("/tmp/.mount_HubchAbCdEf")
        ));
        assert!(!appimage_extracted(None, Some("/tmp/appimage_extracted_0123abcd")));
    }

    #[test]
    fn first_start_writes_entry_and_sets_default_then_second_start_is_current() {
        let h = TestHome::new("first");
        let first = register_in(&h.inputs()).unwrap();
        assert_eq!(
            first,
            RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Set }
        );
        let text = std::fs::read_to_string(&h.entry).unwrap();
        assert_eq!(
            text,
            "[Desktop Entry]\nType=Application\nName=Hubchat link\nNoDisplay=true\n\
             Exec=/home/u/Applications/Hubchat.AppImage %u\nMimeType=x-scheme-handler/hubchat;\n\
             X-Hubchat-Version=1.3.11\n"
        );
        assert_eq!(
            std::fs::read_to_string(&h.mime).unwrap(),
            "[Default Applications]\nx-scheme-handler/hubchat=dev.orgtree.hubchat.link.desktop;\n"
        );

        let second = register_in(&h.inputs()).unwrap();
        assert_eq!(
            second,
            RegisterResult { entry: EntryStatus::Current, default: DefaultStatus::Ours }
        );
        let files: Vec<String> = std::fs::read_dir(&h.apps)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, vec![LINK_ENTRY], "no temporary file left behind");
    }

    #[test]
    fn extracted_appimage_writes_no_cleanup_and_rewrites_old_entry() {
        let h = TestHome::new("extracted");
        // First write a non-extracted / older entry
        let mut old = h.inputs();
        old.version = "1.3.10";
        assert_eq!(
            register_in(&old).unwrap(),
            RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Set }
        );

        // Next start runs extracted without FUSE at a new path and version
        let mut next = h.inputs();
        next.version = "1.3.11";
        next.appimage = Some("/home/steamos/.local/opt/hubchat/hubchat");
        next.appdir = Some("/tmp/appimage_extracted_fedfad5");
        assert_eq!(
            register_in(&next).unwrap(),
            RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Ours }
        );
        let text = std::fs::read_to_string(&h.entry).unwrap();
        assert!(text.contains(
            "\nExec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/steamos/.local/opt/hubchat/hubchat %u\n"
        ));
        assert!(text.contains("\nX-Hubchat-Version=1.3.11\n"));

        // Repeating the same start is a no-op
        assert_eq!(
            register_in(&next).unwrap(),
            RegisterResult { entry: EntryStatus::Current, default: DefaultStatus::Ours }
        );
    }

    #[test]
    fn installed_package_without_appimage_registers_running_exe() {
        let h = TestHome::new("deb");
        let mut inp = h.inputs();
        inp.appimage = None;
        inp.appdir = None;
        inp.exec_path = "/usr/bin/hubchat";
        assert_eq!(
            register_in(&inp).unwrap(),
            RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Set }
        );
        let text = std::fs::read_to_string(&h.entry).unwrap();
        assert!(text.contains("\nExec=/usr/bin/hubchat %u\n"));
    }

    #[test]
    fn foreign_default_and_handwritten_launcher_are_untouched_byte_for_byte() {
        let h = TestHome::new("foreign");
        std::fs::create_dir_all(&h.apps).unwrap();
        std::fs::create_dir_all(h.mime.parent().unwrap()).unwrap();
        let handwritten = "[Desktop Entry]\nType=Application\nName=Hubchat\n\
            Exec=/home/steamos/.local/opt/hubchat/launch-hubchat\n";
        let launcher_path = h.apps.join("dev.orgtree.hubchat.desktop");
        std::fs::write(&launcher_path, handwritten).unwrap();

        let existing_mime = "[Added Associations]\ntext/html=firefox.desktop;\n\n\
            [Default Applications]\ntext/html=firefox.desktop\n\
            x-scheme-handler/hubchat=custom-hubchat.desktop;\n";
        std::fs::write(&h.mime, existing_mime).unwrap();

        let res = register_in(&h.inputs()).unwrap();
        assert_eq!(
            res,
            RegisterResult {
                entry: EntryStatus::Written,
                default: DefaultStatus::Other("custom-hubchat.desktop".into()),
            }
        );
        assert_eq!(std::fs::read(&h.mime).unwrap(), existing_mime.as_bytes());
        assert_eq!(std::fs::read(&launcher_path).unwrap(), handwritten.as_bytes());
    }

    #[test]
    fn existing_mimeapps_preserves_other_lines_and_handles_sections_and_empty_value() {
        // Case 1: existing [Default Applications] section without our key
        let a = TestHome::new("mime-sec");
        std::fs::create_dir_all(a.mime.parent().unwrap()).unwrap();
        std::fs::write(
            &a.mime,
            "[Default Applications]\ntext/html=firefox.desktop\n\n[Added Associations]\nimage/png=gimp.desktop;\n",
        )
        .unwrap();
        assert_eq!(register_in(&a.inputs()).unwrap().default, DefaultStatus::Set);
        assert_eq!(
            std::fs::read_to_string(&a.mime).unwrap(),
            "[Default Applications]\ntext/html=firefox.desktop\n\
             x-scheme-handler/hubchat=dev.orgtree.hubchat.link.desktop;\n\n\
             [Added Associations]\nimage/png=gimp.desktop;\n"
        );

        // Case 2: file without [Default Applications]
        let b = TestHome::new("mime-nosec");
        std::fs::create_dir_all(b.mime.parent().unwrap()).unwrap();
        std::fs::write(&b.mime, "[Added Associations]\nimage/png=gimp.desktop;").unwrap();
        assert_eq!(register_in(&b.inputs()).unwrap().default, DefaultStatus::Set);
        assert_eq!(
            std::fs::read_to_string(&b.mime).unwrap(),
            "[Added Associations]\nimage/png=gimp.desktop;\n\n\
             [Default Applications]\nx-scheme-handler/hubchat=dev.orgtree.hubchat.link.desktop;\n"
        );

        // Case 3: empty default value counts as no default
        let c = TestHome::new("mime-empty");
        std::fs::create_dir_all(c.mime.parent().unwrap()).unwrap();
        std::fs::write(&c.mime, "[Default Applications]\nx-scheme-handler/hubchat=\n").unwrap();
        assert_eq!(register_in(&c.inputs()).unwrap().default, DefaultStatus::Set);
        assert_eq!(
            std::fs::read_to_string(&c.mime).unwrap(),
            "[Default Applications]\nx-scheme-handler/hubchat=dev.orgtree.hubchat.link.desktop;\n"
        );
    }

    #[test]
    fn xdg_data_and_config_home_move_both_files() {
        let h = TestHome::new("xdg");
        let data = h.dir.join("xdg-data");
        let cfg = h.dir.join("xdg-cfg");
        let data_s = data.to_string_lossy().into_owned();
        let cfg_s = cfg.to_string_lossy().into_owned();
        let mut inp = h.inputs();
        inp.xdg_data_home = Some(&data_s);
        inp.xdg_config_home = Some(&cfg_s);
        assert_eq!(
            register_in(&inp).unwrap(),
            RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Set }
        );
        assert!(data.join("applications").join(LINK_ENTRY).is_file());
        assert!(cfg.join("mimeapps.list").is_file());
        assert!(!h.entry.exists());
        assert!(!h.mime.exists());
    }

    #[test]
    fn guards_skip_test_builds_foreign_home_and_bad_paths() {
        let skipped = RegisterResult { entry: EntryStatus::Skipped, default: DefaultStatus::Skipped };

        // .test identifier
        let h1 = TestHome::new("guard-test");
        let mut i1 = h1.inputs();
        i1.identifier = "dev.orgtree.hubchat.test";
        assert_eq!(register_in(&i1).unwrap(), skipped);
        assert!(std::fs::read_dir(&h1.dir).unwrap().next().is_none());

        // foreign HOME (e.g. smoke.sh setting HOME to a temp dir)
        let h2 = TestHome::new("guard-home");
        let foreign = h2.dir.join("fake-home");
        let foreign_s = foreign.to_string_lossy().into_owned();
        let mut i2 = h2.inputs();
        i2.env_home = Some(&foreign_s);
        assert_eq!(register_in(&i2).unwrap(), skipped);
        assert!(std::fs::read_dir(&h2.dir).unwrap().next().is_none());

        // missing account home
        let h3 = TestHome::new("guard-nohome");
        let mut i3 = h3.inputs();
        i3.account_home = None;
        assert_eq!(register_in(&i3).unwrap(), skipped);
        assert!(std::fs::read_dir(&h3.dir).unwrap().next().is_none());

        // relative or control-char program path
        for bad in ["relative/hubchat", "/a/bad\npath"] {
            let h4 = TestHome::new("guard-badpath");
            let mut i4 = h4.inputs();
            i4.appimage = Some(bad);
            assert_eq!(register_in(&i4).unwrap(), skipped, "{bad:?}");
            assert!(std::fs::read_dir(&h4.dir).unwrap().next().is_none());
        }
    }
}
