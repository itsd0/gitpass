package app.gitpass.android

import android.app.Application
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import app.gitpass.android.session.FfiVaultGateway
import app.gitpass.android.session.PendingSecret
import app.gitpass.android.session.VaultSession
import java.io.File

class GitpassApplication : Application() {
    lateinit var session: VaultSession
        private set
    lateinit var vaultRoot: File
        private set
    val pendingSecret = PendingSecret()

    override fun onCreate() {
        super.onCreate()
        vaultRoot = File(noBackupFilesDir, "vault")
        session = VaultSession(FfiVaultGateway())
        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStop(owner: LifecycleOwner) {
                // Leaving the foreground drops the native session. The
                // one-time Secret Key stays in [pendingSecret] until the
                // owner confirms they wrote it down.
                session.onHostBackground()
            }
        })
    }
}
