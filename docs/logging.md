# Hubchat logging

Desktop Hubchat keeps one local diagnostic log in the shape of Orgtree's engine
log (Orgtree `rust-engine` at `89268b7`: `engine/rs/orgtree-engine/src/trace.rs`,
DECISIONS 34-36), adapted to what Hubchat actually has: a Tauri shell
(`src-tauri`), a platform-independent core (`crates/hubchat-core`) and a web UI
(`src`). The log stays on the device and nothing here adds telemetry or any
upload. Entries are dated in UTC and tagged **[verified: file]** (the source
says so) or **[decided: who date]**; "coordinator" is lead-astra, who set the
task from the user's request of 2026-10-11 22:36Z (match Orgtree's logging
structure) and its extension of 22:40Z (a Developer tab with the verbose
switch).

The numbers below that are not Orgtree's (8 MiB per file, 64 MiB in total,
16 384 queued lines, 40 lines per event) are **coordinator implementation
choices, not user-specified figures**; 30 days is Orgtree's. Change them
freely in `trace.rs` if measurements say so.

## Where files live

Everything Hubchat records about itself on this device is under
`<app data>/diagnostics` (`%APPDATA%\dev.orgtree.hubchat` on Windows), the same
folder name Orgtree uses (`<data>\diagnostics`):

| Path | What | Owner |
| --- | --- | --- |
| `diagnostics/logs/<YYYY-MM-DD_HH-MM-SS>.log` | the normal log, one file per start | `src-tauri/src/trace.rs` |
| `diagnostics/logs/<start>.log.<32 hex>.gz` | a rolled-over part of that file | `trace.rs` |
| `diagnostics/crash-reports/` | one report per panic | `src-tauri/src/crash.rs` (tray work, rig-opus) |
| `diagnostics/log-salt` | 16 random bytes behind identifier tags | `trace.rs` |
| `<app data>/verbose-logging` | `on` or `off`: the Developer setting | `trace.rs` |
| `<app data>/identity-log.txt` | what happened to the identity key (64 KB) | `src-tauri/src/keylog.rs` |

Logs and crash reports are different things and stay apart: a log is running
commentary, bounded by age and size and safe to prune; a crash report is one
event's evidence and is kept by count. The log never writes into
`crash-reports`; the panic hook never writes into `logs` except one pointer
line through `trace::emergency`. [decided: coordinator contract between
hubchat-logs-sonnet and rig-opus, 2026-10-11]

## Mapping from Orgtree

| Orgtree (Rust 4.2.0) | Hubchat | Why it differs |
| --- | --- | --- |
| `orgtree-engine` process with `tracing` + `EngineLayer` | `hubchat_lib` (the Tauri shell) with `HubchatLayer` in `trace.rs`; `hubchat-core` only emits `tracing` events | Hubchat has no separate engine process; the core is a library inside the shell, so the subscriber belongs to the shell and the core stays embeddable (Android, tests) |
| `<data>\diagnostics\logs\<start>.log` | `<app data>/diagnostics/logs/<start>.log` | same name, same folder idea |
| Line `LEVEL [time] T<task> RQ<id> <client> EX<id> message` | identical shape | one reader for both logs |
| Client `user` / `desktop` / `agent:<id>/<name>` / `engine` | `ui` / `engine` / `system` | Hubchat has no agents; the UI's work, the background loops and startup are its three origins |
| A request span per HTTP request | one `ui` request per UI task (`on_core` work in `commands.rs`, and synchronous commands through `trace::commands`); one `engine` request per background loop (`diag::spawn_engine`) | the UI talks to the core through Tauri commands, not HTTP |
| `REQUEST` / `HEADERS` / `RESPONSE` wire lines | `REQUEST` (verbose) and `RESPONSE` lines for calls Hubchat makes to a hub: method, endpoint, hub tag, status, milliseconds | Hubchat is the HTTP client; headers carry the credential and are never logged |
| `#[logged]` on every engine method, with arguments and return values | `diag::traced_with` on selected operations (today `engine.send`) with a shape the caller chooses (tags, counts) and no values; `COMMAND <name>` lines in verbose mode | the arguments and returns of Hubchat's methods are message bodies, addresses, file names and keys, and Orgtree's masking works on JSON field names, which cannot protect free text |
| `#[nolog]` for hot helpers and secret handling | operations are opt-in, so nothing hot or secret is wrapped | opt-in is the safe default for private content |
| "No `Serialize` on secrets" | no `Serialize` on `Identity`; its `Debug` hides the secret [verified: `identity.rs`] | same rule |
| Verbose off by default in packaged builds (`ORGTREE_RELEASE_BUILD`), on in development; App settings > Developer; `ORGTREE_LOG_VERBOSE` | off in release, on in development (`cfg!(debug_assertions)`); Settings > Developer; `HUBCHAT_LOG_VERBOSE`; stored in `<app data>/verbose-logging` | same precedence: environment pin, stored setting, build default |
| `ORGTREE_LOG` env filter | `HUBCHAT_LOG` env filter, default `info` with noisy dependencies at `warn` | same |
| Daily or 256 MiB gzip rollover; 30-day retention by age | daily or **8 MiB** gzip rollover; 30-day retention **and a 64 MiB budget** for the folder | coordinator choice: Orgtree bounds by age only, and a chat client on a laptop should not be able to fill a disk |
| 256 000 queued lines, lossy | 16 384 queued lines, lossy | smaller process, same "never block the app, never grow" |
| Line cap 8 KB | 8 KB per line and 40 lines per event | same cap; Hubchat also bounds an event's line count |
| Console mirror (INFO and up, 500 chars) | development builds only (`HUBCHAT_LOG_CONSOLE=1` forces it) | a release build has no console |
| `codex-kills.log`: a small durable record flushed before a dangerous act | `identity-log.txt` (`keylog.rs`): the durable record of identity-key events; the log gets the same events only as fixed phrases (`keylog::Fact`), never the file's line | the Hubchat event whose loss costs most is losing the key; the file's line can carry the key store's own error text, which the log's rules keep out |
| `crash-reports/` (renderer crashes) + Crashpad | `crash-reports/` (panics), owned by the tray/crash work | separate owner, separate folder |
| `desktop-windows.jsonl`, `engine-liveness.jsonl`, `ui-open` timings, agent/dog log context | not imported | Orgtree-only process topology and instrumentation |
| "Open logs folder" button | Settings > Developer > Open logs folder | same |

## Line format

```text
INFO     [2026-10-12 01:36:53.394512] T42 RQ1a2b3c4d engine hub: Connecting -> Connected (hub software 2.0.1)
INFO     [2026-10-12 01:36:53.406871] T42 RQ1a2b3c4d ui EX5e6f7a8b engine.send(msg=msg:3fa1c2d4, hub=hub:90ab12cd)
WARNING  [2026-10-12 01:36:53.512044] T42 RQ1a2b3c4d ui RESPONSE hub:90ab12cd POST /api/send 403 [105.113 ms]
```

- Level is padded to 8 characters; `WARNING` and `ERROR` as Orgtree prints them.
- Time is local, microseconds. `T<id>` is the emitting Tokio task, `T-` outside one.
- `RQ<id> <client>` appears inside a request; `EX<id>` inside a traced operation.
  A call line inside another operation is prefixed `caller@EX<id>`.
- A message with several lines is split into prefixed lines (at most 40 per
  event, then `[…N more lines omitted]`); a line over 8 KB is cut with
  `…[line cut: x.y kb more]`.
- Events from dependencies and Tauri plugins (a tracing event, or a `log` record
  through the bridge) keep only their level and source:
  `WARNING  [...] T- reqwest.connect: [dependency message withheld]`. Their
  text is free text we did not write and can hold a URL, a host, a path or a
  header value; only warnings and errors are kept at all. Hubchat's own events
  are the targets `hubchat_lib`, `hubchat_core`, `call`, `wire`, `cmd` and
  `keylog`.

## What is logged

Always (verbose off):

- startup (`hubchat starting`: pid, version, build, OS, log file name without
  its folder) and shutdown; the verbose state;
- hub state changes (`Connecting -> Connected`) and the hub's software version;
- session failures with their **kind** (`unreachable:timeout`, `hub-403`,
  `not-a-hub`, …) and the retry delay;
- permanent send failures (message and peer as tags; the reason stays with the
  message in the UI), upload and download failures (kind only);
- hub calls that answer 4xx/5xx, fail, or take over 5 s (long polls excepted);
- identity-key events as fixed phrases (`identity key: the key store has no
  usable key; no backup`), from the facts in `keylog::Fact`;
- warnings from the shell (for example a window icon that cannot be set) and
  the level and source of a dependency's warning or error.

Verbose (Developer tab, or a development build) adds:

- `COMMAND <name>` for every UI command, in order, and `refused: <kind>` when a
  command returns an error;
- `REQUEST` and every successful `RESPONSE` line for hub calls;
- call and return lines with timing for traced operations (`engine.send`).

Verbose adds detail about *what happened and when*. It does not add content: no
mode logs error text, arguments or return values.

## Privacy rules

[decided: coordinator 2026-10-11, in the task description: "Preserve secrets and
private content; no routine credential/message-body logging or new
telemetry/upload"]

The design is that **content is never passed to the log**, not that it is
cleaned up afterwards.

1. **Not passed in, in any mode**: the key, the recovery words, the
   `X-Org-Auth` value, message and file bodies, previews, display names, file
   names, attachment paths, push endpoints, request queries, hub URLs, error
   text.
2. **Identifiers are pseudonymous tags**: `diag::tag(kind, value)` gives
   `peer:3fa1c2d4`, `hub:90ab12cd`, `msg:…` from SHA-256 over a per-device random
   salt, truncated to 32 bits. The same value has the same tag in every file on
   this device, so lines can be matched. A tag is a pseudonym, **not
   anonymisation**: two values can share a tag; the salt file sits next to the
   logs, so whoever holds both can test a guess; the log still shows how many
   peers and hubs there are and when each is active.
3. **Errors are names from a fixed list**: `diag::err_kind` returns `hub-403`,
   `unreachable:timeout`, `io-notfound(os 2)`, `store`, … Nothing of an error's
   own text is copied (it can carry URLs, hub replies, paths, file names and
   addresses). The network cause (`timeout`, `dns`, `refused`, `tls`, `reset`,
   `network`, `redirect`, `body`, `other`) is chosen by looking for known
   phrases, never by quoting them. A regex scrub of free text was considered and
   rejected: it cannot be shown to catch a short secret or an unusual URL.
4. **Wire paths are endpoints**: `diag::wire_path` keeps `/api/<name>` and drops
   ids and query strings (`/api/messages/…`). The hub appears as a tag.
5. **Arguments and return values are never logged.** A traced operation states
   its own shape (tags, counts, sizes). Adding an operation means choosing what
   it may say.
6. **The layer is a second net, not the first**: fields named like a secret or
   like content (`token`, `key`, `body`, `text`, `address`, `url`, `error`,
   `detail`, …) print as `*****`, and any run of 32 or more letters, digits and
   `+/_=-` that mixes letters and digits prints as `<token>`. Known holes: a
   short secret in free text, a long token of letters only, content in a field
   with an ordinary name. Call sites must not rely on it.
7. No `Serialize` on a type that holds a secret, and no `Debug` that prints one.
8. **The identity-key record is mirrored as facts.** `keylog::note(dir, line,
   &[Fact..])` writes `line` to `identity-log.txt` (the durable file, unchanged:
   it may hold the key store's or backup's own error text, never the key and
   never an address) and the facts' fixed phrases to the log. The line itself
   never reaches the log; add a variant to `Fact`, not a format string.
9. **A dependency's text is never kept** (see the line format): the layer drops
   its message and fields and writes level and source, for warnings and errors
   only. The shell's own warnings use fixed text too (the system's error text
   for a window icon is not copied).
10. The stderr fallback of `diag::failure` (no subscriber: Android, tests) prints
   the same kind, not the error's text.

Tests that hold these rules (they fail if a rule is broken):
`diag::tests::an_error_kind_is_a_name_from_a_fixed_list_never_the_errors_own_text`,
`trace::tests::dependency_text_never_reaches_the_log_only_the_level_and_source_of_warnings_and_errors`,
`trace::tests::the_key_log_reaches_the_log_only_as_fixed_facts_in_both_modes_and_the_file_keeps_its_line`,
`trace::tests::failures_and_hub_calls_reach_the_log_as_kinds_and_tags_only_in_both_modes`
(short secrets, long tokens, multiline text, URL variants with credentials,
queries and fragments, hosts with ports, Windows, UNC and Unix paths, addresses
and file names, in both verbose modes) and
`trace::tests::secret_named_fields_and_long_tokens_never_print`.

## Rotation, retention and bounds

- One file per start, `YYYY-MM-DD_HH-MM-SS.log`. At 8 MiB or 24 hours (by wall
  clock, so sleep does not stop the day) the file is gzipped beside itself as
  `<name>.log.<32 hex>.gz` and a fresh file continues under the same name; a
  failed archive keeps the original.
- At every start, after every rollover and once a day, files this log owns
  older than 30 days go, then the oldest ones until the folder is under 64 MiB.
  The active file is never removed. Only files matching the log's own names are
  ever touched (`identity-log.txt`, `log-salt` and anything foreign stay).
- The writer is a separate thread behind a bounded, lossy queue (16 384 lines):
  a slow disk drops lines; it never blocks the app and never grows memory.
- If `diagnostics/logs` cannot take a file, the log goes to its own folder,
  `<temporary folder>/hubchat-logs`, with the same file names, so the same
  pruning and budget apply to it (it is swept at every start, used or not).
  The Developer page and `Open logs folder` show the folder the log really is
  in and say that it is a temporary one. With no folder that takes a file, or
  when another subscriber already owns the process, no log is running: nothing
  is kept, the page says so, the switch is not offered and `Open logs folder`
  refuses. The log is reported as running only after a file is open and the
  subscriber is installed. Tests:
  `trace::tests::a_folder_that_cannot_be_written_falls_back_to_an_owned_folder_that_is_pruned_and_reported`,
  `..::the_usual_folder_wins_when_it_works_and_the_fallback_folder_is_still_swept`,
  `..::with_no_writable_folder_or_no_subscriber_no_log_is_reported_running`.

## Verbose switch

Precedence: `HUBCHAT_LOG_VERBOSE=0|1` for the run, else the stored setting, else
the build (development: on; release: off). Settings > Developer shows the
switch; with the environment pin set it explains instead of offering a switch.
The setting lives in `<app data>/verbose-logging`, outside the identity store,
because the store is wiped when the identity changes and this is a device
preference. [decided: coordinator contract with rig-opus, 2026-10-11]

## Startup order

`app_data_dir` → panic hook (`crash::install`, rig-opus) → `trace::init` →
`core::init` → tray. `trace::init` never fails the app. The panic hook is the
only one in the process; after it saves its report it calls `trace::emergency`,
which appends one pointer line (report name, thread, source line, never the
payload) straight to the active file, because a release build aborts after the
hook and would lose a queued line. The two modules join in
`trace::init`: `crate::crash::note_panics_in(emergency)` registers the pointer
line, and `crate::tray::before_restart(shutdown)` makes Restart Hubchat (which
skips the Exit event) flush the file before it restarts.

## Reading a log

```text
rg "WARNING|ERROR" <start>.log               warnings and errors
rg "hub:90ab12cd" <start>.log                one hub (read the .gz parts of a long run too)
rg "RESPONSE .* (4|5)[0-9][0-9] " <start>.log   hub refusals and failures
rg "engine.send" <start>.log                 sends (verbose: with timing)
```

Rolled-over parts are gzip files (`gzip -dc`, or any archiver); the active file
keeps its start-time name.

## Limits

- Android is not changed: no file log, no Developer tab, no subscriber. The
  core's failures go to stderr there as a kind (`diag::failure` falls back when
  no subscriber is set); before this work they carried the error's text. The
  new Android-only code paths (`keylog::Fact::PushCleanupFailed`) are not
  compiled here.
- `identity-log.txt` is the pre-existing durable record and keeps its lines
  (including a key store's own error text); it is local, 64 KB, and was not
  made part of the normal log's contract.
- `RQ` ids cover work started through `on_core` and synchronous commands. A
  command that is an `async fn` and does not call `on_core` has a `COMMAND`
  line (verbose) but no request span.
- Hub calls other than JSON posts, deletes and the health probe (history,
  conversations, roster and devices GETs, uploads, downloads) are not wrapped in
  `diag::Wire`; their failures are logged by the callers as kinds.
- The renderer does not write to the log.
- Measured results and what was not measured: see the hand-in for the landing.
