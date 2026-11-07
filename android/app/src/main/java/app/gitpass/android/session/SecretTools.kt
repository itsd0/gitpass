package app.gitpass.android.session

/** No private algorithms: production always delegates to the existing core bridge. */
interface SecretTools {
    fun generate(length: Int): String
    fun totp(seed: String): LiveCode
}

class LiveCode(val code: String, val secondsRemaining: Long) {
    override fun toString() = "LiveCode([redacted])"
}

object NativeSecretTools : SecretTools {
    override fun generate(length: Int) = app.gitpass.core.generatePassword(length.toULong())
    override fun totp(seed: String): LiveCode {
        val result = app.gitpass.core.totpCode(seed)
        return LiveCode(result.code, result.secondsRemaining.toLong())
    }
}
