package dev.orgtree.hubchat

import android.content.Context
import android.content.Intent
import android.content.pm.ShortcutInfo
import android.content.pm.ShortcutManager
import android.net.Uri
import android.os.Build
import org.json.JSONArray

object LauncherShortcuts {
  fun update(context: Context, owner: String, json: String): String {
    if (Build.VERSION.SDK_INT < 25) return ""
    val manager = context.getSystemService(ShortcutManager::class.java) ?: return "Launcher unavailable"
    return try {
      val data = JSONArray(json)
      val rows = launcherContacts((0 until data.length()).map { i ->
        val row = data.getJSONObject(i)
        LauncherContact(row.getString("peer"), row.getString("name"), row.optString("kind", "person"),
          if (row.isNull("png")) null else row.optString("png").takeIf { it.isNotBlank() })
      }).take(4)
      val shortcuts = if (owner.isEmpty()) emptyList() else rows.mapIndexed { rank, row ->
        val uri = Uri.Builder().scheme("hubchat").authority("chat")
          .appendQueryParameter("to", row.peer).appendQueryParameter("owner", owner).build()
        ShortcutInfo.Builder(context, "dm:$owner:${row.peer}")
          .setShortLabel(row.name)
          .setLongLabel(row.name)
          .setRank(rank)
          .setIcon(LauncherAvatar.icon(context, row))
          .setIntent(Intent(context, MainActivity::class.java).setAction(Intent.ACTION_VIEW)
            .setData(uri).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP))
          .build()
      }
      // No pinned shortcut is requested. Disable stale pinned copies a user may
      // have dragged out, and also bind every intent to its originating identity.
      val ids = shortcuts.map { it.id }.toSet()
      val prefix = "dm:$owner:"
      val stale = manager.pinnedShortcuts.filter { it.id.startsWith("dm:") && (owner.isEmpty() || !it.id.startsWith(prefix)) }.map { it.id }
      if (stale.isNotEmpty()) manager.disableShortcuts(stale)
      // Clearing another identity's names must not depend on the update quota.
      if (owner.isEmpty() || manager.dynamicShortcuts.any { !it.id.startsWith(prefix) }) manager.removeAllDynamicShortcuts()
      if (!manager.setDynamicShortcuts(shortcuts)) return "Launcher update deferred"
      if (ids.isNotEmpty()) manager.enableShortcuts(ids.toList())
      ""
    } catch (_: Exception) {
      // A launcher/API failure must never break message sync or disclose contact data.
      "Launcher update failed"
    }
  }
}
