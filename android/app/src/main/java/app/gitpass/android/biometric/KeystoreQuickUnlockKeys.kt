package app.gitpass.android.biometric

import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import androidx.annotation.RequiresApi
import java.security.KeyStore
import java.security.UnrecoverableKeyException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * AES-256-GCM key in Android Keystore for biometric unlock (#473):
 * non-exportable, StrongBox-backed when the device has StrongBox,
 * usable only for one operation right after a BIOMETRIC_STRONG
 * authentication (no device-credential fallback, no validity window),
 * only while the device is unlocked, and permanently invalidated when a
 * biometric is enrolled or removed. API 30+ only, so the per-operation
 * BIOMETRIC_STRONG binding is always available; older devices keep using
 * the Master Password.
 */
class KeystoreQuickUnlockKeys : QuickUnlockKeys {
    override fun newEncryptCipher(alias: String): Cipher {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) throw QuickUnlockUnavailableException()
        delete(alias)
        // StrongBox first; fall back to the TEE when the device has no StrongBox
        // or its StrongBox rejects these parameters. Any other failure (no secure
        // lock screen, no strong biometric enrolled) means "not available".
        val key = try {
            generate(alias, strongBox = true)
        } catch (_: Exception) {
            delete(alias)
            try {
                generate(alias, strongBox = false)
            } catch (error: Exception) {
                delete(alias)
                throw QuickUnlockUnavailableException(error)
            }
        }
        return try {
            Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, key) }
        } catch (error: Exception) {
            delete(alias)
            throw QuickUnlockUnavailableException(error)
        }
    }

    override fun decryptCipher(alias: String, iv: ByteArray): Cipher {
        val key = try {
            keyStore().getKey(alias, null) as? SecretKey
        } catch (error: UnrecoverableKeyException) {
            throw QuickUnlockInvalidatedException(error)
        } ?: throw QuickUnlockInvalidatedException()
        return try {
            Cipher.getInstance(TRANSFORMATION).apply {
                init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(QuickUnlockStore.TAG_BYTES * 8, iv))
            }
        } catch (error: KeyPermanentlyInvalidatedException) {
            throw QuickUnlockInvalidatedException(error)
        }
    }

    override fun delete(alias: String) {
        runCatching { keyStore().deleteEntry(alias) }
    }

    /** True when a key exists under [alias] (tests and diagnostics; never exposes key material). */
    fun exists(alias: String): Boolean = runCatching { keyStore().containsAlias(alias) }.getOrDefault(false)

    @RequiresApi(Build.VERSION_CODES.R)
    private fun generate(alias: String, strongBox: Boolean): SecretKey {
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
        generator.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .setUserAuthenticationRequired(true)
                // 0 = every use needs its own authentication; strong biometrics only.
                .setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
                .setInvalidatedByBiometricEnrollment(true)
                .setUnlockedDeviceRequired(true)
                .setIsStrongBoxBacked(strongBox)
                .build(),
        )
        return generator.generateKey()
    }

    private fun keyStore(): KeyStore = KeyStore.getInstance(PROVIDER).apply { load(null) }

    override fun toString() = "KeystoreQuickUnlockKeys([redacted])"

    private companion object {
        const val PROVIDER = "AndroidKeyStore"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
    }
}
