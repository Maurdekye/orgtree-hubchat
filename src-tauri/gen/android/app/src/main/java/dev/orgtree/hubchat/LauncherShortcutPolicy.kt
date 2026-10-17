package dev.orgtree.hubchat

/** Message ordering is supplied by the native store; don't reorder on receipt or launch. */
data class LauncherContact(val peer: String, val name: String, val kind: String = "person", val png: String? = null)

fun launcherContacts(rows: List<LauncherContact>): List<LauncherContact> =
  rows.filter { it.peer.isNotBlank() }.distinctBy { it.peer }.take(4).map {
    it.copy(name = it.name.replace("\u0000", "").ifBlank { it.peer })
  }
