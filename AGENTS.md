# Hubchat development invariants

- 2026-10-13: Help uses one tab per level-two heading from bundled
  `docs/setup.md`; intro and screenshot-block stripping are preserved. All
  guide entry points share `GuideBody`. Local guide anchors select their tab;
  message markdown still accepts only http(s) links. Tabs support roving focus
  and arrow/Home/End keys, with a scrollable strip on narrow screens.
  [decided: user 2026-10-13; verified: Guide.tsx, guide-sections.ts;
  docs/help-tabs.md]

- 2026-10-13: Local Windows `frontendDist` must resolve as Tauri Directory,
  not URL: `C:/...` matches the URL-first untagged parser and omits embedded
  assets. Use `scripts/local-tauri-config.mjs` for out-of-tree builds, then
  check the pinned parser, actual assets and isolated rendered startup;
  native version/hash checks alone are insufficient. CI retains `../dist`.
  [verified: Tauri2.10.1 config parser; docs/local-alpha-packaging.md]

- 2026-10-12: Question cards and multi-reply travel in an additive, bounded
  message `meta` (hub feature `meta`, at most 4096 bytes) with a plain-text
  version in the body for older hubs and clients; never repurpose `reply_to`
  or `thread_id`. A card has 2-10 options (user 2026-10-12 21:12Z). Received
  `meta` is untrusted: `questions::incoming` checks each part on its own and
  keeps only what passes, and the interface checks only a card's shape,
  never more strictly than the core. A `fallback` range leaves the body only
  where its text is exactly what the accepted `meta` stands for (a card's
  numbered list; a reply's quote, `Answer:` and `Skipped` lines); anything
  else stays readable (review R1, 2026-10-13). A card settles once, on the
  first answer or empty-body skip from the other side in hub order
  (received_at, then id; unsent after, in writing order); later replies never
  revise it, its sender cannot withdraw it, and unanswered cards stay open
  (no automatic skip). Settlement never comes from the loaded page alone: the
  view asks `Store::card_answers` (indexed by reply_to and migration 16
  `reply_links`) for each card it holds, and a card the store hasn't answered
  for yet is unknown, never open. The composer checks each card a message
  replies to: with no text (files or not) it waits while any card it plainly
  replies to is unknown, keeping everything (review R2). Once a message is
  queued only what it carried is cleared (`sentDraftAnswers(peer, message)`:
  the messages it replied to and their picks), never the whole reply list:
  Reply and picks in the chat still work while its files are prepared, and
  what is chosen then goes with the next message (review R3, 2026-10-13).
  Unsent replies and picks persist per chat
  (migration 16 `draft_answers`, wiped with the identity); a saved-answers
  read started under another identity is dropped. A migration test that
  fakes an older store must remove every later migration's changes.
  [decided: user 2026-10-11 13:01-13:27Z, 22:42-22:44Z, 2026-10-12 21:12Z;
  verified: questions.rs, store_questions.rs, engine.rs, lib/questions.ts,
  lib/answers.ts, Conversation.tsx, Composer.tsx; contract, measured proof
  and limits: docs/question-cards.md]

- 2026-10-13: A send's preparation (`store.ts` `prepareSend`) goes on after
  every wait (draft save, send command, draft clear) and in its failure path
  only while the identity that started it is still here (a generation that
  `reloadAfterSwitch` moves on) and the chat's slot still holds its message.
  Otherwise it stops, starting no send command and publishing no queued or
  failed state: a send ending after an identity switch must never empty the
  next identity's composer or take over a send that identity started in the
  chat (review R4; it used to check the slot only once, after the send
  command). [verified: store.ts; proof and limits: docs/question-cards.md]

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

- 2026-10-13: Escape cancels a prepared reply exactly as the reply bar's x
  (disabled while sending), from the composer or anywhere in the desktop
  conversation after the highlight and a selection. Window-capture overlays,
  an open `.modal`/`.dialog` and IME composition keep the key; with no reply
  Escape keeps its old meaning. [user 2026-10-13; verified: `Composer.tsx`,
  `Conversation.tsx`; proof `tools/verify-escape-reply-renderer.ps1`]

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
- 2026-10-13: Removing a saved hub forgets everything its sync kept under its
  URL (`forget_hub_sync`: cursor, lazy-history start, owed chat list, clock
  offset, picture acknowledgement, roster picture owners, enabled state), and
  adding a URL that isn't saved forgets leftovers first. Before, removal kept
  the cursor while the directory rows went, so the same address added again
  resumed an incremental sync and every contact unchanged since stayed out of
  the directory (user 2026-10-13: Orgtree's own org lost after the hub was
  re-added under the PC's name). A new per-URL meta key belongs in
  `forget_hub_sync`; the key builders live in store.rs. Migration 17 schedules
  one GET /api/roster snapshot per saved hub (`Engine::refresh_directory_once`)
  so stores already hit heal on their own; a hub that refuses it keeps its
  sync deltas. Hubs are still keyed by URL: the same hub under two names is
  two entries (a stable hub ID is a separate, undecided item).
  [decided: coordinator 2026-10-13, pure fix + one-time repair; verified:
  store.rs, engine.rs; tests: hub_removal.rs, readdress_v2.rs; details and
  limits: docs/hub-removal.md]
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
  in `updates.ts`, and every desktop install goes through its `tracked()`:
  never while a check runs, one at a time (a second request joins it). Exit
  on close is a device file, off by default. Restart frees the single-instance
  lock and passes only `--hidden`, never the old start link.
  [decided: user 2026-10-11, mappings approved by lead-astra; verified:
  `tray.rs`, `updates.ts`; details: `docs/tray-menu.md`]

- 2026-10-11: Normal logging is `hubchat_core::diag` (events only) plus
  `src-tauri/src/trace.rs` (file, rotation, retention, verbose switch). Log
  tags (`diag::tag`) and error kinds (`diag::err_kind`), never message or file
  bodies, names, addresses, hosts, URLs, keys or error text, in verbose mode
  too; new logging goes through the `diag` helpers. The identity-key file
  (`keylog::note`) reaches the log only as fixed `Fact` phrases; a dependency's
  warning keeps its level and source and loses its text. If the diagnostics
  folder cannot be written the log goes to an owned, pruned temporary folder
  and the Developer page says so. A hub's own words (its version, its answers)
  are never logged, and a log file that cannot roll over drops lines instead of
  growing past its limit. The panic hook's pointer line (`trace::emergency`)
  shares the file's lock and size account with the writer (2026-10-13; it
  used to append through a second handle outside both): it stops at the limit,
  has a reserve ordinary lines leave for it, never lands between a rollover's
  archive and the fresh file, and never waits longer than 2 s. The size account
  counts every byte the file took, the prefix of a failed write included
  (2026-10-13; it used to add only after a whole write). Logs
  (`diagnostics/logs`) and crash reports (`diagnostics/crash-reports`) are
  separate folders with separate owners. Rotation at 8 MiB and a 64 MiB budget
  are coordinator choices; 30 days follows Orgtree. [decided: coordinator
  2026-10-11; verified: `trace.rs`, `diag.rs`; mapping from Orgtree, invariants
  and limits: `docs/logging.md`]
