package dev.orgtree.hubchat

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Seals small secrets (the identity key) with an AES-GCM key that lives in
 * the Android Keystore and never leaves it. Rust stores only the sealed text.
 */
object SecretBox {
  private const val ALIAS = "hubchat-identity"

  private fun key(): SecretKey {
    val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
    val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
    gen.init(
      KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
        .setKeySize(256)
        .build())
    return gen.generateKey()
  }

  /** Returns base64(iv || ciphertext), or "" on failure. */
  @JvmStatic
  fun seal(plain: String): String = try {
    val c = Cipher.getInstance("AES/GCM/NoPadding")
    c.init(Cipher.ENCRYPT_MODE, key())
    val out = c.iv + c.doFinal(plain.toByteArray(Charsets.UTF_8))
    Base64.encodeToString(out, Base64.NO_WRAP)
  } catch (e: Exception) {
    ""
  }

  /** Inverse of seal; "" when the text can't be opened (e.g. key lost). */
  @JvmStatic
  fun open(sealed: String): String = try {
    val raw = Base64.decode(sealed, Base64.NO_WRAP)
    val c = Cipher.getInstance("AES/GCM/NoPadding")
    c.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, raw, 0, 12))
    String(c.doFinal(raw, 12, raw.size - 12), Charsets.UTF_8)
  } catch (e: Exception) {
    ""
  }
}
