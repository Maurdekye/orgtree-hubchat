//! Crash reports (user 2026-10-11, docket
//! bring-hubchat-tray-menu-in-line-with-orgtree, as Orgtree's tray has them):
//! when Hubchat's Rust side panics, it writes a report on this device, in
//! diagnostics/crash-reports: the version, the thread, where in the code, the
//! message and a backtrace. HUBCHAT NEVER SENDS ONE ANYWHERE. The tray's
//! "Crash reports…" says how many there are and what they can contain, and
//! opens the folder; sending one to anybody is the person's own act.
//!
//! What this does not cover, said here so nobody reads more into it:
//! - Only panics. A native crash that is not a panic (an access violation in
//!   a library, the process being killed) leaves no Hubchat report.
//! - Release builds carry no symbols (Cargo.toml: strip), so a backtrace may
//!   name no functions; the location and the message are always there.
//! - The WebView2 runtime that draws the window keeps its own crash dumps in
//!   its data folder. The dialog counts them and says where they are; whether
//!   the runtime sends them anywhere is not Hubchat's to decide.
//! - Writing a report is best effort and bounded: nothing in the hook
//!   unwraps, a report stops at REPORT_LIMIT bytes, only the newest KEEP
//!   reports stay, and a report that can't be written is given up quietly.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// Where reports go and the version they name, set once at start.
static TARGET: OnceLock<(PathBuf, String)> = OnceLock::new();
/// Tells apart reports written in the same millisecond.
static SEQ: AtomicUsize = AtomicUsize::new(0);
/// Where a panic is also noted besides its report: the log, which appends
/// the line at once (logging's own, set by it). It gets a pointer to the
/// report, never the message: a panic's message can carry content, and that
/// stays in the report.
static NOTE: OnceLock<fn(&str)> = OnceLock::new();

/// Note each panic in `f` too (once; it must never panic itself).
#[allow(dead_code)] // logging sets it
pub fn note_panics_in(f: fn(&str)) {
    let _ = NOTE.set(f);
}

const PREFIX: &str = "panic-";
const SUFFIX: &str = ".txt";
/// How many reports stay: a panic at every start can't fill the disk.
pub const KEEP: usize = 50;
/// The most one report holds.
pub const REPORT_LIMIT: usize = 64 * 1024;

/// Record panics from now on. The previous hook still runs afterwards, so a
/// panic prints as it did.
pub fn install(app_data: &Path, version: &str) {
    if TARGET
        .set((crate::diagnostics::crash_reports(app_data), version.to_string()))
        .is_err()
    {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some((dir, version)) = TARGET.get() {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "(no message)".into());
            let location = info
                .location()
                .map_or_else(|| "(unknown)".into(), |l| l.to_string());
            let thread = std::thread::current()
                .name()
                .unwrap_or("(unnamed)")
                .to_string();
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            let time = hubchat_core::engine::now();
            let text = report(version, &time, &thread, &location, &message, &backtrace);
            let saved = save(dir, &time, &text);
            if let Some(note) = NOTE.get() {
                let report = saved
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "not written".into());
                note(&format!("panic: report {report} thread={thread} at {location}"));
            }
        }
        previous(info);
    }));
}

/// A report's text, cut at REPORT_LIMIT bytes.
pub fn report(version: &str, time: &str, thread: &str, location: &str, message: &str, backtrace: &str) -> String {
    let mut s = format!(
        "Hubchat crash report\nHubchat never sends this file anywhere.\n\nversion: {version}\ntime: {time}\nos: {} {}\nthread: {thread}\nlocation: {location}\nmessage: {message}\n\nbacktrace:\n{backtrace}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    if s.len() > REPORT_LIMIT {
        const CUT: &str = "\n(the rest was cut: a report holds at most 64 KB)\n";
        let mut end = REPORT_LIMIT - CUT.len();
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
        s.push_str(CUT);
    }
    s
}

/// Write a new report into `dir` and keep only the newest KEEP. `time` is an
/// RFC 3339 time; the file is named after it, so names sort oldest first.
pub fn save(dir: &Path, time: &str, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp: String = time
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'T' || *c == 'Z')
        .collect();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("{PREFIX}{stamp}-{}-{seq}{SUFFIX}", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    f.write_all(text.as_bytes())?;
    drop(f);
    let mut names = report_names(dir);
    if names.len() > KEEP {
        names.sort();
        for old in &names[..names.len() - KEEP] {
            let _ = std::fs::remove_file(dir.join(old));
        }
    }
    Ok(path)
}

fn report_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with(PREFIX) && n.ends_with(SUFFIX))
        .collect()
}

/// How many of Hubchat's reports `dir` holds.
pub fn count_reports(dir: &Path) -> usize {
    report_names(dir).len()
}

/// How many crash dumps (`.dmp`) `dir` holds: the WebView2 runtime's.
pub fn count_dumps(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("dmp")))
        .count()
}

/// The WebView2 runtime's crash dumps for this app: its data folder is the
/// app's local data folder, and Crashpad writes into Crashpad\reports there.
#[cfg(windows)]
pub fn webview_dumps(app_local_data: &Path) -> PathBuf {
    app_local_data.join("EBWebView").join("Crashpad").join("reports")
}

/// What "Crash reports…" says: the count, what a report can contain, that
/// Hubchat sends none, the folder, and (Windows) the WebView2 runtime's own
/// dumps. Pure, so the words a person reads before handing a report to
/// anybody are covered by a test.
pub fn dialog_text(reports: usize, folder: &Path, webview: Option<(&Path, usize)>) -> String {
    let head = match reports {
        0 => "Hubchat has recorded no crash reports on this computer.".to_string(),
        1 => "Hubchat has 1 crash report on this computer.".to_string(),
        n => format!("Hubchat has {n} crash reports on this computer."),
    };
    let mut s = format!(
        "{head}\n\nA crash report says what Hubchat was doing when it stopped: its version, where in its code, the error's message and a backtrace. The message can contain addresses, file names or text from your chats.\n\nHubchat never sends one anywhere. Reports stay in this folder until you delete them (Hubchat keeps the newest {KEEP}), and sending one to anybody is something you do yourself.\n\n{}",
        folder.display()
    );
    if let Some((dir, dumps)) = webview {
        let n = match dumps {
            0 => "none at the moment".to_string(),
            1 => "1 at the moment".to_string(),
            n => format!("{n} at the moment"),
        };
        s.push_str(&format!(
            "\n\nThe WebView2 runtime that draws Hubchat's window keeps its own crash dumps ({n}) in:\n{}\nHubchat doesn't decide whether WebView2 sends those anywhere.",
            dir.display()
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hubchat-crash-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn note_to_stderr(line: &str) {
        eprintln!("NOTED {line}");
    }

    /// Runs only as the child of `a_panic_leaves_a_report`, never on its own:
    /// the hook goes into the folder the parent names, then a thread panics.
    #[test]
    fn panicking_child() {
        let Ok(dir) = std::env::var("HUBCHAT_CRASH_TEST_DIR") else {
            return;
        };
        install(Path::new(&dir), "9.9.9-test");
        note_panics_in(note_to_stderr);
        let worker = std::thread::Builder::new()
            .name("fake-worker".into())
            .spawn(|| {
                let none: Option<u8> = None;
                none.expect("the fake panic of the crash report test");
            })
            .expect("a thread");
        assert!(worker.join().is_err(), "the worker panicked");
    }

    #[test]
    fn a_panic_leaves_a_report() {
        let app_data = temp("child");
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crash::tests::panicking_child", "--nocapture", "--test-threads=1"])
            .env("HUBCHAT_CRASH_TEST_DIR", &app_data)
            .output()
            .expect("the child test ran");
        assert!(out.status.success(), "child: {}", String::from_utf8_lossy(&out.stderr));
        let dir = crate::diagnostics::crash_reports(&app_data);
        assert_eq!(count_reports(&dir), 1);
        let name = report_names(&dir).remove(0);
        let text = std::fs::read_to_string(dir.join(&name)).unwrap();
        println!("{name}\n{text}");
        assert!(text.starts_with("Hubchat crash report\nHubchat never sends this file anywhere.\n"), "{text}");
        assert!(text.contains("\nversion: 9.9.9-test\n"), "{text}");
        assert!(text.contains("\nthread: fake-worker\n"), "{text}");
        assert!(text.contains("\nmessage: the fake panic of the crash report test\n"), "{text}");
        assert!(text.contains("\nlocation: ") && text.contains("crash.rs:"), "{text}");
        assert!(text.contains("\nbacktrace:\n"), "{text}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        // the default hook still ran after ours
        assert!(stderr.contains("panicked at") && stderr.contains("the fake panic of the crash report test"), "{stderr}");
        // the log got a pointer to the report, not the message
        let noted = stderr.lines().find(|l| l.starts_with("NOTED ")).unwrap_or_default();
        assert!(noted.starts_with(&format!("NOTED panic: report {name} thread=fake-worker at ")), "{stderr}");
        assert!(noted.contains("crash.rs:") && !noted.contains("fake panic"), "{noted}");
        let _ = std::fs::remove_dir_all(&app_data);
    }

    #[test]
    fn only_the_newest_reports_stay() {
        let dir = temp("keep");
        for i in 0..KEEP + 5 {
            let time = format!("2026-10-11T22:{:02}:{:02}.000Z", i / 60, i % 60);
            save(&dir, &time, &format!("report {i}")).unwrap();
        }
        let mut names = report_names(&dir);
        names.sort();
        assert_eq!(names.len(), KEEP);
        assert!(names[0].starts_with("panic-20261011T220005000Z-"), "{}", names[0]);
        // other files in the folder are left alone and not counted
        std::fs::write(dir.join("notes.txt"), "mine").unwrap();
        save(&dir, "2026-10-11T23:00:00.000Z", "one more").unwrap();
        assert_eq!(count_reports(&dir), KEEP);
        assert!(dir.join("notes.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_report_stops_at_the_limit() {
        let long = "x".repeat(REPORT_LIMIT * 2);
        let r = report("1.1.0", "t", "main", "src/x.rs:1:1", "é", &long);
        assert!(r.len() <= REPORT_LIMIT, "{}", r.len());
        assert!(r.ends_with("(the rest was cut: a report holds at most 64 KB)\n"));
        let short = report("1.1.0", "t", "main", "src/x.rs:1:1", "boom", "frames");
        assert!(short.contains("message: boom\n\nbacktrace:\nframes\n"), "{short}");
    }

    #[test]
    fn dumps_are_counted_by_extension() {
        let dir = temp("dumps");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.dmp"), "").unwrap();
        std::fs::write(dir.join("b.DMP"), "").unwrap();
        std::fs::write(dir.join("settings.dat"), "").unwrap();
        assert_eq!(count_dumps(&dir), 2);
        assert_eq!(count_dumps(&dir.join("missing")), 0);
        assert_eq!(count_reports(&dir.join("missing")), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_dialog_says_what_a_report_holds_and_that_none_is_sent() {
        let folder = Path::new("C:/data/diagnostics/crash-reports");
        let none = dialog_text(0, folder, None);
        assert!(none.starts_with("Hubchat has recorded no crash reports on this computer.\n\n"), "{none}");
        assert!(none.contains("Hubchat never sends one anywhere."), "{none}");
        assert!(none.contains("addresses, file names or text from your chats"), "{none}");
        assert!(none.ends_with(&folder.display().to_string()), "{none}");
        assert!(!none.contains("WebView2"), "{none}");
        assert!(dialog_text(1, folder, None).starts_with("Hubchat has 1 crash report on this computer."));
        let both = dialog_text(3, folder, Some((Path::new("C:/local/EBWebView/Crashpad/reports"), 4)));
        assert!(both.starts_with("Hubchat has 3 crash reports on this computer."), "{both}");
        assert!(both.contains("keeps its own crash dumps (4 at the moment) in:\nC:/local/EBWebView/Crashpad/reports\n"), "{both}");
        assert!(both.ends_with("Hubchat doesn't decide whether WebView2 sends those anywhere."), "{both}");
    }
}
