# Enable or disable a saved hub

Each saved hub has an independent switch in Settings > Hubs on desktop and
Android. The setting persists on this device only. Existing saved hubs default
to enabled. Disabling preserves the address, identity, roster, local history,
drafts, explicit routing pins, queued messages and transfer checkpoints.

User approved this policy on 2026-10-12 through coordinator lead-astra. Ordinary
messages and notification replies still enter the durable outbox when only a
disabled route exists, displaying Waiting. An explicit pin never silently moves;
Automatic can use another enabled hub. Re-enable starts normal registration and
sync and wakes eligible queue/transfer work. Dispatched requests cannot be
recalled; transport cancellation is best effort. Disable does not unregister the
identity or delete records. Explicit removal remains a separate action.

The device-local `hub.disabled.<url>` meta key requires no migration. Its absence
means enabled. Actual removal clears the key; identity wipe clears meta with the
rest of the identity's data. Shared link bundles retain saved addresses but do
not transmit the device-local flag. Migration16 remains reserved elsewhere.

Engine startup/spawn checks policy while holding the runtime map lock. Disable
removes the runtime and cancels its token. Copied HubClient instances keep that
token; health, registration, polling/sync, history, profile/control calls, body
fetches, uploads and downloads share its network guard. A fresh runtime uses a
fresh token. Status/progress callbacks compare tokens; clock metadata updates
also require the originating generation. The guard boxes work before building
its future to bound nested upload frames. Preview cleanup/publication remains
outside network cancellation; durable downloads retain their checkpoints.

Auxiliary linking uses an active bound client for an already saved hub, including
delayed temporary-address cleanup and door/address discovery. A disabled saved
hub cannot be selected for a link offer. New-device linking to a hub not yet
saved retains its independent setup path. Intentional remove/unregister has an
explicit cleanup client rather than reusing a cancelled runtime token.
Link-start resolves saved existence and enabled state before engine lookup.
Missing/unreadable-key recovery retains saved hubs, so an absent engine cannot
turn a retained hub into unsaved setup. Disabled saved hubs refuse immediately;
enabled saved hubs require an available, uncancelled bound runtime client.

Settings update only after the command succeeds, disable the switch while its
command is pending, preserve it after failure, and refresh state/directory.
Disabled labels and summaries are distinct from offline/refused states. Pinned
conversation messages explain that enabling the hub resumes their route.

## Evidence and current gate

Before the user gate at 2026-10-12 10:48Z, source d707e56 (rebased equivalent
8dbfbdd) passed all162 core unit tests, including8 new toggle controls, with a
warning-free8.12s compile and2.62s test run. These cover local persistence/reopen,
independent state, retained drafts/history/pins, queue and notification UUID
admission, stale picked routes, cancellation and fresh generation callbacks.
Pre-toggle9905800 with only the startup control added (e66940e) fails that one
control as expected: it admits the disabled runtime. No real hub was contacted
by that suspended control. Corrected execution of this added control is pending.

The real shared settings component was mounted in desktop and Android platform
contexts with a synthetic command adapter:16/16 checks passed. Bundling and DOM
execution used the existing repository kernel Job with1536MB/60s limits. A full
renderer TypeScript no-emit exit0 was author-reported in the same containment
using installed dependencies in place; a separate execution log was not supplied
for independent review. Neither proof exercises native IPC or a device.

An earlier broad core run exposed a nested-future stack overflow; boxing fixed
it. A redundant saved-row condition then broke synthetic runtime fixtures and
one test was stopped after91.5s; the corrected run above passed all162. These
failed receipts remain recorded separately and are not counted as passes.

The brief10:48Z verification pause was superseded by user10:53Z: focused tests
and needed compilation may run; publishing remains prohibited. Resumed exact
70b2719 source passed all164 core tests with a warning-free12.66s compile and
2.62s run, including the added startup and retired-generation/identity controls.
The startup control now passes on the corrected source while e66940e fails it.
Earlier source-specific receipts and intermediate failures remain preserved.

Native adapter/library checking on source1ae640c passed without warnings:
`cargo check --locked -j2 -p hubchat --lib --target aarch64-linux-android`,
Android/API24/NDK27,12.08s compiler12.141s wrapper. This compiles the command and
saved-hub linking/door guards. The cancellation helpers have measured core
controls; the complete Tauri link/door workflow has not been executed. Source
review confirms link bundles export addresses, not local enable flags.

The renderer proof is unchanged by these Rust-only additions. Evidence is in
`hub-enable-regression-70b2719.log`, `hub-enable-red-e66940e.log`,
`hub-enable-android-1ae640c.log` and `hub-enable-renderer.json` in the author's
isolated evidence folder. No native linking, IPC, WebView, Android runtime,
APK, device, live-message or release validation is claimed. Independent review
and coordinator integration remain required.

## Independent round1 correction

Review of22de53c found a source-substantiated cold lost-key linking bypass:
the old engine-first match constructed an unbound client without looking at
retained saved policy when no engine existed. No device reproduction was claimed.
The command now calls the shared production `link::start_client` helper, whose
policy lookup precedes its lazy engine lookup and all registration/discovery.

At extracted predecessorbc760c9, all four new helper controls fail: cold reopened
disabled policy is bypassed, saved enabled/no-engine receives an unbound client,
an already-cancelled runtime is selected, and unsaved setup unnecessarily queries
engine availability. The last is an ordering control, not an unsafe network claim.
Correctedb49e7db passes all168 core tests (including the four added controls),
warning-free2.67s compile and2.62s tests. Bound-client registration after token
cancellation returns FilePaused without contacting a hub. Existing164 controls
remain green. Android/API24/NDK27 native library check onb49e7db is warning-free
exit0 in1.76s (1.812s wrapper); it compiles the actual command/helper call.

Receipts: `hub-enable-link-red-bc760c9.log`,
`hub-enable-link-regression-b49e7db.log`, `hub-enable-android-b49e7db.log`.
The cold retained store/absent-engine selection and copied-token controls are
measured components, not a complete Tauri/lost-key UI/link network workflow.
Renderer source is unchanged; earlier16/16 mounted checks remain source-specific,
and the original TypeScript result remains author-reported. No native link,
device, appbuild, publication or live action was performed.
