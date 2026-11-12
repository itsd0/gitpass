package app.gitpass.android.autofill

/**
 * Framework-free view of one autofillable node, so field detection is plain
 * JVM-testable. [id] is the platform `AutofillId` in production.
 */
data class FieldNode<T>(
    val id: T?,
    /** `View.getAutofillHints()`, lower-cased by the parser. */
    val hints: List<String> = emptyList(),
    /** `View.getInputType()`; 0 when unknown (e.g. web content). */
    val inputType: Int = 0,
    /** Resource entry name of the view id (e.g. `login_email`). */
    val idEntry: String? = null,
    /** Visible hint / placeholder text. */
    val hint: String? = null,
    /** HTML attributes for web content (`type`, `name`, `id`, `autocomplete`), lower-cased keys. */
    val html: Map<String, String> = emptyMap(),
    /** True for `AUTOFILL_TYPE_TEXT` nodes; other types are never filled. */
    val isText: Boolean = true,
    val isVisible: Boolean = true,
    /** Value the user typed (only read for save requests). */
    val text: String? = null,
)

/** Detected login fields, in traversal order. */
data class ParsedFields<T>(val usernames: List<FieldNode<T>>, val passwords: List<FieldNode<T>>) {
    val isEmpty: Boolean get() = usernames.isEmpty() && passwords.isEmpty()
    val allIds: List<T> get() = (usernames + passwords).mapNotNull { it.id }
}

/**
 * Finds username and password fields (#475): explicit autofill hints and
 * HTML `autocomplete` first, then input types, then id/name/hint text. A
 * text field right before the first password field is taken as the
 * username when nothing better was found. One-time-code, search and
 * captcha fields are never treated as credentials.
 */
object FieldParser {
    private const val CLASS_MASK = 0x0000000f
    private const val VARIATION_MASK = 0x00000ff0
    private const val CLASS_TEXT = 0x1
    private const val CLASS_NUMBER = 0x2
    private const val TEXT_PASSWORD = 0x80
    private const val TEXT_VISIBLE_PASSWORD = 0x90
    private const val TEXT_WEB_PASSWORD = 0xe0
    private const val TEXT_EMAIL = 0x20
    private const val TEXT_WEB_EMAIL = 0xd0
    private const val NUMBER_PASSWORD = 0x10

    private val PASSWORD_HINTS = setOf("password", "current-password", "new-password", "newpassword", "currentpassword")
    private val USERNAME_HINTS = setOf("username", "emailaddress", "email", "newusername", "phone", "phonenumber", "tel")
    private val PASSWORD_WORDS = listOf("password", "passwd", "passcode", "pwd", "pass", "รหัสผ่าน", "စကားဝှက်")
    private val USERNAME_WORDS = listOf("username", "user", "login", "email", "e-mail", "account", "userid", "identifier", "ชื่อผู้ใช้", "อีเมล")
    private val EXCLUDED_WORDS = listOf("otp", "one-time", "onetime", "2fa", "totp", "verification", "captcha", "search", "promo", "coupon", "code")

    fun <T> parse(nodes: List<FieldNode<T>>): ParsedFields<T> {
        val candidates = nodes.filter { it.isText && it.isVisible && it.id != null }
        val passwords = candidates.filter(::isPassword)
        val usernames = candidates.filter { it !in passwords && isUsername(it) }.toMutableList()
        if (usernames.isEmpty() && passwords.isNotEmpty()) {
            val first = candidates.indexOf(passwords.first())
            candidates.subList(0, first).lastOrNull { it !in passwords && !isExcluded(it) && isPlainText(it) }?.let { usernames += it }
        }
        return ParsedFields(usernames, passwords)
    }

    fun <T> isPassword(n: FieldNode<T>): Boolean {
        val hints = n.hints.map { it.lowercase() }
        if (hints.any { it in PASSWORD_HINTS }) return true
        val auto = n.html["autocomplete"]?.lowercase().orEmpty()
        if (auto.contains("current-password") || auto.contains("new-password")) return true
        if (n.html["type"]?.lowercase() == "password") return true
        if (hints.any { it in USERNAME_HINTS } || isExcluded(n)) return false
        val cls = n.inputType and CLASS_MASK
        val variation = n.inputType and VARIATION_MASK
        if (cls == CLASS_TEXT && variation in setOf(TEXT_PASSWORD, TEXT_VISIBLE_PASSWORD, TEXT_WEB_PASSWORD)) return true
        if (cls == CLASS_NUMBER && variation == NUMBER_PASSWORD) return true
        return words(n).any { w -> PASSWORD_WORDS.any { w.contains(it) } } && n.html["type"]?.lowercase() !in setOf("email", "search", "hidden", "checkbox")
    }

    fun <T> isUsername(n: FieldNode<T>): Boolean {
        if (isExcluded(n)) return false
        val hints = n.hints.map { it.lowercase() }
        if (hints.any { it in USERNAME_HINTS }) return true
        val auto = n.html["autocomplete"]?.lowercase().orEmpty()
        if (auto.split(' ').any { it == "username" || it == "email" }) return true
        if (n.html["type"]?.lowercase() == "email") return true
        val cls = n.inputType and CLASS_MASK
        val variation = n.inputType and VARIATION_MASK
        if (cls == CLASS_TEXT && variation in setOf(TEXT_EMAIL, TEXT_WEB_EMAIL)) return true
        return isPlainText(n) && words(n).any { w -> USERNAME_WORDS.any { w.contains(it) } }
    }

    private fun <T> isPlainText(n: FieldNode<T>): Boolean {
        val type = n.html["type"]?.lowercase()
        if (type != null && type !in setOf("text", "email", "tel")) return false
        val cls = n.inputType and CLASS_MASK
        return n.inputType == 0 || cls == CLASS_TEXT
    }

    private fun <T> isExcluded(n: FieldNode<T>): Boolean {
        val hints = n.hints.map { it.lowercase() }
        if (hints.any { it.contains("otp") || it.contains("smsotp") || it.contains("2fa") }) return true
        if (n.html["autocomplete"]?.lowercase()?.contains("one-time-code") == true) return true
        if (n.html["type"]?.lowercase() in setOf("search", "hidden", "checkbox", "radio", "submit", "button")) return true
        return words(n).any { w -> EXCLUDED_WORDS.any { w.contains(it) } && PASSWORD_WORDS.none { w == it } }
    }

    private fun <T> words(n: FieldNode<T>): List<String> =
        listOfNotNull(n.idEntry, n.hint, n.html["name"], n.html["id"], n.html["placeholder"], n.html["aria-label"])
            .map { it.lowercase() }
}
