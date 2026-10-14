# Hubchat development invariants

- 2026-10-15: Round443 tray flip verification: DOM21/21, controlled old-source
  red19/2; native89 passed/2 ignored; typecheck passes. Exact anchoring outside
  shadow margins and extreme-edge clamping have separate controls. CDP DSF
  and synthetic native origins do not establish real DPI or click latency.
  [verified: round443 receipts at fb96c3a; docs/tray-cursor.md]

- 2026-10-14: A tray context menu that cannot fit right of the cursor must
  flip left before work-area clamping, matching the vertical flip above it.
  Clamping alone fixes its right edge at the work-area edge and misses the
  cursor. Test exact root corners at several edge offsets; bounds-only tests
  missed alpha.5's regression. Scaling proofs use synthetic native geometry
  and browser DSF emulation; real monitor DPI remains user acceptance.
  [verified: traymenu-view.ts; docs/tray-cursor.md]

- 2026-10-14: Attach the UI to the core before anything in setup can pump the
  UI thread; creating a window or WebView does (the tray popup warm-up). An
  event the core sends before the attach is dropped, and a steady state never
  reports again. Alpha.5 showed connected hubs as "Connecting… · hub version
  unknown", and their contacts with the status-unknown badge. Desktop attaches
  right after `core::init`. Attaching emits `{"type":"attached"}`, and the page
  re-reads state, chats, directory and transfers. `startStore` awaits its `hc`
  listener before the first read. The status-unknown badge shows only when no
  enabled, connected hub lists the contact; `presence()` is shared by desktop
  and Android. [decided: user 2026-10-14 22:30Z (badge); verified: lib.rs,
  core.rs, store.ts, peers.ts; docs/startup-attach.md; rig round 441: renderer
  proof 28/28 (base 19/28), desktop e2e 27/27; the base's race did not
  reproduce on the rig]

- 2026-10-14: A desktop build's debug port lists two pages since alpha.5: the
  main window and the tray popup (`tray-menu.html`, prepared at start).
  `tools/cdp.mjs` `attach()` skips the popup's page; an e2e tool that lists
  targets itself must too. A page is `about:blank` until its first navigation
  commits, so the popup's page can be listed under no name of its own:
  `attach()` waits for a page whose URL has settled. The popup's page has
  `__TAURI_INTERNALS__` but may call only event listen/unlisten
  (`capabilities/tray-menu.json`): a refused `plugin:app|identifier` means
  the wrong page (rig round 441). [verified: cdp.mjs, tray_popup.rs,
  capabilities]

- 2026-10-14: Ordinary Reply appends/dedupes targets; selection Reply merges
  into the same draft. Individual remove and Cancel All stay. Sending picks
  and text are exclusive per question, not per send: Reply unpicks that card,
  picking removes its text target. One send can carry picks, text and files
  for different targets in one conversation, regardless of author. USER23:11Z
  corrected the same-sender interpretation: one contact, no cross-chat targets
  or multiple recipients. Empty text targets visibly skip known
  questions; unknown empty targets wait. This replaces alpha.4 separation.
  Generic wire references reuse first `reply_to` plus bounded `meta.replies`.
  [decided: user/coordinator 2026-10-14; verified: Conversation.tsx, answers.ts;
  docs/question-cards.md]

- 2026-10-14: Prepare the hidden Windows tray WebView at startup. Right-menu
  layout clamps its visible root to the event's actual cursor before placing
  the larger submenu envelope; offsets are relative to that monitor's work
  area and converted with its scale. Recent-chat positioning and double-click
  discrimination keep their existing path. Native click-to-visible latency
  remains a separate measurement from DOM paint samples.
  Rig round439: native library 88 passed/2 ignored, DOM19/19 and typecheck
  passed; controlled predecessor gives exactly three cursor failures. Three
  synthetic placement captures inspected; native Session1 acceptance remains
  with the user on the next alpha. [verified: round439 receipts at 112e5dd;
  docs/tray-cursor.md]
  [verified: tray_popup.rs, traymenu-view.ts; docs/tray-cursor.md]

- 2026-10-14: Hide/show the main window only via `desktop::hide()`/`show()`,
  which hide/show the webview with it. Tauri's `WebviewWindow::hide()` leaves
  WebView2's IsVisible on: the page in the tray stayed "visible" (and focused
  after a sign-in start), read its reopened chat, and read-anywhere took the
  phone's notification away. On desktop, reading also needs "in use" (touched
  within 150 s) while `IDLE_STOPS_READING` is on. [decided: user 2026-10-14
  (idle rule, via coordinator); verified from pinned source: tauri 2.12.1,
  tauri-runtime-wry 2.12.1, wry 0.57.0; verified: rig round 435 re-run,
  tools/e2e-hidden-read.mjs 21/21, without the fix the 5 hidden/read checks
  fail; docs/tray-menu.md]

- 2026-10-14: Tauri event snapshots must derive `Clone` as well as `Serialize`.
  A stored `generate_handler!` closure needs an explicit `Invoke` input type
  before wrapping it with the popup command guard. Windows double-click time
  uses windows-sys `UI::Input::KeyboardAndMouse` and its enabled feature.
  [verified from pinned source: Tauri2.12.1, tauri-macros2.7.1,
  windows-sys0.60.2; native verification pending rig round434]

- 2026-10-14: Windows tray single click toggles six recent open chats of all
  kinds, ordered by last message either direction; double click shows main.
  Delay single-click display by Windows' double-click interval to avoid flash.
  Recent metadata, actions and late pictures bind to the current identity and
  popup generation. Native monotonic typing leases expire at one second;
  the icon dot counts unread everywhere, including closed/history-only chats.
  Do not mark a chat read from the popup. [verified: store_tray.rs,
  tray_recent.rs, trayrecent-view.ts; contract/limits: docs/tray-recent.md]

- 2026-10-14: Windows tray presentation uses the separate `tray-menu.html`
  surface; native menu handles remain the sole state/action authority.
  Custom check actions establish the requested tick before the unchanged
  dispatcher, and re-resolve live membership/enabled parents. Popup requests
  bind to an open generation; main commands and plugin capabilities stay out
  of that WebView. Never send an auxiliary Close through main Exit on close.
  Keep native macOS/Linux menus. Light styling follows supplied Orgtree pixels;
  dark is inferred and follows Windows app theme. [verified: tray.rs,
  tray_popup.rs, traymenu-view.ts; contract/verification limits: docs/tray-style.md]

- 2026-10-14: TransfersChip (both desktop placements) and TransfersStrip
  (Android chat list) already start collapsed with local open=false. Keep
  expansion during the mounted view; a newly mounted view starts collapsed.
  No persisted preference or transfer-state changes. Audit fixture:
  tools/transfers-default-fixture.tsx; docs/transfers-default.md.

- 2026-10-14: Keep pick+Send confirmation, multi-select and bundles across
  cards. Preparation/generation/slot guards still apply after awaits, including
  choices made while files prepare. [verified: Composer.tsx, store.ts;
  docs/question-cards.md]

- 2026-10-13: Typing is heartbeats, never start and stop (user 2026-10-13
  22:34Z): a writer sends one about every 500 ms and the window shows the
  row while the last one ARRIVED under 1 s ago by its own clock
  (`store.ts`), so nothing can leave it standing; an `incoming` message from
  that peer ends it at once. Never a notification. A hub's typing-only answer
  is taken on a fast path that stores nothing and waits `PACE` before the
  next sync. Our user's heartbeats (user 22:33Z) go per draft every `BEAT`
  until the last edit is `IDLE` old or the composer, a send, the device
  going out of use or Android suspending stops them; only `{"to": [peer]}`,
  never the draft, through the chat's next hub, and only to hubs listing
  `typing`. [decided: user 2026-10-13, docket
  show-agent-typing-in-hubchat-using-ordered-messa; verified:
  engine_typing.rs, engine.rs sync_session, hub_v2.rs, src/lib/store.ts,
  Composer.tsx; docs/typing.md]

- 2026-10-13: Account-avatar proof runs inside the caller's shared lock/Job
  with `--inside`; standalone callers supply Orgtree's rig helper through
  `HUBCHAT_RIG_LIB`. Never commit a static import into an author's worktree.
  [verified: `tools/proof-avatar-editor.mjs`; docs/account-avatar-editor.md]

- 2026-10-13: Help uses one tab per level-two heading from bundled
  `docs/setup.md`; intro and screenshot-block stripping are preserved. All
  guide entry points share `GuideBody`. Local guide anchors select their tab;
  message markdown still accepts only http(s) links. Tabs support roving focus
  and arrow/Home/End keys, with a scrollable strip on narrow screens.
  [decided: user 2026-10-13; verified: Guide.tsx, guide-sections.ts;
  docs/help-tabs.md]

- 2026-10-13: Windows NSIS uses the vendored Upgrade flow and exact-installed-
  executable close ladder. A successful upgrade alone does not prove graceful
  quit: the direct five-second quit remains a recorded red. Alpha policy asks
  quit first for2s, then exact-process WM_CLOSE6s, then reported force last;
  require the real `.test` fallback/timing receipt. The speculative thread was
  reverted after a trace reached ExitRequested but no Exit. Tray Quit shares
  app.exit(0); a tray hang is not measured. Retain data location,
  fresh/Advanced/silent behavior and signing. Never exercise the production
  install in this proof. [verified: src-tauri/src/lib.rs, windows/installer.nsi,
  tools/windows-installer-upgrade-proof.ps1; docs/windows-installer-upgrade.md;
  decided: coordinator 2026-10-14 alpha fallback]

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
  lost-key recovery: a disabled saved hub refuses with or without an engine.
  While an identity runs, saved enabled hubs require an active bound client.
  With no identity running (the lost-key screen relinking or restoring) an
  enabled saved hub gets a setup client like an unsaved one: no runtime exists
  for a disable to cancel, and the switch needs a running identity. This
  replaces "saved enabled hubs always require a bound client; only genuinely
  unsaved setup may construct an unbound client", which made the lost-key
  relink review show the user's working hubs as "invalid input: This saved hub
  is unavailable" [coordinator ruling 2026-10-14, Orgtree docket
  phone-linking-says-existing-hubs-are-invalid-whe; `link::start_client`].
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

- 2026-10-14: Start with Windows is on by default (Windows only).
  `autostart::settle` decides once per device, before the tray is built and
  before `first_start_of_version` notes the start. A Run entry stays as it is.
  No Run entry but a StartupApproved record stays off on an update (it was on
  and was turned off). Otherwise it is turned on with the plugin's own
  `enable()`: the same entry, `--hidden`. A new install (no `last-run-version`)
  is turned on whenever there is no Run entry. `start-with-windows` in the app
  data folder records the decision (`default`/`kept`) and every use of the
  switch (`on`/`off`, from `tray::set_autostart`). While it exists Hubchat
  never turns Start with Windows on by itself, so an explicit off survives
  restarts and updates. Test (`*.test`) and debug builds never touch the
  entry; macOS/Linux and Android are unchanged. Never re-add a Run entry
  removed from outside. [decided: user 2026-10-14 (on by default, existing
  installs too, explicit off stays off); rule for earlier installs proposed by
  hubchat-opus, approved by coordinator-opus 2026-10-14; verified:
  `autostart.rs`, `tray.rs`, `lib.rs`; details and limits: `docs/tray-menu.md`]

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

- 2026-10-14: Desktop and Android Profile share SquareCrop: drag/resize corner handles over the native preview, arrow movement and Shift-arrow resize; no crop sliders. Native PictureCrop coordinates and original-byte saves stay unchanged. Pointer cancellation ends the gesture. Contract and unrun batch checks: docs/crop-square.md. [decided: user 2026-10-14; verified from source: PictureEditor.tsx, SquareCrop.tsx, cropmath.ts]
