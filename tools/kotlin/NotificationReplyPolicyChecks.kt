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
      { events.add("check"); true }, { events.add("status") }) == ReplyWorkOutcome.COMPLETE)
    check(events == listOf("attach", "queue", "status", "check"))
  }
  case("offline keeps durable work retryable") {
    var admitted = 0
    var notified = 0
    repeat(2) {
      check(runReplyWork({}, { admitted++; "queued" }, { false }, { notified++ }) == ReplyWorkOutcome.RETRY)
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
      val result = runReplyWork({}, { attempts++; if (attempts < 3) "retry" else "queued" }, { checks++; true }, {})
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
      { events.add("check"); true }, { events.add("queued") })
    finishReplyNotification(outcome, { events.add("dismiss") }, { error("no failure") })
    finishReplyHandoff(outcome) { events.add("delete") }
    check(events == listOf("attach", "queue", "queued", "check", "dismiss", "delete"))
  }
  case("retry and exceptions keep notification and saved text with failure status") {
    for (throwCheck in listOf(false, true)) {
      var dismissed = false; var status = false; var saved = true
      val outcome = runReplyWork({}, { "queued" }, { if (throwCheck) error("offline") else false }, {})
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
  println("$passed checks passed; $failed failed")
  check(failed == 0) { "$failed policy checks failed" }
}
