package app.gitpass.android.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import app.gitpass.android.GitpassApplication
import app.gitpass.android.R
import app.gitpass.android.firstpull.RepositorySelection
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Not saveable: all credential fields disappear on lock, background, cancel or Activity disposal. */
@Composable
fun FirstPullScreen(app: GitpassApplication, epoch: Long, padding: PaddingValues, onCancel: () -> Unit) {
    var owner by remember { mutableStateOf("") }
    var repo by remember { mutableStateOf("") }
    var branch by remember { mutableStateOf("") }
    var token by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var secret by remember { mutableStateOf("") }
    var running by remember { mutableStateOf(false) }
    var failed by remember { mutableStateOf(false) }
    var budgetFailure by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    DisposableEffect(Unit) {
        onDispose { app.cancelFirstPull(); token = ""; password = ""; secret = "" }
    }
    Column(Modifier.fillMaxSize().padding(padding).padding(16.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.first_pull), style = MaterialTheme.typography.headlineSmall)
        Text(stringResource(R.string.first_pull_scope))
        Text(stringResource(R.string.first_pull_permission))
        OutlinedTextField(owner, { owner = it }, label = { Text(stringResource(R.string.repository_owner)) }, enabled = !running, singleLine = true, modifier = Modifier.testTag("pull-owner"))
        OutlinedTextField(repo, { repo = it }, label = { Text(stringResource(R.string.repository_name)) }, enabled = !running, singleLine = true, modifier = Modifier.testTag("pull-repo"))
        OutlinedTextField(branch, { branch = it }, label = { Text(stringResource(R.string.repository_branch)) }, enabled = !running, singleLine = true, modifier = Modifier.testTag("pull-branch"))
        for ((tag, label, value) in listOf(Triple("pull-token", R.string.dedicated_pat, token), Triple("pull-password", R.string.master_password, password), Triple("pull-secret", R.string.secret_key, secret))) {
            OutlinedTextField(value, { when (tag) { "pull-token" -> token = it; "pull-password" -> password = it; else -> secret = it } },
                label = { Text(stringResource(label)) }, enabled = !running, singleLine = true,
                visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password), modifier = Modifier.testTag(tag))
        }
        if (failed) Text(stringResource(if (budgetFailure) R.string.first_pull_budget_error else R.string.first_pull_error), modifier = Modifier.testTag("pull-error"))
        if (running) CircularProgressIndicator()
        Button(enabled = !running && listOf(owner, repo, branch, token, password, secret).all { it.isNotEmpty() },
            modifier = Modifier.testTag("pull-start"), onClick = {
                val selected = runCatching { RepositorySelection(owner, repo, branch) }.getOrElse { failed = true; return@Button }
                val typedToken = token
                val typedPassword = password
                val typedSecret = secret
                token = ""; password = ""; secret = ""
                running = true; failed = false; budgetFailure = false
                scope.launch {
                    val result = withContext(Dispatchers.IO) { runCatching { app.firstPull(selected, typedToken, typedPassword, typedSecret, epoch) } }
                    running = false
                    result.onFailure { failed = true; budgetFailure = it.message?.contains("KDF") == true }
                    // Success selects a fully authenticated separate root and re-renders locked UI.
                }
            }) { Text(stringResource(R.string.first_pull_start)) }
        TextButton(onClick = { app.cancelFirstPull(); token = ""; password = ""; secret = ""; onCancel() }, modifier = Modifier.testTag("pull-cancel")) { Text(stringResource(R.string.cancel_edit)) }
        if (!running) {
            val accounts = remember { runCatching { app.accountRoots.accounts() }.getOrDefault(emptyList()) }
            Text(stringResource(R.string.local_accounts))
            accounts.forEachIndexed { index, (name, _) ->
                TextButton(onClick = {
                    token = ""; password = ""; secret = ""
                    scope.launch {
                        val result = withContext(Dispatchers.IO) { runCatching { app.selectLocalAccount(name, epoch) } }
                        result.onFailure { failed = true }
                    }
                }, modifier = Modifier.testTag("pull-account-$index")) {
                    Text(if (name == "legacy") stringResource(R.string.legacy_account) else stringResource(R.string.imported_account, index))
                }
            }
        }
    }
}
