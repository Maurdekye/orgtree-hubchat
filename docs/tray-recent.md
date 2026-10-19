# Recent chats from the Windows tray

Single left click opens a scrollable recent-chat popup with at most ten rows
visible at once (user 2026-10-19). Double click shows the
main window. The popup has no Open Hubchat row. The user specified **all
chat kinds**, not just DMs, on 2026-10-14 11:29Z. macOS/Linux retain their
native menu and existing left-click show-window behavior; Android is unchanged.

## Data and interactions

- `Store::tray_overview` selects every open chat by maximum message
  `created_at` in either direction, matching the existing chat list's ordering.
  Ties use the peer address for deterministic order. Receipt/delivery updates
  cannot move a chat. Closed DMs stay closed and absent from the popup.
- The viewport is ten 28px rows plus padding, reduced to fit the current work
  area. Older chats stay reachable by mouse wheel, scrollbar and Arrow/Home/End
  keys. A new popup generation starts at the top; live data/typing refreshes
  keep the current scroll position and selected peer.
- Names follow the chat list's `org_name || username || address` rule. Pictures
  use the existing validated native picture cache/fetcher; session avatars keep
  the terminal fallback. Late bytes check popup generation, current identity,
  metadata revision and opaque picture provenance before becoming visible.
  Initials and kind colours follow Hubchat's existing avatar conventions.
- Unread counts include loaded incoming messages and retained hub history
  counts. A numbered badge occupies the activity column; zero shows none.
  A typing spinner replaces the badge while a heartbeat is fresh. The displayed
  badge caps its label at `99+`, as Orgtree's org list does; accessible labels
  retain the actual count.
- Native `Event::Typing` is the existing protocol signal: no new wire payload.
  Leases expire by `Instant` at one second, including while the main WebView is
  hidden; an incoming message removes its peer's lease immediately. No typing
  notification is generated. SQL refreshes are coalesced for data events;
  typing updates use cached chat metadata and do not re-query the store.
- A separate SQL existence check covers **any unread anywhere**, including
  hidden/history-only chats. The native icon receives
  Orgtree's top-right red dot and light rim; clearing all unread restores the
  original icon. It does not depend on notification settings or the main UI.
- Chat selection revalidates membership/current identity, then uses the same
  pending chat-link and navigation path as opening that chat elsewhere. The
  popup does not mark read: normal focused-chat read tracking handles that.

## Shared window and appearance

Both left and right surfaces use the same startup-prepared `tray-menu` window, screen-work
area positioning, focus/outside/Escape dismissal, generation guards and narrow
command scope. A single click opens on its first release, without waiting for
Windows' double-click interval. A double click closes that list, suppresses its
second release and shows main, matching Orgtree's immediate single-click path.
Fresh-data sync precedes opening; a click or identity change during sync cancels
that open. An immediate tray blur
consumes one toggle click, as Orgtree's popup does. Right click cancels a pending
left open before showing the context menu.

Measurements come from Orgtree's `apps/desktop/main/traylist.ts` and the supplied
left-click image: 200 px list width, 28 px rows, 5 px pane padding, `#181818`
background, `#dedede` text, `#333` border, 6 px row rounding, 16 px badge,
13 px Segoe UI and a 1.6-second working glyph. Hubchat uses its existing
terracotta accent, matching the supplied reference, and adds a 20 px avatar
column as the user requested. The list remains dark like Orgtree's source;
context-menu theme selection is separate. Extra transparent space contains
shadow only, and all frame bounds stay within the selected monitor work area.

Core reads and final publication bind to a short current-engine identity guard;
no network wait is inside it. Sign-out clears row/typing state, pending click
intent and dot. Unavailable reads show a status sentence instead of claiming
there are no chats; they do not replace a known same-identity unread state
with a fabricated zero.

## Verification and limits

2026-10-16 click-delay fix: `tools/proof-tray-click.mjs <owned-output>` compiles
the actual `Clicks` and click handlers from `tray_recent.rs` with Rust's test
runner. Popup, data refresh and runtime dependencies are test doubles; the
double-click interval is read from Windows. Run under the shared build lock
and a kernel Job. There are no Cargo or Node package dependencies.

Measured predecessor: 4 passed, 1 expected failure; **500.310 ms** from handler
entry to the popup request with the OS interval at **500 ms**. Fixed handler:
**0.001 ms**, 7 passed, no failures or skips. Tests cover immediate open,
toggle-close, double-click main/suppressed release, blur, cancelled intent,
identity change during refresh and a fresh click after suppression expires.
Quick checks: JavaScript syntax and `git diff --check`.

These numbers measure the handler's request to `open_recent`, not native
click-to-visible time. This change was not built as a Tauri app or exercised
against a shell tray icon; popup layout/paint, actual OS click delivery, focus,
multi-monitor and mixed-DPI behavior remain checks for the next desktop build.
No app installation, push or live identity access occurred. Orgtree comparison
is source-based (`apps/desktop/main/index.ts`): its click handler starts the
list immediately and its double-click handler closes it and opens main.
