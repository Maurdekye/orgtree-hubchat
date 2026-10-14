# Question cards and multi-reply on the mail hub

Docket item `hubchat-question-messages-carry-preset-answers-y`. This is the
contract between the mail hub, Orgtree (which sends question cards) and
Hubchat (which shows them and sends the answers).

## What the user sees

- An agent's question arrives as a **card**: one question per message. With
  preset options it is **single-choice** (pick one) or **multiple-choice**
  (pick one or more). Without options it is **free-response**.
- Tapping an option sends nothing. It attaches the answer to the composer,
  and Send sends the picked answers as their own message, with no typed text
  or files. The typed draft and files stay for a separate later Send. Several
  cards' picks still go together, and multi-select still waits for Send.
  This replaces the earlier combined-answer-and-text flow (user correction
  and coordinator mechanics ruling 2026-10-14). Plain replies not among the
  picked cards stay with that draft; the answer send never skips them.
  The card's header says
  "Reply to answer something else" (free-response: "Reply to answer"), and
  tapping it starts a reply.
- A reply to a card with text answers it. A reply with no text **skips** it.
  A picked option answers it even without text.
- **Multi-reply:** select several messages and reply to all of them at once.
  The same text answers every card it replies to. Different answers go as
  separate replies.
- A card **settles once, for good**: the first message that answers or skips
  it decides. Later replies are ordinary chat. The sender can't withdraw a
  card. To change an answer, the agent asks a new question.
- Unsent replies and picked options stay with their chat, across restarts.

## On the hub: `meta`

Mail hubs with the feature `meta` in `/healthz` accept an optional `meta`
on `/api/send`: a JSON object of at most 4096 bytes. The hub stores it with
the message and returns it unchanged, under `meta`, in every envelope it
gives out (poll, sync, history), only when it was set. A repeated send with
the same id keeps the first `meta`. The hub doesn't read it.

Hubchat sends `meta` only to hubs that list the feature. Orgtree sends it to
every hub: a mail hub without the feature ignores the key. Everything in it is
also written as plain text in the body (below), so a hub without `meta`, an
older Hubchat (1.1.0 and earlier), Orgtree and any other client still read
the question, its options and every answer.

**What arrives is untrusted.** A receiver checks every field itself, and a
part that fails is ignored on its own (the rest still counts). The hub
passing something on is no proof of anything.

```json
{
  "question": { "options": ["Beta", "Stable"], "multiple": false },
  "replies": ["<message id>", "<message id>"],
  "answers": [{ "question": "<card id>", "picked": ["Beta"] }],
  "fallback": [0, 54]
}
```

| Key | On | Rules |
|---|---|---|
| `question` | a card (`kind` "question") | `options`: 2-10 labels (user 2026-10-12 21:12Z), each 1-100 characters with no line breaks or control characters, no leading or trailing spaces, all different (ignoring case). `multiple`: true for pick one or more; false or absent for pick one. Anything else: the card is shown as free-response. Hubchat's core checks these rules; its interface then checks only the shape, never more strictly. |
| `replies` | a multi-reply | Every message it answers, oldest first: 2-50 different ids of 1-200 characters. The envelope's `reply_to` must be the first. If not, only `reply_to` counts. |
| `answers` | a reply to cards | For each card answered by picking: `question`, an id among the replies (or `reply_to`), and `picked`, 1-10 of that card's own labels (exactly one for a single-choice card). An entry that doesn't fit the card the receiver holds is ignored. That card then counts as a plain reply. |
| `fallback` | anything with plain-text extras | `[start, end]`: byte offsets into the UTF-8 body of the part written for clients that don't read `meta`. It is a prefix (`start` 0) or a suffix (`end` = the body's length). Receivers that read `meta` leave it out of what they show and of the answer text, but only where its text is exactly what the accepted `meta` stands for (below): the card's numbered list, written from the accepted options; or the reply's lines, one quote line per message it answers (in order), each followed by `Answer: ` and the accepted picks for it, or by `Skipped` in a reply with no text. Anything else stays in the body, and a reply falls back to dropping its one quote line. A long body's preview keeps a range that reaches past it, for a card or a reply only, and checks it again on the whole body. |

## In the body: the plain-text version

**A card with options** (written by Orgtree): the question, a blank line, then
`Pick one:` or `Pick one or more:` and the options numbered from 1, one per
line. That block is the `fallback` suffix.

```
Which channel should rc4 go to?

Pick one:
1. Beta
2. Stable
```

**A reply** (written by Hubchat): one line per message it answers, oldest
first, `> ` and that message's first line (at most 120 characters). After a
card's line comes `Answer: ` and the picked labels (joined by `, `), or
`Skipped` when the reply has no text and picked nothing on that card. Then
the text. When there is more than today's single quote line, those lines are
the `fallback` prefix.

```
> Which channel should rc4 go to?
Answer: Beta
> Anything else for the rc4 release notes?
Skipped
```

A single reply with text and no picked option keeps the format Hubchat has
always sent (one quote line, no `meta`), so older Hubchat reads it as before.

## Settling: the same answer on every device

A card's state comes from the messages that reply to it, from the other side
of the chat only (the card's sender can't answer it). They are taken in hub
order: the hub's `received_at`, then the message id. Messages not yet on a
hub come after, in the order they were written (this device's outbox sends
them in that order). The first one settles the card:

- it picked options on the card: **answered** with those options;
- otherwise, it has text: **answered** by that text;
- otherwise: **skipped**.

Later messages leave the card as it is. Hubchat marks their note on that card
"Not counted: answered earlier". Two devices answering at the same moment
both send; whichever the hub received first counts, on every device. The
asker receives both messages, in that same order.

A card or its first answer may lie further back than the messages a chat has
loaded (the newest 50, then a page at a time). So the interface settles every
card it holds (saved picks and replies first, then quoted cards and the
loaded page's cards) from the loaded messages together with the store's
`card_answers`: each card in the chat and the first message from its other
side that replies to it, in hub order, found by index (reply_to, and
`reply_links` for every message a multi-reply answers; at most 1000 cards per
call). A card the store has not answered for yet is unknown: no saved pick on
it is sent and no empty reply skips it until the store answers. A saved pick
on a card that some answer settled is dropped.

The composer checks every card a message replies to, not the message as a
whole (review R2). A message with no text, with files or without, skips each
open card it plainly replies to (no option picked on it), so while any of
those cards is unknown it waits: Send is off, the composer says it is
checking whether the question is still open, and every reply and pick stays
as it is. Once the store answers, the message can go as before. A message
with text goes at once, since text answers a card whatever its state.

Once a message is queued, only what it carried is cleared: the messages it
replied to, and the picks on those cards (each one it answered is among them).
Everything else stays saved (review R3). That covers a pick it could not carry,
on a card not yet known to be open, and a saved reply to a message not on
screen yet. It also covers a reply or pick chosen in the chat while the
message's files were being prepared. Only the composer is locked then; Reply,
a card's header hint, selection and options still work, and what is chosen
goes with the next message. A message the queued one replied to, chosen again
meanwhile, is cleared with it.

A send that ends after an identity switch changes nothing (review R4).
`prepareSend` (`src/lib/store.ts`) goes on after each wait (saving the draft,
the send command, clearing the draft) and in its failure path only while the
identity that started it is still here and the chat's slot still holds its
message. The identity is a generation number that `reloadAfterSwitch` moves
on. Otherwise it starts no send command and shows nothing as queued or failed,
so it leaves alone the next identity's composer and any send that identity
has started in the chat.

## Verification (2026-10-13, after reviews R1 to R4)

Measured on Hubchat `hubchat-opus/questions` (on main `7eac2bd`, which
brought the Hubchat log), the mail hub's `hubchat-opus/hub-meta` (on
`6a883ad`, unchanged since review R1) and Orgtree's
`hubchat-opus/question-options` (on rust-engine `67fc92e`). Every cargo step
ran under the machine build lock inside Orgtree's kernel Job runner (after
the rebase: 6144 MB, 600 s per step). Each review finding also has a red
control: the same check run on the reviewed code fails, and passes on the
fix. The red controls ran before the branches were rebased (Hubchat on main
`0a1bdde`, Orgtree on `9fc30fb`); everything else below ran again on the
rebased branches, except the mail hub's own tests (its branch is unchanged).

- **Core:** `cargo test --locked -j2 -p hubchat-core --lib --test
  questions_fallback` passes 216 of 216 and 5 of 5. The question tests check
  the option rules, untrusted `meta` part by part, a plain-text part left out
  only where `meta` stands for it (also on a long body's preview and then the
  whole body), what older clients read and the 4096-byte limit; the store
  tests check unsent answers across a reopen, migration 16 from a real
  version-15 store, and `card_answers` (the first answer however far back,
  by reply_to or a multi-reply's links; the card's own side, ties, unsent
  answers and deletes). Red control: `questions_fallback.rs` on the reviewed
  `questions.rs` fails 4 of its 5 tests (a refused card's list cut, a whole
  reply emptied into a skip); its positive control passes.
- **Round trip:** `tests/questions_v2.rs` against the hub branch's own binary
  (scratch port, throwaway PostgreSQL 18): cards written as Orgtree writes
  them arrive as cards on two devices without the plain list; one empty
  reply answers two cards by picks and skips the third; the hub carries the
  plain text, `reply_to` and the `meta`; the other device gets the same
  replies, picks and receipt time; a single plain reply goes out as before.
- **Desktop shell:** `cargo check --locked -j2 -p hubchat` passes with no
  warning. Clippy reports nothing in this work (its findings are all in
  main's code, the log's `diag.rs` included).
- **Interface:** `tools/verify-questions.ps1` (Orgtree's
  `apps/desktop/renderer/tests/joblimit.ps1`, 4096 MB, 300 s) passes 83 of 83
  checks against the browser mock at desktop and phone widths, dark and
  light. Five call `cardStates` and pin the settling order in either list
  order. Six restart with answers saved on cards beyond the first page
  (`?pages=1`): the pick on a card another device answered is gone, the open
  card's pick and a free-response reply stay, one empty send answers and
  skips them with both quotes noted, and scrolled back the cards read the
  same. Nine run the real `questions.ts` (settlement from the store; unknown
  is neither open nor settled) and `answers.ts` with its API held back (an
  old identity's read dropped after a switch; same-identity reads and
  current edits kept). Red control: with the reviewed interface files the
  same proof fails the saved-pick, quote-note, settlement and both
  identity-switch checks (43 of 48); the positive controls pass. Fourteen
  (review R2) drive the real composer with the mock's store answer held back
  or failing (`window.__hcCards`): a pick plus an empty reply to an unknown
  card, an empty multi-reply to a known open card and an unknown one, and a
  file with no text each wait with every reply and pick kept, and Enter
  sends nothing; once the store answers, the same message goes with its
  answer and skip; text goes at once; a pick a message couldn't carry stays
  saved and comes back. Red control: with the R2 interface files the proof
  as it was then fails those checks (56 of 66); the text-reply control
  passes. The R2 reviewer's own module probe of `Composer.tsx`, adapted to
  its new `cardState` prop and run in a 512 MB / 45 s Job, passes 8 of 8 (the
  two mixed sends held back; unknown-only, known skip, picked-only,
  settled-card and text controls). Ten (review R3) hold a message with a
  file while it is prepared (`window.__hcSend`) and meanwhile use a card's
  header hint and picks on two cards. Once queued, the message carries only
  what it was sent with. The reply and the new pick stay in the composer and
  saved, and go with the next message; the pick changed on the card it
  answered is gone. Controls: with nothing chosen meanwhile everything
  clears, and a failed preparation (the selection's Reply used meanwhile)
  sends nothing and keeps the text, the file and the replies. Red control:
  with the R3 interface files the proof fails three of them (73 of 76). The
  reply chosen meanwhile is lost from the composer and the saved answers, and
  the next message doesn't answer its card. A module probe runs the real
  `Composer.tsx` queued step with `Conversation.tsx`'s own callbacks and
  `answers.ts` (512 MB / 45 s Job) and passes 14 of 14. On the R3 files it
  passes 10 of 14: the later reply is lost, also beside a multi-reply, and so
  is a saved reply the message didn't carry. Seven (review R4) hold a sent
  message's draft clearing (`window.__hcSend`) and meanwhile call the store's
  own `reloadAfterSwitch` in the page, as a link switch does. The old send
  ending afterwards leaves the next identity's text in the box, and leaves a
  send it has started in the chat preparing; that send then goes as usual.
  Control: with no switch, the held send is queued once its draft is
  cleared. Red control: with the R4 `store.ts` the proof fails the two
  consequence checks (81 of 83): the box is emptied, and the next identity's
  send is taken over. The R4 reviewer's module probe of the real `store.ts`,
  `answers.ts` and `Composer.tsx`, with three checks added, passes 9 of 9
  (512 MB / 45 s Job). On the R4 files it passes 5 of 9: the reviewer's three
  failures, and a reset during the first draft save still starting the send
  command. The log's own proof of Settings › Developer
  (`tools/proof-developer-tab.mjs`, 4096 MB / 300 s Job) passes 10 of 10 on
  the rebased branch.
- **Mail hub:** at review R1, with a throwaway PostgreSQL 18, `message_meta`,
  `people_and_replies`, `profile_pictures`, `hub_suite`, `sync`, `history`,
  `lazy_history` and `long_messages` passed. Clippy reports nothing in this
  work.
- **Orgtree:** on rust-engine `67fc92e` the `net::cards` unit tests pass 3
  of 3, the engine builds with no warning, and the rig proof
  `question-options.mjs` (18 checks) delivers cards through the real sender
  to a hosted hub with the `meta` feature and reads them as a person
  registered over HTTP; an agent's own question ending in a list the tool
  would refuse arrives whole, without `meta`. Red control: the reviewed
  engine gives that question `meta` (rig proof 17 of 18; the unit test
  fails with the reviewed `meta`).

## Limits

- On a hub without `meta`, the structure doesn't travel. A multi-reply
  reaches others as a reply to its first message, with the other quotes in
  the text. A picked answer or a skip reaches them as text. The sending
  device still shows it as sent.
- A message whose `meta` would exceed 4096 bytes isn't sent. The composer
  asks to split it.
- Orgtree delivers an answer to agents as the plain-text version, with the
  first card quoted. It doesn't track whether its cards are settled.
- Not run: an Android build, an emulator or a device, the installed app, a
  live hub or chat. The browser proof drives the mock API (`src/lib/mock.ts`),
  not the Tauri commands: `hc_send`, `hc_draft_answers`,
  `hc_set_draft_answers` and `hc_card_answers` compile and were compared with
  `src/api.ts` by reading. The mock answers `cardAnswers` with the same
  rule (`cardStates`); the store's own query is checked by the core tests.
  Long-press selection was driven with mouse events at phone width, not by
  touch.
- Settling is the same on every device for answers that crossed one hub. A
  chat whose answers arrive over two different hubs orders them by each hub's
  own clock; that case was not run.
- A device settles cards from what it holds. An answer another device sent
  that has not reached this one yet doesn't count here until it arrives; if
  this device answers meanwhile, hub order decides which one counts.
- A pick whose labels aren't the card's options (another client, a bug) is
  ignored when settling: the card counts as a plain reply, and the reply's
  `Answer:` line, matching its `meta`, is not shown.
- While a message's files are prepared, the composer shows the current
  choice. A reply or pick made in the chat then appears above the locked box,
  though the message being prepared doesn't carry it; it goes with the next
  message. The preparation itself was held in the browser mock, not timed
  against the core's file copy.
- A saved reply to a message this device can no longer load (deleted here)
  stays saved but unseen until the next Reply replaces the reply list, or
  cancelling a reply that shows clears it too. A send doesn't clear it, since
  the message didn't carry it.
- The identity switch in the proofs is the renderer's reset
  (`reloadAfterSwitch`) called in the browser page; the mock's identity stays,
  and no real link switch ran. A command already on its way to the core when
  a switch comes (the draft save or clear, the send command) can't be called
  back: which identity's store it reaches is decided by the core and was not
  measured.
