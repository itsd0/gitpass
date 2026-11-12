package app.gitpass.android.passkey

import android.app.Activity
import android.content.Intent
import androidx.annotation.RequiresApi
import androidx.credentials.GetCredentialResponse
import androidx.credentials.GetPublicKeyCredentialOption
import androidx.credentials.PublicKeyCredential
import androidx.credentials.exceptions.GetCredentialCancellationException
import androidx.credentials.exceptions.GetCredentialUnknownException
import androidx.credentials.provider.PendingIntentHandler
import app.gitpass.android.R

/**
 * Signs in with the passkey the user picked (#477). Always needs a fresh
 * verification (unlock, or a re-check when the vault is open), because every
 * assertion reports user verification. The caller and rpId are checked
 * again here, and core only signs for a passkey stored for that rpId.
 */
@RequiresApi(34)
class PasskeyGetActivity : PasskeyActivity() {
    override fun start() {
        val request = runCatching { PendingIntentHandler.retrieveProviderGetCredentialRequest(intent) }.getOrNull()
        val option = request?.credentialOptions?.filterIsInstance<GetPublicKeyCredentialOption>()?.firstOrNull()
        val identity = PasskeyCallers.identify(this, request?.callingAppInfo)
        val credentialId = intent.getStringExtra(EXTRA_CREDENTIAL_ID)
        if (option == null || identity == null || credentialId.isNullOrEmpty()) return refuse(getString(R.string.passkey_refused))
        background({ app.passkeyBroker.check(identity, option.requestJson, create = false) }) { checked ->
            val ok = checked.getOrNull() ?: return@background refuse(getString(R.string.passkey_refused))
            fun sign() = background({ app.session.passkeyAssert(ok.caller, option.requestJson, credentialId, option.clientDataHash) }) { result ->
                relock()
                result.onSuccess { json ->
                    record("signed")
                    val data = Intent()
                    PendingIntentHandler.setGetCredentialResponse(data, GetCredentialResponse(PublicKeyCredential(json)))
                    setResult(Activity.RESULT_OK, data)
                    finish()
                }.onFailure { fail(getString(R.string.passkey_failed)) }
            }
            requireUser(getString(R.string.passkey_sign_in_title), ok.rpId) { sign() }
        }
    }

    override fun fail(reason: String, cancelled: Boolean) {
        if (!refused) record(if (cancelled) "cancelled" else "error")
        val data = Intent()
        PendingIntentHandler.setGetCredentialException(
            data, if (cancelled) GetCredentialCancellationException(reason) else GetCredentialUnknownException(reason),
        )
        setResult(Activity.RESULT_OK, data)
        finish()
    }

    companion object {
        const val EXTRA_CREDENTIAL_ID = "app.gitpass.passkey.CREDENTIAL_ID"
    }
}
