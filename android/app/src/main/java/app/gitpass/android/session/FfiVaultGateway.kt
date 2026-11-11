package app.gitpass.android.session

import app.gitpass.core.ItemInput
import app.gitpass.core.ItemDetail
import app.gitpass.core.ItemSummary
import app.gitpass.core.Vault
import app.gitpass.core.VaultInfo
import app.gitpass.core.createAccount as ffiCreateAccount
import app.gitpass.core.defaultUnlockOptions
import app.gitpass.core.openVaultWith as ffiOpenVaultWith
import java.io.File

/** UniFFI adapter. Crypto and the vault format stay in gitpass-core. */
class FfiVaultGateway : VaultGateway {
    override fun vaultExists(root: String): Boolean = File(root, "format.json").isFile

    override fun createAccount(root: String, password: String): String = ffiCreateAccount(root, password)

    /**
     * Side-effect-free unlock: no device registration, no trash purge and
     * no KDF upgrade, so unlocking never changes a synced vault file.
     */
    override fun open(root: String, password: String, secretKey: String): OpenVault {
        return FfiOpenVault(ffiOpenVaultWith(root, password, secretKey, defaultUnlockOptions()))
    }
}

private class FfiOpenVault(private val vault: Vault) : OpenVault {
    override fun listVaults(): List<VaultSummary> = vault.listVaults().map(::mapVault)

    override fun listItems(vaultId: String): List<ItemRow> = vault.listItems(vaultId).map(::mapRow)

    override fun getItem(vaultId: String, itemId: String): ItemView = mapDetail(vault.getItem(vaultId, itemId))

    override fun createVault(name: String): String = vault.createVault(name, null)

    override fun createItem(vaultId: String, input: ItemInput): String = vault.createItem(vaultId, input)
    override fun updateItem(vaultId: String, itemId: String, input: ItemInput) = vault.updateItem(vaultId, itemId, input)
    override fun deleteItem(vaultId: String, itemId: String) = vault.deleteItem(vaultId, itemId)
    override fun restoreItem(vaultId: String, itemId: String) = vault.restoreItem(vaultId, itemId)

    override fun close() {
        vault.close()
    }
}

private fun mapVault(info: VaultInfo) = VaultSummary(
    id = info.id,
    name = info.name,
    description = info.description,
    itemCount = info.itemCount.toLong(),
)

private fun mapRow(item: ItemSummary) = ItemRow(
    id = item.id,
    vaultId = item.vaultId,
    title = item.title,
    subtitle = item.subtitle,
    url = item.url,
    category = item.category,
    tags = item.tags,
    favorite = item.favorite,
    archived = item.archived,
    deleted = item.deleted,
    hasTotp = item.hasTotp,
)

private fun mapDetail(item: ItemDetail) = ItemView(
    id = item.id,
    vaultId = item.vaultId,
    title = item.title,
    username = item.username,
    password = item.password,
    url = item.url,
    notes = item.notes,
    totp = item.totp,
    category = item.category,
    tags = item.tags,
    favorite = item.favorite,
    archived = item.archived,
    deleted = item.deleted,
    updatedAt = item.updatedAt,
    sections = item.sections.map { section ->
        SectionView(
            id = section.id,
            title = section.title,
            fields = section.fields.map { field ->
                FieldView(
                    id = field.id,
                    label = field.label,
                    kind = field.kind,
                    value = field.value,
                )
            },
        )
    },
)
