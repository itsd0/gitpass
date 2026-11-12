package app.gitpass.android.passkey

import android.app.PendingIntent
import android.content.Intent
import android.os.CancellationSignal
import android.os.OutcomeReceiver
import androidx.annotation.RequiresApi
import androidx.credentials.exceptions.ClearCredentialException
import androidx.credentials.exceptions.CreateCredentialException
import androidx.credentials.exceptions.GetCredentialException
import androidx.credentials.provider.AuthenticationAction
import androidx.credentials.provider.BeginCreateCredentialRequest
import androidx.credentials.provider.BeginCreateCredentialResponse
import androidx.credentials.provider.BeginCreatePublicKeyCredentialRequest
import androidx.credentials.provider.BeginGetCredentialRequest
import androidx.credentials.provider.BeginGetCredentialResponse
import androidx.credentials.provider.CreateEntry
import androidx.credentials.provider.CredentialProviderService
import androidx.credentials.provider.ProviderClearCredentialStateRequest
import app.gitpass.android.GitpassApplication
import app.gitpass.android.R
import java.util.concurrent.Executors

/**
 * Passkey provider for Android Credential Manager (#477, Android 14+). The
 * service never signs anything and never shows secrets:
 * - sign-in: only when the caller is valid for the request's rpId (core
 *   rules + Digital Asset Links for apps). A locked vault answers with an
 *   "Unlock gitpass" action; an open vault lists its matching passkeys
 *   (names only), and picking one still asks for a fresh verification.
 * - create: offers a "gitpass" entry; the activity verifies the user first.
 * Disabled below API 34 through `@bool/passkey_provider_enabled`.
 */
@RequiresApi(34)
class GitpassCredentialProviderService : CredentialProviderService() {
    private val worker = Executors.newSingleThreadExecutor()

    private val app get() = application as GitpassApplication

    private fun usable() = !app.bridgeIncompatible && !app.accountSelectionDamaged

    override fun onBeginGetCredentialRequest(
        request: BeginGetCredentialRequest,
        cancellationSignal: CancellationSignal,
        callback: OutcomeReceiver<BeginGetCredentialResponse, GetCredentialException>,
    ) {
        val identity = if (usable()) PasskeyCallers.identify(this, request.callingAppInfo) else null
        if (identity == null) return callback.onResult(BeginGetCredentialResponse())
        worker.execute {
            val response = try {
                val checked = PasskeyEntries.checkedOptions(app, identity, request)
                when {
                    checked.isEmpty() -> BeginGetCredentialResponse()
                    app.session.isUnlocked() -> PasskeyEntries.response(this, app, checked, grant = null)
                    else -> {
                        val pending = PendingIntent.getActivity(
                            this, PasskeyEntries.nextRequestCode(), Intent(this, PasskeyUnlockActivity::class.java),
                            PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                        )
                        BeginGetCredentialResponse(authenticationActions = listOf(AuthenticationAction(getString(R.string.passkey_unlock_action), pending)))
                    }
                }
            } catch (_: Exception) {
                BeginGetCredentialResponse()
            }
            if (!cancellationSignal.isCanceled) callback.onResult(response)
        }
    }

    override fun onBeginCreateCredentialRequest(
        request: BeginCreateCredentialRequest,
        cancellationSignal: CancellationSignal,
        callback: OutcomeReceiver<BeginCreateCredentialResponse, CreateCredentialException>,
    ) {
        val pk = request as? BeginCreatePublicKeyCredentialRequest
        val identity = if (usable() && pk != null) PasskeyCallers.identify(this, request.callingAppInfo) else null
        if (pk == null || identity == null) return callback.onResult(BeginCreateCredentialResponse())
        worker.execute {
            val response = try {
                app.passkeyBroker.check(identity, pk.requestJson, create = true)
                val pending = PendingIntent.getActivity(
                    this, PasskeyEntries.nextRequestCode(), Intent(this, PasskeyCreateActivity::class.java),
                    PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                )
                BeginCreateCredentialResponse(createEntries = listOf(CreateEntry(getString(R.string.passkey_account_name), pending)))
            } catch (_: Exception) {
                BeginCreateCredentialResponse()
            }
            if (!cancellationSignal.isCanceled) callback.onResult(response)
        }
    }

    override fun onClearCredentialStateRequest(
        request: ProviderClearCredentialStateRequest,
        cancellationSignal: CancellationSignal,
        callback: OutcomeReceiver<Void?, ClearCredentialException>,
    ) {
        callback.onResult(null)
    }

    override fun onDestroy() {
        worker.shutdown()
        super.onDestroy()
    }
}
