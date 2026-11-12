import Foundation

/// Startup handshake with the native bridge (`crates/gitpass-mobile-ffi`).
///
/// The app refuses to open a vault when the linked native library is not
/// the one this app code was written against (see `ffiVersion()` and
/// `formatVersion()`). No secret is involved in this check.
enum BridgeCompat {
    /// `FFI_API_LEVEL` in `crates/gitpass-mobile-ffi/src/lib.rs`.
    static let expectedApiLevel: UInt32 = 5
    /// Vault format (`FORMAT_VERSION` in gitpass-core) this app handles.
    static let expectedFormat: UInt32 = 5

    static func isCompatible(apiLevel: UInt32, minRead: UInt32, currentWrite: UInt32) -> Bool {
        apiLevel == expectedApiLevel && minRead <= currentWrite && currentWrite == expectedFormat
    }

    /// Calls into the native library.
    static func checkNative() -> Bool {
        let ffi = ffiVersion()
        let format = formatVersion()
        return isCompatible(apiLevel: ffi.apiLevel, minRead: format.minRead, currentWrite: format.currentWrite)
    }

    static let incompatibleMessage = String(localized: "This build of gitpass has a native core that does not match the app. The vault was not opened and nothing was changed. Install a complete, matching build of the app.")
}
