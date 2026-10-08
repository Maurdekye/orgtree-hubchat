package dev.orgtree.hubchat

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.View
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // Test builds (package *.test) can be driven over adb + DevTools, even
    // when built in the release configuration. The user's real app cannot.
    if (packageName.endsWith(".test")) android.webkit.WebView.setWebContentsDebuggingEnabled(true)
    super.onCreate(savedInstanceState)
    followKeyboard()
    if (Build.VERSION.SDK_INT >= 33 &&
      ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
      ActivityCompat.requestPermissions(this, arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
    }
    ConnectionService.start(this)
    takePeer(intent)
  }

  // The keyboard (user 23:24Z). Edge to edge, the window doesn't resize for
  // it, so the WebView's frame is padded by the keyboard's height: the page
  // shrinks to the space above it (header kept, composer on the keyboard).
  // While it's up, the page is told there is no navigation bar below it (the
  // keyboard covers that), so it adds no bottom inset of its own.
  private fun followKeyboard() {
    val frame = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(frame) { v, insets ->
      val ime = WindowInsetsCompat.Type.ime()
      val kb = if (insets.isVisible(ime)) insets.getInsets(ime).bottom else 0
      if (v.paddingBottom != kb) v.setPadding(0, 0, 0, kb)
      if (kb == 0) insets else {
        val b = WindowInsetsCompat.Builder(insets).setInsets(ime, Insets.NONE)
        for (t in intArrayOf(WindowInsetsCompat.Type.navigationBars(), WindowInsetsCompat.Type.tappableElement(),
            WindowInsetsCompat.Type.systemGestures(), WindowInsetsCompat.Type.mandatorySystemGestures())) {
          val i = insets.getInsets(t)
          b.setInsets(t, Insets.of(i.left, i.top, i.right, 0))
        }
        b.build()
      }
    }
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
