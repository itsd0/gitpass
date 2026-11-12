package app.gitpass.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.android.biometric.KeystoreQuickUnlockKeys
import app.gitpass.android.biometric.PlatformBiometricGate
import app.gitpass.android.biometric.QuickUnlockInvalidatedException
import app.gitpass.android.biometric.QuickUnlockStore
import app.gitpass.android.biometric.QuickUnlockUnavailableException
import app.gitpass.android.session.FfiVaultGateway
import app.gitpass.android.session.QuickUnlockStaleException
import app.gitpass.core.GitpassException
import java.io.File
import java.util.UUID
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeFalse
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Real Android Keystore and real native bridge for biometric unlock (#473).
 * CI emulators have no enrolled biometric, so the Keystore part checks that
 * enabling fails closed there (no key, no blob). The bridge part checks the
 * derived key really opens a vault and that a wrong key is reported as stale.
 * Temporary test accounts only; every secret is fake.
 */
@RunWith(AndroidJUnit4::class)
class BiometricKeystoreTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    @Test fun withoutStrongBiometricsEnablingFailsClosed() {
        assumeFalse("device has an enrolled strong biometric", PlatformBiometricGate().available(context))
        val keys = KeystoreQuickUnlockKeys()
        val root = File(context.cacheDir, "bio-ks-${UUID.randomUUID()}").apply { mkdirs() }
        val store = QuickUnlockStore(root, keys)
        try {
            assertThrows(QuickUnlockUnavailableException::class.java) { store.beginEnable() }
            assertFalse("no Keystore key left behind", keys.exists(store.alias))
            assertFalse("no sealed blob written", store.isEnabled())
            assertThrows(QuickUnlockInvalidatedException::class.java) { keys.decryptCipher(store.alias, ByteArray(12)) }
        } finally {
            store.disable()
            root.deleteRecursively()
        }
    }

    @Test fun derivedKeyOpensTheVaultAndAWrongKeyIsStale() {
        val gateway = FfiVaultGateway()
        val root = File(context.cacheDir, "bio-bridge-${UUID.randomUUID()}").apply { mkdirs() }
        try {
            val secret = gateway.createAccount(root.absolutePath, "FAKE biometric test password")
            assertThrows(GitpassException::class.java) { gateway.deriveUnlockKey(root.absolutePath, "FAKE wrong password", secret) }
            val key = gateway.deriveUnlockKey(root.absolutePath, "FAKE biometric test password", secret)
            assertEquals(32, key.size)
            val copy = key.copyOf()
            gateway.openWithUnlockKey(root.absolutePath, key).close()
            assertTrue("caller's array is wiped", key.all { it == 0.toByte() })
            copy[0] = (copy[0].toInt() xor 1).toByte()
            assertThrows(QuickUnlockStaleException::class.java) { gateway.openWithUnlockKey(root.absolutePath, copy) }
            assertThrows(QuickUnlockStaleException::class.java) { gateway.openWithUnlockKey(root.absolutePath, ByteArray(5)) }
        } finally {
            root.deleteRecursively()
        }
    }
}
