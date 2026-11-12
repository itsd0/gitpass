package app.gitpass.android

import android.content.Context
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.android.biometric.BiometricGate
import app.gitpass.android.biometric.BiometricResult
import app.gitpass.android.biometric.QuickUnlockInvalidatedException
import app.gitpass.android.biometric.QuickUnlockKeys
import app.gitpass.android.session.*
import app.gitpass.android.ui.TestTags
import java.io.File
import java.security.SecureRandom
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Biometric unlock UI flow on the real Compose screens (#473): opt in on a
 * Master Password unlock → biometric unlock without the password → key
 * invalidated (enrolment change) → fallback to the Master Password with the
 * setup offered again → stale key after a password change → turn off.
 * The system prompt and the auth-bound Keystore key are faked (CI emulators
 * have no enrolled biometric); [BiometricKeystoreTest] covers the real ones.
 */
@RunWith(AndroidJUnit4::class)
class BiometricUnlockUiTest {
    @get:Rule val rule = createEmptyComposeRule()

    @Test fun enableUseInvalidateStaleAndTurnOff() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val app = context.applicationContext as GitpassApplication
        val root = File(context.cacheDir, "bio-ui-${UUID.randomUUID()}").apply { mkdirs() }
        File(root, "format.json").writeText("{}")
        val gateway = UiKeyGateway()
        val keys = UiSoftwareKeys()
        val gate = UiGate()
        val blob = File(root.parentFile, "${root.name}.quick-unlock")
        app.installForTest(gateway, root, nowMs = { 0L }, quickKeys = keys, gate = gate)
        try {
            ActivityScenario.launch(MainActivity::class.java).use {
                // Opt in on a Master Password unlock.
                rule.onNodeWithTag(TestTags.BIOMETRIC_UNLOCK).assertDoesNotExist()
                passwordUnlock(enroll = true)
                rule.waitUntil(10_000) { blob.isFile }
                assertEquals(1, gate.prompts)
                assertFalse("never stored in plaintext", blob.readBytes().toList().windowed(32).any { it.toByteArray().contentEquals(UiKeyGateway.KEY) })
                rule.onNodeWithTag(TestTags.BIOMETRIC_OFF).assertIsDisplayed()

                // Biometric unlock: no password typed, no password unlock.
                rule.onNodeWithTag(TestTags.LOCK).performClick()
                val passwordOpens = gateway.passwordOpens
                val keyOpens = gateway.keyOpens // setup already opened once with the derived key
                rule.onNodeWithTag(TestTags.BIOMETRIC_UNLOCK).performScrollTo().performClick()
                rule.waitUntil(10_000) { app.session.state.value.unlocked }
                assertEquals(0, passwordOpens)
                assertEquals(passwordOpens, gateway.passwordOpens)
                assertEquals(keyOpens + 1, gateway.keyOpens)
                assertEquals(2, gate.prompts)

                // Enrolment changed: Keystore key invalidated → Master Password + setup offered again.
                rule.onNodeWithTag(TestTags.LOCK).performClick()
                keys.invalidated = true
                rule.onNodeWithTag(TestTags.BIOMETRIC_UNLOCK).performScrollTo().performClick()
                rule.onNodeWithTag(TestTags.BIOMETRIC_NOTICE).assertIsDisplayed()
                rule.onNodeWithTag(TestTags.BIOMETRIC_UNLOCK).assertDoesNotExist()
                rule.onNodeWithTag(TestTags.BIOMETRIC_ENROLL).assertIsOn()
                assertFalse(blob.exists())
                assertFalse(app.session.state.value.unlocked)
                keys.invalidated = false
                passwordUnlock(enroll = false) // box is already ticked
                rule.waitUntil(10_000) { blob.isFile }

                // Password changed elsewhere: key no longer opens the vault → fallback.
                rule.onNodeWithTag(TestTags.LOCK).performClick()
                gateway.stale = true
                rule.onNodeWithTag(TestTags.BIOMETRIC_UNLOCK).performScrollTo().performClick()
                rule.waitUntil(10_000) { !blob.exists() }
                rule.onNodeWithTag(TestTags.BIOMETRIC_NOTICE).assertIsDisplayed()
                assertFalse(app.session.state.value.unlocked)
                gateway.stale = false

                // Turn off from the unlocked screen deletes blob and key.
                passwordUnlock(enroll = false)
                rule.waitUntil(10_000) { blob.isFile }
                rule.onNodeWithTag(TestTags.BIOMETRIC_OFF).performClick()
                assertFalse(blob.exists())
                assertTrue(keys.keys.isEmpty())
                rule.onNodeWithTag(TestTags.LOCK).performClick()
                rule.onNodeWithTag(TestTags.BIOMETRIC_UNLOCK).assertDoesNotExist()

                // Cancelled prompt during setup: nothing saved.
                gate.result = { BiometricResult.Cancelled }
                passwordUnlock(enroll = true)
                rule.waitUntil(10_000) { app.session.state.value.unlocked }
                assertFalse(blob.exists())
                assertTrue(keys.keys.isEmpty())
            }
        } finally {
            app.session.lock()
            app.quickUnlock.disable()
            app.pendingSecret.clear()
            root.deleteRecursively()
        }
    }

    private fun passwordUnlock(enroll: Boolean) {
        rule.onNodeWithTag(TestTags.MASTER_PASSWORD).performTextInput("FAKE-password")
        rule.onNodeWithTag(TestTags.SECRET_KEY).performTextInput("FAKE-key")
        if (enroll) rule.onNodeWithTag(TestTags.BIOMETRIC_ENROLL).performScrollTo().performClick()
        rule.onNodeWithTag(TestTags.UNLOCK).performScrollTo().performClick()
        rule.waitUntil(10_000) { (InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as GitpassApplication).session.state.value.unlocked }
    }
}

private class UiGate : BiometricGate {
    @Volatile var prompts = 0
    @Volatile var result: (Cipher) -> BiometricResult = { BiometricResult.Success(it) }
    override fun available(context: Context) = true
    override fun authenticate(context: Context, cipher: Cipher, title: String, negative: String, onResult: (BiometricResult) -> Unit) {
        prompts++
        onResult(result(cipher))
    }
}

private class UiSoftwareKeys : QuickUnlockKeys {
    val keys = java.util.concurrent.ConcurrentHashMap<String, SecretKey>()
    @Volatile var invalidated = false
    override fun newEncryptCipher(alias: String): Cipher {
        val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
        keys[alias] = key
        val iv = ByteArray(12).also { SecureRandom().nextBytes(it) }
        return Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key, GCMParameterSpec(128, iv)) }
    }
    override fun decryptCipher(alias: String, iv: ByteArray): Cipher {
        if (invalidated) throw QuickUnlockInvalidatedException()
        val key = keys[alias] ?: throw QuickUnlockInvalidatedException()
        return Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, iv)) }
    }
    override fun delete(alias: String) { keys.remove(alias) }
}

private class UiKeyGateway : VaultGateway, OpenVault {
    @Volatile var stale = false
    @Volatile var passwordOpens = 0
    @Volatile var keyOpens = 0
    override fun vaultExists(root: String) = true
    override fun createAccount(root: String, password: String) = "FAKE-key"
    override fun open(root: String, password: String, secretKey: String): OpenVault {
        require(password == "FAKE-password" && secretKey == "FAKE-key")
        passwordOpens++
        return this
    }
    override fun deriveUnlockKey(root: String, password: String, secretKey: String): ByteArray {
        require(password == "FAKE-password" && secretKey == "FAKE-key")
        return KEY.copyOf()
    }
    override fun openWithUnlockKey(root: String, key: ByteArray): OpenVault {
        try {
            if (stale || !key.contentEquals(KEY)) throw QuickUnlockStaleException()
            keyOpens++
            return this
        } finally {
            key.fill(0)
        }
    }
    override fun listVaults() = listOf(VaultSummary("v1", "Fake", null, 1))
    override fun listItems(vaultId: String) = listOf(ItemRow("seed", "v1", "Seed login", "owner", null, "login", emptyList(), false, false, false, false))
    override fun getItem(vaultId: String, itemId: String) = ItemView("seed", "v1", "Seed login", "owner", "FAKE-pw", null, null, null, "login", emptyList(), false, false, false, "", emptyList())
    override fun createVault(name: String) = "v1"
    override fun close() {}

    companion object {
        val KEY = ByteArray(32) { (it * 3 + 1).toByte() }
    }
}
