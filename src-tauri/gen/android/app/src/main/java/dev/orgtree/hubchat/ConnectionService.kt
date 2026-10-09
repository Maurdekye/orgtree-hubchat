package dev.orgtree.hubchat

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.provider.OpenableColumns
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit

/**
 * Keeps Hubchat's hub connection alive in the background (design D6: a
 * persistent "connection" notification, the default). The hub connection
 * itself runs in Rust (hubchat_lib); this service only owns the process
 * lifetime and posts notifications on the core's behalf. It starts the core
 * itself, so a START_STICKY restart without any activity still reconnects.
 * With "Stay connected" off there is no service: CheckWorker runs a check
 * about every 15 minutes instead.
 */
class ConnectionService : Service() {
  companion object {
    const val CHANNEL_CONNECTION = "connection"
    const val CHANNEL_MESSAGES = "messages"
    private const val ONGOING_ID = 1
    private const val PREFS = "hubchat"
    private const val KEY_STAY = "stay_connected"
    private const val WORK = "hubchat-check"

    /** The service runs (the ongoing notification belongs to it). */
    @Volatile private var running = false
    /** The core's latest status line, for an ongoing notification posted later. */
    @Volatile private var lastStatus: String? = null

    @Volatile private var appContext: Context? = null

    init {
      System.loadLibrary("hubchat_lib")
    }

    /** Rust entry: start the hub core once per process (idempotent). */
    @JvmStatic external fun startCore(dataDir: String)

    /** Rust entry: one check (design D6); blocks, so never on the main thread. */
    @JvmStatic external fun checkNow(timeoutSecs: Int): Boolean

    /** What the core needs before it runs without the service. */
    fun attach(ctx: Context) {
      appContext = ctx.applicationContext
      channels(ctx)
      startCore(ctx.filesDir.absolutePath)
    }

    fun stayConnected(ctx: Context): Boolean =
      ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(KEY_STAY, true)

    /** Called from Rust: "1" when Hubchat stays connected (the default). */
    @JvmStatic
    fun isStayConnected(): String {
      val ctx = appContext ?: return "1"
      return if (stayConnected(ctx)) "1" else "0"
    }

    /** Called from Rust when the user flips Settings › Notifications › Stay connected. */
    @JvmStatic
    fun setStayConnected(on: String) {
      val ctx = appContext ?: return
      ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(KEY_STAY, on == "1").apply()
      start(ctx)
    }

    private fun schedule(ctx: Context) {
      val req = PeriodicWorkRequestBuilder<CheckWorker>(15, TimeUnit.MINUTES)
        .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
        .build()
      WorkManager.getInstance(ctx).enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.KEEP, req)
    }

    /** Message notifications form a group of their own, with its summary
     *  (user 2026-10-09 06:19Z): Android 16 otherwise folds a message into
     *  one group with the "connected" notification, and the first tap on it
     *  only unfolds that group. With one chat waiting, Android shows its
     *  notification alone, so one tap opens it. */
    private const val GROUP_MESSAGES = "dev.orgtree.hubchat.MESSAGES"
    private const val MESSAGE_ID = 2
    private const val SUMMARY_TAG = "messages-summary"

    /** Called from Rust (any thread) when a message arrives. */
    @JvmStatic
    fun notifyMessage(title: String, body: String, peer: String) {
      val ctx = appContext ?: return
      // One notification per chat (replaced as new messages arrive); tapping
      // it opens that chat.
      val intent = Intent(ctx, MainActivity::class.java)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        .putExtra("peer", peer)
      val open = PendingIntent.getActivity(
        ctx, peer.hashCode(), intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
      val n = NotificationCompat.Builder(ctx, CHANNEL_MESSAGES)
        .setSmallIcon(R.drawable.ic_stat_hubchat)
        .setContentTitle(title)
        .setContentText(body)
        .setStyle(NotificationCompat.BigTextStyle().bigText(body))
        .setCategory(NotificationCompat.CATEGORY_MESSAGE)
        .setPriority(NotificationCompat.PRIORITY_HIGH)
        .setGroup(GROUP_MESSAGES)
        .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
        .setAutoCancel(true)
        .setContentIntent(open)
        .build()
      val nm = ctx.getSystemService(NotificationManager::class.java)
      nm.notify(peer, MESSAGE_ID, n)
      summary(ctx, nm)
    }

    /** The message group's summary, shown by Android when two or more chats
     *  are waiting; silent (the chats' own notifications alert). Tapping it
     *  opens Hubchat. */
    private fun summary(ctx: Context, nm: NotificationManager) {
      val open = PendingIntent.getActivity(
        ctx, 0, Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
      val s = NotificationCompat.Builder(ctx, CHANNEL_MESSAGES)
        .setSmallIcon(R.drawable.ic_stat_hubchat)
        .setContentTitle("New messages")
        .setCategory(NotificationCompat.CATEGORY_MESSAGE)
        .setGroup(GROUP_MESSAGES)
        .setGroupSummary(true)
        .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
        .setSilent(true)
        .setAutoCancel(true)
        .setContentIntent(open)
        .build()
      nm.notify(SUMMARY_TAG, MESSAGE_ID, s)
    }

    /** Called from Rust when a chat has nothing unread any more (read here
     *  or on another device): its message notification goes, and the
     *  group's summary with the last of them. */
    @JvmStatic
    fun clearMessage(peer: String) {
      val ctx = appContext ?: return
      val nm = ctx.getSystemService(NotificationManager::class.java)
      nm.cancel(peer, MESSAGE_ID)
      val left = nm.activeNotifications.any { it.id == MESSAGE_ID && it.tag != null && it.tag != SUMMARY_TAG && it.tag != peer }
      if (!left) nm.cancel(SUMMARY_TAG, MESSAGE_ID)
    }

    /** Called from Rust: open a content:// URI read-only; returns a detached fd or -1. */
    @JvmStatic
    fun openFd(uri: String): Int {
      val ctx = appContext ?: return -1
      return try {
        ctx.contentResolver.openFileDescriptor(Uri.parse(uri), "r")?.detachFd() ?: -1
      } catch (e: Exception) {
        -1
      }
    }

    /** The chat a tapped notification asked for; the UI takes it once. */
    @Volatile var pendingPeer: String = ""

    /** A hubchat:// link the system opened us with; the UI takes it once. */
    @Volatile var pendingLink: String = ""

    @JvmStatic
    fun takePendingLink(): String {
      val l = pendingLink
      pendingLink = ""
      return l
    }

    @JvmStatic
    fun takePendingPeer(): String {
      val p = pendingPeer
      pendingPeer = ""
      return p
    }

    /**
     * Called from Rust after a download: copy the file into the shared
     * Downloads collection (Android 10+) so other apps and the Files app see
     * it. Returns the new content:// URI, or "" to keep the private copy.
     */
    @JvmStatic
    fun publishDownload(path: String, name: String): String {
      val ctx = appContext ?: return ""
      if (Build.VERSION.SDK_INT < 29) return ""
      return try {
        val values = android.content.ContentValues().apply {
          put(android.provider.MediaStore.Downloads.DISPLAY_NAME, name)
          put(android.provider.MediaStore.Downloads.IS_PENDING, 1)
        }
        val coll = android.provider.MediaStore.Downloads.getContentUri(android.provider.MediaStore.VOLUME_EXTERNAL_PRIMARY)
        val uri = ctx.contentResolver.insert(coll, values) ?: return ""
        ctx.contentResolver.openOutputStream(uri)?.use { out ->
          java.io.File(path).inputStream().use { it.copyTo(out, 256 * 1024) }
        }
        values.clear()
        values.put(android.provider.MediaStore.Downloads.IS_PENDING, 0)
        ctx.contentResolver.update(uri, values, null, null)
        java.io.File(path).delete()
        uri.toString()
      } catch (e: Exception) {
        ""
      }
    }

    /** Called from Rust: the phone's own name (Settings › About phone ›
     *  Device name), else its maker and model (user 00:16Z). */
    @JvmStatic
    fun deviceName(): String {
      val named = appContext?.let {
        try { android.provider.Settings.Global.getString(it.contentResolver, android.provider.Settings.Global.DEVICE_NAME) } catch (e: Exception) { null }
      }?.trim()
      if (!named.isNullOrEmpty()) return named
      val maker = Build.MANUFACTURER.replaceFirstChar { it.uppercase() }
      return if (Build.MODEL.startsWith(Build.MANUFACTURER, ignoreCase = true)) Build.MODEL else "$maker ${Build.MODEL}"
    }

    /** Called from Rust: the display name of a content:// URI, or "". */
    @JvmStatic
    fun displayName(uri: String): String {
      val ctx = appContext ?: return ""
      return try {
        ctx.contentResolver.query(Uri.parse(uri), arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
          if (it.moveToFirst()) it.getString(0) ?: "" else ""
        } ?: ""
      } catch (e: Exception) {
        ""
      }
    }

    /** Called from Rust to update the ongoing notification's text. */
    @JvmStatic
    fun setStatus(text: String) {
      lastStatus = text
      if (!running) return // no service, no ongoing notification
      val ctx = appContext ?: return
      ctx.getSystemService(NotificationManager::class.java).notify(ONGOING_ID, ongoing(ctx, text))
    }

    /** App start and a change of mode: the service (stay connected), or
     *  the core without it and the periodic check. */
    fun start(ctx: Context) {
      val i = Intent(ctx, ConnectionService::class.java)
      if (stayConnected(ctx)) {
        WorkManager.getInstance(ctx).cancelUniqueWork(WORK)
        if (Build.VERSION.SDK_INT >= 26) ctx.startForegroundService(i) else ctx.startService(i)
      } else {
        ctx.stopService(i)
        attach(ctx)
        schedule(ctx)
      }
    }

    private fun ongoing(ctx: Context, text: String): Notification {
      val open = PendingIntent.getActivity(
        ctx, 0, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
      return NotificationCompat.Builder(ctx, CHANNEL_CONNECTION)
        .setSmallIcon(R.drawable.ic_stat_hubchat)
        .setContentTitle("Hubchat")
        .setContentText(text)
        .setOngoing(true)
        .setSilent(true)
        .setPriority(NotificationCompat.PRIORITY_MIN)
        .setContentIntent(open)
        .build()
    }

    private fun channels(ctx: Context) {
      if (Build.VERSION.SDK_INT < 26) return
      val nm = ctx.getSystemService(NotificationManager::class.java)
      nm.createNotificationChannel(NotificationChannel(
        CHANNEL_CONNECTION, "Background connection", NotificationManager.IMPORTANCE_MIN).apply {
        description = "Keeps Hubchat connected to your hubs so messages arrive right away"
      })
      nm.createNotificationChannel(NotificationChannel(
        CHANNEL_MESSAGES, "Messages", NotificationManager.IMPORTANCE_HIGH))
    }
  }

  override fun onCreate() {
    super.onCreate()
    appContext = applicationContext
    channels(this)
    val type = if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING else 0
    ServiceCompat.startForeground(this, ONGOING_ID, ongoing(this, lastStatus ?: "Connecting…"), type)
    running = true
    startCore(filesDir.absolutePath)
  }

  override fun onDestroy() {
    running = false
    super.onDestroy()
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int = START_STICKY

  override fun onBind(intent: Intent?): IBinder? = null
}
