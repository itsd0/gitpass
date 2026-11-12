package app.gitpass.android

import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.SigningInfo
import android.credentials.CredentialOption
import android.os.Build
import android.service.credentials.CallingAppInfo
import android.service.credentials.CreateCredentialRequest
import android.service.credentials.CredentialProviderService
import android.service.credentials.GetCredentialRequest
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.credentials.CreatePublicKeyCredentialRequest
import androidx.credentials.GetPublicKeyCredentialOption
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.android.autofill.AutofillTags
import app.gitpass.android.passkey.AssetLinksSource
import app.gitpass.android.passkey.CallerIdentity
import app.gitpass.android.passkey.PasskeyActivity
import app.gitpass.android.passkey.PasskeyBroker
import app.gitpass.android.passkey.PasskeyCallers
import app.gitpass.android.passkey.PasskeyCreateActivity
import app.gitpass.android.passkey.PasskeyGetActivity
import app.gitpass.android.passkey.HttpsAssetLinks
import app.gitpass.android.session.*
import app.gitpass.core.PasskeyCaller
import app.gitpass.core.PasskeyEntry
import java.io.File
import java.util.UUID
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Passkey provider activities on Android 14+ (#477), driven with the same
 * framework request extras Credential Manager uses. The caller checks run in
 * the real native library; the vault is faked at the [OpenVault] boundary so
 * the test sees exactly when gitpass creates or signs. Nothing is created or
 * signed before the user is verified, mismatched callers are refused before
 * any unlock, and a vault unlocked for the request is locked again.
 */
@RunWith(AndroidJUnit4::class)
class PasskeyProviderUiTest {
    @get:Rule val rule = createEmptyComposeRule()
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private val app = context.applicationContext as GitpassApplication
    private lateinit var root: File
    private val gateway = PasskeyGateway()
    private val createJson = """{"rp":{"id":"example.com","name":"Example"},"user":{"id":"dXNlci0x","name":"alice"},"challenge":"Y3JlYXRl","pubKeyCredParams":[{"type":"public-key","alg":-7}]}"""
    private val getJson = """{"rpId":"example.com","challenge":"Z2V0"}"""
    private lateinit var signing: SigningInfo

    @Before fun setUp() {
        assumeTrue("Credential Manager providers need Android 14+", Build.VERSION.SDK_INT >= 34)
        root = File(context.cacheDir, "passkey-ui-${UUID.randomUUID()}").apply { mkdirs() }
        File(root, "format.json").writeText("{}")
        instrumentation.runOnMainSync { app.installForTest(gateway, root, nowMs = { 0L }) }
        signing = context.packageManager.getPackageInfo(context.packageName, PackageManager.GET_SIGNING_CERTIFICATES).signingInfo!!
        // Treat "com.android.chrome" signed with this test build's key as the privileged browser.
        val fps = (signing.apkContentsSigners.toList() + signing.signingCertificateHistory.orEmpty().toList()).map { s ->
            CallerIdentity.sha256(s.toByteArray()).joinToString(":") { "%02X".format(it) }
        }.distinct()
        val sigs = fps.joinToString(",") { """{"build":"release","cert_fingerprint_sha256":"$it"}""" }
        PasskeyCallers.allowlistOverride = """{"apps":[{"type":"android","info":{"package_name":"com.android.chrome","signatures":[$sigs]}}]}"""
        // Apps: a Digital Asset Links file that lists com.example.app with this build's key; never the network.
        val dal = """[{"relation":["delegate_permission/common.get_login_creds"],"target":{"namespace":"android_app","package_name":"com.example.app","sha256_cert_fingerprints":[${fps.joinToString(",") { "\"$it\"" }}]}}]"""
        app.passkeyBroker = PasskeyBroker(AssetLinksSource { host -> if (host == "example.com") dal else "" })
        PasskeyActivity.lastOutcome = null
    }

    @After fun tearDown() {
        if (Build.VERSION.SDK_INT < 34) return
        app.session.lock()
        app.passkeyGrants.clear()
        PasskeyCallers.allowlistOverride = null
        app.passkeyBroker = PasskeyBroker(HttpsAssetLinks())
        root.deleteRecursively()
    }

    private fun browser(origin: String) = CallingAppInfo("com.android.chrome", signing, origin)
    private fun appCaller(pkg: String) = CallingAppInfo(pkg, signing)

    private fun createIntent(caller: CallingAppInfo): Intent {
        val req = CreatePublicKeyCredentialRequest(createJson)
        return Intent(context, PasskeyCreateActivity::class.java)
            .putExtra(CredentialProviderService.EXTRA_CREATE_CREDENTIAL_REQUEST, CreateCredentialRequest(caller, req.type, req.credentialData))
    }

    private fun getIntent(caller: CallingAppInfo, credentialId: String = "Y3JlZA", grant: String? = null): Intent {
        val opt = GetPublicKeyCredentialOption(getJson)
        val option = CredentialOption.Builder(opt.type, opt.requestData, opt.candidateQueryData).build()
        return Intent(context, PasskeyGetActivity::class.java)
            .putExtra(CredentialProviderService.EXTRA_GET_CREDENTIAL_REQUEST, GetCredentialRequest(caller, listOf(option)))
            .putExtra(PasskeyGetActivity.EXTRA_CREDENTIAL_ID, credentialId)
            .putExtra(PasskeyGetActivity.EXTRA_GRANT, grant)
    }

    private fun unlock() {
        rule.waitUntil(10_000) { runCatching { rule.onNodeWithTag(AutofillTags.PASSWORD).assertExists() }.isSuccess }
        rule.onNodeWithTag(AutofillTags.PASSWORD).performTextInput("FAKE-password")
        rule.onNodeWithTag(AutofillTags.SECRET_KEY).performTextInput("FAKE-key")
        rule.onNodeWithTag(AutofillTags.UNLOCK).performClick()
    }

    private fun waitOutcome(expected: String) {
        rule.waitUntil(10_000) { PasskeyActivity.lastOutcome == expected }
    }

    @Test fun browserCreateHappensOnlyAfterUnlockAndRelocks() {
        ActivityScenario.launch<PasskeyCreateActivity>(createIntent(browser("https://login.example.com"))).use { scenario ->
            unlock()
            waitOutcome("created")
            assertEquals(1, gateway.registered)
            assertEquals(PasskeyCaller.Browser("https://login.example.com"), gateway.lastCaller)
            rule.waitUntil(10_000) { scenario.state == Lifecycle.State.DESTROYED }
        }
        assertFalse("vault locked again", app.session.isUnlocked())
    }

    @Test fun appListedInAssetLinksCanSignAfterUnlock() {
        ActivityScenario.launch<PasskeyGetActivity>(getIntent(appCaller("com.example.app"))).use {
            assertEquals(0, gateway.signed)
            unlock()
            waitOutcome("signed")
            assertEquals(1, gateway.signed)
            assertEquals("com.example.app", (gateway.lastCaller as PasskeyCaller.App).`package`)
        }
        assertFalse(app.session.isUnlocked())
    }

    @Test fun mismatchedRpOrUnlistedAppIsRefusedBeforeAnyUnlock() {
        for (intent in listOf(createIntent(browser("https://example.com.evil.io")), getIntent(appCaller("com.evil.app")))) {
            PasskeyActivity.lastOutcome = null
            val scenario = if (intent.component?.className == PasskeyCreateActivity::class.java.name) ActivityScenario.launch<PasskeyCreateActivity>(intent) else ActivityScenario.launch<PasskeyGetActivity>(intent)
            scenario.use {
                waitOutcome("refused")
                rule.onNodeWithTag(AutofillTags.MESSAGE).assertExists()
                rule.onNodeWithTag(AutofillTags.PASSWORD).assertDoesNotExist()
            }
        }
        assertEquals(0, gateway.registered + gateway.signed)
        assertFalse(app.session.isUnlocked())
    }

    @Test fun anOpenVaultStillNeedsAFreshVerification() {
        app.session.unlock(root.absolutePath, "FAKE-password", "FAKE-key")
        ActivityScenario.launch<PasskeyGetActivity>(getIntent(browser("https://example.com"))).use {
            rule.waitUntil(10_000) { runCatching { rule.onNodeWithTag(AutofillTags.PASSWORD).assertExists() }.isSuccess }
            assertEquals(0, gateway.signed)
            unlock()
            waitOutcome("signed")
            assertEquals(1, gateway.signed)
        }
        assertEquals("the user's own session stays open", true, app.session.isUnlocked())
    }

    @Test fun matchingGrantSignsWithoutASecondPromptAndRelocks() {
        app.session.unlock(root.absolutePath, "FAKE-password", "FAKE-key")
        val grant = app.passkeyGrants.issue(setOf("example.com"), "web:https://example.com", lockAfter = true)
        ActivityScenario.launch<PasskeyGetActivity>(getIntent(browser("https://example.com"), grant = grant)).use {
            waitOutcome("signed")
            rule.onNodeWithTag(AutofillTags.PASSWORD).assertDoesNotExist()
        }
        assertFalse("grant asked for the lock", app.session.isUnlocked())

        // A grant for another site does not skip the prompt.
        app.session.unlock(root.absolutePath, "FAKE-password", "FAKE-key")
        val other = app.passkeyGrants.issue(setOf("example.org"), "web:https://example.com", lockAfter = true)
        PasskeyActivity.lastOutcome = null
        ActivityScenario.launch<PasskeyGetActivity>(getIntent(browser("https://example.com"), grant = other)).use {
            rule.waitUntil(10_000) { runCatching { rule.onNodeWithTag(AutofillTags.PASSWORD).assertExists() }.isSuccess }
            assertEquals(1, gateway.signed)
        }
    }
}

private class PasskeyGateway : VaultGateway, OpenVault {
    @Volatile var registered = 0
    @Volatile var signed = 0
    @Volatile var lastCaller: PasskeyCaller? = null
    override fun vaultExists(root: String) = true
    override fun createAccount(root: String, password: String) = "FAKE-key"
    override fun open(root: String, password: String, secretKey: String): OpenVault {
        require(password == "FAKE-password" && secretKey == "FAKE-key")
        return this
    }
    override fun listVaults() = listOf(VaultSummary("v1", "Fake", null, 1))
    override fun listItems(vaultId: String) = emptyList<ItemRow>()
    override fun getItem(vaultId: String, itemId: String): ItemView = error("unused")
    override fun createVault(name: String) = "v1"
    override fun passkeyEntries(caller: PasskeyCaller, requestJson: String) = listOf(PasskeyEntry("Y3JlZA", "alice", "Fake", 0u))
    override fun passkeyRegister(caller: PasskeyCaller, requestJson: String, vaultId: String?): String {
        registered++; lastCaller = caller
        return """{"id":"Y3JlZA","rawId":"Y3JlZA","type":"public-key","response":{"clientDataJSON":"e30","attestationObject":"oA"},"clientExtensionResults":{}}"""
    }
    override fun passkeyAssert(caller: PasskeyCaller, requestJson: String, credentialId: String, clientDataHash: ByteArray?): String {
        signed++; lastCaller = caller
        return """{"id":"$credentialId","rawId":"$credentialId","type":"public-key","response":{"clientDataJSON":"e30","authenticatorData":"AA","signature":"AA","userHandle":"AA"},"clientExtensionResults":{}}"""
    }
    override fun close() {}
}
