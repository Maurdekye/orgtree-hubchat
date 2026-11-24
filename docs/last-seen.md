# Last seen per kind (Hubchat 1.4.0, Mail hub 2.6.0)

Docket hubchat-last-seen-time-for-orgs-people-and-indep (user 2026-11-24 11:44Z: "for orgs it's the last time the
agent in charge of the inbox finished a turn, or "now" if it's in the middle of one. for people it's the last time
they visited the app [...] for independent chats it the last time they sent a message to anyone").

- **What it means.** An org: the last time an agent in charge of its org mailbox finished a turn, "active now" while
  one is mid-turn (Orgtree says it). A person: the last time they had Hubchat in front (the hub's device in-use
  lease). An agent session (kind `chat`): the last time it sent a message to anyone; never "active now".
- **Where it shows.** Wherever presence words show, in every state: the chat header and the avatar tooltip
  ("Online · last seen 12 min ago", "Busy · active now", "Offline · last seen yesterday at 18:02"), Contact info, the
  Directory and New chat rows and the "is offline (...)" note. A mention's hover keeps the bare word ("Busy · org"). The words are built in one place,
  `src/lib/lastseen.ts` (`ACTIVE_NOW`, `lastSeenWords`, `withSeen`); `peers.ts presence()` uses them. A time that
  moves on is drawn again every 30 s (`Presence.ticks`, `useSeenTick`).
- **The user's two choices** (2026-11-24 ma46b9293f2d5), one constant each in `lastseen.ts`: the words "active now"
  (`ACTIVE_NOW`), and no time at all from a hub without the feature `last_active` (`OLD_HUBS_SHOW_LAST_CALL = false`):
  the old `last_seen` is the last call to the hub, which isn't when a person last visited.
- **Where the times come from** (hubchat-core). Roster rows carry `active` (a snapshot) and `active_at`; a sync
  answer's `active_now` (never ourselves; not `active`, which is our own devices in use) sets `roster.active` per
  hub (`Store::set_active`). The directory takes `active_at` from the latest any hub says and `active` from any hub
  where they are online. A hub is known to have the feature when `/healthz` lists `last_active`
  (`HubStatus.features`; `peers.ts knowsLastActive` looks at the connected hubs that list the contact).
- **Upgrade.** Store migration 25 adds the two roster columns and, like migration 17, has every saved hub fetch its
  whole directory once more (`Engine::refresh_directory_once`; `/api/roster` carries the times), so a hub that is
  already 2.6.0 sends every row with its time; the sync position stays. A hub upgraded later sends them itself (its
  schema 15 marks the rows it fills).
- **Directory order.** Offline contacts sort by `lastAround` (active now, else `active_at`, else the last call).
- Tests: `tools/last-seen.test.ts` (the words and the two choices), `tools/proof-last-seen.mjs` (mounted, desktop and
  Android: headers per kind, Contact info, Directory rows), `store_last_active_tests.rs` (merging across hubs,
  `active_now`, the directory fetched again). The hub's side: Mail hub `tests/last_active.rs`.
