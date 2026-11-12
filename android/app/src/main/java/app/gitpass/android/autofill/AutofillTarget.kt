package app.gitpass.android.autofill

/**
 * Who an autofill request is for (#475). Exactly one identity is used:
 * the web origin when a trusted browser reports the page's domain,
 * otherwise the requesting app's package (reported by the OS, so an app
 * cannot pretend to be another app). A web domain reported by a
 * non-browser app is ignored: any app could put a look-alike page in a
 * WebView, so only its own `android://<package>` logins are offered.
 */
data class AutofillTarget(val webOrigin: String?, val androidPackage: String?) {
    /** Short label for the UI (domain or package); never secret. */
    val label: String get() = webOrigin?.substringAfter("://") ?: androidPackage.orEmpty()

    /** URL saved on a new item for this target. */
    val itemUrl: String get() = webOrigin ?: "android://$androidPackage"

    companion object {
        const val OWN_PACKAGE = "app.gitpass.android"

        /** Browsers whose reported web domain is trusted (also listed for compatibility mode). */
        val TRUSTED_BROWSERS: Set<String> = setOf(
            "com.android.chrome", "com.chrome.beta", "com.chrome.dev", "com.chrome.canary", "org.chromium.chrome",
            "org.mozilla.firefox", "org.mozilla.firefox_beta", "org.mozilla.fenix", "org.mozilla.focus",
            "com.brave.browser", "com.microsoft.emmx", "com.opera.browser", "com.sec.android.app.sbrowser",
            "com.duckduckgo.mobile.android", "com.vivaldi.browser", "com.mi.globalbrowser", "com.android.browser",
            "com.kiwibrowser.browser",
        )

        private val DOMAIN = Regex("^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*$")

        /** Null when gitpass must not offer anything (its own UI, or no usable identity). */
        fun of(packageName: String?, webDomain: String?, webScheme: String?): AutofillTarget? {
            val pkg = packageName?.trim().orEmpty()
            if (pkg.isEmpty() || pkg == OWN_PACKAGE) return null
            val domain = webDomain?.trim()?.trimEnd('.')?.lowercase()
            if (!domain.isNullOrEmpty() && pkg in TRUSTED_BROWSERS) {
                if (!DOMAIN.matches(domain)) return null
                // Only https (and http, which the core rule refuses except on loopback).
                val scheme = when (webScheme?.lowercase()) { null, "", "https" -> "https"; "http" -> "http"; else -> return null }
                return AutofillTarget("$scheme://$domain", null)
            }
            if (pkg in TRUSTED_BROWSERS) return null // browser chrome (URL bar etc.), not a page
            return AutofillTarget(null, pkg)
        }
    }
}
