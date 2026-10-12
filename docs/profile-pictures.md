# Identity profile pictures

User-approved 2026-10-12: person and org identities have pictures; direct AI
sessions (`kind=chat`) cannot own one. Eligible pictures still appear in chat
surfaces. Own Profile is the desktop/Android editor. There is no per-chat image.

## Image boundary

Native decoding accepts static PNG/JPEG/WebP, at most5MiB and16MP. Format sniffing,
animation refusal and dimensions precede full pixel allocation. Orientation is
applied before the shared editor's square crop. It receives a stripped bounded
preview, not the original in an image element. Output is RGB/RGBA PNG with only
IHDR/IDAT/IEND, at most512 square and256KiB; noisy images shrink until bounded.
SVG/animation are not supported. HTTP accepts only verified normalized PNG and
never redirects credentials to an image URL. No bytes enter diagnostic strings.

## Versions and synchronization

Wire records contain revision, sha256, width, height, bytes and base64 data.
Removal is null data/hash and zero dimensions/bytes. Metadata omits data. Revision
is20 zero-padded counter digits, hyphen,32 lowercase random operation-ID digits.
Only explicit edits increment the greatest observed counter under the local
store lock. Automatic publication/reconnect never increments it. Lexicographic
greatest wins; equal conflicting metadata cannot overwrite the winner. Concurrent
unseen edits converge by operation ID, independently of wall-clock time.

The native store uses existing meta keys; SQLite migration16 remains reserved
for paused question work. Metadata high-water marks survive byte-cache eviction.
Peer bytes are capped at80 records (20MiB decoded PNG maximum) plus our own image;
identity wipe deletes the whole namespace. Owner checks inside transactions
refuse late old-engine results after identity adoption. A removal clears bytes
as soon as metadata arrives, before another fetch can finish.

Hub capability `profile_pictures` gates a picture future inside the existing
active session. It runs concurrently with mail polling and drops with the session.
Initial authenticated GET learns newer state before POST. Subsequent publication
is driven by existing poll/roster changes and explicit edits, with per-hub revision
acks avoiding repeat bytes. Failures retain fallback and retry on an existing
update with backoff; they never fail a message send. Peer bytes load on demand
through active capable hubs. Old hubs/clients retain their initials/glyph behavior.
Both picture HTTP methods run through the originating HubClient `session_work`,
including response-body reads. Local image/ack and roster-picture admission use
the same generation's short `session_commit` guard. Disabling a saved hub cancels
held requests and refuses late effects; re-enabling creates a fresh generation
and does not revive retained clients. Existing discovery contexts are unchanged.

## Verification checkpoint

Thirteen focused native tests pass for normalization/bounds, replay/removal ordering,
byte-cache size, identity guards, actual native HTTP reconnect and two-hub relay.
The relay fixture checks the POST body after a newer removal is read, rather than
only checking local state. Eleven mounted renderer checks pass for the shared
avatar/editor through synthetic native commands. Native Hubchat library check
passes including command registration; frontend TypeScript noEmit passes using
existing dependencies. These are component/bridge compilation proofs, not an
installed desktop, Android WebView, device, live hub or pixel acceptance run.

Image0.25.9 is pinned to preserve declared Rust1.85 compatibility (0.25.10 requires
1.88). Both the native command check and the final nine focused core tests now
include0.25.9; earlier0.25.6 receipts remain separate. The added renderer cases
exercise native-preview display, passing original bytes/changed square crop to
Save, and rejecting a late preview after an owner change. Organization settings
publication is a separate Orgtree landing. Final independent review is pending.
No release or deployment is
authorized by these results.

At 10:48Z on 2026-10-12 the user paused every build/check/test until the
coordinator confirms completion of paused Claude work. Later source-only
hardening shares one decoder permit between preview and save and moves it into
the blocking closure, so cancelled callers cannot release a still-running
decoder's slot. The user superseded the pause at10:53Z, allowing focused tests
again but no publication. The slot change now compiles through the actual
native command library; forced-cancellation runtime behavior is source-inferred.
After composition onto the integrated saved-hub base `acdeab4`, all194 core
tests pass (13 picture cases), the Windows native command library check passes,
and all11 mounted renderer cases and TypeScript noEmit pass again. The four
added composition cases cover old clients before dispatch, fresh-generation
positive controls, actual GET/POST body stalls cancelled after dispatch, and
late image/ack/roster admission. The latter exercises the production admission
helper directly; it does not claim a scheduled whole-app race. Renderer runs,
including bundling, used kernel Job memory/deadline containment. No Android
library/device/WebView build or pixel acceptance was performed for this feature.
