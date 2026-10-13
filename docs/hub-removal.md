# Removing a hub and adding it again

Docket item: `hubchat-contacts-lost-after-reconnecting-to-a-hu`.
Fixed in 1.2.0-alpha.2.

## What went wrong

A user removed the hub they reached as `localhost` and added the same hub
again under their PC's name. Their chat with Orgtree's own org was still in the
list, but its header said "Status unknown — not in your hubs' directories". The
message box was replaced by "<org> isn't on any of the mail hubs you're
connected to." The hub itself still listed the org, which had kept polling it
all along.

Hubchat keeps a hub under its URL. Removing a hub deleted that URL's directory
rows (`roster`, a database cascade), but kept what its sync had saved in `meta`
under the URL, including the sync cursor (`sync.cursor.<url>`). The PC's name
had been one of this device's hub addresses once before, so its old cursor was
still there. Adding that address again resumed the old incremental sync rather
than starting a fresh one. The hub then sent only the directory changes made
since that cursor. An entry that hadn't changed (an always-online org) was never
sent again, and the conversation could not be written to.

A fresh first sync was never the problem. The hub sends its whole directory
then (cursor 0), and a lazy-history first sync ends with the hub's chat list
(GET /api/conversations). The user's hub log had no such call after the add,
and the device's store had a cursor without the lazy-history base and no start
key for that address. Both mark a resumed position. (Measured read-only;
details in the docket item.)

## What removal does now

`Store::remove_hub` (Settings › Hubs › Remove):

| Kept | Gone with the hub |
| --- | --- |
| Every message and file record. A message that only this hub held keeps its label. | The hub row and, by cascade, its directory rows, chat pins, history marks and owed deletes |
| Chats, drafts, unsent answers, history floors | Which hubs hold each message (`message_hubs`); messages are relabelled to another hub that holds them |
| Other hubs and everything they keep | Everything its sync kept under its URL (`forget_hub_sync`): cursor, lazy-history start, owed chat list, clock offset, picture acknowledgement, the picture owners its roster named, and its enabled state |

Only the last row of the right-hand column is new: the internal sync state now
goes with the rows it described. Nothing the user sees is kept or deleted
differently.

`Store::add_hub` for a URL that isn't saved forgets any leftovers under that URL
first, so a store that a removal hit before this fix starts that address clean.
Adding a hub that is already saved keeps its position. Engine removal and the
"Also remove my address from this hub" box (an unregister) are unchanged.

## The one-time repair

Stores that this already hit need no action from the user. Migration 17 marks
every saved hub (`sync.directory_refresh_due.<url>`). At that hub's next
session, a hub that syncs and resumes a saved position first fetches its whole
directory (GET /api/roster, the v1 call every hub still serves). That directory
replaces the hub's rows, and the mark is cleared. A first sync (whole directory
anyway), a v1 poll (whole roster on every answer), or a hub that refuses the
call just clears the mark. A network failure ends the session as any failure
does, and the next session tries again. Sync changes after the snapshot still
apply on top; the hub sends them as they stand now, so they agree with it.

## Limits

- Hubs are still keyed by URL. The same hub under two names is two entries, and
  "Also remove my address" on one of them takes the address off the hub that
  both reach. The other entry's next sync is refused once, then it registers
  again by itself. The user's other devices do the same. Recognizing the same
  hub under another name needs a stable hub ID, a separate item offered to the
  user and not part of this fix.
- The repair runs only when each hub is next reached. A hub that refuses
  GET /api/roster keeps only what its sync sends.
- A cursor kept by a removed hub stays in stores until that URL is added again
  or the identity is wiped. It is no longer read by anything.
