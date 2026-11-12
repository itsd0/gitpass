package app.gitpass.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.android.session.SyncTarget
import app.gitpass.android.sync.KeystoreTokenCipher
import app.gitpass.android.sync.SyncSettingsStore
import java.io.File
import java.net.URL
import java.util.UUID
import javax.net.ssl.HttpsURLConnection
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Platform pieces of #455 on a real Android runtime. No network: connections are never opened. */
@RunWith(AndroidJUnit4::class)
class SyncPlatformTest {
    @Test fun keystoreSealsWithRandomIvAndRejectsTampering() {
        val cipher = KeystoreTokenCipher("gitpass.sync.token.platformtest-${UUID.randomUUID()}")
        try {
            val plain = "github_pat_FAKE_KEYSTORE_0123456789".toByteArray()
            val a = cipher.encrypt(plain)
            val b = cipher.encrypt(plain)
            assertFalse(a.contentEquals(b))
            assertFalse(String(a, Charsets.ISO_8859_1).contains("FAKE_KEYSTORE"))
            assertArrayEquals(plain, cipher.decrypt(a))
            val tampered = a.copyOf().also { it[it.size - 1] = (it[it.size - 1].toInt() xor 1).toByte() }
            assertThrows(Exception::class.java) { cipher.decrypt(tampered) }
        } finally {
            cipher.deleteKey()
        }
    }

    @Test fun settingsStoreWithKeystoreKeepsTokenOutOfTheFile() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val cipher = KeystoreTokenCipher("gitpass.sync.token.platformtest-${UUID.randomUUID()}")
        val file = File(context.cacheDir, "sync-platform-${UUID.randomUUID()}.sync.json")
        try {
            val store = SyncSettingsStore(file, cipher)
            store.save(SyncTarget("FAKE-owner", "fake-vault", "main"), "github_pat_FAKE_STORE_0123456789")
            assertFalse(file.readText().contains("FAKE_STORE"))
            assertEquals("github_pat_FAKE_STORE_0123456789", store.load()!!.token)
            cipher.deleteKey()
            assertNull("a lost Keystore key disables sync instead of crashing", store.load())
        } finally {
            file.delete()
            cipher.deleteKey()
        }
    }

    @Test fun platformHttpsConnectionAcceptsPatchForRefUpdates() {
        // Core moves the branch with PATCH /git/refs/...; the JDK rejects PATCH, Android's stack must not.
        val connection = URL("https://api.github.com/repos/o/r/git/refs/heads/main").openConnection() as HttpsURLConnection
        try {
            connection.requestMethod = "PATCH"
            assertEquals("PATCH", connection.requestMethod)
        } finally {
            connection.disconnect()
        }
    }
}
