# Desktop Refresh and keyboard shortcuts

The user requested the Orgtree Refresh control in Hubchat, then F1 Settings,
F2 Directory, F3 New chat, F4 Hubs and Alt+Left/Right to cycle All/Agents/People
(2026-10-15 11:30–11:32Z). This is in the Hubchat1.2.0 source release cut.

## Behavior

Orgtree's actual header calls `window.location.reload()` in
`apps/desktop/renderer/src/shell/header.tsx`; its F5 dispatches that button.
Hubchat mirrors this full renderer reload through `refreshView`, shared by
the title-bar Refresh button and F5. Core sessions keep running; no native
restart, hub reset or duplicate connection request is added. Normal boot
re-reads state, chats, directory and transfers through `startStore`.

Before reloading, active providers synchronously snapshot into one
per-window `sessionStorage` value: desktop chat/filter/search, composer text
and attachment descriptors, and timeline loaded count/scrollTop/pinned state.
Keys include the current identity and chat. Existing last-chat storage and
core-backed drafts remain. The snapshot covers the latest text inside the
existing400ms draft-write debounce; restored text is written to the normal
draft API too. Snapshot reading is nondestructive during render; removal
happens after commit so StrictMode's double initializer sees the same value.

If saving the snapshot fails, the view stays and a normal error toast is
shown. Refresh waits for message preparations to finish (including another
chat's in-memory preparation) instead of destroying that ongoing task. Draft
answer selections retain their existing immediate core persistence; no
message is sent by a shortcut or refresh.

F1–F4 use the same destination callbacks as existing buttons. Existing Ctrl+N,
Ctrl+comma and other shortcuts stay on their existing bubbling listener.
The assigned keys alone use a capture listener: plain F1–F5 or unmodified
Alt+Left/Right. Defaults and held repeats are consumed, but repeats do not
act. A modal/join/picture viewer blocks these actions while still suppressing
the WebView default. The composer remains eligible; plain/Ctrl arrows and
modified function keys remain unchanged. In avatar-rail mode, cycling
returns to the full list to show the selected filter. Wrapping is explicit
All→Agents→People→All, matching the user's default.

Refresh sits before `.window-controls`, outside `.tb-fill`, using `icon-btn
tb-refresh`, tooltip `Refresh (F5)` and no Tauri drag attribute. Both
`-webkit-app-region:no-drag` and `app-region:no-drag` are on the button itself.
Minimize/maximize/close indices stay0/1/2. The native drag proof gains a
Refresh hit-test point expecting no HTCAPTION. Android is unchanged.

## Source and verification

Own branch `frozenlabel-sol/hubchat-refresh` in
`D:/orgtree-work/worktrees/frozenlabel-sol-hubchat-transfers` started onbdeac5b,
then rebased onto hubchat-opus's verified377ed6d and its final title-bar
candidate524557a. Coordinator controls integration/landing order.

Test-only84b550a (rebased46d7ee0) preceded the implementation. The mounted
proof uses real Desktop/TitleBar/Conversation/Composer under StrictMode with
the existing fake API. It replaces only the browser reload boundary with
a counter, then tests the same snapshot/callback behavior. Checks include
each key's destination/default handling, repeats/modifiers/modal guard,
composer focus, filter wrapping, no-drag declaration and draft/chat/scroll
restoration across a simulated reload/remount. JSDOM scroll offsets are
synthetic; native WebView/history/layout behavior is not proved by them.

Run under rig-sol-b's shared slot/kernel Job with existing dependency roots,
fresh authorizedC output, no copied or linked node_modules:

```text
HUBCHAT_DEPENDENCY_ROOT=<real Hubchat dependency checkout>
HUBCHAT_DOM_DEPENDENCY_ROOT=<real Orgtree dependency checkout>
HUBCHAT_RENDERER_OUT=C:/orgtree-build/rig-sol-b/.../desktop-shortcuts
HUBCHAT_JOB_CONTAINED=1
node tools/proof-desktop-shortcuts.mjs
```

For the base control set `HUBCHAT_SHORTCUT_BASE=377ed6d`; the runner overlays
the four original UI components. New helpers remain unreachable from those
components. It must execute assertions, not stop at a setup/build error.
No exact red count is claimed before measurement. Existing tool syntax and
whitespace checks pass; rig red/green/typecheck and final inherited-titlebar
approval are pending at this source hand-in. Native Refresh point, actual
installed reload and Android are unmeasured. No app build/install/push.
