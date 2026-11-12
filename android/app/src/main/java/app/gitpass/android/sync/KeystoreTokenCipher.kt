package app.gitpass.android.sync

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * AES-256-GCM with a non-exportable key generated inside Android Keystore
 * (hardware-backed where the device supports it). Only the app's own uid can
 * use the key. User authentication is not required for the key, so automatic
 * sync can run while the vault is unlocked; the vault data itself stays
 * protected by the Master Password + Secret Key, which are never stored.
 * This is platform key storage, not vault crypto: no vault crypto is
 * re-implemented in Kotlin.
 */
class KeystoreTokenCipher(private val alias: String = DEFAULT_ALIAS) : TokenCipher {
    private fun key(): SecretKey {
        val store = KeyStore.getInstance(PROVIDER).apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
        generator.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .build(),
        )
        return generator.generateKey()
    }

    override fun encrypt(plain: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        cipher.updateAAD(AAD)
        val iv = cipher.iv
        require(iv.size == IV_BYTES)
        return byteArrayOf(VERSION) + iv + cipher.doFinal(plain)
    }

    override fun decrypt(sealed: ByteArray): ByteArray {
        require(sealed.size > 1 + IV_BYTES + TAG_BYTES && sealed[0] == VERSION) { "Invalid sealed token" }
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(TAG_BYTES * 8, sealed, 1, IV_BYTES))
        cipher.updateAAD(AAD)
        return cipher.doFinal(sealed, 1 + IV_BYTES, sealed.size - 1 - IV_BYTES)
    }

    /** Removes the Keystore key; any sealed token becomes unreadable. */
    fun deleteKey() {
        runCatching { KeyStore.getInstance(PROVIDER).apply { load(null) }.deleteEntry(alias) }
    }

    override fun toString() = "KeystoreTokenCipher([redacted])"

    companion object {
        const val DEFAULT_ALIAS = "gitpass.sync.token.v1"
        private const val PROVIDER = "AndroidKeyStore"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val VERSION: Byte = 1
        private const val IV_BYTES = 12
        private const val TAG_BYTES = 16
        private val AAD = "gitpass-sync-token-v1".toByteArray(Charsets.UTF_8)
    }
}
