package dev.orgtree.hubchat

import android.content.Context
import androidx.work.Worker
import androidx.work.WorkerParameters

/**
 * The periodic check when "Stay connected" is off (design D6): about every
 * 15 minutes Android runs this; it starts the core if the process was gone,
 * lets it fetch what arrived and send what waits (notifications included),
 * then lets Android put the process to sleep again.
 */
class CheckWorker(ctx: Context, params: WorkerParameters) : Worker(ctx, params) {
  override fun doWork(): Result {
    // switched back to "stay connected": the service does the work
    if (ConnectionService.stayConnected(applicationContext)) return Result.success()
    ConnectionService.attach(applicationContext)
    ConnectionService.checkNow(45)
    return Result.success()
  }
}
