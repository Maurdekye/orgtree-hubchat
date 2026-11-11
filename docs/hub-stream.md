# Hub stream (Mail hub 2.5.0)

Docket: `mail-hub-websocket-for-new-mail-clients-prefer-i`. Spec:
`spec-hub-websocket.md` sections 9 and 12 (Mail hub project). Code:
`crates/hubchat-core/src/stream.rs`, `hub.rs` (`HubClient.carrier`,
`transport()`), `hub_v2.rs` (`sync`). Proof: `crates/hubchat-core/tests/stream.rs`
(S1 to S8, fake hub).

## What changes

`HubClient::sync`, the long request that carries the sync cursor, is carried
over one WebSocket (`GET /api/stream`, subprotocol `orgtree-hub-stream.1`)
when the hub lists `stream` in `/healthz` `features`, and over HTTP long
polling otherwise (an older hub, or any failure). The request body, the
`SyncResult` and the cursor rules are identical on both carriers; only the
carrier differs. There is no new transport setting or UI (user ruling).

Everything else stays HTTP, unchanged: register, send, ack, receipts, typing,
presence, status, devices, history, files and links.

## Choosing and falling back

- One `Carrier` per `HubClient`; copies of a client share it. It keeps the
  socket between calls, so a cancelled `sync` (the engine's status wake)
  leaves the socket in place and the next call supersedes the parked request.
- `stream` not listed: long poll (`no_feature`), looked at again every 30 min.
- Handshake or welcome failure (non-101, TLS/connect failure, nothing within
  10 s, a close before the welcome): long poll, retry the stream after 1, 5,
  15, then every 60 minutes of long polling; reset after any open stream.
- 4429 after the hello: long poll for 15 min. 1009 or 4400: long poll until
  the app restarts (a bug on one side).
- A stream that drops after it opened is reconnected after 0.5 to 1.5 s, then
  2, 4, 8, 16, 30 s (cap), with jitter. Three sessions in a row that each end
  within 30 s of their welcome with no answer count as unusable.
- 401 at the handshake or a 4401 close is handed to the caller once exactly
  like an HTTP 401 (register once, retry); a second in a row falls back to
  the long poll, which runs the existing 401 handling.
- Switching happens only between requests, so the cursor and the queue never
  move; an unanswered request is asked again with the same cursor.

## Delivery and secrecy

- Same delivery semantics: the client adopts the answer's cursor only after
  storing the answer; a lost or superseded answer leaves the cursor where it
  was, and the store upserts by message id, so a repeat is not a duplicate.
- Native only: a direct connection (no proxy retry for the stream; the long
  poll keeps its own). Credentials go in the `X-Org-Auth` upgrade header, the
  hello carries no `auth`, redirects are not followed, and the secret is never
  logged.
- Diagnostics: `HubClient::transport()` returns `TransportStatus`
  (`transport`, `transport_since_ms`, `fallback_reason`, `next_probe_ms`).

## Verification limits

The tests run against a scripted fake hub. They do not exercise a real
Mail hub 2.5.0, a proxy, or the Android app.
