package app.gitpass.android.firstpull

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayInputStream
import java.net.URL
import java.security.Principal
import java.security.cert.Certificate
import java.util.Base64
import javax.net.ssl.HttpsURLConnection

class GitHubFirstPullTransportTest {
    private val selection = RepositorySelection("FAKE-owner", "FAKE-repo", "feature/first-pull")
    private val commit = "a".repeat(40)
    private val tree = "b".repeat(40)
    private open class Fake(url: URL, val body: ByteArray, val status: Int = 200, val length: Long = body.size.toLong()) : HttpsURLConnection(url) {
        var disconnected = false
        override fun getResponseCode() = status
        override fun getContentLengthLong() = length
        override fun getInputStream(): java.io.InputStream = ByteArrayInputStream(body)
        override fun disconnect() { disconnected = true }
        override fun connect() {}
        override fun usingProxy() = false
        override fun getCipherSuite() = "FAKE"
        override fun getLocalCertificates(): Array<Certificate>? = null
        override fun getServerCertificates(): Array<Certificate> = emptyArray()
        override fun getPeerPrincipal(): Principal = Principal { "FAKE" }
        override fun getLocalPrincipal(): Principal? = null
    }
    private fun failure(action: () -> Unit): String {
        try { action(); fail("Expected rejection") } catch (e: FirstPullFailure) {
            assertFalse(e.toString().contains("FAKE-PAT"))
            return e.message.orEmpty()
        }
        error("unreachable")
    }

    @Test fun pinnedTreeBlobAndEncodedBranchUseOnlyFixedHttpsOrigin() {
        val bytes = "FAKE encrypted bytes".toByteArray()
        val sha = EncryptedFirstPull.gitBlobSha(bytes)
        val bodies = ArrayDeque(listOf(
            JSONObject().put("sha", commit).toString(),
            JSONObject().put("sha", commit).put("tree", JSONObject().put("sha", tree)).toString(),
            """{"sha":"$tree","truncated":false,"tree":[{"path":"keyset.enc","type":"blob","mode":"100644","sha":"$sha","size":${bytes.size}}]}""",
            JSONObject().put("sha", sha).put("size", bytes.size).put("encoding", "base64")
                .put("content", Base64.getEncoder().encodeToString(bytes) + "\n").toString(),
        ))
        val requests = mutableListOf<Fake>()
        val transport = GitHubFirstPullTransport.forTest("FAKE-PAT", connect = { url ->
            assertEquals("https", url.protocol)
            assertEquals("api.github.com", url.host)
            Fake(url, bodies.removeFirst().toByteArray()).also { requests += it }
        })
        assertEquals(commit, transport.resolve(selection))
        val result = transport.tree(selection, commit)
        assertEquals(commit, result.commit)
        assertEquals(sha, result.entries.single().sha)
        assertArrayEquals(bytes, transport.blob(selection, sha))
        assertTrue(requests.first().url.path.endsWith("feature%2Ffirst-pull"))
        assertTrue(requests[1].url.path.endsWith("/git/commits/$commit"))
        assertTrue(requests[2].url.path.endsWith("/git/trees/$tree"))
        requests.forEach {
            assertFalse(it.instanceFollowRedirects)
            assertFalse(it.useCaches)
            assertEquals(15_000, it.connectTimeout)
            assertEquals(15_000, it.readTimeout)
            assertEquals("GET", it.requestMethod)
            assertEquals("Bearer FAKE-PAT", it.getRequestProperty("Authorization"))
            assertTrue(it.disconnected)
        }
        assertFalse(transport.toString().contains("FAKE-PAT"))
        transport.close()
    }

    @Test fun redirectsAndCredentialErrorsNeverFollowOrExposeBody() {
        for (status in listOf(301, 302, 307, 308, 401, 403, 404, 500)) {
            var calls = 0
            val transport = GitHubFirstPullTransport.forTest("FAKE-PAT", connect = { url ->
                calls++
                Fake(url, "FAKE-PAT response must never be shown".toByteArray(), status)
            })
            failure { transport.resolve(selection) }
            assertEquals(1, calls)
            transport.close()
        }
    }

    @Test fun responseBoundsApplyWithAndWithoutContentLength() {
        for (declared in listOf(-1L, 100_000L)) {
            val transport = GitHubFirstPullTransport.forTest("FAKE-PAT", connect = {
                Fake(it, ByteArray(70_000) { 32 }, length = declared)
            })
            failure { transport.resolve(selection) }
            transport.close()
        }
    }

    @Test fun cancellationCloseAndDeadlinePreventRequests() {
        var calls = 0
        var time = 0L
        var cancelled = false
        val transport = GitHubFirstPullTransport.forTest("FAKE-PAT", { cancelled }, { time }, {
            calls++
            Fake(it, "{}".toByteArray())
        })
        cancelled = true
        failure { transport.resolve(selection) }
        cancelled = false
        time = 300_000_000_000L
        failure { transport.resolve(selection) }
        time = 0L
        transport.close()
        failure { transport.resolve(selection) }
        assertEquals(0, calls)
    }

    @Test fun malformedJsonAndProviderExceptionAreRedacted() {
        val malformed = GitHubFirstPullTransport.forTest("FAKE-PAT", connect = { Fake(it, "FAKE-PAT not json".toByteArray()) })
        failure { malformed.resolve(selection) }
        val broken = GitHubFirstPullTransport.forTest("FAKE-PAT", connect = { throw IllegalStateException("FAKE-PAT") })
        failure { broken.resolve(selection) }
    }

    @Test fun treeTruncationAndCommitSubstitutionReject() {
        for (second in listOf(
            """{"sha":"${"c".repeat(40)}","tree":{"sha":"$tree"}}""",
            """{"sha":"$commit","tree":{"sha":"$tree"}}""",
        )) {
            var count = 0
            val transport = GitHubFirstPullTransport.forTest("FAKE-PAT", connect = {
                count++
                Fake(it, (if (count == 1) second else """{"sha":"$tree","truncated":true,"tree":[]}""").toByteArray())
            })
            failure { transport.tree(selection, commit) }
            transport.close()
        }
    }


    @Test fun closeClearsCredentialWithoutBlockingSessionLockOnHttpTeardown() {
        val reading = java.util.concurrent.CountDownLatch(1)
        val releaseRead = java.util.concurrent.CountDownLatch(1)
        val disconnecting = java.util.concurrent.CountDownLatch(1)
        val releaseDisconnect = java.util.concurrent.CountDownLatch(1)
        val pool = java.util.concurrent.Executors.newFixedThreadPool(2)
        val transport = GitHubFirstPullTransport.forTest("FAKE-PAT", connect = { url ->
            object : Fake(url, "{}".toByteArray()) {
                override fun getInputStream(): java.io.InputStream {
                    reading.countDown()
                    releaseRead.await(5, java.util.concurrent.TimeUnit.SECONDS)
                    return super.getInputStream()
                }
                override fun disconnect() {
                    disconnecting.countDown()
                    releaseDisconnect.await(5, java.util.concurrent.TimeUnit.SECONDS)
                    super.disconnect()
                }
            }
        })
        try {
            val reader = pool.submit { runCatching { transport.resolve(selection) } }
            assertTrue(reading.await(2, java.util.concurrent.TimeUnit.SECONDS))
            pool.submit { transport.close() }.get(1, java.util.concurrent.TimeUnit.SECONDS)
            assertTrue(disconnecting.await(2, java.util.concurrent.TimeUnit.SECONDS))
            failure { transport.resolve(selection) }
            releaseRead.countDown(); releaseDisconnect.countDown()
            reader.get(2, java.util.concurrent.TimeUnit.SECONDS)
        } finally {
            releaseRead.countDown(); releaseDisconnect.countDown(); transport.close(); pool.shutdownNow()
        }
    }

    @Test fun blobHashSizeAndBase64AreStrict() {
        val bytes = "FAKE bytes".toByteArray()
        val sha = EncryptedFirstPull.gitBlobSha(bytes)
        val valid = JSONObject().put("sha", sha).put("size", bytes.size).put("encoding", "base64")
            .put("content", Base64.getEncoder().encodeToString(bytes)).toString()
        for (bad in listOf(
            JSONObject(valid).put("sha", "a".repeat(40)),
            JSONObject(valid).put("size", bytes.size + 1),
            JSONObject(valid).put("size", "${bytes.size}"),
            JSONObject(valid).put("size", 4_194_305),
            JSONObject(valid).put("content", "!FAKE"),
            JSONObject(valid).put("content", Base64.getEncoder().encodeToString("FAKE tampered".toByteArray())),
        )) {
            val transport = GitHubFirstPullTransport.forTest("FAKE-PAT", connect = { Fake(it, bad.toString().toByteArray()) })
            failure { transport.blob(selection, sha) }
            transport.close()
        }
    }
}
