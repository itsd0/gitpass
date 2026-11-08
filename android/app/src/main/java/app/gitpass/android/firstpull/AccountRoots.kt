package app.gitpass.android.firstpull

import java.io.File
import java.io.FileOutputStream
import java.nio.channels.FileChannel
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.util.UUID

/** Non-secret account selection journal. Never replaces/deletes a registered account or legacy root. */
class AccountRoots(private val base: File, private val legacyRoot: File = File(base, "vault")) {
    val staging = File(base, "first-pull-staging")
    private val imports = File(base, "imported-accounts")
    private val journal = File(base, "account-roots")
    private val id = Regex("import-[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}")
    private data class Index(val selected: String, val names: List<String>)

    private fun read(): Index {
        if (!journal.exists()) return Index("legacy", listOf("legacy"))
        if (Files.isSymbolicLink(journal.toPath()) || journal.length() > 16_384) throw FirstPullFailure("Account selection cannot be read; existing data was preserved")
        val lines = journal.readLines()
        if (lines.size !in 2..129 || lines.drop(1).distinct().size != lines.size - 1 ||
            lines.any { it != "legacy" && !id.matches(it) } || lines[0] !in lines.drop(1) || "legacy" !in lines.drop(1)) {
            throw FirstPullFailure("Account selection cannot be read; existing data was preserved")
        }
        return Index(lines.first(), lines.drop(1))
    }
    private fun root(name: String) = if (name == "legacy") legacyRoot else File(imports, name)
    @Synchronized fun selectedRoot(): File = root(read().selected)
    @Synchronized fun accounts(): List<Pair<String, File>> = read().names.map { it to root(it) }

    @Synchronized fun startupCleanup() {
        val known = read().names // A corrupt journal must NEVER trigger account deletion.
        EncryptedFirstPull.cleanupOwnedStages(staging)
        if (imports.isDirectory && !Files.isSymbolicLink(imports.toPath())) {
            imports.listFiles()?.filter { id.matches(it.name) && it.name !in known && !Files.isSymbolicLink(it.toPath()) }
                ?.forEach { it.deleteRecursively() }
        }
        File(base, "account-roots.tmp").delete()
    }

    internal fun prepare(copy: StagedEncryptedCopy, checkpoint: () -> Unit) {
        syncTree(copy.directory, checkpoint)
    }

    /** Caller holds the session's locked epoch gate across this short commit. Native verification must precede it. */
    @Synchronized internal fun promote(copy: StagedEncryptedCopy, checkpoint: () -> Unit): File {
        val previous = read()
        if (previous.names.size >= 128) throw FirstPullFailure("Local account limit reached")
        require(copy.directory.parentFile.canonicalFile == staging.canonicalFile)
        imports.mkdirs()
        if (Files.isSymbolicLink(imports.toPath())) throw FirstPullFailure("Unsafe account destination")
        checkpoint()
        val name = "import-${UUID.randomUUID()}"
        val destination = root(name)
        var registered = false
        try {
            checkpoint()
            Files.move(copy.directory.toPath(), destination.toPath(), StandardCopyOption.ATOMIC_MOVE)
            syncDirectory(imports)
            checkpoint()
            write(Index(name, previous.names + name))
            registered = true
            return destination
        } finally {
            // If journal replacement succeeded but directory fsync failed, treat its new
            // index as authoritative. Never delete a directory referenced by the journal.
            if (!registered && runCatching { name !in read().names }.getOrDefault(false)) destination.deleteRecursively()
        }
    }

    @Synchronized fun select(name: String): File {
        val old = read()
        if (name !in old.names) throw FirstPullFailure("Unknown local account")
        write(Index(name, old.names))
        return root(name)
    }
    private fun write(index: Index) {
        base.mkdirs()
        val temporary = File(base, "account-roots.tmp")
        FileOutputStream(temporary).use { out ->
            out.write((listOf(index.selected) + index.names).joinToString("\n", postfix = "\n").toByteArray(Charsets.US_ASCII))
            out.fd.sync()
        }
        Files.move(temporary.toPath(), journal.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        syncDirectory(base)
    }
    private fun syncTree(file: File, checkpoint: () -> Unit) {
        checkpoint()
        if (Files.isSymbolicLink(file.toPath())) throw FirstPullFailure("Unsafe staged file")
        if (file.isDirectory) {
            file.listFiles()?.forEach { syncTree(it, checkpoint) } ?: throw FirstPullFailure("Cannot read staged files")
            syncDirectory(file)
        } else FileOutputStream(file, true).use { it.fd.sync() }
    }
    private fun syncDirectory(file: File) {
        FileChannel.open(file.toPath(), StandardOpenOption.READ).use { it.force(true) }
    }
}
