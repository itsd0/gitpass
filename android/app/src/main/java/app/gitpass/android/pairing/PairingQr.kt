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
 * Camera-side QR decode for pairing v2 (#491). The desktop QR has no secrets.
 * gitpass-core parses it. This only rejects anything that is not that QR.
 */
internal object PairingQr {
    private const val PREFIX = "gitpass-pair:2:qr:"

    fun isDesktopQr(text: String): Boolean = text.length in 1..4096 && text.startsWith(PREFIX)

    fun encode(text: String, size: Int): BitMatrix =
        QRCodeWriter().encode(
            text,
            BarcodeFormat.QR_CODE,
            size,
            size,
            mapOf(EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M, EncodeHintType.MARGIN to 2),
        )

    fun decode(source: LuminanceSource): String? = runCatching {
        QRCodeReader().decode(
            BinaryBitmap(HybridBinarizer(source)),
            mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE), DecodeHintType.TRY_HARDER to true),
        ).text
    }.getOrNull()

    /** Y plane of a camera frame. `rowStride` >= `width`. The frame is not stored. */
    fun decodeLuma(data: ByteArray, rowStride: Int, width: Int, height: Int): String? {
        if (width <= 0 || height <= 0 || rowStride < width || data.size < rowStride * (height - 1) + width) return null
        val rows = (data.size + rowStride - 1) / rowStride
        return decode(PlanarYUVLuminanceSource(data, rowStride, rows, 0, 0, width, height, false))
    }
}
