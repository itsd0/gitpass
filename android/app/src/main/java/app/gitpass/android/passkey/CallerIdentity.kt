package app.gitpass.android.passkey

import java.security.MessageDigest

/**
 * Who asked for a passkey (#477), established from what Android reports
 * about the calling app, never from the request JSON.
 */
sealed class CallerIdentity {
    abstract val packageName: String

    /** A browser on the privileged allowlist; [origin] is the web origin it vouches for. */
    data class Browser(override val packageName: String, val origin: String) : CallerIdentity()

    /** Any other app: package + SHA-256 of its single signing certificate. */
    class App(override val packageName: String, val certSha256: ByteArray) : CallerIdentity() {
        override fun equals(other: Any?) = other is App && other.packageName == packageName && other.certSha256.contentEquals(certSha256)
        override fun hashCode() = 31 * packageName.hashCode() + certSha256.contentHashCode()
        override fun toString() = "App($packageName)"
    }

    /** Stable key that binds an unlock grant to this caller. */
    val key: String
        get() = when (this) {
            is Browser -> "web:$origin"
            is App -> "app:$packageName:${certSha256.joinToString("") { "%02x".format(it) }}"
        }

    companion object {
        fun sha256(bytes: ByteArray): ByteArray = MessageDigest.getInstance("SHA-256").digest(bytes)

        /**
         * @param originPopulated the request carries a web origin (only apps
         *   holding CREDENTIAL_MANAGER_SET_ORIGIN, i.e. browsers, can set one)
         * @param privilegedOrigin `CallingAppInfo.getOrigin(allowlist)`:
         *   non-null only for a browser on the privileged allowlist
         * @param signers the caller's APK signing certificates (DER)
         * @return null when gitpass must not serve this caller at all
         */
        fun resolve(packageName: String, ownPackage: String, originPopulated: Boolean, privilegedOrigin: String?, signers: List<ByteArray>): CallerIdentity? {
            if (packageName.isBlank() || packageName == ownPackage) return null
            if (privilegedOrigin != null) {
                return if (privilegedOrigin.isBlank()) null else Browser(packageName, privilegedOrigin.trimEnd('/'))
            }
            // Claims to act for a web origin but is not an allowlisted browser.
            if (originPopulated) return null
            // Several signers cannot be matched against one Digital Asset Links fingerprint.
            if (signers.size != 1) return null
            return App(packageName, sha256(signers[0]))
        }
    }
}
