# Question cards and multi-reply on the mail hub

Docket item `hubchat-question-messages-carry-preset-answers-y`. This is the
contract between the mail hub, Orgtree (which sends question cards) and
Hubchat (which shows them and sends the answers).

## What the user sees

- An agent's question arrives as a **card**: one question per message. With
  preset options it is **single-choice** (pick one) or **multiple-choice**
  (pick one or more). Without options it is **free-response**.
- Tapping an option sends nothing. It attaches the answer to the composer,
  and the answer goes out with the next message. The card's header says
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

Clients send `meta` only to hubs that list the feature. Everything in it is
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
| `question` | a card (`kind` "question") | `options`: 2-10 labels, each 1-100 characters with no line breaks or control characters, no leading or trailing spaces, all different (ignoring case). `multiple`: true for pick one or more; false or absent for pick one. Anything else: the card is shown as free-response. |
| `replies` | a multi-reply | Every message it answers, oldest first: 2-50 different ids of 1-200 characters. The envelope's `reply_to` must be the first. If not, only `reply_to` counts. |
| `answers` | a reply to cards | For each card answered by picking: `question`, an id among the replies (or `reply_to`), and `picked`, 1-10 of that card's own labels (exactly one for a single-choice card). An entry that doesn't fit the card the receiver holds is ignored. That card then counts as a plain reply. |
| `fallback` | anything with plain-text extras | `[start, end]`: byte offsets into the UTF-8 body of the part written for clients that don't read `meta`. It is a prefix (`start` 0) or a suffix (`end` = the body's length). Receivers that read `meta` leave it out of what they show and of the answer text. |

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

## Limits

- On a hub without `meta`, the structure doesn't travel. A multi-reply
  reaches others as a reply to its first message, with the other quotes in
  the text. A picked answer or a skip reaches them as text. The sending
  device still shows it as sent.
- A message whose `meta` would exceed 4096 bytes isn't sent. The composer
  asks to split it.
- Orgtree delivers an answer to agents as the plain-text version, with the
  first card quoted. It doesn't track whether its cards are settled.
