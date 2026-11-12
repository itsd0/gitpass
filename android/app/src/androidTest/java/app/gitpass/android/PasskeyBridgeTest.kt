package app.gitpass.android

import android.content.res.AssetManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.core.GitpassException
import app.gitpass.core.PasskeyCaller
import app.gitpass.core.openVault
import app.gitpass.core.passkeyVerifyCaller
import java.io.File
import java.util.UUID
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The passkey bridge (#477) through the real native library on the device:
 * register for an app vouched for by Digital Asset Links, list, sign, and
 * refuse other callers and RPs. Uses a copy of the shared v5 test fixture.
 */
@RunWith(AndroidJUnit4::class)
class PasskeyBridgeTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val dir = File(instrumentation.targetContext.cacheDir, "passkey-bridge-${UUID.randomUUID()}")
    private val cert = ByteArray(32) { 0x5A }
    private val fp = cert.joinToString(":") { "%02X".format(it) }
    private fun dal(pkg: String) = """[{"relation":["delegate_permission/common.get_login_creds"],"target":{"namespace":"android_app","package_name":"$pkg","sha256_cert_fingerprints":["$fp"]}}]"""
    private fun app(pkg: String, links: String = dal("com.example.app")) = PasskeyCaller.App(pkg, cert, links)
    private val create = """{"rp":{"id":"example.com","name":"Example"},"user":{"id":"dXNlci0x","name":"alice"},"challenge":"Y3JlYXRl","pubKeyCredParams":[{"type":"public-key","alg":-7}]}"""
    private val get = """{"rpId":"example.com","challenge":"Z2V0"}"""

    @After fun tearDown() { dir.deleteRecursively() }

    @Test fun registerListSignAndRefuseOthers() {
        copyAssetDir(instrumentation.context.assets, "v5", dir)
        val vault = openVault(dir.absolutePath, SharedFixturesTest.PASSWORD, File(dir, "secret.key.txt").readText().trim())
        try {
            assertThrows(GitpassException::class.java) { vault.passkeyRegister(app("com.evil.app"), create, null) }
            val reg = JSONObject(vault.passkeyRegister(app("com.example.app"), create, null))
            val id = reg.getString("id")
            assertEquals("public-key", reg.getString("type"))
            assertEquals(-7, reg.getJSONObject("response").getInt("publicKeyAlgorithm"))

            val entries = vault.passkeyEntries(app("com.example.app"), get)
            assertEquals(listOf(id), entries.map { it.credentialId })
            assertEquals("alice", entries.single().userName)
            assertThrows(GitpassException::class.java) { vault.passkeyEntries(app("com.example.app", ""), get) }
            assertThrows(GitpassException::class.java) { vault.passkeyEntries(PasskeyCaller.Browser("https://example.com.evil.io"), get) }

            val signed = JSONObject(vault.passkeyAssert(app("com.example.app"), get, id, null))
            assertEquals(id, signed.getString("id"))
            assertTrue(signed.getJSONObject("response").getString("signature").isNotEmpty())
            val viaBrowser = JSONObject(vault.passkeyAssert(PasskeyCaller.Browser("https://www.example.com"), get, id, ByteArray(32) { 1 }))
            assertEquals(id, viaBrowser.getString("id"))
            assertThrows(GitpassException::class.java) {
                vault.passkeyAssert(PasskeyCaller.Browser("https://example.org"), get.replace("example.com", "example.org"), id, null)
            }
            assertEquals("example.com", passkeyVerifyCaller(PasskeyCaller.Browser("https://login.example.com"), "example.com"))
        } finally {
            vault.close()
        }
    }

    private fun copyAssetDir(assets: AssetManager, path: String, dest: File) {
        val children = assets.list(path)
        if (children.isNullOrEmpty()) {
            dest.parentFile?.mkdirs()
            assets.open(path).use { input -> dest.outputStream().use { input.copyTo(it) } }
        } else {
            dest.mkdirs()
            for (child in children) copyAssetDir(assets, "$path/$child", File(dest, child))
        }
    }
}
