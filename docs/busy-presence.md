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

## Do not disturb

User request 2026-10-31 08:51Z (same docket): "when an org's outside operations are set to do not disturb, it should
show a red status icon in hubchat / elsewhere"; offline wins over it, "Offline for both" (09:03Z). An org whose
outside mail is set to Do not disturb in Orgtree (Org settings > Connections > External org inbox) shows **Do not
disturb**: the presence dot filled red through one token (`--dnd` is `var(--bad)` on every theme scope,
`src/styles/tokens.css`) and the words Do not disturb wherever presence shows (Android's app bar line: "do not
disturb"). The hub picker says "<name> is on Do not disturb here"; Contact info says its outside mail is on hold (kept,
but no agent there sees it until one checks its inbox); the Directory gives it its own section, **Do not disturb**,
after Busy now. It outranks busy, and offline outranks it.

- The hub (Mail hub 2.3.0, feature `dnd`): an org says it with `POST /api/presence {busy, dnd}`. Roster rows carry
  `dnd`, and a sync answer's `dnd` comes with `online` and `busy`; a DND address is in `busy` too, so a Hubchat that
  knows busy but not DND shows it busy.
- The core keeps it per hub like busy: `roster.dnd` (store migration 22), `Store::set_dnd` from a sync answer's list
  (an answer with `online` and no `dnd` clears it: a hub before DND), `Store::directory`'s merge (DND only if DND on
  every hub where the address is online, so the most available presence any hub lists wins), and `Store::dnd_hubs`
  for the hub picker (`RouteHub.dnd`).
- The web: `presence()` says `dnd` (text `Do not disturb`, short `do not disturb`) before busy.
- Proofs: the DND tests in `store_busy_tests.rs`, and `tools/proof-dnd-presence.mjs [repo]` (Electron, as the busy
  proof: the chat list, the header, the hub picker, a mention's hover, Contact info and the Directory;
  `HUBCHAT_PROOF_PHASE=red` expects exactly the DND checks to fail on the source before DND).

## Your status

User request 2026-10-31 08:52Z (same docket): "users on hubchat should be able to manually set their status (do not
disturb prevents notifications from appearing on their desktop / phone)". The person picks **Automatic** (the hubs'
rules: online, busy an hour after their last device in use), **Online** (even while away), **Busy** or **Do not
disturb**. It is theirs, not a device's: the hubs keep it until they pick again, so it is the same on all their
devices, and others see it only while one of them is connected (offline wins, 09:03Z "Offline for both").

- Where: your own avatar wears the dot others see (offline while none of your hubs is connected) and opens the picker:
  desktop, the chat list's head (and its rail), a popover under it; Android, the Chats screen's app bar (the avatar is
  new there), a bottom sheet. The rows are the hub picker's (`ChoiceRow` in `src/components/HubPicker.tsx`): each
  choice with what it does and its dot, the one in use checked; then **Profile**, which the avatar opened before.
- Do not disturb: no notification and no sound on any of your devices, for a message or a failed file (the shell's
  `notify_now`, `src-tauri/src/core.rs`; desktop and Android both notify through it). Messages still arrive, unread
  counts and badges still update, and turning it off replays nothing. Settings > Notifications says so while it is on,
  with **Turn it off**; its own settings stay as set.
- The hub (Mail hub 2.3.0, feature `status`): `POST /api/status {status}` keeps it in `identities.status`; every sync
  answer of ours carries `status`; others see only its effect (`online`, `busy`, `dnd`).
- The core (`crates/hubchat-core/src/engine_status.rs`): `status.mine` is what this device shows and acts on. A pick is
  owed to every saved hub (`status.owed.<url>`) and told by that hub's sync before its next request; a pick wakes a
  parked sync to do so at once. Each hub's last word is kept (`status.said.<url>`): a word that changed is a pick on
  another of our devices and is taken (a hub still owed the old pick is owed this one instead); a first word (a new
  device, a new hub, a reset) is taken when it is a choice, and a hub that says Automatic without having heard ours is
  told ours. While a hub is owed our pick, only its own word ends the wait, so an answer from before the pick is not
  taken as a change. A hub without the feature, or one that refuses it, is owed nothing.
- Contact info says what a person's states mean: Busy, "they set themselves busy, or Hubchat hasn't been in front on
  any of their devices for an hour"; Do not disturb, "they've turned off notifications on all their devices: your
  messages still arrive, and they'll see them when they look".
- Proofs: `crates/hubchat-core/src/engine_status_tests.rs` (`cargo test -p hubchat-core --lib status_tests`: the rules,
  telling a hub (200/403/503), and a pick waking a parked sync against a fake hub on 127.0.0.1); the shell's
  `do_not_disturb_tests`; `tools/proof-status-picker.mjs [repo]` (Electron, desktop and Android: the avatar's dot, the
  picker, picking Do not disturb, Settings > Notifications, Profile, a person on Do not disturb, offline winning;
  `HUBCHAT_PROOF_PHASE=red` expects every check to fail on the source before the picker).

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
