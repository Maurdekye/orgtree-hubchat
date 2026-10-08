package dev.orgtree.hubchat

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    if (Build.VERSION.SDK_INT >= 33 &&
      ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
      ActivityCompat.requestPermissions(this, arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
    }
    ConnectionService.start(this)
    takePeer(intent)
  }

  // A tapped message notification names its chat; the UI picks it up.
  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    takePeer(intent)
  }

  private fun takePeer(intent: Intent?) {
    intent?.getStringExtra("peer")?.let { ConnectionService.pendingPeer = it }
    // hubchat://link?... from a camera or QR app (user 19:12Z)
    intent?.data?.takeIf { it.scheme == "hubchat" }?.let { ConnectionService.pendingLink = it.toString() }
  }
}
