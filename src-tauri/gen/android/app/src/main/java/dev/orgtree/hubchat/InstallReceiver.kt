package dev.orgtree.hubchat

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.os.Build

/**
 * Android's answer to an in-app update (ConnectionService.installUpdate):
 * when it wants the person to confirm (always the first time), this opens
 * its window; otherwise it records the outcome for the update banner.
 */
class InstallReceiver : BroadcastReceiver() {
  override fun onReceive(ctx: Context, intent: Intent) {
    when (val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
      PackageInstaller.STATUS_PENDING_USER_ACTION -> {
        val confirm: Intent? = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
          else @Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_INTENT)
        ConnectionService.lastInstall = "confirm"
        confirm?.let { ctx.startActivity(it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
      }
      PackageInstaller.STATUS_SUCCESS -> ConnectionService.lastInstall = "done"
      else -> ConnectionService.lastInstall = "failed: " + (intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: "status $status")
    }
  }
}
