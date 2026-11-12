package app.gitpass.android.passkey

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.annotation.RequiresApi
import androidx.credentials.provider.BeginGetCredentialRequest
import androidx.credentials.provider.BeginGetCredentialResponse
import androidx.credentials.provider.BeginGetPublicKeyCredentialOption
import androidx.credentials.provider.CredentialEntry
import androidx.credentials.provider.PublicKeyCredentialEntry
import app.gitpass.android.GitpassApplication
import java.time.Instant
import java.util.concurrent.atomic.AtomicInteger

/** Builds the sign-in entries for a begin-get request from an unlocked vault (#477). Never on the main thread. */
@RequiresApi(34)
object PasskeyEntries {
    const val MAX_ENTRIES = 20
    private val requestCodes = AtomicInteger(0x6b00)

    fun nextRequestCode(): Int = requestCodes.incrementAndGet()

    /** Valid public-key options for [identity] with their checked rpIds. */
    fun checkedOptions(app: GitpassApplication, identity: CallerIdentity, request: BeginGetCredentialRequest) =
        request.beginGetCredentialOptions.filterIsInstance<BeginGetPublicKeyCredentialOption>().mapNotNull { option ->
            runCatching { option to app.passkeyBroker.check(identity, option.requestJson, create = false) }.getOrNull()
        }

    /** Entries (names only) for every matching passkey; each opens [PasskeyGetActivity], which verifies the user again. */
    fun response(context: Context, app: GitpassApplication, checked: List<Pair<BeginGetPublicKeyCredentialOption, PasskeyBroker.Checked>>): BeginGetCredentialResponse {
        val entries = mutableListOf<CredentialEntry>()
        for ((option, ok) in checked) {
            for (e in app.session.passkeyEntries(ok.caller, option.requestJson)) {
                if (entries.size >= MAX_ENTRIES) break
                val intent = Intent(context, PasskeyGetActivity::class.java)
                    .putExtra(PasskeyGetActivity.EXTRA_CREDENTIAL_ID, e.credentialId)
                val pending = PendingIntent.getActivity(context, nextRequestCode(), intent, PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
                val builder = PublicKeyCredentialEntry.Builder(context, e.userName.ifEmpty { ok.rpId }, pending, option)
                    .setDisplayName(e.vaultName.ifEmpty { null })
                if (e.lastUsed > 0u) builder.setLastUsedTime(Instant.ofEpochSecond(e.lastUsed.toLong()))
                entries += builder.build()
            }
        }
        return BeginGetCredentialResponse(credentialEntries = entries)
    }
}
