package app.gitpass.android.sync

import app.gitpass.android.session.SyncTarget
import java.io.File
import java.nio.file.Files
import org.junit.Assert.*
import org.junit.Test

/** Fake cipher (reversible, NOT secure) stands in for Android Keystore on the JVM. */
class FakeCipher : TokenCipher {
    var broken = false
    override fun encrypt(plain: ByteArray) = byteArrayOf(9) + plain.map { (it.toInt() xor 0x5A).toByte() }
    override fun decrypt(sealed: ByteArray): ByteArray {
        check(!broken && sealed[0] == 9.toByte()) { "key invalidated" }
        return sealed.drop(1).map { (it.toInt() xor 0x5A).toByte() }.toByteArray()
    }
}

class SyncSettingsStoreTest {
    private val token = "github_pat_FAKE_TOKEN_0123456789"
    private val target = SyncTarget("FAKE-owner", "fake-vault", "main")

    private fun store(cipher: TokenCipher = FakeCipher()): Pair<SyncSettingsStore, File> {
        val dir = Files.createTempDirectory("sync-settings").toFile()
        val file = File(dir, "vault.sync.json")
        return SyncSettingsStore(file, cipher) to file
    }

    @Test fun roundTripSealsTokenAndKeepsItOutOfTheFileAndToString() {
        val (s, file) = store()
        assertNull(s.load())
        assertFalse(s.hasToken())
        s.save(target, token)
        val loaded = s.load()!!
        assertEquals(target, loaded.target)
        assertEquals(token, loaded.token)
        assertTrue(s.hasToken())
        assertFalse(file.readText().contains("FAKE_TOKEN"))
        assertFalse(loaded.toString().contains("FAKE_TOKEN"))
        assertFalse(target.toString().contains("fake-vault"))
    }

    @Test fun keepTokenOnEditLastSuccessResetOnNewTargetAndClear() {
        val (s, file) = store()
        assertThrows(IllegalArgumentException::class.java) { s.save(target, null) }
        s.save(target, token)
        s.recordSuccess(1234L)
        assertEquals(1234L, s.lastSuccessMs())
        s.save(target, null)
        assertEquals(token, s.load()!!.token)
        assertEquals(1234L, s.lastSuccessMs())
        val other = SyncTarget("FAKE-owner", "other-vault", "main")
        s.save(other, null)
        assertEquals(other, s.target())
        assertNull(s.lastSuccessMs())
        s.clear()
        assertFalse(file.exists())
        assertNull(s.target())
    }

    @Test fun rejectsClassicTokensAndSurvivesCorruptOrUnreadableState() {
        val cipher = FakeCipher()
        val (s, file) = store(cipher)
        assertThrows(IllegalArgumentException::class.java) { s.save(target, "ghp_FAKE_classic_token_000") }
        assertFalse(file.exists())
        s.save(target, token)
        cipher.broken = true
        assertNull("an unreadable sealed token disables sync instead of crashing", s.load())
        assertEquals(target, s.target())
        file.writeText("{not json")
        assertNull(s.target())
        assertNull(s.load())
    }
}
