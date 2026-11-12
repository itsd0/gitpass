package app.gitpass.android.biometric

import android.content.Context
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import javax.crypto.Cipher

/** Result of one biometric prompt. Only [Success] carries the authenticated cipher. */
sealed interface BiometricResult {
    class Success(val cipher: Cipher) : BiometricResult
    /** The user chose the Master Password, cancelled, or the prompt was dismissed. */
    data object Cancelled : BiometricResult
    /** Too many attempts, hardware error, or biometrics no longer available. Carries no secret. */
    class Failed(val message: String) : BiometricResult
}

/** Shows the system biometric prompt bound to a Keystore cipher. Faked in UI tests. */
interface BiometricGate {
    /** True when a BIOMETRIC_STRONG biometric is enrolled and usable on API 30+. */
    fun available(context: Context): Boolean

    /** Calls [onResult] exactly once, on the main thread. */
    fun authenticate(context: Context, cipher: Cipher, title: String, negative: String, onResult: (BiometricResult) -> Unit)
}

/**
 * Platform `android.hardware.biometrics.BiometricPrompt` (no androidx
 * dependency; the Activity is a ComponentActivity). Strong biometrics only:
 * no device-credential fallback, so a PIN/pattern never releases the key.
 */
class PlatformBiometricGate : BiometricGate {
    override fun available(context: Context): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return false
        val manager = context.getSystemService(BiometricManager::class.java) ?: return false
        return manager.canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG) == BiometricManager.BIOMETRIC_SUCCESS
    }

    override fun authenticate(context: Context, cipher: Cipher, title: String, negative: String, onResult: (BiometricResult) -> Unit) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            onResult(BiometricResult.Failed("Biometric unlock is not available on this device."))
            return
        }
        val executor = context.mainExecutor
        var delivered = false
        fun deliver(result: BiometricResult) {
            if (!delivered) {
                delivered = true
                onResult(result)
            }
        }
        val prompt = BiometricPrompt.Builder(context)
            .setTitle(title)
            .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
            .setConfirmationRequired(false)
            .setNegativeButton(negative, executor) { _, _ -> deliver(BiometricResult.Cancelled) }
            .build()
        prompt.authenticate(
            BiometricPrompt.CryptoObject(cipher),
            CancellationSignal(),
            executor,
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    val authenticated = result.cryptoObject?.cipher
                    deliver(if (authenticated != null) BiometricResult.Success(authenticated) else BiometricResult.Failed("Biometric unlock failed."))
                }

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    val cancelled = errorCode == BiometricPrompt.BIOMETRIC_ERROR_USER_CANCELED ||
                        errorCode == BiometricPrompt.BIOMETRIC_ERROR_CANCELED
                    deliver(if (cancelled) BiometricResult.Cancelled else BiometricResult.Failed(errString.toString()))
                }
                // onAuthenticationFailed (one unrecognised finger) keeps the prompt open.
            },
        )
    }
}
