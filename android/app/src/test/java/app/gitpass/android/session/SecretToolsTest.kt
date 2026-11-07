package app.gitpass.android.session

import org.junit.Assert.*
import org.junit.Test

class SecretToolsTest {
    @Test fun generatorValidationNeverWritesAndRejectsLockedOrStaleRequests() {
        val tools = ControlledSecretTools()
        val session = VaultSession(ToolGateway(), nowMs = { 0L }, secretTools = tools)
        try { session.generatePassword(20, 0); fail("locked generation") } catch (_: VaultLockedException) { }
        session.unlock("fake", "FAKE", "FAKE")
        val epoch = session.state.value.epoch
        assertEquals(20, session.generatePassword(20, epoch).length)
        for (length in listOf(0, 11, 65, Int.MAX_VALUE)) {
            try { session.generatePassword(length, epoch); fail("bad length") } catch (_: IllegalArgumentException) { }
        }
        assertNull(session.state.value.detail)
        session.lock()
        session.unlock("fake", "FAKE", "FAKE")
        try { session.generatePassword(20, epoch); fail("stale generation") } catch (_: UnlockCancelledException) { }
    }

    @Test fun totpUsesBackendWallTimeWhileInactivityUsesIndependentMonotonicTime() {
        var monotonic = 0L
        val tools = ControlledSecretTools()
        val session = VaultSession(ToolGateway(), nowMs = { monotonic }, secretTools = tools)
        session.unlock("fake", "FAKE", "FAKE")
        session.openDetail("item")
        val epoch = session.state.value.epoch
        assertTrue(detailRows(session.state.value.detail!!).single().isTotp)
        tools.wallSeconds = 59
        assertEquals("94287082", session.liveTotp("item", "totp", epoch).code)
        assertEquals(1, session.liveTotp("item", "totp", epoch).secondsRemaining.toInt())
        tools.wallSeconds = 60
        assertEquals("37359152", session.liveTotp("item", "totp", epoch).code)
        tools.wallSeconds = 59 // time correction is reflected, not cached by countdown
        assertEquals("94287082", session.liveTotp("item", "totp", epoch).code)
        monotonic = VaultSession.DEFAULT_AUTO_LOCK_MS
        try { session.liveTotp("item", "totp", epoch); fail("expired session") } catch (_: VaultLockedException) { }
        assertNull(session.state.value.detail)
    }

    @Test fun invalidParserMessagesAndResultToStringCannotLeakSeedOrCode() {
        val session = VaultSession(ToolGateway(), nowMs = { 0L }, secretTools = object : SecretTools {
            override fun generate(length: Int) = "unused"
            override fun totp(seed: String): LiveCode = throw IllegalArgumentException("FAKE-sensitive-seed")
        })
        session.unlock("fake", "FAKE", "FAKE")
        session.openDetail("item")
        try { session.liveTotp("item", "totp", 0); fail("invalid config") }
        catch (e: VaultSessionException) { assertEquals("Invalid TOTP configuration", e.message) }
        assertFalse(LiveCode("94287082", 1).toString().contains("94287082"))
    }
}

private class ControlledSecretTools : SecretTools {
    var wallSeconds = 59L
    override fun generate(length: Int) = "X".repeat(length)
    override fun totp(seed: String) = if (wallSeconds == 59L) LiveCode("94287082", 1) else LiveCode("37359152", 30)
}

private class ToolGateway : VaultGateway, OpenVault {
    override fun vaultExists(root: String) = true
    override fun createAccount(root: String, password: String) = "FAKE"
    override fun open(root: String, password: String, secretKey: String): OpenVault = this
    override fun listVaults() = listOf(VaultSummary("v", "Fake", null, 1))
    override fun listItems(vaultId: String) = emptyList<ItemRow>()
    override fun getItem(vaultId: String, itemId: String) = ItemView("item", "v", "TOTP", null, null, null, null, "FAKE-seed", "login", emptyList(), false, false, false, "", emptyList())
    override fun createVault(name: String) = "v"
    override fun close() { }
}
