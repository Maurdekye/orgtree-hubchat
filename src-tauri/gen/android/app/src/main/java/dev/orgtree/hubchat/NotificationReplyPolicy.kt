package dev.orgtree.hubchat

import java.util.UUID

internal fun validReplyId(id: String): Boolean = try {
  UUID.fromString(id).toString() == id
} catch (_: IllegalArgumentException) { false }

internal enum class ReplyWorkOutcome { COMPLETE, RETRY, REJECTED }

/** The notification episode, not merely the peer, owns status/dismissal. */
internal fun matchesReplyNotification(peer: String, token: String, tag: String?, currentToken: String?): Boolean =
  tag == peer && currentToken == token

/** A complete send check dismisses the episode; failures retain visible status. */
internal fun finishReplyNotification(outcome: ReplyWorkOutcome, dismiss: () -> Unit, failed: () -> Unit) {
  if (outcome == ReplyWorkOutcome.COMPLETE) dismiss() else failed()
}

/** Admission always precedes the network check, even offline or on a cold process. */
internal fun runReplyWork(
  attach: () -> Unit,
  queue: () -> String,
  check: () -> Boolean,
  queued: () -> Unit,
): ReplyWorkOutcome {
  return try {
    attach()
    when (queue()) {
      "queued" -> { queued(); if (check()) ReplyWorkOutcome.COMPLETE else ReplyWorkOutcome.RETRY }
      "rejected" -> ReplyWorkOutcome.REJECTED
      else -> ReplyWorkOutcome.RETRY
    }
  } catch (_: Exception) { ReplyWorkOutcome.RETRY }
}

/** Only durable completion or an established rejection may remove saved text. */
internal fun finishReplyHandoff(outcome: ReplyWorkOutcome, remove: () -> Unit) {
  if (outcome != ReplyWorkOutcome.RETRY) remove()
}
