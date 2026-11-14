package app.gitpass.android.pairing

import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketTimeoutException

/**
 * Raw TCP between the phone and the desktop (#491). This is not HTTP, so the
 * network security config's cleartext rules do not apply and are left unchanged.
 * The bytes are opaque: gitpass-core encrypts the grant. Nothing here is logged.
 */
internal interface PairingTransport {
    fun connect(addresses: List<String>, port: Int, timeoutMs: Int)
    fun write(bytes: ByteArray)
    fun read(buf: ByteArray): Int
    fun close()
}

/** Fixed message only. The cause is never attached, so it cannot carry an address. */
internal class PairingLinkException(message: String) : Exception(message) {
    override fun toString() = message ?: "pairing failed"
}

internal fun allowlisted(raw: String): String {
    val known = listOf(
        "pairing session expired",
        "pairing session already used",
        "pairing payload failed authentication",
        "unsupported pairing protocol version",
        "unrecognized pairing message",
        "pairing key rejected",
        "pairing payload field rejected",
        "pairing state unavailable",
        "no pairing in progress",
        "too many wrong pairing codes",
        "pairing message too large",
        "pairing message truncated",
        "pairing session does not match",
        "pairing could not connect",
        "pairing timed out",
        "pairing connection closed",
        "pairing failed",
    )
    return known.firstOrNull { raw.contains(it) } ?: "pairing failed"
}

internal class TcpPairingTransport : PairingTransport {
    private val gate = Any()
    private var socket: Socket? = null

    override fun connect(addresses: List<String>, port: Int, timeoutMs: Int) {
        if (port !in 1..65535) throw PairingLinkException("pairing could not connect")
        var last = PairingLinkException("pairing could not connect")
        for (host in addresses) {
            val addr = numeric(host) ?: continue
            val opened = Socket()
            try {
                opened.tcpNoDelay = true
                opened.connect(InetSocketAddress(addr, port), timeoutMs)
                opened.soTimeout = SLICE_MS
                synchronized(gate) { socket = opened }
                return
            } catch (e: SocketTimeoutException) {
                closeQuiet(opened)
                last = PairingLinkException("pairing timed out")
            } catch (e: IOException) {
                closeQuiet(opened)
                last = PairingLinkException("pairing could not connect")
            }
        }
        throw last
    }

    override fun write(bytes: ByteArray) {
        if (bytes.isEmpty()) return
        val current = synchronized(gate) { socket } ?: throw PairingLinkException("pairing connection closed")
        try {
            current.getOutputStream().write(bytes)
            current.getOutputStream().flush()
        } catch (e: IOException) {
            throw PairingLinkException("pairing connection closed")
        }
    }

    /** 0 means this slice had no bytes. The caller keeps waiting until its own deadline. */
    override fun read(buf: ByteArray): Int {
        val current = synchronized(gate) { socket } ?: throw PairingLinkException("pairing connection closed")
        try {
            val n = current.getInputStream().read(buf)
            if (n < 0) throw PairingLinkException("pairing connection closed")
            return n
        } catch (e: PairingLinkException) {
            throw e
        } catch (e: SocketTimeoutException) {
            return 0
        } catch (e: IOException) {
            throw PairingLinkException("pairing connection closed")
        }
    }

    override fun close() {
        val current = synchronized(gate) { socket.also { socket = null } }
        closeQuiet(current)
    }

    private fun closeQuiet(current: Socket?) {
        try {
            current?.close()
        } catch (e: IOException) {
            // The caller already has a fixed message.
        }
    }

    private fun numeric(host: String): InetAddress? {
        val v4 = host.split('.')
        if (v4.size == 4 && v4.all { part -> part.length in 1..3 && part.all { it.isDigit() } }) {
            val numbers = v4.map { it.toIntOrNull() ?: return null }
            if (numbers.any { it !in 0..255 }) return null
            return InetAddress.getByAddress(numbers.map { it.toByte() }.toByteArray())
        }
        if (host.contains(':') && host.all { it.isDigit() || it in 'a'..'f' || it in 'A'..'F' || it == ':' }) {
            return runCatching { InetAddress.getByName(host) }.getOrNull()
        }
        return null
    }

    private companion object {
        const val SLICE_MS = 200
    }
}
