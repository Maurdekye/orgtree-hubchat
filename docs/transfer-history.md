# Transfer history and attachment actions

Transfers remains available in the desktop sidebar and above Android's chat
list when idle. Its current jobs keep their progress and Cancel behavior.
Opening it also reads one metadata page of finished transfers; Load more
advances the stable attachment-ID cursor in pages of at most 100. Undownloaded
incoming attachments are not transfer history. Closing the list releases its
history state; progress ticks alone do not load history pages.
Partial indexes select completed download/upload candidates before joining
message metadata, so unrequested incoming attachment history is not scanned.

Finished incoming rows offer View and Redownload. View opens the existing copy
with the system viewer. Finished outgoing rows offer View of their retained
source. Native admission binds the message and attachment, resolves the stored
path or content URI, and checks that the platform can still read it. A missing
or inaccessible local copy disables View with a visible reason. Files removed
while the list is open are checked again at View and report an error.

Redownload admits durable work, without waiting for the network in the command.
For a completed incoming job it resets the checkpoint, validator, digest and
export state, selects new private/destination paths, and leaves the old saved
copy intact. Queued/running work is reused, so repeated admission does not
create additional copies. The UI also disables pending actions. The last saved
copy remains the attachment's local pointer until a new publication wins.
Existing resumable retries and collision-safe publication remain in use.

SQLite migration 18 adds download_jobs.copy_key. Its default empty value keeps
old Android export receipts recoverable. An explicit fresh copy has a new
persisted suffix, stable across process death/retry. Export, cleanup and
abandonment all use the same suffixed receipt key, so identical contents save a
new public MediaStore copy. Platform URI strings are passed unchanged to the
existing Android typed ACTION_VIEW opener. No hub protocol changes.

A hub 404 or 410 marks the request expired. History explains that the file is no
longer available and disables Redownload; transport failures retain the existing
retry behavior. Other admission, local-viewer and download failures remain
visible. History pages serialize metadata and availability booleans, with no
message bodies, local paths, URIs or checkpoint validators. Platform availability
checks run after the bounded DB read releases the connection.

Verification sources: tools/proof-transfer-history.mjs mounts both platform
components with synthetic APIs; engine_download tests exercise actual local
files, loopback downloads, process-style store reopen, and a synthetic content
URI/export host; transfer_view tests check bounded/private pages. These tests do
not establish real Android viewer, permission or device behavior. No app build,
installer, live hub or real-chat action is part of this author hand-in.
