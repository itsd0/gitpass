package app.gitpass.android.session

import app.gitpass.core.FfiField
import app.gitpass.core.FfiSection
import app.gitpass.core.ItemInput

/** Only the basic login fields are editable in this milestone. Never log this object. */
class ItemEdit(
    val title: String,
    val username: String = "",
    val password: String = "",
    val url: String = "",
    val notes: String = "",
) {
    override fun toString(): String = "ItemEdit([redacted])"

    fun input(existing: ItemView?): ItemInput {
        require(title.isNotBlank()) { "Title is required" }
        require(existing == null || existing.category == "login") { "Only login editing is supported" }
        // Keep custom fields, metadata and TOTP. Canonical section copies must
        // agree with edited legacy fields or they could resurrect old values.
        val canonical = mapOf("username" to username, "password" to password)
        val sections = existing?.sections.orEmpty().map { section ->
            FfiSection(section.id, section.title, section.fields.map { field ->
                val value = if (section.id == "login" && canonical.containsKey(field.id)) canonical[field.id] else field.value
                FfiField(field.id, field.label, field.kind, value)
            })
        }
        return ItemInput(title.trim(), username, password, url, notes,
            existing?.totp, "login", existing?.tags.orEmpty(), existing?.favorite ?: false, sections)
    }
}
