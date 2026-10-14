//! Start with Windows is on by default (docket
//! hubchat-start-with-windows-on-by-default: user 2026-10-14; the rule for
//! earlier installs approved by coordinator-opus the same day).
//!
//! Hubchat decides once on each device, at its first start with this code.
//! It turns Start with Windows on with the switch's own call (the autostart
//! plugin, whose Run entry starts `hubchat.exe --hidden`, in the tray), and
//! from then on only the person changes it: the switch (tray, Settings ›
//! General) or Windows' Startup apps. An off stays off across restarts and
//! updates.
//!
//! `start-with-windows` in the app data folder says it is decided: `default`
//! (turned on here), `kept` (left as an earlier choice had it), or the
//! switch's last `on` / `off`. A device file, as Exit on close, because
//! switching identity empties the store's settings.
//!
//! Earlier versions kept no record, so the first decision reads what Windows
//! keeps under Hubchat's name:
//!
//! - a Run entry: it is on, or the person turned it off in Startup apps
//!   (which keeps the entry and marks it disabled). It stays as it is;
//! - no Run entry but a StartupApproved record: the plugin writes the record
//!   when it turns Start with Windows on and leaves it when it turns it off,
//!   so it was on and has been turned off, with the switch or by an
//!   uninstall. It stays off;
//! - neither: it was never on. It is turned on;
//! - a new install (no `last-run-version` in the data folder) turns it on
//!   unless there is a Run entry, whatever an uninstalled Hubchat left in
//!   StartupApproved.
//!
//! Accepted limits: the plugin writes no record where Windows has no
//! StartupApproved key yet, so on such a profile an earlier on and off looks
//! like never, and is turned on once; and Hubchat never adds back a Run entry
//! that something else removed (an uninstall that kept the data folder, a
//! cleanup tool).
//!
//! A test build (`*.test`, as for the hubchat:// handler) or a debug build
//! never adds itself to Windows' startup. Windows only: "Start at login" on
//! macOS and Linux is as before.

use std::io;
use std::path::Path;

use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

use crate::desktop::LAST_RUN;

/// The device's record (above).
const FILE: &str = "start-with-windows";

/// Where Windows keeps what starts at sign-in, under HKCU and HKLM (the
/// plugin reads both).
const WINDOWS: &str = r"Software\Microsoft\Windows\CurrentVersion";

/// What Windows keeps for Hubchat's entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// `Run`: on, unless turned off in Startup apps.
    pub run: bool,
    /// `Explorer\StartupApproved\Run`: on or off, as the plugin or Startup
    /// apps last set it.
    pub approved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum First {
    TurnOn,
    Keep,
}

/// The first decision on a device. `ran_before`: Hubchat 1.0 or later has
/// run with this data folder.
pub fn first(ran_before: bool, found: Found) -> First {
    if found.run || (ran_before && found.approved) {
        First::Keep
    } else {
        First::TurnOn
    }
}

/// What a start did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Decided before: nothing to do.
    Decided,
    TurnedOn,
    Kept,
    /// Nothing is recorded, so the next start tries again.
    CouldNotRead,
    CouldNotTurnOn,
}

/// Decide once with the data folder `dir`: `found` reads what Windows keeps,
/// `enable` turns Start with Windows on (true when it did).
pub fn settle_in(
    dir: &Path,
    found: impl FnOnce() -> io::Result<Found>,
    enable: impl FnOnce() -> bool,
) -> Outcome {
    if dir.join(FILE).exists() {
        return Outcome::Decided;
    }
    let ran_before = dir.join(LAST_RUN).exists();
    let Ok(found) = found() else {
        return Outcome::CouldNotRead;
    };
    if first(ran_before, found) == First::Keep {
        record(dir, "kept");
        return Outcome::Kept;
    }
    if !enable() {
        return Outcome::CouldNotTurnOn;
    }
    record(dir, "default");
    Outcome::TurnedOn
}

/// The switch was used: the person's choice, with the data folder `dir`.
pub fn chosen_in(dir: &Path, on: bool) {
    record(dir, if on { "on" } else { "off" });
}

fn record(dir: &Path, what: &str) {
    let saved = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(dir.join(FILE), what));
    if saved.is_err() {
        tracing::warn!("could not save the Start with Windows record");
    }
}

/// What Windows keeps for `name` under `base` in each of `roots`.
fn found_in(roots: &[RegKey], base: &str, name: &str) -> io::Result<Found> {
    let mut found = Found::default();
    for root in roots {
        found.run |= has_value(root, &format!(r"{base}\Run"), name)?;
        found.approved |= has_value(root, &format!(r"{base}\Explorer\StartupApproved\Run"), name)?;
    }
    Ok(found)
}

fn has_value(root: &RegKey, key: &str, name: &str) -> io::Result<bool> {
    match root.open_subkey(key).and_then(|k| k.get_raw_value(name)) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

fn found(name: &str) -> io::Result<Found> {
    let roots = [RegKey::predef(HKEY_CURRENT_USER), RegKey::predef(HKEY_LOCAL_MACHINE)];
    found_in(&roots, WINDOWS, name)
}

/// Setup: decide once on this device, before the tray reads the switch and
/// before `first_start_of_version` notes this start.
pub fn settle(app: &AppHandle) {
    // a test or debug build must not add itself to Windows' startup
    if cfg!(debug_assertions) || app.config().identifier.ends_with(".test") {
        return;
    }
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    // the plugin names its entry after the product
    let name = &app.package_info().name;
    match settle_in(&dir, || found(name), || app.autolaunch().enable().is_ok()) {
        Outcome::Decided => {}
        Outcome::TurnedOn => tracing::info!("Start with Windows: turned on, the default"),
        Outcome::Kept => tracing::info!("Start with Windows: an earlier choice is kept"),
        Outcome::CouldNotRead => tracing::warn!("Start with Windows: could not read Windows' startup entries"),
        Outcome::CouldNotTurnOn => tracing::warn!("Start with Windows: could not turn it on"),
    }
}

/// The switch (tray, Settings › General) was used: from now on only the
/// person turns Start with Windows on.
pub fn chosen(app: &AppHandle, on: bool) {
    if let Ok(dir) = app.path().app_data_dir() {
        chosen_in(&dir, on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const NOTHING: Found = Found { run: false, approved: false };
    /// The Run entry alone: on, or off in Startup apps on a Windows without
    /// the record.
    const RUN: Found = Found { run: true, approved: false };
    /// The Run entry and its record: on, or off in Startup apps.
    const RUN_APPROVED: Found = Found { run: true, approved: true };
    /// The record alone: it was on, and the switch or an uninstall turned it
    /// off.
    const APPROVED: Found = Found { run: false, approved: true };

    /// A data folder of the test's own: a new install's, or one Hubchat has
    /// run with.
    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str, ran_before: bool) -> Folder {
            let d = std::env::temp_dir().join(format!("hubchat-autostart-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            if ran_before {
                std::fs::write(d.join(LAST_RUN), "1.2.0-alpha.3").unwrap();
            }
            Folder(d)
        }

        fn record(&self) -> Option<String> {
            std::fs::read_to_string(self.0.join(FILE)).ok()
        }

        /// A later start: decided, so Windows isn't read and nothing is
        /// turned on.
        fn later_start(&self) -> Outcome {
            settle_in(&self.0, || panic!("Windows read after the decision"), || panic!("turned on after the decision"))
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_first_decision() {
        // (ran before, what Windows keeps, decision): the rule above
        for (ran_before, found, want) in [
            (true, RUN, First::Keep),
            (true, RUN_APPROVED, First::Keep),
            (true, APPROVED, First::Keep),
            (true, NOTHING, First::TurnOn),
            (false, RUN, First::Keep),
            (false, RUN_APPROVED, First::Keep),
            (false, APPROVED, First::TurnOn),
            (false, NOTHING, First::TurnOn),
        ] {
            assert_eq!(first(ran_before, found), want, "ran before: {ran_before}, {found:?}");
        }
    }

    #[test]
    fn a_run_entry_stays_as_it_is() {
        // on, or turned off in Startup apps: an update and a new install alike
        for (name, ran_before, found) in [
            ("run-update", true, RUN),
            ("run-approved-update", true, RUN_APPROVED),
            ("run-new", false, RUN),
            ("run-approved-new", false, RUN_APPROVED),
        ] {
            let d = Folder::new(name, ran_before);
            assert_eq!(settle_in(&d.0, || Ok(found), || panic!("turned on over a Run entry")), Outcome::Kept, "{name}");
            assert_eq!(d.record().as_deref(), Some("kept"), "{name}");
            assert_eq!(d.later_start(), Outcome::Decided, "{name}");
        }
    }

    #[test]
    fn an_update_keeps_off_what_was_turned_off() {
        // the switch removed the Run entry and left the record (or an
        // uninstall did, with the data folder kept)
        let d = Folder::new("turned-off", true);
        assert_eq!(settle_in(&d.0, || Ok(APPROVED), || panic!("turned on after an off")), Outcome::Kept);
        assert_eq!(d.record().as_deref(), Some("kept"));
        assert_eq!(d.later_start(), Outcome::Decided);
    }

    #[test]
    fn an_update_turns_on_what_was_never_on() {
        let d = Folder::new("never-on", true);
        let mut turned_on = 0;
        let outcome = settle_in(&d.0, || Ok(NOTHING), || {
            turned_on += 1;
            true
        });
        assert_eq!((outcome, turned_on), (Outcome::TurnedOn, 1));
        assert_eq!(d.record().as_deref(), Some("default"));
        assert_eq!(d.later_start(), Outcome::Decided);
    }

    #[test]
    fn a_new_install_turns_it_on() {
        let d = Folder::new("new", false);
        assert_eq!(settle_in(&d.0, || Ok(NOTHING), || true), Outcome::TurnedOn);
        assert_eq!(d.record().as_deref(), Some("default"));
        assert_eq!(d.later_start(), Outcome::Decided);
        // whatever an uninstalled Hubchat left in StartupApproved
        let d = Folder::new("new-after-uninstall", false);
        assert_eq!(settle_in(&d.0, || Ok(APPROVED), || true), Outcome::TurnedOn);
        assert_eq!(d.record().as_deref(), Some("default"));
    }

    #[test]
    fn an_explicit_off_stays_off() {
        // on by default, then the person turns it off with the switch
        let d = Folder::new("explicit-off", false);
        assert_eq!(settle_in(&d.0, || Ok(NOTHING), || true), Outcome::TurnedOn);
        chosen_in(&d.0, false);
        assert_eq!(d.record().as_deref(), Some("off"));
        // the next start, and an update (a new version noted): Hubchat reads
        // nothing and turns nothing on, even where Windows keeps no trace of
        // the earlier on (a profile without the StartupApproved key)
        assert_eq!(d.later_start(), Outcome::Decided);
        std::fs::write(d.0.join(LAST_RUN), "1.3.0").unwrap();
        assert_eq!(d.later_start(), Outcome::Decided);
        assert_eq!(d.record().as_deref(), Some("off"));
        // a choice made before the first decision could be recorded counts too
        let d = Folder::new("off-before-decided", true);
        chosen_in(&d.0, false);
        assert_eq!(d.later_start(), Outcome::Decided);
        chosen_in(&d.0, true);
        assert_eq!(d.record().as_deref(), Some("on"));
    }

    #[test]
    fn nothing_is_recorded_until_it_is_decided() {
        let d = Folder::new("retry", true);
        let denied = || Err(io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(settle_in(&d.0, denied, || panic!("turned on unread")), Outcome::CouldNotRead);
        assert_eq!(d.record(), None);
        assert_eq!(settle_in(&d.0, || Ok(NOTHING), || false), Outcome::CouldNotTurnOn);
        assert_eq!(d.record(), None);
        // the next start tries again
        assert_eq!(settle_in(&d.0, || Ok(NOTHING), || true), Outcome::TurnedOn);
        assert_eq!(d.record().as_deref(), Some("default"));
    }

    #[test]
    fn windows_entries_are_read_by_name() {
        use winreg::enums::REG_BINARY;
        use winreg::RegValue;
        // a key of the test's own under HKCU, removed after
        let base = format!(r"Software\hubchat-autostart-test-{}", std::process::id());
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        struct Gone<'a>(&'a RegKey, &'a str);
        impl Drop for Gone<'_> {
            fn drop(&mut self) {
                let _ = self.0.delete_subkey_all(self.1);
            }
        }
        let _ = hkcu.delete_subkey_all(&base);
        let _gone = Gone(&hkcu, &base);
        let roots = [RegKey::predef(HKEY_CURRENT_USER)];
        let read = |name: &str| found_in(&roots, &base, name).unwrap();
        // no keys at all: a Windows without the StartupApproved key
        assert_eq!(read("Hubchat"), NOTHING);
        let (run, _) = hkcu.create_subkey(format!(r"{base}\Run")).unwrap();
        let (approved, _) = hkcu.create_subkey(format!(r"{base}\Explorer\StartupApproved\Run")).unwrap();
        assert_eq!(read("Hubchat"), NOTHING);
        // the record the plugin writes when it turns it on, and leaves after off
        let enabled = RegValue { bytes: vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].into(), vtype: REG_BINARY };
        approved.set_raw_value("Hubchat", &enabled).unwrap();
        assert_eq!(read("Hubchat"), APPROVED);
        run.set_value("Hubchat", &r"C:\Hubchat\hubchat.exe --hidden").unwrap();
        assert_eq!(read("Hubchat"), RUN_APPROVED);
        approved.delete_value("Hubchat").unwrap();
        assert_eq!(read("Hubchat"), RUN);
        // another product's entries are not Hubchat's
        assert_eq!(read("Hubchat Test"), NOTHING);
    }

    #[test]
    fn the_real_entries_can_be_read() {
        // read only, under a name nothing uses: HKCU's and HKLM's keys open
        // without admin rights
        assert_eq!(found(&format!("hubchat-autostart-test-{}", std::process::id())).unwrap(), NOTHING);
    }
}
