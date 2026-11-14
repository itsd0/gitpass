package app.gitpass.android.ui

import android.Manifest
import android.content.pm.PackageManager
import android.os.Handler
import android.os.Looper
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.VisibleForTesting
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import app.gitpass.android.GitpassApplication
import app.gitpass.android.R
import app.gitpass.android.firstpull.RepositorySelection
import app.gitpass.android.pairing.PairingClients
import app.gitpass.android.pairing.PairingQr
import app.gitpass.android.pairing.allowlisted
import app.gitpass.android.session.SyncTarget
import app.gitpass.core.PairedSync
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Test stand-in for the camera (#491). Instrumented tests emit a scanned QR here. */
@VisibleForTesting
object PairingScanInjector {
    @Volatile var cameraEnabled: Boolean = true
    @Volatile var frames = MutableSharedFlow<String>(extraBufferCapacity = 4)

    fun reset() {
        cameraEnabled = true
        frames = MutableSharedFlow(extraBufferCapacity = 4)
    }
}

private sealed interface PairStep {
    data object Scan : PairStep
    data object Connecting : PairStep
    class Code(val code: String) : PairStep {
        override fun toString() = "Code([redacted])"
    }
    class Password(val owner: String, val repo: String, val branch: String, val paired: PairedSync) : PairStep {
        override fun toString() = "Password([redacted])"
    }
    data class Failed(val message: Int) : PairStep
}

internal fun pairingErrorRes(message: String): Int = when (allowlisted(message)) {
    "pairing session expired" -> R.string.pair_expired
    "pairing could not connect" -> R.string.pair_connect
    "pairing timed out" -> R.string.pair_timeout
    "pairing connection closed" -> R.string.pair_closed
    else -> R.string.pair_failed
}

/**
 * Set up this phone from the desktop QR (#491). The activity is FLAG_SECURE.
 * Crypto stays in gitpass-core. The Master Password is typed here and is not
 * sent on the socket.
 */
@Composable
fun PairingSetupScreen(app: GitpassApplication, epoch: Long, padding: PaddingValues, onCancel: () -> Unit) {
    val context = LocalContext.current
    val stateDir = remember { File(context.noBackupFilesDir, "pairing").apply { mkdirs() } }
    var attempt by remember { mutableStateOf(0) }
    val client = remember(attempt) { PairingClients.create(stateDir) }
    val gen = remember { AtomicInteger(0) }
    val scope = rememberCoroutineScope()
    var step by remember { mutableStateOf<PairStep>(PairStep.Scan) }
    var expiresAt by remember { mutableStateOf<Long?>(null) }
    var remaining by remember { mutableStateOf<Long?>(null) }
    var password by remember { mutableStateOf("") }
    var running by remember { mutableStateOf(false) }
    var hint by remember { mutableStateOf<Int?>(null) }

    DisposableEffect(client) { onDispose { client.close() } }

    fun restart() {
        gen.incrementAndGet()
        (step as? PairStep.Password)?.paired?.close()
        password = ""
        hint = null
        expiresAt = null
        running = false
        step = PairStep.Scan
        attempt += 1
    }

    fun leave() {
        gen.incrementAndGet()
        (step as? PairStep.Password)?.paired?.close()
        password = ""
        onCancel()
    }

    LaunchedEffect(expiresAt) {
        val exp = expiresAt ?: return@LaunchedEffect
        while (true) {
            val left = exp - System.currentTimeMillis() / 1000
            remaining = left.coerceAtLeast(0)
            if (left <= 0) {
                val current = step
                if (current is PairStep.Connecting || current is PairStep.Code) {
                    gen.incrementAndGet()
                    client.close()
                    step = PairStep.Failed(R.string.pair_expired)
                }
                break
            }
            delay(1000)
        }
    }

    fun onScanned(text: String) {
        if (step !is PairStep.Scan || !PairingQr.isDesktopQr(text)) return
        val ticket = gen.get()
        val link = client
        step = PairStep.Connecting
        hint = null
        scope.launch {
            try {
                val connected = withContext(Dispatchers.IO) { link.connectUntilCode(text) }
                if (!isActive || gen.get() != ticket) return@launch
                step = PairStep.Code(connected.code)
                expiresAt = connected.expiresAt
                val paired = withContext(Dispatchers.IO) { link.receiveGrant() }
                if (!isActive || gen.get() != ticket) {
                    paired.close()
                    return@launch
                }
                step = PairStep.Password(paired.repoOwner(), paired.repoName(), paired.branch(), paired)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                if (isActive && gen.get() == ticket) step = PairStep.Failed(pairingErrorRes(e.message ?: ""))
            }
        }
    }

    Column(
        Modifier.fillMaxSize().padding(padding).padding(16.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(stringResource(R.string.pair_title), style = MaterialTheme.typography.headlineSmall, modifier = Modifier.testTag("pair-title"))
        when (val current = step) {
            PairStep.Scan -> {
                Text(stringResource(R.string.pair_scan), modifier = Modifier.testTag("pair-scan"))
                QrScanner(onText = ::onScanned, modifier = Modifier.fillMaxWidth().height(320.dp))
            }
            PairStep.Connecting -> {
                Text(stringResource(R.string.pair_connecting))
                CircularProgressIndicator()
            }
            is PairStep.Code -> {
                Text(stringResource(R.string.pair_code_help))
                Text(
                    current.code.chunked(3).joinToString(" "),
                    fontFamily = FontFamily.Monospace,
                    fontSize = 40.sp,
                    modifier = Modifier.align(Alignment.CenterHorizontally).testTag("pair-code"),
                )
                Text(stringResource(R.string.pair_waiting), modifier = Modifier.testTag("pair-waiting"))
                remaining?.let { Text(stringResource(R.string.pair_expires_in, it.toInt())) }
            }
            is PairStep.Password -> {
                Text(
                    stringResource(R.string.pair_received, "${current.owner}/${current.repo}", current.branch),
                    modifier = Modifier.testTag("pair-repo"),
                )
                Text(stringResource(R.string.pair_password_hint))
                OutlinedTextField(
                    password,
                    { password = it },
                    label = { Text(stringResource(R.string.master_password)) },
                    enabled = !running,
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                    modifier = Modifier.fillMaxWidth().testTag("pair-password"),
                )
                hint?.let { Text(stringResource(it), color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("pair-hint")) }
                if (running) CircularProgressIndicator()
                Button(
                    enabled = !running && password.isNotEmpty(),
                    modifier = Modifier.testTag("pair-finish"),
                    onClick = {
                        if (password.length < 10) {
                            password = ""
                            hint = R.string.password_too_short
                            return@Button
                        }
                        val typed = password
                        val paired = current.paired
                        val ticket = gen.get()
                        password = ""
                        hint = null
                        running = true
                        val ownerName = current.owner
                        val repoName = current.repo
                        val branchName = current.branch
                        val token = paired.syncToken()
                        val secret = paired.secretKey()
                        paired.close()
                        scope.launch {
                            var pulled = false
                            try {
                                withContext(Dispatchers.IO) {
                                    val selection = RepositorySelection(ownerName, repoName, branchName)
                                    app.firstPull(selection, token, typed, secret, epoch)
                                    pulled = true
                                    app.syncSettings.save(SyncTarget(ownerName, repoName, branchName), token)
                                }
                            } catch (e: CancellationException) {
                                throw e
                            } catch (e: Exception) {
                                if (isActive && gen.get() == ticket) {
                                    running = false
                                    step = PairStep.Failed(if (pulled) R.string.pair_failed else R.string.pair_pull_failed)
                                }
                            }
                        }
                    },
                ) { Text(stringResource(R.string.pair_finish)) }
            }
            is PairStep.Failed -> {
                Text(stringResource(current.message), color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("pair-error"))
                Button(onClick = ::restart, modifier = Modifier.testTag("pair-restart")) { Text(stringResource(R.string.pair_restart)) }
            }
        }
        TextButton(
            onClick = {
                if (running) app.cancelFirstPull()
                leave()
            },
            modifier = Modifier.testTag("pair-cancel"),
        ) {
            Text(stringResource(R.string.cancel_edit))
        }
    }
}

/** CameraX preview. Each frame is decoded on the device and dropped. */
@Composable
private fun QrScanner(onText: (String) -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    @Suppress("DEPRECATION") val lifecycleOwner = androidx.compose.ui.platform.LocalLifecycleOwner.current
    val currentOnText by rememberUpdatedState(onText)
    LaunchedEffect(Unit) {
        PairingScanInjector.frames.collect { text ->
            if (PairingQr.isDesktopQr(text)) currentOnText(text)
        }
    }
    if (!PairingScanInjector.cameraEnabled) return
    var granted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    var failed by remember { mutableStateOf(false) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted = it }
    LaunchedEffect(Unit) { if (!granted) launcher.launch(Manifest.permission.CAMERA) }
    if (!granted || failed) {
        Text(stringResource(R.string.pair_camera_unavailable), modifier = Modifier.testTag("pair-camera-unavailable"))
        return
    }
    val previewView = remember { PreviewView(context) }
    DisposableEffect(lifecycleOwner) {
        val executor = Executors.newSingleThreadExecutor()
        val main = Handler(Looper.getMainLooper())
        val future = ProcessCameraProvider.getInstance(context)
        var provider: ProcessCameraProvider? = null
        var live = true
        var last: String? = null
        future.addListener({
            if (!live) return@addListener
            runCatching {
                val bound = future.get()
                provider = bound
                val preview = Preview.Builder().build().also { it.setSurfaceProvider(previewView.surfaceProvider) }
                val analysis = ImageAnalysis.Builder().setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST).build()
                analysis.setAnalyzer(executor) { image ->
                    try {
                        val text = runCatching {
                            val plane = image.planes[0]
                            val buffer = plane.buffer
                            val bytes = ByteArray(buffer.remaining())
                            buffer.get(bytes)
                            PairingQr.decodeLuma(bytes, plane.rowStride, image.width, image.height)
                        }.getOrNull()
                        if (text != null && text != last && PairingQr.isDesktopQr(text)) {
                            last = text
                            main.post { if (live) currentOnText(text) }
                        }
                    } finally {
                        image.close()
                    }
                }
                bound.unbindAll()
                bound.bindToLifecycle(lifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analysis)
            }.onFailure { failed = true }
        }, ContextCompat.getMainExecutor(context))
        onDispose {
            live = false
            runCatching { provider?.unbindAll() }
            executor.shutdown()
        }
    }
    AndroidView(factory = { previewView }, modifier = modifier)
}
