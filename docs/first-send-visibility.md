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

The round1 review found that the pre-existing history reader could overwrite
an admission's lower boundary after reading stale metadata. All production
writers now share a short atomic merge against the current persisted value.
Remote/history candidates are computed outside the connection lock; the reader
filters using the returned winner, including a concurrently established `all`,
instead of its stale candidate. There is no engine-wide lock or transaction
across remote work.

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

## Round2 concurrency correction

Predecessor `2060db5` extracts the original reader behind a candidate closure
without changing its behavior. Barriers pause that real reader, commit a real
ordinary or notification admission on another thread, then resume it. All four
combinations (absent/numeric prior floor, ordinary/notification) overwrite the
admission on the predecessor. A fifth controlled interleaving loses the `all`
sentinel. The four original first-send controls still pass there: measured red
is four passing and five failing tests, not a failure of compilation or setup.

Correction `9b9e802` passes all nine focused tests. Each reader returns a floor
that allows its accepted row; subsequent actual `Engine::chat` reloads show one
queued row. Notification UUID replay stays consumed without a duplicate or
boundary mutation. All 154 core unit tests pass after the correction, including
the original notification/identity and Close DM controls. These are deterministic
production-component schedules, not a Tauri/WebView race reproduction or proof
of the user's runtime cause.

The contained renderer receipts now serialize all checks as explicit booleans;
red mode requires the exact three expected failing names. Original round1
receipts remain retained separately from the round2 evidence.

## Sync retiming correction (2026-10-19)

The earlier correction `9b9e802` is still on the base for this fix. It covered
admission and competing floor writers, but not later hub sync choosing an
earlier `created_at` for the same accepted UUID. A two-second clock correction
reproduces a persisted, sent first row falling below its admission floor. A
range reload anchored to its old timestamp independently excludes that UUID.
This controlled ordering matches the intermittent symptom; the user's live
clock, data and runtime were not inspected.

The sync transaction now preserves visibility only when the existing row was
already above the persisted numeric floor. Its new earlier time lowers that
floor in the same transaction. Hidden historical rows cannot establish this
visibility, and `all` remains unchanged. The chat range query resolves the
same peer/UUID anchor's current timestamp in its own SQLite snapshot and uses
the earlier of that and the caller's supplied time. Missing or other-peer
anchors retain the supplied range. No migration, renderer send path or DM
reopening rule changes.

At test-only `b14707a`, the focused native red is exactly one retiming failure
and ten passing controls. At test-only `8e87502`, the mounted renderer red is
exactly five expected failures and four controls per layout (desktop/Android).
The real Conversation/Composer consumes the native query fixture: a single
accepted send, no inbound message and no native event are needed for the
initial reload. A subsequent event exercises an already loaded anchor.

Product correction `63a0025` passes the same eleven native tests and all
eighteen renderer checks. Both layouts retain details when genuinely empty,
clear the composer after admission, show the sent bubble and hide the intro
without a reply; retimed range reloads retain that state. A single combined
Chromium capture shows both layouts. Native tests and the core compile check
use cargo `-j1` under a 6144 MiB kernel Job; renderer runs use a 3072 MiB Job.
All phases require a current `build-slot` lease and assigned C: evidence paths.
The repeatable focused fixture is `tools/proof-first-send-retime.mjs`, with
native export `HUBCHAT_RETIMED_SEND_FIXTURE` and contained phase wrappers in
the author's evidence root.

Two initial renderer attempts timed out without a verdict: hidden iframe
animation frames stopped after conversation auto-scroll. The fixture now
settles React/event timers separately; the capture host still waits for fonts
and two animation frames before capture. The failed setup receipts remain
retained. No full suite, independent review, app build, Tauri/device run, live
send, install or release was performed for this phase-1 correction.
