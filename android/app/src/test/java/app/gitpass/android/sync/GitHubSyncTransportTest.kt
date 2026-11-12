package app.gitpass.android.sync

import app.gitpass.core.HttpHeader
import app.gitpass.core.HttpTransportException
import app.gitpass.core.HttpTransportRequest
import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.net.SocketTimeoutException
import java.net.URL
import java.security.Principal
import java.security.cert.Certificate
import javax.net.ssl.HttpsURLConnection

/** Fake connections only: no network, fake token. */
class GitHubSyncTransportTest {
    private val token = "github_pat_FAKE_TOKEN_0123456789"

    private open class Fake(
        url: URL,
        val status: Int = 200,
        val body: ByteArray = "{}".toByteArray(),
        val headers: Map<String, String> = emptyMap(),
        val failWith: Exception? = null,
    ) : HttpsURLConnection(url) {
        var disconnected = false
        var method = "GET"
        val sent = ByteArrayOutputStream()
        override fun setRequestMethod(method: String) { this.method = method } // the JDK rejects PATCH; Android does not
        override fun getRequestMethod() = method
        override fun getResponseCode(): Int { failWith?.let { throw it }; return status }
        override fun getContentLengthLong() = body.size.toLong()
        override fun getInputStream(): InputStream = ByteArrayInputStream(body)
        override fun getErrorStream(): InputStream = ByteArrayInputStream(body)
        override fun getOutputStream(): OutputStream = sent
        override fun getHeaderField(name: String): String? = headers[name.lowercase()]
        override fun disconnect() { disconnected = true }
        override fun connect() {}
        override fun usingProxy() = false
        override fun getCipherSuite() = "FAKE"
        override fun getLocalCertificates(): Array<Certificate>? = null
        override fun getServerCertificates(): Array<Certificate> = emptyArray()
        override fun getPeerPrincipal(): Principal = Principal { "FAKE" }
        override fun getLocalPrincipal(): Principal? = null
    }

    private fun request(method: String = "GET", path: String = "/repos/o/vault/git/ref/heads/main", body: ByteArray? = null, max: ULong = 1024u) =
        HttpTransportRequest(method, path, listOf(HttpHeader("Accept", "application/vnd.github+json"), HttpHeader("Authorization", "Bearer EVIL")), body, max, 15_000u)

    @Test fun fixedOriginBearerHeaderNoRedirectsNoCacheAndPatchBody() {
        val seen = mutableListOf<Fake>()
        val transport = GitHubSyncTransport.forTest(token) { url ->
            assertEquals("https", url.protocol)
            assertEquals("api.github.com", url.host)
            Fake(url, headers = mapOf("x-ratelimit-remaining" to "42", "retry-after" to "7", "x-ratelimit-reset" to "1700000000")).also { seen += it }
        }
        val response = transport.send(request("PATCH", "/repos/o/vault/git/refs/heads/feature%2Fx", """{"sha":"a","force":false}""".toByteArray()))
        assertEquals(200.toUShort(), response.status)
        assertEquals(42uL, response.rateLimitRemaining)
        assertEquals(7uL, response.retryAfter)
        assertEquals(1_700_000_000uL, response.rateLimitReset)
        val c = seen.single()
        assertEquals("PATCH", c.method)
        assertEquals("/repos/o/vault/git/refs/heads/feature%2Fx", c.url.path)
        assertFalse(c.instanceFollowRedirects)
        assertFalse(c.useCaches)
        assertEquals("Bearer $token", c.getRequestProperty("Authorization"))
        assertEquals("application/vnd.github+json", c.getRequestProperty("Accept"))
        assertEquals("""{"sha":"a","force":false}""", c.sent.toString(Charsets.UTF_8))
        assertTrue(c.disconnected)
        assertFalse(transport.toString().contains("FAKE_TOKEN"))
    }

    @Test fun redirectsAreReturnedNotFollowedAndErrorBodiesAreBounded() {
        for (status in listOf(301, 302, 307, 308)) {
            var calls = 0
            val t = GitHubSyncTransport.forTest(token) { url -> calls++; Fake(url, status = status, body = "BODY".toByteArray()) }
            val r = t.send(request())
            assertEquals(status.toUShort(), r.status)
            assertEquals(0, r.body.size)
            assertEquals(1, calls)
        }
        val t = GitHubSyncTransport.forTest(token) { url -> Fake(url, status = 422, body = """{"message":"x"}""".toByteArray()) }
        assertEquals(422.toUShort(), t.send(request()).status)
        val big = GitHubSyncTransport.forTest(token) { url -> Fake(url, body = ByteArray(4096)) }
        assertThrows(HttpTransportException.TooLarge::class.java) { big.send(request(max = 1024u)) }
    }

    @Test fun pathMethodAndHeaderValidation() {
        var calls = 0
        val t = GitHubSyncTransport.forTest(token) { url -> calls++; Fake(url) }
        for (bad in listOf("/user", "/repos/o/../x", "https://evil.example/repos/x", "/repos//x", "/repos/o/x@evil", "/repos/o/x y", "/repos/o/x#f")) {
            assertThrows(HttpTransportException.Transport::class.java) { t.send(request(path = bad)) }
        }
        assertThrows(HttpTransportException.Transport::class.java) { t.send(request(method = "DELETE")) }
        assertEquals(0, calls)
        assertThrows(IllegalArgumentException::class.java) { GitHubSyncTransport.forTest("ghp_classic_FAKE_0123456789") { Fake(it) } }
        assertThrows(IllegalArgumentException::class.java) { GitHubSyncTransport.forTest("github_pat_FAKE with space") { Fake(it) } }
    }

    @Test fun failuresMapToFixedErrorsAndCancellationStopsWork() {
        val timeout = GitHubSyncTransport.forTest(token) { url -> Fake(url, failWith = SocketTimeoutException("https://api.github.com/x?token=$token")) }
        assertThrows(HttpTransportException.Timeout::class.java) { timeout.send(request()) }
        val io = GitHubSyncTransport.forTest(token) { url -> Fake(url, failWith = java.io.IOException("leaky $token")) }
        val e = assertThrows(HttpTransportException.Transport::class.java) { io.send(request()) }
        assertFalse((e.message.orEmpty() + e.toString()).contains("FAKE_TOKEN"))

        var cancelled = false
        var calls = 0
        val c = GitHubSyncTransport.forTest(token, cancelled = { cancelled }) { url -> calls++; Fake(url) }
        cancelled = true
        assertThrows(HttpTransportException.Cancelled::class.java) { c.send(request()) }
        assertEquals(0, calls)

        val closed = GitHubSyncTransport.forTest(token) { url -> Fake(url) }
        closed.close()
        assertThrows(HttpTransportException.Cancelled::class.java) { closed.send(request()) }
    }
}
