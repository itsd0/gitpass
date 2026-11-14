package app.gitpass.android.pairing

import app.gitpass.core.PairingHostInput
import app.gitpass.core.pairingHostBegin
import app.gitpass.core.pairingHostCancel
import app.gitpass.core.pairingHostConfirm
import app.gitpass.core.pairingHostPush
import app.gitpass.core.pairingJoinerCancel
import java.io.ByteArrayOutputStream
import java.io.Closeable
import java.io.File
import java.io.IOException
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.SocketTimeoutException
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

internal const val PAIRING_TEST_TOKEN = "github_pat_FAKE_android_loopback"
internal const val PAIRING_TEST_SECRET = "A3-7DPGPV-RCYDPG-PVRCY-DPGPV-RCYD3"

/**
 * Desktop side of one loopback pairing, on the device. The socket is 127.0.0.1.
 * Bytes are recorded so a test can prove a secret never crossed the socket.
 */
internal class PairingHostPeer(private val dir: File) : Closeable {
    private val server = ServerSocket()
    private val grant = LinkedBlockingQueue<ByteArray>()
    private val wire = ByteArrayOutputStream()
    private val wireLock = Any()
    @Volatile var failure: String? = null
    val qr: String
    private val thread: Thread

    init {
        dir.mkdirs()
        server.bind(InetSocketAddress("127.0.0.1", 0))
        try {
            val input = PairingHostInput(
                dir.absolutePath,
                "00000000-0000-4000-8000-000000000001",
                "fake-owner",
                "fake-vault",
                "main",
                PAIRING_TEST_SECRET,
                PAIRING_TEST_TOKEN,
                "[\"127.0.0.1\"]",
                server.localPort.toUShort(),
            )
            val view = pairingHostBegin(input)
            input.close()
            qr = view.qr
        } catch (e: Exception) {
            server.close()
            throw e
        }
        thread = Thread {
            try {
                serve()
            } catch (e: Exception) {
                failure = e.javaClass.simpleName
            }
        }
        thread.isDaemon = true
        thread.start()
    }

    fun confirm(code: String) {
        val drive = pairingHostConfirm(dir.absolutePath, code)
        if (drive.error.isNotEmpty()) throw IllegalStateException(allowlisted(drive.error))
        if (drive.outbound.isEmpty()) throw IllegalStateException("grant missing")
        grant.put(drive.outbound)
    }

    fun wireText(): String = synchronized(wireLock) { String(wire.toByteArray(), Charsets.ISO_8859_1) }

    private fun serve() {
        server.soTimeout = 20_000
        val sock = server.accept()
        sock.tcpNoDelay = true
        sock.soTimeout = 5_000
        sock.use { current ->
            val input = current.getInputStream()
            val output = current.getOutputStream()
            val buf = ByteArray(8192)
            while (true) {
                val n = readSlice(input, buf)
                if (n < 0) return
                if (n == 0) continue
                record(buf, n)
                val drive = pairingHostPush(dir.absolutePath, buf.copyOf(n))
                if (drive.outbound.isNotEmpty()) {
                    record(drive.outbound, drive.outbound.size)
                    output.write(drive.outbound)
                    output.flush()
                }
                if (drive.error.isNotEmpty() || drive.phase == "code") break
            }
            val bytes = grant.poll(20, TimeUnit.SECONDS) ?: return
            record(bytes, bytes.size)
            output.write(bytes)
            output.flush()
            val end = System.nanoTime() + 20_000_000_000L
            while (System.nanoTime() < end) {
                val n = try {
                    readSlice(input, buf)
                } catch (e: IOException) {
                    return
                }
                if (n < 0) return
                if (n > 0) record(buf, n)
            }
        }
    }

    private fun readSlice(input: java.io.InputStream, buf: ByteArray): Int =
        try {
            input.read(buf)
        } catch (e: SocketTimeoutException) {
            0
        }

    private fun record(bytes: ByteArray, len: Int) {
        synchronized(wireLock) { wire.write(bytes, 0, len) }
    }

    override fun close() {
        try {
            server.close()
        } catch (e: IOException) {}
        thread.join(3_000)
        runCatching { pairingHostCancel(dir.absolutePath) }
        runCatching { pairingJoinerCancel() }
    }
}
