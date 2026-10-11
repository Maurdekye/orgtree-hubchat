# Hubchat development invariants

- 2026-10-12: Poll, sync snapshot/upsert/remove/online, and setup roster refresh
  admit complete roster/provenance plus picture metadata under the originating
  session_commit guard. An early cancellation check alone is insufficient.
  Keep that closure synchronous without runtime-map acquisition or nested
  generation guards. [verified: engine.rs, engine_pictures.rs;
  measured admission-boundary controls: docs/profile-pictures.md]

- 2026-10-12: Peer picture bytes/high-water marks bind to hub-reported opaque
  identity provenance, not reusable addresses. Per-hub roster ownership updates
  atomically; absent/conflicting owners use fallback. Retain same-owner removal
  history across departure and reject old-owner native/renderer completions.
  [verified: store_pictures.rs, store.rs, hub_pictures.rs, lib/pictures.ts;
  contract/tests: docs/profile-pictures.md]

- 2026-10-12: Picture HTTP and response-body reads use the bound hub session;
  picture/ack/roster admission uses its short generation commit guard. Disabled
  clients stay cancelled after re-enable. [verified: hub_pictures.rs,
  engine_pictures.rs; tests and limits: docs/profile-pictures.md]

- 2026-10-12: Picture preview/save share one native decoder slot, retained
  inside blocking decoding even if its async caller is cancelled. Image bytes
  must not appear in diagnostics. Native command compilation passes; permit
  lifetime under forced cancellation remains a source-inspected property.
  [verified from source: `src-tauri/src/commands.rs`; limits:
  `docs/profile-pictures.md`]

- 2026-10-12: Locally accepted outgoing messages establish visible history in
  their queue transaction, lowering (never raising) the lazy-history boundary.
  Historical hub imports must not establish this boundary; preserve `all`,
  paging, actual delivery state and closed DM visibility. Notification reply
  admission and history readers share a short atomic monotone merge; readers
  compute candidates outside the connection lock and filter using the returned
  winning value, preserving `all` despite concurrent admissions.
  [verified: engine.rs, store.rs, store_notification_reply.rs,
  engine_first_send_tests.rs; proof and limits: docs/first-send-visibility.md]

- 2026-10-12: Recommend each developer installs Orgtree and Hubchat and connects both to a shared standalone mailhub on a company-accessible private server. Built-in Orgtree mailhub is for individuals and small teams; no numeric cutoff, migration or runtime default change. Keep person/organization identities distinct and hubtool optional. [decided: user 2026-10-12 10:11–10:13Z; guidance: `docs/setup.md`]

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

- 2026-10-12: Saved-hub enable state is device-local and defaults enabled. Disable
  retains identity/configuration, roster/history, pins, outbox and checkpoints;
  it removes/cancels the runtime, not the saved hub. Explicit pins wait while
  Automatic may use enabled alternatives. Copied network clients retain their
  originating cancellation token; late status/clock callbacks cannot change a
  replacement runtime. Only intentional removal unregisters. No migration16.
  Link-start resolves retained saved policy before engine availability, including
  lost-key recovery. Saved enabled hubs require an active bound client; only
  genuinely unsaved setup may construct an unbound client.
  Received-link acknowledgement, discovery (including spawned requests) and
  review publication retain both the originating hub token and listener attempt.
  Each probe also honors its destination's saved policy. Bounded synchronous
  pending-state/event publication serializes with cancellation; no late review
  from a disabled or replaced generation.
  Public preflight/setup probes always resolve saved policy, even without a
  Reception. Door helpers require a Discovery context; direct and spawned
  health requests resolve destination policy inside the origin guard. There
  is no saved-hub diagnostic bypass; unbound test resolvers stay in fixtures.
  [decided: user via coordinator 2026-10-12; source: store_hub_enabled.rs,
  engine_hub_enabled.rs, hub.rs; verification gate and limits:
  docs/hub-enable-disable.md]
- 2026-10-12: Picture revisions are logical counter plus operation ID. Only explicit
  edits mint revisions; metadata and byte cache updates compare them atomically.
  Removals keep metadata after cache eviction. Native decoding accepts bounded
  static PNG/JPEG/WebP and emits stripped square PNG; no renderer remote URL.
  Existing meta storage preserves question-card migration16 reservation.
  Picture HTTP is authenticated, bounded, nonredirecting and concurrent with
  mail inside the active session. Own Profile uses the shared native decoder;
  shared avatars never fetch chat-identity pictures. See docs/profile-pictures.md
  for exact protocol, migration reservation and measured checkpoint limits.
  [verified: pictures.rs, picture_image.rs, store_pictures.rs, engine_pictures.rs,
  hub_pictures.rs, PictureEditor.tsx, ui.tsx]

- 2026-10-11: Crash reports are local only. A Rust panic writes
  `diagnostics/crash-reports/panic-*.txt` (version, time, OS, thread, location,
  message, backtrace); Hubchat never uploads one, and the tray's "Crash
  reports…" is the only way to them. `crash.rs` installs the ONLY panic hook:
  logging gets a pointer line via `note_panics_in`, never the message. The hook
  never unwraps, a report stops at 64 KB and only the newest 50 stay. The
  dialog counts the WebView2 runtime's own dumps but must not imply Hubchat
  controls whether WebView2 sends them. Non-panic native crashes, panics before
  setup and release backtrace names are not covered.
  [decided: user via lead-astra 2026-10-11; verified: `crash.rs`; details and
  limits: `docs/tray-menu.md`]

- 2026-10-11: The desktop tray shows saved values and operation states, never
  guesses. Its menu is built once and changed in place, never rebuilt. Anything
  that writes a value the tray shows must update it: `hc_set_notifications`,
  `hc_set_autostart` (Settings uses it, not the plugin directly),
  `hc_set_exit_on_close`, and the updater's `hc_tray_updates`. Tray writes reach
  the UI as `hc-settings`. Settings › About and the tray share one updater state
  in `updates.ts`, and every desktop install goes through its `tracked()`. Exit
  on close is a device file, off by default. Restart frees the single-instance
  lock and passes only `--hidden`, never the old start link.
  [decided: user 2026-10-11, mappings approved by lead-astra; verified:
  `tray.rs`, `updates.ts`; details: `docs/tray-menu.md`]
