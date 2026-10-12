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
The new saved-hub disable contract must be composed through HubClient session_work
before integration; its author owns that separate feature.

## Verification checkpoint

Nine focused native tests pass for normalization/bounds, replay/removal ordering,
byte-cache size, identity guards, actual native HTTP reconnect and two-hub relay.
The relay fixture checks the POST body after a newer removal is read, rather than
only checking local state. Eight mounted renderer checks pass for the shared
avatar/editor through synthetic native commands. Native Hubchat library check
passes including command registration; frontend TypeScript noEmit passes using
existing dependencies. These are component/bridge compilation proofs, not an
installed desktop, Android WebView, device, live hub or pixel acceptance run.

Image0.25.9 is pinned to preserve declared Rust1.85 compatibility (0.25.10 requires
1.88). Native command check includes0.25.9; the focused tests recorded at this
checkpoint used0.25.6 and require the final dependency-composed rerun. Organization
settings publication and final independent review are still pending. No release
or deployment is authorized by these results.
