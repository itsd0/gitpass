package app.gitpass.android

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.android.firstpull.*
import app.gitpass.android.session.FfiVaultGateway
import app.gitpass.android.session.VaultSession
import app.gitpass.core.createAccount
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class FirstPullUiTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val app get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as GitpassApplication
    private fun enter(tag: String, value: String) { compose.onNodeWithTag(tag).performScrollTo().performTextInput(value) }
    private fun fields(secret: String, password: String) {
        enter("pull-owner", "FAKE-owner"); enter("pull-repo", "FAKE-repo"); enter("pull-branch", "main")
        enter("pull-token", "FAKE-PAT"); enter("pull-password", password); enter("pull-secret", secret)
    }
    private fun start() = compose.onNodeWithTag("pull-start").performScrollTo().performClick()

    @Test fun credentialsAreMaskedAndDisappearAfterBackgroundCancelAndRecreation() {
        val directory = File(app.cacheDir, "fake-pull-ui-${UUID.randomUUID()}").apply { mkdirs() }
        try {
            app.installForTest(FfiVaultGateway(), File(directory, "vault"), VaultSession.ELAPSED_REALTIME)
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                compose.onNodeWithTag("first-pull").performClick()
                fields("FAKE-SecretKey", "FAKE-password")
                for (tag in listOf("pull-token", "pull-password", "pull-secret")) {
                    compose.onNodeWithTag(tag).assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Password))
                }
                scenario.moveToState(Lifecycle.State.CREATED)
                app.session.onHostBackground()
                scenario.moveToState(Lifecycle.State.RESUMED)
                compose.onNodeWithTag("pull-token").assertDoesNotExist()
                compose.onNodeWithTag("first-pull").performClick()
                compose.onNodeWithTag("pull-token").assert(SemanticsMatcher.expectValue(SemanticsProperties.EditableText, AnnotatedString("")))
                enter("pull-token", "FAKE-PAT")
                scenario.recreate()
                compose.onNodeWithTag("pull-token").assertDoesNotExist()
                compose.onNodeWithTag("first-pull").performClick()
                compose.onNodeWithTag("pull-token").assert(SemanticsMatcher.expectValue(SemanticsProperties.EditableText, AnnotatedString("")))
                enter("pull-token", "FAKE-PAT")
                compose.onNodeWithTag("pull-cancel").performScrollTo().performClick()
                compose.onNodeWithTag("first-pull").performClick()
                compose.onNodeWithTag("pull-token").assert(SemanticsMatcher.expectValue(SemanticsProperties.EditableText, AnnotatedString("")))
                assertEquals(File(directory, "vault"), app.vaultRoot)
            }
        } finally { app.session.lock(); directory.deleteRecursively(); File(directory.parentFile, "first-pull-test-${directory.name}").deleteRecursively() }
    }

    @Test fun nativeWrongCredentialsPreserveOldAccountThenAuthenticatedSnapshotSelectsSeparateRoot() {
        val directory = File(app.cacheDir, "fake-pull-native-${UUID.randomUUID()}").apply { mkdirs() }
        val legacy = File(directory, "vault")
        val password = "FAKE import fixture password"
        try {
            val secret = createAccount(legacy.absolutePath, password)
            val files = legacy.walkTopDown().filter { it.isFile && (it.extension == "enc" || it.name == "format.json" || it.name == "identity.pub") }
                .associate { it.relativeTo(legacy).invariantSeparatorsPath to it.readBytes() }
            val blobs = files.values.associateBy { EncryptedFirstPull.gitBlobSha(it) }
            val entries = files.map { (path, bytes) -> RemoteEntry(path, "blob", "100644", EncryptedFirstPull.gitBlobSha(bytes), bytes.size.toLong()) }
            val fake = object : FirstPullTransport {
                override fun resolve(selection: RepositorySelection) = "a".repeat(40)
                override fun tree(selection: RepositorySelection, commit: String) = RemoteTree(commit, false, entries)
                override fun blob(selection: RepositorySelection, sha: String) = blobs.getValue(sha)
            }
            app.installForTest(FfiVaultGateway(), legacy, VaultSession.ELAPSED_REALTIME)
            app.firstPullFactory = { FirstPullAttempt.forTest(fake) }
            ActivityScenario.launch(MainActivity::class.java).use {
                compose.onNodeWithTag("first-pull").performClick()
                fields(secret, "FAKE wrong password")
                start()
                compose.waitUntil(60_000) { compose.onAllNodesWithTag("pull-error").fetchSemanticsNodes().isNotEmpty() }
                assertEquals(legacy, app.vaultRoot)
                files.forEach { (path, bytes) -> assertArrayEquals(bytes, File(legacy, path).readBytes()) }
                assertTrue(app.accountRoots.staging.listFiles().orEmpty().isEmpty())
                // Owner/repo/branch remain explicit; all credentials were cleared after the failed attempt.
                enter("pull-token", "FAKE-PAT"); enter("pull-password", password); enter("pull-secret", secret)
                val revision = app.accountRevision.value
                start()
                compose.waitUntil(60_000) { app.accountRevision.value > revision }
                assertNotEquals(legacy, app.vaultRoot)
                assertFalse(app.session.isUnlocked())
                assertEquals(app.vaultRoot, app.accountRoots.selectedRoot())
                files.forEach { (path, bytes) -> assertArrayEquals(bytes, File(legacy, path).readBytes()) }
                compose.onNodeWithTag("pull-token").assertDoesNotExist()
                compose.onNodeWithTag("first-pull").performClick()
                compose.onNodeWithTag("pull-account-0").performScrollTo().performClick()
                compose.waitUntil(10_000) { app.vaultRoot == legacy }
                assertFalse(app.session.isUnlocked())
            }
        } finally {
            app.cancelFirstPull(); app.session.lock()
            app.accountRoots.staging.parentFile?.deleteRecursively() // explicitly fake test-only registry root
            directory.deleteRecursively()
        }
    }
}
