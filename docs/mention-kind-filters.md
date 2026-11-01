# Mention kind filters: `@org:`, `@person:` and `@chat:`

User request 2026-11-01 20:19Z (docket
hubchat-mention-list-org-person-and-chat-prefix-f): typing `@org:`,
`@person:` or `@chat:` in Hubchat's message box filters the mention list to
addresses of that kind, the way Orgtree's mention menu filters by `@net:` /
`@org:`.

## Behaviour

- In the composer (desktop and Android share `Composer.tsx`), `@org:`,
  `@person:` and `@chat:` — in any case — narrow the `@` hint list to hub
  addresses of that kind: orgs and their agents, Hubchat people, or AI chat
  sessions. Text after the prefix narrows further within that kind, with the
  usual word-substring rule; ordering (in-chat mentions first, then recency)
  is unchanged within the kind.
- The prefixes are filter words only. Picking an entry inserts its normal
  `@net:<address>`, never the filter word, so nothing new is ever sent.
  `@net:` and plain `@` still list every kind, and an unknown prefix opens
  no list, as before.
- Kinds are read with the same `kindOf` (`src/lib/peers.ts`) that every
  other surface shows, so an address with no kind counts as a person here,
  exactly as its glyph and labels already say. The liaison spec's
  parenthetical ("no kind counts as org, as the hub does") describes the
  hub's registration store: hub roster entries already carry the stored
  kind, so this case does not arise from hub data. Deviation recorded on
  the docket item.
- Scope is Hubchat only (user ruling 2026-11-01 20:22Z): Orgtree's mention
  menu does not get these filters, and `@org:` there keeps its inter-org
  meaning. How `@org:<slug>` already written in a message renders is
  unchanged.

## Where

`activeMentionToken` (the token's `kind`) and `filterMentionContacts`
(`kindFilter`) in `src/lib/mentions.ts`; `Composer.tsx` passes the token's
kind and keys its menu session by it. Completion is unchanged:
`completeMentionText` already replaces the whole token with
`@net:<address>`.

## Proof

`tools/proof-mention-kind-filters.mjs` through
`tools/verify-mention-kind-filters.ps1` (contained Job, synthetic
directory): phase red at base `d462929` fails exactly the six kind-filter
checks with the three controls passing; phase green passes all nine. The
existing `tools/proof-mentions-renderer.mjs` still passes at the tip, and
the B-vs-X typecheck adds no errors (16 = 16 pre-existing asset imports).
No Session 0 capture is listed for the integration job: the menu's look,
ordering and keyboard behaviour are unchanged — only which rows appear —
and that is asserted row by row in the DOM proof.
