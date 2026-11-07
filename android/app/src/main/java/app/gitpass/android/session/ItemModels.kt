package app.gitpass.android.session

import java.util.Locale

/** In-account vault, mapped from the UniFFI record. No secrets. */
data class VaultSummary(
    val id: String,
    val name: String,
    val description: String?,
    val itemCount: Long,
)

/**
 * List row. Passwords, notes, and TOTP secrets are intentionally absent:
 * the core summary does not include them, and search must not need them.
 */
data class ItemRow(
    val id: String,
    val vaultId: String,
    val title: String,
    val subtitle: String,
    val url: String?,
    val category: String,
    val tags: List<String>,
    val favorite: Boolean,
    val archived: Boolean,
    val deleted: Boolean,
    val hasTotp: Boolean,
)

data class FieldView(
    val id: String,
    val label: String,
    val kind: String,
    val value: String?,
)

data class SectionView(
    val id: String,
    val title: String?,
    val fields: List<FieldView>,
)

/** Full item. Every section field from core is kept, including kinds this UI does not edit. */
data class ItemView(
    val id: String,
    val vaultId: String,
    val title: String,
    val username: String?,
    val password: String?,
    val url: String?,
    val notes: String?,
    val totp: String?,
    val category: String,
    val tags: List<String>,
    val favorite: Boolean,
    val archived: Boolean,
    val deleted: Boolean,
    val updatedAt: String,
    val sections: List<SectionView>,
)

data class DetailRow(
    val key: String,
    val label: String,
    val value: String,
    val secret: Boolean,
    val sectionTitle: String?,
)

data class VaultSnapshot(
    val unlocked: Boolean,
    val vaults: List<VaultSummary>,
    val selectedVaultId: String?,
    val items: List<ItemRow>,
    val query: String,
    val detail: ItemView?,
    /** Lock generation. Changes on every lock, so UI state keyed on it starts fresh. */
    val epoch: Long = 0,
)

object ItemSearch {
    /** Default list hides deleted and archived rows, then applies a case-insensitive substring. */
    fun visible(items: List<ItemRow>, query: String): List<ItemRow> {
        val q = query.trim().lowercase(Locale.ROOT)
        return items
            .filter { !it.deleted && !it.archived }
            .filter { row ->
                if (q.isEmpty()) {
                    true
                } else {
                    row.title.lowercase(Locale.ROOT).contains(q) ||
                        row.subtitle.lowercase(Locale.ROOT).contains(q) ||
                        (row.url?.lowercase(Locale.ROOT)?.contains(q) == true) ||
                        row.category.lowercase(Locale.ROOT).contains(q) ||
                        row.tags.any { it.lowercase(Locale.ROOT).contains(q) }
                }
            }
    }
}

fun FieldView.isSecret(): Boolean = kind == "concealed" || kind == "totp"

/**
 * Rows for the detail screen. Section fields are all shown. Legacy login
 * fields are added only when that id is not already present in a section,
 * so a desktop section is not dropped and a login field is not duplicated.
 */
fun detailRows(item: ItemView): List<DetailRow> {
    val sectionIds = item.sections.flatMap { it.fields }.map { it.id }.toSet()
    val rows = mutableListOf<DetailRow>()
    fun legacy(id: String, label: String, value: String?, secret: Boolean) {
        if (value.isNullOrEmpty() || id in sectionIds) return
        rows += DetailRow(id, label, value, secret, null)
    }
    legacy("username", "username", item.username, false)
    legacy("password", "password", item.password, true)
    legacy("url", "url", item.url, false)
    legacy("notes", "notes", item.notes, false)
    legacy("totp", "totp", item.totp, true)
    for (section in item.sections) {
        for (field in section.fields) {
            rows += DetailRow(
                key = "${section.id}:${field.id}",
                label = field.label.ifBlank { field.id },
                value = field.value.orEmpty(),
                secret = field.isSecret(),
                sectionTitle = section.title,
            )
        }
    }
    return rows
}
