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
renderer TypeScript no-emit check passed in the same containment using installed
dependencies in place. Neither proof exercises native IPC or a device.

An earlier broad core run exposed a nested-future stack overflow; boxing fixed
it. A redundant saved-row condition then broke synthetic runtime fixtures and
one test was stopped after91.5s; the corrected run above passed all162. These
failed receipts remain recorded separately and are not counted as passes.

After the gate, only source/audit/docs changed: atomic identity-bound toggle,
retired-generation clock handling, explicit disabled-hub removal, saved-hub
link/door guards, and added startup/identity controls have NOT been compiled or
tested. All verification is paused until coordinator confirms every paused
Claude task complete; an account reset alone is insufficient. Remaining work:
run the expanded core controls/regression, native adapter/library checks and
changed linking-path proofs, then fresh independent review. There is no claim
of final verification, native linking, WebView, Android runtime, APK, device,
live-message or release validation.
