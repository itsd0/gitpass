package app.gitpass.android.biometric

import java.io.File
import java.nio.file.Files
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** Software stand-in for the auth-bound Keystore key: same cipher contract, no hardware. */
class SoftwareQuickUnlockKeys : QuickUnlockKeys {
    val keys = mutableMapOf<String, SecretKey>()
    var invalidated = false
    var unavailable = false

    override fun newEncryptCipher(alias: String): Cipher {
        if (unavailable) throw QuickUnlockUnavailableException()
        val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
        keys[alias] = key
        return Cipher.getInstance("AES/GCM/NoPadding").apply {
            val iv = ByteArray(12).also { SecureRandom().nextBytes(it) }
            init(Cipher.ENCRYPT_MODE, key, GCMParameterSpec(128, iv))
        }
    }

    override fun decryptCipher(alias: String, iv: ByteArray): Cipher {
        if (invalidated) throw QuickUnlockInvalidatedException()
        val key = keys[alias] ?: throw QuickUnlockInvalidatedException()
        return Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, iv)) }
    }

    override fun delete(alias: String) { keys.remove(alias) }
}

class QuickUnlockStoreTest {
    private val dir: File = Files.createTempDirectory("quick-unlock").toFile()
    private val root = File(dir, "vault").apply { mkdirs() }
    private val keys = SoftwareQuickUnlockKeys()
    private val store = QuickUnlockStore(root, keys)
    private val blob = File(dir, "vault.quick-unlock")
    private fun fakeKey() = ByteArray(32) { (it + 1).toByte() }

    private fun enable(key: ByteArray = fakeKey()) = store.finishEnable(store.beginEnable(), key)

    @Test fun enableThenUnlockRoundTripsAndWipesInputs() {
        val input = fakeKey()
        enable(input)
        assertTrue("the caller's copy is wiped", input.all { it == 0.toByte() })
        assertTrue(store.isEnabled())
        val bytes = blob.readBytes()
        assertEquals(1 + 12 + 32 + 16, bytes.size)
        assertFalse("never stored in plaintext", bytes.toList().windowed(32).any { it.toByteArray().contentEquals(fakeKey()) })
        val opened = store.finishUnlock(store.beginUnlock())
        assertArrayEquals(fakeKey(), opened)
        assertFalse("blob is outside the synced account directory", blob.parentFile == root)
        assertTrue(store.alias.startsWith(QuickUnlockStore.ALIAS_PREFIX))
        assertFalse(store.alias.contains(root.name))
    }

    @Test fun disableDeletesBlobAndKey() {
        enable()
        store.disable()
        assertFalse(store.isEnabled())
        assertFalse(blob.exists())
        assertTrue(keys.keys.isEmpty())
        assertThrows(QuickUnlockInvalidatedException::class.java) { store.beginUnlock() }
    }

    @Test fun invalidatedKeyFallsBackAndCleansUp() {
        enable()
        keys.invalidated = true
        assertThrows(QuickUnlockInvalidatedException::class.java) { store.beginUnlock() }
        assertFalse(store.isEnabled())
        assertTrue(keys.keys.isEmpty())
    }

    @Test fun tamperedOrForeignBlobIsRejectedAndDeleted() {
        enable()
        val bytes = blob.readBytes()
        bytes[bytes.size - 1] = (bytes[bytes.size - 1].toInt() xor 1).toByte()
        blob.writeBytes(bytes)
        val cipher = store.beginUnlock()
        assertThrows(QuickUnlockInvalidatedException::class.java) { store.finishUnlock(cipher) }
        assertFalse(store.isEnabled())

        // Another account's blob under this account's name does not open (path is AAD).
        val otherRoot = File(dir, "other").apply { mkdirs() }
        val other = QuickUnlockStore(otherRoot, keys)
        other.finishEnable(other.beginEnable(), fakeKey())
        File(dir, "other.quick-unlock").copyTo(blob, overwrite = true)
        keys.keys[store.alias] = keys.keys.getValue(other.alias)
        val c = store.beginUnlock()
        assertThrows(QuickUnlockInvalidatedException::class.java) { store.finishUnlock(c) }
    }

    @Test fun truncatedBlobIsInvalid() {
        enable()
        blob.writeBytes(byteArrayOf(1, 2, 3))
        assertThrows(QuickUnlockInvalidatedException::class.java) { store.beginUnlock() }
        assertFalse(blob.exists())
    }

    @Test fun unavailableDeviceNeverWritesAnything() {
        keys.unavailable = true
        assertThrows(QuickUnlockUnavailableException::class.java) { store.beginEnable() }
        assertFalse(store.isEnabled())
        assertTrue(keys.keys.isEmpty())
    }

    @Test fun badKeyLengthIsRefusedAndWiped() {
        val short = ByteArray(16) { 9 }
        assertThrows(IllegalArgumentException::class.java) { store.finishEnable(store.beginEnable(), short) }
        assertTrue(short.all { it == 0.toByte() })
        assertFalse(store.isEnabled())
        assertTrue(keys.keys.isEmpty())
    }

    @Test fun reEnableReplacesThePreviousKey() {
        enable(ByteArray(32) { 1 })
        enable(ByteArray(32) { 2 })
        assertArrayEquals(ByteArray(32) { 2 }, store.finishUnlock(store.beginUnlock()))
        assertEquals(1, keys.keys.size)
    }

    @Test fun toStringIsRedacted() {
        assertFalse(store.toString().contains(root.path))
    }
}
