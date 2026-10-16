# Opening downloaded files on Android

The completed download card already called `hc_open_attachment`, including on
Android. Its content URI went through the generic URL opener, which constructs
an ACTION_VIEW without an explicit MIME type or URI read grant. This is a
source diagnosis; a phone reproduction is separate from the source checks.

Android now routes that command through `DownloadOpener`. MediaStore downloads
retain their existing content URI. Filesystem downloads use the existing
FileProvider, with authority derived from `context.packageName`. The private
`filesDir/downloads` root is exposed narrowly; no additional copy is made.
ACTION_VIEW includes the read grant and ClipData. APK names force
`application/vnd.android.package-archive`; other names use the extension MIME,
then resolver MIME, then `application/octet-stream`. Android owns viewer choice
and package-install consent. The existing REQUEST_INSTALL_PACKAGES permission
is retained. Missing handlers and inaccessible files return short errors through
the existing card toast. Desktop open and reveal retain their existing routing.

This follows the coordinator's 2026-10-16 ruling: MediaStore is already a
shareable content URI, so FileProvider is needed only for filesystem paths.

## Verification

- Source routing regression committed before implementation: old source 1 pass,
  4 failures; fixed source 5 passes. These assert wiring, not Android execution.
- Mounted renderer controls: 5 passes, covering card click, native error toast,
  and desktop open/reveal. This path was already functional before the fix, so
  these are preservation controls, not a claimed red reproduction.
- `DownloadOpenerTest` contains seven native instrumentation cases using captured
  intents, synthetic file contents and the runtime applicationId. It does not
  launch a viewer or installer. One case proves that a private file outside
  the provider roots is refused and becomes a short error without dispatch.
  All seven passed on the Android TEST identity (`dev.orgtree.hubchat.test`).
  The same-cert instrumentation runner was removed afterward; the app and its
  data were kept. Real-identity runtime execution is not claimed.
- Both TEST and real merged manifests were generated and inspected: each has
  its own applicationId-based FileProvider authority, URI grant support and
  REQUEST_INSTALL_PACKAGES. Real-app verification stops at the manifest.

## Device acceptance, 2026-10-16

The independently reviewed source is `a0ed83f` on `933fba2`. Device source
`f5a87ba` applies the identical download changes on approved Back fix `bd6e178`.
Only the local build configuration changes its TEST version to
`1.2.1-alpha.3`; no source version bump is included.

The signed ARM64 APK was checked for the TEST identity, unchanged test
certificate, v2 signature and 16 KiB alignment. Its packaged
`lib/arm64-v8a/libhubchat_lib.so` matches the compiled archive byte for byte.
An initial local packaging-script mistake retained the archive's filename;
ZIP inspection caught it before installation. The corrected package was
installed with `-r`, preserving TEST data. The earlier artifact is retained as
failed packaging evidence, not a deliverable.

On the authorized phone, the coordinator supplied a text file and APK in the
existing TEST chat. Both downloaded to completed Saved/Open cards. Clicking
the text card opened Android's chooser; selecting HTML Viewer just once
displayed the fixture text. Clicking the APK card opened Android's own
"Permission required / Hubchat Test" install-source permission sheet.
The sheet was cancelled. No security setting was changed and no attachment
was installed. The coordinator accepted this sheet as the installer-handoff
endpoint; final installation confirmation and installation remain unmeasured.
The TEST app and fixture messages remain in place.

An explicit force-stop and relaunch also confirmed that Contact info's Back
arrow returns to a responsive conversation. No messages were sent from the
TEST identity. Device results, screenshots and build logs are in the docket
handoff; they are distinct from the source and mounted checks above.
