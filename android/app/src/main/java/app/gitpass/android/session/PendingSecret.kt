package app.gitpass.android.session

import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock

/**
 * The one-time Secret Key from account creation, held only until the owner
 * confirms they saved it. Not written to saved state, logs, or backups.
 * [toString] does not include the key.
 */
class PendingSecret {
    private val mutex = ReentrantLock()
    private var value: String? = null

    fun set(secret: String) {
        mutex.withLock { value = secret }
    }

    fun get(): String? = mutex.withLock { value }

    fun clear() {
        mutex.withLock { value = null }
    }

    override fun toString(): String = mutex.withLock { "PendingSecret(present=${value != null})" }
}
