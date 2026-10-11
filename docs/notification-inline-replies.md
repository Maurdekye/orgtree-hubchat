# Inline notification replies on Android

Hubchat's currently implemented mobile message notifications use Android's
ConnectionService (minimum API24). The per-chat notification gains a **Reply**
action using Android RemoteInput. Normal notification taps still open that chat;
the group summary opens Hubchat, and transfer failure notifications keep their
existing behavior. This change does not add a mobile platform.

The reply action names the account and chat in its explicit base Intent URI,
with a unique notification token. Only its broadcast PendingIntent is mutable,
so Android can attach RemoteInput; the tap PendingIntent remains immutable.
The receiver is unexported. Distinct URIs prevent PendingIntent hash collisions
from mixing chats. Replies do not open an activity or change connection settings.

The receiver atomically stores a Keystore-sealed handoff in the app's private
files before enqueueing unique WorkManager work. WorkManager Data contains only
the handoff UUID, so its size limit does not truncate or limit the reply text.
Blank input is rejected; the nonblank text, including Unicode, newlines and
leading/trailing spaces, is preserved. The in-chat composer draft is untouched.
On core attachment, saved handoffs recover the crash gap before work enqueue.

The worker attaches the existing core without requiring an activity. It admits
the reply into the ordinary durable outbox before its bounded45-second message
check, including offline, UnifiedPush and periodic-check modes. Android31+ work
is expedited with ordinary-work fallback; earlier Android uses ordinary work.
Android scheduling and background restrictions can delay execution.

Admission verifies the notification's account against the active engine and
the store's identity in one transaction. A consumed UUID and queued outgoing
message commit together. Migration14 keeps UUID receipts after message deletion,
so interrupted or repeated workers cannot send a second copy or resurrect a
deleted message. Receipts contain only UUIDs, remain for this stored identity,
and are cleared on identity wipe. Hub selection, network retries, message ids
and delivery receipts continue through the existing sender. A text reply is an
ordinary chat message, without an attachment or an inferred quote target.

Notification status says **Reply queued**, never claims delivery without a hub
receipt. Admission rejection directs the user to open the chat and check their
reply. Updates match the original notification token and share the posting/clear
lock, so a newer incoming notification or read receipt is not overwritten.

## Verification

Measured2026-10-11 on branch `hubchat-reply-sol/inline-replies`: six focused
Rust tests passed (`0 failed`, `122 filtered out`) after a locked core test
compile. Standalone Kotlin2.2.10 compiled ConnectionService, SecretBox, reply
receiver/worker and policy against Android37, AndroidX core1.13.1 and
WorkManager2.10.5 APIs. All six policy checks passed. The first API attempt
found a wrong notification builder recovery call, corrected to AndroidX's
notification-copy constructor before the passing compile. Owned build output
measured1,151,088,918 bytes, under the100GB cleanup threshold.

Focused Rust tests exercise offline admission, exact text/draft preservation,
identity rejection, stopped-engine rejection, receipt rollback, competing workers,
restart replay and replay after message deletion. They run against scratch SQLite
stores without a hub or live data. The standalone Kotlin script
`tools/verify-notification-reply.ps1` compiles the changed Android code against
installed Android/AndroidX APIs while stubbing unrelated app dependencies, then
runs pure policy checks for cold-start order, offline retry, invalid ids and
rejection. It requires the machine build lock and writes only to the reserved
`C:\orgtree-build\hubchat-reply-sol` folder.

Source/API checks do not prove Android's notification UI, actual RemoteInput
delivery, Keystore/spool lifecycle, WorkManager scheduling or JNI linkage in an
APK. Those need a coordinator-assigned isolated device/app verification session,
including API24 and API31+, cold process, two chats, offline, preview disabled,
new incoming mail during reply, read-receipt dismissal and identity changes.

Android API references: [direct reply](https://developer.android.com/develop/ui/compose/notifications/create-notification)
and [WorkManager requests](https://developer.android.com/develop/background-work/background-tasks/persistent/getting-started/define-work).
