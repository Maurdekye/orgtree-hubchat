# Android secondary-screen Back defense

The user reported that Contact info traps them on Android 1.2.0, for every
contact after a full restart. Their screenshot shows an org contact and files.
The real-device cause has **not** been reproduced. At published `71b5afe`, all
282 mounted real-path checks pass, including restoring an org's last chat at
startup, opening Contact info with files, and comparing it with a tapped chat.
An extra synthetic missing-history model produces 10 failed assertions.

Coordinator ruling of 2026-10-15 23:10Z authorizes this bounded **defensive** fix.
The phone on a Hubchat 1.2.1 test build remains the real acceptance check.

Secondary-screen corner arrows now move to their explicit stack parent before
trying to mirror that move in browser history. A screen with no known parent
returns to the chat list. Settings sections retain their Settings parent;
Contact info retains its conversation parent. Browser traversal cannot prevent
the visible arrow action. React state, the current stack ref and its depth
are updated together so consecutive navigation uses the current parent.

System Back asks the page first. The conversation menu and history-owning
overlays keep priority over the screen beneath them. The page uses normal
history traversal for its route; missing history or no route change within
one animation frame invokes the same explicit parent navigation. A real pop,
newer intent or unmount cancels the delayed fallback. At the chat-list root,
unhandled Back retains WebView history and Android's normal background action.
Desktop source and behavior are outside this change.

`tools/proof-mobile-back.mjs` mounts real Android components in StrictMode
against the in-memory mock core and JSDOM history. Run only under the shared
FIFO/kernel Job, with these variables:

- `HUBCHAT_JOB_CONTAINED=1`
- `HUBCHAT_RENDERER_OUT`: an assigned directory under `C:/orgtree-build/`
- `HUBCHAT_DEPENDENCY_ROOT`: existing Hubchat dependencies, read in place
- `HUBCHAT_DOM_DEPENDENCY_ROOT`: existing Orgtree dependencies, read in place
- optional `HUBCHAT_MOBILE_BACK_BASE=71b5afe`: load the baseline Android
  component with the same final tests; all other product sources are unchanged

The fixture models a fresh page by resetting only the module's startup-take
bookkeeping. It includes normal name/menu/directory entry, org/person/agent,
last-chat restoration, notification/chat links, draft/history state and
JavaScript viewport/resume/hub-state events. Additional tests cover stalled
traversal, immediate arrows, late pops, overlay priority and cancelled fallbacks.
The synthetic restored Contact-info initializer and no-op history seam are
explicit: production does not persist a Contact-info route. They establish
the behavior under assumed missing history, not its origin on the phone.

No native gesture delivery, Activity lifecycle, Android compilation, install,
real WebView timing/history intervention or live hub acceptance is claimed by
the mounted proof. Viewport/resume events and geometry are synthetic. The
Kotlin dispatch order is source-reviewed; it needs the phone test build.

Author measurement receipts and final review are recorded on
`hubchat-mobile-can-t-go-back-from-the-client-inf`.
