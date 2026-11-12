package app.gitpass.android.passkey

import android.app.Activity
import android.content.Intent
import androidx.annotation.RequiresApi
import androidx.credentials.CreatePublicKeyCredentialRequest
import androidx.credentials.CreatePublicKeyCredentialResponse
import androidx.credentials.exceptions.CreateCredentialCancellationException
import androidx.credentials.exceptions.CreateCredentialUnknownException
import androidx.credentials.provider.PendingIntentHandler
import app.gitpass.android.R

/** Creates a passkey after the user is verified (#477). The request and caller are checked again here. */
@RequiresApi(34)
class PasskeyCreateActivity : PasskeyActivity() {
    override fun start() {
        val request = runCatching { PendingIntentHandler.retrieveProviderCreateCredentialRequest(intent) }.getOrNull()
        val pk = request?.callingRequest as? CreatePublicKeyCredentialRequest
        val identity = PasskeyCallers.identify(this, request?.callingAppInfo)
        if (pk == null || identity == null) return refuse(getString(R.string.passkey_refused))
        background({ app.passkeyBroker.check(identity, pk.requestJson, create = true) }) { checked ->
            val ok = checked.getOrNull() ?: return@background refuse(getString(R.string.passkey_refused))
            val who = listOfNotNull(ok.userName, ok.rpId).joinToString(" · ")
            requireUser(getString(R.string.passkey_create_title), who) {
                background({ app.session.passkeyRegister(ok.caller, pk.requestJson) }) { result ->
                    relock()
                    result.onSuccess { json ->
                        record("created")
                        val data = Intent()
                        PendingIntentHandler.setCreateCredentialResponse(data, CreatePublicKeyCredentialResponse(json))
                        setResult(Activity.RESULT_OK, data)
                        finish()
                    }.onFailure { fail(getString(R.string.passkey_failed)) }
                }
            }
        }
    }

    override fun fail(reason: String, cancelled: Boolean) {
        if (!refused) record(if (cancelled) "cancelled" else "error")
        val data = Intent()
        PendingIntentHandler.setCreateCredentialException(
            data, if (cancelled) CreateCredentialCancellationException(reason) else CreateCredentialUnknownException(reason),
        )
        setResult(Activity.RESULT_OK, data)
        finish()
    }
}
