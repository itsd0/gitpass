package app.gitpass.android.autofill

import android.app.Activity
import android.app.assist.AssistStructure
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.service.autofill.Dataset
import android.service.autofill.FillResponse
import android.view.WindowManager
import android.view.autofill.AutofillManager
import android.view.autofill.AutofillValue
import android.widget.RemoteViews
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.gitpass.android.GitpassApplication
import app.gitpass.android.R
import app.gitpass.android.ui.theme.GitpassTheme

/**
 * Authentication step of an autofill fill request (#475). Unlocks the vault
 * (biometric or Master Password), asks gitpass-core for the logins that
 * match the requesting app or site, and returns them as datasets. Only now
 * do secrets enter a Dataset. No match means nothing is returned. If this
 * activity unlocked the vault, it locks it again before finishing.
 */
class AutofillAuthActivity : ComponentActivity() {
    private var unlockedHere = false
    private var message by mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
        val app = application as GitpassApplication
        val target = requestTarget(intent)
        if (target == null || app.bridgeIncompatible || app.accountSelectionDamaged) {
            setResult(Activity.RESULT_CANCELED)
            finish()
            return
        }
        val wasUnlocked = app.session.isUnlocked()
        if (wasUnlocked) {
            deliver(app, target)
            return
        }
        setContent {
            GitpassTheme {
                val text = message
                if (text == null) {
                    AutofillUnlock(
                        app = app,
                        title = stringResource(R.string.autofill_unlock_to_fill),
                        target = target.label,
                        onUnlocked = {
                            unlockedHere = true
                            deliver(app, target)
                        },
                        onCancel = { cancel(app) },
                    )
                } else {
                    Column(Modifier.padding(24.dp)) {
                        Text(text, modifier = Modifier.testTag(AutofillTags.MESSAGE))
                        TextButton(onClick = { cancel(app) }, modifier = Modifier.testTag(AutofillTags.CANCEL)) { Text(stringResource(R.string.autofill_cancel)) }
                    }
                }
            }
        }
    }

    /** Builds the datasets from matching logins only, then locks again if this activity unlocked. */
    private fun deliver(app: GitpassApplication, target: AutofillTarget) {
        val usernameIds = GitpassAutofillService.ids(intent, EXTRA_USERNAME_IDS)
        val passwordIds = GitpassAutofillService.ids(intent, EXTRA_PASSWORD_IDS)
        val plans = try {
            FillPlan.datasets(app.session.autofillLogins(target), usernameIds, passwordIds)
        } catch (_: Exception) {
            emptyList()
        } finally {
            if (unlockedHere) app.session.lock()
        }
        lastDatasetCount = plans.size
        if (plans.isEmpty()) {
            message = getString(R.string.autofill_no_match, target.label)
            if (!unlockedHere && app.session.isUnlocked()) cancel(app) // nothing to show over an already-open vault
            return
        }
        val response = FillResponse.Builder()
        for (plan in plans) {
            val presentation = RemoteViews(packageName, R.layout.autofill_item).apply {
                setTextViewText(R.id.autofill_title, plan.title)
                setTextViewText(R.id.autofill_subtitle, plan.subtitle)
            }
            val dataset = Dataset.Builder(presentation)
            @Suppress("DEPRECATION")
            for ((id, value) in plan.values) dataset.setValue(id, AutofillValue.forText(value))
            response.addDataset(dataset.build())
        }
        setResult(Activity.RESULT_OK, Intent().putExtra(AutofillManager.EXTRA_AUTHENTICATION_RESULT, response.build()))
        lastResultHadFillResponse = true
        finish()
    }

    private fun cancel(app: GitpassApplication) {
        if (unlockedHere) app.session.lock()
        setResult(Activity.RESULT_CANCELED)
        finish()
    }

    override fun onStop() {
        super.onStop()
        // Never leave a vault opened for autofill unlocked in the background.
        if (unlockedHere) (application as GitpassApplication).session.lock()
    }

    companion object {
        const val EXTRA_USERNAME_IDS = "app.gitpass.autofill.USERNAME_IDS"
        const val EXTRA_PASSWORD_IDS = "app.gitpass.autofill.PASSWORD_IDS"
        const val EXTRA_WEB_ORIGIN = "app.gitpass.autofill.WEB_ORIGIN"
        const val EXTRA_PACKAGE = "app.gitpass.autofill.PACKAGE"

        /** Test hook: how many datasets the last request returned. Never holds values. */
        @Volatile var lastDatasetCount: Int = -1

        /**
         * Test hook: true once a FillResponse was set as the result. (The response itself can be parceled only once,
         * which ActivityScenario's result plumbing would break, and it holds secrets, so it is never kept here.)
         */
        @Volatile var lastResultHadFillResponse: Boolean = false

        /**
         * The target this request is for. When the framework attached the
         * view structure, the target is re-derived from it and must equal the
         * one the service put in the Intent; otherwise the request is refused.
         */
        fun requestTarget(intent: Intent): AutofillTarget? {
            val claimed = AutofillTarget(intent.getStringExtra(EXTRA_WEB_ORIGIN), intent.getStringExtra(EXTRA_PACKAGE))
            if (claimed.webOrigin == null && claimed.androidPackage == null) return null
            val structure = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(AutofillManager.EXTRA_ASSIST_STRUCTURE, AssistStructure::class.java)
            else @Suppress("DEPRECATION") intent.getParcelableExtra(AutofillManager.EXTRA_ASSIST_STRUCTURE)
            if (structure != null) {
                val snapshot = StructureReader.read(structure)
                val actual = AutofillTarget.of(snapshot.packageName, snapshot.webDomain, snapshot.webScheme)
                if (actual != claimed) return null
            }
            return claimed
        }
    }
}
