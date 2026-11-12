package app.gitpass.android.session

import app.gitpass.android.autofill.AutofillTarget
import app.gitpass.core.ItemInput
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** VaultSession autofill paths (#475) against a fake bridge that applies the matching rule. */
class AutofillSessionTest {
    private class Row(val vaultId: String, val id: String, var title: String, var username: String?, var password: String?, val url: String)

    private class Gateway : VaultGateway, OpenVault {
        val rows = mutableListOf(Row("v1", "1", "Example", "alice", "old-FAKE", "https://example.com"))
        var created = 0
        override fun vaultExists(root: String) = true
        override fun createAccount(root: String, password: String) = error("unused")
        override fun open(root: String, password: String, secretKey: String): OpenVault = this
        override fun listVaults() = listOf(VaultSummary("v1", "Personal", null, rows.size.toLong()))
        override fun listItems(vaultId: String) = emptyList<ItemRow>()
        override fun getItem(vaultId: String, itemId: String): ItemView {
            val r = rows.single { it.id == itemId }
            return ItemView(r.id, r.vaultId, r.title, r.username, r.password, r.url, null, null, "login", emptyList(), false, false, false, "", emptyList())
        }
        override fun createVault(name: String) = "v1"
        override fun autofillLogins(target: AutofillTarget) =
            rows.filter { it.url == target.itemUrl }.map { AutofillCredential(it.vaultId, it.id, it.title, it.username, it.password) }
        override fun createItem(vaultId: String, input: ItemInput): String {
            created++
            rows += Row(vaultId, "n$created", input.title, input.username, input.password, input.url.orEmpty())
            return "n$created"
        }
        override fun updateItem(vaultId: String, itemId: String, input: ItemInput) {
            rows.single { it.id == itemId }.apply { username = input.username; password = input.password }
        }
        override fun close() = Unit
    }

    private val site = AutofillTarget("https://example.com", null)

    @Test fun lockedSessionNeverReturnsLogins() {
        val session = VaultSession(Gateway(), nowMs = { 0L })
        assertThrows(VaultLockedException::class.java) { session.autofillLogins(site) }
        assertThrows(VaultLockedException::class.java) { session.saveAutofillLogin(site, "alice", "x") }
    }

    @Test fun returnsOnlyMatchingLogins() {
        val session = VaultSession(Gateway(), nowMs = { 0L })
        session.unlock("root", "FAKE", "A3-FAKE")
        assertEquals(listOf("Example"), session.autofillLogins(site).map { it.title })
        assertTrue(session.autofillLogins(AutofillTarget("https://other.example", null)).isEmpty())
    }

    @Test fun saveUpdatesSameUsernameOrCreates() {
        val gateway = Gateway()
        val session = VaultSession(gateway, nowMs = { 0L })
        session.unlock("root", "FAKE", "A3-FAKE")
        assertEquals("Example", session.saveAutofillLogin(site, "alice", "new-FAKE"))
        assertEquals("new-FAKE", gateway.rows.single { it.id == "1" }.password)
        assertEquals(0, gateway.created)
        assertEquals("example.com", session.saveAutofillLogin(site, "bob", "bob-FAKE"))
        assertEquals(1, gateway.created)
        assertEquals("https://example.com", gateway.rows.last().url)
        val app = AutofillTarget(null, "com.example.app")
        session.saveAutofillLogin(app, "carol", "c-FAKE")
        assertEquals("android://com.example.app", gateway.rows.last().url)
        assertThrows(IllegalArgumentException::class.java) { session.saveAutofillLogin(site, "dan", "") }
    }
}
