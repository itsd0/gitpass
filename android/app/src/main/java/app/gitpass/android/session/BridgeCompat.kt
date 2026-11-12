package app.gitpass.android.session

import app.gitpass.core.ffiVersion
import app.gitpass.core.formatVersion

/**
 * Startup handshake with the native bridge (`crates/gitpass-mobile-ffi`).
 *
 * The app refuses to open any vault when the loaded native library is not
 * the one this app code was written against. Fails closed: a library that
 * cannot be loaded, or whose UniFFI checksums do not match the generated
 * bindings, also counts as incompatible.
 */
object BridgeCompat {
    /** `FFI_API_LEVEL` in `crates/gitpass-mobile-ffi/src/lib.rs`. */
    const val EXPECTED_API_LEVEL: UInt = 5u

    /** Vault format (`FORMAT_VERSION` in gitpass-core) this app handles. */
    const val EXPECTED_FORMAT: UInt = 5u

    fun isCompatible(apiLevel: UInt, minRead: UInt, currentWrite: UInt): Boolean =
        apiLevel == EXPECTED_API_LEVEL && minRead <= currentWrite && currentWrite == EXPECTED_FORMAT

    /** Calls into the native library; never throws. */
    fun checkNative(): Boolean = runCatching {
        val ffi = ffiVersion()
        val format = formatVersion()
        isCompatible(ffi.apiLevel, format.minRead, format.currentWrite)
    }.getOrDefault(false)
}
