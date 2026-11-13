package app.gitpass.android.pairing

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PairingQrTest {
    private fun luma(text: String, size: Int = 360, rowStride: Int = size + 16): ByteArray {
        val m = PairingQr.encode(text, size)
        val data = ByteArray(rowStride * size) { 0x7f }
        for (y in 0 until size) for (x in 0 until size) data[y * rowStride + x] = if (m[x, y]) 0 else 0xff.toByte()
        return data
    }

    @Test fun roundTripsAGrantSizedFrameThroughACameraLikeBuffer() {
        val grant = "gitpass-pair:1:grant:" + "A".repeat(22) + "." + "B".repeat(43) + "." + "C".repeat(43) + ".1700000120." + "x".repeat(700)
        val data = luma(grant, size = 600, rowStride = 640)
        assertEquals(grant, PairingQr.decodeLuma(data, rowStride = 640, width = 600, height = 600))
    }

    @Test fun recognisesFramesByPrefixOnly() {
        assertEquals(PairingQr.Frame.OFFER, PairingQr.frameOf("gitpass-pair:1:offer:s.p.1"))
        assertEquals(PairingQr.Frame.ANNOUNCE, PairingQr.frameOf("gitpass-pair:1:announce:s.p.h.1"))
        assertEquals(PairingQr.Frame.GRANT, PairingQr.frameOf("gitpass-pair:1:grant:s.p.h.1.e"))
        assertNull(PairingQr.frameOf("gitpass-pair:2:grant:s"))
        assertNull(PairingQr.frameOf("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP"))
        assertNull(PairingQr.frameOf("gitpass-pair:1:grant:" + "x".repeat(5000)))
    }

    @Test fun rejectsBadBuffersAndBlankFrames() {
        assertNull(PairingQr.decodeLuma(ByteArray(10), rowStride = 4, width = 8, height = 8))
        assertNull(PairingQr.decodeLuma(ByteArray(100 * 100) { 0x7f }, rowStride = 100, width = 100, height = 100))
    }

    @Test fun readsTheOfferExpiry() {
        assertEquals(1700000120L, PairingQr.offerExpiry("gitpass-pair:1:offer:sid.pub.1700000120"))
        assertNull(PairingQr.offerExpiry("gitpass-pair:1:offer:sid.pub.x"))
    }
}
