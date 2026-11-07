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
}
