package app.gitpass.android.sync

import app.gitpass.android.session.SessionEvents
import app.gitpass.android.session.SyncFailedException
import app.gitpass.android.session.SyncInProgressException
import app.gitpass.android.session.UnlockCancelledException
import app.gitpass.android.session.VaultLockedException
import app.gitpass.android.session.VaultSession
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Runs tasks one at a time, in order, off the UI thread. */
interface SyncScheduler {
    fun schedule(delayMs: Long, task: () -> Unit): ScheduledTask
}

fun interface ScheduledTask { fun cancel() }

/** Production scheduler: one daemon thread, so at most one sync runs at a time. */
class ExecutorSyncScheduler(
    private val executor: ScheduledExecutorService = Executors.newSingleThreadScheduledExecutor { task ->
        Thread(task, "gitpass-sync").apply { isDaemon = true }
    },
) : SyncScheduler {
    override fun schedule(delayMs: Long, task: () -> Unit): ScheduledTask {
        val future = executor.schedule({ runCatching(task) }, delayMs.coerceAtLeast(0L), TimeUnit.MILLISECONDS)
        return ScheduledTask { future.cancel(false) }
    }
}

/**
 * Automatic and manual sync for one account (#455).
 *
 * Triggers: on unlock, after a local save/delete (debounced), every
 * [periodMs] while the vault stays unlocked, and "Sync now". Every trigger
 * needs saved settings, a network ([online]) and an unlocked session; a lock
 * cancels pending runs and the running one. Because the vault locks when the
 * app leaves the foreground and the Master Password is never stored, there is
 * no background sync while locked (no WorkManager job): periodic sync is a
 * foreground-while-unlocked timer.
 */
class SyncCoordinator(
    private val session: VaultSession,
    private val settings: SyncSettingsStore,
    private val scheduler: SyncScheduler,
    private val online: () -> Boolean = { true },
    private val wallClock: () -> Long = System::currentTimeMillis,
    private val debounceMs: Long = DEBOUNCE_MS,
    private val periodMs: Long = PERIOD_MS,
) : SessionEvents {
    enum class Phase { NotConfigured, Idle, Syncing, Failed, Offline }

    /** UI state. [error] is fixed text from core or this class; never a token, path or item data. */
    data class Status(
        val phase: Phase,
        val lastSuccessMs: Long? = null,
        val conflicts: List<String> = emptyList(),
        val error: String? = null,
    )

    private val mutable = MutableStateFlow(initialStatus())
    val status: StateFlow<Status> = mutable.asStateFlow()

    private val lock = Any()
    private var epoch: Long? = null
    private var oneShot: ScheduledTask? = null
    private var periodic: ScheduledTask? = null

    private fun initialStatus() = Status(
        phase = if (settings.target() != null && settings.hasToken()) Phase.Idle else Phase.NotConfigured,
        lastSuccessMs = settings.lastSuccessMs(),
    )

    override fun unlocked(epoch: Long) {
        synchronized(lock) {
            this.epoch = epoch
            request(0L)
            schedulePeriodicLocked(epoch)
        }
    }

    override fun locked() {
        synchronized(lock) {
            epoch = null
            oneShot?.cancel(); oneShot = null
            periodic?.cancel(); periodic = null
        }
        mutable.value = mutable.value.let { if (it.phase == Phase.Syncing) it.copy(phase = Phase.Idle) else it }
    }

    override fun localChange() { request(debounceMs) }

    /** Manual "Sync now". */
    fun syncNow() { request(0L) }

    /** Settings saved or removed: refresh state and sync right away when configured. */
    fun settingsChanged() {
        val configured = settings.target() != null && settings.hasToken()
        mutable.value = Status(if (configured) Phase.Idle else Phase.NotConfigured, settings.lastSuccessMs())
        if (configured) syncNow()
    }

    private fun request(delayMs: Long) {
        synchronized(lock) {
            val current = epoch ?: return
            oneShot?.cancel()
            oneShot = scheduler.schedule(delayMs) { runOnce(current) }
        }
    }

    private fun schedulePeriodicLocked(current: Long) {
        periodic?.cancel()
        periodic = scheduler.schedule(periodMs) {
            runOnce(current)
            synchronized(lock) { if (epoch == current) schedulePeriodicLocked(current) }
        }
    }

    /** Runs on the scheduler thread. */
    private fun runOnce(current: Long) {
        if (synchronized(lock) { epoch } != current) return
        val credentials = settings.load()
        if (credentials == null) {
            mutable.value = Status(Phase.NotConfigured, settings.lastSuccessMs())
            return
        }
        if (!online()) {
            mutable.value = mutable.value.copy(phase = Phase.Offline, error = null)
            return
        }
        mutable.value = mutable.value.copy(phase = Phase.Syncing, error = null)
        try {
            val outcome = session.sync(current, credentials.target, credentials.token)
            val now = wallClock()
            settings.recordSuccess(now)
            mutable.value = Status(Phase.Idle, now, outcome.conflicts)
        } catch (_: UnlockCancelledException) {
            mutable.value = mutable.value.copy(phase = Phase.Idle)
        } catch (_: VaultLockedException) {
            mutable.value = mutable.value.copy(phase = Phase.Idle)
        } catch (_: SyncInProgressException) {
            request(debounceMs)
        } catch (error: SyncFailedException) {
            mutable.value = mutable.value.copy(phase = Phase.Failed, error = error.message)
        } catch (_: Exception) {
            mutable.value = mutable.value.copy(phase = Phase.Failed, error = GENERIC_FAILURE)
        }
    }

    override fun toString() = "SyncCoordinator([redacted])"

    companion object {
        const val DEBOUNCE_MS = 3_000L
        const val PERIOD_MS = 15 * 60 * 1000L
        const val GENERIC_FAILURE = "Sync failed; try again"
    }
}
