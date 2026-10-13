# Windows installer: Upgrade over an existing install

Hubchat's installer behaves like Orgtree's when Hubchat is already installed:
it detects the installation, offers **Upgrade**, closes the running Hubchat
gracefully, installs over it, and starts Hubchat again.

## What the user sees

1. Run the new `Hubchat_<version>_x64-setup.exe` while an older Hubchat is installed.
2. The first page is **Upgrade Hubchat** ("An existing Hubchat installation was
   found."), with the install location and scope (kept unchanged) and two
   buttons: **Upgrade** and **Advanced setup**. (Pressing Next without choosing is
   Advanced setup.)
3. **Upgrade** closes the running Hubchat, installs into the same folder and scope,
   closes the installer by itself and starts Hubchat once. Settings, saved hubs,
   identities, history and keys live in the app data folders, which the installer
   never touches.
4. **Advanced setup** is the stock wizard unchanged (including its "already
   installed" reinstall/uninstall page).

A fresh install (nothing installed) shows no Upgrade page. Silent installs
(`/S`), passive installs (`/P`) and the in-app updater (`/UPDATE`) never show it
and behave as before.

## How Upgrade closes Hubchat

`src-tauri/windows/installer-upgrade.ps1` only touches processes whose
executable path equals the installed `hubchat.exe` exactly (another copy, or a
different product such as "Hubchat Test", is never closed). In order:

1. If the installation says it understands it (the `HubchatQuitControl` value that
   installs from this version on write to the uninstall key), start the installed
   executable with `--quit-for-upgrade`. The single-instance plugin hands that to
   the running Hubchat, which quits exactly like the tray's **Quit Hubchat**
   (`lib.rs`, `desktop::quit_for_upgrade_requested`). Wait up to 25 s.
2. Otherwise, or if it did not quit: post `WM_CLOSE` to every window of those
   processes (a Hubchat set to quit on close does; one that hides in the tray does
   not). Wait 6 s.
3. Only then terminate the process. This is reported: the installer prints
   `WARNING: FORCED: Hubchat did not close gracefully, so it was terminated (pid …)`
   and the helper logs it to `%TEMP%\hubchat-installer-upgrade.log`. If it still
   runs afterwards the installer shows **Retry / Cancel**. Cancel returns to the
   Upgrade page with nothing changed (Hubchat keeps running); from there the user can
   cancel Setup, choose Advanced setup, or try Upgrade again.

The same close runs again right before files are replaced
(`NSIS_HOOK_PREINSTALL`), so a Hubchat started in between is closed the same way.
An installed Hubchat older than this change cannot be asked to quit (step 1 is
skipped), so it ends at step 2 or 3.

`--quit-for-upgrade` as the *first* instance (the old one had already quit)
exits at once instead of becoming a Hubchat.

## How the install runs

Choosing Upgrade sets the stock passive and update modes (`$PassiveMode`,
`$UpdateMode`), the path the in-app updater has always used: same install
location and scope, no uninstall first, existing shortcuts kept, the wizard pages
skipped, the installer closing itself. Then `.onInstSuccess` starts Hubchat once
(directly for the usual non-elevated per-user installer; through `RunAsUser` when
the installer is elevated so Hubchat does not inherit that).

Same version, older and newer installed versions are all offered Upgrade, as in
Orgtree's installer; only a newer installed version with downgrades disallowed
(`allowDowngrades: false`, not the default) stays on the stock page.

## Where it lives

- `src-tauri/windows/installer.nsi`: the stock Tauri 2.12.1 NSIS template
  (`tauri-cli-v2.12.1`, `crates/tauri-bundler/.../nsis/installer.nsi`) with the
  edits marked `HUBCHAT EDIT` / `HUBCHAT END` (git history: the first commit adds
  the unchanged template, the next one the edits, so the diff is small).
  **When the Tauri CLI is upgraded, diff the new stock template against
  commit b2cde24 and port its changes.**
- `src-tauri/windows/installer-hooks.nsh`: `NSIS_HOOK_PREINSTALL` (close again),
  `NSIS_HOOK_POSTINSTALL` (write `HubchatQuitControl`), and `HUBCHAT_WINDOWS_DIR`.
- `src-tauri/windows/installer-upgrade.ps1`: the close helper.
- `src-tauri/tauri.conf.json`: `bundle.windows.nsis.template` and `installerHooks`.
  The one bundle config is used by the local alpha builds and by CI
  (`npx tauri build --bundles nsis`), so both get it; signing and publication are
  unchanged.
- `src-tauri/src/lib.rs`: the `--quit-for-upgrade` handling.

## Proof

`tools/windows-installer-upgrade-proof.ps1` upgrades a DISPOSABLE product
(`Hubchat Upgrade Test`, identifier `dev.orgtree.hubchat.upgrade.test`): silent
prior install, canary in its app data, the app running, the new setup with its
window and an Upgrade click, then it checks the old process is gone and how, the
files and version are the new ones, the canary is untouched, Hubchat was started
once from the install folder, and the real Hubchat is unchanged. It refuses
anything that is not a `.test` identity or whose folders already exist.
`tools/installer-proof/standin-hubchat.cs` is a stand-in `hubchat.exe` for
exercising the close ladder (graceful, hide-in-tray, ignores everything) without
the real app.
