//! Linux desktop integration and `hubchat://` scheme registration (dockets
//! `frame-linux-arm64-orgtree-4-6-7-doesn-t-detect-h` and
//! `steamos-steam-frame-automate-the-install-workaro`; mirror of Orgtree's
//! `apps/desktop/main/orgtree-link-linux.ts`).
//!
//! Windows registers `hubchat://` in the per-user registry (`lib.rs`
//! `register_scheme`). Linux has no equivalent call for an AppImage, so at each
//! start Hubchat writes its per-user files:
//!
//!   `$XDG_DATA_HOME/applications/dev.orgtree.hubchat.link.desktop`
//!   `$XDG_CONFIG_HOME/mimeapps.list` (default for `x-scheme-handler/hubchat`)
//!   `$XDG_DATA_HOME/applications/dev.orgtree.hubchat.desktop` + icon
//!     (AppImage or `HUBCHAT_LAUNCHER` launches only, when no system or
//!     hand-written entry exists)
//!
//! It only creates or repairs `dev.orgtree.hubchat.desktop` when absent (with
//! no system entry under `XDG_DATA_DIRS`) or when the existing file carries
//! Hubchat's `X-Hubchat-Version=` marker; hand-written entries, system entries,
//! concurrent user edits, and other `.desktop` files are left untouched. It
//! claims the `hubchat://` default only when no default exists or it is already
//! ours.
//!
//! Everything except the Linux `getpwuid_r` / `AppHandle` wrapper is pure path
//! logic built on Windows tests too so its unit tests run on every dev host.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const LINK_ENTRY: &str = "dev.orgtree.hubchat.link.desktop";
pub const LINK_MIME: &str = "x-scheme-handler/hubchat";
pub const APP_ENTRY: &str = "dev.orgtree.hubchat.desktop";
pub const APP_ICON_NAME: &str = "dev.orgtree.hubchat";
const APP_ICON_PNG: &[u8] = include_bytes!("../icons/128x128.png");
const DEFAULTS: &str = "[Default Applications]";
const MAX_ENTRY_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryStatus {
    Written,
    Current,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEntryStatus {
    Written,
    Current,
    Other,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconStatus {
    Written,
    Exists,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntryResult {
    pub entry: AppEntryStatus,
    pub icon: IconStatus,
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
    pub xdg_data_dirs: Option<&'a str>,
    pub xdg_config_home: Option<&'a str>,
    pub appimage: Option<&'a str>,
    pub appdir: Option<&'a str>,
    /// Optional `HUBCHAT_LAUNCHER` opt-in wrapper script path.
    pub launcher: Option<&'a str>,
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

fn normalize_posix_str(raw: &str) -> Option<String> {
    if !raw.starts_with('/') || raw.bytes().any(|b| b <= 0x1f || b == 0x7f) {
        return None;
    }
    let mut stack: Vec<&str> = Vec::new();
    for seg in raw.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    if stack.is_empty() {
        Some("/".to_string())
    } else {
        Some(format!("/{}", stack.join("/")))
    }
}

/// True when `exec_path` is strictly inside `appdir` (canonicalizing both when
/// both exist on disk; otherwise using normalized POSIX paths).
fn exec_inside_appdir(exec_path: &str, appdir: &str) -> bool {
    let (Some(norm_exec), Some(norm_dir)) = (normalize_posix_str(exec_path), normalize_posix_str(appdir)) else {
        return false;
    };
    if norm_dir == "/" {
        return false;
    }
    if let (Ok(real_dir), Ok(real_exec)) = (std::fs::canonicalize(appdir), std::fs::canonicalize(exec_path)) {
        return real_dir != Path::new("/") && real_exec != real_dir && real_exec.starts_with(&real_dir);
    }
    norm_exec.len() > norm_dir.len() + 1
        && norm_exec.starts_with(&norm_dir)
        && norm_exec.as_bytes()[norm_dir.len()] == b'/'
}

/// Validate that `APPIMAGE` and `APPDIR` belong to this running process rather
/// than being inherited from a parent AppImage that launched a native install.
fn own_appimage<'a>(
    exec_path: &str,
    appimage: Option<&'a str>,
    appdir: Option<&str>,
) -> Option<(&'a str, bool)> {
    let program = appimage.filter(|s| !s.is_empty())?;
    let dir = appdir.filter(|s| !s.is_empty())?;
    let norm_dir = normalize_posix_str(dir)?;
    let base = norm_dir.rsplit('/').next().unwrap_or("");
    let extracted = appimage_extracted(Some(program), Some(dir)) && base.starts_with("appimage_extracted_");
    let mounted = base.starts_with(".mount_");
    if !extracted && !mounted {
        return None;
    }
    if !exec_inside_appdir(exec_path, dir) {
        return None;
    }
    Some((program, extracted))
}

fn system_data_dirs(raw: Option<&str>, data_home: &Path) -> Vec<PathBuf> {
    let entries: Vec<&str> = match raw.filter(|s| !s.is_empty()) {
        Some(val) => {
            // On Linux XDG_DATA_DIRS is strictly colon-delimited; ';' is a valid
            // filename character, never a path-list separator. Only Windows host
            // test fixtures use ';' with drive-letter paths (e.g. C:\...).
            #[cfg(unix)]
            let is_win_list = false;
            #[cfg(not(unix))]
            let is_win_list = val.len() >= 3
                && val.as_bytes()[0].is_ascii_alphabetic()
                && val.as_bytes()[1] == b':'
                && matches!(val.as_bytes()[2], b'\\' | b'/');
            if is_win_list {
                val.split(';').collect()
            } else {
                val.split(':').collect()
            }
        }
        None => vec!["/usr/local/share", "/usr/share"],
    };
    entries
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| {
            let abs = p.is_absolute() || p.to_str().is_some_and(|s| s.starts_with('/'));
            abs && !same_folder(p, data_home)
        })
        .collect()
}

fn has_system_entry(xdg_data_dirs: Option<&str>, data_home: &Path, entry_name: &str) -> bool {
    system_data_dirs(xdg_data_dirs, data_home)
        .into_iter()
        .any(|dir| std::fs::symlink_metadata(dir.join("applications").join(entry_name)).is_ok())
}

fn clean_version(version: &str) -> String {
    version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
        .collect()
}

/// Full text of `dev.orgtree.hubchat.link.desktop`.
pub fn link_entry_text(exec: &str, version: &str) -> String {
    let ver = clean_version(version);
    format!(
        "[Desktop Entry]\nType=Application\nName=Hubchat link\nNoDisplay=true\nExec={exec}\nMimeType={LINK_MIME};\nX-Hubchat-Version={ver}\n"
    )
}

/// Full text of Hubchat's visible app-list entry (`dev.orgtree.hubchat.desktop`).
pub fn app_entry_text(exec: &str, version: &str) -> String {
    let ver = clean_version(version);
    format!(
        "[Desktop Entry]\nType=Application\nName=Hubchat\nComment=Chat on an Orgtree mail hub\n\
         Exec={exec}\nIcon={APP_ICON_NAME}\nTerminal=false\nCategories=Network;InstantMessaging;\n\
         StartupWMClass=hubchat\nX-Hubchat-Version={ver}\n"
    )
}

fn is_managed_hubchat_entry(text: &str) -> bool {
    let mut inside = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            inside = line == "[Desktop Entry]";
            continue;
        }
        if !inside {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == "X-Hubchat-Version" && !v.trim().is_empty() {
                return true;
            }
        }
    }
    false
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

pub(crate) fn same_folder(a: &Path, b: &Path) -> bool {
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

#[cfg(test)]
thread_local! {
    static TEST_BEFORE_PUBLISH: std::cell::RefCell<Option<Box<dyn FnMut(&Path)>>> = const { std::cell::RefCell::new(None) };
    static TEST_TMP_PATH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

fn run_test_before_publish(_dest: &Path) {
    #[cfg(test)]
    TEST_BEFORE_PUBLISH.with(|cell| {
        if let Some(cb) = cell.borrow_mut().as_mut() {
            cb(_dest);
        }
    });
}

fn next_temp_path(file: &Path) -> PathBuf {
    #[cfg(test)]
    if let Some(custom) = TEST_TMP_PATH.with(|cell| cell.borrow_mut().take()) {
        return custom;
    }
    use rand::RngCore;
    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let rand_val = rand::rngs::OsRng.next_u64();
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    file.with_file_name(format!("{name}.{}.{seq}.{rand_val:016x}.tmp", std::process::id()))
}

struct OwnedTempFile {
    path: PathBuf,
    armed: bool,
}

impl OwnedTempFile {
    fn disarm(&mut self) {
        self.armed = false;
    }
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for OwnedTempFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Exclusively create, write, fchmod (`0o644` on Unix), and sync a temporary
/// file beside `file`. If the exclusive open fails (e.g. collision), the
/// pre-existing path was NOT created by us and is never unlinked.
fn write_temp_exclusive(file: &Path, bytes: &[u8]) -> std::io::Result<OwnedTempFile> {
    use std::io::Write;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = next_temp_path(file);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o644);
        #[cfg(target_os = "linux")]
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut f = opts.open(&tmp)?;
    // Only arm cleanup AFTER exclusive open succeeds so we only ever delete our own file.
    let guard = OwnedTempFile { path: tmp, armed: true };
    f.write_all(bytes)?;
    f.flush()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Change permissions through the open file descriptor (fchmod), never by pathname.
        f.set_permissions(std::fs::Permissions::from_mode(0o644))?;
    }
    f.sync_all()?;
    drop(f);
    Ok(guard)
}

fn write_atomic_bytes(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut guard = write_temp_exclusive(file, bytes)?;
    std::fs::rename(guard.path(), file)?;
    guard.disarm();
    Ok(())
}

fn write_atomic(file: &Path, text: &str) -> std::io::Result<()> {
    write_atomic_bytes(file, text.as_bytes())
}

fn is_hardlink_unsupported(err: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        matches!(
            err.raw_os_error(),
            Some(libc::ENOSYS | libc::EOPNOTSUPP | libc::EXDEV | libc::EPERM)
        )
    }
    #[cfg(not(unix))]
    {
        let _ = err;
        false
    }
}

#[cfg(target_os = "linux")]
fn rename_noreplace(src: &Path, dest: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let csrc = CString::new(src.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in path"))?;
    let cdest = CString::new(dest.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in path"))?;
    let rc = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            csrc.as_ptr(),
            libc::AT_FDCWD,
            cdest.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Publish a brand-new file with atomic no-replace semantics: on Linux prefers
/// `renameat2(RENAME_NOREPLACE)` from a complete synced temp file, falling back
/// to `hard_link`. Returns `Ok(false)` without touching `file` if another writer
/// created `file` first or if the filesystem supports neither atomic primitive
/// (never writing partial bytes directly into the visible final pathname).
fn write_no_replace(file: &Path, bytes: &[u8]) -> std::io::Result<bool> {
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut guard = write_temp_exclusive(file, bytes)?;
    run_test_before_publish(file);
    #[cfg(target_os = "linux")]
    {
        match rename_noreplace(guard.path(), file) {
            Ok(()) => {
                guard.disarm();
                return Ok(true);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
            Err(e) if matches!(e.raw_os_error(), Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP)) => {}
            Err(e) => return Err(e),
        }
    }
    match std::fs::hard_link(guard.path(), file) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) if is_hardlink_unsupported(&e) => Ok(false),
        Err(e) => Err(e),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSnap {
    text: String,
    len: u64,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(unix)]
    mtime_sec: i64,
    #[cfg(unix)]
    mtime_nsec: i64,
    #[cfg(unix)]
    ctime_sec: i64,
    #[cfg(unix)]
    ctime_nsec: i64,
    #[cfg(not(unix))]
    modified: Option<std::time::SystemTime>,
}

impl FileSnap {
    /// Post-`renameat2(RENAME_EXCHANGE)` identity check: `rename_exchange` itself
    /// updates `ctime` on the swapped inode, so compare inode, size, mtime, and bytes.
    #[cfg(target_os = "linux")]
    fn same_after_exchange(&self, expected: &Self) -> bool {
        self.text == expected.text
            && self.len == expected.len
            && self.dev == expected.dev
            && self.ino == expected.ino
            && self.mtime_sec == expected.mtime_sec
            && self.mtime_nsec == expected.mtime_nsec
    }
}

enum ExistingEntry {
    Missing,
    Untrusted,
    Regular(FileSnap),
}

fn read_existing_entry(path: &Path) -> std::io::Result<ExistingEntry> {
    use std::io::Read;
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ExistingEntry::Missing),
        Err(e) => return Err(e),
        Ok(meta) => {
            if !meta.is_file() || meta.len() > MAX_ENTRY_BYTES {
                return Ok(ExistingEntry::Untrusted);
            }
        }
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut f = match opts.open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ExistingEntry::Missing),
        #[cfg(unix)]
        Err(e) if matches!(e.raw_os_error(), Some(libc::ELOOP | libc::EISDIR | libc::ENOTDIR)) => {
            return Ok(ExistingEntry::Untrusted);
        }
        Err(e) => return Err(e),
    };
    let meta = f.metadata()?;
    if !meta.is_file() || meta.len() > MAX_ENTRY_BYTES {
        return Ok(ExistingEntry::Untrusted);
    }
    let mut buf = Vec::new();
    (&mut f).take(MAX_ENTRY_BYTES + 1).read_to_end(&mut buf)?;
    if (buf.len() as u64) > MAX_ENTRY_BYTES {
        return Ok(ExistingEntry::Untrusted);
    }
    let Ok(text) = String::from_utf8(buf) else {
        return Ok(ExistingEntry::Untrusted);
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(ExistingEntry::Regular(FileSnap {
            text,
            len: meta.len(),
            dev: meta.dev(),
            ino: meta.ino(),
            mtime_sec: meta.mtime(),
            mtime_nsec: meta.mtime_nsec(),
            ctime_sec: meta.ctime(),
            ctime_nsec: meta.ctime_nsec(),
        }))
    }
    #[cfg(not(unix))]
    {
        Ok(ExistingEntry::Regular(FileSnap {
            text,
            len: meta.len(),
            modified: meta.modified().ok(),
        }))
    }
}

#[cfg(target_os = "linux")]
fn rename_exchange(a: &Path, b: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let ca = CString::new(a.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in path"))?;
    let cb = CString::new(b.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in path"))?;
    let rc = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            ca.as_ptr(),
            libc::AT_FDCWD,
            cb.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn recover_displaced_entry(displaced: &Path, dest: &Path, our_bytes: &[u8]) {
    // Only attempt to swap the displaced manual file back if `dest` still holds
    // our just-written bytes. If a newer third edit arrived at `dest`, or if
    // rollback fails, leave BOTH files in place and NEVER unlink the displaced file.
    if std::fs::read(dest).is_ok_and(|cur| cur == our_bytes) && rename_exchange(displaced, dest).is_ok() {
        if std::fs::read(displaced).is_ok_and(|back| back == our_bytes) {
            let _ = std::fs::remove_file(displaced);
            return;
        }
    }
    tracing::warn!("Hubchat desktop entry changed during repair; preserved both files without deleting displaced entry");
}

/// Replace an existing managed entry only if its inode, timestamps, and bytes
/// are unchanged right up to publication. On Linux uses `renameat2(RENAME_EXCHANGE)`,
/// disarms cleanup before inspecting the displaced file so a concurrent manual
/// edit is never unlinked, and skips repair if the filesystem lacks atomic exchange.
fn replace_if_unchanged(dest: &Path, expected: &FileSnap, bytes: &[u8]) -> std::io::Result<bool> {
    let mut guard = write_temp_exclusive(dest, bytes)?;
    run_test_before_publish(dest);
    match read_existing_entry(dest)? {
        ExistingEntry::Missing | ExistingEntry::Untrusted => {
            tracing::info!("Hubchat desktop entry changed or vanished before repair; leaving destination untouched");
            Ok(false)
        }
        ExistingEntry::Regular(cur) if cur != *expected => {
            tracing::info!("Hubchat desktop entry changed before repair; leaving destination untouched");
            Ok(false)
        }
        ExistingEntry::Regular(_) => {
            #[cfg(target_os = "linux")]
            {
                match rename_exchange(guard.path(), dest) {
                    Ok(()) => {
                        // `guard.path()` now points to whatever inode was at `dest` at the exact
                        // swap instant. Disarm BEFORE inspecting so a displaced manual file is
                        // NEVER unlinked on rollback failure, error, or panic.
                        guard.disarm();
                        let swapped_ok = matches!(
                            read_existing_entry(guard.path()),
                            Ok(ExistingEntry::Regular(ref old)) if old.same_after_exchange(expected)
                        );
                        if swapped_ok {
                            let _ = std::fs::remove_file(guard.path());
                            return Ok(true);
                        }
                        recover_displaced_entry(guard.path(), dest, bytes);
                        return Ok(false);
                    }
                    Err(e) if matches!(e.raw_os_error(), Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP)) => {
                        tracing::info!("filesystem does not support atomic desktop entry exchange; skipping repair");
                        return Ok(false);
                    }
                    Err(e) => return Err(e),
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                std::fs::rename(guard.path(), dest)?;
                guard.disarm();
                Ok(true)
            }
        }
    }
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
#[cfg(test)]
pub fn register_in(inputs: &Inputs<'_>) -> std::io::Result<RegisterResult> {
    register_in_with(inputs, |_| false)
}

pub fn register_in_with(
    inputs: &Inputs<'_>,
    is_executable_file: impl Fn(&Path) -> bool,
) -> std::io::Result<RegisterResult> {
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

    let owned_appimage = own_appimage(inputs.exec_path, inputs.appimage, inputs.appdir);
    let extracted = owned_appimage.is_some_and(|(_, ext)| ext);

    let exec = if let Some(arg) = inputs
        .launcher
        .filter(|s| !s.is_empty())
        .and_then(|launcher| {
            let arg = desktop_exec_arg(launcher)?;
            is_executable_file(Path::new(launcher)).then_some(arg)
        })
    {
        if extracted {
            format!("env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 {arg} %u")
        } else {
            format!("env NO_CLEANUP=1 {arg} %u")
        }
    } else if let Some((program, ext)) = owned_appimage {
        let Some(arg) = desktop_exec_arg(program) else {
            return Ok(skipped);
        };
        if ext {
            format!("env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 {arg} %u")
        } else {
            format!("{arg} %u")
        }
    } else {
        let Some(arg) = desktop_exec_arg(inputs.exec_path) else {
            return Ok(skipped);
        };
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

fn install_icon_if_missing(data_home: &Path) -> IconStatus {
    let icon_path = data_home
        .join("icons")
        .join("hicolor")
        .join("128x128")
        .join("apps")
        .join(format!("{APP_ICON_NAME}.png"));
    match std::fs::symlink_metadata(&icon_path) {
        Ok(_) => return IconStatus::Exists,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return IconStatus::Skipped,
    }
    match write_no_replace(&icon_path, APP_ICON_PNG) {
        Ok(true) => IconStatus::Written,
        Ok(false) => IconStatus::Exists,
        Err(_) => IconStatus::Skipped,
    }
}

/// Self-install or repair Hubchat's visible `dev.orgtree.hubchat.desktop` and
/// icon on Linux (AppImage or valid `HUBCHAT_LAUNCHER` installs only, so `.deb`
/// installs do not get a duplicate app-menu entry). Respects the same guards as
/// `register_in_with`, keeps `HUBCHAT_LAUNCHER` winning (preserving extract
/// mode when detected), verifies AppImage executable ownership, never shadows
/// an administrator-installed system entry under `XDG_DATA_DIRS`, never
/// overwrites a hand-written entry (without `X-Hubchat-Version=`), a symlink,
/// a concurrent user edit, or an existing icon, and treats icon write failures
/// as non-fatal.
pub fn register_app_in_with(
    inputs: &Inputs<'_>,
    is_executable_file: impl Fn(&Path) -> bool,
) -> std::io::Result<AppEntryResult> {
    let skipped = AppEntryResult { entry: AppEntryStatus::Skipped, icon: IconStatus::Skipped };
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

    let owned_appimage = own_appimage(inputs.exec_path, inputs.appimage, inputs.appdir);
    let extracted = owned_appimage.is_some_and(|(_, ext)| ext);

    let exec = if let Some(arg) = inputs
        .launcher
        .filter(|s| !s.is_empty())
        .and_then(|launcher| {
            let arg = desktop_exec_arg(launcher)?;
            is_executable_file(Path::new(launcher)).then_some(arg)
        })
    {
        if extracted {
            format!("env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 {arg}")
        } else {
            format!("env NO_CLEANUP=1 {arg}")
        }
    } else if let Some((appimage, ext)) = owned_appimage {
        let Some(arg) = desktop_exec_arg(appimage) else {
            return Ok(skipped);
        };
        if ext {
            format!("env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 {arg}")
        } else {
            arg
        }
    } else {
        // .deb or non-AppImage build without HUBCHAT_LAUNCHER: leave system/user menus alone.
        return Ok(skipped);
    };

    let data_home = xdg_dir(inputs.xdg_data_home, home.join(".local").join("share"));
    let app_path = data_home.join("applications").join(APP_ENTRY);
    let text = app_entry_text(&exec, inputs.version);

    let entry = match read_existing_entry(&app_path)? {
        ExistingEntry::Missing => {
            if has_system_entry(inputs.xdg_data_dirs, &data_home, APP_ENTRY) {
                return Ok(AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped });
            }
            if !write_no_replace(&app_path, text.as_bytes())? {
                return Ok(AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped });
            }
            AppEntryStatus::Written
        }
        ExistingEntry::Untrusted => {
            return Ok(AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped });
        }
        ExistingEntry::Regular(snap) => {
            if !is_managed_hubchat_entry(&snap.text) {
                return Ok(AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped });
            }
            if snap.text.as_bytes() == text.as_bytes() {
                AppEntryStatus::Current
            } else if replace_if_unchanged(&app_path, &snap, text.as_bytes())? {
                AppEntryStatus::Written
            } else {
                return Ok(AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped });
            }
        }
    };

    let icon = install_icon_if_missing(&data_home);
    Ok(AppEntryResult { entry, icon })
}

/// The account's real home directory from the system password database
/// (`geteuid` + `getpwuid_r`), independent of `$HOME`.
#[cfg(target_os = "linux")]
pub(crate) fn account_home() -> Option<PathBuf> {
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

#[cfg(target_os = "linux")]
fn is_executable_regular_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && (m.permissions().mode() & 0o111 != 0))
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
    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS").ok();
    let xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
    let appimage = std::env::var("APPIMAGE").ok();
    let appdir = std::env::var("APPDIR").ok();
    let launcher = std::env::var("HUBCHAT_LAUNCHER").ok();
    let version = app.package_info().version.to_string();
    let inputs = Inputs {
        identifier: app.config().identifier.as_str(),
        version: &version,
        account_home: home.as_deref(),
        exec_path: &exec_path,
        env_home: env_home.as_deref(),
        xdg_data_home: xdg_data_home.as_deref(),
        xdg_data_dirs: xdg_data_dirs.as_deref(),
        xdg_config_home: xdg_config_home.as_deref(),
        appimage: appimage.as_deref(),
        appdir: appdir.as_deref(),
        launcher: launcher.as_deref(),
    };
    match register_in_with(&inputs, is_executable_regular_file) {
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
    match register_app_in_with(&inputs, is_executable_regular_file) {
        Ok(res) => match res.entry {
            AppEntryStatus::Written => {
                tracing::info!("registered Hubchat desktop entry");
            }
            AppEntryStatus::Other => {
                tracing::info!("existing Hubchat desktop entry was not written by Hubchat; left alone");
            }
            AppEntryStatus::Current | AppEntryStatus::Skipped => {}
        },
        Err(_) => {
            tracing::warn!("could not register Hubchat desktop entry");
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
        sys_data: PathBuf,
        sys_data_str: String,
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
            let sys_data = dir.join("sys-data");
            let sys_data_str = sys_data.to_string_lossy().into_owned();
            Self { dir, apps, entry, mime, dir_str, sys_data, sys_data_str }
        }

        fn inputs<'a>(&'a self) -> Inputs<'a> {
            Inputs {
                identifier: "dev.orgtree.hubchat",
                version: "1.3.11",
                account_home: Some(&self.dir),
                exec_path: "/tmp/.mount_HubchAbCdEf/usr/bin/hubchat",
                env_home: Some(&self.dir_str),
                xdg_data_home: None,
                xdg_data_dirs: Some(&self.sys_data_str),
                xdg_config_home: None,
                appimage: Some("/home/u/Applications/Hubchat.AppImage"),
                appdir: Some("/tmp/.mount_HubchAbCdEf"),
                launcher: None,
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
        next.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";
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

    #[test]
    fn launcher_opt_in_uses_bare_or_quoted_launcher_with_no_cleanup() {
        let h = TestHome::new("launcher-ok");
        // First write the direct AppImage form, then switch to HUBCHAT_LAUNCHER.
        let mut base = h.inputs();
        base.appimage = Some("/home/steamos/.local/opt/hubchat/hubchat");
        base.appdir = Some("/tmp/appimage_extracted_fedfad5");
        base.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";
        assert_eq!(
            register_in_with(&base, |_| true).unwrap(),
            RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Set }
        );

        let launcher = "/home/steamos/.local/opt/hubchat/launch-hubchat";
        let mut with_launcher = base.clone();
        with_launcher.launcher = Some(launcher);
        let is_exec = |p: &Path| p == Path::new(launcher);
        assert_eq!(
            register_in_with(&with_launcher, is_exec).unwrap(),
            RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Ours }
        );
        let text = std::fs::read_to_string(&h.entry).unwrap();
        assert!(
            text.contains(
                "\nExec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/steamos/.local/opt/hubchat/launch-hubchat %u\n"
            ),
            "{text}"
        );
        assert_eq!(
            register_in_with(&with_launcher, is_exec).unwrap(),
            RegisterResult { entry: EntryStatus::Current, default: DefaultStatus::Ours }
        );

        // Non-extracted launcher (e.g. FUSE-mounted AppImage) carries NO_CLEANUP=1 without extract var.
        let mut mounted_launcher = h.inputs();
        mounted_launcher.launcher = Some(launcher);
        assert_eq!(
            register_in_with(&mounted_launcher, is_exec).unwrap(),
            RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Ours }
        );
        let text = std::fs::read_to_string(&h.entry).unwrap();
        assert!(
            text.contains(
                "\nExec=env NO_CLEANUP=1 /home/steamos/.local/opt/hubchat/launch-hubchat %u\n"
            ),
            "{text}"
        );

        // A launcher path needing quotes follows the same Desktop Entry quoting rules.
        let spaced = "/home/u/My Scripts/launch hubchat";
        let mut with_spaced = base;
        with_spaced.launcher = Some(spaced);
        assert_eq!(
            register_in_with(&with_spaced, |p| p == Path::new(spaced)).unwrap(),
            RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Ours }
        );
        let text = std::fs::read_to_string(&h.entry).unwrap();
        assert!(
            text.contains(
                "\nExec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 \"/home/u/My Scripts/launch hubchat\" %u\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn invalid_missing_or_non_executable_launcher_falls_back_to_appimage() {
        let cases: [(&str, Option<&str>, bool); 6] = [
            ("unset", None, true),
            ("empty", Some(""), true),
            ("relative", Some("launch-hubchat"), true),
            ("control char", Some("/opt/hubchat/launch\nsh"), true),
            ("missing", Some("/home/steamos/.local/opt/hubchat/missing-launcher"), false),
            ("non-executable or dir", Some("/home/steamos/.local/opt/hubchat/not-exec"), false),
        ];
        for (idx, (why, launcher, exec_ok)) in cases.into_iter().enumerate() {
            let h = TestHome::new(&format!("launcher-fb-{idx}"));
            let mut inp = h.inputs();
            inp.appimage = Some("/home/steamos/.local/opt/hubchat/hubchat");
            inp.appdir = Some("/tmp/appimage_extracted_fedfad5");
            inp.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";
            inp.launcher = launcher;
            assert_eq!(
                register_in_with(&inp, |_| exec_ok).unwrap(),
                RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Set },
                "{why}"
            );
            let text = std::fs::read_to_string(&h.entry).unwrap();
            assert!(
                text.contains(
                    "\nExec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/steamos/.local/opt/hubchat/hubchat %u\n"
                ),
                "{why}: {text}"
            );
        }
    }

    #[test]
    fn launcher_opt_in_still_respects_test_and_foreign_home_guards() {
        let skipped = RegisterResult { entry: EntryStatus::Skipped, default: DefaultStatus::Skipped };
        let h = TestHome::new("launcher-guard");
        let mut inp = h.inputs();
        inp.identifier = "dev.orgtree.hubchat.test";
        inp.launcher = Some("/home/steamos/.local/opt/hubchat/launch-hubchat");
        assert_eq!(register_in_with(&inp, |_| true).unwrap(), skipped);
        assert!(std::fs::read_dir(&h.dir).unwrap().next().is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_executable_predicate_and_account_home_work_on_linux() {
        use std::os::unix::fs::PermissionsExt;
        let h = TestHome::new("linux-exec");
        let exec_file = h.dir.join("launch-hubchat");
        std::fs::write(&exec_file, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&exec_file, std::fs::Permissions::from_mode(0o755)).unwrap();
        let plain_file = h.dir.join("not-exec");
        std::fs::write(&plain_file, "data").unwrap();
        std::fs::set_permissions(&plain_file, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(is_executable_regular_file(&exec_file));
        assert!(!is_executable_regular_file(&plain_file));
        assert!(!is_executable_regular_file(&h.dir));
        assert!(!is_executable_regular_file(&h.dir.join("missing")));
        assert!(account_home().is_some());
    }

    // Requests 2+3: visible dev.orgtree.hubchat.desktop + icon + extract-mode env

    #[test]
    fn visible_app_entry_text_carries_required_fields_without_nodisplay_or_mimetype() {
        let extracted = app_entry_text(
            "env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/steamos/.local/opt/hubchat/hubchat",
            "1.3.12",
        );
        assert_eq!(
            extracted,
            "[Desktop Entry]\nType=Application\nName=Hubchat\nComment=Chat on an Orgtree mail hub\n\
             Exec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/steamos/.local/opt/hubchat/hubchat\n\
             Icon=dev.orgtree.hubchat\nTerminal=false\nCategories=Network;InstantMessaging;\n\
             StartupWMClass=hubchat\nX-Hubchat-Version=1.3.12\n"
        );
        assert!(!extracted.contains("NoDisplay="));
        assert!(!extracted.contains("MimeType="));
    }

    #[test]
    fn visible_app_entry_and_icon_created_on_extracted_appimage_then_noop_and_repaired() {
        let h = TestHome::new("app-extract");
        let app_file = h.apps.join(APP_ENTRY);
        let icon_file = h.dir.join(".local/share/icons/hicolor/128x128/apps/dev.orgtree.hubchat.png");

        let mut inp = h.inputs();
        inp.version = "1.3.12";
        inp.appimage = Some("/home/steamos/.local/opt/hubchat/hubchat");
        inp.appdir = Some("/tmp/appimage_extracted_fedfad5");
        inp.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";

        assert_eq!(
            register_app_in_with(&inp, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Written, icon: IconStatus::Written }
        );
        let text = std::fs::read_to_string(&app_file).unwrap();
        assert!(text.contains(
            "\nExec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/steamos/.local/opt/hubchat/hubchat\n"
        ));
        assert!(!text.contains("appimage_extracted_"));
        let icon_bytes = std::fs::read(&icon_file).unwrap();
        assert!(icon_bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "valid PNG icon installed");

        // Second start is a no-op.
        assert_eq!(
            register_app_in_with(&inp, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Current, icon: IconStatus::Exists }
        );

        // Update / moved AppImage / missing NO_CLEANUP repairs our managed entry and preserves icon.
        let mut moved = inp.clone();
        moved.version = "1.3.13";
        moved.appimage = Some("/home/steamos/Applications/Hubchat.AppImage");
        assert_eq!(
            register_app_in_with(&moved, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Written, icon: IconStatus::Exists }
        );
        let updated = std::fs::read_to_string(&app_file).unwrap();
        assert!(updated.contains(
            "\nExec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/steamos/Applications/Hubchat.AppImage\n"
        ));
        assert!(updated.contains("\nX-Hubchat-Version=1.3.13\n"));
    }

    #[test]
    fn visible_app_entry_honors_hubchat_launcher_and_mounted_appimage() {
        let h = TestHome::new("app-launcher");
        let app_file = h.apps.join(APP_ENTRY);

        // FUSE-mounted AppImage writes plain bare Exec without extract flags.
        let mut mounted = h.inputs();
        mounted.appimage = Some("/home/u/Applications/Hubchat.AppImage");
        mounted.appdir = Some("/tmp/.mount_HubchAbCdEf");
        assert_eq!(
            register_app_in_with(&mounted, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Written, icon: IconStatus::Written }
        );
        let text = std::fs::read_to_string(&app_file).unwrap();
        assert!(text.contains("\nExec=/home/u/Applications/Hubchat.AppImage\n"), "{text}");

        // Valid HUBCHAT_LAUNCHER wins over AppImage; carries APPIMAGE_EXTRACT_AND_RUN=1 when extracted.
        let launcher = "/home/steamos/.local/opt/hubchat/launch-hubchat";
        let mut with_launcher = mounted.clone();
        with_launcher.appdir = Some("/tmp/appimage_extracted_fedfad5");
        with_launcher.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";
        with_launcher.launcher = Some(launcher);
        assert_eq!(
            register_app_in_with(&with_launcher, |p| p == Path::new(launcher)).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Written, icon: IconStatus::Exists }
        );
        let text = std::fs::read_to_string(&app_file).unwrap();
        assert!(
            text.contains(
                "\nExec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/steamos/.local/opt/hubchat/launch-hubchat\n"
            ),
            "{text}"
        );

        // Non-extracted HUBCHAT_LAUNCHER keeps only NO_CLEANUP=1.
        let mut mounted_launcher = mounted.clone();
        mounted_launcher.launcher = Some(launcher);
        assert_eq!(
            register_app_in_with(&mounted_launcher, |p| p == Path::new(launcher)).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Written, icon: IconStatus::Exists }
        );
        let text = std::fs::read_to_string(&app_file).unwrap();
        assert!(
            text.contains("\nExec=env NO_CLEANUP=1 /home/steamos/.local/opt/hubchat/launch-hubchat\n"),
            "{text}"
        );

        // Invalid/missing HUBCHAT_LAUNCHER falls back to extract-mode AppImage.
        let mut bad_launcher = with_launcher;
        bad_launcher.launcher = Some("/home/steamos/missing-launcher");
        assert_eq!(
            register_app_in_with(&bad_launcher, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Written, icon: IconStatus::Exists }
        );
        let text = std::fs::read_to_string(&app_file).unwrap();
        assert!(
            text.contains(
                "\nExec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/u/Applications/Hubchat.AppImage\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn visible_app_entry_preserves_handwritten_entry_custom_icon_and_other_files() {
        let h = TestHome::new("app-handwritten");
        std::fs::create_dir_all(&h.apps).unwrap();
        let app_file = h.apps.join(APP_ENTRY);
        let handwritten = "[Desktop Entry]\nType=Application\nName=Hubchat\n\
            Exec=/home/steamos/.local/opt/hubchat/launch-hubchat\nIcon=/home/steamos/.local/opt/hubchat/hubchat.png\n";
        std::fs::write(&app_file, handwritten).unwrap();

        let mut inp = h.inputs();
        inp.appdir = Some("/tmp/appimage_extracted_fedfad5");
        inp.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";
        assert_eq!(
            register_app_in_with(&inp, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped }
        );
        assert_eq!(std::fs::read_to_string(&app_file).unwrap(), handwritten);
        assert!(!h.dir.join(".local/share/icons").exists());

        // Commented or other-section X-Hubchat-Version= does not claim ownership.
        let tricky = "[Desktop Entry]\nType=Application\nName=Hubchat\n# X-Hubchat-Version=1.3.11\n\
            Exec=/opt/custom\n\n[Other]\nX-Hubchat-Version=1.3.11\n";
        std::fs::write(&app_file, tricky).unwrap();
        assert_eq!(
            register_app_in_with(&inp, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped }
        );
        assert_eq!(std::fs::read_to_string(&app_file).unwrap(), tricky);

        // When our entry is created, an existing custom icon is preserved byte-for-byte.
        std::fs::remove_file(&app_file).unwrap();
        let icon_file = h.dir.join(".local/share/icons/hicolor/128x128/apps/dev.orgtree.hubchat.png");
        std::fs::create_dir_all(icon_file.parent().unwrap()).unwrap();
        std::fs::write(&icon_file, b"custom-icon").unwrap();
        assert_eq!(
            register_app_in_with(&inp, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Written, icon: IconStatus::Exists }
        );
        assert_eq!(std::fs::read(&icon_file).unwrap(), b"custom-icon");
    }

    #[test]
    fn visible_app_entry_skips_deb_test_and_foreign_home_and_survives_icon_failure() {
        let skipped = AppEntryResult { entry: AppEntryStatus::Skipped, icon: IconStatus::Skipped };

        // Installed .deb (no APPIMAGE, no valid HUBCHAT_LAUNCHER) does not write a duplicate menu entry.
        let h_deb = TestHome::new("app-deb");
        let mut deb = h_deb.inputs();
        deb.appimage = None;
        deb.appdir = None;
        deb.exec_path = "/usr/bin/hubchat";
        assert_eq!(register_app_in_with(&deb, |_| false).unwrap(), skipped);
        assert!(std::fs::read_dir(&h_deb.dir).unwrap().next().is_none());

        // .test build skipped.
        let h_test = TestHome::new("app-test");
        let mut t = h_test.inputs();
        t.identifier = "dev.orgtree.hubchat.test";
        assert_eq!(register_app_in_with(&t, |_| true).unwrap(), skipped);

        // Foreign HOME skipped.
        let h_home = TestHome::new("app-foreign-home");
        let foreign = h_home.dir.join("other");
        let foreign_s = foreign.to_string_lossy().into_owned();
        let mut fh = h_home.inputs();
        fh.env_home = Some(&foreign_s);
        assert_eq!(register_app_in_with(&fh, |_| true).unwrap(), skipped);

        // Failed optional icon write still writes visible desktop entry and leaves link registration working.
        let h_fail = TestHome::new("app-icon-fail");
        let icons_blocker = h_fail.dir.join(".local/share/icons");
        std::fs::create_dir_all(icons_blocker.parent().unwrap()).unwrap();
        std::fs::write(&icons_blocker, b"blocker").unwrap();
        let mut fail_inp = h_fail.inputs();
        fail_inp.appdir = Some("/tmp/appimage_extracted_fedfad5");
        fail_inp.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";
        assert_eq!(
            register_app_in_with(&fail_inp, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Written, icon: IconStatus::Skipped }
        );
        assert!(h_fail.apps.join(APP_ENTRY).is_file());
        assert_eq!(
            register_in_with(&fail_inp, |_| false).unwrap(),
            RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Set }
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn visible_app_entry_and_icon_refuse_symlinks_on_linux() {
        use std::os::unix::fs::symlink;
        let h = TestHome::new("app-symlink");
        std::fs::create_dir_all(&h.apps).unwrap();
        let target = h.dir.join("target.desktop");
        let target_text = "[Desktop Entry]\nType=Application\nName=Hubchat\nExec=/opt/x\nX-Hubchat-Version=1.3.11\n";
        std::fs::write(&target, target_text).unwrap();
        let link = h.apps.join(APP_ENTRY);
        symlink(&target, &link).unwrap();

        let mut inp = h.inputs();
        inp.appdir = Some("/tmp/appimage_extracted_fedfad5");
        inp.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";
        assert_eq!(
            register_app_in_with(&inp, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped }
        );
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), target_text);

        // Symlink at icon path is preserved untouched while desktop entry is written.
        std::fs::remove_file(&link).unwrap();
        let icon_target = h.dir.join("other-icon.png");
        std::fs::write(&icon_target, b"keep-me").unwrap();
        let icon_link = h.dir.join(".local/share/icons/hicolor/128x128/apps/dev.orgtree.hubchat.png");
        std::fs::create_dir_all(icon_link.parent().unwrap()).unwrap();
        symlink(&icon_target, &icon_link).unwrap();
        assert_eq!(
            register_app_in_with(&inp, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Written, icon: IconStatus::Exists }
        );
        assert!(std::fs::symlink_metadata(&icon_link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(&icon_target).unwrap(), b"keep-me");
    }

    // Round 1 review RED tests: M9 (HUBCHAT_LAUNCHER extract mode), M10 (AppImage exe ownership),
    // M12 (system-entry shadowing, concurrent write no-replace / repair guard, temp collision).

    #[test]
    fn m9_hubchat_launcher_preserves_detected_extract_mode() {
        let h = TestHome::new("m9-launcher-extract");
        let launcher = "/home/steamos/.local/opt/hubchat/launch-hubchat";
        let mut inp = h.inputs();
        inp.version = "1.3.12";
        inp.appimage = Some("/home/steamos/.local/opt/hubchat/hubchat");
        inp.appdir = Some("/tmp/appimage_extracted_fedfad5");
        inp.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";
        inp.launcher = Some(launcher);

        assert_eq!(
            register_app_in_with(&inp, |p| p == Path::new(launcher)).unwrap().entry,
            AppEntryStatus::Written
        );
        let app_text = std::fs::read_to_string(h.apps.join(APP_ENTRY)).unwrap();
        assert!(
            app_text.contains(
                "\nExec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/steamos/.local/opt/hubchat/launch-hubchat\n"
            ),
            "visible entry must preserve detected extract mode for HUBCHAT_LAUNCHER: {app_text}"
        );

        assert_eq!(
            register_in_with(&inp, |p| p == Path::new(launcher)).unwrap().entry,
            EntryStatus::Written
        );
        let link_text = std::fs::read_to_string(&h.entry).unwrap();
        assert!(
            link_text.contains(
                "\nExec=env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 /home/steamos/.local/opt/hubchat/launch-hubchat %u\n"
            ),
            "link entry must preserve detected extract mode for HUBCHAT_LAUNCHER: {link_text}"
        );
    }

    #[test]
    fn m10_native_install_inheriting_foreign_appimage_never_registers_parent_appimage() {
        let h = TestHome::new("m10-foreign");
        let mut inp = h.inputs();
        inp.exec_path = "/usr/bin/hubchat";
        inp.appimage = Some("/home/u/Applications/OtherApp.AppImage");
        inp.appdir = Some("/tmp/appimage_extracted_deadbeef");

        // Visible entry must skip (native install must not register parent's AppImage).
        assert_eq!(
            register_app_in_with(&inp, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Skipped, icon: IconStatus::Skipped }
        );
        assert!(!h.apps.join(APP_ENTRY).exists());

        // Link handler must register our own native executable without extract-mode flags.
        assert_eq!(
            register_in_with(&inp, |_| false).unwrap(),
            RegisterResult { entry: EntryStatus::Written, default: DefaultStatus::Set }
        );
        let link_text = std::fs::read_to_string(&h.entry).unwrap();
        assert!(link_text.contains("\nExec=/usr/bin/hubchat %u\n"), "{link_text}");
        assert!(!link_text.contains("OtherApp"), "{link_text}");
        assert!(!link_text.contains("APPIMAGE_EXTRACT_AND_RUN"), "{link_text}");

        // Broad spoofed APPDIR ("/", "/usr") must not claim ownership of /usr/bin/hubchat.
        for spoof in ["/", "/usr"] {
            let hs = TestHome::new("m10-spoof");
            let mut si = hs.inputs();
            si.exec_path = "/usr/bin/hubchat";
            si.appimage = Some("/home/u/Other.AppImage");
            si.appdir = Some(spoof);
            assert_eq!(
                register_app_in_with(&si, |_| false).unwrap(),
                AppEntryResult { entry: AppEntryStatus::Skipped, icon: IconStatus::Skipped },
                "spoof={spoof}"
            );
            assert_eq!(register_in_with(&si, |_| false).unwrap().entry, EntryStatus::Written);
            let txt = std::fs::read_to_string(&hs.entry).unwrap();
            assert!(txt.contains("\nExec=/usr/bin/hubchat %u\n"), "spoof={spoof}: {txt}");
        }
    }

    #[test]
    fn m12_system_desktop_entry_under_xdg_data_dirs_is_not_shadowed() {
        let h = TestHome::new("m12-sys");
        let sys_apps = h.sys_data.join("applications");
        std::fs::create_dir_all(&sys_apps).unwrap();
        std::fs::write(
            sys_apps.join(APP_ENTRY),
            "[Desktop Entry]\nType=Application\nName=Hubchat\nExec=/usr/bin/hubchat\n",
        )
        .unwrap();
        let mut inp = h.inputs();
        inp.appimage = Some("/home/steamos/.local/opt/hubchat/hubchat");
        inp.appdir = Some("/tmp/appimage_extracted_fedfad5");
        inp.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";

        assert_eq!(
            register_app_in_with(&inp, |_| false).unwrap(),
            AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped }
        );
        assert!(!h.apps.join(APP_ENTRY).exists());
        assert!(!h.dir.join(".local/share/icons").exists());

        // XDG_DATA_DIRS pointing only at data_home itself must not block our own user entry.
        let h2 = TestHome::new("m12-sys-self");
        let data_home = h2.dir.join(".local").join("share");
        let data_home_s = data_home.to_string_lossy().into_owned();
        let mut inp2 = h2.inputs();
        inp2.xdg_data_dirs = Some(&data_home_s);
        inp2.appdir = Some("/tmp/appimage_extracted_fedfad5");
        inp2.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";
        assert_eq!(register_app_in_with(&inp2, |_| false).unwrap().entry, AppEntryStatus::Written);
    }

    #[test]
    fn m12_concurrent_first_install_repair_and_temp_collision_preserve_existing_files() {
        let h = TestHome::new("m12-races");
        let app_path = h.apps.join(APP_ENTRY);
        let icon_path = h.dir.join(".local/share/icons/hicolor/128x128/apps/dev.orgtree.hubchat.png");
        let mut inp = h.inputs();
        inp.version = "1.3.12";
        inp.appimage = Some("/home/steamos/.local/opt/hubchat/hubchat");
        inp.appdir = Some("/tmp/appimage_extracted_fedfad5");
        inp.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";

        // 1. Concurrent custom desktop entry appears right before no-replace publish.
        let concurrent_desktop = "[Desktop Entry]\nType=Application\nName=Custom\nExec=/opt/user\n";
        let app_clone = app_path.clone();
        TEST_BEFORE_PUBLISH.with(|c| {
            *c.borrow_mut() = Some(Box::new(move |dest| {
                if dest == app_clone {
                    std::fs::write(&app_clone, concurrent_desktop).unwrap();
                }
            }));
        });
        let res = register_app_in_with(&inp, |_| false).unwrap();
        TEST_BEFORE_PUBLISH.with(|c| *c.borrow_mut() = None);
        assert_eq!(res, AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped });
        assert_eq!(std::fs::read_to_string(&app_path).unwrap(), concurrent_desktop);
        assert_eq!(std::fs::read_dir(&h.apps).unwrap().count(), 1, "temp cleaned on EEXIST");

        // 2. Concurrent custom icon appears right before icon no-replace publish.
        std::fs::remove_file(&app_path).unwrap();
        let icon_clone = icon_path.clone();
        TEST_BEFORE_PUBLISH.with(|c| {
            *c.borrow_mut() = Some(Box::new(move |dest| {
                if dest == icon_clone {
                    std::fs::write(&icon_clone, b"concurrent-user-icon").unwrap();
                }
            }));
        });
        let res = register_app_in_with(&inp, |_| false).unwrap();
        TEST_BEFORE_PUBLISH.with(|c| *c.borrow_mut() = None);
        assert_eq!(res, AppEntryResult { entry: AppEntryStatus::Written, icon: IconStatus::Exists });
        assert_eq!(std::fs::read(&icon_path).unwrap(), b"concurrent-user-icon");
        assert_eq!(std::fs::read_dir(icon_path.parent().unwrap()).unwrap().count(), 1);

        // 3. Concurrent edit replaces our managed entry during repair -> preserved byte-for-byte.
        let mut next = inp.clone();
        next.version = "1.3.13";
        let app_clone2 = app_path.clone();
        let user_edit = "[Desktop Entry]\nType=Application\nName=Hubchat\nExec=/home/steamos/custom\n";
        TEST_BEFORE_PUBLISH.with(|c| {
            *c.borrow_mut() = Some(Box::new(move |dest| {
                if dest == app_clone2 {
                    std::fs::write(&app_clone2, user_edit).unwrap();
                }
            }));
        });
        let res = register_app_in_with(&next, |_| false).unwrap();
        TEST_BEFORE_PUBLISH.with(|c| *c.borrow_mut() = None);
        assert_eq!(res, AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped });
        assert_eq!(std::fs::read_to_string(&app_path).unwrap(), user_edit);
        assert_eq!(std::fs::read_dir(&h.apps).unwrap().count(), 1);

        // 4. Exclusive temp-open collision never deletes the pre-existing file at that path.
        let colliding = h.apps.join("preexisting.tmp");
        std::fs::write(&colliding, b"keep-preexisting-tmp").unwrap();
        TEST_TMP_PATH.with(|c| *c.borrow_mut() = Some(colliding.clone()));
        let err = write_atomic_bytes(&h.apps.join("other.desktop"), b"x").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&colliding).unwrap(), b"keep-preexisting-tmp");
    }

    #[test]
    fn m12_repair_aborts_on_disappearance_and_semicolon_xdg_data_dirs_stays_intact() {
        // 1. Linux XDG_DATA_DIRS with a literal ';' in a POSIX path splits on ':' only.
        let dirs = system_data_dirs(Some("/opt/a;b/share:/usr/share"), Path::new("/home/u/.local/share"));
        assert_eq!(dirs, vec![PathBuf::from("/opt/a;b/share"), PathBuf::from("/usr/share")]);

        // 2. If a managed desktop entry disappears between snapshot and publish, repair aborts
        // and leaves the destination absent rather than recreating it.
        let h = TestHome::new("m12-disappear");
        let app_path = h.apps.join(APP_ENTRY);
        let mut inp = h.inputs();
        inp.version = "1.3.12";
        inp.appimage = Some("/home/steamos/.local/opt/hubchat/hubchat");
        inp.appdir = Some("/tmp/appimage_extracted_fedfad5");
        inp.exec_path = "/tmp/appimage_extracted_fedfad5/usr/bin/hubchat";
        assert_eq!(register_app_in_with(&inp, |_| false).unwrap().entry, AppEntryStatus::Written);

        let mut next = inp.clone();
        next.version = "1.3.13";
        let app_clone = app_path.clone();
        TEST_BEFORE_PUBLISH.with(|c| {
            *c.borrow_mut() = Some(Box::new(move |dest| {
                if dest == app_clone {
                    std::fs::remove_file(&app_clone).unwrap();
                }
            }));
        });
        let res = register_app_in_with(&next, |_| false).unwrap();
        TEST_BEFORE_PUBLISH.with(|c| *c.borrow_mut() = None);
        assert_eq!(res, AppEntryResult { entry: AppEntryStatus::Other, icon: IconStatus::Skipped });
        assert!(!app_path.exists(), "aborted repair must not recreate a concurrently removed file");
        assert_eq!(std::fs::read_dir(&h.apps).unwrap().count(), 0, "temp file cleaned on abort");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn m12_linux_recover_displaced_entry_never_deletes_user_edit() {
        let h = TestHome::new("m12-recover");
        std::fs::create_dir_all(&h.apps).unwrap();
        let our_bytes = b"[Desktop Entry]\nName=Hubchat\nX-Hubchat-Version=1.3.13\n";
        let manual_v1 = b"[Desktop Entry]\nName=ManualV1\nExec=/opt/v1\n";
        let manual_v2 = b"[Desktop Entry]\nName=ManualV2\nExec=/opt/v2\n";

        // 1. When dest still holds our_bytes, rollback swaps manual_v1 back and unlinks our temp.
        let dest1 = h.apps.join("case1.desktop");
        let disp1 = h.apps.join("case1.tmp");
        std::fs::write(&dest1, our_bytes).unwrap();
        std::fs::write(&disp1, manual_v1).unwrap();
        recover_displaced_entry(&disp1, &dest1, our_bytes);
        assert_eq!(std::fs::read(&dest1).unwrap(), manual_v1);
        assert!(!disp1.exists());

        // 2. When dest already holds a newer third edit (manual_v2), BOTH files stay untouched!
        let dest2 = h.apps.join("case2.desktop");
        let disp2 = h.apps.join("case2.tmp");
        std::fs::write(&dest2, manual_v2).unwrap();
        std::fs::write(&disp2, manual_v1).unwrap();
        recover_displaced_entry(&disp2, &dest2, our_bytes);
        assert_eq!(std::fs::read(&dest2).unwrap(), manual_v2);
        assert_eq!(std::fs::read(&disp2).unwrap(), manual_v1);
    }
}
