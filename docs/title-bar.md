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
IPC round trip. Two side effects (see
[Right-click and the version's tooltip](#right-click-and-the-versions-tooltip-2026-10-15)):

- **The system menu.** Right-clicking the bar shows Windows' system menu
  (Restore, Move, Size, Minimize, Maximize, Close), as on a real title bar
  and on Orgtree's. The user wants it.
- **No hover in the bar.** A drag region gets no mouse events in the page,
  so the version's tooltip stopped showing. The version label is now a
  no-drag island, so it shows again.

A WebView2 without non-client regions ignores `app-region`; there the
`data-tauri-drag-region` attributes keep the old path.

**Keep it that way:** anything clickable added to the bar needs
`app-region: no-drag`, or a press on it drags the window instead.

## Right-click and the version's tooltip (2026-10-15)

**The right-click opens the window's system menu, as Orgtree's title bar
does.** The user first ruled the menu out (11:04Z), then reversed it at
11:30Z: "orgtree title bar right click does show the default title bar
actions. make hubchat do the same." Orgtree's main window is a frameless
Electron 44 window that drags by `-webkit-app-region: drag`, and it has no
`system-context-menu` handler, so Electron leaves the menu on.

Hubchat needs no code for this. WebView2's non-client regions open the
system menu on a right-click in a drag region themselves, as Microsoft
documents for `IsNonClientRegionSupportEnabled`. Agents cannot see it happen
(a posted right-click opens nothing on the rig), so the user checks it.

Between the two rulings, a hook that dropped the right button's caption
messages was built (`caption_menu.rs`, a `WH_GETMESSAGE` hook; branch
`hubchat-opus/drag-window` at 81af48e). It was never verified or merged.
Don't bring it back unless the user asks again.

**The version's tooltip.** `.titlebar .ver` is an `app-region: no-drag`
island (`app-desktop.css`), so hover reaches the label again and its `title`
tooltip ("Hubchat <version>") shows. A press exactly on the label takes
Tauri's `data-tauri-drag-region` path, the one that failed for the user on
alpha.5. The bar around the label stays caption. The coordinator accepted
that edge case (11:29Z) over a native tooltip driven from a hook.

Along the whole bar, the top few pixels are the window's top resize border
instead, as on a standard title bar. WebView2's hit test answers `HTTOP`
there, in the base as well (measured: see below), so a press there resizes
the window (inferred).

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

- the computed `app-region` of the bar, of its buttons and of the version
  label;
- Windows' hit test (`WM_NCHITTEST`, `tools/e2e-drag-region.ps1`), made the
  way Windows makes it. The window under the point must answer `HTCAPTION`
  at the bar's empty part, name and logo, and just below the version
  (halfway to the bar's bottom edge; above the label is the resize border).
  At the version, the buttons and in the window body, neither it nor any
  other window that takes the point may;
- clicks posted the way Windows delivers real input to the window under the
  point (`e2e-drag-region.ps1 -Mode clicks`), since agents have no mouse:
  - a control first: Alt+Space (`WM_SYSCOMMAND SC_KEYMENU`) must open the
    system menu, which shows that the rig can see and recognise one;
  - a right-click at the bar's empty part, name and logo must open the
    window's system menu. The UI thread's menu flags (`GetGUIThreadInfo`)
    and its visible `#32768` windows show a menu. It is the system menu if
    `GUI_SYSTEMMENUMODE` is set, or if its items (`MN_GETHMENU`,
    `GetMenuItemID`) hold `SC_MOVE` and `SC_CLOSE`. It is closed again
    after each check;
  - a double-click on the caption must maximise the window, and a second one
    restore it.

The red control is the caption without the version label's fix (377ed6d
plus the tests): its version label is part of the caption. Everything else
should pass there too. Posted messages are not real input: the rig cannot
move the cursor, so a handler that reads the cursor's position instead of
the message's would see somewhere else. If the right-click checks fail on
both builds while the control passes, that is a limit of the rig.

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

Two more measured on 2026-10-15, at the tip with the version label's
`no-drag` island:

- WebView2 cuts the drag region into rectangles, in bands, one window each:
  `[0,0,890,10]` above the label, `[0,10,96,21]` and `[162,10,890,21]` to
  its left and right, `[0,21,890,31]` below it, and the bottom border
  `[0,31,1028,32]`. The label itself, `[96,10,162,21]`, is left to the page.
- At the window's top edge they answer `HTTOP` (12), the window's top resize
  border: at page y 5 (screen y 6) in both builds, while points 15 pixels
  down answer `HTCAPTION`. Test points stay below it.

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

### Measured: rig round 450 (2026-10-15)

Tip 377ed6d, the same fix and tests rebased onto main bdeac5b; red control
51681a4 (bdeac5b plus the tests only). Both were built fresh, with no Rust
warnings. The tip's bundle holds `app-region:drag` and `app-region:no-drag`
twice each; the red's holds none.

- **Tip: 11/11**, with the same answers as in round 447.
- **Red: 5/11**, with the same 6 expected failures.
- **Leftovers:** none, and the installed Hubchat Test profile was untouched.

### Measured: the right-click menu and the version label (2026-10-15)

Rig-sol-b, 13:48 to 13:50Z. Tip 524557a: 377ed6d plus the tests, the
version label's no-drag island and docs, built fresh with no Rust warnings.
Red control 203b8bf: 377ed6d plus the tests only, run on round 450's tip
binary, whose app source is the same (no diff outside `tools`; sha256
checked).

- **Built CSS:** the bundle holds `app-region:drag` twice and
  `app-region:no-drag` twice, not four times. The minifier merged the
  buttons' and the label's rules into one:
  `.titlebar .window-controls,.titlebar .ver{-webkit-app-region:no-drag;app-region:no-drag}`.
  The computed styles in the page are right.
- **Tip 14/18, red 12/18.** They differ by exactly the two intended checks.
  At the tip the version label is `no-drag`, and at its point the page's
  `Chrome_RenderWidgetHostHWND` answers `HTCLIENT`. At the red it is `drag`,
  and a caption window answers `HTCAPTION`. The tooltip reads "Hubchat
  1.2.0-alpha.5" in both.
- **Failing in both builds, as findings:**
  - The three right-clicks opened no menu: the UI thread's menu flags stayed
    0 for 1.5 s. The press did reach WebView2: its caption window took the
    mouse capture, and the release was posted to it. The Alt+Space control
    opened the system menu in both builds, and it was recognised (flags 12,
    `GUI_INMENUMODE | GUI_SYSTEMMENUMODE`, owned by the main window), so the
    rig can see one. A posted right-click does not reach WebView2's menu;
    that it needs real input, such as a cursor position `GetCursorPos`
    cannot read here, is inferred. The right-click menu is the user's
    check, like the drag.
  - "Just above the version" (page y 5, screen y 6) answered `HTTOP`: the
    window's top resize border. The point now sits below the label
    (c70cc6a, tests only).
- **Double-click:** on the caption it maximised the window within 51 ms
  (`[0,0,1044,769]` to `[-8,-8,1032,776]`), and a second one restored it,
  in both builds.
- **Leftovers:** none, and the installed Hubchat Test profile was untouched.

### Rerun with the point below the version: pending

Tip: 524557a plus c70cc6a and these docs. Red control 0ac604c: 203b8bf plus
c70cc6a. The app source is unchanged, so the same two binaries apply.
Expected: tip 15/18 and red 13/18. The three right-clicks fail in both, as
above; the red also fails the version label's two checks.
