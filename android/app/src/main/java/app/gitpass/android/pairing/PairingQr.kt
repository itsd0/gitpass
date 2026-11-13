package app.gitpass.android.pairing

import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.EncodeHintType
import com.google.zxing.LuminanceSource
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.BitMatrix
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel

/**
 * QR encode/decode for device pairing (#458). Pure ZXing, no Android types, so it is unit-tested on the
 * JVM. Nothing here touches secrets: the phone only encodes its `offer` (public key + session id) and
 * decodes the desktop's `announce` and `grant` frames, which the Rust core then validates.
 */
object PairingQr {
    private const val PREFIX = "gitpass-pair:1:"

    enum class Frame { OFFER, ANNOUNCE, GRANT }

    /** Cheap prefix check before handing scanned text to the core; the core does the real parsing. */
    fun frameOf(text: String): Frame? = when {
        text.length > 4096 -> null
        text.startsWith(PREFIX + "offer:") -> Frame.OFFER
        text.startsWith(PREFIX + "announce:") -> Frame.ANNOUNCE
        text.startsWith(PREFIX + "grant:") -> Frame.GRANT
        else -> null
    }

    fun encode(text: String, size: Int): BitMatrix =
        QRCodeWriter().encode(
            text, BarcodeFormat.QR_CODE, size, size,
            mapOf(EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M, EncodeHintType.MARGIN to 2),
        )

    /** Text of a QR code in a luminance image, or null. */
    fun decode(source: LuminanceSource): String? = runCatching {
        QRCodeReader().decode(
            BinaryBitmap(HybridBinarizer(source)),
            mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE), DecodeHintType.TRY_HARDER to true),
        ).text
    }.getOrNull()

    /** Decode the Y (luminance) plane of a camera frame. `rowStride` >= `width`, pixel stride 1. */
    fun decodeLuma(data: ByteArray, rowStride: Int, width: Int, height: Int): String? {
        if (width <= 0 || height <= 0 || rowStride < width || data.size < rowStride * (height - 1) + width) return null
        val rows = (data.size + rowStride - 1) / rowStride
        return decode(PlanarYUVLuminanceSource(data, rowStride, rows, 0, 0, width, height, false))
    }

    /** Seconds left before the offer's absolute expiry (`...<exp>` is the last dot field of an offer). */
    fun offerExpiry(offer: String): Long? = offer.substringAfterLast('.', "").toLongOrNull()
}
