# Android secondary-screen Back defense

The user reported that Contact info traps them on Android 1.2.0, for every
contact after a full restart. Their screenshot shows an org contact and files.

## The real cause, measured on the phone (2026-10-16)

The defensive change below (Hubchat Test 1.2.1-alpha.1) did not help. The cause
was then found on the user's phone (Samsung SM-A576B) over adb and the
WebView's debugger, reproduced 7 times out of 7:

- Back from Contact info re-opens Conversation, and so does Back from Message
  info. Until the chat's messages load, `useMessages` (`src/lib/store.ts`)
  returned `msgs: same ? data.msgs : []`, a new array on every render.
- Conversation's question-card lookup depends on that list (the `asked` memo,
  then the card-answers effect). It re-ran on every render. With nothing to
  look up it resolved at once (`Promise.resolve(NONE)`) and stored a new
  `{peer, msgs}` state, which rendered again.
- Back arrives as a `click` (the arrow) or a `popstate` (the gesture's
  `history.back()`), which React treats as discrete. Each round therefore ran
  as a SyncLane render in a microtask, with its effects flushed synchronously.
  The page never returned to its event loop, so the `api.chat` reply that
  would have ended the loop could not be delivered.
- On the phone the renderer's main thread ran at 100% CPU and the page
  answered no devtools evaluate. Every later tap, and MainActivity's
  `evaluateJavascript` call for native Back, was lost until the app was
  killed. That is the trap the user saw.
- Evidence: mid-loop, React's root kept `pendingLanes` 2 (SyncLane) on the
  Conversation fiber, with the queued action `setFound({peer, msgs: []})`.
  Stack samples showed render, commit, render inside
  `processRootScheduleInMicrotask`.

Opening a chat from the list is not affected: it navigates after the async
`openDm` call, at default priority, so the loop yields and stops once the
messages arrive. The mounted proof below never reproduced the trap because
its mock core answers `api.chat` at once.

**Fix:** `useMessages` returns one shared empty list (and hub list) until the
chat loads, and the card-answers effect keeps its state when nothing changed.
On the phone, after a cold start, Contact info's arrow, system back key and
edge swipe, and Message info's arrow and back key all return to the
conversation (Hubchat Test 1.2.1-alpha.2).

`tools/proof-mobile-back-loading.mjs` is the regression proof. It holds the
conversation's message load open across Back (arrow, `__hcBack` and
`history.back`), counts commits with a Profiler, and stops a runaway loop at
400 commits so that a red run fails instead of hanging. It runs with the same
contained variables as `proof-mobile-back.mjs` below.

## The defensive change (2026-10-15)

Before the real cause was known: at published `71b5afe`, all 282 mounted
real-path checks passed, including restoring an org's last chat at startup,
opening Contact info with files, and comparing it with a tapped chat. An extra
synthetic missing-history model produced 10 failed assertions.

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
one animation frame invokes the same explicit parent navigation. A route-changing pop,
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
An unchanged-depth popstate must retain the still-current fallback rather
than cancel it merely because an event fired.
The synthetic restored Contact-info initializer and no-op history seam are
explicit: production does not persist a Contact-info route. They establish
the behavior under assumed missing history, not its origin on the phone.

No native gesture delivery, Activity lifecycle, Android compilation, install,
real WebView timing/history intervention or live hub acceptance is claimed by
the mounted proof. Viewport/resume events and geometry are synthetic. The
Kotlin dispatch order is source-reviewed; it needs the phone test build.

Author measurement receipts and final review are recorded on
`hubchat-mobile-can-t-go-back-from-the-client-inf`.
