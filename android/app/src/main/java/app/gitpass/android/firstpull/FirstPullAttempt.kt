package app.gitpass.android.firstpull

import app.gitpass.core.validateFirstPullSnapshot
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/** Single-use memory-only attempt. Cancellation disconnects HTTP; native derivation must finish before disposal. */
class FirstPullAttempt private constructor(
    private val createTransport: (String, () -> Boolean) -> FirstPullTransport,
    private val authenticate: (String, String, String) -> Unit,
) : AutoCloseable {
    constructor() : this({ token, cancelled -> GitHubFirstPullTransport(token, cancelled) }, ::validateFirstPullSnapshot)
    private val stopped = AtomicBoolean(false)
    private val transport = AtomicReference<AutoCloseable?>()
    fun checkpoint() {
        if (stopped.get()) throw FirstPullFailure("First pull cancelled")
    }
    fun stageAndVerify(roots: AccountRoots, selection: RepositorySelection, token: String, password: String, secret: String): StagedEncryptedCopy {
        checkpoint()
        val http = createTransport(token, stopped::get)
        transport.set(http as? AutoCloseable)
        try {
            checkpoint()
            val copy = EncryptedFirstPull(roots.staging, http).stage(selection, stopped::get)
            try {
                checkpoint()
                authenticate(copy.directory.absolutePath, password, secret)
                roots.prepare(copy, ::checkpoint)
                checkpoint()
                return copy
            } catch (error: Throwable) { copy.close(); throw error }
        } finally {
            (http as? AutoCloseable)?.close()
            transport.compareAndSet(http as? AutoCloseable, null)
        }
    }
    override fun close() {
        stopped.set(true)
        transport.getAndSet(null)?.close()
    }
    companion object {
        internal fun forTest(transport: FirstPullTransport, authenticate: (String, String, String) -> Unit = ::validateFirstPullSnapshot) =
            FirstPullAttempt({ _, _ -> transport }, authenticate)
    }
    override fun toString() = "FirstPullAttempt([redacted])"
}
