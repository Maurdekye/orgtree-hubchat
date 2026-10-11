package dev.orgtree.hubchat

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat

/** Temporary coverage for user-started files when the connection service is absent. */
class TransferService : Service() {
  companion object {
    private const val CHANNEL = "files"
    private const val ID = 4
    private val main = Handler(Looper.getMainLooper())
    private val policy = FileServicePolicy()
    @Volatile var running = false
      private set
    @Volatile private var hint = ""
    private var stopping = false
    private var reason = ""
    private var lastState = ""

    // Always post: Rust calls this under its short lifetime lock.
    fun work(ctx: Context, wanted: Boolean) { main.post {
      policy.wanted = wanted
      reconcile(ctx.applicationContext)
    } }
    fun foreground(ctx: Context) { main.post {
      policy.foreground()
      reason = ""
      reconcile(ctx.applicationContext)
    } }
    fun coverageChanged(ctx: Context) { main.post { reconcile(ctx.applicationContext) } }
    fun backgroundHint(): String = hint

    private fun report() {
      hint = if (!policy.wanted || ConnectionService.isRunning() || running) ""
        else if (reason.isNotEmpty()) reason
        else if (MainActivity.inFront) "Keep Hubchat open until files can continue in the background."
        else "Files are paused. Open Hubchat to resume them."
      val state = "${ConnectionService.isRunning()}:$running:${policy.wanted}:$hint"
      if (state != lastState) {
        lastState = state
        ConnectionService.fileServiceChanged()
      }
    }
    private fun reconcile(ctx: Context) {
      if (running && (!policy.wanted || ConnectionService.isRunning())) {
        stopping = true
        ctx.stopService(Intent(ctx, TransferService::class.java))
      } else if (policy.shouldStart(MainActivity.inFront, ConnectionService.isRunning(), running)) {
        policy.starting = true
        try {
          val intent = Intent(ctx, TransferService::class.java)
          if (Build.VERSION.SDK_INT >= 26) ctx.startForegroundService(intent) else ctx.startService(intent)
        } catch (_: IllegalStateException) { refused() }
          catch (_: SecurityException) { refused() }
      }
      report()
    }
    private fun refused() {
      policy.refused()
      reason = "Android could not keep files running in the background. Keep Hubchat open to continue."
    }
  }

  override fun onCreate() {
    super.onCreate()
    val nm = getSystemService(NotificationManager::class.java)
    if (Build.VERSION.SDK_INT >= 26) {
      nm.createNotificationChannel(NotificationChannel(CHANNEL, "Sending and saving files", NotificationManager.IMPORTANCE_LOW).apply {
        setShowBadge(false)
        description = "Keeps files moving after you leave Hubchat"
      })
    }
    val open = PendingIntent.getActivity(this, ID, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
    val notice = NotificationCompat.Builder(this, CHANNEL)
      .setSmallIcon(R.drawable.ic_stat_hubchat).setContentTitle("Hubchat files")
      .setContentText("Sending or saving files?").setProgress(0, 0, true)
      .setOngoing(true).setSilent(true).setOnlyAlertOnce(true).setContentIntent(open).build()
    try {
      // Fulfil a pending start before inspecting whether the work is still wanted.
      ServiceCompat.startForeground(this, ID, notice,
        if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC else 0)
      running = true
      stopping = false
      policy.starting = false
      report()
      reconcile(this)
    } catch (_: IllegalStateException) { refused(); report(); stopSelf() }
      catch (_: SecurityException) { refused(); report(); stopSelf() }
  }
  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int = START_NOT_STICKY
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onTimeout(startId: Int, fgsType: Int) {
    running = false
    policy.refused()
    reason = "Android paused background files after its time limit. Keep Hubchat open to continue."
    report()
    stopForeground(STOP_FOREGROUND_REMOVE)
    stopSelf()
  }
  override fun onDestroy() {
    val expected = stopping
    stopping = false
    running = false
    policy.starting = false
    if (!expected && policy.wanted) {
      policy.blocked = true
      if (reason.isEmpty()) reason = "Android stopped background files. Keep Hubchat open to continue."
    }
    report()
    // A new admission may have arrived while an unneeded service was stopping.
    if (expected) reconcile(applicationContext)
    super.onDestroy()
  }
}
