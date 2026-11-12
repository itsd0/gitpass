package app.gitpass.android.passkey

import java.io.ByteArrayOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ConcurrentHashMap

/** The rpId's Digital Asset Links document, or "" when it is unavailable (#477). */
fun interface AssetLinksSource {
    /** [host] was validated by gitpass-core (`passkeyAppRpHost`). Never call on the main thread. */
    fun fetch(host: String): String
}

/**
 * `https://<host>/.well-known/assetlinks.json` over HTTPS only, without
 * following redirects, with short timeouts and a size cap. Results are
 * cached briefly in memory (failures for less time) so the begin step and
 * the following activity do not fetch twice.
 */
class HttpsAssetLinks(
    private val connect: (URL) -> HttpURLConnection = { it.openConnection() as HttpURLConnection },
    private val nowMs: () -> Long = System::currentTimeMillis,
) : AssetLinksSource {
    private val cache = ConcurrentHashMap<String, Pair<Long, String>>()

    override fun fetch(host: String): String {
        val now = nowMs()
        cache[host]?.let { (until, body) -> if (now < until) return body }
        val body = runCatching { download(host) }.getOrDefault("")
        cache[host] = (now + if (body.isEmpty()) FAILURE_TTL_MS else SUCCESS_TTL_MS) to body
        return body
    }

    fun clear() = cache.clear()

    private fun download(host: String): String {
        val conn = connect(url(host))
        try {
            conn.instanceFollowRedirects = false
            conn.connectTimeout = TIMEOUT_MS
            conn.readTimeout = TIMEOUT_MS
            conn.useCaches = false
            conn.setRequestProperty("Accept", "application/json")
            if (conn.responseCode != HttpURLConnection.HTTP_OK) return ""
            val out = ByteArrayOutputStream()
            conn.inputStream.use { input ->
                val buf = ByteArray(8192)
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    if (out.size() > MAX_BYTES) return ""
                }
            }
            return out.toString(Charsets.UTF_8.name())
        } finally {
            conn.disconnect()
        }
    }

    companion object {
        const val MAX_BYTES = 512 * 1024
        const val TIMEOUT_MS = 5_000
        const val SUCCESS_TTL_MS = 5 * 60_000L
        const val FAILURE_TTL_MS = 30_000L
        fun url(host: String) = URL("https", host, "/.well-known/assetlinks.json")
    }
}
