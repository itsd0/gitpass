package app.gitpass.android

import android.app.Activity
import android.content.Intent
import android.view.autofill.AutofillId
import android.view.autofill.AutofillManager
import android.widget.EditText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.android.autofill.AutofillAuthActivity
import app.gitpass.android.autofill.AutofillSaveActivity
import app.gitpass.android.autofill.AutofillTags
import app.gitpass.android.autofill.AutofillTarget
import app.gitpass.android.session.*
import app.gitpass.core.ItemInput
import java.io.File
import java.util.UUID
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Autofill unlock activities on a real device runtime (#475): nothing is
 * shown or returned before an unlock, matching logins come back as datasets,
 * a non-matching request returns nothing, the vault is locked again
 * afterwards, and a save request updates the matching login. The vault is
 * faked at the [OpenVault] boundary; Rust tests cover the real matching.
 */
@RunWith(AndroidJUnit4::class)
class AutofillUiTest {
    @get:Rule val rule = createEmptyComposeRule()
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private val app = context.applicationContext as GitpassApplication
    private lateinit var root: File
    private val gateway = AutofillGateway()

    @Before fun setUp() {
        root = File(context.cacheDir, "autofill-ui-${UUID.randomUUID()}").apply { mkdirs() }
        File(root, "format.json").writeText("{}")
        instrumentation.runOnMainSync { app.installForTest(gateway, root, nowMs = { 0L }) }
    }

    @After fun tearDown() {
        app.session.lock()
        app.autofillSaves.clear()
        root.deleteRecursively()
    }

    private fun ids(): Pair<AutofillId, AutofillId> {
        var pair: Pair<AutofillId, AutofillId>? = null
        instrumentation.runOnMainSync { pair = EditText(context).autofillId to EditText(context).autofillId }
        return pair!!
    }

    private fun fillIntent(target: AutofillTarget): Intent {
        val (u, p) = ids()
        return Intent(context, AutofillAuthActivity::class.java)
            .putExtra(AutofillAuthActivity.EXTRA_USERNAME_IDS, arrayOf(u))
            .putExtra(AutofillAuthActivity.EXTRA_PASSWORD_IDS, arrayOf(p))
            .putExtra(AutofillAuthActivity.EXTRA_WEB_ORIGIN, target.webOrigin)
            .putExtra(AutofillAuthActivity.EXTRA_PACKAGE, target.androidPackage)
    }

    private fun unlock() {
        rule.onNodeWithTag(AutofillTags.PASSWORD).performTextInput("FAKE-password")
        rule.onNodeWithTag(AutofillTags.SECRET_KEY).performTextInput("FAKE-key")
        rule.onNodeWithTag(AutofillTags.UNLOCK).performClick()
    }

    @Test fun matchingLoginIsReturnedOnlyAfterUnlockAndTheVaultLocksAgain() {
        AutofillAuthActivity.lastDatasetCount = -1
        AutofillAuthActivity.lastResultHadFillResponse = false
        // launch (not launchActivityForResult): the FillResponse's dataset list can be parceled only once, and
        // ActivityScenario re-parcels results; the autofill framework parcels it exactly once.
        ActivityScenario.launch<AutofillAuthActivity>(fillIntent(AutofillTarget("https://example.com", null))).use { scenario ->
            rule.onNodeWithText("Example").assertDoesNotExist()
            assertEquals(0, gateway.lookups)
            assertFalse(AutofillAuthActivity.lastResultHadFillResponse)
            unlock()
            rule.waitUntil(10_000) { AutofillAuthActivity.lastDatasetCount >= 0 }
            assertEquals(1, AutofillAuthActivity.lastDatasetCount)
            assertTrue("FillResponse delivered as the auth result", AutofillAuthActivity.lastResultHadFillResponse)
            rule.waitUntil(10_000) { scenario.state == Lifecycle.State.DESTROYED }
        }
        assertFalse("vault locked again after autofill", app.session.isUnlocked())
    }

    @Test fun mismatchedSiteReturnsNothing() {
        AutofillAuthActivity.lastDatasetCount = -1
        ActivityScenario.launchActivityForResult<AutofillAuthActivity>(fillIntent(AutofillTarget("https://example.com.evil.io", null))).use { scenario ->
            unlock()
            rule.waitUntil(10_000) { AutofillAuthActivity.lastDatasetCount >= 0 }
            assertEquals(0, AutofillAuthActivity.lastDatasetCount)
            rule.onNodeWithTag(AutofillTags.MESSAGE).assertExists()
            assertFalse(app.session.isUnlocked())
            rule.onNodeWithTag(AutofillTags.CANCEL).performClick()
            assertEquals(Activity.RESULT_CANCELED, scenario.result.resultCode)
        }
    }

    @Test fun requestWithoutTargetIsRefused() {
        val (u, p) = ids()
        val intent = Intent(context, AutofillAuthActivity::class.java)
            .putExtra(AutofillAuthActivity.EXTRA_USERNAME_IDS, arrayOf(u))
            .putExtra(AutofillAuthActivity.EXTRA_PASSWORD_IDS, arrayOf(p))
        ActivityScenario.launchActivityForResult<AutofillAuthActivity>(intent).use { scenario ->
            assertEquals(Activity.RESULT_CANCELED, scenario.result.resultCode)
        }
        assertEquals(0, gateway.lookups)
    }

    @Test fun saveUpdatesTheMatchingLoginAfterUnlock() {
        val token = app.autofillSaves.put(AutofillTarget("https://example.com", null), "alice", "new-FAKE")
        val intent = Intent(context, AutofillSaveActivity::class.java).putExtra(AutofillSaveActivity.EXTRA_TOKEN, token)
        ActivityScenario.launch<AutofillSaveActivity>(intent).use {
            rule.onNodeWithTag(AutofillTags.SAVE).assertDoesNotExist()
            unlock()
            rule.onNodeWithTag(AutofillTags.SAVE).performClick()
            rule.waitUntil(10_000) { gateway.password == "new-FAKE" }
        }
        assertFalse(app.session.isUnlocked())
        assertNull("token is single use", app.autofillSaves.take(token))
    }
}

private class AutofillGateway : VaultGateway, OpenVault {
    @Volatile var lookups = 0
    @Volatile var password: String? = "old-FAKE"
    override fun vaultExists(root: String) = true
    override fun createAccount(root: String, password: String) = "FAKE-key"
    override fun open(root: String, password: String, secretKey: String): OpenVault {
        require(password == "FAKE-password" && secretKey == "FAKE-key")
        return this
    }
    override fun listVaults() = listOf(VaultSummary("v1", "Fake", null, 1))
    override fun listItems(vaultId: String) = emptyList<ItemRow>()
    override fun getItem(vaultId: String, itemId: String) = ItemView("1", "v1", "Example", "alice", password, "https://example.com", null, null, "login", emptyList(), false, false, false, "", emptyList())
    override fun createVault(name: String) = "v1"
    override fun autofillLogins(target: AutofillTarget): List<AutofillCredential> {
        lookups++
        return if (target.webOrigin == "https://example.com") listOf(AutofillCredential("v1", "1", "Example", "alice", password)) else emptyList()
    }
    override fun updateItem(vaultId: String, itemId: String, input: ItemInput) { password = input.password }
    override fun close() {}
}
