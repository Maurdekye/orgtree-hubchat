# Hub status after start-up: the attach gap

User report 2026-10-14 22:24Z, Hubchat 1.2.0-alpha.5 with Orgtree 4.3.0-alpha.4:
"desktop hubchat appears unable to connect to the upgraded mailhub, despite
orgtree reporting it as live". Settings › Hubs showed both doors of the user's
own hub as "Connecting… · hub version unknown". The Orgtree contact, listed only
on that hub, had the status-unknown badge. All the while, mail between them went
through that same hub, both ways (the hub's request log; Orgtree docket
`desktop-hubchat-1-2-0-alpha-5-can-t-connect-to-t`).

## Cause

- The page shows what the core reports. It reads again only when an `hc` event
  says something changed (`src/lib/store.ts`). A steady state reports nothing
  again: a hub that stays connected sends no further event.
- The setup hook started the core, and with it the hub connections, but it
  attached the UI handle only at the end of setup. `ShellHost::event` drops
  every event while no UI is attached (`src-tauri/src/core.rs`).
- Since 1.2.0-alpha.5, `tray::build` prepares the tray popup's WebView
  ([tray-cursor.md](tray-cursor.md)). wry creates a WebView2 under
  `webview2_com::wait_with_pump`, a GetMessage/DispatchMessage loop over the
  whole UI thread. So the main page can load, start its store and read the
  core's state while setup is still running.
- A hub that connected after that read and before the attach never reached the
  page. Its row kept the core's initial status (Connecting…, no version), and
  every contact listed only on it showed the status-unknown badge. Timing
  decides which hubs: on the user's PC, star-hub connected before the page's
  read and both pendragon doors after it.

## The rule now

1. Desktop attaches the UI right after `core::init`, before anything else in
   setup (`src-tauri/src/lib.rs`). The polling attach thread is Android's only,
   because its service may still be creating the core.
2. Attaching emits `{"type":"attached"}`. The page then reads state, chats,
   directory and transfers again. This covers a page that read before the
   attach, on either platform.
3. `startStore` waits for its `hc` listener before its first read, so a change
   reported after that read cannot be lost.

Keep it that way: anything new in setup that creates a window or WebView, or
otherwise pumps messages, belongs after the attach.

## The status-unknown badge (user 2026-10-14 22:30Z)

"should only show that badge for a hub client if no active servers can connect
you to them, not just any single one." The slashed badge (presence
`disconnected`) shows only when none of the user's enabled, connected hubs lists
the contact. A hub that is down, still connecting, refused, disabled or not
listing them never shows it while another connected hub reaches them. Desktop
and Android share `presence()` (`src/lib/peers.ts`). The rule already held
there; the false badge came from the stale hub list above.

## Verification

- `tools/proof-attach-renderer.mjs`: the renderer in JSDOM against a scripted
  core, 28 checks.
  - The attach gap on desktop and Android: mounted Settings › Hubs rows and the
    chat-list badge, before and after `attached`.
  - Listen-before-read.
  - The badge ruling: nine `presence()` cases and four mounted badges.
  - On the base, 9d46e62, exactly 9 checks are expected to fail: the 8 "after
    attached" checks and listen-before-read.
- `tools/e2e-attach.mjs`: the desktop test build against a scratch hub behind a
  proxy that holds `/healthz` for a swept delay, so the hub's connection lands
  across the start-up window. Every start must show the hub connected: the row,
  its version, the pill and the contact's badge. On the base, how many starts
  are stale is timing; the run lists it per delay.
- Rig results: pending.
