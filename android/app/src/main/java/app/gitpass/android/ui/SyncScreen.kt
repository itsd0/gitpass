package app.gitpass.android.ui

import android.text.format.DateUtils
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.gitpass.android.GitpassApplication
import app.gitpass.android.R
import app.gitpass.android.session.SyncTarget
import app.gitpass.android.sync.GitHubSyncTransport
import app.gitpass.android.sync.SyncCoordinator
import app.gitpass.android.ui.theme.GitpassAccent
import app.gitpass.android.ui.theme.GitpassSecondary
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Test tags for the sync UI (#455). */
object SyncTags {
    const val STATUS = "sync-status"
    const val OPEN = "sync-open"
    const val NOW = "sync-now"
    const val OWNER = "sync-owner"
    const val REPO = "sync-repo"
    const val BRANCH = "sync-branch"
    const val TOKEN = "sync-token"
    const val SAVE = "sync-save"
    const val REMOVE = "sync-remove"
    const val CONFLICTS = "sync-conflicts"
    const val ERROR = "sync-error"
}

@Composable
fun syncStatusText(status: SyncCoordinator.Status): String {
    val last = status.lastSuccessMs?.let {
        stringResource(R.string.sync_last, DateUtils.getRelativeTimeSpanString(it, System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS).toString())
    } ?: stringResource(R.string.sync_never)
    return when (status.phase) {
        SyncCoordinator.Phase.NotConfigured -> stringResource(R.string.sync_not_configured)
        SyncCoordinator.Phase.Syncing -> stringResource(R.string.sync_in_progress)
        SyncCoordinator.Phase.Offline -> stringResource(R.string.sync_offline)
        SyncCoordinator.Phase.Failed -> stringResource(R.string.sync_failed, status.error.orEmpty())
        SyncCoordinator.Phase.Idle ->
            if (status.conflicts.isEmpty()) last
            else last + " · " + stringResource(R.string.sync_conflicts, status.conflicts.size)
    }
}

/** One line on the item list: state + entry to the sync screen. */
@Composable
fun SyncStatusLine(app: GitpassApplication, onOpen: () -> Unit) {
    val status by app.sync.status.collectAsState()
    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween) {
        Text(syncStatusText(status), color = if (status.phase == SyncCoordinator.Phase.Failed) MaterialTheme.colorScheme.error else GitpassSecondary,
            fontSize = 13.sp, modifier = Modifier.weight(1f).testTag(SyncTags.STATUS))
        if (status.phase == SyncCoordinator.Phase.Syncing) CircularProgressIndicator(Modifier.padding(4.dp), strokeWidth = 2.dp)
        TextButton(onClick = onOpen, modifier = Modifier.testTag(SyncTags.OPEN)) { Text(stringResource(R.string.sync_open), color = GitpassAccent) }
    }
}

/**
 * Sync status, "Sync now", the last conflict report and the settings form.
 * The token field is never prefilled and is cleared on save, lock and leave.
 */
@Composable
fun SyncScreen(app: GitpassApplication, padding: PaddingValues, onBack: () -> Unit) {
    val status by app.sync.status.collectAsState()
    val saved = remember { app.syncSettings.target() }
    var owner by remember { mutableStateOf(saved?.owner.orEmpty()) }
    var repo by remember { mutableStateOf(saved?.repository.orEmpty()) }
    var branch by remember { mutableStateOf(saved?.branch ?: "main") }
    var token by remember { mutableStateOf("") }
    var hasToken by remember { mutableStateOf(app.syncSettings.hasToken()) }
    var invalid by remember { mutableStateOf(false) }
    var savedNotice by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    BackHandler { token = ""; onBack() }
    DisposableEffect(Unit) { onDispose { token = "" } }
    Column(Modifier.fillMaxSize().padding(padding).padding(16.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.sync_title), style = MaterialTheme.typography.headlineSmall)
        Text(syncStatusText(status), modifier = Modifier.testTag(SyncTags.STATUS),
            color = if (status.phase == SyncCoordinator.Phase.Failed) MaterialTheme.colorScheme.error else GitpassSecondary)
        if (status.phase == SyncCoordinator.Phase.Failed && status.error != null) {
            Text(status.error.orEmpty(), color = MaterialTheme.colorScheme.error, fontSize = 13.sp, modifier = Modifier.testTag(SyncTags.ERROR))
        }
        Button(onClick = { app.sync.syncNow() }, enabled = status.phase != SyncCoordinator.Phase.NotConfigured && status.phase != SyncCoordinator.Phase.Syncing,
            modifier = Modifier.testTag(SyncTags.NOW)) { Text(stringResource(R.string.sync_now)) }
        if (status.conflicts.isNotEmpty()) {
            Column(Modifier.testTag(SyncTags.CONFLICTS)) {
                Text(stringResource(R.string.sync_conflicts_title), fontSize = 14.sp)
                Text(stringResource(R.string.sync_conflicts_help), color = GitpassSecondary, fontSize = 12.sp)
                status.conflicts.forEach { Text("• $it", fontSize = 12.sp, color = GitpassSecondary) }
            }
        }
        Text(stringResource(R.string.sync_scope), color = GitpassSecondary, fontSize = 13.sp)
        OutlinedTextField(owner, { owner = it.trim() }, label = { Text(stringResource(R.string.repository_owner)) }, enabled = !busy, singleLine = true, modifier = Modifier.fillMaxWidth().testTag(SyncTags.OWNER))
        OutlinedTextField(repo, { repo = it.trim() }, label = { Text(stringResource(R.string.repository_name)) }, enabled = !busy, singleLine = true, modifier = Modifier.fillMaxWidth().testTag(SyncTags.REPO))
        OutlinedTextField(branch, { branch = it.trim() }, label = { Text(stringResource(R.string.repository_branch)) }, enabled = !busy, singleLine = true, modifier = Modifier.fillMaxWidth().testTag(SyncTags.BRANCH))
        OutlinedTextField(token, { token = it.trim() }, label = { Text(stringResource(R.string.sync_token)) }, enabled = !busy, singleLine = true,
            visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
            modifier = Modifier.fillMaxWidth().testTag(SyncTags.TOKEN))
        if (hasToken) Text(stringResource(R.string.sync_token_saved), color = GitpassSecondary, fontSize = 12.sp)
        if (invalid) Text(stringResource(R.string.sync_invalid), color = MaterialTheme.colorScheme.error, fontSize = 13.sp)
        if (savedNotice) Text(stringResource(R.string.sync_saved), color = GitpassAccent, fontSize = 13.sp)
        Button(enabled = !busy && owner.isNotEmpty() && repo.isNotEmpty() && branch.isNotEmpty() && (token.isNotEmpty() || hasToken),
            modifier = Modifier.testTag(SyncTags.SAVE), onClick = {
                val target = runCatching { SyncTarget(owner, repo, branch) }.getOrNull()
                val typed = token.ifEmpty { null }
                token = ""
                if (target == null || (typed != null && !GitHubSyncTransport.validToken(typed))) {
                    invalid = true; savedNotice = false
                    return@Button
                }
                busy = true; invalid = false
                scope.launch {
                    val result = withContext(Dispatchers.IO) { runCatching { app.syncSettings.save(target, typed) } }
                    busy = false
                    result.onSuccess {
                        hasToken = true; savedNotice = true
                        app.sync.settingsChanged()
                    }.onFailure { invalid = true; savedNotice = false }
                }
            }) { Text(stringResource(R.string.sync_save)) }
        if (saved != null || hasToken) {
            TextButton(enabled = !busy, modifier = Modifier.testTag(SyncTags.REMOVE), onClick = {
                token = ""
                app.syncSettings.clear()
                hasToken = false; savedNotice = false
                app.sync.settingsChanged()
            }) { Text(stringResource(R.string.sync_remove)) }
        }
        TextButton(onClick = { token = ""; onBack() }) { Text(stringResource(R.string.back)) }
    }
}
