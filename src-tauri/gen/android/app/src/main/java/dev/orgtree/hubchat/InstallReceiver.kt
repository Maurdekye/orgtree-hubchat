package dev.orgtree.hubchat

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.os.Build

/**
 * Android's answer to an in-app update (ConnectionService.installUpdate):
 * when it wants the person to confirm (always the first time), this opens
 * its window; otherwise it records the outcome, in plain words, for the
 * update banner.
 */
class InstallReceiver : BroadcastReceiver() {
  override fun onReceive(ctx: Context, intent: Intent) {
    when (val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
      PackageInstaller.STATUS_PENDING_USER_ACTION -> {
        val confirm: Intent? = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
          else @Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_INTENT)
        ConnectionService.lastInstall = "confirm"
        // Android opens an app's windows only while the app is in front: if
        // the person left Hubchat during the download, it opens when they
        // come back (MainActivity.onResume)
        confirm?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)?.let {
          if (MainActivity.inFront) ctx.startActivity(it) else ConnectionService.pendingConfirm = it
        }
      }
      PackageInstaller.STATUS_SUCCESS -> ConnectionService.lastInstall = "done"
      else -> ConnectionService.lastInstall = "failed: " + when (status) {
        PackageInstaller.STATUS_FAILURE_ABORTED -> "it was cancelled"
        PackageInstaller.STATUS_FAILURE_BLOCKED -> "Android blocked it"
        PackageInstaller.STATUS_FAILURE_CONFLICT -> "it isn't signed like the Hubchat on this phone"
        PackageInstaller.STATUS_FAILURE_INCOMPATIBLE -> "it doesn't run on this phone"
        PackageInstaller.STATUS_FAILURE_INVALID -> "the download is damaged"
        PackageInstaller.STATUS_FAILURE_STORAGE -> "there isn't enough space on this phone"
        else -> intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: "Android's installer stopped (status $status)"
      }
    }
  }
}
