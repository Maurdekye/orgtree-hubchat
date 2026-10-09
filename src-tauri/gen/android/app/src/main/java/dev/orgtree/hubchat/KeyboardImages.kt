package dev.orgtree.hubchat

import android.os.Build
import android.util.Base64
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.webkit.WebView
import androidx.core.view.inputmethod.EditorInfoCompat
import androidx.core.view.inputmethod.InputConnectionCompat
import androidx.core.view.inputmethod.InputContentInfoCompat
import org.json.JSONObject

/**
 * Pictures from the keyboard (Gboard's GIFs and stickers, a picture on its
 * clipboard; user 2026-10-09): the WebView tells keyboards that it takes
 * pictures, and each one that arrives goes to the page, which puts it in the
 * message box as if it were pasted (src/lib/keyboardImages.ts). wry
 * generates RustWebView, the app's WebView, on every build, so the build
 * routes its input connection here (app/build.gradle.kts, patchRustWebView).
 */
object KeyboardImages {
  private val TYPES = arrayOf("image/png", "image/jpeg", "image/gif", "image/webp", "image/*")

  /** Bigger pictures are refused: they travel to the page as text. */
  private const val MAX_BYTES = 8 * 1024 * 1024

  @JvmStatic
  fun wrap(view: WebView, attrs: EditorInfo, ic: InputConnection?): InputConnection? {
    if (ic == null) return null
    EditorInfoCompat.setContentMimeTypes(attrs, TYPES)
    return InputConnectionCompat.createWrapper(ic, attrs) { info, flags, _ -> receive(view, info, flags) }
  }

  // On the keyboard's thread (the WebView's input thread): read the picture
  // here, hand it to the page on the main thread.
  private fun receive(view: WebView, info: InputContentInfoCompat, flags: Int): Boolean {
    if (Build.VERSION.SDK_INT >= 25 && (flags and InputConnectionCompat.INPUT_CONTENT_GRANT_READ_URI_PERMISSION) != 0) {
      try { info.requestPermission() } catch (e: Exception) { return false }
    }
    val d = info.description
    val type = (0 until d.mimeTypeCount).map { d.getMimeType(it) }.firstOrNull { it.startsWith("image/") && it != "image/*" } ?: "image/png"
    val bytes = try {
      view.context.contentResolver.openInputStream(info.contentUri)?.use { input ->
        val out = java.io.ByteArrayOutputStream()
        val buf = ByteArray(64 * 1024)
        var total = 0
        var n = input.read(buf)
        while (n >= 0 && total <= MAX_BYTES) {
          out.write(buf, 0, n)
          total += n
          n = input.read(buf)
        }
        if (total > MAX_BYTES) null else out.toByteArray()
      }
    } catch (e: Exception) {
      null
    } finally {
      info.releasePermission()
    }
    if (bytes == null || bytes.isEmpty()) return false
    val js = "window.__hubchatKeyboardImage&&window.__hubchatKeyboardImage(" +
      JSONObject.quote(Base64.encodeToString(bytes, Base64.NO_WRAP)) + "," + JSONObject.quote(type) + ")"
    view.post { view.evaluateJavascript(js, null) }
    return true
  }
}
