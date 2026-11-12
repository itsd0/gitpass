package app.gitpass.android.autofill

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import app.gitpass.android.GitpassApplication
import app.gitpass.android.R
import app.gitpass.android.biometric.BiometricResult
import app.gitpass.android.biometric.QuickUnlockInvalidatedException
import app.gitpass.android.session.QuickUnlockStaleException
import app.gitpass.core.GitpassException
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Test tags for the autofill unlock screens. */
object AutofillTags {
    const val PASSWORD = "autofill-password"
    const val SECRET_KEY = "autofill-secret-key"
    const val UNLOCK = "autofill-unlock"
    const val BIOMETRIC = "autofill-biometric"
    const val CANCEL = "autofill-cancel"
    const val MESSAGE = "autofill-message"
    const val SAVE = "autofill-save"
}

/**
 * Unlock step shared by the fill and save activities (#475) and the passkey
 * provider (#477): biometric unlock when it is set up (#473), otherwise
 * Master Password + Secret Key. Calls [onUnlocked] on the main thread once
 * the session is open. Nothing from the vault is shown before that.
 *
 * With [verifyOnly] (the vault is already open) the same credentials are
 * checked again without touching the open session, so the user is verified
 * freshly for this request.
 */
@Composable
fun AutofillUnlock(app: GitpassApplication, title: String, target: String, onUnlocked: () -> Unit, onCancel: () -> Unit, verifyOnly: Boolean = false) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var password by remember { mutableStateOf("") }
    var secretKey by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val quick = app.quickUnlock
    var quickEnabled by remember { mutableStateOf(quick.isEnabled()) }
    val biometric = remember(quickEnabled) { quickEnabled && app.biometricGate.available(context) }
    val promptTitle = stringResource(R.string.biometric_prompt_unlock)
    val usePassword = stringResource(R.string.biometric_use_password)
    val invalidated = stringResource(R.string.biometric_invalidated)
    val root = app.vaultRoot.absolutePath

    fun failure(e: Throwable): String = when (e) {
        is GitpassException.Generic -> e.v1
        else -> e.message ?: "Something went wrong."
    }

    fun dropQuick() {
        quick.disable()
        quickEnabled = false
        error = invalidated
    }

    Column(
        Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(title, style = MaterialTheme.typography.titleLarge)
        Text(target, style = MaterialTheme.typography.bodyMedium)
        if (biometric) {
            Button(
                onClick = {
                    if (busy) return@Button
                    error = null
                    val cipher = try { quick.beginUnlock() } catch (_: QuickUnlockInvalidatedException) { dropQuick(); return@Button }
                    busy = true
                    app.biometricGate.authenticate(context, cipher, promptTitle, usePassword) { result ->
                        if (result !is BiometricResult.Success) {
                            busy = false
                            if (result is BiometricResult.Failed) error = result.message
                            return@authenticate
                        }
                        val key = try { quick.finishUnlock(result.cipher) } catch (_: QuickUnlockInvalidatedException) { busy = false; dropQuick(); return@authenticate }
                        scope.launch(start = CoroutineStart.ATOMIC) {
                            val outcome = try {
                                withContext(Dispatchers.IO) { runCatching { if (verifyOnly) app.session.verifyWithKey(root, key) else app.session.unlockWithKey(root, key) } }
                            } finally { key.fill(0) }
                            busy = false
                            outcome.onSuccess { onUnlocked() }.onFailure { e -> if (e is QuickUnlockStaleException) dropQuick() else error = failure(e) }
                        }
                    }
                },
                enabled = !busy,
                modifier = Modifier.fillMaxWidth().testTag(AutofillTags.BIOMETRIC),
            ) { Text(stringResource(R.string.biometric_unlock)) }
        }
        OutlinedTextField(
            value = password, onValueChange = { password = it }, enabled = !busy, singleLine = true,
            label = { Text(stringResource(R.string.master_password)) },
            visualTransformation = PasswordVisualTransformation(),
            modifier = Modifier.fillMaxWidth().testTag(AutofillTags.PASSWORD),
        )
        OutlinedTextField(
            value = secretKey, onValueChange = { secretKey = it }, enabled = !busy, singleLine = true,
            label = { Text(stringResource(R.string.secret_key)) },
            visualTransformation = PasswordVisualTransformation(),
            modifier = Modifier.fillMaxWidth().testTag(AutofillTags.SECRET_KEY),
        )
        error?.let { Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag(AutofillTags.MESSAGE)) }
        if (busy) CircularProgressIndicator()
        Button(
            onClick = {
                if (busy) return@Button
                error = null
                busy = true
                val p = password
                val s = secretKey
                scope.launch {
                    val outcome = withContext(Dispatchers.IO) { runCatching { if (verifyOnly) app.session.verify(root, p, s) else app.session.unlock(root, p, s) } }
                    busy = false
                    password = ""
                    secretKey = ""
                    outcome.onSuccess { onUnlocked() }.onFailure { error = failure(it) }
                }
            },
            enabled = !busy && password.isNotEmpty() && secretKey.isNotEmpty(),
            modifier = Modifier.fillMaxWidth().testTag(AutofillTags.UNLOCK),
        ) { Text(stringResource(R.string.unlock)) }
        TextButton(onClick = onCancel, modifier = Modifier.testTag(AutofillTags.CANCEL)) { Text(stringResource(R.string.autofill_cancel)) }
    }
}
