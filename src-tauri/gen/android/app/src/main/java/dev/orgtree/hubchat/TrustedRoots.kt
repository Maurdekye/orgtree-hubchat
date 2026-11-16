package dev.orgtree.hubchat

import android.util.Base64
import java.security.KeyStore

/**
 * The certificate authorities this device trusts, for the core's hub connections
 * (hubchat_core::tls): Android's own store, which holds the system's roots and the ones the user
 * or a device policy installed, without the system roots the user switched off. The core adds
 * them to its built-in public list, so a hub with a certificate from its operator's own authority
 * works once that authority is installed here. Nothing in this file decides what is trusted.
 */
object TrustedRoots {
  /** One certificate per line, each the base64 of its DER bytes. Throws when the store can't be
   *  read, which reaches the core as "not now": it asks again later. */
  fun read(): String {
    val store = KeyStore.getInstance("AndroidCAStore").apply { load(null) }
    val out = StringBuilder()
    for (alias in store.aliases()) {
      // one unreadable entry doesn't cost the others
      val cert = try { store.getCertificate(alias) } catch (e: Exception) { null } ?: continue
      out.append(Base64.encodeToString(cert.encoded, Base64.NO_WRAP)).append('\n')
    }
    return out.toString()
  }
}
