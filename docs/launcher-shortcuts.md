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
layout rejects a different identity's shortcut. Android also disables stale
pinned copies the user may have dragged out. Dynamic updates can be rate limited
by Android; failures remain retryable on the next data/lifecycle refresh.

Windows view tasks work without an identity (the setup screen keeps the pending
activation until a signed-in layout consumes it). Explicit launcher activations
are admitted on the first start of a new version, while ordinary stale setup and
chat arguments keep the existing first-version suppression.

## Author checks

- `node --test tools/launcher-shortcuts.test.mjs tools/chatlink-shortcuts.test.ts`
  covers source wiring and executes the actual URL parser.
- `cargo test -p hubchat-core store_shortcuts -j 2` exercises real in-memory
  SQLite: all open DMs, Android's four across both directions/closed contacts,
  deletions/wipe, labels and group filtering.
- `cargo test -p hubchat --lib jumplist -j 2` constructs real COM shell links
  and checks activation/title, without publishing any shell state.
- `cargo test -p hubchat --lib start_tests -j 2` includes first-version launch.
- `tools/verify-launcher-shortcuts.ps1` compiles the real Kotlin publisher
  against the installed Android37 API (only MainActivity's inheritance is
  stubbed), then executes five JVM policy checks. It requires the shared build
  lock and writes only inside the agent's authorized C: scope.

Native taskbar presentation, launcher UI and a cold/foreground tap on a real
device are separate checks; source/COM/API checks do not claim those results.
