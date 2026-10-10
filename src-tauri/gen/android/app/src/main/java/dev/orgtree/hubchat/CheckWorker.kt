package dev.orgtree.hubchat

import android.content.Context
import androidx.work.Worker
import androidx.work.WorkerParameters

/**
 * The periodic check when "Stay connected" is off (design D6), and the
 * backup while push is on (user 2026-10-10 17:42Z): about every 15 minutes
 * Android runs this; it starts the core if the process was gone, lets it
 * fetch what arrived and send what waits (notifications included), then
 * lets Android put the process to sleep again.
 */
class CheckWorker(ctx: Context, params: WorkerParameters) : Worker(ctx, params) {
  override fun doWork(): Result {
    // A push app that Android paused, or that was removed, would otherwise
    // hold messages back until Hubchat is opened; active() also notices a
    // removed one and falls back to the saved connection mode. Without push
    // this covers Android refusing to restart the foreground service while
    // the app was hidden.
    PushController.active(applicationContext)
    ConnectionService.attach(applicationContext)
    ConnectionService.checkNow(45)
    return Result.success()
  }
}
