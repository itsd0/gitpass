package app.gitpass.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import app.gitpass.core.*

@RunWith(AndroidJUnit4::class)
class NativeBridgeSmokeTest {

    @Test
    fun testGeneratePasswordNativeCall() {
        // Native smoke call across UniFFI bridge to Rust gitpass-core crypto
        val password = generatePassword(24UL)
        assertNotNull("Generated password must not be null", password)
        assertEquals("Generated password length must match requested length", 24, password.length)
        assertTrue(
            "Password must contain non-whitespace characters",
            password.isNotEmpty() && !password.contains(" ")
        )
    }

    @Test
    fun testTotpCodeNativeCall() {
        // Test TOTP code derivation with fake RFC 6238 test secret
        val fakeSecret = "JBSWY3DPEHPK3PXP"
        val totpResult = totpCode(fakeSecret)
        assertNotNull("TOTP result must not be null", totpResult)
        assertEquals("TOTP code must be 6 digits", 6, totpResult.code.length)
        assertTrue(
            "TOTP code must consist of digits only",
            totpResult.code.all { it.isDigit() }
        )
        val remaining = totpResult.secondsRemaining.toLong()
        assertTrue(
            "Seconds remaining must be between 1 and 30, got $remaining",
            remaining in 1L..30L
        )
    }

    @Test
    fun testBridgeHandshakeMatchesApp() {
        val ffi = ffiVersion()
        val format = formatVersion()
        assertEquals(app.gitpass.android.session.BridgeCompat.EXPECTED_API_LEVEL, ffi.apiLevel)
        assertTrue("crate version must be semver", Regex("""^\d+\.\d+\.\d+.*$""").matches(ffi.crateVersion))
        assertEquals(app.gitpass.android.session.BridgeCompat.EXPECTED_FORMAT, format.currentWrite)
        assertTrue(format.minRead <= format.currentWrite)
        assertTrue(app.gitpass.android.session.BridgeCompat.checkNative())
    }

    @Test
    fun testPairingVectorRoundTrip() {
        val opened = pairingTestVectorOpen()
        assertEquals("fake-owner", opened.repoOwner())
        assertEquals("fake-vault", opened.repoName())
        assertEquals("main", opened.branch())
        assertEquals("00000000-0000-4000-8000-000000000001", opened.accountId())
        assertEquals("fine_grained_pat", opened.tokenKind())
        assertEquals("github_pat_FAKE_pairing_test_vector", opened.syncToken())
        assertTrue(opened.secretKey().startsWith("A3-"))
        assertFalse(opened.syncToken().lowercase().contains("password"))
        // Never log the object: toString must not include the PAT or Secret Key.
        val rendered = opened.toString()
        assertFalse(rendered.contains("github_pat_"))
        assertFalse(rendered.contains(opened.secretKey()))
        val vector = pairingTestVector()
        assertTrue(vector.contains("gitpass-pair:2:qr:"))
        assertTrue(vector.contains("fine_grained_pat"))
        assertFalse(vector.contains("FAKE-master-password"))
        assertFalse(vector.contains("password"))
    }

    @Test
    fun testDefaultUnlockOptionsAreSideEffectFree() {
        val options = defaultUnlockOptions()
        assertFalse(options.registerDevice)
        assertFalse(options.purgeExpiredTrash)
        assertFalse(options.upgradeKdf)
        assertNull(options.deviceName)
    }
}
