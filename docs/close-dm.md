# Close a DM

Close DM hides a conversation from this device's chat list. Its messages,
attachments, drafts, unread state, hub routing and history paging positions stay.
Starting a DM with the same address restores its existing history. A new incoming
message brings it back to the list without opening the conversation on screen.

On desktop, use the conversation header's Close DM button or the chat row's
right-click menu (including the collapsed sidebar). On Android, use Close DM
in the conversation's More menu. The action returns to the chat list; reopening
works through New chat, Directory, address links and notification taps.

## Scope and rulings

- User 2026-10-10 22:39Z: hide only, keep the full history, incoming reopens.
- User via coordinator 2026-10-11 22:35Z: persistent on THIS DEVICE only.
- Coordinator 2026-10-11: existing conversation actions; direct messages only.
  Group chat behavior is excluded. Hubchat currently keys a DM by the peer's
  address, including org and agent-session addresses.

## Storage and incoming messages

SQLite migration 15 adds `closed_dms(peer TEXT PRIMARY KEY)` after the real
notification-reply receipt migration 14. Closing writes only a visibility row;
opening deletes it. No hub operation runs. Identity wipe clears visibility.

`Store::chats` excludes peers in that table. `Store::chat` and the engine's
existing lazy-history paging are unchanged. Closing never calls the message/chat
deletion code and never queues remote deletes or cancels a file transfer.

Both live v1 insertion and forward v2 sync clear visibility within the same
transaction that admits a genuinely new incoming message. Duplicate redelivery,
receipt updates, another device's outgoing sync and historical pages keep it
closed. New incoming already read on another device still reopens, independently
of whether it requires a notification here. Existing chat events refresh the UI.

Explicit UI opening calls `hc_open_dm` before navigation. Closing navigates only
after `hc_close_dm` succeeds; a failed close displays its error and retains the
conversation. Android clears the conversation navigation stack on close so Back
does not restore a hidden DM. Existing draft unmount handling flushes typed text.

Async open and close completions belong to the mounted layout and a navigation
intent. A newer open, close, overlay, info-panel action or Back invalidates the
older completion. Unmount invalidates it even if a new layout for another identity
is mounted immediately. Only a current completion may select a conversation,
reset Android's stack or show an error; core operations and chat-list refreshes
still complete. Desktop Contact info from a row opens its panel only with the
corresponding accepted navigation. Android's native Back hook invalidates pending
navigation even at the chat-list root, before deciding whether to background.

## Verification

Measured on 2026-10-11: the command below exited 0 with 31 store unit tests
passing, including all six Close DM tests; compilation took 1m 58s and tests
took 0.47s. The 97 other unit tests were filtered out. Integration-test binaries
compiled but ran zero tests because of the `store::` filter; they are not pass
claims. Typecheck exited 0, and the isolated browser proof passed all 22 checks
with no browser errors. Both menu screenshots were inspected.

`cargo test --locked -p hubchat-core store:: -j2` covers Close DM's retained
history/files/drafts/custody, new incoming versus duplicate/history/receipt paths,
read-on-another-device incoming, persistent restart, device isolation, identity
wipe and upgrading a version-14 scratch store. It also runs existing store tests.
The test stores are temporary and use no real hub or identity.

Typecheck: `node node_modules/typescript/bin/tsc --noEmit`.

`tools/proof-close-dm.mjs` uses the browser mock, a disposable Chrome profile and
a loopback Vite development server. Set `HUBCHAT_VITE_CACHE` to an assigned build
folder, `HUBCHAT_PROOF_OUT` to a scratch evidence folder, and TEMP/TMP to an
isolated test-state folder before running it. The proof records desktop and
Android-menu screenshots plus `result.json`. It exercises actions, full loaded
history retention, draft flushing, restart, explicit reopening, incoming
reopening, close failure and phone Back navigation.

Review follow-up on 2026-10-11: `tools/proof-dm-navigation-races.mjs` defers API
responses in the actual mounted Desktop and Android layouts. The predecessor
`39549f3` (proof-only commit `a6facba`) fails 11 navigation invariants while request
order/unresolved-request controls pass and no browser exceptions occur. The fixed
layouts pass all 14 checks: reverse A/B completion, newer and dismissed overlays,
same-peer reopening after close, obsolete failure notices, phone Back/native Back,
newer conversation/Settings after delayed close, and unmounted history reset.
Run with `HUBCHAT_PREDECESSOR=1` only for the predecessor control; the normal run
fails on any failed check. It uses the same cache/evidence/temp variables as the
other browser proof. After correction, typecheck and all 22 original feature
checks pass again. Rust/store/native sources are unchanged by this follow-up,
so the earlier measured store result remains applicable; no Rust build repeats.

Limits: browser proof measures the shared UI against its mock, not a packaged
Windows or Android app. Store tests exercise SQLite and live admission methods,
not a real hub transport. No app build, device installation, live-chat write or
publication is part of this verification.
