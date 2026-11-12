package app.gitpass.android

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextContains
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
import app.gitpass.android.sync.KeystoreTokenCipher
import app.gitpass.android.ui.SyncTags
import app.gitpass.android.ui.TestTags
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Sync status flow on the real Compose screens (#455): not set up → settings
 * saved (token sealed by a test-only Android Keystore key) → automatic sync →
 * conflict report → "Sync now" failure with fixed text → lock. The engine is
 * faked at the [OpenVault] boundary: no network, no real repository; Rust
 * tests cover the real engine through the bridge.
 */
@RunWith(AndroidJUnit4::class)
class SyncStatusUiTest {
    @get:Rule val rule = createEmptyComposeRule()
    private val token = "github_pat_FAKE_UI_TOKEN_0123456789"

    @Test fun syncStatusSettingsConflictsFailureAndLock() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val app = context.applicationContext as GitpassApplication
        val root = File(context.cacheDir, "sync-ui-${UUID.randomUUID()}").apply { mkdirs() }
        File(root, "format.json").writeText("{}")
        val gateway = UiSyncGateway()
        val cipher = KeystoreTokenCipher("gitpass.sync.token.uitest")
        app.installForTest(gateway, root, nowMs = { 0L }, cipher = cipher)
        try {
            ActivityScenario.launch(MainActivity::class.java).use {
                rule.onNodeWithTag(TestTags.MASTER_PASSWORD).performTextInput("FAKE-password")
                rule.onNodeWithTag(TestTags.SECRET_KEY).performTextInput("FAKE-key")
                rule.onNodeWithTag(TestTags.UNLOCK).performClick()
                rule.waitUntil(10_000) { app.session.state.value.unlocked }
                rule.onNodeWithTag(SyncTags.STATUS).assertTextContains("Sync is not set up")
                assertEquals(0, gateway.syncs.get())

                rule.onNodeWithTag(SyncTags.OPEN).performClick()
                rule.onNodeWithTag(SyncTags.OWNER).performTextInput("FAKE-owner")
                rule.onNodeWithTag(SyncTags.REPO).performTextInput("fake-vault")
                rule.onNodeWithTag(SyncTags.BRANCH).performTextReplacement("main")
                rule.onNodeWithTag(SyncTags.TOKEN).performTextInput(token)
                rule.onNodeWithTag(SyncTags.SAVE).performScrollTo().performClick()
                rule.waitUntil(10_000) { gateway.syncs.get() == 1 && !app.session.isSyncing() }
                assertEquals(token, gateway.lastToken)
                assertFalse("token is sealed at rest", File(root.parentFile, "${root.name}.sync.json").readText().contains("FAKE_UI_TOKEN"))
                rule.onNodeWithText(token, substring = true).assertDoesNotExist()
                rule.waitUntil(10_000) { app.sync.status.value.conflicts.isNotEmpty() }
                rule.onNodeWithTag(SyncTags.STATUS).assertTextContains("1 conflict(s)", substring = true)
                rule.onNodeWithTag(SyncTags.CONFLICTS).performScrollTo().assertIsDisplayed()
                rule.onNodeWithText("v1/seed: remote deleted, local edited — kept local", substring = true).assertExists()

                gateway.failure = SyncFailedException("GitHub rejected the sync token (invalid or expired)")
                rule.onNodeWithTag(SyncTags.NOW).performScrollTo().performClick()
                rule.waitUntil(10_000) { gateway.syncs.get() == 2 && app.sync.status.value.error != null }
                rule.onNodeWithTag(SyncTags.ERROR).assertTextContains("GitHub rejected the sync token (invalid or expired)")
                rule.onNodeWithText(token, substring = true).assertDoesNotExist()

                rule.onNodeWithText("Back").performScrollTo().performClick()
                rule.onNodeWithTag(TestTags.LOCK).performClick()
                rule.onNodeWithTag(TestTags.MASTER_PASSWORD).assertIsDisplayed()
                assertFalse(app.session.isSyncing())
            }
        } finally {
            app.session.lock()
            app.syncSettings.clear()
            cipher.deleteKey()
            app.pendingSecret.clear()
            root.deleteRecursively()
        }
    }
}

private class UiSyncGateway : VaultGateway, OpenVault {
    val syncs = AtomicInteger()
    @Volatile var lastToken: String? = null
    @Volatile var failure: Exception? = null
    override fun vaultExists(root: String) = true
    override fun createAccount(root: String, password: String) = "FAKE-key"
    override fun open(root: String, password: String, secretKey: String): OpenVault {
        require(password == "FAKE-password" && secretKey == "FAKE-key")
        return this
    }
    override fun listVaults() = listOf(VaultSummary("v1", "Fake", null, 1))
    override fun listItems(vaultId: String) = listOf(ItemRow("seed", "v1", "Seed login", "owner", null, "login", emptyList(), false, false, false, false))
    override fun getItem(vaultId: String, itemId: String) = ItemView("seed", "v1", "Seed login", "owner", "FAKE-pw", null, null, null, "login", emptyList(), false, false, false, "", emptyList())
    override fun createVault(name: String) = "v1"
    override fun syncGitHub(target: SyncTarget, token: String, cancel: SyncCancel): SyncOutcome {
        syncs.incrementAndGet()
        lastToken = token
        failure?.let { throw it }
        return SyncOutcome(pushed = true, localChanged = false, conflicts = listOf("v1/seed: remote deleted, local edited — kept local"), mergedItems = 0, localOnlyItems = 1)
    }
    override fun close() {}
}
