# Orgtree mentions are chips that open in Orgtree

User rulings 2026-11-08 (docket `hubchat-render-orgtree-mentions-as-chips-linking`,
relayed by the Orgtree coordinator):
- 14:49Z m601d7df721da: the idea.
- 14:52Z m0dc5cb44bd7f: one generic icon for agents and projects.
- 14:54Z m888c908115a5: chips in people's messages too.
- 14:55Z m60c68ee07683: build it.
- 15:08Z mb3e5b66f1e00 "i suggest we mirror it": an `orgtree://` link, the
  mirror of `hubchat://`.
- 15:09Z mb6a4bdb7c944: a menu entry that opens just the org.
- 15:10Z m7c183781895c: the rules below, accepted as proposed.

Orgtree implements the other half. Its `docs/rust-engine/orgtree-links.md` is
the contract both apps follow.

## What the user sees

- Orgtree's mention tokens in any message become chips: `@item:<org>/<slug>`
  (tickets), `@doc:`, `@app:`, `@panel:`, `@mail:`, `@agent:` and `@project:`.
  A chip shows Orgtree's own mark for its kind, then the target's own id.
  Agents and projects share one generic mark (a node in a tree). Hubchat
  fetches nothing from Orgtree, so there is no title, read state or model.
  Copying a chip copies the token as written.
- A bare `@name` becomes the agent/project chip only in a conversation with
  an org, and only when it isn't a contact's name. A contact's name stays that
  contact's mention, and `@user` stays text. Between people, bare names stay
  text. `@net:` mentions are unchanged.
- **Desktop, an org of this PC:** the chip is a link. Its hover says
  "<Kind> <org>/<id> · Open in Orgtree", and a click (or Enter) opens the
  target in Orgtree. Right-click adds **Open <org> in Orgtree**, which opens
  only that org.
- **Anything else:** the chip is muted and does nothing. Its hover, and the
  disabled menu entry's, says "Opens in Orgtree on the PC that runs <org>".
  On a phone, a tap shows that sentence and there is no menu.

## When a chip opens

- **The message's org.** A message's org is the org that sent it, or the org
  I sent it to. A bare name belongs to that org.
- **Identity is the address.** The message's org must be one of the orgs this
  PC's Orgtree lists in `local-orgs.json`, matched by address, and the
  token's `<org>` must be one of that listing's slugs. So a token from
  someone else's org with the same slug as mine stays inert.
- **People's messages.** Messages between people have no org. Theirs are
  display-only. Mine count as this PC's, so their tokens can open.
- **Orgtree version.** The registered Orgtree must take links: its Uninstall
  entry's DisplayVersion (else the version in its name) is 4.5.9 or later,
  prereleases included, and `orgtree://` is registered (`HKCR\orgtree`). On
  any other Windows set-up, the click could only bring up Windows' "look for
  an app" prompt.

## How it is built

- `src/lib/orgtreeRefs.ts` is pure, with no imports:
  - the token grammar, copied from Orgtree's
    `packages/contracts/reference-tokens.ts` and pinned to Orgtree's own
    fixture (`tools/orgtree-ref-tokens.json`);
  - the rules above (`orefContext`);
  - the chip markup, hover text, click and menu decisions.
- `md(text, oref)` renders tokens through the `oref` inline extension, which
  runs before contact mentions so `@app:` is never a contact called "app".
  Bare names go through `onode`, after contact mentions.
- Unlike mention marks, chips are md() markup. The sanitizer allows one more
  attribute, `data-oref`, kept only on `a.oref` and only when the grammar
  reads it. Only an opening chip is a keyboard stop.
- `MessageView` builds the message's context from `useLocalOrgtree`: the
  listing plus `hc_orgtree_links`, re-read at most once a minute and on focus.
- A click re-decides from the token (never from the chip's class) and calls
  `hc_open_orgtree_link(org, token)`.
- The native side (`local_orgtree.rs`) builds the link from checked parts,
  only for an org listed now and an Orgtree that takes links, and hands it to
  the system opener. `token` null means the org-only link of the menu entry.

## Tests

- `node --test tools/orgtree-chips.test.ts`: the grammar against Orgtree's
  fixture, bare names, Q1/Q2, chips, clicks and the menu entry.
- `tools/verify-orgtree-chips.ps1` → `tools/proof-orgtree-chips.mjs`: the real
  Conversation and MessageView in an isolated DOM with a stubbed api.
  - Desktop D1-D16 and phone A1-A2.
  - The red phase fails exactly the 13 checks the change adds; the controls
    D5, D6, D7, D15 and A2 pass.
- `cargo test -p hubchat --lib local_orgtree`: the version gate, link shapes
  mirroring Orgtree's contract exactly, and only an org of this PC.
