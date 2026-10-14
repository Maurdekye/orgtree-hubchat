# Picked answers and typed replies

Alpha.4 implemented per-send separation under the earlier 2026-10-14 ruling.
The final user ruling at 16:15:55Z supersedes that policy: exclusivity is per
question, and one send can combine picks for some cards with text for others.
Reply moves a card from picks to text targets; picking moves it back. Empty
text skips text-target questions, visibly noted before Send. The 23:11Z user
correction permits different authors within one conversation and keeps one
recipient. See [multi-replies.md](multi-replies.md) for the
current mechanics, compatibility and measured round 438 verification.

The old separate-draft preparation field remains readable for compatibility,
but new mixed sends carry their text/files together. Preparation failures,
remounts, identity barriers and choices made while a send is prepared retain
their existing guards. The former proof-answer-separation entry point now
runs the mixed-reply proof.

## Historical alpha.4 verification

Rig-opus approved round 120a6b on 2026-10-14. The mounted predecessor ran
36 checks with exactly 18 expected failures and every control passing; the
alpha.4 fix passed 36/36. Source typecheck passed, Escape-reply passed 8/8 and
first-send passed 6/6. The full question browser proof passed 87/87 at batch
`543fa9b`, including the retained reply and file as separate later sends,
unknown-card waiting, paging, settlement and identity controls. Test-only fixes
corrected dependency import conditions, CDP boolean serialization and old
bundled-send expectations; earlier failed receipts were preserved.

The owner inspected the final JSON/logs and Desktop/Android-layout screenshots.
Those were synthetic renderer checks under the shared lock/kernel Job, with
dependencies read in place and no links or copies. They were not device
acceptance or real chat sends. These historical receipts verify the superseded
alpha.4 policy. The later mixed multi-reply implementation has its own measured
round 438 receipts in [multi-replies.md](multi-replies.md).
