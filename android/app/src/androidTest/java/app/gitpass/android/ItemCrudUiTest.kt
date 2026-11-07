package app.gitpass.android

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.android.session.*
import app.gitpass.android.ui.TestTags
import app.gitpass.core.ItemInput
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Actual Compose screens; all data and storage are fake/cache-only. Rust tests cover the real bridge. */
@RunWith(AndroidJUnit4::class)
class ItemCrudUiTest {
    @get:Rule val rule = createEmptyComposeRule()

    @Test fun createEditPreserveTrashRestoreAndLockEditor() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val app = context.applicationContext as GitpassApplication
        val root = File(context.cacheDir, "crud-ui-${UUID.randomUUID()}").apply { mkdirs() }
        File(root, "format.json").writeText("{}")
        val gateway = UiCrudGateway()
        app.installForTest(gateway, root, nowMs = { 0L })
        try {
            ActivityScenario.launch(MainActivity::class.java).use {
                rule.onNodeWithTag(TestTags.MASTER_PASSWORD).performTextInput("FAKE-password")
                rule.onNodeWithTag(TestTags.SECRET_KEY).performTextInput("FAKE-key")
                rule.onNodeWithTag(TestTags.UNLOCK).performClick()
                rule.waitUntil(10_000) { app.session.state.value.unlocked }
                rule.onNodeWithText("Seed login").performClick()
                rule.onNodeWithText("FAKE-preserved-password").assertDoesNotExist()
                rule.onNodeWithText("Edit login").performClick()
                rule.onNodeWithTag("crud-title").performTextReplacement("Edited seed")
                rule.onNodeWithTag("crud-save").performScrollTo().performClick()
                rule.waitUntil(10_000) { app.session.state.value.items.any { row -> row.title == "Edited seed" } }
                val edited = gateway.records.getValue("seed")
                assertEquals("FAKE-preserved-password", edited.password)
                assertEquals(listOf("keep-tag"), edited.tags)
                assertTrue(edited.favorite)
                assertEquals("FAKE-totp", edited.totp)
                assertEquals("FAKE-custom", edited.sections.single().fields.single().value)

                rule.onNodeWithText("New login").performClick()
                rule.onNodeWithTag("crud-title").performTextInput("Created login")
                rule.onNodeWithTag("crud-username").performTextInput("owner@example.invalid")
                rule.onNodeWithTag("crud-password").performTextInput("FAKE-created-password")
                rule.onNodeWithTag("crud-save").performScrollTo().performClick()
                rule.waitUntil(10_000) { app.session.state.value.items.any { row -> row.title == "Created login" } }
                rule.onNodeWithText("Created login").performClick()
                rule.onNodeWithText("FAKE-created-password").assertDoesNotExist()
                rule.onNodeWithText("Reveal").performClick()
                rule.onNodeWithText("FAKE-created-password").assertIsDisplayed()
                rule.onNodeWithText("Move to trash").performClick()
                rule.waitUntil(10_000) { app.session.state.value.detail == null }
                rule.onNodeWithText("Created login").assertDoesNotExist()
                rule.onNodeWithText("Trash").performClick()
                rule.onNodeWithText("Created login").performClick()
                rule.onNodeWithText("Restore").performClick()
                rule.waitUntil(10_000) { !gateway.records.getValue("created").deleted }
                rule.onNodeWithText("Active items").performClick()
                rule.onNodeWithText("Created login").performClick()
                rule.onNodeWithText("Edit login").performClick()
                rule.onNodeWithTag("crud-password").performTextReplacement("FAKE-unsaved")
                rule.onNodeWithText("Lock").performClick()
                rule.onNodeWithTag(TestTags.MASTER_PASSWORD).assertIsDisplayed()
                rule.onNodeWithTag("crud-password").assertDoesNotExist()
                assertNull(app.session.state.value.detail)
                assertEquals("FAKE-created-password", gateway.records.getValue("created").password)
            }
        } finally {
            app.session.lock()
            app.pendingSecret.clear()
            root.deleteRecursively()
        }
    }
}

private class UiCrudGateway : VaultGateway, OpenVault {
    val records = java.util.concurrent.ConcurrentHashMap<String, ItemView>()
    init {
        records["seed"] = ItemView("seed", "v", "Seed login", "owner", "FAKE-preserved-password", "https://example.invalid", "notes", "FAKE-totp", "login", listOf("keep-tag"), true, false, false, "", listOf(
            SectionView("custom", "Custom", listOf(FieldView("x", "Extra", "concealed", "FAKE-custom")))
        ))
    }
    override fun vaultExists(root: String) = true
    override fun createAccount(root: String, password: String) = "FAKE-key"
    override fun open(root: String, password: String, secretKey: String): OpenVault {
        require(password == "FAKE-password" && secretKey == "FAKE-key")
        return this
    }
    override fun listVaults() = listOf(VaultSummary("v", "Fake", null, records.size.toLong()))
    override fun listItems(vaultId: String) = records.values.map {
        ItemRow(it.id, "v", it.title, it.username.orEmpty(), it.url, it.category, it.tags, it.favorite, it.archived, it.deleted, it.totp != null)
    }
    override fun getItem(vaultId: String, itemId: String) = records.getValue(itemId)
    override fun createVault(name: String) = "v"
    override fun createItem(vaultId: String, input: ItemInput): String {
        records["created"] = ItemView("created", "v", input.title, input.username, input.password, input.url, input.notes, input.totp, input.category, input.tags, input.favorite, false, false, "", emptyList())
        return "created"
    }
    override fun updateItem(vaultId: String, itemId: String, input: ItemInput) {
        records[itemId] = records.getValue(itemId).copy(title = input.title, username = input.username, password = input.password,
            url = input.url, notes = input.notes, totp = input.totp, tags = input.tags, favorite = input.favorite,
            sections = input.sections.map { s -> SectionView(s.id, s.title, s.fields.map { FieldView(it.id, it.label, it.kind, it.value) }) })
    }
    override fun deleteItem(vaultId: String, itemId: String) { records[itemId] = records.getValue(itemId).copy(deleted = true) }
    override fun restoreItem(vaultId: String, itemId: String) { records[itemId] = records.getValue(itemId).copy(deleted = false) }
    override fun close() { }
}
