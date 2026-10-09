package dev.orgtree.hubchat

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.View
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  companion object {
    /** Is Hubchat's window in front? Android lets an app open a window only then. */
    @Volatile var inFront = false
  }

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

  override fun onResume() {
    super.onResume()
    inFront = true
    // an update that finished downloading while Hubchat was in the
    // background: Android's confirmation opens now (InstallReceiver)
    ConnectionService.pendingConfirm?.let {
      ConnectionService.pendingConfirm = null
      startActivity(it)
    }
  }

  override fun onPause() {
    inFront = false
    super.onPause()
  }

  // Back: the WebView's own history first, then the page, then Android's
  // default (Hubchat goes to the background). A screen the page opened before
  // the first tap (the chat reopened at start, a tapped notification's) has
  // the entry under it skipped by the WebView's back (Chromium's history
  // intervention), so canGoBack() says no and Back would leave Hubchat; the
  // page's own history.back() still reaches it (measured on the emulator,
  // hubchat-opus 2026-10-09 20:15Z). Tauri queues its own Back handler before
  // the WebView exists, so this one, added after, runs first.
  override fun onWebViewCreate(webView: android.webkit.WebView) {
    super.onWebViewCreate(webView)
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        if (webView.canGoBack()) {
          webView.goBack()
          return
        }
        webView.evaluateJavascript("!!(window.__hcBack && window.__hcBack())") { handled ->
          if (handled != "true") {
            isEnabled = false
            onBackPressedDispatcher.onBackPressed()
            isEnabled = true
          }
        }
      }
    })
  }

  // A tapped message notification names its chat; the UI picks it up.
  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    takePeer(intent)
    // Already in front (singleTask), the page gets no focus or visibility
    // change, so a link or a tapped chat would wait until Hubchat is hidden
    // and shown again: tell it now (hubchat-opus 18:52Z)
    if (intent.data?.scheme == "hubchat" || intent.hasExtra("peer")) {
      findWebView(window.decorView)?.evaluateJavascript("window.dispatchEvent(new Event('hc-pending'))", null)
    }
  }

  private fun findWebView(v: View): android.webkit.WebView? {
    if (v is android.webkit.WebView) return v
    if (v is android.view.ViewGroup) {
      for (i in 0 until v.childCount) findWebView(v.getChildAt(i))?.let { return it }
    }
    return null
  }

  private fun takePeer(intent: Intent?) {
    intent?.getStringExtra("peer")?.let { ConnectionService.pendingPeer = it }
    // hubchat://link?... from a camera or QR app (user 19:12Z)
    intent?.data?.takeIf { it.scheme == "hubchat" }?.let { ConnectionService.pendingLink = it.toString() }
  }
}
