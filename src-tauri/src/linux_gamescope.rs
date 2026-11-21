//! Gamescope graphics environment re-exec on Linux before GTK/WebKit start
//! (docket `steamos-steam-frame-automate-the-install-workaro`, Request 4).
//!
//! On SteamOS's SteamVR Gamescope session (`XDG_CURRENT_DESKTOP=gamescope`),
//! WebKitGTK / Mesa can hit an explicit-sync crash unless the graphics
//! environment is configured before GTK/WebKit and dynamic graphics layers
//! initialize:
//! - Always: `DISABLE_GAMESCOPE_WSI=1`.
//! - Both X11 (`DISPLAY`) and Wayland (`WAYLAND_DISPLAY`): `GDK_BACKEND=x11`
//!   and unset `EGL_PLATFORM`.
//! - Wayland only (`WAYLAND_DISPLAY` without `DISPLAY`):
//!   `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
//!
//! Runs at the very start of `main()` before `hubchat_lib::run()`, before
//! GTK/WebKit and before single-instance setup. Strictly gated on
//! `XDG_CURRENT_DESKTOP=gamescope`, loop-proof per process PID via
//! `HUBCHAT_GAMESCOPE_REEXEC`, snapshots the launching values of the managed
//! variables in `HUBCHAT_GAMESCOPE_ORIG_ENV` so external programs opened by
//! Hubchat (browsers, Orgtree, file openers) receive the original desktop
//! environment rather than Hubchat's WebKit overrides, and is a no-op when the
//! environment is already configured (including deliberate restarts).
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

pub const REEXEC_MARKER: &str = "HUBCHAT_GAMESCOPE_REEXEC";
pub const ORIG_ENV_VAR: &str = "HUBCHAT_GAMESCOPE_ORIG_ENV";

pub const MANAGED_VARS: [&str; 4] = [
    "DISABLE_GAMESCOPE_WSI",
    "GDK_BACKEND",
    "EGL_PLATFORM",
    "WEBKIT_DISABLE_DMABUF_RENDERER",
];

/// Hard ceiling per captured managed variable and for the combined snapshot env
/// var so `HUBCHAT_GAMESCOPE_ORIG_ENV` can never approach Linux's 128 KiB
/// per-string `MAX_ARG_STRLEN` exec limit (even on the re-exec failure path).
const MAX_ORIG_VAL_BYTES: usize = 256;
const MAX_ORIG_ENV_BYTES: usize = 2560;

static REEXEC_FAILURE: OnceLock<String> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReexecPlan {
    pub set: Vec<(&'static str, String)>,
    pub remove: Vec<&'static str>,
}

fn non_empty(val: Option<String>) -> bool {
    val.is_some_and(|s| !s.is_empty())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

/// Snapshot the launching values of `MANAGED_VARS` into `KEY=-` (unset) or
/// `KEY=+<hex>` entries separated by `;`. Values exceeding `MAX_ORIG_VAL_BYTES`
/// are omitted so the snapshot stays strictly bounded well below Linux's
/// 128 KiB `MAX_ARG_STRLEN` exec limit.
/// Known Low (accepted): if an original value of a managed graphics variable
/// exceeds `MAX_ORIG_VAL_BYTES` (256 bytes) and is omitted here, an external
/// child launched by Hubchat can inherit Hubchat's override for that variable.
pub fn encode_orig_env(env: &impl Fn(&str) -> Option<String>) -> String {
    let mut parts = Vec::with_capacity(MANAGED_VARS.len());
    for &k in &MANAGED_VARS {
        match env(k) {
            Some(v) if v.len() <= MAX_ORIG_VAL_BYTES => {
                parts.push(format!("{k}=+{}", hex_encode(v.as_bytes())));
            }
            Some(_) => {}
            None => parts.push(format!("{k}=-")),
        }
    }
    parts.join(";")
}

/// Decode a snapshot produced by `encode_orig_env` back into `(key, Option<val>)`.
pub fn decode_orig_env(raw: &str) -> Vec<(&'static str, Option<String>)> {
    let mut out = Vec::new();
    for entry in raw.split(';') {
        let Some((k, rest)) = entry.split_once('=') else {
            continue;
        };
        let Some(&key) = MANAGED_VARS.iter().find(|&&m| m == k) else {
            continue;
        };
        if rest == "-" {
            out.push((key, None));
        } else if let Some(hex) = rest.strip_prefix('+') {
            if let Some(bytes) = hex_decode(hex) {
                if let Ok(val) = String::from_utf8(bytes) {
                    out.push((key, Some(val)));
                }
            }
        }
    }
    out
}

/// Compute the Gamescope re-exec environment plan for `pid`, or `None` if no
/// re-exec is needed (outside `XDG_CURRENT_DESKTOP=gamescope`, marker already
/// matches `pid`, or all required graphics variables are already in the desired
/// state). A stale marker from a different PID does NOT suppress re-evaluation
/// when the display environment changed.
pub fn plan_reexec_for_pid(pid: u32, env: impl Fn(&str) -> Option<String>) -> Option<ReexecPlan> {
    let pid_str = pid.to_string();
    if env(REEXEC_MARKER).as_deref() == Some(pid_str.as_str()) {
        return None;
    }
    if env("XDG_CURRENT_DESKTOP").as_deref() != Some("gamescope") {
        return None;
    }
    let has_wayland = non_empty(env("WAYLAND_DISPLAY"));
    let has_x11 = non_empty(env("DISPLAY"));

    let mut set: Vec<(&'static str, String)> = vec![("DISABLE_GAMESCOPE_WSI", "1".to_string())];
    let mut remove: Vec<&'static str> = Vec::new();
    if has_wayland {
        if has_x11 {
            set.push(("GDK_BACKEND", "x11".to_string()));
            remove.push("EGL_PLATFORM");
        } else {
            set.push(("WEBKIT_DISABLE_DMABUF_RENDERER", "1".to_string()));
        }
    }

    let already_set = set.iter().all(|(k, v)| env(k).as_deref() == Some(v.as_str()));
    let already_removed = remove.iter().all(|k| env(k).is_none());
    if already_set && already_removed {
        return None;
    }

    // Preserve any already-captured bounded launching env across deliberate restarts.
    let orig = env(ORIG_ENV_VAR)
        .filter(|s| !s.is_empty() && s.len() <= MAX_ORIG_ENV_BYTES)
        .unwrap_or_else(|| encode_orig_env(&env));
    set.push((ORIG_ENV_VAR, orig));
    set.push((REEXEC_MARKER, pid_str));
    Some(ReexecPlan { set, remove })
}

/// Compute the Gamescope re-exec environment plan for the current process PID.
pub fn plan_reexec(env: impl Fn(&str) -> Option<String>) -> Option<ReexecPlan> {
    plan_reexec_for_pid(std::process::id(), env)
}

/// Apply a re-exec environment plan onto a command.
pub fn apply_plan(cmd: &mut Command, plan: &ReexecPlan) {
    for (k, v) in &plan.set {
        cmd.env(k, v);
    }
    for k in &plan.remove {
        cmd.env_remove(k);
    }
}

/// Restore the launching desktop environment onto an external child command
/// (browser, Orgtree, file opener) so Hubchat's Gamescope WebKit overrides and
/// re-exec marker never leak to other applications.
pub fn restore_external_env_from(cmd: &mut Command, orig_env: Option<&str>) {
    cmd.env_remove(REEXEC_MARKER);
    cmd.env_remove(ORIG_ENV_VAR);
    if let Some(raw) = orig_env.filter(|s| !s.is_empty()) {
        for (k, val) in decode_orig_env(raw) {
            match val {
                Some(v) => {
                    cmd.env(k, v);
                }
                None => {
                    cmd.env_remove(k);
                }
            }
        }
    }
}

/// Restore the current process's recorded launching environment onto `cmd`.
pub fn restore_external_env(cmd: &mut Command) {
    let orig = std::env::var(ORIG_ENV_VAR).ok();
    restore_external_env_from(cmd, orig.as_deref());
}

/// Build the re-exec command preserving `argv[0]` and all launch arguments once.
pub fn build_command(exe: &Path, args: &[OsString], plan: &ReexecPlan) -> Command {
    let mut cmd = Command::new(exe);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        if let Some(arg0) = args.first() {
            cmd.arg0(arg0);
        }
    }
    if args.len() > 1 {
        cmd.args(&args[1..]);
    }
    apply_plan(&mut cmd, plan);
    cmd
}

/// Prefix a non-URL filesystem path starting with `-` with `./` so `xdg-open`,
/// `gio`, and `gnome-open` cannot treat it as a command-line option.
fn option_safe_target(target: &OsStr, is_url: bool) -> OsString {
    if is_url {
        return target.to_os_string();
    }
    if target.to_string_lossy().starts_with('-') {
        let mut safe = OsString::from("./");
        safe.push(target);
        safe
    } else {
        target.to_os_string()
    }
}

/// Build the Linux desktop opener candidate commands with Hubchat's launching
/// environment restored.
pub fn external_open_commands(target: &OsStr, is_url: bool, orig_env: Option<&str>) -> Vec<Command> {
    let safe = option_safe_target(target, is_url);
    let mut cmds = Vec::with_capacity(4);

    let mut xdg = Command::new("xdg-open");
    xdg.arg(&safe);
    cmds.push(xdg);

    let mut gio = Command::new("gio");
    gio.arg("open").arg(&safe);
    cmds.push(gio);

    let mut gnome = Command::new("gnome-open");
    gnome.arg(&safe);
    cmds.push(gnome);

    let mut kde = Command::new("kde-open");
    kde.arg("--").arg(target);
    cmds.push(kde);

    for cmd in &mut cmds {
        restore_external_env_from(cmd, orig_env);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
    }
    cmds
}

#[cfg(target_os = "linux")]
fn spawn_external(target: &OsStr, is_url: bool) -> Result<(), String> {
    let orig = std::env::var(ORIG_ENV_VAR).ok();
    let mut last_err = None;
    for mut cmd in external_open_commands(target, is_url, orig.as_deref()) {
        match cmd.spawn() {
            Ok(mut child) => {
                // Reap the short-lived opener wrapper in the background so no zombie lingers.
                let _ = std::thread::Builder::new()
                    .name("hc-open-reap".into())
                    .spawn(move || {
                        let _ = child.wait();
                    });
                return Ok(());
            }
            Err(e) => last_err = Some(e.to_string()),
        }
    }
    Err(last_err.unwrap_or_else(|| "no Linux desktop opener could be launched".into()))
}

/// Open a URL with the Linux desktop opener using the restored launching environment.
#[cfg(target_os = "linux")]
pub fn open_external_url(url: &str) -> Result<(), String> {
    spawn_external(OsStr::new(url), true)
}

/// Open a filesystem path with the Linux desktop opener using the restored launching environment.
#[cfg(target_os = "linux")]
pub fn open_external_path(path: &Path) -> Result<(), String> {
    std::fs::metadata(path).map_err(|e| e.to_string())?;
    spawn_external(path.as_os_str(), false)
}

/// Record a failed re-exec and apply the graphics environment plan in-process
/// as a last-resort fallback before GTK/WebKit starts.
///
/// SAFETY: Must only be called during single-threaded `main()` startup before
/// any threads or libraries initialize.
#[cfg(target_os = "linux")]
fn apply_plan_after_reexec_failure(reason: String, plan: &ReexecPlan) {
    let _ = REEXEC_FAILURE.set(reason.clone());
    eprintln!("hubchat: Gamescope re-exec failed ({reason}); applying graphics environment in-process");
    for (k, v) in &plan.set {
        // SAFETY: Called at the start of `main()` before any other threads exist.
        unsafe { std::env::set_var(k, v) };
    }
    for k in &plan.remove {
        // SAFETY: Called at the start of `main()` before any other threads exist.
        unsafe { std::env::remove_var(k) };
    }
}

/// Emit a structured tracing warning after logging initializes if Gamescope
/// re-exec failed and fell back to in-process environment configuration.
#[cfg(target_os = "linux")]
pub fn log_reexec_status() {
    if let Some(reason) = REEXEC_FAILURE.get() {
        tracing::warn!(reason = %reason, "Gamescope re-exec failed; applied graphics environment in-process");
    }
}

/// Re-exec the current executable in place with Gamescope graphics env vars
/// before GTK/WebKit or single-instance setup runs.
#[cfg(target_os = "linux")]
pub fn reexec_if_needed() {
    use std::os::unix::process::CommandExt;
    let Some(plan) = plan_reexec(|k| std::env::var(k).ok()) else {
        return;
    };
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            apply_plan_after_reexec_failure(format!("current_exe failed: {e}"), &plan);
            return;
        }
    };
    let args: Vec<OsString> = std::env::args_os().collect();
    let err = build_command(&exe, &args, &plan).exec();
    apply_plan_after_reexec_failure(format!("exec failed: {err}"), &plan);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
        move |k: &str| map.get(k).cloned()
    }

    #[test]
    fn non_gamescope_sessions_never_reexec() {
        for desktop in [None, Some(""), Some("KDE"), Some("GNOME"), Some("ubuntu:GNOME"), Some("gamescope-extra")] {
            let mut pairs = vec![("DISPLAY", ":0"), ("WAYLAND_DISPLAY", "wayland-0")];
            if let Some(d) = desktop {
                pairs.push(("XDG_CURRENT_DESKTOP", d));
            }
            assert_eq!(plan_reexec(env_of(&pairs)), None, "desktop={desktop:?}");
        }
    }

    #[test]
    fn marker_env_var_makes_reexec_loop_proof() {
        let pid_str = std::process::id().to_string();
        let env = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("EGL_PLATFORM", "wayland"),
            (REEXEC_MARKER, &pid_str),
        ]);
        assert_eq!(plan_reexec(env), None);
    }

    #[test]
    fn gamescope_both_displays_sets_wsi_x11_and_unsets_egl_platform() {
        let pid_str = std::process::id().to_string();
        let env = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("EGL_PLATFORM", "wayland"),
        ]);
        let expected_orig = encode_orig_env(&env);
        assert_eq!(
            plan_reexec(env),
            Some(ReexecPlan {
                set: vec![
                    ("DISABLE_GAMESCOPE_WSI", "1".into()),
                    ("GDK_BACKEND", "x11".into()),
                    (ORIG_ENV_VAR, expected_orig),
                    (REEXEC_MARKER, pid_str),
                ],
                remove: vec!["EGL_PLATFORM"],
            })
        );

        // Even if only EGL_PLATFORM is still set (even empty), re-exec unsets it.
        let egl_only = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("DISABLE_GAMESCOPE_WSI", "1"),
            ("GDK_BACKEND", "x11"),
            ("EGL_PLATFORM", ""),
        ]);
        assert!(plan_reexec(egl_only).is_some());

        // Already configured launch (e.g. launch-hubchat or deliberate restart): no re-exec needed.
        let configured = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("DISABLE_GAMESCOPE_WSI", "1"),
            ("GDK_BACKEND", "x11"),
        ]);
        assert_eq!(plan_reexec(configured), None);
    }

    #[test]
    fn gamescope_wayland_only_sets_wsi_and_disables_dmabuf() {
        let pid_str = std::process::id().to_string();
        let env = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("WAYLAND_DISPLAY", "wayland-0"),
        ]);
        let expected_orig = encode_orig_env(&env);
        assert_eq!(
            plan_reexec(env),
            Some(ReexecPlan {
                set: vec![
                    ("DISABLE_GAMESCOPE_WSI", "1".into()),
                    ("WEBKIT_DISABLE_DMABUF_RENDERER", "1".into()),
                    (ORIG_ENV_VAR, expected_orig.clone()),
                    (REEXEC_MARKER, pid_str.clone()),
                ],
                remove: vec![],
            })
        );

        // Empty DISPLAY counts as Wayland-only.
        let empty_display = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("DISPLAY", ""),
        ]);
        assert_eq!(
            plan_reexec(empty_display),
            Some(ReexecPlan {
                set: vec![
                    ("DISABLE_GAMESCOPE_WSI", "1".into()),
                    ("WEBKIT_DISABLE_DMABUF_RENDERER", "1".into()),
                    (ORIG_ENV_VAR, expected_orig),
                    (REEXEC_MARKER, pid_str),
                ],
                remove: vec![],
            })
        );

        // Already configured Wayland-only launch: no re-exec.
        let configured = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("DISABLE_GAMESCOPE_WSI", "1"),
            ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
        ]);
        assert_eq!(plan_reexec(configured), None);
    }

    #[test]
    fn gamescope_x11_only_sets_wsi_only_and_leaves_backend_and_egl_untouched() {
        let pid_str = std::process::id().to_string();
        let env = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("EGL_PLATFORM", "x11"),
        ]);
        let expected_orig = encode_orig_env(&env);
        assert_eq!(
            plan_reexec(env),
            Some(ReexecPlan {
                set: vec![
                    ("DISABLE_GAMESCOPE_WSI", "1".into()),
                    (ORIG_ENV_VAR, expected_orig),
                    (REEXEC_MARKER, pid_str),
                ],
                remove: vec![],
            })
        );

        // Already configured X11-only launch: no re-exec.
        let configured = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("DISABLE_GAMESCOPE_WSI", "1"),
        ]);
        assert_eq!(plan_reexec(configured), None);
    }

    #[test]
    fn command_preserves_launch_arguments_once_including_uri_and_hidden() {
        let plan = ReexecPlan {
            set: vec![
                ("DISABLE_GAMESCOPE_WSI", "1".into()),
                ("GDK_BACKEND", "x11".into()),
                (ORIG_ENV_VAR, "DISABLE_GAMESCOPE_WSI=-;GDK_BACKEND=-;EGL_PLATFORM=+7761796c616e64;WEBKIT_DISABLE_DMABUF_RENDERER=-".into()),
                (REEXEC_MARKER, "12345".into()),
            ],
            remove: vec!["EGL_PLATFORM"],
        };
        let args: Vec<OsString> = vec![
            "hubchat".into(),
            "--hidden".into(),
            "hubchat://chat?to=alex.123456".into(),
        ];
        let cmd = build_command(Path::new("/tmp/app/usr/bin/hubchat"), &args, &plan);
        let got_args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
        assert_eq!(
            got_args,
            vec![
                std::ffi::OsStr::new("--hidden"),
                std::ffi::OsStr::new("hubchat://chat?to=alex.123456"),
            ]
        );
        let envs: HashMap<_, _> = cmd.get_envs().collect();
        assert_eq!(
            envs.get(std::ffi::OsStr::new("DISABLE_GAMESCOPE_WSI")),
            Some(&Some(std::ffi::OsStr::new("1")))
        );
        assert_eq!(
            envs.get(std::ffi::OsStr::new("GDK_BACKEND")),
            Some(&Some(std::ffi::OsStr::new("x11")))
        );
        assert_eq!(
            envs.get(std::ffi::OsStr::new(REEXEC_MARKER)),
            Some(&Some(std::ffi::OsStr::new("12345")))
        );
        assert_eq!(envs.get(std::ffi::OsStr::new("EGL_PLATFORM")), Some(&None));
    }

    #[test]
    fn main_rs_wires_gamescope_reexec_before_hubchat_lib_run() {
        let main_rs = include_str!("main.rs");
        let reexec_pos = main_rs
            .find("linux_gamescope::reexec_if_needed()")
            .expect("main.rs must call linux_gamescope::reexec_if_needed()");
        let run_pos = main_rs
            .find("hubchat_lib::run()")
            .expect("main.rs must call hubchat_lib::run()");
        assert!(reexec_pos < run_pos, "re-exec must run before hubchat_lib::run()");
    }

    #[test]
    fn m11_restore_external_env_restores_launching_values_and_strips_marker() {
        let launching = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("EGL_PLATFORM", "wayland"),
        ]);
        let plan = plan_reexec(&launching).expect("re-exec planned");
        let orig_snap = plan
            .set
            .iter()
            .find(|(k, _)| *k == ORIG_ENV_VAR)
            .map(|(_, v)| v.as_str())
            .expect("orig snapshot");

        // External opener commands restore EGL_PLATFORM=wayland, unset our added
        // DISABLE_GAMESCOPE_WSI / GDK_BACKEND / WEBKIT_DISABLE_DMABUF_RENDERER, and strip both HUBCHAT_GAMESCOPE_* vars.
        let cmds = external_open_commands(OsStr::new("orgtree://org/team"), true, Some(orig_snap));
        assert_eq!(cmds.len(), 4);
        for cmd in &cmds {
            let envs: HashMap<_, _> = cmd.get_envs().collect();
            assert_eq!(envs.get(OsStr::new(REEXEC_MARKER)), Some(&None));
            assert_eq!(envs.get(OsStr::new(ORIG_ENV_VAR)), Some(&None));
            assert_eq!(envs.get(OsStr::new("DISABLE_GAMESCOPE_WSI")), Some(&None));
            assert_eq!(envs.get(OsStr::new("GDK_BACKEND")), Some(&None));
            assert_eq!(envs.get(OsStr::new("WEBKIT_DISABLE_DMABUF_RENDERER")), Some(&None));
            assert_eq!(
                envs.get(OsStr::new("EGL_PLATFORM")),
                Some(&Some(OsStr::new("wayland")))
            );
        }

        // Without ORIG_ENV_VAR (non-reexeced launch), user/wrapper graphics vars are untouched;
        // only our internal markers are stripped.
        let plain_cmds = external_open_commands(OsStr::new("-leading-dash.txt"), false, None);
        let xdg_args: Vec<&OsStr> = plain_cmds[0].get_args().collect();
        assert_eq!(xdg_args, vec![OsStr::new("./-leading-dash.txt")]);
        let envs: HashMap<_, _> = plain_cmds[0].get_envs().collect();
        assert_eq!(envs.get(OsStr::new(REEXEC_MARKER)), Some(&None));
        assert_eq!(envs.get(OsStr::new(ORIG_ENV_VAR)), Some(&None));
        assert!(!envs.contains_key(OsStr::new("DISABLE_GAMESCOPE_WSI")));

        // Deliberate restart in the same Gamescope session (new PID, env already configured)
        // preserves WebKit's overrides and ORIG_ENV_VAR without re-execing; if display changes,
        // it re-execs while preserving the original ORIG_ENV_VAR snapshot.
        let next_pid = std::process::id().wrapping_add(1);
        let same_session_restart = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("DISABLE_GAMESCOPE_WSI", "1"),
            ("GDK_BACKEND", "x11"),
            (ORIG_ENV_VAR, orig_snap),
            (REEXEC_MARKER, &std::process::id().to_string()),
        ]);
        assert_eq!(plan_reexec_for_pid(next_pid, same_session_restart), None);

        let changed_display_restart = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("DISABLE_GAMESCOPE_WSI", "1"),
            ("GDK_BACKEND", "x11"),
            (ORIG_ENV_VAR, orig_snap),
            (REEXEC_MARKER, &std::process::id().to_string()),
        ]);
        let replan = plan_reexec_for_pid(next_pid, changed_display_restart)
            .expect("changed display in a new PID must re-plan");
        assert!(
            replan.set.iter().any(|(k, v)| *k == ORIG_ENV_VAR && v == orig_snap),
            "re-plan must preserve original pre-reexec env snapshot: {replan:?}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_reexec_command_applies_env_unsets_egl_preserves_args_and_restores_external_child() {
        let pid_str = std::process::id().to_string();
        let plan = plan_reexec(env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("EGL_PLATFORM", "wayland"),
        ]))
        .expect("re-exec planned");
        let orig_snap = plan
            .set
            .iter()
            .find(|(k, _)| *k == ORIG_ENV_VAR)
            .map(|(_, v)| v.clone())
            .unwrap();
        let args: Vec<OsString> = vec![
            "hubchat".into(),
            "-c".into(),
            r#"printf 'wsi=%s\ngdk=%s\negl=%s\nmark=%s\nkeep=%s\nargs=%s|%s\n' \
                "${DISABLE_GAMESCOPE_WSI:-}" "${GDK_BACKEND:-}" "${EGL_PLATFORM-<unset>}" \
                "${HUBCHAT_GAMESCOPE_REEXEC:-}" "${APPIMAGE:-}" "$1" "$2""#
                .into(),
            "sh".into(),
            "--hidden".into(),
            "hubchat://chat?to=alex.123456".into(),
        ];
        // First: verify apply_plan removes an existing EGL_PLATFORM even when set earlier.
        let mut seeded = Command::new("/bin/sh");
        seeded.args(&args[1..]);
        seeded.env("EGL_PLATFORM", "wayland");
        seeded.env("APPIMAGE", "/home/steamos/.local/opt/hubchat/hubchat");
        apply_plan(&mut seeded, &plan);
        let out = seeded.output().expect("spawn /bin/sh with seeded env");
        assert!(out.status.success());
        let stdout = String::from_utf8(out.stdout).unwrap();
        let expected = format!(
            "wsi=1\ngdk=x11\negl=<unset>\nmark={pid_str}\nkeep=/home/steamos/.local/opt/hubchat/hubchat\n\
             args=--hidden|hubchat://chat?to=alex.123456\n"
        );
        assert_eq!(stdout, expected);

        // Second: verify build_command end-to-end preserves args, marker, and env.
        let mut cmd = build_command(Path::new("/bin/sh"), &args, &plan);
        cmd.env("APPIMAGE", "/home/steamos/.local/opt/hubchat/hubchat");
        let out = cmd.output().expect("spawn /bin/sh via build_command");
        assert!(out.status.success());
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);

        // Third: verify external child spawned with restore_external_env_from receives
        // the original launching environment with zero Gamescope overrides or markers.
        let mut ext = Command::new("/bin/sh");
        ext.args([
            "-c",
            r#"printf 'wsi=%s\ngdk=%s\negl=%s\nmark=%s\norig=%s\n' \
                "${DISABLE_GAMESCOPE_WSI-<unset>}" "${GDK_BACKEND-<unset>}" \
                "${EGL_PLATFORM-<unset>}" "${HUBCHAT_GAMESCOPE_REEXEC-<unset>}" \
                "${HUBCHAT_GAMESCOPE_ORIG_ENV-<unset>}""#,
        ]);
        apply_plan(&mut ext, &plan);
        restore_external_env_from(&mut ext, Some(&orig_snap));
        let ext_out = ext.output().expect("spawn external child with restored env");
        assert!(ext_out.status.success());
        assert_eq!(
            String::from_utf8(ext_out.stdout).unwrap(),
            "wsi=<unset>\ngdk=<unset>\negl=wayland\nmark=<unset>\norig=<unset>\n"
        );

        // Feeding the post-reexec environment back into plan_reexec must not re-exec again.
        assert_eq!(
            plan_reexec(env_of(&[
                ("XDG_CURRENT_DESKTOP", "gamescope"),
                ("DISPLAY", ":0"),
                ("WAYLAND_DISPLAY", "wayland-0"),
                ("DISABLE_GAMESCOPE_WSI", "1"),
                ("GDK_BACKEND", "x11"),
                (ORIG_ENV_VAR, &orig_snap),
                (REEXEC_MARKER, &pid_str),
            ])),
            None
        );
    }

    // Round 1 review RED tests: M11 (PID-scoped marker, original-env snapshot for external openers).

    #[test]
    fn m11_stale_marker_from_other_pid_does_not_skip_reexec_and_plan_records_orig_env() {
        // A child / subsequent Hubchat process inheriting another PID's marker must NOT
        // skip re-exec when its display environment needs different overrides.
        let other_pid = format!("{}", std::process::id().wrapping_add(10_000));
        let inherited = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            (REEXEC_MARKER, &other_pid),
        ]);
        let plan = plan_reexec(inherited)
            .expect("marker from a different PID must not suppress re-exec on a changed display");
        assert!(
            plan.set.iter().any(|(k, _)| *k == "HUBCHAT_GAMESCOPE_ORIG_ENV"),
            "plan must snapshot original env for external openers: {plan:?}"
        );
        let pid_str = std::process::id().to_string();
        assert!(
            plan.set.iter().any(|(k, v)| *k == REEXEC_MARKER && AsRef::<str>::as_ref(v) == pid_str),
            "re-exec marker must carry the current PID: {plan:?}"
        );
    }

    #[test]
    fn m11_external_openers_route_through_gamescope_env_restoration_on_linux() {
        let commands_rs = include_str!("commands.rs");
        let lib_rs = include_str!("lib.rs");
        let trace_rs = include_str!("trace.rs");
        let tray_rs = include_str!("tray.rs");
        let native_ts = include_str!("../../src/lib/native.ts");
        assert!(
            commands_rs.contains("linux_gamescope::open_external"),
            "commands.rs must use linux_gamescope::open_external on Linux"
        );
        assert!(
            trace_rs.contains("linux_gamescope::open_external"),
            "trace.rs must use linux_gamescope::open_external on Linux"
        );
        assert!(
            tray_rs.contains("linux_gamescope::open_external_path"),
            "tray.rs crash reports folder opener must use linux_gamescope::open_external_path on Linux"
        );
        assert!(
            !commands_rs.contains("fn hc_open_file") && !lib_rs.contains("hc_open_file") && !native_ts.contains("hc_open_file"),
            "M11: hc_open_file must be removed so renderer path opening stays under 1.3.11 Tauri ACLs"
        );
        assert!(
            native_ts.contains("openPath") && native_ts.contains("hc_open_url"),
            "native.ts must keep openFile on plugin-opener and route openLink through hc_open_url"
        );
    }

    #[test]
    fn m11_oversized_graphics_env_value_is_bounded_below_exec_limit() {
        let huge = "x".repeat(70_000);
        let plan = plan_reexec(env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("DISPLAY", ":0"),
            ("GDK_BACKEND", &huge),
            ("EGL_PLATFORM", "wayland"),
        ]))
        .expect("re-exec plan expected");
        for (k, v) in &plan.set {
            assert!(
                v.len() <= 4096,
                "env entry {k} must stay far below Linux 128 KiB MAX_ARG_STRLEN, got {} bytes",
                v.len()
            );
        }
        // Small managed var (EGL_PLATFORM=wayland) is still captured even when GDK_BACKEND was oversized.
        let orig = plan
            .set
            .iter()
            .find(|(k, _)| *k == ORIG_ENV_VAR)
            .map(|(_, v)| v.as_str())
            .expect("ORIG_ENV_VAR present");
        let decoded = decode_orig_env(orig);
        assert!(decoded.contains(&("EGL_PLATFORM", Some("wayland".to_string()))));
        assert!(!decoded.iter().any(|(k, _)| *k == "GDK_BACKEND"));

        // An inherited oversized ORIG_ENV_VAR must also never be copied verbatim into the plan.
        let plan2 = plan_reexec(env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            (ORIG_ENV_VAR, &huge),
        ]))
        .expect("re-exec plan expected");
        let orig2 = plan2
            .set
            .iter()
            .find(|(k, _)| *k == ORIG_ENV_VAR)
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert!(orig2.len() <= 4096, "inherited oversized ORIG_ENV_VAR must be bounded");
    }
}
