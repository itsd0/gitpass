package app.gitpass.android.session

import app.gitpass.core.PasskeyCaller
import app.gitpass.core.PasskeyEntry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** VaultSession passkey paths (#477) against a fake bridge. */
class PasskeySessionTest {
    private class Gateway : VaultGateway, OpenVault {
        var opens = 0
        var closes = 0
        var registeredIn: String? = null
        override fun vaultExists(root: String) = true
        override fun createAccount(root: String, password: String) = error("unused")
        override fun open(root: String, password: String, secretKey: String): OpenVault {
            if (password != "FAKE") throw IllegalStateException("wrong password")
            opens++
            return this
        }
        override fun listVaults() = listOf(VaultSummary("v1", "Personal", null, 0))
        override fun listItems(vaultId: String) = emptyList<ItemRow>()
        override fun getItem(vaultId: String, itemId: String): ItemView = error("unused")
        override fun createVault(name: String) = "v1"
        override fun passkeyEntries(caller: PasskeyCaller, requestJson: String) = listOf(PasskeyEntry("Y3JlZA", "alice", "Personal", 0u))
        override fun passkeyRegister(caller: PasskeyCaller, requestJson: String, vaultId: String?): String { registeredIn = vaultId; return "{\"id\":\"new\"}" }
        override fun passkeyAssert(caller: PasskeyCaller, requestJson: String, credentialId: String, clientDataHash: ByteArray?) = "{\"id\":\"$credentialId\"}"
        override fun close() { closes++ }
    }

    private val web = PasskeyCaller.Browser("https://example.com")

    @Test fun lockedSessionNeverListsCreatesOrSigns() {
        val session = VaultSession(Gateway(), nowMs = { 0L })
        assertThrows(VaultLockedException::class.java) { session.passkeyEntries(web, "{}") }
        assertThrows(VaultLockedException::class.java) { session.passkeyRegister(web, "{}") }
        assertThrows(VaultLockedException::class.java) { session.passkeyAssert(web, "{}", "Y3JlZA", null) }
    }

    @Test fun unlockedSessionUsesTheBridgeAndTheSelectedVault() {
        val gateway = Gateway()
        val session = VaultSession(gateway, nowMs = { 0L })
        session.unlock("root", "FAKE", "A3-FAKE")
        assertEquals(listOf("alice"), session.passkeyEntries(web, "{}").map { it.userName })
        assertEquals("{\"id\":\"new\"}", session.passkeyRegister(web, "{}"))
        assertEquals("v1", gateway.registeredIn)
        assertEquals("{\"id\":\"Y3JlZA\"}", session.passkeyAssert(web, "{}", "Y3JlZA", ByteArray(32)))
    }

    @Test fun verifyChecksTheCredentialsWithoutTouchingTheOpenSession() {
        val gateway = Gateway()
        val session = VaultSession(gateway, nowMs = { 0L })
        session.unlock("root", "FAKE", "A3-FAKE")
        session.verify("root", "FAKE", "A3-FAKE")
        assertEquals(2, gateway.opens)
        assertEquals(1, gateway.closes, "the verification handle is closed at once")
        assertTrue(session.isUnlocked())
        assertThrows(IllegalStateException::class.java) { session.verify("root", "wrong", "A3-FAKE") }
        assertTrue(session.isUnlocked())
        session.lock()
        assertFalse(session.isUnlocked())
    }

    private fun assertEquals(expected: Int, actual: Int, message: String) = org.junit.Assert.assertEquals(message, expected.toLong(), actual.toLong())
}
