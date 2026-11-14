package app.gitpass.android.pairing

import app.gitpass.core.PairedSync
import app.gitpass.core.PairingDrive
import app.gitpass.core.PairingEndpoint
import app.gitpass.core.pairingJoinerCancel
import app.gitpass.core.pairingJoinerCode
import app.gitpass.core.pairingJoinerEndpoint
import app.gitpass.core.pairingJoinerFinish
import app.gitpass.core.pairingJoinerPhase
import app.gitpass.core.pairingJoinerPush
import app.gitpass.core.pairingJoinerStart
import java.io.File
import kotlin.coroutines.cancellation.CancellationException

/**
 * What the phone does after a scan. The platform only moves opaque bytes.
 * gitpass-core owns the key exchange, the code and the grant.
 */
interface PairingClient {
    fun connectUntilCode(qr: String): PairingConnected
    fun receiveGrant(): PairedSync
    fun close()
}

/** Shown on the phone. Not a secret, but not useful in a log either. */
class PairingConnected(val code: String, val expiresAt: Long) {
    override fun toString() = "PairingConnected([redacted])"
}

object PairingClients {
    @Volatile
    var factory: (File) -> PairingClient = { dir -> LanPairingClient(dir) }

    fun create(dir: File): PairingClient = factory(dir)

    fun reset() {
        factory = { dir -> LanPairingClient(dir) }
    }
}

internal interface PairingJoiner {
    fun start(qr: String): ByteArray
    fun endpoint(): PairingEndpoint
    fun push(chunk: ByteArray): PairingDrive
    fun phase(): String
    fun code(): String
    fun finish(stateDir: String): PairedSync
    fun cancel()
}

internal class FfiJoiner(private val stateDir: File) : PairingJoiner {
    override fun start(qr: String) = pairingJoinerStart(qr, stateDir.absolutePath)
    override fun endpoint() = pairingJoinerEndpoint()
    override fun push(chunk: ByteArray) = pairingJoinerPush(chunk)
    override fun phase() = pairingJoinerPhase()
    override fun code() = pairingJoinerCode()
    override fun finish(stateDir: String) = pairingJoinerFinish(stateDir)
    override fun cancel() {
        pairingJoinerCancel()
    }
}

class LanPairingClient internal constructor(
    private val stateDir: File,
    private val joiner: PairingJoiner,
    private val link: PairingTransport,
    private val maxWaitMs: Long,
) : PairingClient {
    constructor(stateDir: File) : this(stateDir, FfiJoiner(stateDir), TcpPairingTransport(), 120_000)

    @Volatile private var stopped = false
    private var cancelled = false
    private var waitNs = 120_000_000_000L

    override fun connectUntilCode(qr: String): PairingConnected {
        val commit = guard { joiner.start(qr) }
        val ep = guard { joiner.endpoint() }
        val now = System.currentTimeMillis() / 1000
        if (ep.expiresAt <= now) {
            close()
            throw PairingLinkException("pairing session expired")
        }
        val remainingMs = ((ep.expiresAt - now) * 1000).coerceAtMost(maxWaitMs).coerceAtLeast(200)
        waitNs = remainingMs * 1_000_000L
        try {
            link.connect(ep.addresses, ep.port.toInt(), CONNECT_MS)
        } catch (e: PairingLinkException) {
            close()
            throw e
        }
        return try {
            pump(commit, "code")
            PairingConnected(guard { joiner.code() }, ep.expiresAt)
        } catch (e: PairingLinkException) {
            close()
            throw e
        }
    }

    override fun receiveGrant(): PairedSync {
        try {
            pump(ByteArray(0), "done")
            return guard { joiner.finish(stateDir.absolutePath) }
        } finally {
            close()
        }
    }

    @Synchronized
    override fun close() {
        stopped = true
        link.close()
        if (!cancelled) {
            cancelled = true
            runCatching { joiner.cancel() }
        }
    }

    private fun pump(initial: ByteArray, target: String) {
        var outbound = initial
        val buf = ByteArray(8192)
        val deadline = System.nanoTime() + waitNs
        while (System.nanoTime() < deadline) {
            if (stopped) throw PairingLinkException("pairing failed")
            if (outbound.isNotEmpty()) link.write(outbound)
            outbound = ByteArray(0)
            val phase = guard { joiner.phase() }
            if (phase == "failed") throw PairingLinkException("pairing failed")
            if (phase == target) return
            val n = link.read(buf)
            if (n == 0) continue
            val drive = guard { joiner.push(buf.copyOf(n)) }
            if (drive.error.isNotEmpty()) {
                if (drive.outbound.isNotEmpty()) runCatching { link.write(drive.outbound) }
                throw PairingLinkException(allowlisted(drive.error))
            }
            outbound = drive.outbound
            if (drive.phase == "failed") throw PairingLinkException("pairing failed")
            if (drive.phase == target) {
                if (outbound.isNotEmpty()) link.write(outbound)
                return
            }
        }
        throw PairingLinkException("pairing timed out")
    }

    private fun <T> guard(block: () -> T): T {
        try {
            return block()
        } catch (e: CancellationException) {
            throw e
        } catch (e: PairingLinkException) {
            throw e
        } catch (e: Exception) {
            throw PairingLinkException(allowlisted(e.message ?: ""))
        }
    }

    private companion object {
        const val CONNECT_MS = 5_000
    }
}
