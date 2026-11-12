package app.gitpass.android.passkey

import android.app.Activity
import android.content.Intent
import android.os.Handler
import android.os.Looper
import androidx.annotation.RequiresApi
import androidx.credentials.provider.PendingIntentHandler
import app.gitpass.android.R

/**
 * "Unlock gitpass" action of a sign-in request (#477): unlocks the vault,
 * lists the passkeys valid for the caller, and returns them to Credential
 * Manager. If entries were returned, the vault stays open for at most
 * [PasskeyGrants.TTL_MS] so the passkey the user picks next can sign
 * without a second prompt; then it is locked again.
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
                val lockAfter = unlockedHere
                val grant = app.passkeyGrants.issue(checked.map { it.second.rpId }.toSet(), identity.key, lockAfter)
                background({ PasskeyEntries.response(this, app, checked, grant) }) { built ->
                    val response = built.getOrNull()
                    if (response == null || response.credentialEntries.isEmpty()) {
                        app.passkeyGrants.take(grant, "", "") // drop it
                        relock()
                        return@background refuse(getString(R.string.passkey_none, label))
                    }
                    // The vault stays open for the entry the user picks next; the grant's expiry locks it otherwise.
                    unlockedHere = false
                    if (lockAfter) scheduleExpiryLock()
                    record("listed")
                    val data = Intent()
                    PendingIntentHandler.setBeginGetCredentialResponse(data, response)
                    setResult(Activity.RESULT_OK, data)
                    finish()
                }
            }
        }
    }

    private fun scheduleExpiryLock() {
        val application = app
        Handler(Looper.getMainLooper()).postDelayed({
            if (application.passkeyGrants.expire()) application.session.lock()
        }, PasskeyGrants.TTL_MS + 500)
    }

    override fun fail(reason: String, cancelled: Boolean) {
        if (!refused) record(if (cancelled) "cancelled" else "error")
        setResult(Activity.RESULT_CANCELED)
        finish()
    }
}
