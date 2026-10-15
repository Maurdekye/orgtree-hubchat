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

## Alpha.5 failed acceptance and horizontal flip (2026-10-14)

The user reported that alpha.5 still missed the cursor. Its horizontal formula
clamped the root against the work-area edge when it could not fit to the right,
so every cursor in that zone produced the same root right edge. The earlier
proof asserted exact anchoring only in the middle; its near-edge bounds check
did not catch this. Flip the root left of the cursor first, just as the existing
vertical formula flips above it, then apply work-area bounds for extreme edges.
The transparent submenu envelope continues to surround that root.

Prepared tests assert exact right/bottom corners across several cursor offsets.
The native pure-function matrix covers scales 1, 1.25 and 1.5 with primary,
negative-left and above monitor origins (27 cases, rounding tolerance 0.501
physical pixels). The browser proof adds 27 actual DOM corner cases under CDP
deviceScaleFactor 1/1.25/1.5, 54 physical-origin compositions and three extra
captures. These new suites are prepared, not yet run. A tiny source-extracted
horizontal-expression probe measured 18 old failures becoming 18 exact matches;
its maximum old error was 282 physical pixels in synthetic coordinate layouts.

The pinned Windows dependency gets the event point from physical GetCursorPos.
Snapshot conversion subtracts the physical work-area origin and divides by the
destination monitor scale; native placement reverses that conversion and uses
PhysicalPosition. No separate DPI conversion defect has been established.
Session0 has one 1024x768 display at 100%, with Explorer in Session1. Browser
DSF emulation and synthetic origins do not measure native DPI or a second
monitor. Coordinator 2026-10-14 22:38Z accepts these bounded proofs, with native
125%/150% and multi-monitor acceptance left to the user on the next alpha.

## Round443 bounded verification (2026-10-15)

Coordinator approved and locally integrated `fb96c3a` onto `c2c5f6c`.
The five author picks are patch-identical, including horizontal fix `5a9fd5c`
as `d323311` and test-only fixture correction `9b19eea` as `fb96c3a`.
The corrected tall-submenu case anchors exactly at x=1888; the separate
extreme-edge control retains right=1908 and bottom=1032 inside the shadow
margins. The existing near-right offset assertions remain unchanged.

Rig-sol-b measured DOM green21/21, with no skipped or cancelled tests.
Restoring only the old placement source produces19 passes and exactly two
failures: tall-submenu anchoring and the near-right scale/origin matrix.
The cursor proof passes27 DOM corner cases at CDP DSF1/1.25/1.5 and54
physical-origin compositions, with13 renderer samples and no browser errors.
Its old-source control fails at the first near-right corner: right888 !=700.
Native library tests pass89 with2 ignored DPAPI tests (not executed).
The new native rounding test actually executes its27 synthetic cases;
typecheck passes. Rig-sol-b viewed all six captures and reports the complete
light menu visible and readable. Raw hand-in: round443, exact `fb96c3a`.

Five renderer cold mounts measure6.1–33.5ms and five warm updates31.6–34ms
through two animation frames. These exclude native event delivery, WebView
startup and window show. Native real125%/150% monitor DPI, second-monitor
placement, GetCursorPos comparison and click-to-visible latency remain
unmeasured; acceptance stays with the user on the next alpha. Local source
integration is not an app build, installation or native acceptance result.
