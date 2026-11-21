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

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const LINK_ENTRY: &str = "dev.orgtree.hubchat.link.desktop";
pub const LINK_MIME: &str = "x-scheme-handler/hubchat";
const DEFAULTS: &str = "[Default Applications]";

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

/// One program path as a Desktop Entry `Exec` argument. Plain paths matching
/// `^/[A-Za-z0-9/._+-]+$` stay bare so SteamOS gamescope's `xdg-open` (which
/// keeps quotes as part of the program name) can launch them; any other
/// absolute path without control characters is quoted and escaped per the
/// Desktop Entry specification. Returns `None` for relative paths or paths
/// containing ASCII control characters.
pub fn desktop_exec_arg(program: &str) -> Option<String> {
    if !program.starts_with('/') || program.bytes().any(|b| b <= 0x1f || b == 0x7f) {
        return None;
    }
    if program.len() > 1
        && program.as_bytes()[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'+' | b'-'))
    {
        return Some(program.to_string());
    }
    let mut out = String::with_capacity(program.len() + 2);
    out.push('"');
    for ch in program.chars() {
        match ch {
            '"' | '`' | '$' => {
                out.push_str(r"\\");
                out.push(ch);
            }
            '\\' => out.push_str(r"\\\\"),
            '%' => out.push_str("%%"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    Some(out)
}

/// True when running from an AppImage without a FUSE mount (`APPIMAGE` set and
/// `APPDIR`'s basename starts with `appimage_extracted_`).
pub fn appimage_extracted(appimage: Option<&str>, appdir: Option<&str>) -> bool {
    let Some(_) = appimage.filter(|s| !s.is_empty()) else {
        return false;
    };
    let Some(dir) = appdir.filter(|s| !s.is_empty()) else {
        return false;
    };
    let trimmed = dir.trim_end_matches('/');
    let base = trimmed.rsplit('/').next().unwrap_or(trimmed);
    base.starts_with("appimage_extracted_")
}

/// Full text of `dev.orgtree.hubchat.link.desktop`.
pub fn link_entry_text(exec: &str, version: &str) -> String {
    let clean_version: String = version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
        .collect();
    format!(
        "[Desktop Entry]\nType=Application\nName=Hubchat link\nNoDisplay=true\nExec={exec}\nMimeType={LINK_MIME};\nX-Hubchat-Version={clean_version}\n"
    )
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn same_folder(a: &Path, b: &Path) -> bool {
    if !a.is_absolute() || !b.is_absolute() {
        return false;
    }
    if let (Ok(ca), Ok(cb)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        return ca == cb;
    }
    normalize_lexical(a) == normalize_lexical(b)
}

fn xdg_dir(value: Option<&str>, fallback: PathBuf) -> PathBuf {
    match value.filter(|s| !s.is_empty()).map(PathBuf::from) {
        Some(p) if p.is_absolute() => p,
        _ => fallback,
    }
}

fn write_atomic(file: &Path, text: &str) -> std::io::Result<()> {
    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = file.with_file_name(format!("{name}.{}.{seq}.tmp", std::process::id()));
    let write_res = (|| -> std::io::Result<()> {
        std::fs::write(&tmp, text.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))?;
        }
        std::fs::rename(&tmp, file)?;
        Ok(())
    })();
    if write_res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    write_res
}

fn claim_default(file: &Path) -> std::io::Result<DefaultStatus> {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let mut lines: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        text.strip_suffix('\n')
            .unwrap_or(&text)
            .split('\n')
            .map(String::from)
            .collect()
    };

    let mut inside = false;
    let mut section: Option<usize> = None;
    let mut found_at: Option<usize> = None;
    let mut found_val = String::new();
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if line.starts_with('[') {
            inside = line == DEFAULTS;
            if inside && section.is_none() {
                section = Some(i);
            }
            continue;
        }
        if !inside {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == LINK_MIME {
                found_at = Some(i);
                found_val = v.split(';').next().unwrap_or("").trim().to_string();
                break;
            }
        }
    }

    if found_val == LINK_ENTRY {
        return Ok(DefaultStatus::Ours);
    }
    if !found_val.is_empty() {
        return Ok(DefaultStatus::Other(found_val));
    }

    let ours = format!("{LINK_MIME}={LINK_ENTRY};");
    if let Some(at) = found_at {
        lines[at] = ours;
    } else if let Some(sec) = section {
        let mut end = sec + 1;
        for (i, raw) in lines.iter().enumerate().skip(sec + 1) {
            let t = raw.trim();
            if t.starts_with('[') {
                break;
            }
            if !t.is_empty() {
                end = i + 1;
            }
        }
        lines.insert(end, ours);
    } else {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(DEFAULTS.to_string());
        lines.push(ours);
    }

    let out = format!("{}\n", lines.join("\n"));
    write_atomic(file, &out)?;
    Ok(DefaultStatus::Set)
}

/// Register `hubchat://` for this Hubchat on Linux, writing only our own
/// `.desktop` entry and claiming the default only when no default exists or it
/// is already ours. Skips `.test` builds, missing or foreign `HOME`, and
/// non-absolute or control-character program paths.
pub fn register_in(inputs: &Inputs<'_>) -> std::io::Result<RegisterResult> {
    let skipped = RegisterResult { entry: EntryStatus::Skipped, default: DefaultStatus::Skipped };
    if inputs.identifier.ends_with(".test") {
        return Ok(skipped);
    }
    let Some(home) = inputs.account_home.filter(|p| !p.as_os_str().is_empty()) else {
        return Ok(skipped);
    };
    if let Some(env_home) = inputs.env_home.filter(|s| !s.is_empty()) {
        if !same_folder(Path::new(env_home), home) {
            return Ok(skipped);
        }
    }

    let program = inputs
        .appimage
        .filter(|s| !s.is_empty())
        .unwrap_or(inputs.exec_path);
    let Some(arg) = desktop_exec_arg(program) else {
        return Ok(skipped);
    };
    let extracted = appimage_extracted(inputs.appimage, inputs.appdir);
    let exec = if extracted {
        format!("env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 {arg} %u")
    } else {
        format!("{arg} %u")
    };

    let data_home = xdg_dir(inputs.xdg_data_home, home.join(".local").join("share"));
    let entry_path = data_home.join("applications").join(LINK_ENTRY);
    let text = link_entry_text(&exec, inputs.version);
    let current = std::fs::read(&entry_path).is_ok_and(|bytes| bytes == text.as_bytes());
    if !current {
        write_atomic(&entry_path, &text)?;
    }

    let config_home = xdg_dir(inputs.xdg_config_home, home.join(".config"));
    let default = claim_default(&config_home.join("mimeapps.list"))?;

    Ok(RegisterResult {
        entry: if current { EntryStatus::Current } else { EntryStatus::Written },
        default,
    })
}

/// The account's real home directory from the system password database
/// (`geteuid` + `getpwuid_r`), independent of `$HOME`.
#[cfg(target_os = "linux")]
fn account_home() -> Option<PathBuf> {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;

    let uid = unsafe { libc::geteuid() };
    let mut buf_len = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    if buf_len <= 0 {
        buf_len = 16384;
    }
    let mut buf = vec![0u8; buf_len as usize];
    let mut pwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    loop {
        let rc = unsafe {
            libc::getpwuid_r(
                uid,
                pwd.as_mut_ptr(),
                buf.as_mut_ptr().cast::<libc::c_char>(),
                buf.len(),
                &mut result,
            )
        };
        if rc == libc::ERANGE && buf.len() < 1024 * 1024 {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        if rc != 0 || result.is_null() {
            return None;
        }
        let dir = unsafe { (*result).pw_dir };
        if dir.is_null() {
            return None;
        }
        let bytes = unsafe { CStr::from_ptr(dir) }.to_bytes();
        if bytes.is_empty() {
            return None;
        }
        return Some(PathBuf::from(OsStr::from_bytes(bytes)));
    }
}

/// Register `hubchat://` at desktop start on Linux.
#[cfg(target_os = "linux")]
pub fn register(app: &tauri::AppHandle) {
    let home = account_home();
    let exec_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.into_os_string().into_string().ok())
        .unwrap_or_default();
    let env_home = std::env::var("HOME").ok();
    let xdg_data_home = std::env::var("XDG_DATA_HOME").ok();
    let xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
    let appimage = std::env::var("APPIMAGE").ok();
    let appdir = std::env::var("APPDIR").ok();
    let version = app.package_info().version.to_string();
    let inputs = Inputs {
        identifier: app.config().identifier.as_str(),
        version: &version,
        account_home: home.as_deref(),
        exec_path: &exec_path,
        env_home: env_home.as_deref(),
        xdg_data_home: xdg_data_home.as_deref(),
        xdg_config_home: xdg_config_home.as_deref(),
        appimage: appimage.as_deref(),
        appdir: appdir.as_deref(),
    };
    match register_in(&inputs) {
        Ok(res) => match &res.default {
            DefaultStatus::Other(other) => {
                tracing::info!(other = %other, "hubchat:// default belongs to another entry; left alone");
            }
            DefaultStatus::Set => {
                tracing::info!("registered hubchat:// scheme handler");
            }
            DefaultStatus::Ours if res.entry == EntryStatus::Written => {
                tracing::info!("updated hubchat:// scheme handler entry");
            }
            _ => {}
        },
        Err(_) => {
            tracing::warn!("could not register hubchat:// scheme handler");
        }
    }
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
