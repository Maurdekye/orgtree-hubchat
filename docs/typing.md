# Typing

Docket: `show-agent-typing-in-hubchat-using-ordered-messa`. Hubchat shows
"<name> is typing…" while someone writes a message to us, and tells the
people we write to while our user writes. Both directions use the mail hub's
`typing` feature (hub `docs/v2-additions.md`), and only hubs that list it in
`/healthz` are used. An older hub gets nothing and sends nothing.

## Heartbeats, not start and stop (user 2026-10-13 22:34Z)

A writer sends a **heartbeat** about every 500 ms while it writes. There is
no start message and no stop message: the heartbeats simply stop. The
receiver shows the indicator while the last heartbeat **arrived** within one
second, by its own clock. So the indicator can't get stuck: a writer that
crashes, loses its connection or forgets to stop leaves nothing behind, and
clock skew between machines plays no part.

## Shown: someone writes to us

- Each sync asks for heartbeats when the hub lists `typing`: `"typing":
  true` at first, then the last answer's `typing_seq`, so the hub tells each
  heartbeat once.
- A device **in use** gets each heartbeat at once. The hub wakes its parked
  sync and answers from memory with a **typing-only answer**
  (`typing_only: true`, the same cursor, no changes). The core takes that on
  a fast path: it tells the window, stores nothing (no cursor, no clock
  reading), waits `PACE` (250 ms), and syncs again. Heartbeats that come
  during that wait come together in the next answer, so a device takes at
  most four typing-only answers a second, however many people write to it.
- The core emits `Event::Typing { peer }` once per heartbeat. A message from
  the same peer in the same answer wins: its heartbeat is dropped.
- The window (`src/lib/store.ts`) keeps when each peer's last heartbeat came
  (`performance.now()`). The peer shows as typing while that is under
  `TYPING_WINDOW_MS` (1000 ms) old. A timer ends the row a second after the
  last heartbeat, and an `incoming` message from that peer ends it at once.
- Display: a row under the newest message, `role="status"` with
  `aria-live="polite"`: three dots and "<name> is typing…". The chat list's
  preview reads "typing…". **Never a notification**, sound or badge.
- A device not in use is never woken for typing; its next answer relays
  only heartbeats that are still fresh.

## Sent: our user writes

- The composer calls `api.composing(peer, true)` on each edit that leaves
  text in the box, at most every 250 ms, and `api.composing(peer, false)`
  when the box is cleared, when the message is sent, when the chat is left,
  and when the app leaves the front (`visibilitychange` to hidden, `pagehide`,
  and on desktop a window `blur`). The shell command is `hc_composing`, the
  same on desktop and Android.
- The core (`engine_typing.rs`) keeps one loop per draft. From the first
  edit it sends a heartbeat every `BEAT` (500 ms) until the last edit is
  `IDLE` (3 s) old, or the draft is stopped. These stop every draft:
  sending a message to that peer (`send_with`), the device going out of use
  (`set_active(false)`, which Android also calls natively when the app goes
  to the background, even while its JavaScript sleeps), and Android
  suspending the hub loops.
- Each heartbeat goes through the hub the chat's next message would take
  (`route(peer).next`, the pinned or the automatic hub), and only when that
  hub lists `typing`. A heartbeat that fails is simply missing.
- A heartbeat carries `{"to": [peer]}` and nothing else, and the hub refuses
  any other field. **The draft never leaves the device.**
- Recipients are whoever the chat is with: a person, a chat or an org. An
  Orgtree org's engine never asks for heartbeats, so it ignores human
  typing (user 2026-10-13 22:33Z).
- There is no setting to turn it off. None was asked for; the coordinator
  offers one to the user separately.

## Defaults chosen here

- `IDLE` 3 s: "stop sending once the user pauses", read with the earlier
  "after a few seconds idle" (user 2026-10-13 22:33Z).
- `PACE` 250 ms, the composer's 250 ms throttle, and the 1 s window fit the
  hub's limits: one heartbeat per sender and recipient per 300 ms, kept 2 s,
  relayed while under 1 s old.

## Tests

- Core unit tests (`engine_typing.rs`): a draft beats until its last edit is
  `IDLE` old; a stop ends it at its next beat; a new draft started at once
  ends the old loop; two peers are apart and the device resting ends both; a
  heartbeat's body is the addresses and nothing else.
- Round trips against a scratch hub with `typing` (`HUBCHAT_V2_HUB`):
  - `tests/typing_v2.rs`: a writer's heartbeats reach the window, never a
    second apart, nothing after the last; its message ends it; heartbeats
    never notify. It runs, and fails, on a core without heartbeats.
  - `tests/composing_v2.rs`: bob writing sends heartbeats from the first
    edit until 3 s after his last; a stop, a send and the device going out
    of use each end them.
- Browser proof on the mock (`window.__hcTyping`): the composer's calls
  (first edit, throttle, clear, send, leaving the chat), the row staying up
  without a break while heartbeats come, the answer ending it, and the row
  ending by itself about a second after heartbeats stop.
