package app.gitpass.android.autofill

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.gitpass.android.GitpassApplication
import app.gitpass.android.R
import app.gitpass.android.ui.theme.GitpassTheme

/**
 * Save step of an autofill save request (#475): unlock, then confirm saving
 * the typed login for the app or site (updating the password of an item with
 * the same username, otherwise creating one). Values come from the in-memory
 * [SaveStash], never from the Intent. Locks again before finishing.
 */
class AutofillSaveActivity : ComponentActivity() {
    private var unlockedHere = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
        val app = application as GitpassApplication
        val pending = app.autofillSaves.take(intent.getStringExtra(EXTRA_TOKEN))
        if (pending == null || app.bridgeIncompatible || app.accountSelectionDamaged) {
            finish()
            return
        }
        setContent {
            GitpassTheme {
                var unlocked by remember { mutableStateOf(app.session.isUnlocked()) }
                var result by remember { mutableStateOf<String?>(null) }
                if (!unlocked) {
                    AutofillUnlock(
                        app = app,
                        title = stringResource(R.string.autofill_unlock_to_save),
                        target = pending.target.label,
                        onUnlocked = { unlockedHere = true; unlocked = true },
                        onCancel = { done(app) },
                    )
                } else {
                    Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Text(stringResource(R.string.autofill_save_question, pending.target.label))
                        if (pending.username.isNotEmpty()) Text(pending.username)
                        result?.let { Text(it, modifier = Modifier.testTag(AutofillTags.MESSAGE)) }
                        Button(
                            onClick = {
                                result = runCatching { app.session.saveAutofillLogin(pending.target, pending.username, pending.password) }
                                    .fold({ getString(R.string.autofill_saved, it) }, { getString(R.string.autofill_save_failed) })
                                done(app)
                            },
                            modifier = Modifier.fillMaxWidth().testTag(AutofillTags.SAVE),
                        ) { Text(stringResource(R.string.autofill_save)) }
                        TextButton(onClick = { done(app) }, modifier = Modifier.testTag(AutofillTags.CANCEL)) { Text(stringResource(R.string.autofill_not_now)) }
                    }
                }
            }
        }
    }

    private fun done(app: GitpassApplication) {
        if (unlockedHere) app.session.lock()
        finish()
    }

    override fun onStop() {
        super.onStop()
        if (unlockedHere) (application as GitpassApplication).session.lock()
    }

    companion object {
        const val EXTRA_TOKEN = "app.gitpass.autofill.SAVE_TOKEN"
    }
}
