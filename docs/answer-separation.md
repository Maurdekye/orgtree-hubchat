# Picked answers and typed replies

User 2026-10-14 found that button answers and typed text went together in one
message. The coordinator approved keeping pick-then-Send confirmation so
multiple-choice and bundles across questions still work. Send while there are
picks now queues only those picks: empty body, no files, and replies only to
the picked cards. Typed text, files and unrelated plain replies remain for a
separate later Send. A short composer note explains this; the question no
longer promises that an answer goes with the next typed message.

The ordinary reply/skip/file-send behavior remains. While an answer queues,
the separate draft is retained in the renderer's preparation record, including
failures and remounts. The native text draft is saved unchanged rather than
being replaced by the empty answer or cleared after its admission. This field
is local preparation state, never part of `NewOutgoing` or the wire payload.
The generation and message-slot guards after awaits remain in place. Only the
reply ids/picks carried by the queued message clear; choices made meanwhile
remain. A retained over-limit text/file draft cannot block its separate answer,
but retains its ordinary limit for its later Send.

Markdown is explicitly out of scope: the user corrected the initial report,
saying lists and links worked and that the test choices were random. No
markdown, link, core, native command or protocol change is included.

`tools/proof-answer-separation.mjs` mounts the real Composer, preparation store
and answer cache with controlled native calls, on both desktop and Android.
It checks isolated payloads, retained text/files/native draft, later ordinary
sends, failed admission/retry/remount, unknown plain-card guards, held sends
with later choices, oversize retained drafts and two identity barriers. Run
the same proof with `HUBCHAT_PHONE_BASE=78474d3` for the predecessor and without
it for the fix. It requires all expected predecessor failures and no control
failure; green requires every check to pass. The old question browser proof
is updated for the changed flow; ordinary file-preparation, settlement and
identity controls remain there. Exact batch commands are supplied to rig-opus.

Verification is pending the assigned single batch. Static JavaScript syntax
and whitespace checks pass; no runtime pass is claimed. Dependencies are read
in place, with no links or copies. Build/evidence outputs require the assigned
C scope and the shared lock/kernel Job. No native Android build, installation,
real chat send or device acceptance has been performed.
