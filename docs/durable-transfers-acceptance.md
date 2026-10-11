# Durable file transfers: review and device acceptance

The branch `agy-astra/durable-transfers` adds durable local admission, progress,
recovery and controls for files that can take minutes to send or save. It is based
on Hubchat 1.0.5 (`6bd201b`). Source slices through `3099dfc` have been reviewed by
hubchat-opus; this is **not whole-phase acceptance or a release approval**.

## User-visible contract

- Sending first makes an immutable private copy with Preparing feedback. The
  command returns after local admission; network work belongs to the engine.
- No message is sent until all its files finish uploading. Failure or cancellation
  holds the message. Retry starts a new attempt. Remove file sends the remaining
  message once its other files are ready; an empty file-only message cannot be sent.
- A failed message does not block later messages to the same person.
- Download returns after durable queue admission. Cards and the global list show
  confirmed progress, waiting, finishing, failure and retry across app restarts.
- Success is silent. In particular, the previous Saved to Downloads toast is gone;
  the card changes to Saved with Open. User cancellation is also silent. Terminal
  file failures create one local notice per message and explicit retry episode.
- Android uses actual foreground-service coverage. Stay connected keeps its
  existing service; otherwise visible file admission starts a temporary dataSync
  service. Hidden uncovered file work pauses with instructions. Push checks do not
  authorize file work. Mail polling retains the released push-only suspension rule.
- Saved partial bytes expire after 24 hours idle, including wanted jobs waiting on
  an offline hub. The next attempt starts over, with an explanation on the card.

## Measured before whole-phase device acceptance

- Core unit/integration-in-lib suite: 118 passed, including real loopback HTTP
  interruption, resumed offsets, exact bytes, scheduler capacity and ordinary mail,
  store reopen, cancellation, failure episodes, publication and cleanup races.
- Core library clippy with warnings denied passed. Desktop shell: 24 passed,
  two existing DPAPI tests ignored. Kotlin compiles; six JVM policy tests pass.
- Browser mocks cover Preparing, download admission/progress, upload controls,
  lifecycle instructions and paged transfer lists, including phone widths and
  inspected screenshots. They do not prove Android services or notifications.
- A scratch published 2.1.0 hub previously proved a 24 MB upload/download through
  the engine, interruption and confirmed offsets at 0, 8 MiB, 16 MiB and completion.
  It was run before the final lifecycle and cleanup deltas, not against this tip.
- Android Rust shell check passed for `aarch64-linux-android` at `3099dfc`.
  This compiles the JNI bridge but does not execute it on Android.
- No 1 GiB run, native notification visual proof, actual process-kill recovery or
  Android MediaStore replay is claimed yet. Source approval is not runtime proof.

## Device session (hubchat-opus owns device/build actions)

Use isolated test identities and a scratch hub, never live user data. Record the
commit, APK hash, Android version, hub version and file SHA-256. Configure the
scratch hub limit to admit a 1 GiB file (1,073,741,824 bytes). Keep traces and
screenshots on D:. Stop test processes and remove disposable fixtures afterward.

1. **Admission and large files.** Send a 1 GiB file with text. Check Preparing
   progress, duplicate-send prevention and local admission while offline. Change
   the original after admission; upload must use the snapshot. Restore the hub,
   observe confirmed progress and Finishing, and compare receiver SHA-256. Confirm
   the recipient gets no message until every file is uploaded. Measure peak memory
   and final PATCH/digest duration. Ordinary messages to another peer and image
   previews must keep working during the transfer.
2. **Interrupt and reopen.** Interrupt upload and download after a confirmed
   checkpoint. Kill the test app, reopen and inspect the saved job. Upload must
   reconcile the hub offset; download must use Range/If-Range and exact bytes.
   Repeat with the hub restarting, an expired upload, and a changed download
   validator. No false success or duplicate visible saved copy is acceptable.
3. **Controls.** Cancel a running upload and a queued one; the whole message stays
   held and there is no failure notice. Retry resumes appropriately. Remove the
   held file: remaining text/files send only when ready. A file-only empty message
   remains held. Cancel and retry a download, including a file in one's sent mail.
4. **Failure notices.** Fail two files in one message: one local notice, correct
   upload/download wording, no path/URL/body leak. Reopen after reservation before
   acknowledgement: same notice key, no repeated sound. Retry then fail: a new
   episode. Settings off suppress notices; opening the chat does not clear a file
   failure notice. Success and user cancellation stay silent. Check native visuals.
5. **Android lifecycle modes.** With push off and Stay connected on, hide the app:
   files continue and no second ongoing notification remains. With push on, start
   files while visible: temporary service and its ongoing notification appear;
   hide and confirm continued progress. Completion stops it. With push off and
   Stay connected off, verify mail's prior polling behavior is preserved while the
   process lives. A background push/check may fetch ordinary mail but must not
   restart paused file jobs without service coverage.
6. **Refusal and timeout.** Force service-start refusal and Android's dataSync
   timeout. Pending jobs remain durable, no terminal notice, plain keep-open hint,
   and reopening allows resumption. Also force failure inside onCreate's
   startForeground after startForegroundService returned: verify the app does not
   crash with the foreground-service deadline exception. Passed on the Android 15
   emulator by hubchat-opus on 2026-10-11 at 09:08Z: refusal at service start and
   inside onCreate both showed the hint without a crash. A forced 20-second
   dataSync timeout paused the hidden upload silently; reopening resumed it.
7. **MediaStore boundaries.** For typed .jpg/.pdf files, stop after pending insert
   but before receipt write, during copy, and after IS_PENDING clears but before
   Rust commits the location. Replay must find/reuse one URI even if the provider
   appends a MIME extension. No visible duplicate. During pending copy, delete the
   message or reset damaged private bytes; durable cleanup must remove only the
   app-owned pending row and unreferenced private copy. Reopen across cleanup.
   A completed public URI always survives abandonment and user edits. Delete a
   saved file manually; Save again downloads afresh. Check Android 10 inclusion
   behavior separately if a device is available.
8. **UI and storage.** Inspect phone light/dark screens, image viewer, two jobs on
   different hubs and a large transfer list. Simulate 24-hour idle partial expiry
   in a disposable database and verify the Starting over explanation persists.
   Confirm private staged copies disappear after send/delete, failed copies remain
   for Retry, receipt cleanup follows pointer commit and undeletable bytes retry.

## Limits to retain in the hand-in

- macOS/Linux notification fallback has no stable-key replay de-duplication yet;
  Windows and Android do. These prototype platforms need separate native acceptance.
- Unix no-overwrite publication uses hard links; filesystems without hard-link
  support (for example exFAT) do not have a fallback. Hard power-loss durability is
  not proved by staged crash-boundary or graceful-stop tests.
- Cancelling/removing an upload currently leaves its hub partial until expiry;
  there is no best-effort remote DELETE yet.
- Long message bodies have a separate bounded durable lane; missing/oversized
  bodies retain their preview and stop retrying. A pending/unavailable body hint
  has not been added to the message UI.
- A 24-hour offline interval intentionally discards saved partial progress.
- Preview requests use their own two-slot lane, with no image-size cap.
- Receipt and MediaStore checks above require the device session; JVM policy tests
  and Kotlin compilation cannot establish provider behavior.
