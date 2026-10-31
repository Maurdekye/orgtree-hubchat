# Busy presence

User requests 2026-10-30 (Orgtree docket `org-presence-busy-orange-when-no-org-inbox-holde`): "if an org has no
outbox audience holders, or all audience holders are frozen / halted / retired, or otherwise indisposed and unable to
start a turn, the org appears as "busy" with an orange marker, instead of online"; for people "use a similar metric
but give it a longer timeout, say an hour"; "Yes, widen to Mail hub 2.3.0" and "Amber (existing caution colour)".
Hubchat 1.2.7 shows it with Mail hub 2.3.0 and Orgtree 4.5.3, as Orgtree does.

## What you see

An address a hub lists online can be **busy** instead: connected, but no one can answer it right now.

- An **org** is busy while none of the agents who answer its outside mail (its org-inbox holders) can start a turn:
  there are none, or every one is halted, frozen or retired, or the org's Stop all is latched. Its Orgtree tells the
  hub; it is online again within a few seconds of one holder being able to run.
- A **person** is busy once Hubchat hasn't been in front on any of their devices for an hour: "in front" is Hubchat's
  existing "in use" (the window focused on desktop, the app in the foreground on Android, and touched in the last
  150 s, `src/lib/visibility.ts`). Bringing Hubchat to the front on any device makes them online at once. The hub owns
  the hour (`BUSY_AFTER` in the hub's `presence.rs`).
- An agent session (kind chat) is never busy.

Busy wears the presence dot filled amber, one token: `--busy` is `var(--warn)` on every theme scope
(`src/styles/tokens.css`), so a change of colour is one line. It shows wherever presence shows, with the word Busy:
the avatar dot in the chat list, the open chat's header (desktop: dot and word; Android: its app bar line, "busy"),
the hub picker (the hub where they are busy: "<name> is busy here"), a mention's hover ("Busy · org"), Contact info
(with what busy means, in What this is, while they are busy), New chat's card and rows, and the Directory: its own
section **Busy now**, after Online now, sorted by name; the Directory's "N online" counts only Online now. Settings >
Privacy says you show as busy after the hour.

## Where it comes from

- The hub (Mail hub 2.3.0, feature `busy`): busy is a part of online. Roster rows carry `busy`; a sync answer that
  carries `online` carries `busy` with it (the online addresses that are busy). `online` alone comes from a hub before
  busy, and no one is busy there.
- The core keeps it per hub, like online: `roster.busy` (store migration 21, `ALTER TABLE ... DEFAULT 0`), written by
  `set_roster`/`upsert_roster` from each row and by `Store::set_busy` from a sync answer's list
  (`Engine::admit_roster_batch` sets it whenever `online` comes). It survives a restart, as online does: the hub tells
  a change, not the state again.
- The directory merges hubs (`Store::directory`): online if online on any hub; busy if busy on every hub where the
  address is online, so one hub where it is online and not busy wins; offline is never busy, whatever an older row
  said. `Store::busy_hubs` gives the hub picker the hubs where it is online but busy (`RouteHub.busy`); routing is
  unchanged (busy is still online there).
- The web reads `Contact.busy` through `presence()` (`src/lib/peers.ts`): state `busy`, text `Busy`, short `busy`,
  after the disconnected check (a contact whose hubs are all down stays status unknown).

## Proofs

- Core: `crates/hubchat-core/src/store_busy_tests.rs` (`cargo test -p hubchat-core --lib store::busy_tests`): a row's
  busy is kept, online and not busy on one hub wins, a hub's list marks exactly those there, offline is never busy, a
  sync answer's busy comes with online and a hub before busy marks no one, a store from before busy gains the column
  and keeps it, and the send route says on which hub they are busy.
- Renderer: `tools/proof-busy-presence.mjs [repo]` (Electron, Session 0, muted, offscreen; synthetic contacts and
  chats, no network): desktop and Android, the chat list, the header, the hub picker, a mention's hover, Contact info
  of a busy person and a busy org, and the Directory. `HUBCHAT_PROOF_PHASE=red` expects exactly the busy checks to fail
  on the source before busy.
- The hub's own proof is its `tests/busy.rs` (`--features test-clock`: an org online, busy, online; a person busy an
  hour after their last device in use, online at once in front again; the last in-use time kept across a restart).

Not measured here: a phone (Android runs the same web code; the proof's Android view is the mobile layout in
Electron) and a real hub end to end with Orgtree (the Orgtree integration covers the hub and the engine).
