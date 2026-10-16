# Taskbar and launcher shortcuts

Windows taskbar right-click lists open direct conversations, newest message in
either direction first, followed by Settings, Directory and New chat. These are
native Jump List tasks bound to the application's AppUserModelID. Each launches
the same executable with one encoded Hubchat URL; the single-instance handler
hands that activation to the running window. This does not alter the tray menu
or its single/double-click behavior.

Android 7.1/API25 and later publishes only the last four recently messaged
contacts (or every available contact when fewer exist). Closing a DM does not
erase its recent-contact history. Groups are excluded. Each shortcut uses an
explicit MainActivity intent and the existing pending-link route, for both cold
starts and an already foreground singleTask activity. No Settings, Directory or
New chat shortcut is published on Android. Android7.0/API24 has no dynamic
launcher shortcut API. The launcher controls how many shortcuts it displays.

The projection contains contact addresses and labels, never message bodies,
attachments or drafts. Native engine events refresh it without the main window.
Receipts do not change message ordering, and identical projections do not call
the OS again. Directory updates refresh labels; delete-chat and identity wipe
remove entries. Every contact URL carries its originating identity; the active
layout rejects a different identity's shortcut. Android also disables another
identity's pinned copies the user may have dragged out; same-identity pins remain
usable when a contact leaves the recent four. Dynamic updates can be rate limited
by Android; failures remain retryable on the next data/lifecycle refresh.

Windows view tasks work without an identity (the setup screen keeps the pending
activation until a signed-in layout consumes it). The existing first-start-of-a-
version suppression remains: the first Jump List click immediately after an
update may only show the restored app; later clicks open their target normally.
This prevents replaying the old process's arguments handed over by the updater.
Coordinator ruling 2026-10-16 20:31Z: no launcher exception; restart-argument
cleanup stays separate.

## Author checks

- `node --test tools/launcher-shortcuts.test.mjs tools/chatlink-shortcuts.test.ts`
  covers source wiring and executes the actual URL parser.
- `cargo test -p hubchat-core store_shortcuts -j 2` exercises real in-memory
  SQLite: all open DMs, Android's four across both directions/closed contacts,
  deletions/wipe, labels and group filtering.
- `cargo test -p hubchat --lib jumplist -j 2` constructs real COM shell links
  and checks activation/title, without publishing any shell state.
- `cargo test -p hubchat --lib start_tests -j 2` includes first-version launch.
- `tools/verify-launcher-shortcuts.ps1` compiles the real Kotlin publisher and
  changed ConnectionService bridge against the installed Android37 API
  (unrelated app dependencies are stubbed), then executes five JVM policy
  checks. It requires the shared build
  lock and writes only inside the agent's authorized C: scope.

Native taskbar presentation, launcher UI and a cold/foreground tap on a real
device are separate checks; source/COM/API checks do not claim those results.

## Measured author results (2026-10-16)

Based on Hubchat 1.2.1 `6a4c978`; production source through `f05f891`:

- Initial source-wiring control: three expected failures before the publishers
  existed. Native SQLite stub control `09edaeb`: zero passed, three expected
  failures. A subsequent shared-target run reused that control executable and
  was rejected; the valid implementation run used a separate Cargo target.
- SQLite contact projection: **4/4**. Real Windows COM shell-link construction
  and activation encoding: **2/2**, without publishing any shell state.
  Startup behavior, including preserved first-version suppression: **7/7**.
  These Rust sources have not changed since the successful `192a47f` checks.
- Android `aarch64-linux-android` Rust `cargo check --offline`: **exit 0**, using
  installed NDK 27.3 and two Cargo jobs.
- Actual changed Kotlin publisher, policy and ConnectionService bridge compile
  against installed Android37 APIs; unrelated app dependencies are stubbed.
  JVM policy checks: **5/5**.
- Actual URL parser and source-wiring Node checks: **5/5**. Full renderer
  TypeScript check: **exit 0**, with dependencies resolved from the provisioned
  checkout and no dependency links, copies or installs.

The checks ran under the shared machine build lock and bounded kernel Jobs.
No full APK, installer, installed-app run, shell-state publication or device
action was performed. Full packaging and interactive taskbar/launcher smoke
remain with the integrated Hubchat 1.2.2 alpha, as approved by the coordinator.
