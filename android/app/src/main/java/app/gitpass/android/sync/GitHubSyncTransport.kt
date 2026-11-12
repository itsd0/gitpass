package app.gitpass.android.sync

import app.gitpass.core.HttpTransport
import app.gitpass.core.HttpTransportException
import app.gitpass.core.HttpTransportRequest
import app.gitpass.core.HttpTransportResponse
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.net.SocketTimeoutException
import java.net.URL
import java.util.concurrent.atomic.AtomicReference
import javax.net.ssl.HttpsURLConnection

/**
 * HTTPS round trip for the core sync engine (#455), on the platform network
 * stack. Fixed origin (`https://api.github.com`), no redirects, no cache,
 * bounded reads, cancellation checked between chunks. The token is held here
 * only for the `Authorization` header (it is not part of the request record
 * from Rust) and cleared on [close]. Never logs; errors carry no text from
 * the TLS/HTTP layers.
 */
class GitHubSyncTransport internal constructor(
    token: String,
    private val cancelled: () -> Boolean,
    private val connect: (URL) -> HttpsURLConnection,
) : HttpTransport, AutoCloseable {
    private val credential = AtomicReference<String?>(token)
    private val active = AtomicReference<HttpsURLConnection?>()

    init { require(validToken(token)) { "Invalid sync token" } }

    constructor(token: String, cancelled: () -> Boolean = { false }) :
        this(token, cancelled, { it.openConnection() as HttpsURLConnection })

    override fun send(request: HttpTransportRequest): HttpTransportResponse {
        checkpoint()
        if (request.method !in METHODS) throw HttpTransportException.Transport()
        if (!validPath(request.path)) throw HttpTransportException.Transport()
        val limit = request.maxResponseBytes.coerceAtMost(MAX_RESPONSE.toULong()).toLong()
        val timeout = request.timeoutMs.coerceIn(1_000uL, 120_000uL).toInt()
        val connection = try { connect(URL(ORIGIN + request.path)) } catch (_: Exception) { throw HttpTransportException.Transport() }
        active.set(connection)
        try {
            checkpoint()
            connection.instanceFollowRedirects = false
            connection.useCaches = false
            connection.connectTimeout = timeout
            connection.readTimeout = timeout
            connection.requestMethod = request.method
            for (header in request.headers) {
                if (header.name.equals("Authorization", ignoreCase = true)) continue
                if (!header.name.matches(HEADER_NAME) || header.value.any { it.code !in 32..126 }) throw HttpTransportException.Transport()
                connection.setRequestProperty(header.name, header.value)
            }
            connection.setRequestProperty("Authorization", "Bearer ${credential.get() ?: throw HttpTransportException.Cancelled()}")
            val body = request.body
            if (body != null) {
                connection.doOutput = true
                connection.setFixedLengthStreamingMode(body.size)
                connection.outputStream.use { it.write(body) }
            }
            val status = connection.responseCode
            checkpoint()
            val stream: InputStream? = if (status >= 400) connection.errorStream else if (status in 300..399) null else connection.inputStream
            if (connection.contentLengthLong > limit) throw HttpTransportException.TooLarge()
            val bytes = stream?.use { read(it, limit) } ?: ByteArray(0)
            checkpoint()
            return HttpTransportResponse(
                status = status.toUShort(),
                body = bytes,
                retryAfter = connection.getHeaderField("retry-after")?.toULongOrNull(),
                rateLimitRemaining = connection.getHeaderField("x-ratelimit-remaining")?.toULongOrNull(),
                rateLimitReset = connection.getHeaderField("x-ratelimit-reset")?.toULongOrNull(),
            )
        } catch (error: HttpTransportException) {
            throw error
        } catch (_: SocketTimeoutException) {
            throw if (cancelled()) HttpTransportException.Cancelled() else HttpTransportException.Timeout()
        } catch (_: Exception) {
            // TLS/HTTP errors can contain URLs or header values: never propagate their text.
            throw if (cancelled() || credential.get() == null) HttpTransportException.Cancelled() else HttpTransportException.Transport()
        } finally {
            active.compareAndSet(connection, null)
            connection.disconnect()
        }
    }

    private fun read(input: InputStream, limit: Long): ByteArray {
        val output = ByteArrayOutputStream()
        val chunk = ByteArray(8192)
        while (true) {
            checkpoint()
            val count = input.read(chunk)
            if (count < 0) break
            if (output.size().toLong() + count > limit) throw HttpTransportException.TooLarge()
            output.write(chunk, 0, count)
        }
        return output.toByteArray()
    }

    private fun checkpoint() {
        if (cancelled() || credential.get() == null) throw HttpTransportException.Cancelled()
    }

    /** Clears the token and aborts the in-flight request without blocking the caller. */
    override fun close() {
        credential.set(null)
        active.getAndSet(null)?.let { connection ->
            runCatching { cancelExecutor.execute { runCatching { connection.disconnect() } } }
        }
    }

    override fun toString() = "GitHubSyncTransport([redacted])"

    companion object {
        const val ORIGIN = "https://api.github.com"
        private val METHODS = setOf("GET", "POST", "PATCH", "PUT")
        private const val MAX_RESPONSE = 64 * 1024 * 1024
        private val HEADER_NAME = Regex("[A-Za-z0-9-]{1,64}")
        private val cancelExecutor = java.util.concurrent.Executors.newSingleThreadExecutor { task ->
            Thread(task, "sync-cancel").apply { isDaemon = true }
        }

        /** Fine-grained PATs only (core enforces the same); printable ASCII, bounded. */
        fun validToken(token: String) = token.startsWith("github_pat_") && token.length in 20..512 && token.all { it.code in 33..126 }

        /** `/repos/...` built by core from validated parts; refuse anything that could change origin or traverse. */
        internal fun validPath(path: String) = path.startsWith("/repos/") && path.length <= 2048 &&
            path.all { it.code in 33..126 } && !path.contains("..") && !path.contains("//") && !path.contains('@') &&
            !path.contains('\\') && !path.contains('#')

        /** Fake connection injection only. Production has no configurable origin. */
        internal fun forTest(token: String, cancelled: () -> Boolean = { false }, connect: (URL) -> HttpsURLConnection) =
            GitHubSyncTransport(token, cancelled, connect)
    }
}
