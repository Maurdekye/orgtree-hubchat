# Multi-replies and mixed question answers

Ordinary Reply appends a target. Selection Reply merges its targets into the
draft, deduplicating overlaps. Remove clears an individual target; Cancel All
clears the list. Replies survive reload with their chat. Every target in a
send must come from the same sender in that chat; the collector explains a
refusal, and core admission independently refuses unavailable, foreign-chat
or mixed-sender targets before queuing or staging files.

The final user ruling on 2026-10-14 makes exclusivity per question. One send
may carry picks for one card, shared text for several other cards, ordinary
reply targets and files. Reply on a picked card clears its picks; picking a
card removes it from text targets. Empty text skips open question cards in
the text targets while other cards receive their picks. The footer explains
skips before Send. Blank replies wait while a target's state is unknown.
The existing empty-send gate remains for plain messages.

The phone sequence is Reply Q1, Reply Q3, picks Red/Green/Blue on Q2, type
one answer and Send once. Q1 and Q3 receive that text; Q2 receives its picks.
The user's corrected report identified replacing the first target, rather
than established text loss. Alpha.4's per-send separation is superseded.

Wire/storage fields are unchanged: `reply_to` is the first target,
`meta.replies` contains all ordered target IDs and `meta.answers` contains
entries only for picked cards. The body is shared text. Receivers classify
valid picks first, otherwise body text (or Skipped when empty). Bounds stay
2–50 plural IDs, 200 characters per ID and 4096 bytes of metadata. Older
clients/hubs retain the first structured link and every quote/answer/skip
in fallback text. No hub pin, runtime or schema change is required.

Preparation failure preserves the complete draft. Remount and identity
guards remain. Queued completion clears only sent choices, keeping later
ones. Text plus files count toward the existing message limit together.

Verification is pending Rig's contained batch. `proof-questions.mjs` has a
focused Desktop/Android browser path (`HUBCHAT_MULTI_ONLY=1`). Its red control
uses the base Conversation only, with the final Composer/cache, isolating
replacement behavior from the superseded alpha.4 send policy. The mounted
`proof-mixed-replies.mjs` separately compares the alpha.4 Composer/store with
the final send behavior and exercises failure/remount/identity barriers.
The former `proof-answer-separation.mjs` forwards to this replacement.
`engine_multi_reply_tests.rs` exercises actual core admission, wire fallback,
empty mixed skips and sender rejection without network tasks. No runtime,
native build, real send or device pass is claimed until batch receipts exist.
