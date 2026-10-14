# Recent chats from the Windows tray

Single left click opens a six-row recent-chat popup. Double click shows the
main window. The popup has no Open Hubchat row. The user specified **all
chat kinds**, not just DMs, on 2026-10-14 11:29Z. macOS/Linux retain their
native menu and existing left-click show-window behavior; Android is unchanged.

## Data and interactions

- `Store::tray_overview` selects up to six open chats by maximum message
  `created_at` in either direction, matching the existing chat list's ordering.
  Ties use the peer address for deterministic order. Receipt/delivery updates
  cannot move a chat. Closed DMs stay closed and absent from the popup.
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
  typing updates use cached six-row metadata and do not re-query the store.
- A separate SQL existence check covers **any unread anywhere**, including
  chats outside the six and hidden/history-only chats. The native icon receives
  Orgtree's top-right red dot and light rim; clearing all unread restores the
  original icon. It does not depend on notification settings or the main UI.
- Chat selection revalidates membership/current identity, then uses the same
  pending chat-link and navigation path as opening that chat elsewhere. The
  popup does not mark read: normal focused-chat read tracking handles that.

## Shared window and appearance

Both left and right surfaces use the same lazy `tray-menu` window, screen-work
area positioning, focus/outside/Escape dismissal, generation guards and narrow
command scope. A single click waits Windows' own double-click interval before
showing the list; a double click invalidates that pending open, suppresses its
second release and shows main without leaving a popup. An immediate tray blur
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

Author renderer typecheck passes under the 4096 MB/150 s kernel Job. Prepared
but **not yet run**: 7 actual-DOM recent-list cases, 9 right-menu regressions,
3 store cases (limit/direction/unread outside six, closed chat, history-only
unread), and native double-click/blur/typing-expiry/dot cases. The shared
`tools/proof-tray-style.mjs` runner now includes both DOM suites and 15
synthetic captures, including light right-menu and dark recent-list comparisons
beside the supplied references. Rig-opus owns their shared batch execution.

No runtime/test/visual pass is claimed yet. No app build, installation, push,
live identity access or actual tray interaction occurred. Headless pixels
cannot establish Windows focus, shell icon rendering, double-click delivery,
multi-monitor or mixed-DPI behavior. The native code and pure timing/geometry
tests cover intent, but the OS behavior remains an explicit acceptance limit.
