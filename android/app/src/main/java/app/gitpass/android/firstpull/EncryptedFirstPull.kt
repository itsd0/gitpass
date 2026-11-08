package app.gitpass.android.firstpull

import java.io.File
import java.nio.file.Files
import java.security.MessageDigest
import java.util.UUID

/** Explicit selection only: there is no default owner, repository, branch or credential. */
class RepositorySelection(val owner: String, val repository: String, val branch: String) {
    init {
        require(owner.matches(Regex("[A-Za-z0-9][A-Za-z0-9-]{0,38}")))
        require(repository.matches(Regex("[A-Za-z0-9_.-]{1,100}")) && repository !in listOf(".", ".."))
        require(branch.length in 1..200 && branch.split('/').all {
            it.matches(Regex("[A-Za-z0-9_.-]+")) && !it.startsWith('.') && !it.endsWith('.') &&
                !it.endsWith(".lock") && !it.contains("..")
        })
    }
    override fun toString() = "RepositorySelection([redacted])"
}

/** Implementations must resolve branch once, then read this exact commit's recursive tree. */
interface FirstPullTransport {
    fun resolve(selection: RepositorySelection): String
    fun tree(selection: RepositorySelection, commit: String): RemoteTree
    fun blob(selection: RepositorySelection, sha: String): ByteArray
}
class RemoteTree(val commit: String, val truncated: Boolean, val entries: List<RemoteEntry>)
class RemoteEntry(val path: String, val type: String, val mode: String, val sha: String, val size: Long)
class FirstPullFailure(message: String) : Exception(message)

/**
 * Encrypted staging ONLY. Not an active account and never promoted by this component.
 * Production integration must authenticate this complete copy natively, then commit
 * separate-account selection under the locked session epoch gate. Staging alone is never trusted.
 */
class EncryptedFirstPull(private val stagingParent: File, private val transport: FirstPullTransport) {
    fun stage(selection: RepositorySelection, cancelled: () -> Boolean = { false }): StagedEncryptedCopy {
        stagingParent.mkdirs()
        check(!Files.isSymbolicLink(stagingParent.toPath()))
        val directory = File(stagingParent, "pull-${UUID.randomUUID()}")
        check(directory.mkdir())
        try {
            fun checkpoint() { if (cancelled()) throw FirstPullFailure("First pull cancelled") }
            checkpoint()
            val commit = transport.resolve(selection)
            if (!shaPattern.matches(commit)) throw FirstPullFailure("Invalid commit identity")
            val tree = transport.tree(selection, commit)
            if (tree.commit != commit || tree.truncated || tree.entries.size > MAX_ENTRIES) throw FirstPullFailure("Incomplete or excessive remote tree")
            val seen = HashSet<String>()
            val files = mutableListOf<RemoteEntry>()
            var total = 0L
            for (entry in tree.entries) {
                checkpoint()
                if (!seen.add(entry.path) || !safePath(entry.path)) throw FirstPullFailure("Unsafe or duplicate remote path")
                if (entry.type == "tree" && entry.mode == "040000" && allowedDirectory(entry.path)) continue
                if (entry.type != "blob" || entry.mode != "100644") throw FirstPullFailure("Unsupported remote entry mode")
                // Repository documentation is never imported. Even ignored entries must be regular files.
                if (entry.path in setOf("README.md", ".gitignore")) continue
                if (!allowedFile(entry.path) || !shaPattern.matches(entry.sha)) throw FirstPullFailure("Unsupported vault file")
                if (entry.size < 0 || entry.size > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES - entry.size) throw FirstPullFailure("Remote snapshot exceeds limits")
                total += entry.size
                files += entry
            }
            if (!files.map { it.path }.containsAll(required)) throw FirstPullFailure("Missing required vault files")
            for (entry in files) {
                checkpoint()
                val bytes = transport.blob(selection, entry.sha)
                checkpoint()
                if (bytes.size.toLong() != entry.size || bytes.size > MAX_FILE_BYTES || gitBlobSha(bytes) != entry.sha) throw FirstPullFailure("Blob identity mismatch")
                val file = File(directory, entry.path)
                if (!file.canonicalPath.startsWith(directory.canonicalPath + File.separator)) throw FirstPullFailure("Unsafe destination")
                file.parentFile?.mkdirs()
                file.outputStream().use { it.write(bytes) }
            }
            checkpoint()
            return StagedEncryptedCopy(directory, commit)
        } catch (error: Throwable) {
            directory.deleteRecursively()
            // Never propagate transport errors that may contain credentials, URLs or response bodies.
            if (error is FirstPullFailure) throw error
            throw FirstPullFailure("Encrypted first-pull staging failed")
        }
    }

    companion object {
        const val MAX_ENTRIES = 10_000
        const val MAX_FILE_BYTES = 4 * 1024 * 1024
        const val MAX_TOTAL_BYTES = 64L * 1024 * 1024
        private val shaPattern = Regex("[a-f0-9]{40}")
        private val id = "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}"
        private val required = setOf("format.json", "keyset.enc", "manifest.enc")
        private val roots = required + setOf("identity.pub", "identity.enc", "devices.enc", "generator_history.enc")
        private fun safePath(path: String) = path.length in 1..512 && !path.contains('\\') &&
            path.split('/').all { it.isNotEmpty() && it != "." && it != ".." && it.all { c -> c.code in 33..126 } }
        private fun allowedDirectory(path: String) = path == "vaults" ||
            path.matches(Regex("vaults/$id(/(items|files|members))?"))
        private fun allowedFile(path: String) = path in roots ||
            path.matches(Regex("vaults/$id/(vault\\.key\\.enc|meta\\.enc|items/($id\\.enc|\\.keep)|files/$id\\.enc|members/[a-f0-9]{16,128}\\.enc)"))
        fun gitBlobSha(bytes: ByteArray): String {
            val digest = MessageDigest.getInstance("SHA-1")
            digest.update("blob ${bytes.size}\u0000".toByteArray(Charsets.US_ASCII))
            return digest.digest(bytes).joinToString("") { "%02x".format(it.toInt() and 255) }
        }
        /** Only this subsystem's UUID-named staging folders; never account roots. */
        fun cleanupOwnedStages(parent: File) {
            if (!parent.isDirectory || Files.isSymbolicLink(parent.toPath())) return
            parent.listFiles()?.filter { it.name.matches(Regex("pull-$id")) && !Files.isSymbolicLink(it.toPath()) }
                ?.forEach { it.deleteRecursively() }
        }
    }
}

class StagedEncryptedCopy internal constructor(val directory: File, val commit: String) : AutoCloseable {
    /** This type deliberately provides no activate/promote API or authenticated flag. */
    override fun close() { directory.deleteRecursively() }
    override fun toString() = "StagedEncryptedCopy([not authenticated, not active])"
}
