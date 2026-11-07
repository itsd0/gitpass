package app.gitpass.android.ui

import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.gitpass.android.GitpassApplication
import app.gitpass.android.R
import app.gitpass.android.session.AccountCreatedButLocked
import app.gitpass.android.session.AccountSetup
import app.gitpass.android.session.DetailRow
import app.gitpass.android.session.ItemEdit
import app.gitpass.android.session.ItemRow
import app.gitpass.android.session.ItemView
import app.gitpass.android.session.VaultSession
import app.gitpass.android.session.VaultSnapshot
import app.gitpass.android.session.detailRows
import app.gitpass.android.ui.theme.GitpassAccent
import app.gitpass.android.ui.theme.GitpassBackground
import app.gitpass.android.ui.theme.GitpassOnSurface
import app.gitpass.android.ui.theme.GitpassSecondary
import app.gitpass.android.ui.theme.GitpassSurface
import app.gitpass.core.GitpassException
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.delay

private sealed interface Screen {
    data object List : Screen
    data class Detail(val itemId: String) : Screen
    data class Edit(val item: ItemView?) : Screen
}

@Composable
fun GitpassRoot(app: GitpassApplication) {
    val session = app.session
    val setup = app.accountSetup
    val scope = rememberCoroutineScope()
    // Main.immediate: a lock on the main thread (background, Lock button,
    // inactivity) updates this state synchronously, before the next frame.
    val snapshot by session.state.collectAsState(context = Dispatchers.Main.immediate)
    val hasPending by app.pendingSecret.present.collectAsState(context = Dispatchers.Main.immediate)
    val setupState by setup.state.collectAsState(context = Dispatchers.Main.immediate)
    var unlocking by remember { mutableStateOf(false) }
    var errorText by remember { mutableStateOf<String?>(null) }
    var password by remember { mutableStateOf("") }
    var confirmPassword by remember { mutableStateOf("") }
    var secretKey by remember { mutableStateOf("") }
    var showSecretKey by remember { mutableStateOf(false) }
    var copiedNotice by remember { mutableStateOf(false) }
    // Keyed on the lock generation: every lock discards the open screen,
    // revealed fields, and search text, whatever triggered it.
    var screen by remember(snapshot.epoch) { mutableStateOf<Screen>(Screen.List) }
    var revealedKeys by remember(snapshot.epoch) { mutableStateOf(setOf<String>()) }
    var queryText by remember(snapshot.epoch) { mutableStateOf("") }
    var writing by remember(snapshot.epoch) { mutableStateOf(false) }

    val tooShort = stringResource(R.string.password_too_short)
    val mismatch = stringResource(R.string.passwords_do_not_match)
    val context = LocalContext.current
    val root = app.vaultRoot.absolutePath
    val creating = setupState == AccountSetup.State.Creating
    val busy = unlocking || creating

    LaunchedEffect(snapshot.unlocked, snapshot.epoch) {
        // Wakes near the deadline only; the lock itself reaches the UI
        // through session.state, not through this loop.
        while (snapshot.unlocked) {
            val wait = session.millisUntilAutoLock() ?: break
            delay(wait.coerceIn(MIN_TICK_MS, MAX_TICK_MS))
            session.enforceInactivity()
        }
    }

    LaunchedEffect(snapshot.unlocked) {
        if (!snapshot.unlocked) {
            password = ""
            confirmPassword = ""
            secretKey = ""
        }
    }

    LaunchedEffect(setupState) {
        when (val current = setupState) {
            is AccountSetup.State.Failed -> errorText = userVisibleError(current.error)
            AccountSetup.State.Idle -> Unit
            AccountSetup.State.Creating -> {
                password = ""
                confirmPassword = ""
            }
        }
    }

    val pending = if (hasPending) app.pendingSecret.get() else null
    if (pending != null) {
        SecretKeyGate(
            secret = pending,
            copied = copiedNotice,
            onCopy = {
                copySensitive(context, "gitpass", pending)
                copiedNotice = true
            },
            onSaved = {
                setup.confirmSaved()
                copiedNotice = false
            },
        )
        return
    }

    val vaultExists = remember(snapshot.epoch, snapshot.unlocked, hasPending, setupState) {
        File(app.vaultRoot, "format.json").isFile
    }
    val interrupted = remember(snapshot.epoch, snapshot.unlocked, hasPending, setupState) {
        setup.isInterrupted(root)
    }

    Scaffold(
        modifier = Modifier
            .fillMaxSize()
            .pointerInput(snapshot.unlocked) {
                if (!snapshot.unlocked) return@pointerInput
                awaitPointerEventScope {
                    while (true) {
                        awaitPointerEvent(PointerEventPass.Initial)
                        session.noteActivity()
                    }
                }
            },
        containerColor = GitpassBackground,
    ) { padding ->
        if (!snapshot.unlocked) {
            UnlockScreen(
                padding = padding,
                vaultExists = vaultExists,
                interrupted = interrupted,
                password = password,
                confirmPassword = confirmPassword,
                secretKey = secretKey,
                showSecretKey = showSecretKey,
                busy = busy,
                errorText = errorText,
                onPassword = { password = it },
                onConfirm = { confirmPassword = it },
                onSecretKey = { secretKey = it },
                onToggleSecret = { showSecretKey = !showSecretKey },
                onUnlock = {
                    if (busy) return@UnlockScreen
                    errorText = null
                    unlocking = true
                    val typedPassword = password
                    val typedSecret = secretKey
                    scope.launch {
                        val result = withContext(Dispatchers.IO) {
                            runCatching { session.unlock(root, typedPassword, typedSecret) }
                        }
                        unlocking = false
                        result.onSuccess {
                            setup.noteUnlocked()
                            password = ""
                            secretKey = ""
                            errorText = null
                        }.onFailure { error ->
                            errorText = userVisibleError(error)
                        }
                    }
                },
                onCreate = {
                    if (busy) return@UnlockScreen
                    errorText = null
                    if (password.length < VaultSession.MIN_PASSWORD_LENGTH) {
                        errorText = tooShort
                        return@UnlockScreen
                    }
                    if (password != confirmPassword) {
                        errorText = mismatch
                        return@UnlockScreen
                    }
                    // Application-scoped: survives Activity recreation and
                    // hands the Secret Key to app.pendingSecret itself.
                    setup.start(root, password)
                    password = ""
                    confirmPassword = ""
                },
            )
        } else when (val current = screen) {
            Screen.List -> Column {
                Row {
                    TextButton(onClick = { screen = Screen.Edit(null) }, enabled = snapshot.selectedVaultId != null && !snapshot.trash) { Text(stringResource(R.string.new_login)) }
                    TextButton(onClick = { session.setTrash(!snapshot.trash); queryText = "" }) { Text(stringResource(if (snapshot.trash) R.string.active_items else R.string.trash)) }
                }
                ItemListScreen(
                padding = padding,
                snapshot = snapshot,
                query = queryText,
                onQuery = {
                    queryText = it
                    runCatching { session.setQuery(it) }
                },
                onSelectVault = { id ->
                    runCatching { session.selectVault(id) }
                        .onFailure { errorText = userVisibleError(it) }
                },
                onOpen = { row ->
                    revealedKeys = emptySet()
                    runCatching { session.openDetail(row.id) }
                        .onSuccess { screen = Screen.Detail(row.id) }
                        .onFailure { errorText = userVisibleError(it) }
                },
                onLock = { session.lock() },
                errorText = errorText,
            )
            }
            is Screen.Edit -> {
                val vaultId = snapshot.selectedVaultId
                LoginEditor(current.item, writing, errorText,
                    onCancel = { screen = Screen.List },
                    onSave = { edit ->
                        if (vaultId != null && !writing) {
                            val epoch = snapshot.epoch
                            writing = true
                            errorText = null
                            scope.launch {
                                val result = withContext(Dispatchers.IO) {
                                    runCatching { session.saveItem(vaultId, current.item?.id, edit, epoch) }
                                }
                                if (session.state.value.epoch == epoch) {
                                    writing = false
                                    result.onSuccess { screen = Screen.List }
                                        .onFailure { errorText = userVisibleError(it) }
                                }
                            }
                        }
                    })
            }
            is Screen.Detail -> {
                BackHandler {
                    session.clearDetail()
                    revealedKeys = emptySet()
                    screen = Screen.List
                }
                Column {
                    val item = snapshot.detail?.takeIf { it.id == current.itemId }
                    Row {
                        if (item != null && !item.deleted && !item.archived && item.category == "login") {
                            TextButton(onClick = { screen = Screen.Edit(item) }, enabled = !writing) { Text(stringResource(R.string.edit_item)) }
                        }
                        if (item != null) {
                            // Soft deletion only; never remove encrypted files permanently.
                            TextButton(enabled = !writing, onClick = {
                                val epoch = snapshot.epoch
                                writing = true
                                errorText = null
                                scope.launch {
                                    val result = withContext(Dispatchers.IO) {
                                        runCatching { session.setDeleted(item.vaultId, item.id, !item.deleted, epoch) }
                                    }
                                    if (session.state.value.epoch == epoch) {
                                        writing = false
                                        result.onSuccess { screen = Screen.List }
                                            .onFailure { errorText = userVisibleError(it) }
                                    }
                                }
                            }) { Text(stringResource(if (item.deleted) R.string.restore_item else R.string.move_to_trash)) }
                        }
                    }
                ItemDetailScreen(
                    padding = padding,
                    item = snapshot.detail?.takeIf { it.id == current.itemId },
                    revealed = revealedKeys,
                    errorText = errorText,
                    onToggle = { key ->
                        session.noteActivity()
                        revealedKeys = if (key in revealedKeys) revealedKeys - key else revealedKeys + key
                    },
                    onBack = {
                        session.clearDetail()
                        revealedKeys = emptySet()
                        screen = Screen.List
                    },
                    onLock = { session.lock() },
                )
                }
            }
        }
    }
}

private const val MIN_TICK_MS = 250L
private const val MAX_TICK_MS = 1_000L

@Composable
private fun localizedLabel(row: DetailRow): String = when (row.key) {
    "username" -> stringResource(R.string.username)
    "password" -> stringResource(R.string.password)
    "url" -> stringResource(R.string.website)
    "notes" -> stringResource(R.string.notes)
    "totp" -> stringResource(R.string.one_time_password)
    else -> row.label
}

private fun userVisibleError(error: Throwable): String {
    val raw = when (error) {
        is GitpassException.Generic -> error.v1
        is AccountCreatedButLocked -> error.cause?.let { userVisibleError(it) } ?: error.message
        else -> error.message
    }.orEmpty()
    return if (raw.contains("not initialized")) {
        "No vault on this device yet."
    } else {
        raw.ifBlank { "Something went wrong." }
    }
}

@Composable
private fun fieldColors() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = GitpassAccent,
    unfocusedBorderColor = GitpassSecondary,
    focusedLabelColor = GitpassAccent,
    unfocusedLabelColor = GitpassSecondary,
    cursorColor = GitpassAccent,
    focusedTextColor = GitpassOnSurface,
    unfocusedTextColor = GitpassOnSurface,
)

@Composable
private fun UnlockScreen(
    padding: PaddingValues,
    vaultExists: Boolean,
    interrupted: Boolean,
    password: String,
    confirmPassword: String,
    secretKey: String,
    showSecretKey: Boolean,
    busy: Boolean,
    errorText: String?,
    onPassword: (String) -> Unit,
    onConfirm: (String) -> Unit,
    onSecretKey: (String) -> Unit,
    onToggleSecret: () -> Unit,
    onUnlock: () -> Unit,
    onCreate: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(padding)
            .padding(24.dp)
            .verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
    ) {
        Text(
            text = stringResource(R.string.app_name),
            fontSize = 32.sp,
            fontWeight = FontWeight.Bold,
            color = GitpassAccent,
        )
        Text(
            text = stringResource(R.string.app_tagline),
            color = GitpassSecondary,
            fontSize = 14.sp,
        )
        Text(
            text = stringResource(if (vaultExists) R.string.vault_exists_help else R.string.create_account_help),
            color = GitpassSecondary,
            fontSize = 13.sp,
        )
        if (interrupted) {
            Text(
                text = stringResource(R.string.setup_interrupted),
                color = MaterialTheme.colorScheme.error,
                fontSize = 13.sp,
                modifier = Modifier.testTag(TestTags.SETUP_INTERRUPTED),
            )
        }
        OutlinedTextField(
            value = password,
            onValueChange = onPassword,
            modifier = Modifier.fillMaxWidth().testTag(TestTags.MASTER_PASSWORD),
            label = { Text(stringResource(R.string.master_password)) },
            singleLine = true,
            enabled = !busy,
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(
                keyboardType = KeyboardType.Password,
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                imeAction = if (vaultExists) ImeAction.Next else ImeAction.Next,
            ),
            colors = fieldColors(),
        )
        if (vaultExists) {
            OutlinedTextField(
                value = secretKey,
                onValueChange = onSecretKey,
                modifier = Modifier.fillMaxWidth().testTag(TestTags.SECRET_KEY),
                label = { Text(stringResource(R.string.secret_key)) },
                placeholder = { Text(stringResource(R.string.secret_key_hint)) },
                singleLine = true,
                enabled = !busy,
                visualTransformation = if (showSecretKey) VisualTransformation.None else PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Ascii,
                    capitalization = KeyboardCapitalization.None,
                    autoCorrectEnabled = false,
                    imeAction = ImeAction.Done,
                ),
                keyboardActions = KeyboardActions(onDone = { if (password.isNotEmpty() && secretKey.isNotEmpty()) onUnlock() }),
                colors = fieldColors(),
            )
            TextButton(onClick = onToggleSecret, enabled = !busy) {
                Text(stringResource(R.string.show_secret_key), color = GitpassAccent)
            }
        } else {
            OutlinedTextField(
                value = confirmPassword,
                onValueChange = onConfirm,
                modifier = Modifier.fillMaxWidth().testTag(TestTags.CONFIRM_PASSWORD),
                label = { Text(stringResource(R.string.confirm_master_password)) },
                singleLine = true,
                enabled = !busy,
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Password,
                    capitalization = KeyboardCapitalization.None,
                    autoCorrectEnabled = false,
                    imeAction = ImeAction.Done,
                ),
                keyboardActions = KeyboardActions(onDone = { onCreate() }),
                colors = fieldColors(),
            )
        }
        if (errorText != null) {
            Text(errorText, color = MaterialTheme.colorScheme.error, fontSize = 13.sp)
        }
        if (busy) {
            CircularProgressIndicator(color = GitpassAccent)
        }
        if (vaultExists) {
            AccentButton(
                text = stringResource(R.string.unlock),
                enabled = !busy && password.isNotEmpty() && secretKey.isNotEmpty(),
                onClick = onUnlock,
                tag = TestTags.UNLOCK,
            )
        } else {
            AccentButton(
                text = stringResource(R.string.create_account),
                enabled = !busy && password.isNotEmpty() && confirmPassword.isNotEmpty(),
                onClick = onCreate,
                tag = TestTags.CREATE_ACCOUNT,
            )
        }
    }
}

@Composable
private fun ItemListScreen(
    padding: PaddingValues,
    snapshot: VaultSnapshot,
    query: String,
    onQuery: (String) -> Unit,
    onSelectVault: (String) -> Unit,
    onOpen: (ItemRow) -> Unit,
    onLock: () -> Unit,
    errorText: String?,
) {
    var menuOpen by remember { mutableStateOf(false) }
    val selected = snapshot.vaults.firstOrNull { it.id == snapshot.selectedVaultId }
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(padding)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box {
                TextButton(onClick = { menuOpen = true }) {
                    Text(selected?.name ?: stringResource(R.string.vaults), color = GitpassAccent, fontWeight = FontWeight.SemiBold)
                }
                DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                    snapshot.vaults.forEach { vault ->
                        DropdownMenuItem(
                            text = {
                                Text(stringResource(R.string.vault_menu_entry, vault.name, vault.itemCount.toString()))
                            },
                            onClick = {
                                menuOpen = false
                                onSelectVault(vault.id)
                            },
                        )
                    }
                }
            }
            TextButton(onClick = onLock, modifier = Modifier.testTag(TestTags.LOCK)) {
                Text(stringResource(R.string.lock), color = GitpassAccent, fontWeight = FontWeight.Bold)
            }
        }
        OutlinedTextField(
            value = query,
            onValueChange = onQuery,
            modifier = Modifier.fillMaxWidth(),
            label = { Text(stringResource(R.string.search_items)) },
            singleLine = true,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
            colors = fieldColors(),
        )
        if (errorText != null) {
            Text(errorText, color = MaterialTheme.colorScheme.error, fontSize = 13.sp)
        }
        when {
            snapshot.vaults.isEmpty() -> Hint(stringResource(R.string.no_vaults))
            snapshot.items.isEmpty() && query.isBlank() -> Hint(stringResource(R.string.no_items))
            snapshot.items.isEmpty() -> Hint(stringResource(R.string.no_matches))
            else -> LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                items(snapshot.items, key = { it.id }) { row ->
                    Card(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { onOpen(row) },
                        colors = CardDefaults.cardColors(containerColor = GitpassSurface),
                        shape = RoundedCornerShape(12.dp),
                    ) {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text(row.title, color = GitpassOnSurface, fontWeight = FontWeight.SemiBold)
                            val secondary = listOf(row.subtitle, row.category)
                                .filter { it.isNotBlank() }
                                .joinToString(" · ")
                            if (secondary.isNotBlank()) {
                                Text(secondary, color = GitpassSecondary, fontSize = 13.sp)
                            }
                            if (row.favorite || row.hasTotp) {
                                Text(
                                    listOfNotNull(
                                        if (row.favorite) stringResource(R.string.favorite) else null,
                                        if (row.hasTotp) stringResource(R.string.one_time_password) else null,
                                    ).joinToString(" · "),
                                    color = GitpassAccent,
                                    fontSize = 12.sp,
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ItemDetailScreen(
    padding: PaddingValues,
    item: ItemView?,
    revealed: Set<String>,
    errorText: String?,
    onToggle: (String) -> Unit,
    onBack: () -> Unit,
    onLock: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(padding)
            .padding(horizontal = 16.dp, vertical = 12.dp)
            .verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            TextButton(onClick = onBack) { Text(stringResource(R.string.back), color = GitpassAccent) }
            TextButton(onClick = onLock, modifier = Modifier.testTag(TestTags.LOCK)) {
                Text(stringResource(R.string.lock), color = GitpassAccent, fontWeight = FontWeight.Bold)
            }
        }
        if (item == null) {
            Hint(stringResource(R.string.no_items))
            return
        }
        Text(item.title, color = GitpassOnSurface, fontSize = 24.sp, fontWeight = FontWeight.Bold)
        Text(stringResource(R.string.category_value, item.category), color = GitpassSecondary, fontSize = 13.sp)
        if (item.tags.isNotEmpty()) {
            Text(stringResource(R.string.tags_value, item.tags.joinToString(", ")), color = GitpassSecondary, fontSize = 13.sp)
        }
        if (errorText != null) {
            Text(errorText, color = MaterialTheme.colorScheme.error, fontSize = 13.sp)
        }
        var lastSection: String? = null
        detailRows(item).forEach { row ->
            if (row.sectionTitle != null && row.sectionTitle != lastSection) {
                lastSection = row.sectionTitle
                Spacer(Modifier.height(4.dp))
                Text(row.sectionTitle, color = GitpassAccent, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
            }
            DetailField(row, revealed.contains(row.key), onToggle)
        }
    }
}

@Composable
private fun DetailField(row: DetailRow, revealed: Boolean, onToggle: (String) -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = GitpassSurface),
        shape = RoundedCornerShape(12.dp),
    ) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(localizedLabel(row), color = GitpassSecondary, fontSize = 12.sp)
            val shown = when {
                row.value.isEmpty() -> stringResource(R.string.field_empty)
                row.secret && !revealed -> "••••••"
                else -> row.value
            }
            Text(
                text = shown,
                color = GitpassOnSurface,
                fontFamily = if (row.secret && revealed) FontFamily.Monospace else FontFamily.Default,
            )
            if (row.secret && row.value.isNotEmpty()) {
                TextButton(onClick = { onToggle(row.key) }) {
                    Text(
                        stringResource(if (revealed) R.string.hide else R.string.reveal),
                        color = GitpassAccent,
                    )
                }
            }
        }
    }
}

@Composable
private fun SecretKeyGate(
    secret: String,
    copied: Boolean,
    onCopy: () -> Unit,
    onSaved: () -> Unit,
) {
    BackHandler { }
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp)
            .verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
    ) {
        Text(stringResource(R.string.secret_saved_title), color = GitpassAccent, fontSize = 28.sp, fontWeight = FontWeight.Bold)
        Text(stringResource(R.string.secret_saved_body), color = GitpassSecondary, fontSize = 14.sp)
        Text(
            text = secret,
            modifier = Modifier.testTag(TestTags.SECRET_KEY_VALUE),
            color = GitpassOnSurface,
            fontFamily = FontFamily.Monospace,
            fontWeight = FontWeight.Bold,
            fontSize = 16.sp,
        )
        if (copied) {
            Text(stringResource(R.string.copied), color = GitpassSecondary, fontSize = 13.sp)
        }
        AccentButton(text = stringResource(R.string.copy_secret_key), enabled = true, onClick = onCopy)
        AccentButton(text = stringResource(R.string.i_saved_it), enabled = true, onClick = onSaved, tag = TestTags.I_SAVED_IT)
    }
}

@Composable
private fun AccentButton(text: String, enabled: Boolean, onClick: () -> Unit, tag: String? = null) {
    Button(
        onClick = onClick,
        enabled = enabled,
        modifier = Modifier.fillMaxWidth().let { if (tag != null) it.testTag(tag) else it },
        colors = ButtonDefaults.buttonColors(
            containerColor = GitpassAccent,
            contentColor = GitpassBackground,
            disabledContainerColor = GitpassSurface,
            disabledContentColor = GitpassSecondary,
        ),
        shape = RoundedCornerShape(8.dp),
    ) {
        Text(text, fontWeight = FontWeight.Bold, textAlign = TextAlign.Center)
    }
}

@Composable
private fun Hint(text: String) {
    Text(text, color = GitpassSecondary, fontSize = 14.sp)
}

/** Semantics tags for UI tests. Not shown to the user. */
object TestTags {
    const val MASTER_PASSWORD = "master-password"
    const val CONFIRM_PASSWORD = "confirm-master-password"
    const val SECRET_KEY = "secret-key"
    const val UNLOCK = "unlock"
    const val CREATE_ACCOUNT = "create-account"
    const val LOCK = "lock"
    const val SECRET_KEY_VALUE = "secret-key-value"
    const val I_SAVED_IT = "i-saved-it"
    const val SETUP_INTERRUPTED = "setup-interrupted"
}

@Composable
private fun LoginEditor(
    item: ItemView?, busy: Boolean, error: String?,
    onCancel: () -> Unit, onSave: (ItemEdit) -> Unit,
) {
    var title by remember { mutableStateOf(item?.title.orEmpty()) }
    var username by remember { mutableStateOf(item?.username.orEmpty()) }
    var password by remember { mutableStateOf(item?.password.orEmpty()) }
    var url by remember { mutableStateOf(item?.url.orEmpty()) }
    var notes by remember { mutableStateOf(item?.notes.orEmpty()) }
    BackHandler(enabled = !busy, onBack = onCancel)
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(stringResource(if (item == null) R.string.new_login else R.string.edit_item))
        OutlinedTextField(title, { title = it }, label = { Text(stringResource(R.string.item_title)) }, enabled = !busy, singleLine = true)
        OutlinedTextField(username, { username = it }, label = { Text(stringResource(R.string.username)) }, enabled = !busy, singleLine = true)
        OutlinedTextField(password, { password = it }, label = { Text(stringResource(R.string.password)) }, enabled = !busy,
            singleLine = true, visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false))
        OutlinedTextField(url, { url = it }, label = { Text(stringResource(R.string.website)) }, enabled = !busy, singleLine = true)
        OutlinedTextField(notes, { notes = it }, label = { Text(stringResource(R.string.notes)) }, enabled = !busy)
        if (error != null) Text(error, color = MaterialTheme.colorScheme.error)
        Button(enabled = !busy && title.isNotBlank(), onClick = { onSave(ItemEdit(title, username, password, url, notes)) }) { Text(stringResource(R.string.save_item)) }
        TextButton(enabled = !busy, onClick = onCancel) { Text(stringResource(R.string.cancel_edit)) }
    }
}
