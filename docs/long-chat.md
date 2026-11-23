# Long chats: what keeps a long history cheap

User 2026-11-23 (Steam Frame): "the lag with hubchat on frame: its when the
accumulated history of the current chat is very long. with short chats
theres no lag."

## What was wrong (measured on 1.3.12, `tools/proof-long-chat.mjs`)

- Every keystroke re-rendered every loaded message row. The composer writes
  the draft into the store (`noteDraft`), which re-renders the open chat.
  Each row (`MessageView`) is memoised, but its `onDelete` was new on every
  render because `useMessages().reload` was a fresh arrow. 1,000 loaded rows:
  about 160-270 ms of React work per keystroke, against 18 ms with 100.
- A new message, a receipt or a focus gain re-read rows from the core, which
  sends new objects every time, so every row it returned re-rendered too
  (1,000 rows: 0.8 s per incoming message or focus gain). A window that keeps
  gaining and losing focus, as in VR, paid that each time.
- Older pages loaded while scrolling back stayed mounted for as long as the
  chat was open: 5,000 messages are about 138,000 DOM nodes.

## The rules now

- A row re-renders only when what it shows changes. `useMessages` keeps the
  shown list and `reload`/`refresh`/`retryOlder`/`trim` stable; Conversation
  builds the rows again only when their inputs change (`msgRows`); its
  `onPick` and `collectReplies` read the draft and messages through a ref; a
  card whose state didn't change keeps its object. Any new prop on
  `MessageView` must be stable too, or every row re-renders again.
- A re-read keeps the shown object of every unchanged message
  (`chatwindow.reuseSame`), so only changed or new rows re-render.
- Back at the newest message with more than `TRIM_AT` (300) rows loaded, the
  chat keeps the newest `TRIM_KEEP` (100) and marks older ones loadable again
  (`useMessages().trim`); scrolling up loads them as before. Not while
  messages are selected, one is highlighted (Shift+Tab), a jump to a reply is
  under way or an older page is loading.

## The benchmark

`tools/proof-long-chat.mjs` bundles `tools/long-chat-entry.tsx` (the real
Conversation, a stubbed api returning new objects per read like IPC, a
synthetic chat of mixed messages) and runs `tools/long-chat-bench.cjs` in
offscreen Electron, Session 0 only, muted, no network or clipboard, inside a
kernel Job. Environment: `HUBCHAT_DEPENDENCY_ROOT`, `HUBCHAT_DOM_DEPENDENCY_ROOT`
(esbuild and electron), `HUBCHAT_RENDERER_OUT`, optional
`HUBCHAT_LONG_CHAT_SIZES` (default 100,1000,5000,20000) and
`HUBCHAT_LONG_CHAT_LABEL`, and `HUBCHAT_LONG_CHAT_BACK` (scroll back only that many rows; default the
whole history, which takes minutes past a few thousand). It prints `LONG-CHAT PASS` or `LONG-CHAT FAIL`
with the failed gates and exits 0, 2 (a gate failed) or 1 (it broke).

Gates, per history length: a keystroke re-renders no row; a new incoming
message re-renders at most 2 older rows; a focus gain's re-read re-renders
no unchanged row (and did re-read); back at the newest message at most 300
rows stay mounted and the view is still at the bottom. Timings are recorded,
not gated: Chromium on a PC is only a proxy for WebKitGTK on the Frame.
