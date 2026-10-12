package dev.orgtree.hubchat

fun main() {
  var passed = 0
  fun case(name: String, run: () -> Unit) { run(); passed++; println("PASS $name") }
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
  println("$passed checks passed")
}
