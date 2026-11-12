package app.gitpass.android.passkey

import app.gitpass.core.PasskeyCaller

/** gitpass-core's passkey rules, behind an interface so the flow can be tested without the native library. */
interface PasskeyRules {
    /** (rpId, rpName, userName) from a WebAuthn options JSON; throws when malformed. */
    fun requestInfo(requestJson: String, create: Boolean): app.gitpass.core.PasskeyRequestInfo
    /** The domain whose Digital Asset Links an app must be listed in; throws when no app may use [rpId]. */
    fun appRpHost(rpId: String): String
    /** The rpId [caller] may use, or throws. */
    fun verifyCaller(caller: PasskeyCaller, rpId: String?): String
}

object FfiPasskeyRules : PasskeyRules {
    override fun requestInfo(requestJson: String, create: Boolean) = app.gitpass.core.passkeyRequestInfo(requestJson, create)
    override fun appRpHost(rpId: String) = app.gitpass.core.passkeyAppRpHost(rpId)
    override fun verifyCaller(caller: PasskeyCaller, rpId: String?) = app.gitpass.core.passkeyVerifyCaller(caller, rpId)
}

class PasskeyRefusedException(message: String) : Exception(message)

/**
 * Turns an Android caller + WebAuthn request into a core [PasskeyCaller]
 * whose rpId gitpass-core has accepted (#477). For apps the Digital Asset
 * Links document is fetched only for a host core validated first.
 */
class PasskeyBroker(private val assetLinks: AssetLinksSource, private val rules: PasskeyRules = FfiPasskeyRules) {
    data class Checked(val caller: PasskeyCaller, val rpId: String, val rpName: String?, val userName: String?)

    /** Never on the main thread (may fetch). Throws when the caller may not use the request's rpId. */
    fun check(identity: CallerIdentity, requestJson: String, create: Boolean): Checked {
        val info = rules.requestInfo(requestJson, create)
        val caller = when (identity) {
            is CallerIdentity.Browser -> PasskeyCaller.Browser(identity.origin)
            is CallerIdentity.App -> {
                val rp = info.rpId ?: throw PasskeyRefusedException("An app request must name its rpId")
                PasskeyCaller.App(identity.packageName, identity.certSha256, assetLinks.fetch(rules.appRpHost(rp)))
            }
        }
        val rpId = rules.verifyCaller(caller, info.rpId)
        return Checked(caller, rpId, info.rpName, info.userName)
    }
}
