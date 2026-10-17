package dev.orgtree.hubchat

import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.util.AtomicFile
import androidx.core.app.NotificationCompat
import androidx.core.app.RemoteInput
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import org.json.JSONObject
import java.io.File
import java.util.UUID
import java.util.concurrent.TimeUnit

/** Explicit, unexported receiver: only the notification's PendingIntent grants access. */
class NotificationReplyReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    if (intent.action != NotificationReplies.ACTION) return
    val actionTarget = NotificationReplies.target(intent.data) ?: return
    val body = RemoteInput.getResultsFromIntent(intent)?.getCharSequence(NotificationReplies.TEXT)?.toString() ?: return
    val target = NotificationReplies.capture(context, actionTarget)
    if (body.isBlank()) {
      NotificationReplies.status(context, target, "Enter a reply before sending.")
      return
    }
    val pending = goAsync()
    // Keystore and durable enqueue never block Android's main thread.
    Thread {
      try {
        val id = UUID.randomUUID().toString()
        val payload = JSONObject().put("owner", target.owner).put("peer", target.peer)
          .put("token", target.token).put("action", target.action).put("body", body)
        NotificationReplies.save(context, id, payload)
        NotificationReplies.status(context, target, "Reply queued")
        NotificationReplies.enqueue(context, id).result.get(8, TimeUnit.SECONDS)
      } catch (_: Exception) {
        // Keep a saved spool for recovery if enqueue itself was interrupted.
        NotificationReplies.status(context, target, "Open chat to check your reply.", body)
      } finally { pending.finish() }
    }.start()
  }
}

/** Durable work survives a stopped process; only the spool id enters WorkManager Data. */
class NotificationReplyWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
  override fun doWork(): Result {
    val id = inputData.getString("reply_id") ?: return Result.failure()
    val file = NotificationReplies.file(applicationContext, id) ?: return Result.failure()
    if (!file.baseFile.exists() && !File(file.baseFile.path + ".bak").exists()) return Result.success()
    var knownTarget: ReplyTarget? = null
    var knownBody: String? = null
    return try {
      val raw = SecretBox.open(String(file.readFully(), Charsets.UTF_8))
      if (raw.isEmpty()) return Result.retry() // transient Keystore failure: retain text
      val reply = JSONObject(raw)
      val token = reply.getString("token")
      val target = ReplyTarget(reply.getString("owner"), reply.getString("peer"), token, reply.optString("action", token))
      knownTarget = target
      val body = reply.getString("body")
      knownBody = body
      val outcome = runReplyWork(
        attach = { ConnectionService.attach(applicationContext) },
        queue = { ConnectionService.queueNotificationReply(target.owner, id, target.peer, body) },
        check = { ConnectionService.checkNotificationReply(target.owner, id, target.peer, 45) },
        queued = { NotificationReplies.status(applicationContext, target, "Reply queued") },
      )
      finishReplyNotification(outcome,
        dismiss = { NotificationReplies.dismiss(applicationContext, target, body) },
        failed = { NotificationReplies.status(applicationContext, target,
          if (outcome == ReplyWorkOutcome.FAILED) "Could not send reply. Open chat to retry."
          else if (outcome == ReplyWorkOutcome.REJECTED) "Open chat to check your reply."
          else "Could not confirm sending. Retrying.", body) },
      )
      finishReplyHandoff(outcome) { file.delete() }
      when (outcome) {
        ReplyWorkOutcome.COMPLETE -> Result.success()
        ReplyWorkOutcome.REJECTED, ReplyWorkOutcome.FAILED -> Result.failure()
        else -> Result.retry()
      }
    } catch (_: Exception) {
      knownTarget?.let { target -> runCatching {
        NotificationReplies.status(applicationContext, target, "Could not confirm sending. Retrying.", knownBody)
      } }
      Result.retry()
    }
  }
}

internal data class ReplyTarget(val owner: String, val peer: String, val token: String, val action: String = token)

internal object NotificationReplies {
  const val ACTION = "dev.orgtree.hubchat.REPLY"
  const val TEXT = "hubchat_reply_text"
  const val TOKEN = "hubchat_reply_token"
  const val ACTION_TOKEN = "hubchat_reply_action_token"
  const val OWNER = "hubchat_reply_owner"
  private const val DIRECTORY = "notification-replies"

  fun target(uri: Uri?): ReplyTarget? {
    if (uri?.scheme != "hubchat-reply" || uri.authority != "reply") return null
    val parts = uri.pathSegments
    if (parts.size != 3 || parts[0].isBlank() || parts[1].isBlank() || !validReplyId(parts[2])) return null
    return ReplyTarget(parts[0], parts[1], parts[2])
  }

  fun action(context: Context, owner: String, peer: String, token: String): NotificationCompat.Action {
    val uri = Uri.Builder().scheme("hubchat-reply").authority("reply")
      .appendPath(owner).appendPath(peer).appendPath(token).build()
    // Identity lives in the filled base URI, not mutable extras; hash collisions
    // cannot send another chat's reply. Only this PendingIntent is mutable.
    val intent = Intent(context, NotificationReplyReceiver::class.java).setAction(ACTION).setData(uri)
    val flags = PendingIntent.FLAG_UPDATE_CURRENT or
      if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0
    val send = PendingIntent.getBroadcast(context, 0, intent, flags)
    return NotificationCompat.Action.Builder(R.drawable.ic_stat_hubchat, "Reply", send)
      .addRemoteInput(RemoteInput.Builder(TEXT).setLabel("Reply").build())
      .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_REPLY)
      .setShowsUserInterface(false).build()
  }

  @Synchronized fun capture(context: Context, action: ReplyTarget): ReplyTarget {
    val manager = context.getSystemService(NotificationManager::class.java)
    val current = manager.activeNotifications.firstOrNull { it.id == 2 && it.tag == action.peer }?.notification
      ?: return action // preserve a reply even if its notice was just dismissed
    val owner = current.extras.getString(OWNER)
    val episode = current.extras.getString(ACTION_TOKEN) ?: current.extras.getString(TOKEN)
    val version = captureReplyVersion(action.owner, owner, action.action, episode, current.extras.getString(TOKEN))
    return action.copy(token = version,
      action = if (owner == action.owner && !episode.isNullOrBlank()) episode else action.action)
  }

  private fun sameEpisode(target: ReplyTarget, notification: android.app.Notification): Boolean {
    val owner = notification.extras.getString(OWNER)
    val action = notification.extras.getString(ACTION_TOKEN) ?: notification.extras.getString(TOKEN)
    return (owner == target.owner && action == target.action) ||
      (owner == null && notification.extras.getString(TOKEN) == target.token)
  }

  @Synchronized fun status(context: Context, target: ReplyTarget, text: String, history: String? = null) {
    val manager = context.getSystemService(NotificationManager::class.java)
    val current = manager.activeNotifications.firstOrNull {
      it.id == 2 && it.tag == target.peer && sameEpisode(target, it.notification) &&
        (history != null || it.notification.extras.getString(TOKEN) == target.token)
    } ?: return
    val updated = NotificationCompat.Builder(context, current.notification)
      .setSubText(text).setOnlyAlertOnce(true).setSilent(true)
    // Even a failed send must release RemoteInput's sending spinner. Retain the
    // latest incoming content, version, and error; never mark another chat read.
    if (history != null) updated.setRemoteInputHistory(arrayOf<CharSequence>(history))
    manager.notify(target.peer, 2, updated.build())
  }

  @Synchronized fun dismiss(context: Context, target: ReplyTarget, body: String) {
    val manager = context.getSystemService(NotificationManager::class.java)
    val current = manager.activeNotifications.firstOrNull { it.id == 2 && it.tag == target.peer }?.notification
    // Re-notifying with history acknowledges RemoteInput before cancel. A bare
    // cancel can leave SystemUI's retained sending row even after NMS removes it.
    // A post-Send message stays visible but still gets its sending state ended.
    completeReplyVersion(target.token, current?.let { it.extras.getString(TOKEN) ?: "" },
      current != null && sameEpisode(target, current),
      acknowledge = {
        val updated = NotificationCompat.Builder(context, checkNotNull(current))
          .setRemoteInputHistory(arrayOf<CharSequence>(body)).setSubText(null)
          .setOnlyAlertOnce(true).setSilent(true).build()
        manager.notify(target.peer, 2, updated)
      },
      clear = { ConnectionService.clearMessage(target.peer) },
    )
  }

  fun file(context: Context, id: String): AtomicFile? {
    if (!validReplyId(id)) return null
    return AtomicFile(File(File(context.filesDir, DIRECTORY), "$id.reply"))
  }

  fun save(context: Context, id: String, payload: JSONObject) {
    val sealed = SecretBox.seal(payload.toString())
    check(sealed.isNotEmpty()) { "Could not save reply" }
    val file = checkNotNull(file(context, id))
    file.baseFile.parentFile!!.mkdirs()
    val stream = file.startWrite()
    try { stream.write(sealed.toByteArray(Charsets.UTF_8)); file.finishWrite(stream) }
    catch (e: Exception) { file.failWrite(stream); throw e }
  }

  fun enqueue(context: Context, id: String): androidx.work.Operation {
    val work = OneTimeWorkRequestBuilder<NotificationReplyWorker>().setInputData(workDataOf("reply_id" to id))
    // Older Android would require an extra foreground-service notification for
    // expedited work; ordinary work there still starts without opening the app.
    if (Build.VERSION.SDK_INT >= 31) work.setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
    return WorkManager.getInstance(context).enqueueUniqueWork("hubchat-reply-$id", ExistingWorkPolicy.KEEP, work.build())
  }

  fun recover(context: Context) {
    // Recover the narrow crash gap between the atomic spool write and WM enqueue.
    try {
      File(context.filesDir, DIRECTORY).listFiles()?.map {
        it.name.removeSuffix(".bak").removeSuffix(".reply")
      }?.distinct()?.filter { validReplyId(it) }?.forEach { id ->
        try { enqueue(context, id) } catch (_: Exception) { /* retain spool for next attach */ }
      }
    } catch (_: Exception) { /* recovery cannot prevent the core from attaching */ }
  }
}
