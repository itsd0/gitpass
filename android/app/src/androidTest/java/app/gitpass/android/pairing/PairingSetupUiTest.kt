package app.gitpass.android.pairing

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.android.GitpassApplication
import app.gitpass.android.MainActivity
import app.gitpass.android.R
import app.gitpass.android.firstpull.FirstPullAttempt
import app.gitpass.android.firstpull.FirstPullFailure
import app.gitpass.android.firstpull.FirstPullTransport
import app.gitpass.android.firstpull.RemoteTree
import app.gitpass.android.firstpull.RepositorySelection
import app.gitpass.android.session.FfiVaultGateway
import app.gitpass.android.session.VaultSession
import app.gitpass.android.ui.PairingScanInjector
import app.gitpass.core.PairingHostInput
import app.gitpass.core.pairingHostBegin
import app.gitpass.core.pairingHostCancel
import app.gitpass.core.pairingJoinerCancel
import java.io.File
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.util.UUID
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PairingSetupUiTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val app get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as GitpassApplication
    private val password = "FAKE-master-password-ok"

    @Before fun quietCamera() {
        PairingScanInjector.reset()
        PairingScanInjector.cameraEnabled = false
        PairingClients.reset()
    }

    @After fun restore() {
        PairingScanInjector.reset()
        PairingClients.reset()
        app.firstPullFactory = ::FirstPullAttempt
        runCatching { pairingJoinerCancel() }
    }

    @Test fun cancelLeavesTheSetupScreen() {
        val directory = tempVault()
        try {
            app.installForTest(FfiVaultGateway(), File(directory, "vault"), VaultSession.ELAPSED_REALTIME)
            ActivityScenario.launch(MainActivity::class.java).use {
                compose.onNodeWithTag("pair-setup").performClick()
                compose.onNodeWithTag("pair-scan").assertExists()
                compose.onNodeWithTag("pair-cancel").performClick()
                compose.onNodeWithTag("pair-setup").assertExists()
                compose.onNodeWithTag("pair-scan").assertDoesNotExist()
            }
        } finally {
            app.session.lock()
            directory.deleteRecursively()
        }
    }

    @Test fun aRefusedConnectionShowsTheNetworkSentenceWithoutTheAddress() {
        val directory = tempVault()
        val hostDir = File(directory, "host").apply { mkdirs() }
        val port = ServerSocket().use { socket ->
            socket.bind(InetSocketAddress("127.0.0.1", 0))
            socket.localPort
        }
        val input = PairingHostInput(
            hostDir.absolutePath,
            "00000000-0000-4000-8000-000000000001",
            "fake-owner",
            "fake-vault",
            "main",
            PAIRING_TEST_SECRET,
            PAIRING_TEST_TOKEN,
            "[\"127.0.0.1\"]",
            port.toUShort(),
        )
        val qr = pairingHostBegin(input).qr
        input.close()
        try {
            app.installForTest(FfiVaultGateway(), File(directory, "vault"), VaultSession.ELAPSED_REALTIME)
            ActivityScenario.launch(MainActivity::class.java).use {
                compose.onNodeWithTag("pair-setup").performClick()
                compose.waitUntil(5_000) { PairingScanInjector.frames.subscriptionCount.value > 0 }
                assertTrue(PairingScanInjector.frames.tryEmit(qr))
                val context = InstrumentationRegistry.getInstrumentation().targetContext
                val connect = context.getString(R.string.pair_connect)
                val timedOut = context.getString(R.string.pair_timeout)
                compose.waitUntil(15_000) {
                    compose.onAllNodesWithText(connect).fetchSemanticsNodes().isNotEmpty() ||
                        compose.onAllNodesWithText(timedOut).fetchSemanticsNodes().isNotEmpty()
                }
                assertEquals(0, compose.onAllNodesWithText("127.0.0.1", substring = true).fetchSemanticsNodes().size)
                assertEquals(0, compose.onAllNodesWithText("github_pat_", substring = true).fetchSemanticsNodes().size)
                compose.onNodeWithTag("pair-cancel").performClick()
            }
        } finally {
            runCatching { pairingHostCancel(hostDir.absolutePath) }
            app.session.lock()
            directory.deleteRecursively()
        }
    }

    @Test fun scanShowsTheCodeThenThePasswordAndAFailedPullStaysFixed() {
        val directory = tempVault()
        val hostDir = File(directory, "host")
        try {
            app.installForTest(FfiVaultGateway(), File(directory, "vault"), VaultSession.ELAPSED_REALTIME)
            app.firstPullFactory = {
                FirstPullAttempt.forTest(object : FirstPullTransport {
                    override fun resolve(selection: RepositorySelection) = throw FirstPullFailure("SENTINEL-must-not-appear")
                    override fun tree(selection: RepositorySelection, commit: String): RemoteTree = throw FirstPullFailure("SENTINEL-must-not-appear")
                    override fun blob(selection: RepositorySelection, sha: String) = throw FirstPullFailure("SENTINEL-must-not-appear")
                })
            }
            PairingHostPeer(hostDir).use { peer ->
                ActivityScenario.launch(MainActivity::class.java).use {
                    compose.onNodeWithTag("pair-setup").performClick()
                    compose.waitUntil(5_000) { PairingScanInjector.frames.subscriptionCount.value > 0 }
                    assertTrue(PairingScanInjector.frames.tryEmit(peer.qr))
                    compose.waitUntil(20_000) { compose.onAllNodesWithTag("pair-code").fetchSemanticsNodes().isNotEmpty() }
                    val typed = shownCode()
                    assertEquals(6, typed.length)
                    peer.confirm(typed)
                    compose.waitUntil(20_000) { compose.onAllNodesWithTag("pair-password").fetchSemanticsNodes().isNotEmpty() }
                    compose.onNodeWithTag("pair-repo").assertExists()
                    compose.onNodeWithTag("pair-password").performScrollTo().assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Password))
                    compose.onNodeWithTag("pair-password").performTextInput(password)
                    compose.onNodeWithTag("pair-finish").performScrollTo().performClick()
                    val failed = InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.pair_pull_failed)
                    compose.waitUntil(20_000) { compose.onAllNodesWithText(failed).fetchSemanticsNodes().isNotEmpty() }
                    assertEquals(0, compose.onAllNodesWithText(password, substring = true).fetchSemanticsNodes().size)
                    assertEquals(0, compose.onAllNodesWithText("SENTINEL-must-not-appear", substring = true).fetchSemanticsNodes().size)
                    assertEquals(0, compose.onAllNodesWithText("github_pat_", substring = true).fetchSemanticsNodes().size)
                    val wire = peer.wireText()
                    assertFalse(
                        "wire has a secret",
                        wire.contains(PAIRING_TEST_TOKEN) || wire.contains(PAIRING_TEST_SECRET) || wire.contains(password) || wire.contains("master_password"),
                    )
                    assertNullPeer(peer.failure)
                }
            }
        } finally {
            app.session.lock()
            directory.deleteRecursively()
        }
    }

    private fun shownCode(): String {
        val nodes = compose.onAllNodesWithTag("pair-code").fetchSemanticsNodes()
        assertFalse("code missing", nodes.isEmpty())
        val text = nodes.first().config[SemanticsProperties.Text].joinToString("") { it.text }
        return text.filter { it.isDigit() }
    }

    private fun tempVault() = File(
        InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
        "fake-pair-ui-${UUID.randomUUID()}",
    ).apply { mkdirs() }

    private fun assertNullPeer(value: String?) {
        assertEquals(null, value)
    }
}
