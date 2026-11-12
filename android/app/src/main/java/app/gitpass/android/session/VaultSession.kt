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
    private val secretTools: SecretTools = NativeSecretTools,
    private val onLock: () -> Unit = {},
) {
    private val mutex = ReentrantLock()
    private var generation: Long = 0
    private var opened: OpenVault? = null
    private var vaults: List<VaultSummary> = emptyList()
    private var selectedVaultId: String? = null
    private var items: List<ItemRow> = emptyList()
    private var detail: ItemView? = null
    private var query: String = ""
    private var trash: Boolean = false
    private var lastActivityMs: Long = 0
    /** Set while a sync holds the native session; cancelled by every lock. */
    private var syncing: SyncCancel? = null
    /** Sync coordinator hooks (#455). */
    @Volatile var events: SessionEvents? = null
    private val mutableState = MutableStateFlow(lockedSnapshot())

    /** Current UI state. Locked snapshots never carry vaults, items, query, or detail. */
    val state: StateFlow<VaultSnapshot> = mutableState.asStateFlow()

    fun isUnlocked(): Boolean = mutex.withLock { opened != null }

    /** Serializes import selection with every lock path; never authenticate or download inside this gate. */
    fun <T> atLockedEpoch(epoch: Long, action: () -> T): T = mutex.withLock {
        if (opened != null || generation != epoch) throw UnlockCancelledException()
        action()
    }

    fun unlock(root: String, password: String, secretKey: String) {
        val attempt = beginUnlock(root)
        installUnlocked(attempt, gateway.open(root, password, secretKey))
    }

    /**
     * Master Password unlock that also turns on biometric unlock (#473):
     * gitpass-core derives the Account Unlock Key once (full KDF, verified
     * against the keyset) and the session is opened with it, so enabling
     * costs no extra KDF run. Returns the key for the caller to seal with the
     * biometric Keystore key; the caller must wipe it. Nothing is returned
     * when the unlock fails or is cancelled.
     */
    fun unlockForQuickSetup(root: String, password: String, secretKey: String): ByteArray {
        val attempt = beginUnlock(root)
        val key = gateway.deriveUnlockKey(root, password, secretKey)
        try {
            installUnlocked(attempt, gateway.openWithUnlockKey(root, key.copyOf()))
            return key
        } catch (error: Throwable) {
            key.fill(0)
            throw error
        }
    }

    /**
     * Biometric unlock with a key released by Android Keystore after a
     * BIOMETRIC_STRONG authentication. [key] is wiped before returning.
     * Throws [QuickUnlockStaleException] when the key no longer opens the
     * vault (password, Secret Key or KDF changed), so the caller can delete
     * it and fall back to the Master Password. Side-effect free, like the
     * password unlock.
     */
    fun unlockWithKey(root: String, key: ByteArray) {
        try {
            val attempt = beginUnlock(root)
            installUnlocked(attempt, gateway.openWithUnlockKey(root, key.copyOf()))
        } finally {
            key.fill(0)
        }
    }

    private fun beginUnlock(root: String): Long {
        val attempt = mutex.withLock {
            if (opened != null) throw VaultSessionException("Already unlocked")
            generation
        }
        if (!gateway.vaultExists(root)) {
            throw VaultSessionException("No vault on this device yet.")
        }
        return attempt
    }

    private fun installUnlocked(attempt: Long, fresh: OpenVault) {
        mutex.withLock {
            if (generation != attempt || opened != null) {
                fresh.close()
                throw UnlockCancelledException()
            }
            installLocked(fresh)
            events?.unlocked(generation)
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
                events?.unlocked(generation)
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
            ensureNotSyncing()
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
            ensureNotSyncing()
            val vaultId = selectedVaultId ?: throw VaultSessionException("No vault selected")
            val loaded = current.getItem(vaultId, itemId)
            detail = loaded
            lastActivityMs = nowMs()
            publishLocked()
            loaded
        }
    }

    fun generatePassword(length: Int, epoch: Long): String = mutex.withLock {
        enforceInactivity()
        if (opened == null) throw VaultLockedException()
        if (generation != epoch) throw UnlockCancelledException()
        require(length in 12..64) { "Length must be 12..64" }
        // Never store generated values in the session or write the vault here.
        secretTools.generate(length)
    }

    fun liveTotp(itemId: String, fieldKey: String, epoch: Long): LiveCode = mutex.withLock {
        enforceInactivity()
        if (opened == null) throw VaultLockedException()
        if (generation != epoch) throw UnlockCancelledException()
        val current = detail?.takeIf { it.id == itemId } ?: throw VaultSessionException("Item is not open")
        val field = detailRows(current).singleOrNull { it.key == fieldKey && it.isTotp }
            ?: throw VaultSessionException("No TOTP configuration")
        // Core uses Unix wall time; the inactivity clock must never be used here.
        // Never surface native parser errors: they may quote seed characters.
        try { secretTools.totp(field.value) }
        catch (_: Exception) { throw VaultSessionException("Invalid TOTP configuration") }
    }

    fun setTrash(value: Boolean) = mutex.withLock {
        if (opened == null) throw VaultLockedException()
        trash = value
        detail = null
        query = ""
        lastActivityMs = nowMs()
        publishLocked()
    }

    /** Vault and epoch are captured before dispatch, preventing writes after lock/re-unlock. */
    fun saveItem(vaultId: String, itemId: String?, edit: ItemEdit, epoch: Long): String = mutex.withLock {
        enforceInactivity()
        val current = opened ?: throw VaultLockedException()
        if (generation != epoch || selectedVaultId != vaultId) throw UnlockCancelledException()
        ensureNotSyncing()
        val existing = itemId?.let { current.getItem(vaultId, it) }
        if (existing?.deleted == true || existing?.archived == true) throw VaultSessionException("Restore item before editing")
        val input = edit.input(existing)
        val id = if (itemId == null) current.createItem(vaultId, input) else {
            current.updateItem(vaultId, itemId, input)
            itemId
        }
        detail = null
        reloadLocked()
        lastActivityMs = nowMs()
        publishLocked()
        events?.localChange()
        id
    }

    fun setDeleted(vaultId: String, itemId: String, deleted: Boolean, epoch: Long) = mutex.withLock {
        enforceInactivity()
        val current = opened ?: throw VaultLockedException()
        if (generation != epoch || selectedVaultId != vaultId) throw UnlockCancelledException()
        ensureNotSyncing()
        if (deleted) current.deleteItem(vaultId, itemId) else current.restoreItem(vaultId, itemId)
        detail = null
        reloadLocked()
        lastActivityMs = nowMs()
        publishLocked()
        events?.localChange()
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
            ensureNotSyncing()
            reloadLocked()
            lastActivityMs = nowMs()
            publishLocked()
        }
    }

    /**
     * Logins for an autofill request (#475). Only from an unlocked session and
     * only items whose URL matches [target] (rules in gitpass-core).
     */
    fun autofillLogins(target: app.gitpass.android.autofill.AutofillTarget): List<AutofillCredential> = mutex.withLock {
        val current = opened ?: throw VaultLockedException()
        ensureNotSyncing()
        lastActivityMs = nowMs()
        current.autofillLogins(target)
    }

    /**
     * Saves a login from an autofill save request (#475): updates the password
     * of the matching item with the same username, otherwise creates a new
     * login in the selected vault. Returns the item title.
     */
    fun saveAutofillLogin(target: app.gitpass.android.autofill.AutofillTarget, username: String, password: String): String = mutex.withLock {
        val current = opened ?: throw VaultLockedException()
        ensureNotSyncing()
        require(password.isNotEmpty()) { "Nothing to save" }
        val match = current.autofillLogins(target).firstOrNull { (it.username ?: "") == username }
        val title = if (match != null) {
            val existing = current.getItem(match.vaultId, match.itemId)
            val edit = ItemEdit(existing.title, username, password, existing.url ?: target.itemUrl, existing.notes.orEmpty())
            current.updateItem(match.vaultId, match.itemId, edit.input(existing))
            existing.title
        } else {
            val vaultId = selectedVaultId ?: vaults.firstOrNull()?.id ?: throw VaultSessionException("No vault selected")
            current.createItem(vaultId, ItemEdit(target.label, username, password, target.itemUrl).input(null))
            target.label
        }
        reloadLocked()
        lastActivityMs = nowMs()
        publishLocked()
        events?.localChange()
        title
    }

    /** True while a sync holds the native session. */
    fun isSyncing(): Boolean = mutex.withLock { syncing != null }

    /**
     * Two-way sync of the unlocked account on the shared core engine (#455).
     *
     * The network part runs WITHOUT this session's lock, so a lock (background,
     * Lock button, inactivity) never waits for the network: it cancels the
     * sync instead. While the sync runs, operations that would touch the native
     * session fail fast with [SyncInProgressException] rather than block a UI
     * thread on the native lock. Results from a sync that outlived its unlock
     * epoch are discarded.
     */
    fun sync(epoch: Long, target: SyncTarget, token: String): SyncOutcome {
        val cancel = SyncCancel()
        val current = mutex.withLock {
            enforceInactivity()
            val c = opened ?: throw VaultLockedException()
            if (generation != epoch) throw UnlockCancelledException()
            if (syncing != null) throw SyncInProgressException()
            syncing = cancel
            c
        }
        try {
            val outcome = current.syncGitHub(target, token, cancel)
            mutex.withLock {
                if (generation != epoch || opened !== current) throw UnlockCancelledException()
                syncing = null
                if (outcome.localChanged) {
                    reloadLocked()
                    detail = detail?.let { open -> runCatching { current.getItem(open.vaultId, open.id) }.getOrNull() }
                }
                publishLocked()
            }
            return outcome
        } catch (error: Throwable) {
            if (cancel.isCancelled) throw UnlockCancelledException()
            throw error
        } finally {
            mutex.withLock { if (syncing === cancel) syncing = null }
        }
    }

    private fun ensureNotSyncing() {
        check(mutex.isHeldByCurrentThread)
        if (syncing != null) throw SyncInProgressException()
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
            items = ItemSearch.visible(items, query, trash),
            query = query,
            detail = detail,
            epoch = generation,
            trash = trash,
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
        onLock()
        syncing?.cancel()
        syncing = null
        events?.locked()
        val current = opened
        opened = null
        vaults = emptyList()
        selectedVaultId = null
        items = emptyList()
        detail = null
        query = ""
        trash = false
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

    /**
     * The 32-byte Account Unlock Key (gitpass-core, full KDF) for biometric
     * unlock (#473). Wrong credentials fail like [open] and count towards the
     * unlock throttle. The caller wipes the result.
     */
    fun deriveUnlockKey(root: String, password: String, secretKey: String): ByteArray = throw UnsupportedOperationException()

    /**
     * Side-effect-free unlock with a key from [deriveUnlockKey]; wipes [key].
     * Throws [QuickUnlockStaleException] when the key no longer opens the vault.
     */
    fun openWithUnlockKey(root: String, key: ByteArray): OpenVault = throw UnsupportedOperationException()
}

interface OpenVault {
    fun listVaults(): List<VaultSummary>
    fun listItems(vaultId: String): List<ItemRow>
    fun getItem(vaultId: String, itemId: String): ItemView
    fun createVault(name: String): String
    fun createItem(vaultId: String, input: app.gitpass.core.ItemInput): String = throw UnsupportedOperationException()
    fun updateItem(vaultId: String, itemId: String, input: app.gitpass.core.ItemInput): Unit = throw UnsupportedOperationException()
    fun deleteItem(vaultId: String, itemId: String): Unit = throw UnsupportedOperationException()
    fun restoreItem(vaultId: String, itemId: String): Unit = throw UnsupportedOperationException()
    /** Matching logins for autofill (#475); secrets only for matching items. */
    fun autofillLogins(target: app.gitpass.android.autofill.AutofillTarget): List<AutofillCredential> = throw UnsupportedOperationException()
    /** Pull, merge and push with GitHub on the shared core engine; honour [cancel]. */
    fun syncGitHub(target: SyncTarget, token: String, cancel: SyncCancel): SyncOutcome = throw UnsupportedOperationException()
    fun close()
}

class VaultLockedException : Exception("Vault is locked")

/** One autofill match. [toString] never includes the username or password. */
class AutofillCredential(val vaultId: String, val itemId: String, val title: String, val username: String?, val password: String?) {
    override fun toString() = "AutofillCredential([redacted])"
}

/** The saved biometric unlock key no longer opens this vault (password, Secret Key or KDF changed). */
class QuickUnlockStaleException : Exception("Biometric unlock no longer matches this vault. Unlock with your Master Password.")

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
