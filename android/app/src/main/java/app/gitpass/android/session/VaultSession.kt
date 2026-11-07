package app.gitpass.android.session

import android.os.SystemClock
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * One unlocked gitpass account.
 *
 * The native session is the [OpenVault] implementation (UniFFI `Vault`).
 * [lock] drops it. Kotlin `String` values that cross the bridge cannot be
 * wiped from the JVM heap; this class does not retain the Master Password
 * or Secret Key after the call returns.
 *
 * [state] is the observable UI state. Every change, including a lock from
 * any thread, publishes a new [VaultSnapshot] before the call returns, so
 * the UI never needs a ticker to notice that the session locked.
 *
 * [nowMs] must be a monotonic clock that keeps counting during deep sleep
 * ([ELAPSED_REALTIME]). A wall clock would let a time change delay the
 * inactivity lock.
 */
class VaultSession(
    private val gateway: VaultGateway,
    private val nowMs: () -> Long = ELAPSED_REALTIME,
    private val autoLockMs: Long = DEFAULT_AUTO_LOCK_MS,
) {
    private val mutex = ReentrantLock()
    private var generation: Long = 0
    private var opened: OpenVault? = null
    private var vaults: List<VaultSummary> = emptyList()
    private var selectedVaultId: String? = null
    private var items: List<ItemRow> = emptyList()
    private var detail: ItemView? = null
    private var query: String = ""
    private var lastActivityMs: Long = 0
    private val mutableState = MutableStateFlow(lockedSnapshot())

    /** Current UI state. Locked snapshots never carry vaults, items, query, or detail. */
    val state: StateFlow<VaultSnapshot> = mutableState.asStateFlow()

    fun isUnlocked(): Boolean = mutex.withLock { opened != null }

    fun unlock(root: String, password: String, secretKey: String) {
        val attempt = mutex.withLock {
            if (opened != null) throw VaultSessionException("Already unlocked")
            generation
        }
        if (!gateway.vaultExists(root)) {
            throw VaultSessionException("No vault on this device yet.")
        }
        val fresh = gateway.open(root, password, secretKey)
        mutex.withLock {
            if (generation != attempt || opened != null) {
                fresh.close()
                throw UnlockCancelledException()
            }
            installLocked(fresh)
        }
    }

    /**
     * Creates an account and unlocks it. On any failure after the account
     * exists, the returned secret is on [AccountCreatedButLocked] and the
     * session is left locked.
     *
     * [onCreated] runs with the new Secret Key as soon as the account is on
     * disk, before the unlock is attempted, so the caller can hand the key
     * off even if a later step fails or the caller stops listening.
     */
    fun createAccount(root: String, password: String, onCreated: (String) -> Unit = {}): String {
        if (password.length < MIN_PASSWORD_LENGTH) {
            throw VaultSessionException("Use at least 10 characters for the Master Password.")
        }
        val attempt = mutex.withLock {
            if (opened != null) throw VaultSessionException("Lock the current vault first.")
            generation
        }
        if (gateway.vaultExists(root)) {
            throw VaultSessionException("A vault already exists at this location.")
        }
        val secret = gateway.createAccount(root, password)
        onCreated(secret)
        val fresh = try {
            gateway.open(root, password, secret)
        } catch (error: Throwable) {
            throw AccountCreatedButLocked(secret, error)
        }
        mutex.withLock {
            if (generation != attempt) {
                fresh.close()
                throw AccountCreatedButLocked(secret, UnlockCancelledException())
            }
            try {
                installLocked(fresh)
                if (vaults.isEmpty()) {
                    opened?.createVault("Personal")
                    reloadLocked()
                }
                publishLocked()
            } catch (error: Throwable) {
                lockLocked()
                throw AccountCreatedButLocked(secret, error)
            }
        }
        return secret
    }

    fun lock() {
        mutex.withLock { lockLocked() }
    }

    fun onHostBackground() {
        lock()
    }

    /** Milliseconds until the inactivity lock, or null when locked or disabled. */
    fun millisUntilAutoLock(): Long? = mutex.withLock {
        if (opened == null || autoLockMs <= 0L) null
        else (autoLockMs - (nowMs() - lastActivityMs)).coerceAtLeast(0L)
    }

    fun noteActivity() {
        mutex.withLock {
            if (opened != null) lastActivityMs = nowMs()
        }
    }

    fun enforceInactivity() {
        mutex.withLock {
            if (opened == null || autoLockMs <= 0L) return
            if (nowMs() - lastActivityMs >= autoLockMs) lockLocked()
        }
    }

    fun selectVault(vaultId: String) {
        mutex.withLock {
            val current = opened ?: throw VaultLockedException()
            if (vaults.none { it.id == vaultId }) {
                throw VaultSessionException("Unknown vault")
            }
            val listed = current.listItems(vaultId)
            selectedVaultId = vaultId
            items = listed
            detail = null
            lastActivityMs = nowMs()
            publishLocked()
        }
    }

    fun setQuery(value: String) {
        mutex.withLock {
            if (opened == null) throw VaultLockedException()
            query = value
            lastActivityMs = nowMs()
            publishLocked()
        }
    }

    fun openDetail(itemId: String): ItemView {
        return mutex.withLock {
            val current = opened ?: throw VaultLockedException()
            val vaultId = selectedVaultId ?: throw VaultSessionException("No vault selected")
            val loaded = current.getItem(vaultId, itemId)
            detail = loaded
            lastActivityMs = nowMs()
            publishLocked()
            loaded
        }
    }

    fun clearDetail() {
        mutex.withLock {
            detail = null
            publishLocked()
        }
    }

    fun reload() {
        enforceInactivity()
        mutex.withLock {
            if (opened == null) throw VaultLockedException()
            reloadLocked()
            lastActivityMs = nowMs()
            publishLocked()
        }
    }

    fun snapshot(): VaultSnapshot {
        enforceInactivity()
        return mutex.withLock { buildLocked() }
    }

    private fun buildLocked(): VaultSnapshot {
        check(mutex.isHeldByCurrentThread)
        if (opened == null) return lockedSnapshot(generation)
        return VaultSnapshot(
            unlocked = true,
            vaults = vaults,
            selectedVaultId = selectedVaultId,
            items = ItemSearch.visible(items, query),
            query = query,
            detail = detail,
            epoch = generation,
        )
    }

    private fun publishLocked() {
        mutableState.value = buildLocked()
    }

    private fun installLocked(fresh: OpenVault) {
        check(mutex.isHeldByCurrentThread)
        if (opened != null) {
            fresh.close()
            throw VaultSessionException("Already unlocked")
        }
        opened = fresh
        try {
            reloadLocked()
            lastActivityMs = nowMs()
            publishLocked()
        } catch (error: Throwable) {
            opened = null
            fresh.close()
            throw error
        }
    }

    private fun reloadLocked() {
        check(mutex.isHeldByCurrentThread)
        val current = opened ?: throw VaultLockedException()
        val listed = current.listVaults()
        vaults = listed
        val selected = selectedVaultId?.takeIf { id -> listed.any { it.id == id } }
            ?: listed.firstOrNull()?.id
        selectedVaultId = selected
        items = if (selected == null) emptyList() else current.listItems(selected)
    }

    private fun lockLocked() {
        check(mutex.isHeldByCurrentThread)
        generation += 1
        val current = opened
        opened = null
        vaults = emptyList()
        selectedVaultId = null
        items = emptyList()
        detail = null
        query = ""
        // Publish before closing the native handle so observers drop
        // plaintext detail even if close() is slow or throws.
        publishLocked()
        current?.close()
    }

    companion object {
        const val DEFAULT_AUTO_LOCK_MS: Long = 5 * 60 * 1000L
        const val MIN_PASSWORD_LENGTH: Int = 10

        /** Monotonic, includes deep sleep, unaffected by wall-clock changes. */
        val ELAPSED_REALTIME: () -> Long = { SystemClock.elapsedRealtime() }

        fun lockedSnapshot(epoch: Long = 0) = VaultSnapshot(false, emptyList(), null, emptyList(), "", null, epoch)
    }
}

interface VaultGateway {
    fun vaultExists(root: String): Boolean
    fun createAccount(root: String, password: String): String
    fun open(root: String, password: String, secretKey: String): OpenVault
}

interface OpenVault {
    fun listVaults(): List<VaultSummary>
    fun listItems(vaultId: String): List<ItemRow>
    fun getItem(vaultId: String, itemId: String): ItemView
    fun createVault(name: String): String
    fun close()
}

class VaultLockedException : Exception("Vault is locked")

class UnlockCancelledException : Exception("Unlock cancelled because the vault locked.")

class VaultSessionException(message: String) : Exception(message)

/**
 * The account is on disk. [secretKey] is the only copy; it is not part of
 * [message], so a log of the exception text does not include it.
 */
class AccountCreatedButLocked(val secretKey: String, cause: Throwable) : Exception(
    "The account was created, but unlocking it failed.",
    cause,
)
