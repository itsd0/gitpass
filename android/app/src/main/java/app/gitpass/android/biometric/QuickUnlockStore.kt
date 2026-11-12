package app.gitpass.android.biometric

import java.io.File
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.MessageDigest
import javax.crypto.Cipher

/**
 * Platform side of opt-in biometric unlock (#473). [KeystoreQuickUnlockKeys]
 * is the production implementation: a non-exportable AES-256-GCM key in
 * Android Keystore that can only be used right after a BIOMETRIC_STRONG
 * authentication and is invalidated when biometric enrolment changes.
 */
interface QuickUnlockKeys {
    /**
     * Deletes any key under [alias], creates a fresh one and returns an
     * ENCRYPT cipher that becomes usable once the user authenticates.
     * Throws [QuickUnlockUnavailableException] when the device cannot hold
     * such a key (no secure lock screen, no strong biometric enrolled).
     */
    fun newEncryptCipher(alias: String): Cipher

    /**
     * A DECRYPT cipher for [iv] that becomes usable once the user
     * authenticates. Throws [QuickUnlockInvalidatedException] when the key is
     * missing or was permanently invalidated (enrolment change, lock screen
     * removed).
     */
    fun decryptCipher(alias: String, iv: ByteArray): Cipher

    fun delete(alias: String)
}

/** Strong biometrics or an auth-bound Keystore key are not available on this device. */
class QuickUnlockUnavailableException(cause: Throwable? = null) : Exception("Biometric unlock is not available on this device.", cause)

/**
 * The saved biometric unlock can no longer be used (biometrics changed,
 * screen lock removed, key or file damaged, or the vault's password, Secret
 * Key or KDF changed). It has been deleted; unlock with the Master Password
 * and set biometric unlock up again.
 */
class QuickUnlockInvalidatedException(cause: Throwable? = null) : Exception("Biometric unlock was turned off. Unlock with your Master Password.", cause)

/**
 * One account's saved biometric unlock: the 32-byte Account Unlock Key
 * (derived by gitpass-core from the Master Password and Secret Key) sealed
 * with an auth-bound Keystore key. Plaintext key bytes only ever exist in
 * memory between an authentication and the native unlock, and every array
 * this class returns or receives is wiped by its owner.
 *
 * The sealed blob lives next to the account directory in noBackupFilesDir
 * (like the sync settings), never inside the synced vault and never in
 * SharedPreferences. Format: version(1) | iv(12) | AES-GCM(key)+tag(16),
 * with the account path as associated data so a blob copied to another
 * account's slot does not open.
 */
class QuickUnlockStore(private val root: File, private val keys: QuickUnlockKeys) {
    private val file = File(root.parentFile, "${root.name}.quick-unlock")
    private val aad = "gitpass-quick-unlock-v1\u0000${root.absolutePath}".toByteArray(Charsets.UTF_8)

    /** Keystore alias for this account; never contains the path itself. */
    val alias: String = ALIAS_PREFIX + MessageDigest.getInstance("SHA-256")
        .digest(root.absolutePath.toByteArray(Charsets.UTF_8))
        .take(16).joinToString("") { "%02x".format(it) }

    fun isEnabled(): Boolean = file.isFile

    /**
     * Step 1 of enabling: a fresh auth-bound key and its ENCRYPT cipher for
     * the BiometricPrompt. Any previous blob is removed first, because the
     * new key replaces the old one.
     */
    fun beginEnable(): Cipher {
        file.delete()
        return try {
            keys.newEncryptCipher(alias)
        } catch (error: QuickUnlockUnavailableException) {
            keys.delete(alias)
            throw error
        } catch (error: GeneralSecurityException) {
            keys.delete(alias)
            throw QuickUnlockUnavailableException(error)
        } catch (error: RuntimeException) {
            keys.delete(alias)
            throw QuickUnlockUnavailableException(error)
        }
    }

    /** Step 2 of enabling, after a successful authentication. Always wipes [unlockKey]. */
    fun finishEnable(authenticated: Cipher, unlockKey: ByteArray) {
        try {
            require(unlockKey.size == KEY_BYTES) { "Invalid unlock key" }
            authenticated.updateAAD(aad)
            val iv = authenticated.iv
            require(iv != null && iv.size == IV_BYTES) { "Invalid IV" }
            val sealed = authenticated.doFinal(unlockKey)
            val tmp = File(file.parentFile, "${file.name}.tmp")
            tmp.writeBytes(byteArrayOf(VERSION) + iv + sealed)
            if (!tmp.renameTo(file)) {
                tmp.delete()
                throw IOException("Could not save biometric unlock")
            }
        } catch (error: Throwable) {
            disable()
            throw error
        } finally {
            unlockKey.fill(0)
        }
    }

    /**
     * Step 1 of unlocking: the DECRYPT cipher for the BiometricPrompt.
     * Throws [QuickUnlockInvalidatedException] (after deleting everything)
     * when the key or the blob is unusable.
     */
    fun beginUnlock(): Cipher {
        val blob = readBlob()
        return try {
            keys.decryptCipher(alias, blob.copyOfRange(1, 1 + IV_BYTES))
        } catch (error: QuickUnlockInvalidatedException) {
            disable()
            throw error
        } catch (error: GeneralSecurityException) {
            disable()
            throw QuickUnlockInvalidatedException(error)
        }
    }

    /** Step 2 of unlocking, after a successful authentication. The caller wipes the result. */
    fun finishUnlock(authenticated: Cipher): ByteArray {
        val blob = readBlob()
        val key = try {
            authenticated.updateAAD(aad)
            authenticated.doFinal(blob, 1 + IV_BYTES, blob.size - 1 - IV_BYTES)
        } catch (error: GeneralSecurityException) {
            disable()
            throw QuickUnlockInvalidatedException(error)
        }
        if (key.size != KEY_BYTES) {
            key.fill(0)
            disable()
            throw QuickUnlockInvalidatedException()
        }
        return key
    }

    /** Turns biometric unlock off: deletes the sealed blob and the Keystore key. */
    fun disable() {
        file.delete()
        File(file.parentFile, "${file.name}.tmp").delete()
        runCatching { keys.delete(alias) }
    }

    private fun readBlob(): ByteArray {
        val blob = try {
            file.readBytes()
        } catch (error: IOException) {
            disable()
            throw QuickUnlockInvalidatedException(error)
        }
        if (blob.size != 1 + IV_BYTES + KEY_BYTES + TAG_BYTES || blob[0] != VERSION) {
            disable()
            throw QuickUnlockInvalidatedException()
        }
        return blob
    }

    override fun toString() = "QuickUnlockStore([redacted])"

    companion object {
        const val ALIAS_PREFIX = "gitpass.quickunlock.v1."
        const val KEY_BYTES = 32
        const val IV_BYTES = 12
        const val TAG_BYTES = 16
        private const val VERSION: Byte = 1
    }
}
