package app.gitpass.android.autofill

import java.security.SecureRandom
import java.util.concurrent.ConcurrentHashMap

/**
 * Values from a save request, held in process memory only until the user
 * unlocks and confirms (#475). The save activity gets a random token, never
 * the values, in its Intent. Entries expire after [TTL_MS] and are removed
 * when taken.
 */
class SaveStash(private val nowMs: () -> Long = { System.nanoTime() / 1_000_000 }) {
    class Pending(val target: AutofillTarget, val username: String, val password: String, internal val atMs: Long) {
        override fun toString() = "Pending([redacted])"
    }

    private val entries = ConcurrentHashMap<String, Pending>()
    private val random = SecureRandom()

    fun put(target: AutofillTarget, username: String, password: String): String {
        prune()
        val token = ByteArray(16).also(random::nextBytes).joinToString("") { "%02x".format(it) }
        entries[token] = Pending(target, username, password, nowMs())
        return token
    }

    /** Removes and returns the entry, or null when unknown or expired. */
    fun take(token: String?): Pending? {
        prune()
        return token?.let { entries.remove(it) }
    }

    fun clear() = entries.clear()

    private fun prune() {
        val now = nowMs()
        entries.entries.removeIf { now - it.value.atMs > TTL_MS }
    }

    companion object {
        const val TTL_MS = 5 * 60 * 1000L
    }
}
