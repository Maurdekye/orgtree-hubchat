package dev.orgtree.hubchat

import android.content.Context
import android.os.Build
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import org.json.JSONArray
import org.json.JSONObject
import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.UnifiedPush
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage
import java.util.UUID
import java.util.concurrent.TimeUnit

/** Only sanitized settings go to the webview. The capability is sealed with
 * Android Keystore; WorkManager's database never holds endpoint/key material. */
object PushController {
  private const val PREFS = "hubchat-push"
  private const val WORK = "hubchat-push-work"
  // A cancelled WorkManager worker can still be inside native code. Keep
  // remote registration/removal ordered, and read settings after acquiring.
  private val workLock = Any()
  private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

  fun active(ctx: Context): Boolean {
    val p = prefs(ctx)
    return p.getBoolean("enabled", false) && p.getBoolean("active", false) &&
      UnifiedPush.getDistributors(ctx).contains(p.getString("distributor", ""))
  }

  fun state(ctx: Context): String {
    val p = prefs(ctx)
    return JSONObject().put("enabled", p.getBoolean("enabled", false))
      .put("active", active(ctx)).put("distributor", p.getString("distributor", ""))
      .put("distributors", JSONArray(UnifiedPush.getDistributors(ctx)))
      .put("status", p.getString("status", "Off")).toString()
  }

  fun set(ctx: Context, on: Boolean, distributor: String): String {
    if (on && !UnifiedPush.getDistributors(ctx).contains(distributor))
      return "Install a UnifiedPush distributor such as ntfy, then select it here."
    val oldInstance: String
    val instance = if (on) UUID.randomUUID().toString() else ""
    synchronized(this) {
      val p = prefs(ctx)
      oldInstance = p.getString("instance", "") ?: ""
      p.edit().putBoolean("enabled", on).putBoolean("active", false)
        .putString("instance", instance).putString("distributor", distributor)
        .putString("generation", UUID.randomUUID().toString()).remove("capability")
        .putString("status", if (on) "Waiting for the distributor…" else "Off")
        .putBoolean("cleanup", !on).commit()
    }
    if (oldInstance.isNotEmpty()) UnifiedPush.unregister(ctx, instance = oldInstance)
    if (on) {
      UnifiedPush.saveDistributor(ctx, distributor)
      UnifiedPush.register(ctx, instance = instance, messageForDistributor = "Hubchat message notifications")
    } else enqueue(ctx, "remove")
    modeChanged(ctx)
    return ""
  }

  /** App/boot entry: an unchanged endpoint is harmlessly re-registered. */
  fun startup(ctx: Context) {
    val p = prefs(ctx)
    if (!p.getBoolean("enabled", false)) {
      if (p.getBoolean("cleanup", false)) enqueue(ctx, "remove")
      return
    }
    val distributor = p.getString("distributor", "") ?: ""
    val instance = p.getString("instance", "") ?: ""
    if (instance.isEmpty() || !UnifiedPush.getDistributors(ctx).contains(distributor)) {
      unavailable(ctx, instance, "The selected distributor is not installed. Install ntfy or select another distributor.")
      return
    }
    UnifiedPush.saveDistributor(ctx, distributor)
    UnifiedPush.register(ctx, instance = instance, messageForDistributor = "Hubchat message notifications")
    if (p.contains("capability")) enqueue(ctx, "sync")
  }

  /** Adding/removing a hub invalidates the all-hubs registration proof. */
  fun refresh(ctx: Context) {
    synchronized(this) {
      val p = prefs(ctx)
      if (!p.getBoolean("enabled", false)) return
      p.edit().putBoolean("active", false).putString("generation", UUID.randomUUID().toString())
        .putString("status", "Registering with your hubs…").commit()
    }
    enqueue(ctx, "sync")
    modeChanged(ctx)
  }

  fun endpoint(ctx: Context, endpoint: PushEndpoint, instance: String) {
    val keys = endpoint.pubKeySet
    if (keys == null) {
      unavailable(ctx, instance, "This distributor does not provide encrypted Web Push. Select a current distributor such as ntfy.")
      return
    }
    val sealed = SecretBox.seal(JSONObject().put("endpoint", endpoint.url)
      .put("p256dh", keys.pubKey).put("auth", keys.auth).toString())
    if (sealed.isEmpty()) {
      unavailable(ctx, instance, "Android could not securely store the push registration.")
      return
    }
    synchronized(this) {
      val p = prefs(ctx)
      if (!p.getBoolean("enabled", false) || p.getString("instance", "") != instance) return
      // Keep an already working registration active on an identical callback.
      val previous = p.getString("capability", "")?.let { SecretBox.open(it) }
      val changed = previous != SecretBox.open(sealed)
      p.edit().putString("capability", sealed)
        .putString("generation", UUID.randomUUID().toString())
        .putBoolean("active", p.getBoolean("active", false) && !changed)
        .putString("status", "Registering with your hubs…").commit()
    }
    enqueue(ctx, "sync")
    modeChanged(ctx)
  }

  fun unavailable(ctx: Context, instance: String, message: String) {
    synchronized(this) {
      val p = prefs(ctx)
      if (!p.getBoolean("enabled", false) || p.getString("instance", "") != instance) return
      p.edit().putBoolean("active", false).remove("capability")
        .putString("generation", UUID.randomUUID().toString()).putString("status", message)
        .putBoolean("cleanup", true).commit()
    }
    enqueue(ctx, "remove")
    modeChanged(ctx)
  }

  fun wake(ctx: Context, message: PushMessage, instance: String) {
    val p = prefs(ctx)
    if (!p.getBoolean("enabled", false) || p.getString("instance", "") != instance) return
    if (message.decrypted && message.content.contentEquals("wake".toByteArray(Charsets.UTF_8)))
      enqueue(ctx, "wake")
  }

  private fun enqueue(ctx: Context, action: String) {
    val builder = OneTimeWorkRequestBuilder<PushWorker>()
      .setInputData(Data.Builder().putString("action", action).build())
      .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
      .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 15, TimeUnit.SECONDS)
    // Android 12+ can expedite a short wake check without a foreground
    // notification. Older versions use the normal scheduler: WorkManager's
    // expedited compatibility path would require another foreground service.
    if (Build.VERSION.SDK_INT >= 31) builder.setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
    val request = builder.build()
    // Replace rather than KEEP: a late wake needs another fetch, and turning
    // push off must not sit behind an offline registration's retry backoff.
    WorkManager.getInstance(ctx).enqueueUniqueWork(WORK, ExistingWorkPolicy.REPLACE, request)
  }

  private fun modeChanged(ctx: Context) {
    ConnectionService.start(ctx)
    ConnectionService.pushModeChanged()
  }

  fun work(ctx: Context, requested: String): Boolean = synchronized(workLock) { workLocked(ctx, requested) }

  private fun workLocked(ctx: Context, requested: String): Boolean {
    ConnectionService.attach(ctx)
    val generation: String
    val capability: String
    val action: String
    synchronized(this) {
      val p = prefs(ctx)
      generation = p.getString("generation", "") ?: ""
      val sealed = p.getString("capability", "") ?: ""
      capability = if (sealed.isEmpty()) "" else SecretBox.open(sealed)
      if (sealed.isNotEmpty() && capability.isEmpty()) {
        p.edit().putBoolean("active", false).remove("capability").putBoolean("cleanup", true)
          .putString("status", "Android could not open the saved push registration. Retry to register again.").commit()
      }
      action = if (!p.getBoolean("enabled", false) || capability.isEmpty()) "remove"
        else if (requested == "wake" && p.getBoolean("active", false)) "wake" else "sync"
    }
    val error = ConnectionService.pushWork(action, capability)
    synchronized(this) {
      val p = prefs(ctx)
      if (generation != p.getString("generation", "")) return true // newer work follows
      if (action == "sync") p.edit()
        .putBoolean("active", error.isEmpty())
        .putString("status", if (error.isEmpty()) "Push is on" else error).commit()
      if (action == "remove" && error.isEmpty()) p.edit().putBoolean("cleanup", false).commit()
    }
    modeChanged(ctx)
    return error.isEmpty()
  }
}

class PushWorker(ctx: Context, params: WorkerParameters) : Worker(ctx, params) {
  override fun doWork(): Result =
    if (PushController.work(applicationContext, inputData.getString("action") ?: "sync")) Result.success()
    else Result.retry()
}

class HubchatPushService : PushService() {
  override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) =
    PushController.endpoint(this, endpoint, instance)
  override fun onMessage(message: PushMessage, instance: String) = PushController.wake(this, message, instance)
  override fun onUnregistered(instance: String) =
    PushController.unavailable(this, instance, "The distributor removed this registration. Open Hubchat to register again.")
  override fun onRegistrationFailed(reason: FailedReason, instance: String) =
    PushController.unavailable(this, instance, when (reason) {
      FailedReason.VAPID_REQUIRED -> "This distributor requires VAPID, which Hubchat does not support yet. Select another distributor such as ntfy."
      FailedReason.NETWORK -> "The distributor is unreachable. Open Hubchat to retry."
      FailedReason.ACTION_REQUIRED -> "Open your distributor to finish setting it up, then retry in Hubchat."
      else -> "The distributor could not register Hubchat. Open it to check its settings."
    })
  override fun onTempUnavailable(instance: String) =
    PushController.unavailable(this, instance, "The distributor is temporarily unavailable. Open Hubchat to retry.")
}
