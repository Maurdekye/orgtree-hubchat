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

/**
 * Keeps Hubchat's hub connection alive in the background (design D6: a
 * persistent "connection" notification). The hub connection itself runs in
 * Rust (hubchat_lib); this service only owns the process lifetime and posts
 * notifications on the core's behalf. It starts the core itself, so a
 * START_STICKY restart without any activity still reconnects.
 */
class ConnectionService : Service() {
  companion object {
    const val CHANNEL_CONNECTION = "connection"
    const val CHANNEL_MESSAGES = "messages"
    private const val ONGOING_ID = 1

    @Volatile private var appContext: Context? = null

    init {
      System.loadLibrary("hubchat_lib")
    }

    /** Rust entry: start the hub core once per process (idempotent). */
    @JvmStatic external fun startCore(dataDir: String)

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
        .setSmallIcon(R.mipmap.ic_launcher)
        .setContentTitle(title)
        .setContentText(body)
        .setStyle(NotificationCompat.BigTextStyle().bigText(body))
        .setCategory(NotificationCompat.CATEGORY_MESSAGE)
        .setPriority(NotificationCompat.PRIORITY_HIGH)
        .setAutoCancel(true)
        .setContentIntent(open)
        .build()
      ctx.getSystemService(NotificationManager::class.java).notify(peer, 2, n)
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
      val ctx = appContext ?: return
      ctx.getSystemService(NotificationManager::class.java).notify(ONGOING_ID, ongoing(ctx, text))
    }

    fun start(ctx: Context) {
      val i = Intent(ctx, ConnectionService::class.java)
      if (Build.VERSION.SDK_INT >= 26) ctx.startForegroundService(i) else ctx.startService(i)
    }

    private fun ongoing(ctx: Context, text: String): Notification {
      val open = PendingIntent.getActivity(
        ctx, 0, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
      return NotificationCompat.Builder(ctx, CHANNEL_CONNECTION)
        .setSmallIcon(R.mipmap.ic_launcher)
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
    ServiceCompat.startForeground(this, ONGOING_ID, ongoing(this, "Connecting…"), type)
    startCore(filesDir.absolutePath)
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int = START_STICKY

  override fun onBind(intent: Intent?): IBinder? = null
}
