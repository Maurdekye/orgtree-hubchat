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
//! `XDG_CURRENT_DESKTOP=gamescope`, loop-proof via `HUBCHAT_GAMESCOPE_REEXEC`,
//! and a no-op when the environment is already configured.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

pub const REEXEC_MARKER: &str = "HUBCHAT_GAMESCOPE_REEXEC";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReexecPlan {
    pub set: Vec<(&'static str, &'static str)>,
    pub remove: Vec<&'static str>,
}

fn non_empty(val: Option<String>) -> bool {
    val.is_some_and(|s| !s.is_empty())
}

/// Compute the Gamescope re-exec environment plan, or `None` if no re-exec is
/// needed (outside `XDG_CURRENT_DESKTOP=gamescope`, marker already set, or all
/// required graphics variables are already in the desired state).
pub fn plan_reexec(env: impl Fn(&str) -> Option<String>) -> Option<ReexecPlan> {
    if non_empty(env(REEXEC_MARKER)) {
        return None;
    }
    if env("XDG_CURRENT_DESKTOP").as_deref() != Some("gamescope") {
        return None;
    }
    let has_wayland = non_empty(env("WAYLAND_DISPLAY"));
    let has_x11 = non_empty(env("DISPLAY"));

    let mut set: Vec<(&'static str, &'static str)> = vec![("DISABLE_GAMESCOPE_WSI", "1")];
    let mut remove: Vec<&'static str> = Vec::new();
    if has_wayland {
        if has_x11 {
            set.push(("GDK_BACKEND", "x11"));
            remove.push("EGL_PLATFORM");
        } else {
            set.push(("WEBKIT_DISABLE_DMABUF_RENDERER", "1"));
        }
    }

    let already_set = set.iter().all(|(k, v)| env(k).as_deref() == Some(*v));
    let already_removed = remove.iter().all(|k| env(k).is_none());
    if already_set && already_removed {
        return None;
    }

    set.push((REEXEC_MARKER, "1"));
    Some(ReexecPlan { set, remove })
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

/// Re-exec the current executable in place with Gamescope graphics env vars
/// before GTK/WebKit or single-instance setup runs.
#[cfg(target_os = "linux")]
pub fn reexec_if_needed() {
    use std::os::unix::process::CommandExt;
    let Some(plan) = plan_reexec(|k| std::env::var(k).ok()) else {
        return;
    };
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let args: Vec<OsString> = std::env::args_os().collect();
    let _ = build_command(&exe, &args, &plan).exec();
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
        let env = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("EGL_PLATFORM", "wayland"),
            (REEXEC_MARKER, "1"),
        ]);
        assert_eq!(plan_reexec(env), None);
    }

    #[test]
    fn gamescope_both_displays_sets_wsi_x11_and_unsets_egl_platform() {
        let env = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("EGL_PLATFORM", "wayland"),
        ]);
        assert_eq!(
            plan_reexec(env),
            Some(ReexecPlan {
                set: vec![
                    ("DISABLE_GAMESCOPE_WSI", "1"),
                    ("GDK_BACKEND", "x11"),
                    (REEXEC_MARKER, "1"),
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

        // Already configured launch (e.g. launch-hubchat): no re-exec needed.
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
        let env = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("WAYLAND_DISPLAY", "wayland-0"),
        ]);
        assert_eq!(
            plan_reexec(env),
            Some(ReexecPlan {
                set: vec![
                    ("DISABLE_GAMESCOPE_WSI", "1"),
                    ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
                    (REEXEC_MARKER, "1"),
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
                    ("DISABLE_GAMESCOPE_WSI", "1"),
                    ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
                    (REEXEC_MARKER, "1"),
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
        let env = env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("EGL_PLATFORM", "x11"),
        ]);
        assert_eq!(
            plan_reexec(env),
            Some(ReexecPlan {
                set: vec![("DISABLE_GAMESCOPE_WSI", "1"), (REEXEC_MARKER, "1")],
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
                ("DISABLE_GAMESCOPE_WSI", "1"),
                ("GDK_BACKEND", "x11"),
                (REEXEC_MARKER, "1"),
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
            Some(&Some(std::ffi::OsStr::new("1")))
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

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_reexec_command_applies_env_unsets_egl_preserves_args_and_stops_loop() {
        let plan = plan_reexec(env_of(&[
            ("XDG_CURRENT_DESKTOP", "gamescope"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("EGL_PLATFORM", "wayland"),
        ]))
        .expect("re-exec planned");
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
        let expected = "wsi=1\ngdk=x11\negl=<unset>\nmark=1\nkeep=/home/steamos/.local/opt/hubchat/hubchat\n\
             args=--hidden|hubchat://chat?to=alex.123456\n";
        assert_eq!(stdout, expected);

        // Second: verify build_command end-to-end preserves args, marker, and env.
        let mut cmd = build_command(Path::new("/bin/sh"), &args, &plan);
        cmd.env("APPIMAGE", "/home/steamos/.local/opt/hubchat/hubchat");
        let out = cmd.output().expect("spawn /bin/sh via build_command");
        assert!(out.status.success());
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
        // Feeding the post-reexec environment back into plan_reexec must not re-exec again.
        assert_eq!(
            plan_reexec(env_of(&[
                ("XDG_CURRENT_DESKTOP", "gamescope"),
                ("DISPLAY", ":0"),
                ("WAYLAND_DISPLAY", "wayland-0"),
                ("DISABLE_GAMESCOPE_WSI", "1"),
                ("GDK_BACKEND", "x11"),
                (REEXEC_MARKER, "1"),
            ])),
            None
        );
    }
}
