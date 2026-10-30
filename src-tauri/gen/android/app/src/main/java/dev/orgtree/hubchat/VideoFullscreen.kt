package dev.orgtree.hubchat

import android.graphics.Bitmap
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.view.View
import android.view.ViewGroup
import android.webkit.ConsoleMessage
import android.webkit.GeolocationPermissions
import android.webkit.JsPromptResult
import android.webkit.JsResult
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

/**
 * A video's fullscreen (Hubchat 1.2.7, inline video; docs/inline-media.md). wry's chrome client refuses it: its
 * onShowCustomView hides the view at once, so a video's fullscreen button would bounce back. This client shows that
 * view over the activity, system bars hidden, until the page leaves fullscreen or Back is pressed. Every other call
 * wry's client overrides goes to that client unchanged, by delegation (tools/check-fullscreen-client.mjs checks none
 * is missed, against the wry that Cargo.lock pins), so the file chooser and permission prompts work exactly as before.
 * Android 8.0+ only: WebView.getWebChromeClient() is how wry's client is found; on 7.x the refusal stays.
 */
class FullscreenChromeClient private constructor(
  private val inner: WebChromeClient,
  private val activity: ComponentActivity,
) : WebChromeClient() {
  private var shown: View? = null
  private var hidden: CustomViewCallback? = null

  /** Back leaves fullscreen first: added after MainActivity's own Back, so it is asked before it. */
  private val back = object : OnBackPressedCallback(false) {
    override fun handleOnBackPressed() = onHideCustomView()
  }

  init {
    activity.onBackPressedDispatcher.addCallback(activity, back)
  }

  companion object {
    /** Wraps wry's client, once it is set: wry sets it right after onWebViewCreate, in the same main-thread step. */
    fun install(activity: ComponentActivity, webView: WebView) {
      if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
      val inner = webView.webChromeClient ?: return
      if (inner is FullscreenChromeClient) return
      webView.webChromeClient = FullscreenChromeClient(inner, activity)
    }
  }

  // ---- fullscreen: this client's own

  override fun onShowCustomView(view: View, callback: CustomViewCallback) {
    if (shown != null) {
      callback.onCustomViewHidden()
      return
    }
    val decor = activity.window.decorView as ViewGroup
    view.setBackgroundColor(Color.BLACK)
    decor.addView(view, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
    shown = view
    hidden = callback
    back.isEnabled = true
    WindowCompat.getInsetsController(activity.window, decor).apply {
      systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
      hide(WindowInsetsCompat.Type.systemBars())
    }
  }

  override fun onHideCustomView() {
    val view = shown ?: return
    val decor = activity.window.decorView as ViewGroup
    decor.removeView(view)
    WindowCompat.getInsetsController(activity.window, decor).show(WindowInsetsCompat.Type.systemBars())
    shown = null
    back.isEnabled = false
    val ended = hidden
    hidden = null
    ended?.onCustomViewHidden()
  }

  // ---- everything else wry's client overrides: that client, unchanged

  override fun onPermissionRequest(request: PermissionRequest) = inner.onPermissionRequest(request)

  override fun getDefaultVideoPoster(): Bitmap? = inner.defaultVideoPoster

  override fun onJsAlert(view: WebView, url: String, message: String, result: JsResult): Boolean =
    inner.onJsAlert(view, url, message, result)

  override fun onJsConfirm(view: WebView, url: String, message: String, result: JsResult): Boolean =
    inner.onJsConfirm(view, url, message, result)

  override fun onJsPrompt(view: WebView, url: String, message: String, defaultValue: String, result: JsPromptResult): Boolean =
    inner.onJsPrompt(view, url, message, defaultValue, result)

  override fun onGeolocationPermissionsShowPrompt(origin: String, callback: GeolocationPermissions.Callback) =
    inner.onGeolocationPermissionsShowPrompt(origin, callback)

  override fun onShowFileChooser(webView: WebView, filePathCallback: ValueCallback<Array<Uri?>?>, fileChooserParams: FileChooserParams): Boolean =
    inner.onShowFileChooser(webView, filePathCallback, fileChooserParams)

  override fun onConsoleMessage(consoleMessage: ConsoleMessage): Boolean = inner.onConsoleMessage(consoleMessage)

  override fun onReceivedTitle(view: WebView, title: String) = inner.onReceivedTitle(view, title)
}
