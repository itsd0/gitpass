package app.gitpass.android.passkey

import java.security.SecureRandom

/**
 * Short-lived, single-use record that the user just unlocked gitpass for a
 * passkey sign-in (#477): the unlock step lists the passkeys, and the entry
 * the user then picks can sign without a second prompt, but only for the
 * same rpId and caller and only within [ttlMs]. Process memory only.
 */
class PasskeyGrants(private val nowMs: () -> Long, private val ttlMs: Long = TTL_MS) {
    class Grant(val rpIds: Set<String>, val callerKey: String, val expiresAt: Long, val lockAfter: Boolean)

    private val grants = HashMap<String, Grant>()
    private val random = SecureRandom()

    @Synchronized
    fun issue(rpIds: Set<String>, callerKey: String, lockAfter: Boolean): String {
        val raw = ByteArray(16).also(random::nextBytes)
        val token = raw.joinToString("") { "%02x".format(it) }
        grants[token] = Grant(rpIds, callerKey, nowMs() + ttlMs, lockAfter)
        return token
    }

    /** The grant for [token] if it is fresh and was issued for [rpId] and [callerKey]. Always consumed. */
    @Synchronized
    fun take(token: String?, rpId: String, callerKey: String): Grant? {
        val g = grants.remove(token ?: return null) ?: return null
        return g.takeIf { nowMs() < it.expiresAt && rpId in it.rpIds && it.callerKey == callerKey }
    }

    /** Drops expired grants; true when one of them asked for the vault to be locked again. */
    @Synchronized
    fun expire(): Boolean {
        val now = nowMs()
        val expired = grants.filterValues { now >= it.expiresAt }
        expired.keys.forEach(grants::remove)
        return expired.values.any { it.lockAfter }
    }

    @Synchronized
    fun pending(): Int = grants.size

    @Synchronized
    fun clear() = grants.clear()

    companion object {
        const val TTL_MS = 60_000L
    }
}
