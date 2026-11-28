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

- A jump to a message (reply quote, Contact info) holds the view where it put
  it until the user moves it (wheel, touch, pointer, key, jump-to-bottom), so
  a resize during the smooth scroll can't pull it back to the bottom, and a
  chat re-read that changed nothing keeps its state (`chatwindow.sameRows`).
- An older page (scrolling back, or after a jump lands near the top) keeps
  the first message in view where it is: the anchor is that message and its
  offset from the top, held until the page is in. Keeping the distance from
  the bottom (1.3.13 and before) moved the view when rows below changed in
  the same commit: a far jump could end with its original out of view, and
  scrolling back drifted up to 469 px per page.
- At the bottom, only the view moving up unpins it (Hubchat 1.4.1; user 2026-11-27 "when i click onto certain
  chats it always starts scrolled a little bit up from the bottom"). Before, any scroll event more than 80 px from
  the bottom unpinned it, including the pin's own event when content below grew in between (a picture decoding,
  mention marks, a narrower window): the chat then stayed that far up and stopped following. Now a scroll event
  that didn't move up follows the new bottom. Proof: `tools/proof-open-bottom.mjs` (red: the forced late growth
  leaves a 240 px gap; green: 0). Natural opens of a chat ending in four pictures depend on timing: before the
  fix one desktop run's first open stopped 319 px up (about one picture), another run's 16 opens all reached the
  bottom; after it, every open did. Which content triggers it in the user's own chats is not measured.
- During the user's own gesture, any step up unpins (Hubchat 1.4.4; user 2026-11-28 on Android "too sticky to the
  bottom now, its annoying to try and scroll up from it"). 1.4.1's rule took a scroll event that moved up by 1 px
  or less for the pin's own and snapped back to the bottom; a slow touch drag moves less than that per event. And
  a short scroll up (under 80 px) stayed pinned, so growth below or a shorter timeline (the soft keyboard) pulled it
  back down. Now touch, wheel, keys and mouse on the timeline start a gesture that lasts through its momentum (1 s
  after the last touch or wheel, 300 ms after each scroll event): within it any step up more than 4 px from the
  bottom unpins and only reaching the bottom (4 px) re-pins, with no forced scroll; scroll events outside a
  gesture keep 1.4.1's rule. Proof: the four user-gesture steps of `tools/proof-open-bottom.mjs` (red at 1.4.3:
  slow drag gap 0, growth +240 px, shorter timeline +250 px; green: all 7 checks on desktop and Android). Synthetic
  events in Electron; a real phone's touch scroll is not measured.

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
rows stay mounted and the view is still at the bottom; after trimming, a jump
to an original 1,000 messages back loads, highlights and keeps it, and a jump
to an original already mounted ends with it in view; a far jump ends with the
original in view even when another older page loads after it; scrolling back,
the row in view moves at most 2 px while a page loads above it. Timings are recorded,
not gated: Chromium on a PC is only a proxy for WebKitGTK on the Frame.
