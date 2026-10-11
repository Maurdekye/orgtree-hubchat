package dev.orgtree.hubchat

import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.ParcelFileDescriptor
import android.provider.MediaStore
import android.util.AtomicFile
import java.io.File
import java.security.MessageDigest

/** One core writer owns each key. No activity or UI command owns this copy. */
object DownloadPublisher {
    private class SaveProblem(message: String) : java.io.IOException(message)
    fun failure(error: Exception): String {
        if (error is SaveProblem) return error.message ?: "Could not save the file; retry"
        var cause: Throwable? = error
        while (cause != null) {
            if (cause is android.system.ErrnoException && cause.errno == android.system.OsConstants.ENOSPC)
                return "Not enough space to save the file. Free some space and retry."
            if (cause is SecurityException)
                return "Hubchat cannot access Downloads. Check its file permissions and retry."
            cause = cause.cause
        }
        return "Could not save to Downloads. Check free space and retry."
    }
    private fun receipt(context: Context, key: String): AtomicFile {
        val journalDir = File(context.filesDir, "download-exports")
        check(journalDir.isDirectory || journalDir.mkdirs()) { "Cannot create download receipt" }
        val journalName = MessageDigest.getInstance("SHA-256").digest(key.toByteArray())
            .joinToString("") { "%02x".format(it) }
        return AtomicFile(File(journalDir, "$journalName.uri"))
    }
    fun committed(context: Context, key: String) { receipt(context, key).delete() }
    fun publish(context: Context, key: String, path: String, name: String): String {
        if (Build.VERSION.SDK_INT < 29) return path
        val source = File(path)
        val resolver = context.contentResolver
        val journal = receipt(context, key)
        val previous = if (journal.baseFile.exists()) {
            journal.openRead().bufferedReader().use { it.readText() }.let(Uri::parse)
        } else null
        @Suppress("DEPRECATION")
        fun pendingUri(uri: Uri): Uri = MediaStore.setIncludePending(uri)
        fun pending(uri: Uri): Boolean? {
            val projection = arrayOf(MediaStore.Downloads.IS_PENDING)
            val cursor = if (Build.VERSION.SDK_INT >= 30) {
                resolver.query(uri, projection, android.os.Bundle().apply {
                    putInt(MediaStore.QUERY_ARG_MATCH_PENDING, MediaStore.MATCH_INCLUDE)
                }, null)
            } else resolver.query(pendingUri(uri), projection, null, null, null)
            return cursor?.use { if (it.moveToFirst()) it.getInt(0) != 0 else null }
        }
        // The URI was journalled before any bytes became visible. Once the
        // pending bit is clear, the prior copy finished, even if Rust died
        // before committing its pointer. Never overwrite the user's saved file.
        val oldPending = previous?.let(::pending)
        if (previous != null && oldPending == false) return previous.toString()
        val uri = if (previous != null && oldPending == true) previous else {
            if (!source.isFile) throw SaveProblem("The private download is missing. Download it again.")
            val values = ContentValues().apply {
                put(MediaStore.Downloads.DISPLAY_NAME, name)
                put(MediaStore.Downloads.IS_PENDING, 1)
            }
            val collection = MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
            val created = checkNotNull(resolver.insert(collection, values)) { "Cannot create download" }
            var output: java.io.FileOutputStream? = null
            try {
                output = journal.startWrite()
                output.write(created.toString().toByteArray())
                journal.finishWrite(output)
            } catch (e: Exception) {
                journal.failWrite(output)
                resolver.delete(pendingUri(created), null, null)
                throw e
            }
            created
        }
        // Keep the source until the core commits the URI. A crash during this
        // copy leaves a pending URI and replay truncates that same destination.
        val descriptor = checkNotNull(resolver.openFileDescriptor(uri, "rwt")) { "Cannot write download" }
        ParcelFileDescriptor.AutoCloseOutputStream(descriptor).use { output ->
            val digest = MessageDigest.getInstance("SHA-256")
            source.inputStream().use { input ->
                val buffer = ByteArray(256 * 1024)
                while (true) {
                    val n = input.read(buffer)
                    if (n < 0) break
                    output.write(buffer, 0, n)
                    digest.update(buffer, 0, n)
                }
            }
            val copiedHash = digest.digest().joinToString("") { "%02x".format(it) }
            if (!key.endsWith("-$copiedHash")) throw SaveProblem("The private download changed. Download it again.")
            output.flush()
            output.fd.sync()
        }
        val published = resolver.update(pendingUri(uri), ContentValues().apply {
            put(MediaStore.Downloads.IS_PENDING, 0)
        }, null, null)
        check(published == 1) { "Cannot finish saving the download" }
        return uri.toString()
    }
}
