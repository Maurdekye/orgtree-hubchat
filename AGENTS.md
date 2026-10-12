# Hubchat development invariants

- 2026-10-12: Locally accepted outgoing messages establish visible history in
  their queue transaction, lowering (never raising) the lazy-history boundary.
  Historical hub imports must not establish this boundary; preserve `all`,
  paging, actual delivery state and closed DM visibility. Notification reply
  admission and history readers share a short atomic monotone merge; readers
  compute candidates outside the connection lock and filter using the returned
  winning value, preserving `all` despite concurrent admissions.
  [verified: engine.rs, store.rs, store_notification_reply.rs,
  engine_first_send_tests.rs; proof and limits: docs/first-send-visibility.md]

- 2026-10-11: Close DM is persistent device-local visibility. Preserve every
  message, attachment, draft, unread state and paging cursor; never call delete
  paths. Fresh live incoming reopens atomically, including messages already read
  on another device. Duplicate/receipt updates, historical paging and synced
  outgoing messages keep it closed. Explicit UI opens unhide before navigation;
  identity wipe clears closures. Groups are outside this feature's scope.
  [decided: user via coordinator 2026-10-11; verified: `store.rs`, `close_dm.rs`,
  `engine.rs`; details and proof limits: `docs/close-dm.md`]

- Append SQLite migrations only after the real predecessor; don't invent empty
  migration entries to reserve numbers. Close DM migration 15 depends on
  notification reply receipt migration 14.

- 2026-10-11: Async DM navigation must check both the mounted layout and latest
  navigation intent before selecting a chat, resetting history or showing errors.
  Newer opens/closes, overlays/info panels, Back/native Back and unmount invalidate
  old completions. Core operations and list refreshes still finish.
  [verified: `navigationIntent.ts`, `Desktop.tsx`, `Android.tsx`, `dm.ts`;
  measured deferred-response predecessor controls: `docs/close-dm.md`]

- 2026-10-11: Inline Android notification replies use the existing durable outbox and sender. Bind admission to the captured notification identity and verify identity.address in the same transaction. Migration14 consumed worker receipts contain no body, survive message/chat deletion, and clear on identity wipe. [verified: engine_notification_reply.rs, store_notification_reply.rs; docs/notification-inline-replies.md]

- 2026-10-12: Incoming notification reply owners originate in the immutable Engine identity (both hub paths), never current store metadata during posting. Unavailable core/engine, stopped engine, unreadable key and initialization failures retry sealed handoffs. Serialize identity establishment and latch genuine engine readiness from cold initialization, explicit key recovery and adoption; missing/unreadable/error results never establish it. Busy/nested attachments retry, avoiding mode-callback deadlocks. A completion latch suppresses background reload of a departing key, never an intentional adoption. UUID/outbox admission remains atomic. [verified: engine_notification_reply.rs, notification_reply.rs, core.rs, android.rs, NotificationReplyPolicy.kt; docs/notification-inline-replies.md]
