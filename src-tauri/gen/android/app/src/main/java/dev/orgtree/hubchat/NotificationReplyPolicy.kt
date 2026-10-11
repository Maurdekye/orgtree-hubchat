package dev.orgtree.hubchat

import java.util.UUID

internal fun validReplyId(id: String): Boolean = try {
  UUID.fromString(id).toString() == id
} catch (_: IllegalArgumentException) { false }

internal enum class ReplyWorkOutcome { COMPLETE, RETRY, REJECTED }

/** Admission always precedes the network check, even offline or on a cold process. */
internal fun runReplyWork(
  attach: () -> Unit,
  queue: () -> String,
  check: () -> Boolean,
  queued: () -> Unit,
): ReplyWorkOutcome {
  attach()
  return when (queue()) {
    "queued" -> { queued(); if (check()) ReplyWorkOutcome.COMPLETE else ReplyWorkOutcome.RETRY }
    "rejected" -> ReplyWorkOutcome.REJECTED
    else -> ReplyWorkOutcome.RETRY
  }
}
