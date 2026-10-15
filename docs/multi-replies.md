# Multi-replies and mixed question answers

Ordinary Reply appends a target. Selection Reply merges its targets into the
draft, deduplicating overlaps. Remove clears an individual target; Cancel All
clears the list. Replies survive reload with their chat. Every target in a
send must belong to that chat, regardless of sender; incoming and outgoing
messages can be combined. Core admission independently refuses unavailable or
foreign-chat targets before queuing or staging files. The open conversation's
contact remains the one recipient. This is the USER correction at 23:11Z on
2026-10-14, replacing the earlier same-sender interpretation.

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

## Sender correction verification — 2026-10-15, Rig round 445

The browser fixture combines incoming/outgoing targets on both layouts, then
sends once and checks the contact and every reference. With
`HUBCHAT_MULTI_ONLY=1`, `HUBCHAT_SENDER_BASE=9d46e62` overrides only Conversation
with the sender-restricting predecessor. Rig-sol-b measured green35/35 and
exactly4 named acceptance failures at red, with all31 controls passing. The
final candidate is `0ddfa14`; its product source matches tested `5b2b907`.
Core mixed-reply cases pass4/4, retaining missing/foreign-chat refusal
controls. Mounted preparation regression passes40/40. Hubchat typecheck passes.
Rig inspected all18 green/red browser captures: mixed-author references are
visible at green and the old sender refusal is visible at red, on both layouts.

These are synthetic core/browser/DOM proofs, not a real-device or external-hub
conversation acceptance run. Kind transport, schema and pin are unchanged.
The source author ran syntax/whitespace checks only; measured runtime results
above come from Rig445's shared-lock, kernel-Job verification.
The coordinator integrated the checked source locally at main `0ddfa14`;
inclusion in a built alpha remains a separate delivery step.

## Historical verification — 2026-10-14, Rig round 438

These receipts predate the 23:11Z correction. The old sender-refusal cases
below verify the superseded interpretation and have now become acceptance
cases with a one-contact send check.

Rig-opus approved committed batch `c5cf473` on `4f3db23`, under the shared
build lock and per-step kernel Jobs. Measured results:

- `proof-questions.mjs` with `HUBCHAT_MULTI_ONLY=1` passes 33/33. Replacing only
  Conversation with the predecessor produces exactly 22 target failures; all
  11 controls pass. This isolates target replacement from the superseded
  alpha.4 send policy. Screens show one phone send with text for Q1/Q3 and
  Red/Green/Blue for Q2; the predecessor drops Q1. Blank text skips Q1/Q3
  while Q2 receives its pick. Desktop and Android sender guards keep the
  existing target and explain refusal.
- Mounted `proof-mixed-replies.mjs` passes 40/40 on desktop and Android
  layouts; the alpha.4 Composer/store control fails exactly 23 targets with
  all controls passing. It covers files, unknown-card waiting, failure,
  remount, later selections, limits and both identity barriers. The former
  `proof-answer-separation.mjs` forwards to this proof.
- Full question checks pass 84/84, Escape 8/8, first-send 6/6, typecheck
  passes, core lib 229/229 and clippy has no new diagnostics. The three new
  `engine_multi_reply_tests.rs` cases exercise actual core admission, wire
  fallback, mixed skips and sender rejection without network tasks.

Earlier failed receipts were kept: fixture corrections moved the synthetic
Ctrl+click onto the bubble, gave picked cards two options, reset toggled picks
and measured send counters per layout. The final committed proofs were rerun.

The full three-question browser sequence is Android; desktop mixed sending is
covered by the mounted composer proof. These are synthetic renderer/browser
checks, not device acceptance. Hosted real-hub transport of `meta.replies` was
not measured in this round. No native app build, install, live-data write or
real chat send is included. No hub pin, lockfile, schema or migration changed.
