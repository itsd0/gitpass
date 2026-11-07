package app.gitpass.android.session

import app.gitpass.core.ItemInput
import org.junit.Assert.*
import org.junit.Test

class ItemCrudTest {
    private fun item() = ItemView("i", "v", "Old", "owner", "FAKE-old", "https://example.invalid", "notes", "FAKE-totp", "login", listOf("tag"), true, false, false, "", listOf(
        SectionView("login", null, listOf(FieldView("password", "Password", "concealed", "FAKE-old"))),
        SectionView("custom", "Extra", listOf(FieldView("x", "Extra", "concealed", "FAKE-extra"))),
    ))

    @Test fun editingPreservesMetadataTotpAndCustomFieldsAndUpdatesCanonicalCopies() {
        val input = ItemEdit("Changed", "new-user", "FAKE-new", "https://example.invalid", "new notes").input(item())
        assertEquals(listOf("tag"), input.tags)
        assertTrue(input.favorite)
        assertEquals("FAKE-totp", input.totp)
        assertEquals("FAKE-new", input.sections[0].fields[0].value)
        assertEquals("FAKE-extra", input.sections[1].fields[0].value)
        assertFalse(ItemEdit("Title", password = "FAKE-secret").toString().contains("FAKE-secret"))
    }

    @Test fun createEditTrashRestoreAndStaleWrites() {
        val gateway = CrudGateway(item())
        val session = VaultSession(gateway, nowMs = { 0L })
        session.unlock("fake", "FAKE-password", "FAKE-key")
        val epoch = session.state.value.epoch
        session.saveItem("v", null, ItemEdit("Created"), epoch)
        assertEquals("Created", session.state.value.items.single().title)
        session.saveItem("v", "i", ItemEdit("Edited"), epoch)
        session.setDeleted("v", "i", true, epoch)
        assertTrue(session.state.value.items.isEmpty())
        session.setTrash(true)
        assertTrue(session.state.value.items.single().deleted)
        session.setDeleted("v", "i", false, epoch)
        assertTrue(session.state.value.items.isEmpty())
        session.setTrash(false)
        assertEquals("Edited", session.state.value.items.single().title)
        session.lock()
        session.unlock("fake", "FAKE-password", "FAKE-key")
        val count = gateway.writes
        try {
            session.saveItem("v", "i", ItemEdit("Must not write"), epoch)
            fail("stale epoch accepted")
        } catch (_: UnlockCancelledException) { }
        assertEquals(count, gateway.writes)
    }

    @Test fun emptyTitlesAndNonLoginEditsAreRejected() {
        try { ItemEdit(" ").input(null); fail("blank title") } catch (_: IllegalArgumentException) { }
        try { ItemEdit("Title").input(item().copy(category = "card")); fail("unsupported edit") } catch (_: IllegalArgumentException) { }
    }
}

private class CrudGateway(var item: ItemView) : VaultGateway, OpenVault {
    var writes = 0
    override fun vaultExists(root: String) = true
    override fun createAccount(root: String, password: String) = "FAKE-key"
    override fun open(root: String, password: String, secretKey: String): OpenVault = this
    override fun listVaults() = listOf(VaultSummary("v", "Fake", null, 1))
    override fun listItems(vaultId: String) = listOf(ItemRow("i", "v", item.title, "", null, "login", item.tags, item.favorite, item.archived, item.deleted, true))
    override fun getItem(vaultId: String, itemId: String) = item
    override fun createVault(name: String) = "v"
    override fun createItem(vaultId: String, input: ItemInput): String { updateItem(vaultId, "i", input); return "i" }
    override fun updateItem(vaultId: String, itemId: String, input: ItemInput) { writes++; item = item.copy(title = input.title, username = input.username, password = input.password) }
    override fun deleteItem(vaultId: String, itemId: String) { writes++; item = item.copy(deleted = true) }
    override fun restoreItem(vaultId: String, itemId: String) { writes++; item = item.copy(deleted = false) }
    override fun close() { }
}
