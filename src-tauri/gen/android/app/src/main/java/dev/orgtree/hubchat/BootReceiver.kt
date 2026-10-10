package dev.orgtree.hubchat

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * After a phone restart or an app update (user 23:05Z): with "Stay connected"
 * on, the background connection starts by itself, without opening Hubchat.
 * The periodic check needs nothing here: WorkManager keeps its own schedule.
 */
class BootReceiver : BroadcastReceiver() {
  override fun onReceive(ctx: Context, intent: Intent) {
    when (intent.action) {
      Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED -> {
        PushController.startup(ctx)
        if (ConnectionService.stayConnected(ctx) || PushController.active(ctx)) ConnectionService.start(ctx)
      }
    }
  }
}
