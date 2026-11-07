package app.gitpass.android

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.android.session.ItemRow
import app.gitpass.android.session.ItemView
import app.gitpass.android.session.OpenVault
import app.gitpass.android.session.VaultGateway
import app.gitpass.android.session.VaultSession
import app.gitpass.android.session.VaultSummary
import app.gitpass.android.ui.TestTags
import java.io.File
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class VaultLifecycleRegressionTest {

    @get:Rule
    val composeTestRule = createEmptyComposeRule()

    private lateinit var app: GitpassApplication
    private lateinit var testDir: File
    private lateinit var fakeGateway: RegressionFakeGateway
    private var simulatedTimeMs = 100_000L

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        app = context.applicationContext as GitpassApplication
        testDir = File(context.cacheDir, "test-vault-${UUID.randomUUID()}")
        testDir.mkdirs()
        fakeGateway = RegressionFakeGateway(testDir)
        simulatedTimeMs = 100_000L
        app.installForTest(
            gateway = fakeGateway,
            root = testDir,
            nowMs = { simulatedTimeMs },
        )
    }

    @After
    fun tearDown() {
        if (this::app.isInitialized) {
            app.session.lock()
            app.pendingSecret.clear()
        }
        if (this::testDir.isInitialized) {
            testDir.deleteRecursively()
        }
    }

    @Test
    fun revealDetail_backgroundApp_resume_showsLockedUiWithNoDetailVisible() {
        // Prepare an existing vault with one item containing a secret password
        fakeGateway.prepareExistingVault(
            password = "correct-horse-battery",
            secretKey = "A3-REGRESSION-KEY",
            itemPassword = "super-secret-plaintext",
        )

        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            composeTestRule.waitForIdle()

            // 1. Unlock screen: fill Master Password and Secret Key
            composeTestRule.onNodeWithTag(TestTags.MASTER_PASSWORD)
                .performTextInput("correct-horse-battery")
            composeTestRule.onNodeWithTag(TestTags.SECRET_KEY)
                .performTextInput("A3-REGRESSION-KEY")
            composeTestRule.onNodeWithTag(TestTags.UNLOCK)
                .performClick()
            composeTestRule.waitForIdle()

            // Verify item list is shown
            composeTestRule.onNodeWithText("Login Item").assertIsDisplayed()

            // 2. Open item detail and reveal secret field
            composeTestRule.onNodeWithText("Login Item").performClick()
            composeTestRule.waitForIdle()
            composeTestRule.onNodeWithText("••••••").assertIsDisplayed()
            composeTestRule.onNodeWithText("Reveal").performClick()
            composeTestRule.waitForIdle()
            composeTestRule.onNodeWithText("super-secret-plaintext").assertIsDisplayed()

            // 3. Background the app (simulate leaving the foreground)
            scenario.moveToState(Lifecycle.State.CREATED)
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                app.session.onHostBackground()
            }
            composeTestRule.waitForIdle()

            // 4. Resume the app
            scenario.moveToState(Lifecycle.State.RESUMED)
            composeTestRule.waitForIdle()

            // Verify: UI MUST immediately show the locked UnlockScreen with NO detail visible
            composeTestRule.onNodeWithTag(TestTags.MASTER_PASSWORD).assertIsDisplayed()
            composeTestRule.onNodeWithText("super-secret-plaintext").assertDoesNotExist()
            composeTestRule.onNodeWithText("Login Item").assertDoesNotExist()
            assertFalse(app.session.isUnlocked())

            // 5. Test manual locking: unlock, reveal, and tap Lock
            composeTestRule.onNodeWithTag(TestTags.MASTER_PASSWORD)
                .performTextInput("correct-horse-battery")
            composeTestRule.onNodeWithTag(TestTags.SECRET_KEY)
                .performTextInput("A3-REGRESSION-KEY")
            composeTestRule.onNodeWithTag(TestTags.UNLOCK)
                .performClick()
            composeTestRule.waitForIdle()
            composeTestRule.onNodeWithText("Login Item").performClick()
            composeTestRule.waitForIdle()
            composeTestRule.onNodeWithText("Reveal").performClick()
            composeTestRule.waitForIdle()
            composeTestRule.onNodeWithText("super-secret-plaintext").assertIsDisplayed()

            composeTestRule.onNodeWithTag(TestTags.LOCK).performClick()
            composeTestRule.waitForIdle()
            composeTestRule.onNodeWithTag(TestTags.MASTER_PASSWORD).assertIsDisplayed()
            composeTestRule.onNodeWithText("super-secret-plaintext").assertDoesNotExist()
            assertFalse(app.session.isUnlocked())

            // 6. Test inactivity locking: unlock, reveal, advance monotonic clock past 5 min
            composeTestRule.onNodeWithTag(TestTags.MASTER_PASSWORD)
                .performTextInput("correct-horse-battery")
            composeTestRule.onNodeWithTag(TestTags.SECRET_KEY)
                .performTextInput("A3-REGRESSION-KEY")
            composeTestRule.onNodeWithTag(TestTags.UNLOCK)
                .performClick()
            composeTestRule.waitForIdle()
            composeTestRule.onNodeWithText("Login Item").performClick()
            composeTestRule.waitForIdle()
            composeTestRule.onNodeWithText("Reveal").performClick()
            composeTestRule.waitForIdle()
            composeTestRule.onNodeWithText("super-secret-plaintext").assertIsDisplayed()

            simulatedTimeMs += VaultSession.DEFAULT_AUTO_LOCK_MS + 1000
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                app.session.enforceInactivity()
            }
            composeTestRule.waitForIdle()
            composeTestRule.onNodeWithTag(TestTags.MASTER_PASSWORD).assertIsDisplayed()
            composeTestRule.onNodeWithText("super-secret-plaintext").assertDoesNotExist()
            assertFalse(app.session.isUnlocked())
        }
    }

    @Test
    fun accountCreation_recreateActivityDuringCreation_preservesKeyHandoffAndAllowsUnlock() {
        // Vault does not exist yet (first run)
        fakeGateway.ensureNoVault()

        fakeGateway.creationEntered = CountDownLatch(1)
        fakeGateway.creationRelease = CountDownLatch(1)

        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            composeTestRule.waitForIdle()

            // 1. Enter matching Master Password and confirm password
            val testPassword = "new-secure-master-password"
            composeTestRule.onNodeWithTag(TestTags.MASTER_PASSWORD)
                .performTextInput(testPassword)
            composeTestRule.onNodeWithTag(TestTags.CONFIRM_PASSWORD)
                .performTextInput(testPassword)

            // 2. Click Create Account
            composeTestRule.onNodeWithTag(TestTags.CREATE_ACCOUNT)
                .performClick()

            // Wait until account creation is running on IO thread
            assertTrue(fakeGateway.creationEntered.await(5, TimeUnit.SECONDS))

            // 3. Recreate the Activity during creation
            scenario.recreate()
            composeTestRule.waitForIdle()

            // Release creation IO to complete
            fakeGateway.creationRelease.countDown()
            composeTestRule.waitForIdle()

            // 4. Verify the SecretKeyGate is displayed with the pending Secret Key
            composeTestRule.onNodeWithTag(TestTags.SECRET_KEY_VALUE).assertIsDisplayed()
            val handedKey = app.pendingSecret.get()
            assertNotNull("Pending secret must be established in-memory across recreate", handedKey)
            assertEquals("A3-NEW-GENERATED-KEY", handedKey)
            composeTestRule.onNodeWithText("A3-NEW-GENERATED-KEY").assertIsDisplayed()

            // 5. Confirm saved key
            composeTestRule.onNodeWithTag(TestTags.I_SAVED_IT).performClick()
            composeTestRule.waitForIdle()

            // 6. Test that the created account can be unlocked with the handed-off key
            app.session.lock()
            composeTestRule.waitForIdle()

            composeTestRule.onNodeWithTag(TestTags.MASTER_PASSWORD)
                .performTextInput(testPassword)
            composeTestRule.onNodeWithTag(TestTags.SECRET_KEY)
                .performTextInput(handedKey!!)
            composeTestRule.onNodeWithTag(TestTags.UNLOCK)
                .performClick()
            composeTestRule.waitForIdle()

            assertTrue("Created account must successfully unlock with the handed key", app.session.isUnlocked())
        }
    }
}

private class RegressionFakeGateway(private val rootDir: File) : VaultGateway {
    var hasVault = false
    var expectedPassword = ""
    var expectedSecretKey = ""
    var itemPlaintext = ""

    var creationEntered = CountDownLatch(0)
    var creationRelease = CountDownLatch(0)

    fun prepareExistingVault(password: String, secretKey: String, itemPassword: String) {
        hasVault = true
        expectedPassword = password
        expectedSecretKey = secretKey
        itemPlaintext = itemPassword
        File(rootDir, "format.json").writeText("{}")
    }

    fun ensureNoVault() {
        hasVault = false
        File(rootDir, "format.json").delete()
    }

    override fun vaultExists(root: String): Boolean = hasVault && File(rootDir, "format.json").isFile

    override fun createAccount(root: String, password: String): String {
        creationEntered.countDown()
        creationRelease.await(5, TimeUnit.SECONDS)
        hasVault = true
        expectedPassword = password
        expectedSecretKey = "A3-NEW-GENERATED-KEY"
        itemPlaintext = "initial-secret"
        File(rootDir, "format.json").writeText("{}")
        return expectedSecretKey
    }

    override fun open(root: String, password: String, secretKey: String): OpenVault {
        if (!hasVault) throw IllegalStateException("Vault does not exist")
        if (password != expectedPassword || secretKey != expectedSecretKey) {
            throw IllegalStateException("Wrong password or secret key")
        }
        return RegressionOpenVault(itemPlaintext)
    }
}

private class RegressionOpenVault(private val itemPassword: String) : OpenVault {
    override fun listVaults(): List<VaultSummary> =
        listOf(VaultSummary(id = "v1", name = "Personal", description = null, itemCount = 1))

    override fun listItems(vaultId: String): List<ItemRow> = listOf(
        ItemRow(
            id = "item-1",
            vaultId = vaultId,
            title = "Login Item",
            subtitle = "alice@example.invalid",
            url = "https://example.invalid",
            category = "login",
            tags = listOf("finance"),
            favorite = true,
            archived = false,
            deleted = false,
            hasTotp = false,
        ),
    )

    override fun getItem(vaultId: String, itemId: String): ItemView = ItemView(
        id = itemId,
        vaultId = vaultId,
        title = "Login Item",
        username = "alice@example.invalid",
        password = itemPassword,
        url = "https://example.invalid",
        notes = "test notes",
        totp = null,
        category = "login",
        tags = listOf("finance"),
        favorite = true,
        archived = false,
        deleted = false,
        updatedAt = "1",
        sections = emptyList(),
    )

    override fun createVault(name: String): String = "v1"

    override fun close() {}
}
