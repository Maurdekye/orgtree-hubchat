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
    fun publish(context: Context, key: String, path: String, name: String): String {
        if (Build.VERSION.SDK_INT < 29) return path
        val source = File(path)
        val resolver = context.contentResolver
        val journalDir = File(context.filesDir, "download-exports")
        check(journalDir.isDirectory || journalDir.mkdirs()) { "Cannot create download receipt" }
        val journalName = MessageDigest.getInstance("SHA-256").digest(key.toByteArray())
            .joinToString("") { "%02x".format(it) }
        val journal = AtomicFile(File(journalDir, "$journalName.uri"))
        val previous = if (journal.baseFile.exists()) {
            journal.openRead().bufferedReader().use { it.readText() }.let(Uri::parse)
        } else null
        fun pending(uri: Uri): Boolean? = resolver.query(
            uri, arrayOf(MediaStore.Downloads.IS_PENDING), null, null, null
        )?.use { if (it.moveToFirst()) it.getInt(0) != 0 else null }
        // The URI was journalled before any bytes became visible. Once the
        // pending bit is clear, the prior copy finished, even if Rust died
        // before committing its pointer. Never overwrite the user's saved file.
        val oldPending = previous?.let(::pending)
        if (previous != null && oldPending == false) return previous.toString()
        val uri = if (previous != null && oldPending == true) previous else {
            check(source.isFile) { "The private download is missing; download it again" }
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
                resolver.delete(created, null, null)
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
            check(key.endsWith("-$copiedHash")) { "The private download changed; download it again" }
            output.flush()
            output.fd.sync()
        }
        val published = resolver.update(uri, ContentValues().apply {
            put(MediaStore.Downloads.IS_PENDING, 0)
        }, null, null)
        check(published == 1) { "Cannot finish saving the download" }
        return uri.toString()
    }
}
