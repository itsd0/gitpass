package app.gitpass.android.sync

import app.gitpass.android.session.SyncTarget
import java.io.File
import java.util.Base64
import org.json.JSONObject

/** Seals the sync token at rest. Production: [KeystoreTokenCipher]. */
interface TokenCipher {
    fun encrypt(plain: ByteArray): ByteArray
    fun decrypt(sealed: ByteArray): ByteArray
}

/** Saved sync settings. The token is only ever in memory here; never logged. */
class SyncCredentials(val target: SyncTarget, val token: String) {
    override fun toString() = "SyncCredentials([redacted])"
}

/**
 * Per-account sync settings (#455), stored next to (never inside) the vault
 * directory under `noBackupFilesDir`, so they are neither synced nor backed up.
 *
 * What is stored, exactly: owner, repository, branch and the last successful
 * sync time in plain JSON (not secret), and the GitHub token sealed by
 * [TokenCipher] (AES-256-GCM with a non-exportable Android Keystore key in
 * production). The Master Password, Secret Key and account key are never
 * stored: background work only runs while the vault is unlocked in memory.
 */
class SyncSettingsStore(private val file: File, private val cipher: TokenCipher) {
    @Synchronized fun target(): SyncTarget? = read()?.let(::targetOf)

    @Synchronized fun hasToken(): Boolean = read()?.optString("token").orEmpty().isNotEmpty()

    /** Decrypts the token; null when not configured or the sealed token can no longer be opened. */
    @Synchronized fun load(): SyncCredentials? {
        val json = read() ?: return null
        val target = targetOf(json) ?: return null
        val sealed = json.optString("token").takeIf { it.isNotEmpty() } ?: return null
        val token = runCatching { String(cipher.decrypt(Base64.getDecoder().decode(sealed)), Charsets.UTF_8) }.getOrNull() ?: return null
        if (!GitHubSyncTransport.validToken(token)) return null
        return SyncCredentials(target, token)
    }

    /** Saves the target; replaces the token when [token] is given, otherwise keeps the saved one. */
    @Synchronized fun save(target: SyncTarget, token: String?) {
        val previous = read()
        val sealed = when {
            token != null -> {
                require(GitHubSyncTransport.validToken(token)) { "Invalid sync token" }
                Base64.getEncoder().encodeToString(cipher.encrypt(token.toByteArray(Charsets.UTF_8)))
            }
            else -> previous?.optString("token").orEmpty().ifEmpty { throw IllegalArgumentException("Sync token required") }
        }
        val json = JSONObject()
            .put("v", 1)
            .put("owner", target.owner)
            .put("repo", target.repository)
            .put("branch", target.branch)
            .put("token", sealed)
        // A new target starts without a last-sync time.
        if (previous != null && targetOf(previous) == target && previous.has("last_success_ms")) {
            json.put("last_success_ms", previous.getLong("last_success_ms"))
        }
        write(json)
    }

    @Synchronized fun clear() {
        file.delete()
        File(file.path + ".tmp").delete()
    }

    @Synchronized fun lastSuccessMs(): Long? = read()?.takeIf { it.has("last_success_ms") }?.optLong("last_success_ms")

    @Synchronized fun recordSuccess(wallMs: Long) {
        val json = read() ?: return
        write(json.put("last_success_ms", wallMs))
    }

    private fun read(): JSONObject? = runCatching {
        if (!file.isFile || file.length() > MAX_BYTES) return null
        JSONObject(file.readText(Charsets.UTF_8)).takeIf { it.optInt("v") == 1 }
    }.getOrNull()

    private fun write(json: JSONObject) {
        file.parentFile?.mkdirs()
        val tmp = File(file.path + ".tmp")
        tmp.writeText(json.toString(), Charsets.UTF_8)
        if (!tmp.renameTo(file)) {
            tmp.delete()
            throw IllegalStateException("Could not save sync settings")
        }
    }

    private fun targetOf(json: JSONObject): SyncTarget? = runCatching {
        SyncTarget(json.getString("owner"), json.getString("repo"), json.getString("branch"))
    }.getOrNull()

    override fun toString() = "SyncSettingsStore([redacted])"

    private companion object { const val MAX_BYTES = 16 * 1024L }
}
