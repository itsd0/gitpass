package app.gitpass.android.pairing

import app.gitpass.android.R
import app.gitpass.android.ui.pairingErrorRes
import app.gitpass.core.PairingDrive
import app.gitpass.core.PairingEndpoint
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.SocketTimeoutException
import java.nio.charset.Charset
import java.util.concurrent.atomic.AtomicReference
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** JVM tests for the phone's byte shuttle. No bridge and no camera. */
class PairingCoreTest {
    private val ascii: Charset = Charsets.ISO_8859_1

    @Test fun knownErrorsDropAnythingAfterThem() {
        assertEquals("pairing could not connect", allowlisted("pairing could not connect 203.0.113.8"))
        assertEquals("pairing failed", allowlisted("203.0.113.8"))
        assertEquals("pairing session expired", allowlisted("v1=pairing session expired"))
        assertEquals(R.string.pair_connect, pairingErrorRes("v1=pairing could not connect"))
        assertEquals(R.string.pair_timeout, pairingErrorRes("pairing timed out"))
        assertEquals(R.string.pair_closed, pairingErrorRes("pairing connection closed"))
        assertEquals(R.string.pair_expired, pairingErrorRes("pairing session expired"))
        assertEquals(R.string.pair_failed, pairingErrorRes("203.0.113.8"))
    }

    @Test fun aHostnameIsNotDialedAndIsNotInTheError() {
        val thrown = assertThrows(PairingLinkException::class.java) {
            TcpPairingTransport().connect(listOf("not-a-host.invalid", "127.0.0.1"), 1, 400)
        }
        assertTrue(thrown.message == "pairing could not connect" || thrown.message == "pairing timed out")
        assertFalse(thrown.toString().contains("not-a-host"))
        assertFalse(thrown.toString().contains("invalid"))
        assertFalse(thrown.toString().contains("127.0.0.1"))
    }

    @Test fun aClosedPortDoesNotEchoTheAddress() {
        val thrown = assertThrows(PairingLinkException::class.java) {
            TcpPairingTransport().connect(listOf("127.0.0.1"), 1, 400)
        }
        assertTrue(thrown.message == "pairing could not connect" || thrown.message == "pairing timed out")
        assertFalse(thrown.toString().contains("127.0.0.1"))
    }

    @Test fun anExpiredQrDoesNotOpenASocket() {
        var connected = false
        val link = object : PairingTransport {
            override fun connect(addresses: List<String>, port: Int, timeoutMs: Int) { connected = true }
            override fun write(bytes: ByteArray) {}
            override fun read(buf: ByteArray) = 0
            override fun close() {}
        }
        val client = LanPairingClient(java.io.File("."), ScriptJoiner(9), link, 1_000)
        val thrown = assertThrows(PairingLinkException::class.java) {
            client.connectUntilCode("gitpass-pair:2:qr:FAKE")
        }
        assertEquals("pairing session expired", thrown.message)
        assertFalse(connected)
    }

    @Test fun theShuttleReachesTheCodeWithoutSendingAPassword() {
        val server = ServerSocket()
        server.bind(InetSocketAddress("127.0.0.1", 0))
        val seen = ByteArrayOutputStream()
        val failure = AtomicReference<String>()
        val thread = Thread {
            try {
                val sock = server.accept()
                sock.soTimeout = 3_000
                sock.tcpNoDelay = true
                sock.use {
                    val input = it.getInputStream()
                    val output = it.getOutputStream()
                    seen.write(readUntil(input, "COMMIT"))
                    output.write("REVEAL".toByteArray())
                    output.flush()
                    seen.write(readUntil(input, "OPEN"))
                    output.write("READY".toByteArray())
                    output.flush()
                }
            } catch (e: Exception) {
                failure.set(e.javaClass.simpleName)
            }
        }
        thread.isDaemon = true
        thread.start()
        try {
            val client = LanPairingClient(
                java.io.File("."),
                ScriptJoiner(server.localPort, expiresInSeconds = 30),
                TcpPairingTransport(),
                5_000,
            )
            val connected = client.connectUntilCode("gitpass-pair:2:qr:FAKE")
            assertEquals("123456", connected.code)
            client.close()
            thread.join(3_000)
            val wire = seen.toString(ascii.name())
            assertTrue(wire.contains("COMMIT"))
            assertTrue(wire.contains("OPEN"))
            assertFalse(wire.contains("FAKE-master-password"))
            assertFalse(wire.contains("github_pat_"))
            assertNull(failure.get())
        } finally {
            server.close()
        }
    }

    @Test fun aSilentPeerTimesOutWithAFixedMessage() {
        val server = ServerSocket()
        server.bind(InetSocketAddress("127.0.0.1", 0))
        val thread = Thread {
            try {
                val sock = server.accept()
                sock.soTimeout = 2_000
                try { sock.getInputStream().read(ByteArray(32)) } catch (_: Exception) {}
                Thread.sleep(2_000)
                sock.close()
            } catch (_: Exception) {}
        }
        thread.isDaemon = true
        thread.start()
        try {
            val client = LanPairingClient(
                java.io.File("."),
                ScriptJoiner(server.localPort, expiresInSeconds = 30),
                TcpPairingTransport(),
                600,
            )
            val thrown = assertThrows(PairingLinkException::class.java) {
                client.connectUntilCode("gitpass-pair:2:qr:FAKE")
            }
            assertEquals("pairing timed out", thrown.message)
            assertFalse(thrown.toString().contains("127.0.0.1"))
        } finally {
            server.close()
        }
    }

    @Test fun aPeerThatClosesUsesAFixedMessage() {
        val server = ServerSocket()
        server.bind(InetSocketAddress("127.0.0.1", 0))
        val thread = Thread {
            try {
                val sock = server.accept()
                sock.soTimeout = 2_000
                try { sock.getInputStream().read(ByteArray(64)) } catch (_: Exception) {}
                sock.close()
            } catch (_: Exception) {}
        }
        thread.isDaemon = true
        thread.start()
        try {
            val client = LanPairingClient(
                java.io.File("."),
                ScriptJoiner(server.localPort, expiresInSeconds = 30),
                TcpPairingTransport(),
                2_000,
            )
            val thrown = assertThrows(PairingLinkException::class.java) {
                client.connectUntilCode("gitpass-pair:2:qr:FAKE")
            }
            assertEquals("pairing connection closed", thrown.message)
            assertFalse(thrown.toString().contains("127.0.0.1"))
        } finally {
            server.close()
        }
    }

    @Test fun aDesktopQrRoundTripsAndAVersionOneQrIsIgnored() {
        val text = "gitpass-pair:2:qr:FAKE"
        assertTrue(PairingQr.isDesktopQr(text))
        assertFalse(PairingQr.isDesktopQr("gitpass-pair:1:offer:FAKE"))
        assertFalse(PairingQr.isDesktopQr("x".repeat(4097)))
        val size = 240
        val matrix = PairingQr.encode(text, size)
        val bytes = ByteArray(size * size)
        for (y in 0 until size) {
            for (x in 0 until size) bytes[y * size + x] = if (matrix.get(x, y)) 0 else 255.toByte()
        }
        assertEquals(text, PairingQr.decodeLuma(bytes, size, size, size))
        assertFalse(text.contains("github_pat_"))
        assertFalse(text.contains("master"))
    }

    private fun readUntil(input: InputStream, marker: String): ByteArray {
        val out = ByteArrayOutputStream()
        val buf = ByteArray(64)
        while (!out.toString(ascii.name()).contains(marker)) {
            val n = try {
                input.read(buf)
            } catch (e: SocketTimeoutException) {
                throw e
            }
            if (n < 0) break
            out.write(buf, 0, n)
        }
        return out.toByteArray()
    }

}

/** Speaks the scripted byte stream the shuttle test drives. Not the real protocol. */
private class ScriptJoiner(private val port: Int, private val expiresInSeconds: Long = 0) : PairingJoiner {
    private var phase = "wait"
    private val inbox = ByteArrayOutputStream()
    private var opened = false

    override fun start(qr: String): ByteArray {
        check(PairingQr.isDesktopQr(qr))
        phase = "wait"
        return "COMMIT".toByteArray()
    }

    override fun endpoint(): PairingEndpoint {
        val exp = if (expiresInSeconds == 0L) 0L else System.currentTimeMillis() / 1000 + expiresInSeconds
        return PairingEndpoint(port.toUShort(), listOf("127.0.0.1"), exp)
    }

    override fun push(chunk: ByteArray): PairingDrive {
        inbox.write(chunk)
        val text = inbox.toString(Charsets.ISO_8859_1.name())
        return when {
            text.contains("READY") -> {
                phase = "code"
                PairingDrive(ByteArray(0), "code", "")
            }
            text.contains("REVEAL") && !opened -> {
                opened = true
                PairingDrive("OPEN".toByteArray(), "wait", "")
            }
            else -> PairingDrive(ByteArray(0), phase, "")
        }
    }

    override fun phase() = phase
    override fun code() = if (phase == "code") "123456" else throw IllegalStateException("early")
    override fun finish(stateDir: String) = throw IllegalStateException("unused")
    override fun cancel() { phase = "failed" }
}
