# A hub connected later fills in the chats a device already has

Docket: hubchat-after-connecting-a-second-hub-a-contact-s (user report 2026-11-28; Hubchat 1.4.2).

## The case

A device used hub A only. A contact also wrote through hub B, which the user's other devices use. When the device
then connects hub B, B starts "from now" on it (lazy history: `start: "now"` on its first sync), and B's older
messages came in only once the user scrolled past the oldest message this device already held for that chat
(`useMessages().loadOlder` asks the hubs only when the local page runs short). In a chat that already had rows,
B's messages from the time range on screen never showed.

## What happens now

- After a hub's first sync from now, `take_chat_list` notes, BEFORE that hub's newest messages join, where each chat
  this device has already shown starts on screen (`fill_target`: its stored history floor; with floor `all`, its
  oldest stored message), then `fill_in` pages that hub's history for each such chat back to that point: at most
  `FILL_PAGES` (10) pages of `FILL_PAGE` (200). The rest pages in on scrolling back, as before.
- A chat this device has never shown (no stored floor: every chat of a brand-new device, or one only on the chat
  list) is not filled in: it keeps the lazy behaviour (the newest message from the hub's chat list, older on
  scroll). The existing tests `paging_two_hubs_stores_a_shared_message_once` and
  `a_hub_that_is_down_is_named_and_fills_in_when_back` pin this: a first version that also filled such chats
  failed both.
- A failure while filling in is logged (`diag::failure`) and leaves the chat to scrolling back; the sync goes on.
- When rows were added, the engine sends `Event::ChatFilled { peer }` (`type: "chat_filled"`); the open chat
  re-reads its whole loaded range (not only the newest `WINDOW` rows), since rows may join between rows on screen.
- `load_older` and `fill_in` share one page step (`history_page`).

## Tests

`crates/hubchat-core/tests/lazy_v2.rs`, two scratch hubs with separate databases (`HUBCHAT_LAZY_HUB`,
`HUBCHAT_LAZY_HUB2`):

- `a_hub_connected_later_fills_in_a_chat_the_device_already_has`: the device on hub A has the chat open (its live
  message from A on screen); the contact wrote 5 messages through B since; after the device connects B they show
  without scrolling, after A's message. Fails before this change (timed out).
- `a_hub_connected_later_shows_its_messages_once_paged_to_the_start`: paging to the start shows every message B
  holds, each once. Passed before this change too: the engine's paging was never the gap.

## Not explained yet

The user reported that scrolling to the top and reopening the chat did not show B's messages either. The core
paging test above passes, so that part is not reproduced; candidates are a hub B that refuses or keeps dropping this
device (the chat then lists it as unreachable), or a device whose identity differs from the other devices'.
