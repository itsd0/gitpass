package app.gitpass.android.session

import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicBoolean

/**
 * GitHub repository + branch holding the vault (#455). Not secret, but kept
 * out of `toString` like [app.gitpass.android.firstpull.RepositorySelection].
 * Same validation rules as the first pull; gitpass-core re-validates.
 */
class SyncTarget(val owner: String, val repository: String, val branch: String) {
    init {
        require(owner.matches(Regex("[A-Za-z0-9][A-Za-z0-9-]{0,38}")))
        require(repository.matches(Regex("[A-Za-z0-9_.-]{1,100}")) && repository !in listOf(".", ".."))
        require(branch.length in 1..200 && branch.split('/').all {
            it.matches(Regex("[A-Za-z0-9_.-]+")) && !it.startsWith('.') && !it.endsWith('.') &&
                !it.endsWith(".lock") && !it.contains("..")
        })
    }
    override fun equals(other: Any?) = other is SyncTarget && other.owner == owner && other.repository == repository && other.branch == branch
    override fun hashCode() = listOf(owner, repository, branch).hashCode()
    override fun toString() = "SyncTarget([redacted])"
}

/** What one sync did. Conflict lines carry vault/item ids and field names only. */
data class SyncOutcome(
    val pushed: Boolean,
    val localChanged: Boolean,
    val conflicts: List<String>,
    val mergedItems: Int,
    val localOnlyItems: Int,
)

/** Cancels one running sync; hooks run once, on the cancelling thread, never throw. */
class SyncCancel {
    private val cancelled = AtomicBoolean(false)
    private val hooks = CopyOnWriteArrayList<() -> Unit>()
    val isCancelled: Boolean get() = cancelled.get()
    fun cancel() {
        if (cancelled.compareAndSet(false, true)) hooks.forEach { runCatching { it() } }
    }
    fun onCancel(hook: () -> Unit) {
        hooks += hook
        if (cancelled.get()) runCatching { hook() }
    }
}

/** Native/vault operations are refused (not blocked) while a sync holds the native session. */
class SyncInProgressException : Exception("Sync in progress; try again in a moment.")

/** A sync failure with fixed, user-safe text (no token, path, URL or item data). */
class SyncFailedException(message: String) : Exception(message)

/** Session lifecycle events for the sync coordinator. Called with the session lock held: never block. */
interface SessionEvents {
    fun unlocked(epoch: Long) {}
    fun locked() {}
    fun localChange() {}
}
