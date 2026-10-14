# Windows tray menu appearance

The Windows right-click tray menu uses `tray-menu.html`, a standalone DOM
surface in one lazy frameless WebView. The existing native menu is retained
as the single source of labels, checks, enabled rows, hierarchy and actions.
macOS and Linux retain their native menus. Android is unchanged.

The user requested Orgtree's look on 2026-10-13. On October 14 the coordinator
supplied three user screenshots of its light right-click menu. A fourth dark
image is its **left-click org list**, not a context-menu colour reference.
The separate recent-chat feature will reuse the popup layer; this commit
retains Hubchat's existing left-click show-window behavior.

## Route and appearance

Electron's [Menu documentation](https://www.electronjs.org/docs/latest/api/menu)
identifies Windows menus as Chromium-style. Hubchat's pinned muda backend
uses Win32 `TrackPopupMenu`; its uxtheme support does not select Chromium's
renderer. A native-theme toggle therefore cannot provide the same presentation.
The custom route was reported to the coordinator before implementation.

The supplied light pixels show a white background, `#1f1f1f` text,
`#d3e3fd` separators, about 28 px rows, 12 px Segoe UI text, 12 px rounded
corners and a soft shadow. Light uses those values; the hover background is
light grey. These are screenshot pixel measurements/estimates, not a DPI
calibration of the user's desktop. Disabled labels remain grey; checks and
chevrons follow the same aligned columns. The **dark palette is inferred**;
no dark context-menu screenshot was supplied. It follows Windows'
`AppsUseLightTheme`, independently of Hubchat's own appearance preference,
as Orgtree's context menu does. Forced-colour mode uses system colours.

## Behavior and custody

- Right click re-reads autostart before opening and snapshots all native
  handles. Native writer changes publish fresh state while open. No updater,
  restart, notification or saved-setting implementation is duplicated.
- Selection must belong to the current native tree, with an enabled row and
  enabled parents. Hidden update-install and disabled actions cannot execute.
  A checkbox request explicitly sets its desired value, then invokes the
  existing dispatcher, retaining its persistence/error restoration/warnings.
- One selection consumes one open generation. Late snapshots, ready messages,
  closes and duplicate click/Enter events cannot operate a newer open.
- Escape, Tab, outside click, native focus loss and selection dismiss. Arrows,
  Home/End, Enter/Space and type-ahead navigate; right/left enter/leave submenus.
  Root and child menus share one focus scope. Tall menus scroll.
- Native monitor work areas and scale factors convert logical measurements
  to physical positions, including negative monitor coordinates. A preallocated
  transparent envelope contains the main menu, submenu and shadows; narrow
  displays overlap submenus inside that envelope. Nothing draws outside the
  selected display's work area. Transparent envelope clicks dismiss the menu.
- The lazy window is retained hidden for subsequent opens, with no timers or
  main React/store boot. It has no taskbar button and never takes main-window
  Exit on close. Its local command allowlist permits only popup operations;
  its capability permits event subscription only, not file/updater access.

## Verification

Measured author check on 2026-10-14: full renderer typecheck exits 0 under a
4096 MB/150 s kernel Job. The first scratch-runner check failed because its
qrcode type mapping resolved the runtime module; adding the explicit mapping
fixed the runner without changing product source. `node --check` on the
proof launcher and `git diff --check` pass.

Prepared, **not yet executed**: 9 focused actual-DOM tests in
`tools/tray-style.test.ts`, plus native placement/command-boundary tests;
synthetic rendered main/submenus in light and inferred dark, both directions,
short/narrow work-area control and a side-by-side light user-reference image.
Run the whole `tools/proof-tray-style.mjs tests|visual <output> <deps> [refs]`
invocation under the shared build slot and kernel Job. No installed app,
live identity or production traffic is needed. Rig-opus owns batch execution.

Limits: Rust compilation, tests, visual rendering and native OS menu interaction
are pending; typecheck alone proves none of them. Headless DOM pixels will not
establish actual Windows focus, taskbar, multi-monitor or mixed-DPI behavior.
No Hubchat app build, install, push or live interaction was performed.
