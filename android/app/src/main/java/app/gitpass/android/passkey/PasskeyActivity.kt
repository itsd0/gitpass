package app.gitpass.android.passkey

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.annotation.RequiresApi
import androidx.annotation.VisibleForTesting
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
import androidx.lifecycle.lifecycleScope
import app.gitpass.android.GitpassApplication
import app.gitpass.android.R
import app.gitpass.android.autofill.AutofillTags
import app.gitpass.android.autofill.AutofillUnlock
import app.gitpass.android.ui.theme.GitpassTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Shared shell of the passkey activities (#477): screenshots blocked, the
 * user is verified (unlock, or a fresh check when the vault is already open)
 * before [requireUser]'s action runs, and a vault this activity unlocked is
 * locked again when it finishes or goes to the background.
 */
@RequiresApi(34)
abstract class PasskeyActivity : ComponentActivity() {
    protected val app get() = application as GitpassApplication
    protected var unlockedHere = false
    /** The request was refused before any unlock. */
    protected var refused = false
    private var message by mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
        if (app.bridgeIncompatible || app.accountSelectionDamaged) {
            refused = true
            record("refused")
            fail(getString(R.string.passkey_unavailable))
            return
        }
        start()
    }

    /** Validate the request (off the main thread as needed), then call [requireUser] or [refuse]. */
    protected abstract fun start()

    /** Return an error or cancellation to Credential Manager and finish. */
    protected abstract fun fail(reason: String, cancelled: Boolean = false)

    /** Runs [block] on IO, then [then] on the main thread with the outcome. */
    protected fun <T> background(block: () -> T, then: (Result<T>) -> Unit) {
        lifecycleScope.launch {
            val r = withContext(Dispatchers.IO) { runCatching(block) }
            then(r)
        }
    }

    protected fun requireUser(title: String, target: String, action: () -> Unit) {
        val verifyOnly = app.session.isUnlocked()
        setContent {
            GitpassTheme {
                val text = message
                if (text == null) {
                    AutofillUnlock(
                        app = app, title = title, target = target, verifyOnly = verifyOnly,
                        onUnlocked = {
                            if (!verifyOnly) unlockedHere = true
                            action()
                        },
                        onCancel = { relock(); fail("cancelled", cancelled = true) },
                    )
                } else {
                    MessageScreen(text)
                }
            }
        }
    }

    /** The request is not allowed: say so, return an error when closed. Nothing was unlocked. */
    protected fun refuse(text: String) {
        refused = true
        record("refused")
        message = text
        setContent { GitpassTheme { MessageScreen(message ?: text) } }
    }

    @androidx.compose.runtime.Composable
    private fun MessageScreen(text: String) {
        Column(Modifier.padding(24.dp)) {
            Text(text, modifier = Modifier.testTag(AutofillTags.MESSAGE))
            TextButton(onClick = { fail(text) }, modifier = Modifier.testTag(AutofillTags.CANCEL)) { Text(stringResource(R.string.autofill_cancel)) }
        }
    }

    protected fun relock() {
        if (unlockedHere) {
            unlockedHere = false
            app.session.lock()
        }
    }

    protected fun record(outcome: String) { lastOutcome = outcome }

    override fun onStop() {
        super.onStop()
        // Never leave a vault opened for a passkey request unlocked in the background.
        relock()
    }

    companion object {
        /** Test hook: "refused", "created", "signed", "listed", "cancelled" or "error". Never holds request data. */
        @VisibleForTesting @Volatile var lastOutcome: String? = null
    }
}
