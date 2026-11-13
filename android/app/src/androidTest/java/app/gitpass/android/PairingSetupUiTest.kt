package app.gitpass.android

import android.Manifest
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.android.firstpull.*
import app.gitpass.android.session.FfiVaultGateway
import app.gitpass.android.session.VaultSession
import app.gitpass.android.ui.PairingScanInjector
import app.gitpass.core.GitpassException
import app.gitpass.core.ItemInput
import app.gitpass.core.PairingHostInput
import app.gitpass.core.createAccount
import app.gitpass.core.openVault
import app.gitpass.core.pairingHostAccept
import app.gitpass.core.pairingHostCancel
import app.gitpass.core.pairingHostRelease
import app.gitpass.core.pairingJoinerCode
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Set up with QR (#458) end to end on the device. The test plays the desktop through the bridge's host
 * role and injects scanned frames in place of the camera. Fake data only; no network.
 */
@RunWith(AndroidJUnit4::class)
class PairingSetupUiTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val app get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as GitpassApplication
    private val token = "github_pat_FAKE_android_pairing_ui_test_0123456789"

    @Before fun grantCamera() {
        // Granting up front avoids the system dialog; the emulator may still have no camera, which is fine.
        InstrumentationRegistry.getInstrumentation().uiAutomation
            .grantRuntimePermission(app.packageName, Manifest.permission.CAMERA)
    }

    private fun waitTag(tag: String) = compose.waitUntil(20_000) { compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty() }
    // A shared flow drops values with no collector, so wait until the scanner on screen is collecting.
    private fun inject(text: String) {
        compose.waitUntil(20_000) { PairingScanInjector.frames.subscriptionCount.value > 0 }
        assertTrue(PairingScanInjector.frames.tryEmit(text))
    }

    @Test fun pairsWithTheDesktopCodeThenPullsWithOnlyTheTypedMasterPassword() {
        val directory = File(app.cacheDir, "fake-pair-ui-${UUID.randomUUID()}").apply { mkdirs() }
        val hostDir = File(directory, "host").apply { mkdirs() }
        val legacy = File(directory, "vault")
        val password = "FAKE pairing fixture password"
        try {
            val secret = createAccount(legacy.absolutePath, password)
            openVault(legacy.absolutePath, password, secret).use { native ->
                val vault = native.createVault("FAKE paired vault", null)
                native.createItem(vault, ItemInput("FAKE paired login", "FAKE-user", "FAKE-password", null, null, null, "login", emptyList(), false, emptyList()))
            }
            val files = legacy.walkTopDown().filter { it.isFile && (it.extension == "enc" || it.name == "format.json" || it.name == "identity.pub") }
                .associate { it.relativeTo(legacy).invariantSeparatorsPath to it.readBytes() }
            val blobs = files.values.associateBy { EncryptedFirstPull.gitBlobSha(it) }
            val entries = files.map { (path, bytes) -> RemoteEntry(path, "blob", "100644", EncryptedFirstPull.gitBlobSha(bytes), bytes.size.toLong()) }
            val seen = mutableListOf<String>()
            val fake = object : FirstPullTransport {
                override fun resolve(selection: RepositorySelection): String {
                    seen += "${selection.owner}/${selection.repository}@${selection.branch}"
                    return "a".repeat(40)
                }
                override fun tree(selection: RepositorySelection, commit: String) = RemoteTree(commit, false, entries)
                override fun blob(selection: RepositorySelection, sha: String) = blobs.getValue(sha)
            }
            app.installForTest(FfiVaultGateway(), legacy, VaultSession.ELAPSED_REALTIME)
            app.firstPullFactory = { FirstPullAttempt.forTest(fake) }
            ActivityScenario.launch(MainActivity::class.java).use {
                compose.onNodeWithTag("first-pull").performClick()
                compose.onNodeWithTag("pair-start").performScrollTo().performClick()
                waitTag("pair-offer-qr")
                compose.onNodeWithTag("pair-remaining").assertExists()
                val offer = requireNotNull(PairingScanInjector.lastOffer)
                assertTrue(offer.startsWith("gitpass-pair:1:offer:"))

                // Desktop role: accept the offer with the sync details (the desktop never shows a code).
                val view = pairingHostAccept(PairingHostInput(hostDir.absolutePath, offer, UUID.randomUUID().toString(),
                    "FAKE-owner", "FAKE-repo", "main", secret, token))
                compose.onNodeWithTag("pair-next").performClick()
                inject("otpauth://totp/FAKE?secret=JBSWY3DPEHPK3PXP")
                waitTag("pair-hint")
                inject(view.announce)
                waitTag("pair-code")
                val shown = compose.onNodeWithTag("pair-code").fetchSemanticsNode()
                    .config[SemanticsProperties.Text].joinToString("") { it.text }.replace(" ", "")
                assertTrue(shown.matches(Regex("\\d{6}")))

                // The person types the phone's code on the desktop; only then is the grant released.
                val grant = pairingHostRelease(hostDir.absolutePath, shown)
                assertFalse(grant.contains(token)); assertFalse(grant.contains(secret))
                compose.onNodeWithTag("pair-next").performClick()
                inject(grant)
                waitTag("pair-password")
                compose.onNodeWithTag("pair-repo").assertTextContains("FAKE-owner/FAKE-repo", substring = true)
                compose.onNodeWithTag("pair-password").assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Password))
                // Neither the token nor the Secret Key is ever shown.
                assertTrue(compose.onAllNodesWithText(token, substring = true).fetchSemanticsNodes().isEmpty())
                assertTrue(compose.onAllNodesWithText(secret, substring = true).fetchSemanticsNodes().isEmpty())

                val revision = app.accountRevision.value
                compose.onNodeWithTag("pair-password").performTextInput(password)
                compose.onNodeWithTag("pair-finish").performClick()
                compose.waitUntil(60_000) { app.accountRevision.value > revision }
                assertEquals(listOf("FAKE-owner/FAKE-repo@main"), seen)
                assertNotEquals(legacy, app.vaultRoot)
                assertFalse("the new account starts locked", app.session.isUnlocked())
                val saved = requireNotNull(app.syncSettings.load())
                assertEquals("FAKE-owner", saved.target.owner)
                assertEquals("FAKE-repo", saved.target.repository)
                assertEquals(token, saved.token)
                // Nothing secret in the pairing replay record.
                File(app.pairingStateDir, ".gitpass-pairing-consumed.json").takeIf { it.exists() }?.readText()?.let {
                    assertFalse(it.contains(token)); assertFalse(it.contains(secret))
                }
            }
        } finally {
            runCatching { pairingHostCancel(hostDir.absolutePath) }
            app.cancelFirstPull(); app.session.lock()
            app.accountRoots.staging.parentFile?.deleteRecursively() // explicitly fake test-only registry root
            directory.deleteRecursively()
        }
    }

    @Test fun cancellingForgetsTheEphemeralKey() {
        val directory = File(app.cacheDir, "fake-pair-cancel-${UUID.randomUUID()}").apply { mkdirs() }
        val hostDir = File(directory, "host").apply { mkdirs() }
        val legacy = File(directory, "vault")
        try {
            val secret = createAccount(legacy.absolutePath, "FAKE cancel password")
            app.installForTest(FfiVaultGateway(), legacy, VaultSession.ELAPSED_REALTIME)
            ActivityScenario.launch(MainActivity::class.java).use {
                compose.onNodeWithTag("first-pull").performClick()
                compose.onNodeWithTag("pair-start").performScrollTo().performClick()
                waitTag("pair-offer-qr")
                val offer = requireNotNull(PairingScanInjector.lastOffer)
                val view = pairingHostAccept(PairingHostInput(hostDir.absolutePath, offer, UUID.randomUUID().toString(),
                    "FAKE-owner", "FAKE-repo", "main", secret, token))
                compose.onNodeWithTag("pair-cancel").performScrollTo().performClick()
                compose.onNodeWithTag("pair-offer-qr").assertDoesNotExist()
                val refused = runCatching { pairingJoinerCode(view.announce) }.exceptionOrNull()
                assertTrue("the phone's ephemeral key is gone after cancel", refused is GitpassException)
            }
        } finally {
            runCatching { pairingHostCancel(hostDir.absolutePath) }
            app.session.lock()
            directory.deleteRecursively()
        }
    }
}
