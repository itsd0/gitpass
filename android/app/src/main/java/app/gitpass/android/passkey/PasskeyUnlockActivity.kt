package app.gitpass.android.passkey

import android.app.Activity
import android.content.Intent
import androidx.annotation.RequiresApi
import androidx.credentials.provider.PendingIntentHandler
import app.gitpass.android.R

/**
 * "Unlock gitpass" action of a sign-in request while the vault is locked
 * (#477): unlocks, lists the passkeys valid for the caller (names only),
 * locks again and returns the list to Credential Manager. Signing with the
 * one the user picks verifies the user again in [PasskeyGetActivity]; the
 * vault is never left open between the two steps (the app also locks
 * whenever it leaves the foreground).
 */
@RequiresApi(34)
class PasskeyUnlockActivity : PasskeyActivity() {
    override fun start() {
        val request = runCatching { PendingIntentHandler.retrieveBeginGetCredentialRequest(intent) }.getOrNull()
        val identity = PasskeyCallers.identify(this, request?.callingAppInfo)
        if (request == null || identity == null) return refuse(getString(R.string.passkey_refused))
        background({ PasskeyEntries.checkedOptions(app, identity, request) }) { result ->
            val checked = result.getOrNull().orEmpty()
            if (checked.isEmpty()) return@background refuse(getString(R.string.passkey_refused))
            val label = checked.map { it.second.rpId }.distinct().joinToString(", ")
            requireUser(getString(R.string.passkey_unlock_title), label) {
                background({ PasskeyEntries.response(this, app, checked) }) { built ->
                    relock()
                    val response = built.getOrNull()
                    if (response == null || response.credentialEntries.isEmpty()) {
                        return@background refuse(getString(R.string.passkey_none, label))
                    }
                    record("listed")
                    val data = Intent()
                    PendingIntentHandler.setBeginGetCredentialResponse(data, response)
                    setResult(Activity.RESULT_OK, data)
                    finish()
                }
            }
        }
    }

    override fun fail(reason: String, cancelled: Boolean) {
        if (!refused) record(if (cancelled) "cancelled" else "error")
        setResult(Activity.RESULT_CANCELED)
        finish()
    }
}
