package app.gitpass.android.session

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** VaultSession paths for biometric unlock (#473) against a fake bridge. */
class QuickUnlockSessionTest {
    private val goodKey = ByteArray(32) { 7 }

    private inner class KeyGateway : VaultGateway {
        var stale = false
        var derives = 0
        var passwordOpens = 0
        var lastOpenKey: ByteArray? = null
        override fun vaultExists(root: String) = true
        override fun createAccount(root: String, password: String) = error("unused")
        override fun open(root: String, password: String, secretKey: String): OpenVault {
            passwordOpens++
            return FakeVault()
        }
        override fun deriveUnlockKey(root: String, password: String, secretKey: String): ByteArray {
            derives++
            if (password != "FAKE-right" || secretKey != "A3-FAKE") throw IllegalStateException("wrong master password or Secret Key")
            return goodKey.copyOf()
        }
        override fun openWithUnlockKey(root: String, key: ByteArray): OpenVault {
            try {
                lastOpenKey = key.copyOf()
                if (stale || !key.contentEquals(goodKey)) throw QuickUnlockStaleException()
                return FakeVault()
            } finally {
                key.fill(0)
            }
        }
    }

    private class FakeVault : OpenVault {
        override fun listVaults() = listOf(VaultSummary("v1", "Personal", null, 0))
        override fun listItems(vaultId: String) = emptyList<ItemRow>()
        override fun getItem(vaultId: String, itemId: String): ItemView = error("unused")
        override fun createVault(name: String) = "v1"
        override fun close() = Unit
    }

    @Test fun setupUnlockDerivesOnceAndReturnsTheKey() {
        val gateway = KeyGateway()
        val session = VaultSession(gateway, nowMs = { 0L })
        val key = session.unlockForQuickSetup("root", "FAKE-right", "A3-FAKE")
        assertTrue(session.isUnlocked())
        assertArrayEquals(goodKey, key)
        assertEquals(1, gateway.derives)
        assertEquals("one KDF run: no separate password open", 0, gateway.passwordOpens)
    }

    @Test fun wrongPasswordDuringSetupStaysLocked() {
        val gateway = KeyGateway()
        val session = VaultSession(gateway, nowMs = { 0L })
        assertThrows(IllegalStateException::class.java) { session.unlockForQuickSetup("root", "FAKE-wrong", "A3-FAKE") }
        assertFalse(session.isUnlocked())
    }

    @Test fun keyUnlockWipesTheKeyAndUnlocks() {
        val session = VaultSession(KeyGateway(), nowMs = { 0L })
        val key = goodKey.copyOf()
        session.unlockWithKey("root", key)
        assertTrue(session.isUnlocked())
        assertTrue(key.all { it == 0.toByte() })
    }

    @Test fun staleKeyIsReportedWipedAndStaysLocked() {
        val gateway = KeyGateway().apply { stale = true }
        val session = VaultSession(gateway, nowMs = { 0L })
        val key = goodKey.copyOf()
        assertThrows(QuickUnlockStaleException::class.java) { session.unlockWithKey("root", key) }
        assertFalse(session.isUnlocked())
        assertTrue(key.all { it == 0.toByte() })
    }

    @Test fun keyUnlockWhileUnlockedIsRefusedAndStillWipes() {
        val session = VaultSession(KeyGateway(), nowMs = { 0L })
        session.unlock("root", "FAKE-right", "A3-FAKE")
        val key = goodKey.copyOf()
        assertThrows(VaultSessionException::class.java) { session.unlockWithKey("root", key) }
        assertTrue(key.all { it == 0.toByte() })
    }

    @Test fun lockAfterKeyUnlockCancelsAndLocks() {
        var locked = 0
        val session = VaultSession(KeyGateway(), nowMs = { 0L }, onLock = { locked++ })
        session.unlockWithKey("root", goodKey.copyOf())
        session.lock()
        assertFalse(session.isUnlocked())
        assertEquals(1, locked)
    }
}
