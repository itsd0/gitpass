package app.gitpass.android.autofill

import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.os.CancellationSignal
import android.service.autofill.AutofillService
import android.service.autofill.FillCallback
import android.service.autofill.FillRequest
import android.service.autofill.FillResponse
import android.service.autofill.SaveCallback
import android.service.autofill.SaveInfo
import android.service.autofill.SaveRequest
import android.view.autofill.AutofillId
import android.widget.RemoteViews
import app.gitpass.android.GitpassApplication
import app.gitpass.android.R

/**
 * Password AutofillService (#475; passwords only, no passkeys).
 *
 * A fill request never gets a secret from this service: the only response is
 * one authentication entry ("Unlock gitpass") whose Intent opens
 * [AutofillAuthActivity]. Datasets with values are built there, after an
 * unlock (biometric from #473 or the Master Password), and only from items
 * whose URL matches the request ([AutofillTarget] + gitpass-core rules).
 * A save request hands the typed values to [AutofillSaveActivity] through an
 * in-memory [SaveStash] token, again behind an unlock.
 */
class GitpassAutofillService : AutofillService() {
    override fun onFillRequest(request: FillRequest, cancellationSignal: CancellationSignal, callback: FillCallback) {
        val structure = request.fillContexts.lastOrNull()?.structure ?: return callback.onSuccess(null)
        val snapshot = StructureReader.read(structure)
        val target = AutofillTarget.of(snapshot.packageName, snapshot.webDomain, snapshot.webScheme)
        val fields = FieldParser.parse(snapshot.nodes)
        if (target == null || fields.isEmpty) return callback.onSuccess(null)
        val usernameIds = fields.usernames.mapNotNull { it.id }.toTypedArray()
        val passwordIds = fields.passwords.mapNotNull { it.id }.toTypedArray()
        val allIds = (usernameIds + passwordIds)
        val intent = Intent(this, AutofillAuthActivity::class.java)
            .putExtra(AutofillAuthActivity.EXTRA_USERNAME_IDS, usernameIds)
            .putExtra(AutofillAuthActivity.EXTRA_PASSWORD_IDS, passwordIds)
            .putExtra(AutofillAuthActivity.EXTRA_WEB_ORIGIN, target.webOrigin)
            .putExtra(AutofillAuthActivity.EXTRA_PACKAGE, target.androidPackage)
        // Mutable: the framework adds EXTRA_ASSIST_STRUCTURE. Explicit component, own app only.
        val flags = PendingIntent.FLAG_CANCEL_CURRENT or (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
        val sender = PendingIntent.getActivity(this, request.id, intent, flags).intentSender
        val presentation = RemoteViews(packageName, R.layout.autofill_item).apply {
            setTextViewText(R.id.autofill_title, getString(R.string.autofill_unlock_to_fill))
            setTextViewText(R.id.autofill_subtitle, target.label)
        }
        val response = FillResponse.Builder().setAuthentication(allIds, sender, presentation)
        if (passwordIds.isNotEmpty()) {
            val type = SaveInfo.SAVE_DATA_TYPE_PASSWORD or (if (usernameIds.isNotEmpty()) SaveInfo.SAVE_DATA_TYPE_USERNAME else 0)
            response.setSaveInfo(
                SaveInfo.Builder(type, passwordIds)
                    .apply { if (usernameIds.isNotEmpty()) setOptionalIds(usernameIds) }
                    .setFlags(SaveInfo.FLAG_SAVE_ON_ALL_VIEWS_INVISIBLE)
                    .build(),
            )
        }
        callback.onSuccess(response.build())
    }

    override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) {
        val structure = request.fillContexts.lastOrNull()?.structure ?: return callback.onSuccess()
        val snapshot = StructureReader.read(structure, withText = true)
        val target = AutofillTarget.of(snapshot.packageName, snapshot.webDomain, snapshot.webScheme) ?: return callback.onSuccess()
        val fields = FieldParser.parse(snapshot.nodes)
        val password = fields.passwords.firstNotNullOfOrNull { it.text?.takeIf(String::isNotEmpty) } ?: return callback.onSuccess()
        val username = fields.usernames.firstNotNullOfOrNull { it.text?.takeIf(String::isNotBlank) }.orEmpty().trim()
        if (Build.VERSION.SDK_INT < 28) {
            // No way to start an unlock from a save request before Android 9.
            callback.onFailure(getString(R.string.autofill_save_needs_android9))
            return
        }
        val token = (application as GitpassApplication).autofillSaves.put(target, username, password)
        val intent = Intent(this, AutofillSaveActivity::class.java).putExtra(AutofillSaveActivity.EXTRA_TOKEN, token)
        val sender = PendingIntent.getActivity(this, token.hashCode(), intent, PendingIntent.FLAG_CANCEL_CURRENT or PendingIntent.FLAG_IMMUTABLE).intentSender
        callback.onSuccess(sender)
    }

    companion object {
        fun ids(intent: Intent, key: String): List<AutofillId> =
            if (Build.VERSION.SDK_INT >= 33) intent.getParcelableArrayExtra(key, AutofillId::class.java)?.toList().orEmpty()
            else @Suppress("DEPRECATION") intent.getParcelableArrayExtra(key)?.filterIsInstance<AutofillId>().orEmpty()
    }
}
