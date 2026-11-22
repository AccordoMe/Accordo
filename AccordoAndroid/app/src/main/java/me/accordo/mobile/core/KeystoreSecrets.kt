// SPDX-License-Identifier: GPL-3.0-or-later
// Where the phone keeps secrets: AES-256-GCM values in SharedPreferences, sealed with a key that
// never leaves the Android Keystore. This is the Kotlin side of `accordo_core::secrets::SecretStore`
// — the desktop backs the same trait with the OS credential store.

package me.accordo.mobile.core

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import uniffi.accordo_core.SecretStoreCallback

/**
 * Sessions (`session:<id>:<nick>@<server>`) and the device's chat key (`e2ee:…`) live here.
 *
 * The AES key is generated once in the Android Keystore (non-exportable); each value gets a fresh
 * 12-byte IV and is stored as base64(iv + ciphertext). A wiped Keystore (factory reset, restore to
 * another phone) makes old entries unreadable — that is the intended behaviour: sign in again.
 */
class KeystoreSecrets(context: Context) : SecretStoreCallback {

    private val prefs = context.getSharedPreferences("accordo.secrets", Context.MODE_PRIVATE)

    private val secretKey: SecretKey by lazy {
        val store = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        (store.getEntry(KEY_ALIAS, null) as? KeyStore.SecretKeyEntry)?.secretKey ?: generateKey()
    }

    private fun generateKey(): SecretKey {
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return generator.generateKey()
    }

    override fun `get`(key: String): String? {
        val stored = prefs.getString(key, null) ?: return null
        return runCatching {
            val bytes = Base64.decode(stored, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, secretKey, GCMParameterSpec(TAG_BITS, bytes, 0, IV_BYTES))
            String(cipher.doFinal(bytes, IV_BYTES, bytes.size - IV_BYTES), Charsets.UTF_8)
        }.getOrNull()
    }

    override fun `set`(key: String, value: String): Boolean = runCatching {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, secretKey)
        val sealed = cipher.iv + cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        prefs.edit().putString(key, Base64.encodeToString(sealed, Base64.NO_WRAP)).apply()
        true
    }.getOrDefault(false)

    override fun `remove`(key: String) {
        prefs.edit().remove(key).apply()
    }

    private companion object {
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val KEY_ALIAS = "accordo.secrets"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val IV_BYTES = 12
        const val TAG_BITS = 128
    }
}
