package dev.orgtree.hubchat

fun main() {
  var passed = 0
  var failed = 0
  fun case(name: String, run: () -> Unit) {
    try { run(); passed++; println("PASS $name") }
    catch (e: Throwable) { failed++; println("FAIL $name: ${e.message}") }
  }
  case("canonical ids only, no path traversal") {
    check(validReplyId("bb05866b-b96c-46c0-ae3f-236b3dd65fa2"))
    for (bad in listOf("../secret", "", "1-1-1-1-1", "BB05866B-B96C-46C0-AE3F-236B3DD65FA2")) check(!validReplyId(bad))
  }
  case("cold start admits before networking") {
    val events = mutableListOf<String>()
    check(runReplyWork({ events.add("attach") }, { events.add("queue"); "queued" },
      { events.add("check"); "sent" }, { events.add("status") }) == ReplyWorkOutcome.COMPLETE)
    check(events == listOf("attach", "queue", "status", "check"))
  }
  case("offline keeps durable work retryable") {
    var admitted = 0
    var notified = 0
    repeat(2) {
      check(runReplyWork({}, { admitted++; "queued" }, { "retry" }, { notified++ }) == ReplyWorkOutcome.RETRY)
    }
    check(admitted == 2 && notified == 2) // the Rust receipt de-duplicates admission
  }
  case("identity rejection never checks or claims queued") {
    check(runReplyWork({}, { "rejected" }, { error("must not send") }, { error("must not claim queued") }) == ReplyWorkOutcome.REJECTED)
  }
  case("storage retry never reaches network") {
    check(runReplyWork({}, { "retry" }, { error("must not send") }, { error("must not claim queued") }) == ReplyWorkOutcome.RETRY)
  }
  case("bridge failure retries without claims") {
    check(runReplyWork({}, { "" }, { error("must not send") }, { error("must not claim queued") }) == ReplyWorkOutcome.RETRY)
  }
  case("unready core keeps sole handoff until eventual completion") {
    var saved = true
    var attempts = 0
    var checks = 0
    repeat(3) {
      val result = runReplyWork({}, { attempts++; if (attempts < 3) "retry" else "queued" }, { checks++; "sent" }, {})
      finishReplyHandoff(result) { saved = false }
      check(saved == (attempts < 3))
    }
    check(!saved && checks == 1)
  }
  case("temporary attach exception retains text without attempting admission") {
    var saved = true
    val result = runReplyWork({ throw java.io.IOException("synthetic") }, { error("no admission") }, { error("no check") }, {})
    finishReplyHandoff(result) { saved = false }
    check(result == ReplyWorkOutcome.RETRY && saved)
  }
  case("successful reply dismisses only after the send check and before deleting handoff") {
    val events = mutableListOf<String>()
    val outcome = runReplyWork({ events.add("attach") }, { events.add("queue"); "queued" },
      { events.add("check"); "sent" }, { events.add("queued") })
    finishReplyNotification(outcome, { events.add("dismiss") }, { error("no failure") })
    finishReplyHandoff(outcome) { events.add("delete") }
    check(events == listOf("attach", "queue", "queued", "check", "dismiss", "delete"))
  }
  case("retry and exceptions keep notification and saved text with failure status") {
    for (throwCheck in listOf(false, true)) {
      var dismissed = false; var status = false; var saved = true
      val outcome = runReplyWork({}, { "queued" }, { if (throwCheck) error("offline") else "retry" }, {})
      finishReplyNotification(outcome, { dismissed = true }, { status = true })
      finishReplyHandoff(outcome) { saved = false }
      check(outcome == ReplyWorkOutcome.RETRY && !dismissed && status && saved)
    }
  }
  case("rejected reply keeps notice and shows failure without changing read state") {
    var failedNotice = 0
    finishReplyNotification(ReplyWorkOutcome.REJECTED, { error("must keep notification") }, { failedNotice++ })
    check(failedNotice == 1)
  }
  case("episode matching preserves newer same-chat and other-chat notices") {
    val notices = mutableMapOf("peer" to "old", "other" to "another")
    fun dismiss(token: String) {
      if (matchesReplyNotification("peer", token, "peer", notices["peer"])) notices.remove("peer")
    }
    notices["peer"] = "new"
    dismiss("old")
    check(notices == mapOf("peer" to "new", "other" to "another"))
    dismiss("new")
    check(notices == mapOf("other" to "another"))
    check(!matchesReplyNotification("peer", "new", "other", "new"))
    check(!matchesReplyNotification("peer", "new", "peer", null))
    check(!matchesReplyNotification("peer", "new", null, "new"))
  }
  case("per-reply success completes without any all-hubs result") {
    for (state in listOf("sent", "gone")) {
      var dismissed = false
      val outcome = runReplyWork({}, { "queued" }, { state }, {})
      finishReplyNotification(outcome, { dismissed = true }, { error("no failure") })
      check(outcome == ReplyWorkOutcome.COMPLETE && dismissed)
    }
  }
  case("failed reply keeps its notice but text remains in the durable outbox history") {
    var dismissed = false; var errorShown = false; var sealedHandoff = true
    val outcome = runReplyWork({}, { "queued" }, { "failed" }, {})
    finishReplyNotification(outcome, { dismissed = true }, { errorShown = true })
    finishReplyHandoff(outcome) { sealedHandoff = false }
    check(outcome == ReplyWorkOutcome.FAILED && !dismissed && errorShown && !sealedHandoff)
  }
  case("changed identity and unknown bridge statuses never dismiss") {
    for (state in listOf("rejected", "", "unexpected")) {
      val outcome = runReplyWork({}, { "queued" }, { state }, {})
      check(outcome != ReplyWorkOutcome.COMPLETE)
      finishReplyNotification(outcome, { error("must not dismiss") }, {})
    }
  }
  case("incoming messages preserve the active Reply action identity") {
    check(replyActionToken("owner", "owner", "stable-action", "new-message") == "stable-action")
  }
  for (count in 1..3) case("one send clears all $count messages already shown, even with an older action binding") {
    val latest = "v$count"
    val captured = captureReplyVersion("owner", "owner", "v1", latest, latest)
    check(captured == latest)
    var notification: String? = latest
    if (matchesReplyNotification("peer", captured, "peer", notification)) notification = null
    check(notification == null)
    check(!matchesReplyNotification("peer", captured, "peer", "arrived-after-send"))
  }
  case("other accounts or replaced episodes cannot inherit an action or version") {
    check(replyActionToken("ownerB", "ownerA", "old-action", "fresh") == "fresh")
    check(replyActionToken("owner", "owner", null, "fresh") == "fresh")
    check(captureReplyVersion("ownerB", "ownerA", "old-action", "new-action", "new-message") == "old-action")
    check(captureReplyVersion("owner", null, "old-action", "new-action", "new-message") == "old-action")
    check(captureReplyVersion("owner", "owner", "action", "action", null) == "action")
  }
  case("a newer message after send remains protected from the completed reply") {
    val captured = captureReplyVersion("owner", "owner", "action", "action", "at-send")
    check(!matchesReplyNotification("peer", captured, "peer", "after-send"))
  }
  case("RemoteInput is acknowledged before exact-version cancellation") {
    val calls = mutableListOf<String>()
    completeReplyVersion("v3", "v3", true, { calls.add("ack") }, { calls.add("clear") })
    check(calls == listOf("ack", "clear"))
  }
  case("post-send newer message gets acknowledgement without being dismissed") {
    val calls = mutableListOf<String>()
    completeReplyVersion("v3", "after-send", true, { calls.add("ack") }, { calls.add("clear") })
    check(calls == listOf("ack"))
    calls.clear()
    completeReplyVersion("v3", "new-account", false, { calls.add("ack") }, { calls.add("clear") })
    check(calls.isEmpty())
  }
  case("two quick replies on three shown messages never leave a sending completion pending") {
    var current: String? = "v3"
    var spinning = true
    val events = mutableListOf<String>()
    repeat(2) { reply ->
      spinning = true
      completeReplyVersion("v3", current, true,
        { spinning = false; events.add("ack$reply") },
        { current = null; spinning = false; events.add("clear$reply") })
      check(!spinning && current == null)
    }
    check(events == listOf("ack0", "clear0", "clear1"))
  }
  println("$passed checks passed; $failed failed")
  kotlin.system.exitProcess(if (failed == 0) 0 else 1)
}
