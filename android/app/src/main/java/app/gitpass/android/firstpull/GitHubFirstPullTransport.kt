package app.gitpass.android.firstpull

import org.json.JSONObject
import java.net.URL
import java.net.URLEncoder
import java.util.Base64
import java.util.concurrent.atomic.AtomicReference
import javax.net.ssl.HttpsURLConnection

/** Fixed GitHub origin only. No redirects, disk cache, stored credentials, response-body errors or push API. */
class GitHubFirstPullTransport private constructor(
    token: String,
    private val cancelled: () -> Boolean,
    private val connect: (URL) -> HttpsURLConnection,
    private val nowNanos: () -> Long,
) : FirstPullTransport, AutoCloseable {
    private val credential = AtomicReference<String?>(token)
    private val active = AtomicReference<HttpsURLConnection?>()
    private val deadline = nowNanos() + 300_000_000_000L
    private var requests = 0

    init { require(token.length in 1..512 && token.all { it.code in 33..126 }) { "Invalid dedicated credential" } }

    constructor(token: String, cancelled: () -> Boolean = { false }) : this(
        token, cancelled, { it.openConnection() as HttpsURLConnection }, System::nanoTime,
    )

    private fun checkpoint() {
        if (cancelled() || credential.get() == null || nowNanos() >= deadline) throw FirstPullFailure("First pull cancelled or timed out")
    }

    private fun request(path: String, limit: Int): JSONObject {
        checkpoint()
        if (++requests > EncryptedFirstPull.MAX_ENTRIES + 3) throw FirstPullFailure("Excessive remote requests")
        // path is built exclusively from validated selection fields and validated object identities.
        val url = URL("https://api.github.com$path")
        val connection = connect(url)
        active.set(connection)
        try {
            checkpoint()
            connection.instanceFollowRedirects = false
            connection.connectTimeout = 15_000
            connection.readTimeout = 15_000
            connection.useCaches = false
            connection.requestMethod = "GET"
            connection.setRequestProperty("Accept", "application/vnd.github+json")
            connection.setRequestProperty("X-GitHub-Api-Version", "2022-11-28")
            connection.setRequestProperty("User-Agent", "gitpass-android-first-pull")
            connection.setRequestProperty("Authorization", "Bearer ${credential.get() ?: throw FirstPullFailure("First pull cancelled")}")
            val status = connection.responseCode
            if (status != 200) throw FirstPullFailure(when (status) {
                401, 403, 404 -> "Repository access failed; verify selection and read-only Contents permission"
                in 300..399 -> "Redirect rejected"
                else -> "GitHub first-pull request failed"
            })
            if (connection.contentLengthLong > limit) throw FirstPullFailure("Remote response exceeds limits")
            val bytes = connection.inputStream.use { input ->
                val output = java.io.ByteArrayOutputStream()
                val chunk = ByteArray(8192)
                while (true) {
                    checkpoint()
                    val count = input.read(chunk)
                    if (count < 0) break
                    if (count > limit - output.size()) throw FirstPullFailure("Remote response exceeds limits")
                    output.write(chunk, 0, count)
                }
                output.toByteArray()
            }
            checkpoint()
            return JSONObject(bytes.toString(Charsets.UTF_8))
        } catch (error: Exception) {
            // Errors from the TLS/HTTP/JSON layers can contain URLs, bodies or header values.
            if (error is FirstPullFailure) throw error
            throw FirstPullFailure("GitHub first-pull response failed")
        } finally {
            active.compareAndSet(connection, null)
            connection.disconnect()
        }
    }

    override fun resolve(selection: RepositorySelection): String = checked {
        identity(request("${repo(selection)}/commits/${encode(selection.branch)}", META_LIMIT).getString("sha"))
    }

    override fun tree(selection: RepositorySelection, commit: String): RemoteTree = checked {
        identity(commit)
        val objectJson = request("${repo(selection)}/git/commits/$commit", META_LIMIT)
        if (identity(objectJson.getString("sha")) != commit) throw FirstPullFailure("Commit identity mismatch")
        val treeSha = identity(objectJson.getJSONObject("tree").getString("sha"))
        val json = request("${repo(selection)}/git/trees/$treeSha?recursive=1", TREE_LIMIT)
        if (identity(json.getString("sha")) != treeSha) throw FirstPullFailure("Tree identity mismatch")
        if (json.get("truncated") !is Boolean || json.getBoolean("truncated")) throw FirstPullFailure("Incomplete remote tree")
        val entries = json.getJSONArray("tree")
        if (entries.length() > EncryptedFirstPull.MAX_ENTRIES) throw FirstPullFailure("Excessive remote tree")
        RemoteTree(commit, false, (0 until entries.length()).map { index ->
            val entry = entries.getJSONObject(index)
            val type = entry.getString("type")
            RemoteEntry(entry.getString("path"), type, entry.getString("mode"), identity(entry.getString("sha")),
                if (type == "tree") 0L else boundedSize(entry))
        })
    }

    override fun blob(selection: RepositorySelection, sha: String): ByteArray = checked {
        identity(sha)
        val json = request("${repo(selection)}/git/blobs/$sha", BLOB_JSON_LIMIT)
        if (identity(json.getString("sha")) != sha || json.getString("encoding") != "base64") throw FirstPullFailure("Blob identity mismatch")
        val size = boundedSize(json)
        val content = json.getString("content")
        // GitHub inserts LF into base64. Reject all other non-alphabet characters (no permissive MIME decoder).
        if (content.any { it != '\n' && it != '\r' && it !in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=" }) throw FirstPullFailure("Invalid blob encoding")
        val bytes = Base64.getDecoder().decode(content.replace("\n", "").replace("\r", ""))
        if (bytes.size.toLong() != size || EncryptedFirstPull.gitBlobSha(bytes) != sha) throw FirstPullFailure("Blob identity mismatch")
        bytes
    }

    private inline fun <T> checked(operation: () -> T): T = try { operation() } catch (error: Exception) {
        if (error is FirstPullFailure) throw error
        throw FirstPullFailure("Invalid GitHub first-pull response")
    }

    override fun close() {
        credential.set(null)
        active.getAndSet(null)?.disconnect()
    }
    override fun toString() = "GitHubFirstPullTransport([redacted])"

    companion object {
        private const val META_LIMIT = 64 * 1024
        private const val TREE_LIMIT = 4 * 1024 * 1024
        private const val BLOB_JSON_LIMIT = 6 * 1024 * 1024
        private fun identity(value: String): String {
            if (!value.matches(Regex("[a-f0-9]{40}"))) throw FirstPullFailure("Invalid object identity")
            return value
        }
        private fun boundedSize(json: JSONObject): Long {
            val raw = json.get("size")
            if (raw !is Int && raw !is Long) throw FirstPullFailure("Invalid blob size")
            val size = (raw as Number).toLong()
            if (size !in 0..EncryptedFirstPull.MAX_FILE_BYTES.toLong()) throw FirstPullFailure("Remote blob exceeds limits")
            return size
        }
        private fun encode(value: String) = URLEncoder.encode(value, "UTF-8").replace("+", "%20")
        private fun repo(selection: RepositorySelection) = "/repos/${encode(selection.owner)}/${encode(selection.repository)}"
        /** Fake connection injection only. Production has no configurable origin or HTTP downgrade. */
        internal fun forTest(token: String, cancelled: () -> Boolean = { false }, nowNanos: () -> Long = System::nanoTime,
            connect: (URL) -> HttpsURLConnection) = GitHubFirstPullTransport(token, cancelled, connect, nowNanos)
    }
}
