# The desktop title bar

Hubchat's desktop window is frameless (user: "frameless, like Orgtree"), and
`src/components/TitleBar.tsx` draws its title bar: the logo, the name, the
version, an empty part, and the minimize, maximize and close buttons. Close
hides the window to the tray.

## Dragging (2026-10-15)

The user could not drag Hubchat 1.2.0-alpha.5's window by its title bar
(2026-10-14 22:31Z). The bar's drag went through Tauri's
`data-tauri-drag-region` path:

1. a mousedown in the page;
2. the `plugin:window|start_dragging` IPC call, an async command that answers
   ok before anything native happens;
3. tao's `drag_window()` on the event loop: `GetCursorPos()?`,
   `ReleaseCapture()?`, then a posted `WM_NCLBUTTONDOWN(HTCAPTION)`, with
   every native error dropped (`let _ = window.drag_window()` in
   tauri-runtime-wry);
4. Windows' move loop, if it starts.

The cause of the failure was not found. Ruled out:

- tauri, tao and wry have not changed since 2026-10-08.
- The markup, CSS and `drag.js` are unchanged.
- Nothing overlays the bar or blocks the press.
- Hubchat is not elevated.
- The WebView2 runtime did not change around the upgrade.
- The event loop was not stuck: the page marked a new message read 229 ms
  after it arrived, a minute before the report.

The two alpha.5 changes around it are the tray popup's hidden WebView window,
created at every start, and the page's WebView hidden and shown with the
window.

Now the bar is a native caption instead: `app-region: drag` on `.titlebar`,
`app-region: no-drag` on its `.window-controls` (`src/styles/app-desktop.css`).
wry turns on WebView2's non-client regions (`IsNonClientRegionSupportEnabled`,
WebView2 1.0.2420.47 and later), so WebView2 treats the bar like a real title
bar. Windows itself moves, snaps and maximises the window from it, with no
IPC round trip. Two side effects:

- **The system menu.** Right-clicking the bar shows Windows' system menu
  (Restore, Move, Size, Minimize, Maximize, Close), as on a real title bar.
- **No hover in the bar.** A drag region gets no mouse events in the page,
  so the version's tooltip no longer shows. The version is spelled out in
  the bar anyway.

A WebView2 without non-client regions ignores `app-region`; there the
`data-tauri-drag-region` attributes keep the old path.

**Keep it that way:** anything clickable added to the bar needs
`app-region: no-drag`, or a press on it drags the window instead.

## Testing

Agents cannot test a window drag. They run in a non-interactive window
station, and there Windows cannot move a window at all. Rig round 446
measured this for alpha.4, alpha.5 and main 94696b0, in three scenarios each
(normal start, `--hidden` then shown, closed then shown):

- `GetCursorPos` fails with error 1459 (`ERROR_REQUIRES_INTERACTIVE_WINDOWSTATION`),
  so tao's drag stops at its first line.
- A `WM_NCLBUTTONDOWN(HTCAPTION)` posted to the window starts no move loop.

`tools/e2e-drag-region.mjs` checks what can be seen without a mouse, in a
desktop test build's real WebView2:

- the computed `app-region` of the bar and of its buttons;
- Windows' hit test (`WM_NCHITTEST`, `tools/e2e-drag-region.ps1`), made the
  way Windows makes it. The window under the point must answer `HTCAPTION`
  at the bar's empty part, name, logo and version. At the buttons and in the
  window body, neither it nor any other window that takes the point may.

How WebView2 does it (measured, rig round 447): with `app-region: drag`,
WebView2 covers the drag region with child windows of its own. They are
`Chrome_WidgetWin_0` windows on the app's UI thread, above the browser's
window. On a 1028-pixel-wide page there were two:

- `[0,0,890,31]`: the bar minus the three 46-pixel buttons;
- `[0,31,1028,32]`: the bar's one-pixel bottom border.

They answer `HTCAPTION` to every `WM_NCHITTEST`, wherever the point is,
because Windows only ever asks them about points they cover. So a hit test
may only ask the windows that take the point. Without `app-region` there are
no such windows.

### Measured: rig round 447 (2026-10-15)

Tip 1dc465f; red control 8f2fbc8 (main 94696b0 plus the tests only). Both
are desktop test builds, made once and reused across the probe fix 5a5cb11,
which changed only the tests.

- **Tip: 11/11.** Computed `app-region` is `drag` on the bar and `no-drag`
  on its buttons, both for `-webkit-app-region` and `app-region`.
  - At the bar's empty part, name, logo and version, the window under the
    point is WebView2's caption window, and it answers `HTCAPTION`.
  - At the three buttons and in the body it is the page's
    `Chrome_RenderWidgetHostHWND`, answering `HTCLIENT`, and no window that
    takes the point answers `HTCAPTION`.
- **Red: 5/11,** exactly the expected 6 failures: both computed styles are
  `none`, and at the four bar points `Chrome_RenderWidgetHostHWND` answers
  `HTCLIENT`. The buttons, the body and cleanup pass.
- **Built CSS:** `app-desktop.css` is imported `?inline` (`src/main.tsx`),
  so its rules are a string in the JS bundle, not in a `.css` file. The
  tip's bundle holds both `app-region` forms for drag and for no-drag.
- **Leftovers:** none, and the installed Hubchat Test profile was untouched.

Not measured: the drag, snap, double-click maximise and system menu
themselves. They need real mouse input, which agents do not have.

The drag itself is the user's check.
