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
  The planned native execution targets TEST (`dev.orgtree.hubchat.test`);
  runtime-derived assertions also support the real identity, but no real-app
  execution is claimed. Neither identity has run these tests yet.
- Native compilation, manifest inspection, and device execution are recorded
  separately in the handoff; no phone verification follows from the source or
  mounted checks alone.
