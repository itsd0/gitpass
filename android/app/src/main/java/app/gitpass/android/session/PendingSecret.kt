package app.gitpass.android.session

import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * The one-time Secret Key from account creation, held only until the owner
 * confirms they saved it. Process memory only: not written to saved state,
 * files, logs, or backups, so it does not survive process death.
 * [toString] does not include the key, and [present] exposes only whether
 * a key is waiting.
 */
class PendingSecret {
    private val mutex = ReentrantLock()
    private var value: String? = null
    private val mutablePresent = MutableStateFlow(false)

    /** True while a key is waiting for confirmation. Never carries the key. */
    val present: StateFlow<Boolean> = mutablePresent.asStateFlow()

    fun set(secret: String) {
        mutex.withLock {
            value = secret
            mutablePresent.value = true
        }
    }

    fun get(): String? = mutex.withLock { value }

    fun clear() {
        mutex.withLock {
            value = null
            mutablePresent.value = false
        }
    }

    override fun toString(): String = mutex.withLock { "PendingSecret(present=${value != null})" }
}
