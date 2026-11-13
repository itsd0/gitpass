package app.gitpass.android.ui

import android.Manifest
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Color
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
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
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
import app.gitpass.android.pairing.PairingQr
import app.gitpass.core.PairedSync
import app.gitpass.core.pairingJoinerBegin
import app.gitpass.core.pairingJoinerCancel
import app.gitpass.core.pairingJoinerCode
import app.gitpass.core.pairingJoinerOpen
import java.util.concurrent.Executors
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Test-only stand-in for the camera: instrumented tests emit scanned QR text here (#458). */
@VisibleForTesting
object PairingScanInjector {
    val frames = MutableSharedFlow<String>(extraBufferCapacity = 4)
    /** The last offer this phone displayed (public key + session id only; no secrets). */
    @Volatile var lastOffer: String? = null
}

private sealed interface PairStep {
    data class Offer(val offer: String) : PairStep
    data object ScanAnnounce : PairStep
    data class Code(val code: String) : PairStep
    data class ScanGrant(val code: String) : PairStep
    class Password(val paired: PairedSync) : PairStep
    data class Failed(val message: Int) : PairStep
}

/** Map fixed bridge error strings (gitpass-core pairing) to UI text. */
private fun pairingError(message: String?): Int {
    val m = message.orEmpty()
    return when {
        "expired" in m -> R.string.pair_expired
        "already used" in m -> R.string.pair_used
        "too many wrong" in m -> R.string.pair_too_many
        "does not match" in m || "failed authentication" in m -> R.string.pair_mismatch
        else -> R.string.pair_failed
    }
}

/**
 * Set up with QR (#458), joiner role of the #457 pairing protocol. All crypto runs in gitpass-core.
 *
 * 1. Show this phone's `offer` QR (public key + session id) for the desktop camera.
 * 2. Scan the desktop's `announce` QR and show the 6-digit code; the person types it on the desktop.
 * 3. Scan the desktop's `grant` QR; the core decrypts repo, branch, Secret Key and sync token.
 * 4. The person types the Master Password (never part of pairing) and the normal first pull runs.
 *
 * The activity is FLAG_SECURE. Nothing here is saveable: leaving the screen cancels the session and
 * drops the decrypted payload. Camera frames are decoded on the device and dropped.
 */
@Composable
fun PairingSetupScreen(app: GitpassApplication, epoch: Long, padding: PaddingValues, onCancel: () -> Unit) {
    var step by remember { mutableStateOf<PairStep?>(null) }
    var expiresAt by remember { mutableStateOf<Long?>(null) }
    var remaining by remember { mutableStateOf<Long?>(null) }
    var password by remember { mutableStateOf("") }
    var running by remember { mutableStateOf(false) }
    var hint by remember { mutableStateOf<Int?>(null) }
    val scope = rememberCoroutineScope()

    fun drop() {
        (step as? PairStep.Password)?.paired?.close()
        password = ""
        runCatching { pairingJoinerCancel() }
    }

    fun begin() {
        drop()
        hint = null
        step = null
        scope.launch {
            val result = withContext(Dispatchers.IO) { runCatching { pairingJoinerBegin() } }
            result.onSuccess { offer -> PairingScanInjector.lastOffer = offer; expiresAt = PairingQr.offerExpiry(offer); step = PairStep.Offer(offer) }
                .onFailure { step = PairStep.Failed(R.string.pair_failed) }
        }
    }

    LaunchedEffect(Unit) { begin() }
    DisposableEffect(Unit) { onDispose { drop() } }

    // Countdown to the offer's absolute expiry. The whole session ends then.
    LaunchedEffect(expiresAt) {
        val exp = expiresAt ?: return@LaunchedEffect
        while (true) {
            val left = exp - System.currentTimeMillis() / 1000
            remaining = left.coerceAtLeast(0)
            val current = step
            if (left <= 0 && current !is PairStep.Password && current !is PairStep.Failed) {
                drop(); step = PairStep.Failed(R.string.pair_expired); break
            }
            delay(1000)
        }
    }

    fun onScanned(text: String) {
        val current = step
        when (PairingQr.frameOf(text)) {
            PairingQr.Frame.ANNOUNCE -> if (current is PairStep.ScanAnnounce) {
                scope.launch {
                    val result = withContext(Dispatchers.IO) { runCatching { pairingJoinerCode(text) } }
                    result.onSuccess { step = PairStep.Code(it); hint = null }
                        .onFailure { hint = pairingError(it.message) }
                }
            } else hint = R.string.pair_wrong_qr
            PairingQr.Frame.GRANT -> if (current is PairStep.ScanGrant) {
                scope.launch {
                    val dir = app.pairingStateDir.absolutePath
                    val result = withContext(Dispatchers.IO) { runCatching { pairingJoinerOpen(dir, text, current.code) } }
                    result.onSuccess { step = PairStep.Password(it); hint = null }
                        .onFailure { drop(); step = PairStep.Failed(pairingError(it.message)) }
                }
            } else hint = R.string.pair_wrong_qr
            else -> hint = R.string.pair_wrong_qr
        }
    }

    Column(
        Modifier.fillMaxSize().padding(padding).padding(16.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(stringResource(R.string.pair_title), style = MaterialTheme.typography.headlineSmall)
        when (val s = step) {
            null -> CircularProgressIndicator()
            is PairStep.Offer -> {
                Text(stringResource(R.string.pair_show_offer))
                QrImage(s.offer, Modifier.testTag("pair-offer-qr"))
                remaining?.let { Text(stringResource(R.string.pair_expires_in, it), modifier = Modifier.testTag("pair-remaining")) }
                Button(onClick = { hint = null; step = PairStep.ScanAnnounce }, modifier = Modifier.testTag("pair-next")) {
                    Text(stringResource(R.string.pair_next_scan))
                }
            }
            is PairStep.ScanAnnounce, is PairStep.ScanGrant -> {
                Text(stringResource(if (s is PairStep.ScanAnnounce) R.string.pair_scan_announce else R.string.pair_scan_grant))
                QrScanner(onText = ::onScanned, modifier = Modifier.fillMaxWidth().height(320.dp))
                remaining?.let { Text(stringResource(R.string.pair_expires_in, it)) }
            }
            is PairStep.Code -> {
                Text(stringResource(R.string.pair_type_code))
                Text(
                    s.code.chunked(3).joinToString(" "),
                    fontFamily = FontFamily.Monospace, fontSize = 40.sp,
                    modifier = Modifier.align(Alignment.CenterHorizontally).testTag("pair-code"),
                )
                remaining?.let { Text(stringResource(R.string.pair_expires_in, it)) }
                Button(onClick = { hint = null; step = PairStep.ScanGrant(s.code) }, modifier = Modifier.testTag("pair-next")) {
                    Text(stringResource(R.string.pair_next_grant))
                }
            }
            is PairStep.Password -> {
                Text(stringResource(R.string.pair_received, "${s.paired.repoOwner()}/${s.paired.repoName()}", s.paired.branch()),
                    modifier = Modifier.testTag("pair-repo"))
                Text(stringResource(R.string.pair_password_hint))
                OutlinedTextField(
                    password, { password = it }, label = { Text(stringResource(R.string.master_password)) },
                    enabled = !running, singleLine = true, visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                    modifier = Modifier.fillMaxWidth().testTag("pair-password"),
                )
                if (running) CircularProgressIndicator()
                Button(
                    enabled = !running && password.isNotEmpty(), modifier = Modifier.testTag("pair-finish"),
                    onClick = {
                        val paired = s.paired
                        val typed = password
                        password = ""
                        running = true; hint = null
                        scope.launch {
                            val result = withContext(Dispatchers.IO) {
                                runCatching {
                                    val selection = RepositorySelection(paired.repoOwner(), paired.repoName(), paired.branch())
                                    app.firstPullPaired(selection, paired.syncToken(), typed, paired.secretKey(), epoch)
                                }
                            }
                            running = false
                            result.onFailure { hint = R.string.pair_pull_failed }
                            // Success installs the new account and re-renders the locked UI.
                        }
                    },
                ) { Text(stringResource(R.string.pair_finish)) }
            }
            is PairStep.Failed -> {
                Text(stringResource(s.message), modifier = Modifier.testTag("pair-error"))
                Button(onClick = ::begin, modifier = Modifier.testTag("pair-restart")) { Text(stringResource(R.string.pair_restart)) }
            }
        }
        hint?.let { Text(stringResource(it), color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("pair-hint")) }
        TextButton(onClick = { drop(); onCancel() }, enabled = !running, modifier = Modifier.testTag("pair-cancel")) {
            Text(stringResource(R.string.cancel_edit))
        }
    }
}

@Composable
private fun QrImage(text: String, modifier: Modifier = Modifier) {
    val bitmap = remember(text) {
        val size = 720
        val m = PairingQr.encode(text, size)
        val pixels = IntArray(size * size) { if (m[it % size, it / size]) Color.BLACK else Color.WHITE }
        Bitmap.createBitmap(pixels, size, size, Bitmap.Config.ARGB_8888)
    }
    Image(bitmap.asImageBitmap(), contentDescription = stringResource(R.string.pair_qr_description), modifier = modifier.fillMaxWidth().aspectRatio(1f))
}

/** CameraX preview + analysis; each frame's luminance plane is decoded with ZXing and dropped. */
@Composable
private fun QrScanner(onText: (String) -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    @Suppress("DEPRECATION") val lifecycleOwner = LocalLifecycleOwner.current
    val currentOnText by rememberUpdatedState(onText)
    var granted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    var failed by remember { mutableStateOf(false) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted = it }
    LaunchedEffect(Unit) { if (!granted) launcher.launch(Manifest.permission.CAMERA) }
    LaunchedEffect(Unit) { PairingScanInjector.frames.collect { currentOnText(it) } }
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
                val p = future.get()
                provider = p
                val preview = Preview.Builder().build().also { it.setSurfaceProvider(previewView.surfaceProvider) }
                val analysis = ImageAnalysis.Builder().setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST).build()
                analysis.setAnalyzer(executor) { image ->
                    val text = runCatching {
                        val plane = image.planes[0]
                        val buffer = plane.buffer
                        val bytes = ByteArray(buffer.remaining()).also { buffer.get(it) }
                        PairingQr.decodeLuma(bytes, plane.rowStride, image.width, image.height)
                    }.getOrNull()
                    image.close()
                    if (text != null && text != last && PairingQr.frameOf(text) != null) {
                        last = text
                        main.post { if (live) currentOnText(text) }
                    }
                }
                p.unbindAll()
                p.bindToLifecycle(lifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analysis)
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
