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
Actual native click-to-visible cold/warm measurement is required separately.
No runtime result or instant-open claim is made before those receipts.
