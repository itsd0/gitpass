package app.gitpass.android

import android.app.Application
import androidx.annotation.VisibleForTesting
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import app.gitpass.android.session.SecretTools
import app.gitpass.android.session.NativeSecretTools
import app.gitpass.android.session.AccountSetup
import app.gitpass.android.session.FfiVaultGateway
import app.gitpass.android.session.PendingSecret
import app.gitpass.android.session.VaultGateway
import app.gitpass.android.session.VaultSession
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

class GitpassApplication : Application() {
    lateinit var session: VaultSession
        private set
    lateinit var vaultRoot: File
        private set
    lateinit var accountSetup: AccountSetup
        private set
    val pendingSecret = PendingSecret()

    /** Outlives every Activity; used for work that must not die with the UI. */
    private val appScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onCreate() {
        super.onCreate()
        install(
            gateway = FfiVaultGateway(),
            root = File(noBackupFilesDir, "vault"),
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
        install(gateway, root, nowMs, secretTools)
    }

    private fun install(gateway: VaultGateway, root: File, nowMs: () -> Long, secretTools: SecretTools = NativeSecretTools) {
        vaultRoot = root
        val fresh = VaultSession(gateway, nowMs = nowMs, secretTools = secretTools)
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
