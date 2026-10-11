# The tray menu (desktop)

Right-clicking Hubchat's tray icon opens a menu in line with Orgtree's tray,
made of Hubchat's own settings and behaviour (docket
bring-hubchat-tray-menu-in-line-with-orgtree; user 2026-10-11, mappings
approved by lead-astra 2026-10-11 22:36Z). A left click still shows the
window. The code is `src-tauri/src/tray.rs` (the menu), `src-tauri/src/crash.rs`
(crash reports) and `src/lib/tray.ts` (the UI's side).

| Row | What it does | Where the value lives |
|---|---|---|
| Open Hubchat | Shows the window. | |
| Version 1.1.0 | Greyed: the version that is running. | the app |
| Update status | Greyed. One of: Updates have not been checked · Checking for updates… · Hubchat is up to date · Hubchat 1.2.0 is available · Couldn't check for updates · Updating to Hubchat 1.2.0… · The update didn't install | the updater in the UI (`src/lib/updates.ts`) |
| Restart to update | Only once a check found an update. Installs it and restarts, as the banner's and Settings › About's buttons do. | |
| Check for updates automatically | When Hubchat starts and every 6 hours. Updates never install on their own. | the same switch as Settings › About |
| Check for updates | Checks now. The result shows on the status row and in Settings › About. | |
| Start with Windows | Starts hidden in the tray at sign-in. "Start at login" on macOS and Linux. | the autostart plugin's entry (on Windows the Run key, which Windows' Startup apps can also turn off); Settings › General |
| Exit on close | Off unless turned on. On: closing the window quits Hubchat, as Quit Hubchat does. Off: closing hides the window, so messages and transfers keep going (design §7). | `exit-on-close` in the app data folder, so it belongs to the device and survives switching identity; Settings › General |
| Notifications › Show notifications, Show message text, Play a sound | The three switches of Settings › Notifications. The last two are greyed while notifications are off. "Show notifications" covers message notifications and the notices of files that couldn't be sent or downloaded. | the core's store |
| Crash reports… | Says how many crash reports there are, what they can contain, that Hubchat sends none, and offers Open folder. | `diagnostics/crash-reports` in the app data folder |
| Restart Hubchat | Restarts Hubchat. It comes back on screen if the window was showing, and hidden in the tray if not. | |
| Quit Hubchat | Quits, as before. | |

## Keeping every place truthful

- The menu is built once and changed in place, never rebuilt, so a menu that
  is open is never torn down under the pointer.
- Whatever changes a value also updates the tray: Settings › Notifications
  (`hc_set_notifications`), Start with Windows (`hc_set_autostart`, which
  Settings › General now uses instead of calling the plugin itself), Exit on
  close (`hc_set_exit_on_close`), and the updater, which reports each change
  of state (`hc_tray_updates`).
- A setting changed in the tray reaches the UI as `hc-settings`, and Settings
  reads it again.
- Windows' Startup apps can turn Start with Windows off behind Hubchat's back,
  so the tray reads it again when the pointer comes to the icon and on a click.
- A check item has already flipped its tick when its click arrives: that tick
  is the request, and the item then shows what really happened (a failure puts
  it back and says why).
- The updater keeps one state for Settings › About and the tray: a check under
  way is joined rather than started twice; nothing is checked while an update
  installs; a failed background check stays silent, as before, while a failed
  check someone asked for shows in both places. Every desktop install
  (banner, Settings, the setup screen, the tray) goes through the same guarded
  function: none starts while a check runs (a check can replace the update),
  a second request joins the running install and gets its outcome, and a
  finishing check never overwrites an install's state. The banner shows that
  shared state rather than its own (`tools/proof-update-races.mjs`).
- The update rows that ask the UI for something stay greyed until the UI has
  reported; the UI starts its side at boot, so the tray works during
  onboarding too. Automatic checks still start only once the main screen is up.

## Restart

"Restart Hubchat" frees the single-instance lock first, then restarts through
Tauri. Without that, the new process could find the old one still running,
hand it its start and quit: Tauri frees the lock on its Exit event, which a
restart from the main thread never sends. The new process gets no arguments,
or only `--hidden` when the window was hidden. It never gets the old process's
own arguments, because a `hubchat://` link it was started with (a setup or
device code) must not open again.

## Crash reports (user ruling 2026-10-11)

The user chose local reports (m9cd2b977789b via lead-astra):

- When Hubchat's Rust side panics, it writes
  `diagnostics/crash-reports/panic-<time>-<process>-<n>.txt` in the app data
  folder. The report holds the version, time, OS, thread, location, message and
  a backtrace.
- **Hubchat never sends a report anywhere.** Sending one is the person's own act.
- Writing is best effort and bounded. The hook never unwraps, a report stops at
  64 KB, only the newest 50 reports stay, a report that can't be written is
  given up quietly, and the previous panic hook still runs afterwards.
- The log gets one line pointing at the report, never the message:
  `crash::note_panics_in`, set by logging. A panic's message can carry
  content, so it stays only in the report.
- On Windows the dialog also counts the WebView2 runtime's own crash dumps of
  the window's processes, in `EBWebView\Crashpad\reports` under the local app
  data folder. It says where they are, and that Hubchat doesn't decide whether
  WebView2 sends them anywhere.

What it does not cover:

- Only panics are recorded. A native crash that is not a panic (an access
  violation in a library, or the process being killed) leaves no Hubchat
  report.
- A panic before the app's setup runs (while plugins start) leaves none either.
- Release builds carry no symbols (`strip` in Cargo.toml), so a backtrace may
  name no functions. The location and the message are always there.
