package app.gitpass.android.firstpull

import java.io.File
import java.nio.file.Files
import org.junit.Assert.*
import org.junit.Test

class EncryptedFirstPullTest {
    private val selection = RepositorySelection("FAKE-owner", "fake-vault", "main")
    private val commit = "a".repeat(40)
    private val bytes = "FAKE encrypted bytes".toByteArray()
    private fun entry(path: String) = RemoteEntry(path, "blob", "100644", EncryptedFirstPull.gitBlobSha(bytes), bytes.size.toLong())
    private fun entries() = listOf("format.json", "keyset.enc", "manifest.enc").map(::entry)
    private fun run(tree: RemoteTree, blob: () -> ByteArray = { bytes }, cancelled: () -> Boolean = { false }, inspect: (File, File) -> Unit = { _, _ -> }) {
        val parent = Files.createTempDirectory("fake-first-pull").toFile()
        val active = File(parent, "active").apply { mkdir() }
        File(active, "FAKE-original").writeText("unchanged")
        val staging = File(parent, "staging")
        val transport = object : FirstPullTransport {
            override fun resolve(selection: RepositorySelection) = commit
            override fun tree(selection: RepositorySelection, commit: String) = tree
            override fun blob(selection: RepositorySelection, sha: String) = blob()
        }
        try {
            EncryptedFirstPull(staging, transport).stage(selection, cancelled).use { inspect(it.directory, active) }
        } finally {
            assertEquals("unchanged", File(active, "FAKE-original").readText())
            assertTrue(staging.listFiles().orEmpty().isEmpty())
            parent.deleteRecursively()
        }
    }
    private fun rejects(tree: RemoteTree, blob: () -> ByteArray = { bytes }) {
        try { run(tree, blob); fail("unsafe snapshot accepted") } catch (_: FirstPullFailure) { }
    }
    @Test fun pinnedEncryptedCopyNeverChangesActiveAccountAndCloseCleansUp() {
        run(RemoteTree(commit, false, entries())) { directory, _ ->
            assertArrayEquals(bytes, File(directory, "manifest.enc").readBytes())
        }
    }
    @Test fun unsafePathsModesDuplicatesAndUnknownFilesAreRejected() {
        for (path in listOf("../escape", "/absolute", "a//b", "vaults/../x", "a\\b", ".git/config", ".gitpass-state.json", "unknown.enc")) {
            rejects(RemoteTree(commit, false, entries() + entry(path)))
        }
        rejects(RemoteTree(commit, false, entries() + entry("format.json")))
        rejects(RemoteTree(commit, false, entries() + RemoteEntry("README.md", "blob", "120000", "a".repeat(40), 1)))
        rejects(RemoteTree(commit, false, entries() + RemoteEntry("vaults", "commit", "160000", "a".repeat(40), 1)))
    }
    @Test fun truncatedTreesCommitMismatchMissingRequiredAndExcessSizeFailClosed() {
        rejects(RemoteTree(commit, true, entries()))
        rejects(RemoteTree("b".repeat(40), false, entries()))
        rejects(RemoteTree(commit, false, entries().drop(1)))
        rejects(RemoteTree(commit, false, entries() + RemoteEntry("devices.enc", "blob", "100644", "a".repeat(40), EncryptedFirstPull.MAX_FILE_BYTES.toLong() + 1)))
    }
    @Test fun fileCountAndAggregateSizeAreBoundedBeforeDownload() {
        rejects(RemoteTree(commit, false, List(EncryptedFirstPull.MAX_ENTRIES + 1) { entry("format.json") }))
        val files = (1..17).map {
            val id = "00000000-0000-0000-0000-" + it.toString().padStart(12, '0')
            RemoteEntry("vaults/$id/meta.enc", "blob", "100644", "a".repeat(40), EncryptedFirstPull.MAX_FILE_BYTES.toLong())
        }
        rejects(RemoteTree(commit, false, entries() + files))
    }
    @Test fun blobHashAndLengthAreVerified() {
        rejects(RemoteTree(commit, false, entries())) { "tampered".toByteArray() }
        rejects(RemoteTree(commit, false, entries())) { ByteArray(bytes.size) }
    }
    @Test fun cancellationAndTransportFailureCleanAllStagingAndRedactErrors() {
        try { run(RemoteTree(commit, false, entries()), cancelled = { true }); fail("cancel accepted") } catch (_: FirstPullFailure) { }
        var fetched = 0
        try { run(RemoteTree(commit, false, entries()), blob = {
            fetched++
            if (fetched > 1) error("FAKE-sensitive-token")
            bytes
        }); fail("transport accepted") }
        catch (e: FirstPullFailure) { assertFalse(e.message.orEmpty().contains("FAKE-sensitive-token")) }
    }
    @Test fun selectionCannotInjectUrlOrEscapePaths() {
        for (owner in listOf("https://evil.invalid", "../owner", "owner@evil")) {
            try { RepositorySelection(owner, "fake", "main"); fail("bad owner") } catch (_: IllegalArgumentException) { }
        }
        for (branch in listOf("../main", "main//nested", "main.lock", "main?token=FAKE")) {
            try { RepositorySelection("fake", "fake", branch); fail("bad branch") } catch (_: IllegalArgumentException) { }
        }
    }
    @Test fun processDeathCleanupOnlyRemovesOwnedStageNames() {
        val parent = Files.createTempDirectory("fake-cleanup").toFile()
        try {
            val stage = File(parent, "pull-12345678-1234-1234-1234-123456789abc").apply { mkdir() }
            File(stage, "fake.enc").writeText("FAKE")
            val account = File(parent, "active-account").apply { mkdir() }
            EncryptedFirstPull.cleanupOwnedStages(parent)
            assertFalse(stage.exists())
            assertTrue(account.exists())
        } finally { parent.deleteRecursively() }
    }
}
