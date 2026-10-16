package dev.orgtree.hubchat

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** Run against either applicationId. No viewer is launched and no existing
 * download is touched; contexts capture or refuse the outbound intent. */
@RunWith(AndroidJUnit4::class)
class DownloadOpenerTest {
    private val context: Context get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Test fun mediaStoreApkKeepsOriginalUriAndGetsReadGrant() {
        val source = "content://media/external_primary/downloads/12345"
        val intent = DownloadOpener.viewIntent(context, source, "Hubchat.APK")
        assertEquals(Intent.ACTION_VIEW, intent.action)
        assertEquals(Uri.parse(source), intent.data)
        assertEquals("application/vnd.android.package-archive", intent.type)
        assertTrue(intent.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION != 0)
        assertTrue(intent.flags and Intent.FLAG_ACTIVITY_NEW_TASK != 0)
        assertEquals(intent.data, intent.clipData!!.getItemAt(0).uri)
    }

    @Test fun privateFileUsesCurrentApplicationIdProviderAndOriginalBytes() {
        val downloads = File(context.filesDir, "downloads").apply { mkdirs() }
        val file = File.createTempFile("open-proof-", ".txt", downloads)
        try {
            file.writeText("synthetic download")
            val intent = DownloadOpener.viewIntent(context, file.absolutePath, file.name)
            assertEquals("content", intent.data!!.scheme)
            assertEquals(context.packageName + ".fileprovider", intent.data!!.authority)
            assertEquals("text/plain", intent.type)
            assertEquals("synthetic download", context.contentResolver.openInputStream(intent.data!!)!!.bufferedReader().use { it.readText() })
        } finally { file.delete() }
    }

    @Test fun apkInstallPermissionIsDeclaredForCurrentIdentity() {
        @Suppress("DEPRECATION")
        val info = context.packageManager.getPackageInfo(context.packageName, android.content.pm.PackageManager.GET_PERMISSIONS)
        assertTrue(info.requestedPermissions!!.contains("android.permission.REQUEST_INSTALL_PACKAGES"))
    }

    @Test fun successfulDispatchReturnsNoError() {
        var captured: Intent? = null
        val receiver = object : ContextWrapper(context) {
            override fun startActivity(intent: Intent) { captured = intent }
        }
        assertEquals("", DownloadOpener.open(receiver, "content://media/external/downloads/1", "proof.pdf"))
        assertEquals("application/pdf", captured!!.type)
        assertTrue(captured!!.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION != 0)
    }

    @Test fun noHandlerBecomesShortVisibleError() {
        val receiver = object : ContextWrapper(context) {
            override fun startActivity(intent: Intent) { throw ActivityNotFoundException() }
        }
        assertEquals("No app can open this file.", DownloadOpener.open(receiver, "content://media/external/downloads/1", "proof.pdf"))
    }

    @Test fun fileOutsideProviderRootsDoesNotDispatch() {
        val file = File.createTempFile("outside-download-proof-", ".txt", context.filesDir)
        val receiver = object : ContextWrapper(context) {
            override fun startActivity(intent: Intent) { fail("Unshared file must not dispatch") }
        }
        try {
            file.writeText("synthetic unshared file")
            try {
                DownloadOpener.viewIntent(context, file.absolutePath, file.name)
                fail("FileProvider must refuse this path outside its roots")
            } catch (_: IllegalArgumentException) { /* expected provider refusal */ }
            assertEquals("Could not open this file. Try again.",
                DownloadOpener.open(receiver, file.absolutePath, file.name))
            assertEquals("synthetic unshared file", file.readText())
        } finally { file.delete() }
    }

    @Test fun missingFileDoesNotDispatch() {
        val receiver = object : ContextWrapper(context) {
            override fun startActivity(intent: Intent) { fail("Missing file must not dispatch") }
        }
        assertEquals("This file is no longer available. Download it again.",
            DownloadOpener.open(receiver, File(context.filesDir, "missing-" + java.util.UUID.randomUUID()).absolutePath, "missing.txt"))
    }
}
