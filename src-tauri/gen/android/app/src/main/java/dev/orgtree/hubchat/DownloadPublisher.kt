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
    private fun pendingName(key: String): String = "hubchat-" +
        MessageDigest.getInstance("SHA-256").digest(key.toByteArray())
            .joinToString("") { "%02x".format(it) } + ".pending"
    private fun collection(): Uri = MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
    @Suppress("DEPRECATION")
    private fun pendingUri(uri: Uri): Uri = MediaStore.setIncludePending(uri)
    private fun pending(context: Context, uri: Uri): Boolean? {
        val projection = arrayOf(MediaStore.Downloads.IS_PENDING, MediaStore.MediaColumns.OWNER_PACKAGE_NAME)
        val cursor = if (Build.VERSION.SDK_INT >= 30) {
            context.contentResolver.query(uri, projection, android.os.Bundle().apply {
                putInt(MediaStore.QUERY_ARG_MATCH_PENDING, MediaStore.MATCH_INCLUDE)
            }, null)
        } else context.contentResolver.query(pendingUri(uri), projection, null, null, null)
        return checkNotNull(cursor) { "Cannot inspect saved downloads" }.use {
            if (it.moveToFirst() && it.getString(1) == context.packageName) it.getInt(0) != 0 else null
        }
    }
    private fun findPending(context: Context, key: String): Uri? {
        val selection = "${MediaStore.Downloads.DISPLAY_NAME} LIKE ? AND ${MediaStore.MediaColumns.OWNER_PACKAGE_NAME}=? AND ${MediaStore.Downloads.IS_PENDING}=1"
        // MediaProvider may append the MIME extension; the fixed SHA-256 prefix
        // has no SQL wildcard characters and remains unique to this export.
        val args = arrayOf(pendingName(key) + "%", context.packageName)
        val projection = arrayOf(MediaStore.Downloads._ID)
        val cursor = if (Build.VERSION.SDK_INT >= 30) {
            context.contentResolver.query(collection(), projection, android.os.Bundle().apply {
                putInt(MediaStore.QUERY_ARG_MATCH_PENDING, MediaStore.MATCH_INCLUDE)
                putString(android.content.ContentResolver.QUERY_ARG_SQL_SELECTION, selection)
                putStringArray(android.content.ContentResolver.QUERY_ARG_SQL_SELECTION_ARGS, args)
                putInt(android.content.ContentResolver.QUERY_ARG_LIMIT, 1)
            }, null)
        } else context.contentResolver.query(pendingUri(collection()), projection, selection, args, null)
        return checkNotNull(cursor) { "Cannot inspect saved downloads" }.use {
            if (it.moveToFirst()) android.content.ContentUris.withAppendedId(collection(), it.getLong(0)) else null
        }
    }
    /** Drop only our invisible pending copy; a completed user file always survives. */
    fun abandoned(context: Context, key: String, source: String, downloadRoot: String) {
        if (Build.VERSION.SDK_INT < 29) return
        val journal = receipt(context, key)
        val previous = if (journal.baseFile.exists()) {
            journal.openRead().bufferedReader().use { it.readText() }.let(Uri::parse)
        } else null
        val uris = listOfNotNull(previous, findPending(context, key)).distinct()
        for (uri in uris) {
            val selection = "${MediaStore.Downloads.IS_PENDING}=1 AND ${MediaStore.MediaColumns.OWNER_PACKAGE_NAME}=?"
            val args = arrayOf(context.packageName)
            if (Build.VERSION.SDK_INT >= 30) {
                context.contentResolver.delete(uri, android.os.Bundle().apply {
                    putInt(MediaStore.QUERY_ARG_MATCH_PENDING, MediaStore.MATCH_INCLUDE)
                    putString(android.content.ContentResolver.QUERY_ARG_SQL_SELECTION, selection)
                    putStringArray(android.content.ContentResolver.QUERY_ARG_SQL_SELECTION_ARGS, args)
                })
            } else context.contentResolver.delete(pendingUri(uri), selection, args)
        }
        // Rust holds both the job claim and publication lock, and passes an empty
        // source when another job still references this private path.
        if (source.isNotEmpty()) {
            val root = File(downloadRoot).absoluteFile
            val file = File(source).absoluteFile
            if (file.parentFile == root && !java.nio.file.Files.isSymbolicLink(root.toPath()) &&
                !java.nio.file.Files.isSymbolicLink(file.toPath()) && file.exists()) {
                check(file.isFile && file.delete()) { "Cannot remove private download" }
            }
        }
        journal.delete()
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
        // The URI was journalled before any bytes became visible. Once the
        // pending bit is clear, the prior copy finished, even if Rust died
        // before committing its pointer. Never overwrite the user's saved file.
        val oldPending = previous?.let { pending(context, it) }
        if (previous != null && oldPending == false) return previous.toString()
        val uri = if (previous != null && oldPending == true) previous else {
            findPending(context, key) ?: run {
                if (!source.isFile) throw SaveProblem("The private download is missing. Download it again.")
                val values = ContentValues().apply {
                    // Recover even a crash after insert but before the URI receipt.
                    put(MediaStore.Downloads.DISPLAY_NAME, pendingName(key))
                    val extension = name.substringAfterLast('.', "").lowercase(java.util.Locale.ROOT)
                    put(MediaStore.Downloads.MIME_TYPE, android.webkit.MimeTypeMap.getSingleton()
                        .getMimeTypeFromExtension(extension) ?: "application/octet-stream")
                    put(MediaStore.Downloads.IS_PENDING, 1)
                }
                checkNotNull(resolver.insert(collection(), values)) { "Cannot create download" }
            }
        }
        var output: java.io.FileOutputStream? = null
        try {
            output = journal.startWrite()
            output.write(uri.toString().toByteArray())
            journal.finishWrite(output)
        } catch (e: Exception) {
            journal.failWrite(output)
            // Leave the keyed pending row discoverable for replay or cleanup.
            throw e
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
            put(MediaStore.Downloads.DISPLAY_NAME, name)
            put(MediaStore.Downloads.IS_PENDING, 0)
        }, null, null)
        check(published == 1) { "Cannot finish saving the download" }
        return uri.toString()
    }
}
