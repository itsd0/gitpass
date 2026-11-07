package app.gitpass.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
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

@RunWith(AndroidJUnit4::class)
class SecretToolsUiTest {
    @get:Rule val rule = createEmptyComposeRule()

    @Test fun generatorCancelApplyAndLockAreExplicitAndNeverAutoSave() {
        withFakeApp { app, tools, gateway ->
            unlock(app)
            rule.onNodeWithText("New login").performClick()
            rule.onNodeWithTag("crud-title").performTextInput("Unsaved")
            rule.onNodeWithTag("crud-password").performTextInput("FAKE-original")
            rule.onNodeWithText("Password generator").performScrollTo().performClick()
            rule.waitUntil(10_000) { tools.generations > 0 }
            rule.onNodeWithText("Length: 20").assertIsDisplayed()
            rule.onNodeWithText("G".repeat(20)).assertDoesNotExist()
            rule.onNodeWithTag("generator-cancel").performClick()
            rule.onNodeWithTag("crud-password").assert(hasText("•".repeat(13)))
            assertEquals(0, gateway.writes)
            rule.onNodeWithTag("crud-save").performScrollTo().performClick()
            rule.waitUntil(10_000) { app.session.state.value.items.size == 3 }
            assertEquals("FAKE-original", gateway.lastSaved?.password)
            rule.onNodeWithText("Unsaved").performClick()
            rule.onNodeWithText("Edit login").performClick()
            rule.onNodeWithText("Password generator").performScrollTo().performClick()
            rule.waitUntil(10_000) { tools.generations > 1 }
            rule.onNodeWithText("+").performClick()
            rule.waitUntil(10_000) { tools.lastLength == 21 }
            rule.onNodeWithText("Regenerate").performClick()
            rule.waitUntil(10_000) { rule.onAllNodes(hasText("Use password") and isEnabled()).fetchSemanticsNodes().isNotEmpty() }
            rule.onNodeWithText("Use password").performClick()
            rule.onNodeWithTag("crud-password").assert(hasText("•".repeat(21)))
            assertEquals(1, gateway.writes) // applying did not write
            rule.onNodeWithTag("crud-save").performScrollTo().performClick()
            rule.waitUntil(10_000) { gateway.writes == 2 && app.session.state.value.detail == null }
            assertEquals("G".repeat(21), gateway.lastSaved?.password)
            rule.onNodeWithText("Unsaved").performClick()
            rule.onNodeWithText("Edit login").performClick()
            rule.onNodeWithText("Password generator").performScrollTo().performClick()
            InstrumentationRegistry.getInstrumentation().runOnMainSync { app.session.onHostBackground() }
            rule.onNodeWithTag(TestTags.MASTER_PASSWORD).assertIsDisplayed()
            rule.onNodeWithTag("generated-value").assertDoesNotExist()
            rule.onNodeWithTag("crud-password").assertDoesNotExist()
        }
    }

    @Test fun totpRolloverInvalidConfigurationAndBackgroundLockNeverShowSeed() {
        withFakeApp { app, tools, _ ->
            unlock(app)
            rule.onNodeWithText("OTP valid").performClick()
            rule.onNodeWithText(FIXTURE_SEED).assertDoesNotExist()
            rule.onNodeWithText("94287082").assertDoesNotExist()
            rule.onNodeWithText("Reveal").performClick()
            rule.waitUntil(10_000) { rule.onAllNodesWithText("94287082").fetchSemanticsNodes().isNotEmpty() }
            tools.wallSeconds = 60
            rule.waitUntil(10_000) { rule.onAllNodesWithText("37359152").fetchSemanticsNodes().isNotEmpty() }
            rule.onNodeWithText(FIXTURE_SEED).assertDoesNotExist()
            rule.onNodeWithText("Hide").performClick()
            rule.onNodeWithText("37359152").assertDoesNotExist()
            rule.onNodeWithText("Back").performClick()
            rule.onNodeWithText("OTP invalid").performClick()
            rule.onNodeWithText("Reveal").performClick()
            rule.waitUntil(10_000) { rule.onAllNodesWithText("Invalid TOTP configuration").fetchSemanticsNodes().isNotEmpty() }
            rule.onNodeWithText("FAKE-invalid-seed").assertDoesNotExist()
            InstrumentationRegistry.getInstrumentation().runOnMainSync { app.session.onHostBackground() }
            rule.onNodeWithTag(TestTags.MASTER_PASSWORD).assertIsDisplayed()
            rule.onNodeWithTag("live-totp").assertDoesNotExist()
        }
    }

    private fun unlock(app: GitpassApplication) {
        rule.onNodeWithTag(TestTags.MASTER_PASSWORD).performTextInput("FAKE-password")
        rule.onNodeWithTag(TestTags.SECRET_KEY).performTextInput("FAKE-key")
        rule.onNodeWithTag(TestTags.UNLOCK).performClick()
        rule.waitUntil(10_000) { app.session.state.value.unlocked }
    }

    private fun withFakeApp(test: (GitpassApplication, UiTools, UiToolGateway) -> Unit) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val app = context.applicationContext as GitpassApplication
        val root = File(context.cacheDir, "secret-tools-${UUID.randomUUID()}").apply { mkdirs() }
        File(root, "format.json").writeText("{}")
        val tools = UiTools()
        val gateway = UiToolGateway()
        app.installForTest(gateway, root, nowMs = { 0L }, secretTools = tools)
        try { ActivityScenario.launch(MainActivity::class.java).use { test(app, tools, gateway) } }
        finally { app.session.lock(); app.pendingSecret.clear(); root.deleteRecursively() }
    }
}

private const val FIXTURE_SEED = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
private class UiTools : SecretTools {
    @Volatile var wallSeconds = 59L
    @Volatile var generations = 0
    @Volatile var lastLength = 0
    override fun generate(length: Int): String { lastLength = length; generations++; return "G".repeat(length) }
    override fun totp(seed: String): LiveCode {
        require(seed == FIXTURE_SEED) { "FAKE-invalid-seed" }
        return if (wallSeconds == 59L) LiveCode("94287082", 1) else LiveCode("37359152", 30)
    }
}
private class UiToolGateway : VaultGateway, OpenVault {
    @Volatile var lastSaved: ItemInput? = null
    @Volatile var writes = 0
    override fun vaultExists(root: String) = true
    override fun createAccount(root: String, password: String) = "FAKE-key"
    override fun open(root: String, password: String, secretKey: String): OpenVault = this
    override fun listVaults() = listOf(VaultSummary("v", "Fake", null, 2))
    override fun listItems(vaultId: String) = listOf("valid", "invalid").map {
        ItemRow(it, "v", "OTP $it", "", null, "login", emptyList(), false, false, false, true)
    } + listOfNotNull(lastSaved?.let { ItemRow("created", "v", it.title, "", null, "login", emptyList(), false, false, false, false) })
    override fun createItem(vaultId: String, input: ItemInput): String { lastSaved = input; writes++; return "created" }
    override fun updateItem(vaultId: String, itemId: String, input: ItemInput) { lastSaved = input; writes++ }
    override fun getItem(vaultId: String, itemId: String): ItemView {
        if (itemId == "created") {
            val item = requireNotNull(lastSaved)
            return ItemView(itemId, "v", item.title, item.username, item.password, item.url, item.notes, item.totp, "login", emptyList(), false, false, false, "", emptyList())
        }
        return ItemView(itemId, "v", "OTP $itemId", null, null, null, null,
        if (itemId == "valid") FIXTURE_SEED else "FAKE-invalid-seed", "login", emptyList(), false, false, false, "", emptyList())
    }
    override fun createVault(name: String) = "v"
    override fun close() { }
}
