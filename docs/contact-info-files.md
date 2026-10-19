# Contact info Files actions

The Contact info Files list shares its attachment actions with the chat's
right-click menu. Desktop rows have a clickable filename area and an always
available actions button, as well as a context menu. Android uses the same
rows with a touch-sized actions button and its existing bottom-sheet style.
Keyboard activation uses native buttons. Back dismisses an Android file menu
before leaving Contact info.

Supported small images open the existing image viewer. Other retained files,
including video, use the native attachment opener and the OS default app;
Hubchat has no video viewer. Native resolution uses the message and attachment
IDs, checks retained file readability, and preserves Android content URIs.
Rows show the same transfer phase, progress, failure/retry and unavailable
state as chat cards. An expired file has no open/download action. Files do
not fetch previews or begin transfers merely because Contact info opened.

Clicking an available remote non-image queues the existing download and opens
it when completion is reported. Closing Contact info, changing contact or
switching identity cancels that deferred open, leaving the durable transfer
alone. Admission/open errors surface through the row and existing toast.
Download in the menu saves without a later open. Completed desktop files offer
Show in folder, including retained outgoing sources; Android's existing
download path saves to shared Downloads and its native opener handles them.
Retry/cancel/remove actions retain the chat card's existing meanings.

Show in chat closes Contact info and requests the exact carrying message in
the real desktop/Android conversation. The existing older-page loader finds
messages beyond the initial page, scrolls to them and flashes the message.
New requests, navigation and unmount invalidate pending jump completions.
This introduces no message, attachment, read-state or DM reopening command.
Contact info retains its existing 5000-message local-history window; it does
not add an unbounded attachment scan or fetch all hub history on opening.

## Focused verification

`tools/proof-contact-files.mjs` bundles the actual App/layouts, Contact info,
Conversation, image viewer and action menu with a synthetic API fixture. It
requires a kernel Job, an assigned C: output root and read-only existing
dependency roots. The fixture supplies 65 messages, files outside the newest
50, local/remote/failed/expired/active transfers and outgoing content URIs.
Every opener/download is intercepted; external HTTP(S) is blocked.

The fixture checks row actions, transfer state, save-only versus deferred
open, older-page highlighting and cancellation on panel close on both
layouts. The final combined capture frames the desktop conversation/panel
and Android's sheet. Renderer proofs establish routing and IPC arguments;
they do not prove the OS launched a file, a real hub transfer completed, or
Android hardware/installed WebView behavior. No app/installer build, install,
live identity, chat send or publishing belongs to this author check.

Measured author run (2026-10-19): tests-only `4cc9400` gives exactly 29 expected
failures and five controls across 34 checks (desktop15/2, Android14/3).
Source `f20c3c1` passes34/34 and TypeScript with exit0. The final offscreen
capture harness `5ad846f` passes the same34 checks in11.98s, records the visible
menu/panel geometry and produces the inspected desktop/Android composition.
There is no product-source delta between the typechecked and captured tips.
All final child exits are0 with null signals/errors under a3072MiB kernel Job.

Initial setup/control failures and preliminary32-check receipts are retained
in the author's C: evidence folder. Hidden iframe and ordinary hidden-window
captures showed previous routes despite current DOM checks; the final harness
uses top-level offscreen windows, explicit invalidation and a fresh verdict
file. One intermediate Electron run exited0 before the second layout loaded;
that has no complete verdict and is not a pass. The harness now stays alive
between windows and rejects a missing/stale verdict. Failed/stale captures
remain evidence, not accepted screenshots.

## Agent parity

Open, download, reveal, retry, cancel and remove are additional entry points
to existing attachment operations, with no new engine capability. Existing
file/shell tools cover local file access; the same existing UI actions remain
reachable through computer-use tools. Show in chat changes only the viewport
and highlight, and computer-use tools can activate it; it exposes no new data
or remote write. There is no new Orgtree agent API or setting in this feature.
