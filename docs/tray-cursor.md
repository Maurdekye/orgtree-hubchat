# Windows tray right-click placement and startup

User 2026-10-14 17:35Z reported the right-click menu appearing above the icon
and sometimes opening slowly. Orgtree assigns Electron's context menu once;
the native menu opens on right-click. Hubchat's pinned tray-icon0.25.1 already
reads GetCursorPos for its click event. The old custom popup instead clamped
the whole transparent submenu reservation, which displaced the visible root,
and created its WebView on first open.

Prepare that same hidden WebView at startup and render its native menu
snapshot while hidden. A ready request still requires an open generation and
revision, so warming cannot show the window. Opening pays no creation or
navigation cost once preparation has completed. A failed preparation retries
at open; a click during startup may still await initial navigation.
Creating that WebView pumps the UI thread while setup still runs, so the main
page can load and read the core before setup ends: the UI is attached to the
core before the warm-up ([startup-attach.md](startup-attach.md)).

Right-menu layout positions its root at the event cursor, flips above it when
needed and clamps the root to the monitor work area. It then places the larger
submenu envelope around that root, rather than moving the root with envelope
clamping. Native window bounds convert logical offsets with that monitor's
scale and position, including negative coordinates. The restyled menu, live
state, action and generation guards remain.

Left-click retains its recent-chat geometry and Windows double-click wait;
it benefits from sharing the already-created WebView. macOS/Linux continue
using their native context menus. No Android changes.

Prepared checks: three actual-DOM cursor/root cases plus existing tray DOM,
native DPI/work-area geometry and command guards, shell compilation and tray
regressions. `tools/proof-tray-cursor.mjs` captures three synthetic placements
and takes five renderer cold-mount and five warm-update paint samples. Those
samples exclude native event delivery, WebView creation and window show.
Agent/rig processes run in Session 0 while Explorer's notification area runs
in Session 1, so the supported isolated rig cannot measure real tray placement
or native click-to-visible latency. Coordinator 2026-10-14 18:11Z accepts the
source/DOM/geometry checks and renderer proxies as agent-side verification;
native "instant at cursor" acceptance is the user's check on the next Hubchat
alpha. No native latency numbers are claimed. No logging instrumentation is
added unless continued slowness is reported and timing is separately assigned.

## Measured agent-side verification (2026-10-14)

Rig round439 tested Hubchat `112e5dd`, containing implementation `5c9c353`
as `c09fca2` and the measurement disposition `0dde565` as `d562a7a`.
The cursor implementation and proof files are identical between author and
picked implementation commits. Typecheck passed; native library tests passed
88 with 2 ignored and no failures; tray DOM tests passed 19/19. Replacing only
`traymenu-view.ts` with its predecessor produced exactly three cursor-case
failures (16 passed); the placement proof also failed at x=12 versus x=350.

The green placement proof completed with 13 samples, three captures and zero
browser errors. All three captures were inspected: the full menu stays on
screen at the middle, bottom-right and top-left cases. Cursor (350,120) gives
root (350,120); (890,640) flips to right=888, bottom=640; (0,0) clamps to
(12,8). Five cold renderer mounts measured 8.5–32.6 ms and five warm renderer
updates measured 31.7–33.6 ms through two animation frames. These synthetic
renderer timings exclude native delivery, WebView startup and window show;
they establish neither native latency nor native cold/warm improvement.

No installation or live tray operation was performed. Native acceptance remains
the user's check on the next Hubchat alpha under the disposition above.
