# First outgoing message visibility

The reported surface is the standalone desktop conversation: after a first
send it still looks like a new chat, until a reply arrives. The send command
already queues the message durably, emits `Chat`, and the mounted composer also
reloads history after successful admission. The controlled ordinary desktop
case displays its first send even when the native event is withheld.

The measured defect is below that renderer: `Engine::chat` filters rows below
the lazy-history boundary, including a newly accepted local outgoing message.
A connected boundary ahead of the local admission time hides that row despite
a successful command and event. A send accepted offline before a later lazy
sync start can also be hidden. The user's actual clock/hub state has not been
read, so this boundary is a reproduced source defect, not a confirmed diagnosis
of that particular desktop installation. Appearance specifically after the
user's inbound reply has not been reproduced.

Local admission now records a visible history boundary in the same SQLite
transaction as its message and prepared attachments. It only lowers the
existing boundary, preserves `all`, and survives later lazy starts. Notification
reply admission uses the same helper in its receipt/message transaction.
Historical hub imports do not establish a local boundary. Message UUIDs,
delivery states, chronological paging, sender wakes and Close DM visibility
remain governed by their existing paths; there is no new send or polling path.
No migration is needed.

## Measured verification

- Native predecessor `db32a93`: both first-send controls fail. The persisted
  queued row and emitted `Chat` exist, while `Engine::chat` returns no row.
- Corrected native tests: four pass, covering the connected floor, later lazy
  start after offline admission, subsequent sends and range reads, pending/
  failed/sent/delivered visibility, duplicate transaction rollback, monotone/
  `all` boundary handling, retained closed DMs, historical outgoing/incoming
  exclusion and live incoming duplicate control.
- All 149 core unit tests pass, including notification receipt/UUID/identity
  and Close DM controls. No integration test suite or live hub was run.
- Real mounted desktop `Conversation` and `Composer`, using the native test's
  serialized query projection: predecessor has three passing controls and
  three expected failures (bubble, empty-state removal, waiting indicator).
  Corrected fixture passes six checks without inbound traffic or a native
  event. The ordinary synthetic no-floor control also passes six.
- Every DOM proof, including its single-entry bundle, runs inside the existing
  repository Windows Job runner: 1536 MB job memory ceiling, 60 second deadline,
  kill-on-close. Dependencies are read in place; products/temp stay in the
  caller's assigned build root. This is a DOM/IPC-shape composition proof, not
  a live Tauri command, WebView or installed-app reproduction.

## Repeating the isolated proof

With the machine build lock, run
`cargo test --locked -j2 -p hubchat-core --lib first_send_tests -- --nocapture`.
Set `HUBCHAT_FIRST_SEND_FIXTURE` to an isolated evidence JSON path to capture
the native persisted and visible first-message rows. For red/green worktrees,
use separate assigned cargo targets or verify that the changed crate really
recompiles: sharing the same target across worktree paths can reuse stale cargo
artifacts. The recorded green compiler line names the corrected worktree.

Run `tools/verify-first-send-renderer.ps1` with these explicit parameters:
`-JobRunner` (existing Orgtree `apps/desktop/renderer/tests/joblimit.ps1`),
`-DependencyRoot` (installed Hubchat React 19/Tauri packages),
`-DomDependencyRoot` (installed esbuild/jsdom packages), `-OutputRoot`
(assigned build folder), `-EvidencePath` (isolated JSON), `-Fixture` (native
JSON), and `-Phase green`. `-Phase red` requires the three specific missing-row
failures; `-Phase control` without a fixture uses a synthetic accepted row.
Never run the renderer script outside the contained verifier.

Independent source review and coordinator integration remain required. No app,
APK, device, live data, real chat send, install or release verification was done.
