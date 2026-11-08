package app.gitpass.android.firstpull

import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.util.UUID

class AccountRootsTest {
    private fun staged(roots: AccountRoots): StagedEncryptedCopy {
        val directory = File(roots.staging, "pull-${UUID.randomUUID()}")
        directory.mkdirs()
        File(directory, "FAKE.enc").writeText("FAKE encrypted fixture")
        return StagedEncryptedCopy(directory, "a".repeat(40))
    }
    @Test fun selectionIsSeparateAtomicAndLegacyRemainsAvailableAcrossRestart() {
        val base = Files.createTempDirectory("fake-first-pull").toFile()
        try {
            val legacy = File(base, "vault").apply { mkdirs() }
            val sentinel = File(legacy, "FAKE.enc").apply { writeText("FAKE original bytes") }
            val roots = AccountRoots(base)
            assertEquals(legacy, roots.selectedRoot())
            val copy = staged(roots)
            roots.prepare(copy) {}
            val imported = roots.promote(copy) {}
            copy.close()
            assertTrue(File(imported, "FAKE.enc").isFile)
            assertEquals("FAKE original bytes", sentinel.readText())
            val restarted = AccountRoots(base)
            restarted.startupCleanup()
            assertEquals(imported, restarted.selectedRoot())
            assertEquals(2, restarted.accounts().size)
            assertEquals(legacy, restarted.select("legacy"))
            assertTrue(imported.exists())
            assertEquals("FAKE original bytes", sentinel.readText())
        } finally { base.deleteRecursively() }
    }
    @Test fun cancellationAfterRenamePreservesOldSelectionAndRemovesUncommittedImport() {
        val base = Files.createTempDirectory("fake-first-pull").toFile()
        try {
            val roots = AccountRoots(base)
            val copy = staged(roots)
            roots.prepare(copy) {}
            var checks = 0
            try {
                roots.promote(copy) { if (++checks == 3) throw FirstPullFailure("FAKE cancellation") }
                fail("Must cancel before index replacement")
            } catch (_: FirstPullFailure) {}
            copy.close()
            assertEquals(File(base, "vault"), roots.selectedRoot())
            assertTrue(File(base, "imported-accounts").listFiles().orEmpty().isEmpty())
        } finally { base.deleteRecursively() }
    }
    @Test fun processDeathCleanupOnlyDeletesOwnedUnregisteredFolders() {
        val base = Files.createTempDirectory("fake-first-pull").toFile()
        try {
            val roots = AccountRoots(base)
            val registered = roots.promote(staged(roots)) {}
            val orphan = File(base, "imported-accounts/import-${UUID.randomUUID()}").apply { mkdirs() }
            val unknown = File(base, "imported-accounts/not-owned").apply { mkdirs() }
            val incomplete = staged(roots)
            AccountRoots(base).startupCleanup()
            assertFalse(orphan.exists())
            assertFalse(incomplete.directory.exists())
            assertTrue(registered.exists())
            assertTrue(unknown.exists())
            assertEquals(registered, roots.selectedRoot())
        } finally { base.deleteRecursively() }
    }
    @Test fun corruptIndexNeverTriggersDestructiveFallback() {
        val base = Files.createTempDirectory("fake-first-pull").toFile()
        try {
            val roots = AccountRoots(base)
            val registered = roots.promote(staged(roots)) {}
            File(base, "account-roots").writeText("../../FAKE\nlegacy\n")
            try { roots.startupCleanup(); fail("Must reject damaged journal") } catch (_: FirstPullFailure) {}
            assertTrue(registered.exists())
        } finally { base.deleteRecursively() }
    }
}
