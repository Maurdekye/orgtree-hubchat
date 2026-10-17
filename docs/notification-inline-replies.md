# Inline notification replies on Android

Hubchat's currently implemented mobile message notifications use Android's
ConnectionService (minimum API24). The per-chat notification gains a **Reply**
action using Android RemoteInput. Normal notification taps still open that chat;
the group summary opens Hubchat, and transfer failure notifications keep their
existing behavior. This change does not add a mobile platform.

Both incoming hub paths capture the producing engine identity in the incoming
event. The shell passes that immutable owner through the Android bridge into
the reply Intent and sealed handoff; a delayed old-account event cannot acquire
a new account owner from mutable current metadata.

The reply action names the account and chat in its explicit base Intent URI,
with a stable per-episode action token. Incoming updates reuse that action
identity; each posted message still has a separate version token. The receiver
captures the current version at Send so later arrivals survive completion.
Only its broadcast PendingIntent is mutable,
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
the reply into the ordinary durable outbox before its bounded45-second per-reply
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

Notification status says **Reply queued** during the send check, never claims
delivery without a hub receipt. A completed worker dismisses the original
notification; a retry or rejection retains it with retry/error status.
Unavailable core/engine, stopped-engine and bridge failures return retry and
retain the sealed text. Core publication can precede key loading: concurrent
cold attaches serialize that load, and a failed attempt can be retried. A
successful cold initialization is latched so later attaches do not reload a
departing key during an intentional identity switch. Unreadable keys keep startup
retryable. Only established invalid input or identity is a permanent rejection.

Admission rejection directs the user to open the chat and check their
reply. Completion uses the notification version captured at Send and shares
the posting/clear lock. A newer incoming version survives; completion still
acknowledges RemoteInput so that its sending spinner does not remain.

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
rejection. Its current invocation requires the machine build lock and writes
only to the assigned `C:/orgtree-build/hubchat-jumplist-sol` folder; the original
2026-10-11 run used its then-author's reserved `hubchat-reply-sol` folder.

Source/API checks do not prove Android's notification UI, actual RemoteInput
delivery, Keystore/spool lifecycle, WorkManager scheduling or JNI linkage in an
APK. Those need a coordinator-assigned isolated device/app verification session,
including API24 and API31+, cold process, two chats, offline, preview disabled,
new incoming mail during reply, read-receipt dismissal and identity changes.

Android API references: [direct reply](https://developer.android.com/develop/ui/compose/notifications/create-notification)
and [WorkManager requests](https://developer.android.com/develop/background-work/background-tasks/persistent/getting-started/define-work).

## Corrective review proof (2026-10-12)

Reconciled with main6c606c0: migration14 was already present; retained DM15,
both wipe entries, both module declarations and Close DM invariants. No
question migration16 change.

The extracted predecessor eb7589e retained the original owner-sampling and
missing-engine rejection behavior in the production payload/admission helpers.
Controlled tests failed all three new assertions (six existing tests passed):
a delayed A event sampled B after the shared scratch store was wiped/adopted,
and unavailable-engine/cold-init-failure calls returned rejected instead of
retry. Round2 corrected source passed all12 focused core tests. The barrier-controlled
production-helper test also covers core publication before engine availability,
two concurrent cold attaches, an injected failed initial load, one eventual
engine load and exactly-once durable UUID admission. An unreadable key leaves
the shell/lost-key UI available while remaining retryable. It verifies that a later
attach does not reload the key after a deliberate departure.

These are measured component schedules using production helpers and scratch
SQLite; they do not invoke Tauri's global CORE, Android JNI, Keystore or an
actual WorkManager process. Shell/JNI wiring is checked separately by source
review and the assigned Android-target compiler. The delayed callback preserves
A as its action owner; Engine B rejects it and creates no outgoing row.

Measured round2 source2b804c6: `cargo test --locked -j2 -p hubchat-core
notification_reply -- --nocapture` passed12/12 (128 filtered), compilation20.47s.
`cargo check --locked -j2 -p hubchat --lib --target aarch64-linux-android`
passed warning-free using installed NDK27.3.13750724 Android24 clang/ar
(arm64-v8a), initial dependency check3m07s and final changed-source check3.93s.
No Android library link, APK build or device action was performed.

Standalone Kotlin API compilation passed with unrelated dependencies stubbed;
all8 policy checks passed, including unready/repeated attempts retaining the
sole handoff until completion and a transient attach exception retaining it
without admission/network work. The actual worker calls the tested handoff
completion helper. Evidence is in the author D rig evidence folder; build
logs/output stay under the reserved author C root. Machine locks were released
after every bounded run. Owned build output measured10,707,725,723bytes, below
the100GB cleanup threshold.

## Round3 completion correction (2026-10-12)

Successful explicit key recovery now preserves `load_key_inner` readiness and
sets the same completion invariant as cold startup. Successful adoption also
runs under the establishment guard and latches completion only after the engine
is established. An existing live engine is positive readiness evidence; missing
or unreadable keys and failed establishment do not latch it. Later unsuccessful
explicit attempts do not clear previously established completion. The latch
suppresses background startup loading, and never skips intentional adoption.

Android push-mode callbacks can synchronously call core attachment while an
adoption is in progress. Attachment uses a nonblocking gate: busy or nested
attachment returns retry rather than waiting on its own guard. Actual identity
loads remain serialized. Post-adoption push refresh runs after the guard.

Measured predecessor eb4e83c retained the missing-latch behavior and failed all3
new controls (prior12 passed): explicit recovery and adoption after unreadable
startup each created a replacement A engine in a worker during simulated
departure while A's synthetic key remained present; an already-ready engine
also failed to complete startup. The adoption wrapper in that control source
serialized establishment but did not latch it. Corrected source004ead8 passes
17 focused tests (0 failed, 128 filtered), compile19.53s. Recovery and adoption
controls each exercise unreadable and failed initialization, simulate taking
A before key deletion, verify no background reload, then successfully adopt B
and admit its durable UUID once. Negative readiness/error and nested mode-change
attachment controls pass. These are production-helper/Engine/scratch-store
schedules, not execution of global CORE, real key storage, unregister or JNI.

Final Android aarch64/NDK27/API24 Rust source check passed warning-free in3.90s.
Kotlin and JNI reply source did not change in this round; the measured8 policy
checks and API compilation from round2 still cover identical Kotlin sources.
Evidence: `inline-review-round3.json` and named logs in the author's D rig
evidence folder. Owned build bytes10,751,615,402, below100GB; all machine locks
released and no build process remains. Native-library linking, APK and device
runtime verification remain unperformed.

## Notification dismissal correction (2026-10-17)

On 1.2.2-alpha.3 a successful reply left **Reply queued** in the notification
shade. The worker now runs notification completion before removing the durable
handoff: `COMPLETE` dismisses; `RETRY` keeps the notice and says it could not
confirm sending and is retrying; `REJECTED` keeps the existing check-your-reply
error. Admission and the bounded send check retain their existing semantics.
No read flag, read receipt, outbox logic or delivery guarantee changes.

Dismissal checks message-notification id, peer tag and the original unique token
under the same `NotificationReplies` monitor as posting, status updates and
clearing. A newer incoming message for that peer or any other peer survives.
The existing `clearMessage` method also removes the group summary only when no
other conversation notification remains. A late receiver update cannot recreate
a dismissed notice; receiver queue feedback now precedes WorkManager enqueue,
so it cannot overwrite a fast worker's later failure status.

`tools/verify-notification-reply.ps1 -ControlPolicy <C-scope predecessor .kt>`
can run the prior completion policy against the new tests before the corrected
SDK/API build and policy checks. Controls use distinct JVM class directories;
the script checks raw exit status and expected failing-case count.

Measured author results (base local main `da65918`, production `de5b4b1`):

- Predecessor policy control `9c58d54`: **10 passed / 2 expected failures**,
  raw JVM exit **1**, before the green run. It fails completed-reply dismissal
  and retry/error feedback, while preserving the previous eight checks.
- Actual ConnectionService, SecretBox, reply receiver/worker and policy compile
  against the installed Android37/AndroidX APIs; unrelated app dependencies are
  stubbed. Corrected pure JVM policy checks: **12/12**, raw exit **0**.
- Source wiring checks: **2/2**. Worker uses the tested completion helper before
  handoff removal; receiver status ordering and shared post/status/clear monitor
  wiring are covered. Kotlin tests cover completion ordering, retry/exception
  preservation, rejection, matching episode and newer/other-chat preservation.

The phase used the shared build lock and a 2 GiB kernel Job. Evidence is
`C:/orgtree-build/hubchat-jumplist-sol/reply-dismissal.log` and
`reply-dismissal-result.json`, with separate control/green JVM class directories
in `kotlin-20261017T011301`. No Rust behavior changed. No APK/installer build,
device install, live notification operation or device test was performed.
Actual Android shade dismissal, WorkManager timing and the summary UI remain
integrated-device smoke checks; pure policy/API checks do not prove those.

## Per-reply completion correction (2026-10-17)

The user reported that alpha.4 still left a notification after its reply
arrived. The earlier fix was insufficient: it reused the general background
check, whose success requires **every hub and the whole outbox** to settle.
That predicate can be false after this reply is sent; it can also be true for
a permanently failed reply because failed messages are absent from the queue.
The previous policy tests supplied a mocked global success and missed both.

The worker now calls `checkNotificationReply(owner, id, peer, 45)`. Rust polls
only that admitted reply's stored metadata. Identity, consumed UUID, outgoing
direction and peer are checked under the short store lock; no body is loaded.
Sent/fetched/delivered/read completes; queued/sending stays retryable; failed
retains the notice with a check-chat error. A consumed UUID whose message was
intentionally deleted completes without recreating it. Database/startup errors
retry. Identity changes reject; stopped engines retry. No read state changes.
The existing token-checked cancellation and final group-summary cleanup remain.

Measured source checks at production `1b2874a` (base `7e41cb0`):

- Global-predicate control `4aa877d`: **1 passed / 2 expected failures**, raw
  cargo exit **101**. A sent reply with unrelated queued work incorrectly
  returned retry; a failed reply with an empty global queue returned sent.
- Corrected core notification-reply tests **22/22**, including those schedules,
  all sent states, owner/peer/engine changes, read-state preservation, deletion,
  admission/replay and cold-start checks.
- Kotlin API compilation succeeds against installed Android37/AndroidX APIs;
  pure JVM policy checks **15/15**. Wiring controls **2/2**. Android
  `aarch64-linux-android` Rust source check passes with installed NDK27/API24.
- Shared build lock, two Cargo jobs, 2/4 GiB kernel Jobs; no APK build/install.
  Evidence is `reply-state-red.log`, `reply-state-red-result.json`,
  `reply-state-{core,kotlin,android}.log`, and `reply-state-results.json` under
  `C:/orgtree-build/hubchat-jumplist-sol`.

Read-only phone control on installed alpha.4: the user's 09:07:21 UTC inline
reply returned WorkManager SUCCESS about 0.3 seconds later; the user reported
dismissal and the subsequent sanitized notification dump had no Hubchat records.
This proves the prior cancellation path can work; it does not establish the
cause of the earlier failure or verify the new code on a phone. Package/version,
Hubchat-only worker diagnostics and notification metadata were read with the
coordinator's explicit device window. Message bodies/titles, other apps and
raw notification dumps were not retained; no phone writes or server stop.
The subsequent controlled drawer run at 09:11:57 UTC also returned SUCCESS
within about 0.3 seconds and removed both the captured child and group summary;
the user confirmed dismissal. The earlier apparent failure was confounded by
new org messages posted immediately after replies. Their original tokens were
not captured, so no retrospective token comparison is claimed. Default Android
notification dumps redact custom string values; a hash of that redaction/type
descriptor is not a token hash. The corrected probe records unknown instead.
This control does not establish a generic drawer cancellation failure. A later
typing/version repro and consecutive-reply spinner repro are described below.
New-build device smoke remains outstanding.

### Incoming-message typing continuity

The user also reported that an incoming message closes the active inline reply
field. The old publisher generated a new Reply PendingIntent URI for every
incoming message. In the alpha.4 phone comparison, the notification's tag/id
and open-chat PendingIntent stayed the same while the Reply PendingIntentRecord
changed after one controlled incoming message. This action-identity change is
consistent with Android discarding a view bound to the old action.

The publisher now retains an owner-bound action token while that conversation
notification is active. It still mints a new notification-version token for each
message. At Send, the receiver snapshots the current version under the same
posting/clearing monitor. Thus messages arriving during typing are part of the
visible conversation when replying, while messages arriving after Send cannot
be cleared by that completion. A replaced owner/episode cannot inherit the
action; a reply racing with a removed notice is still preserved and admitted.
No UI callbacks, app foregrounding, notification suppression, new read semantics
or notification text logging are introduced.

The user confirmed that the field closed after the incoming update, and that
the subsequent reply did not dismiss the notice. Their separate two-message
test, with both messages already present before opening Reply, dismissed after
one reply. Multiple messages alone are not the cause; the changed action and
version during typing explain the failing schedule.

Measured stable-action control `e159142`, with a signature-only adapter for
the added owner argument: **18 passed / 3 expected failures**, raw JVM exit1.
The corrected send-time policy/API compilation passed **21/21**. Controls
include one, two and three visible message versions, older action bindings,
owner changes and protection for arrivals after Send. Evidence:
`reply-action.log` and `reply-action-result.json` in the reserved author C root.

### RemoteInput sending completion

In the user's three-message/two-consecutive-reply test, both reply workers
returned SUCCESS within about0.3 seconds, yet the second reply left the Send
button showing a permanent loading icon. Authorized read-only capture while
the user left that spinner visible found no active Hubchat NotificationManager
record, while SystemUI still referenced the Hubchat child key in RemoteInput
state. The app-filtered crash buffer had no entries. This supports retained
SystemUI sending state after app-side cancellation, rather than a worker crash;
the metadata cannot prove a specific vendor-internal lifetime rule.

Completion now copies the active notification, sets RemoteInput history to
the exact reply text, silently re-notifies, and then cancels if it still matches
the version captured at Send. This standard Android acknowledgement precedes
cancellation. A later incoming version from the same owner/action episode gets
the acknowledgement but retains its incoming content/version and stays visible.
Another owner/episode is untouched. If a concurrent reply already removed the
notice, repeat cancellation and final-summary cleanup instead of skipping
completion. Retry/error updates also acknowledge the input while keeping the
error and notification. Reply text remains inside the notification and sealed
handoff; no text logging is added. Older sealed handoffs without the new action
field remain readable.

Measured acknowledgement control `dc90711`: **21 passed / 3 expected failures**,
raw JVM exit1. Corrected production `fe14a47`: **24/24**, raw JVM exit0,
and compilation of actual ConnectionService, SecretBox, receiver/worker/policy
against installed Android37/AndroidX APIs succeeds with unrelated dependencies
stubbed. Four source wiring checks pass. The policy checks exercise command
ordering, protection of newer post-Send versions and repeated completions;
they do not execute SystemUI. Shared lock and2GiB kernel Job; evidence is
`reply-ack.log`, `reply-ack-result.json`, class root `kotlin-20261017T092454`
under `C:/orgtree-build/hubchat-jumplist-sol`.

The Rust completion source is unchanged from the earlier22-test and Android
aarch64 check. All phone data are from alpha.4; no APK was built or installed
by the author. Verify continuous typing, one successful reply after an incoming
update, consecutive replies without a stuck spinner, and preservation of
post-Send incoming messages in the next coordinator-built alpha.
