package app.gitpass.android.session

import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Runs first-run account creation outside any Activity or composition.
 *
 * [scope] is application-scoped, so rotating the device or recreating the
 * Activity does not cancel creation or drop its result. The Secret Key goes
 * into [pending] (process memory) the moment the account is on disk, before
 * the unlock and before any UI sees it.
 *
 * Process death: memory does not survive it, so the key is gone. A
 * non-secret [marker] file is written before creation and removed only when
 * the owner confirms they saved the key or later unlocks with it. If the
 * marker and a vault exist but no key is pending, [isInterrupted] is true
 * and the UI says so instead of pretending the key can be shown again.
 */
class AccountSetup(
    private val session: VaultSession,
    private val pending: PendingSecret,
    private val marker: File,
    private val scope: CoroutineScope,
    private val vaultExists: (String) -> Boolean,
) {
    sealed interface State {
        data object Idle : State
        data object Creating : State

        /** [error] never carries the Secret Key (see [AccountCreatedButLocked.cause]). */
        data class Failed(val error: Throwable) : State
    }

    private val mutableState = MutableStateFlow<State>(State.Idle)
    val state: StateFlow<State> = mutableState.asStateFlow()

    /** Starts creation. Returns false if a creation is already running. */
    fun start(root: String, password: String): Boolean {
        synchronized(this) {
            if (mutableState.value == State.Creating) return false
            mutableState.value = State.Creating
        }
        scope.launch {
            // Nothing below may be abandoned half way: the account on disk and
            // the key handoff must stay together.
            withContext(NonCancellable) { create(root, password) }
        }
        return true
    }

    private fun create(root: String, password: String) {
        try {
            marker.parentFile?.mkdirs()
            marker.writeText("")
            session.createAccount(root, password) { secret -> pending.set(secret) }
            mutableState.value = State.Idle
        } catch (error: AccountCreatedButLocked) {
            // The key is already pending from onCreated; keep it as a fallback.
            if (pending.get() == null) pending.set(error.secretKey)
            mutableState.value = State.Failed(error.cause ?: VaultSessionException(error.message.orEmpty()))
        } catch (error: Throwable) {
            if (!vaultExists(root)) marker.delete()
            mutableState.value = State.Failed(error)
        }
    }

    /** The owner confirmed they saved the key. */
    fun confirmSaved() {
        pending.clear()
        marker.delete()
        if (mutableState.value is State.Failed) mutableState.value = State.Idle
    }

    /** A successful unlock proves the owner has the Secret Key. */
    fun noteUnlocked() {
        marker.delete()
    }

    /**
     * True when an account was created but its Secret Key was never
     * confirmed and is no longer in memory (the process died in between).
     */
    fun isInterrupted(root: String): Boolean =
        mutableState.value != State.Creating && pending.get() == null && marker.isFile && vaultExists(root)

    /** Drops a stale marker left by a creation that never wrote a vault. */
    fun discardStaleMarker(root: String) {
        if (mutableState.value != State.Creating && marker.isFile && !vaultExists(root)) marker.delete()
    }
}
