package app.gitpass.android

import android.app.Application
import androidx.annotation.VisibleForTesting
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import app.gitpass.android.session.SecretTools
import app.gitpass.android.session.NativeSecretTools
import app.gitpass.android.session.AccountSetup
import app.gitpass.android.session.BridgeCompat
import app.gitpass.android.session.FfiVaultGateway
import app.gitpass.android.session.PendingSecret
import app.gitpass.android.session.VaultGateway
import app.gitpass.android.session.VaultSession
import java.io.File
import app.gitpass.android.firstpull.AccountRoots
import app.gitpass.android.firstpull.FirstPullAttempt
import app.gitpass.android.firstpull.FirstPullFailure
import app.gitpass.android.firstpull.RepositorySelection
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

class GitpassApplication : Application() {
    @Volatile lateinit var session: VaultSession
        private set
    @Volatile lateinit var vaultRoot: File
        private set
    @Volatile lateinit var accountSetup: AccountSetup
        private set
    var accountSelectionDamaged: Boolean = false
        private set
    /** Native bridge handshake failed at startup: the UI stays blocked and no vault is opened. */
    var bridgeIncompatible: Boolean = false
        private set
    val pendingSecret = PendingSecret()
    lateinit var accountRoots: AccountRoots
        private set
    private val revision = MutableStateFlow(0L)
    val accountRevision = revision.asStateFlow()
    private val pull = AtomicReference<FirstPullAttempt?>()
    @VisibleForTesting var firstPullFactory: () -> FirstPullAttempt = ::FirstPullAttempt

    fun cancelFirstPull() { pull.getAndSet(null)?.close() }

    /** IO-thread only. Existing account data is never overwritten, including on failure/cancellation. */
    fun firstPull(selection: RepositorySelection, token: String, password: String, secret: String, epoch: Long) {
        if (accountSelectionDamaged) throw FirstPullFailure("Account selection cannot be read; existing data was preserved")
        val previous = session
        if (pendingSecret.present.value || accountSetup.state.value == AccountSetup.State.Creating) throw FirstPullFailure("Finish account creation first")
        previous.atLockedEpoch(epoch) { Unit }
        val attempt = firstPullFactory()
        if (!pull.compareAndSet(null, attempt)) { attempt.close(); throw FirstPullFailure("First pull already running") }
        try {
            previous.atLockedEpoch(epoch) { attempt.checkpoint() }
            attempt.stageAndVerify(accountRoots, selection, token, password, secret).use { copy ->
                previous.atLockedEpoch(epoch) {
                    attempt.checkpoint()
                    if (session !== previous) throw FirstPullFailure("Account changed; first pull cancelled")
                    val selected = accountRoots.promote(copy, attempt::checkpoint)
                    previous.lock() // invalidate every stale callback before installing the new account
                    install(FfiVaultGateway(), selected, VaultSession.ELAPSED_REALTIME)
                    revision.value += 1
                }
            }
        } finally {
            pull.compareAndSet(attempt, null)
            attempt.close()
        }
    }

    fun selectLocalAccount(name: String, epoch: Long) {
        if (accountSelectionDamaged) throw FirstPullFailure("Account selection cannot be read; existing data was preserved")
        if (pendingSecret.present.value || accountSetup.state.value == AccountSetup.State.Creating) throw FirstPullFailure("Finish account creation first")
        val previous = session
        previous.atLockedEpoch(epoch) {
            if (session !== previous) throw FirstPullFailure("Account changed")
            cancelFirstPull()
            val selected = accountRoots.select(name)
            previous.lock()
            install(FfiVaultGateway(), selected, VaultSession.ELAPSED_REALTIME)
            revision.value += 1
        }
    }

    /** Outlives every Activity; used for work that must not die with the UI. */
    private val appScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onCreate() {
        super.onCreate()
        bridgeIncompatible = !BridgeCompat.checkNative()
        accountRoots = AccountRoots(noBackupFilesDir)
        // Fail closed on a damaged selection journal: preserve every account and do not clean imports.
        val selected = runCatching {
            accountRoots.startupCleanup()
            accountRoots.selectedRoot()
        }.getOrElse {
            accountSelectionDamaged = true
            File(noBackupFilesDir, "vault") // UI remains blocked; this is not silent account selection.
        }
        install(
            gateway = FfiVaultGateway(),
            root = selected,
            nowMs = VaultSession.ELAPSED_REALTIME,
        )
        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStop(owner: LifecycleOwner) {
                // Leaving the foreground drops the native session; observers
                // of session.state see the locked snapshot immediately. The
                // one-time Secret Key stays in [pendingSecret] until the
                // owner confirms they wrote it down.
                session.onHostBackground()
            }
        })
    }

    /**
     * Test-only: points the app at a fake vault under the test's cache dir
     * and a controllable clock. Never used with the production directory.
     */
    @VisibleForTesting
    fun installForTest(gateway: VaultGateway, root: File, nowMs: () -> Long, secretTools: SecretTools = NativeSecretTools) {
        if (this::session.isInitialized) session.lock()
        pendingSecret.clear()
        accountSelectionDamaged = false
        accountRoots = AccountRoots(File(root.parentFile, "first-pull-test-${root.name}"), root)
        firstPullFactory = ::FirstPullAttempt
        install(gateway, root, nowMs, secretTools)
        revision.value += 1
    }

    private fun install(gateway: VaultGateway, root: File, nowMs: () -> Long, secretTools: SecretTools = NativeSecretTools) {
        vaultRoot = root
        val fresh = VaultSession(gateway, nowMs = nowMs, secretTools = secretTools, onLock = ::cancelFirstPull)
        session = fresh
        accountSetup = AccountSetup(
            session = fresh,
            pending = pendingSecret,
            marker = File(root.parentFile, "${root.name}.setup-unconfirmed"),
            scope = appScope,
            vaultExists = gateway::vaultExists,
        ).also { it.discardStaleMarker(root.absolutePath) }
    }
}
