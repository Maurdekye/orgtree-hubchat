package dev.orgtree.hubchat

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import java.io.File
import java.io.FileNotFoundException
import java.util.Locale

/** Opens the existing saved file, without exporting another copy or installing
 * anything itself. Android owns viewer choice and APK installation consent. */
object DownloadOpener {
    internal fun viewIntent(context: Context, source: String, name: String): Intent {
        val uri = if (source.startsWith("content://")) Uri.parse(source) else {
            val file = File(source)
            if (!file.isFile) throw FileNotFoundException()
            FileProvider.getUriForFile(context, context.packageName + ".fileprovider", file)
        }
        val extension = name.substringAfterLast('.', "").lowercase(Locale.ROOT)
        val mime = if (extension == "apk") "application/vnd.android.package-archive" else {
            MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)
                ?: context.contentResolver.getType(uri)
                ?: "application/octet-stream"
        }
        return Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, mime)
            clipData = ClipData.newRawUri("Downloaded file", uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        }
    }

    fun open(context: Context, source: String, name: String): String = try {
        // Do not preflight resolveActivity: Android package visibility can hide
        // a handler that startActivity is allowed to launch.
        context.startActivity(viewIntent(context, source, name))
        ""
    } catch (_: ActivityNotFoundException) {
        "No app can open this file."
    } catch (_: FileNotFoundException) {
        "This file is no longer available. Download it again."
    } catch (_: SecurityException) {
        "Android could not access this file. Try downloading it again."
    } catch (_: Exception) {
        "Could not open this file. Try again."
    }
}
