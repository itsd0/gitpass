package app.gitpass.android.passkey

import app.gitpass.core.PasskeyCaller
import app.gitpass.core.PasskeyRequestInfo
import java.io.ByteArrayInputStream
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** #477: who the caller is and which Digital Asset Links file is fetched. */
class CallerIdentityTest {
    private val cert = byteArrayOf(1, 2, 3)
    private val own = "app.gitpass.android"

    @Test fun allowlistedBrowserUsesTheOriginItVouchesFor() {
        val id = CallerIdentity.resolve("com.android.chrome", own, originPopulated = true, privilegedOrigin = "https://example.com/", signers = listOf(cert))
        assertEquals(CallerIdentity.Browser("com.android.chrome", "https://example.com"), id)
        assertEquals("web:https://example.com", id!!.key)
    }

    @Test fun anAppThatClaimsAnOriginButIsNotAllowlistedIsRefused() {
        assertNull(CallerIdentity.resolve("com.evil.browser", own, originPopulated = true, privilegedOrigin = null, signers = listOf(cert)))
        assertNull(CallerIdentity.resolve("com.android.chrome", own, originPopulated = true, privilegedOrigin = " ", signers = listOf(cert)))
    }

    @Test fun appsAreIdentifiedByPackageAndSingleSigningCertificate() {
        val id = CallerIdentity.resolve("com.example.app", own, originPopulated = false, privilegedOrigin = null, signers = listOf(cert)) as CallerIdentity.App
        assertArrayEquals(CallerIdentity.sha256(cert), id.certSha256)
        assertEquals(32, id.certSha256.size)
        val other = CallerIdentity.resolve("com.example.app", own, false, null, listOf(byteArrayOf(9))) as CallerIdentity.App
        assertNotEquals(id.key, other.key)
        assertNull(CallerIdentity.resolve("com.example.app", own, false, null, emptyList()))
        assertNull(CallerIdentity.resolve("com.example.app", own, false, null, listOf(cert, byteArrayOf(9))), "several signers")
    }

    @Test fun gitpassItselfIsNeverACaller() {
        assertNull(CallerIdentity.resolve(own, own, false, null, listOf(cert)))
        assertNull(CallerIdentity.resolve("", own, false, null, listOf(cert)))
    }

    private fun assertNull(value: Any?, message: String) = org.junit.Assert.assertNull(message, value)
}

class HttpsAssetLinksTest {
    private class FakeConnection(url: URL, private val code: Int, private val body: ByteArray) : HttpURLConnection(url) {
        override fun connect() = Unit
        override fun disconnect() = Unit
        override fun usingProxy() = false
        override fun getResponseCode() = code
        override fun getInputStream(): InputStream = ByteArrayInputStream(body)
    }

    private val opened = mutableListOf<FakeConnection>()
    private var now = 0L

    private fun source(code: Int, body: String) = HttpsAssetLinks(
        connect = { url -> FakeConnection(url, code, body.toByteArray()).also { opened += it } },
        nowMs = { now },
    )

    @Test fun fetchesTheWellKnownFileOverHttpsWithoutRedirects() {
        val s = source(200, "[]")
        assertEquals("[]", s.fetch("example.com"))
        val c = opened.single()
        assertEquals("https://example.com/.well-known/assetlinks.json", c.url.toString())
        assertFalse(c.instanceFollowRedirects)
        assertEquals(HttpsAssetLinks.TIMEOUT_MS, c.connectTimeout)
        assertEquals(HttpsAssetLinks.TIMEOUT_MS, c.readTimeout)
    }

    @Test fun redirectsErrorsAndOversizedFilesCountAsNoFile() {
        assertEquals("", source(301, "[]").fetch("example.com"))
        assertEquals("", source(404, "[]").fetch("example.com"))
        assertEquals("", source(200, "x".repeat(HttpsAssetLinks.MAX_BYTES + 1)).fetch("example.com"))
    }

    @Test fun cachesSuccessLongerThanFailure() {
        val ok = source(200, "[]")
        ok.fetch("a.example"); ok.fetch("a.example")
        assertEquals(1, opened.size)
        now += HttpsAssetLinks.SUCCESS_TTL_MS
        ok.fetch("a.example")
        assertEquals(2, opened.size)
        opened.clear(); now = 0
        val bad = source(500, "")
        bad.fetch("b.example"); bad.fetch("b.example")
        assertEquals(1, opened.size)
        now += HttpsAssetLinks.FAILURE_TTL_MS
        bad.fetch("b.example")
        assertEquals(2, opened.size)
    }
}

class PasskeyBrokerTest {
    private class Rules(private val rpId: String?) : PasskeyRules {
        val hosts = mutableListOf<String>()
        var verified: PasskeyCaller? = null
        override fun requestInfo(requestJson: String, create: Boolean) = PasskeyRequestInfo(rpId, null, null)
        override fun appRpHost(rpId: String): String {
            require(rpId != "co.uk") { "public suffix" }
            return rpId.lowercase().also { hosts += it }
        }
        override fun verifyCaller(caller: PasskeyCaller, rpId: String?): String {
            verified = caller
            if (caller is PasskeyCaller.App && caller.`assetLinksJson`.isEmpty()) throw IllegalStateException("not listed")
            return rpId ?: "derived.example"
        }
    }

    private val fetched = mutableListOf<String>()
    private val links = AssetLinksSource { host -> fetched += host; if (host == "example.com") "[dal]" else "" }
    private val app = CallerIdentity.App("com.example.app", ByteArray(32) { 7 })

    @Test fun browsersAreCheckedByOriginWithoutFetchingAnything() {
        val rules = Rules(null)
        val checked = PasskeyBroker(links, rules).check(CallerIdentity.Browser("com.android.chrome", "https://example.com"), "{}", create = false)
        assertEquals(PasskeyCaller.Browser("https://example.com"), checked.caller)
        assertEquals("derived.example", checked.rpId)
        assertTrue(fetched.isEmpty())
    }

    @Test fun appsFetchTheValidatedRpHostsAssetLinks() {
        val rules = Rules("Example.com")
        val checked = PasskeyBroker(links, rules).check(app, "{}", create = true)
        assertEquals(listOf("example.com"), fetched)
        assertEquals("[dal]", (checked.caller as PasskeyCaller.App).`assetLinksJson`)
        assertEquals("com.example.app", (checked.caller as PasskeyCaller.App).`package`)
    }

    @Test fun appsWithoutRpIdOrWithAnUnusableRpAreRefusedBeforeAnyFetch() {
        assertThrows(PasskeyRefusedException::class.java) { PasskeyBroker(links, Rules(null)).check(app, "{}", create = false) }
        assertThrows(IllegalArgumentException::class.java) { PasskeyBroker(links, Rules("co.uk")).check(app, "{}", create = false) }
        assertTrue(fetched.isEmpty())
        assertThrows(IllegalStateException::class.java) { PasskeyBroker(links, Rules("other.example")).check(app, "{}", create = false) }
    }
}
