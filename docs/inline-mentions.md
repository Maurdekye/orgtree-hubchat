# Inline mentions show who they are, or that no hub reaches them

User request 2026-10-29 (relayed by the Orgtree coordinator): "mentions to
other huchat clients inline show as links. if they're accessible via an
existing hub, show them with their appropriate profile picture or letter
stand-ins (as they appear in the dm list). if they arent accessible, just show
a small grey dot and show a tooltip on hover stating they arent accessible."
The coordinator approved the choices below the same day. The symmetry audit's
item #2 joined it (user 2026-10-29: "A2 use the lettering on the colored
background as the icon instead of the hubchat icon when hubchat is connected
for both"): Orgtree's contact chip now uses this same picture, letters and
colours (agreed with its implementer: `initials()` over the roster name, the
kind tokens, circles for people, 28% squares for orgs and sessions); the
tooltip below follows Orgtree's lines; and, as on Orgtree's chip, a named
contact's mention shows the name (the coordinator confirmed it the same day).

Since 2026-10-17, `@net:<address>`, `@org:<slug>` and `@<known contact>` in
message text are links (`a.addr.mention`, `src/lib/md.ts`). This adds what each
link shows and what it does when nothing can reach the address.

## What the user sees

In every message, received or sent, on desktop and Android:

- **Someone one of my hubs lists**: before the mention, their avatar as the
  chat list draws it, at text size. That is their picture, else their letters
  in their kind's colours and shape; an agent session shows its terminal icon,
  and my own address my own avatar. When the roster gives them a name, the
  mention shows that name in place of the address, as Orgtree's chip does;
  copying it still gives the mention as written, and a contact with no name
  keeps its address. A click opens the chat with them (or
  starts it), as before. The tooltip reads like Orgtree's for the same address
  (symmetry audit #2, user 2026-10-29): presence and type ("Online · person",
  "Offline · independent chat", "Status unknown · org"; "You" for my own
  address), then the name, the hub's description of them and the address (when
  they have a name), then "Open chat".
- **Anyone else**: a small grey dot, muted text with no underline or pointer,
  and the tooltip "<name> isn't on any of the mail hubs you're connected to."
  (the composer's own sentence; the name is the address when no directory
  knows them). A click opens nothing; on Android a tap shows the same sentence
  as a message. The right-click menu offers Copy address, not Open chat.
- Before any hub's directory has arrived, mentions carry no mark and open as
  before (the composer blocks nothing then either).
- A mention inside code, an address typed without `@` and an `@word` that
  matches no known contact stay plain text, as before.

## How it knows

`mentionReach` in `src/lib/mentions.ts`: a mention is reachable when one of my
hubs that isn't disabled on this device lists the address in its directory, or
when it is my own address. That is the composer's test (Conversation shows the
same sentence in place of the box when no directory lists the address). The
directory is each hub's roster as the core stored it, kept while a hub is down,
so a hub that is briefly disconnected doesn't grey its contacts; presence
(online, offline, status unknown) stays the chat list's job.

## How it's drawn

`src/components/MentionMark.tsx`:

- `MentionMark` is the mark alone: give it an address and it reads the store
  itself, so it can be reused anywhere.
- `useMentionMarks` (used by `MessageView`) places one in each mention link.
  The message HTML is `md()`'s sanitized string, set with
  `dangerouslySetInnerHTML`, so the marks are React portals into hosts added
  after each render. `md()`, DOMPurify's allow-list and the rule that a message
  never loads a remote picture are unchanged. The pictures come from the same
  native-verified cache as the chat list (`usePicture`).
- Marks follow hub, directory and picture changes without re-rendering the
  message. The same component also sets the link's tooltip and its muted state.
- Letters are drawn by CSS from `data-initials`, so a copied selection holds
  none; pictures (`alt=""`) and the dot add no text either.
- A shown name is drawn the same way (`.mname`, `data-name`), and the mention
  as written moves into a visually hidden `.mtok` inside the link: still its
  text, so it is what a selection copies, and the link's accessible name is the
  name, then that text.

Render the hook's result inside the element whose handlers should see a click
on a mark: React passes a portal's events up its own tree, not the DOM's.

## Focused verification

`tools/proof-inline-mentions.mjs` (a kernel Job, a new output folder under
`C:/orgtree-build`, a Session 0 turn: its offscreen Electron asserts Session 0)
mounts the real App on the synthetic API with five contacts on a connected hub,
one on a disabled hub and one on none.

On desktop it checks:
- picture, letters, org shape, terminal icon and own avatar against the chat
  list's own avatars, in a received and a sent message;
- the dots, tooltips and muted text;
- code staying code;
- a selection copying no letters and no names;
- a named contact showing the name, the address kept out of sight, and an
  unnamed or unlisted one keeping its address;
- `md()` output without marks;
- the click and menu rules;
- a hub being enabled and a contact leaving the directory, both updating
  live, without re-rendering the message.

On Android it checks the marks, the tap message and an accessible tap opening
the chat. `HUBCHAT_PROOF_PHASE=red` expects exactly the checks the source
before the latest change fails: today the name display (one check each in
desktop marks, desktop live and Android marks). Before any mark (at the
first test commit) it was 10, 2, 2, 3, 1 and 0 checks over the six views.
Green captures each view.
