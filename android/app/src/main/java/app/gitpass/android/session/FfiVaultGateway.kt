package app.gitpass.android.session

import app.gitpass.core.ItemInput
import app.gitpass.core.ItemDetail
import app.gitpass.core.ItemSummary
import app.gitpass.core.Vault
import app.gitpass.core.VaultInfo
import app.gitpass.core.GitHubSyncTarget
import app.gitpass.core.GitpassException
import app.gitpass.core.SyncCancellation
import app.gitpass.android.sync.GitHubSyncTransport
import app.gitpass.core.createAccount as ffiCreateAccount
import app.gitpass.core.defaultUnlockOptions
import app.gitpass.core.openVaultWith as ffiOpenVaultWith
import app.gitpass.core.deriveQuickUnlockKey as ffiDeriveQuickUnlockKey
import app.gitpass.core.openVaultWithQuickUnlockKey as ffiOpenVaultWithQuickUnlockKey
import app.gitpass.core.quickUnlockStaleMessage
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

    override fun deriveUnlockKey(root: String, password: String, secretKey: String): ByteArray =
        ffiDeriveQuickUnlockKey(root, password, secretKey)

    /** Same side-effect-free options as [open]; the native side wipes its copy of [key]. */
    override fun openWithUnlockKey(root: String, key: ByteArray): OpenVault {
        try {
            return FfiOpenVault(ffiOpenVaultWithQuickUnlockKey(root, key, defaultUnlockOptions()))
        } catch (error: GitpassException.Generic) {
            if (error.v1 == quickUnlockStaleMessage()) throw QuickUnlockStaleException()
            throw error
        } finally {
            key.fill(0)
        }
    }
}

private class FfiOpenVault(private val vault: Vault) : OpenVault {
    override fun listVaults(): List<VaultSummary> = vault.listVaults().map(::mapVault)

    override fun listItems(vaultId: String): List<ItemRow> = vault.listItems(vaultId).map(::mapRow)

    override fun getItem(vaultId: String, itemId: String): ItemView = mapDetail(vault.getItem(vaultId, itemId))

    override fun createVault(name: String): String = vault.createVault(name, null)

    override fun autofillLogins(target: app.gitpass.android.autofill.AutofillTarget): List<AutofillCredential> =
        vault.autofillLogins(app.gitpass.core.AutofillQuery(target.webOrigin, target.androidPackage)).map {
            AutofillCredential(it.vaultId, it.itemId, it.title, it.username, it.password)
        }

    override fun createItem(vaultId: String, input: ItemInput): String = vault.createItem(vaultId, input)
    override fun updateItem(vaultId: String, itemId: String, input: ItemInput) = vault.updateItem(vaultId, itemId, input)
    override fun deleteItem(vaultId: String, itemId: String) = vault.deleteItem(vaultId, itemId)
    override fun restoreItem(vaultId: String, itemId: String) = vault.restoreItem(vaultId, itemId)

    /**
     * The merge, safety checks and GitHub protocol run in gitpass-core; this
     * only supplies the HTTPS round trip. Cancelling stops the native engine
     * before its next request and disconnects the in-flight one.
     */
    override fun syncGitHub(target: SyncTarget, token: String, cancel: SyncCancel): SyncOutcome {
        val native = SyncCancellation()
        val transport = GitHubSyncTransport(token, cancel::isCancelled)
        cancel.onCancel { runCatching { native.cancel() }; transport.close() }
        try {
            val report = vault.syncGithub(GitHubSyncTarget(target.owner, target.repository, target.branch), token, transport, native)
            return SyncOutcome(
                pushed = report.pushed,
                localChanged = report.localChanged,
                conflicts = report.conflicts,
                mergedItems = report.mergedItems.toInt(),
                localOnlyItems = report.localOnlyItems.toInt(),
            )
        } catch (error: GitpassException.Generic) {
            // Core sync errors are fixed text without token, URL, path or item data.
            throw SyncFailedException(error.v1)
        } catch (error: GitpassException) {
            throw SyncFailedException(GENERIC_SYNC_FAILURE)
        } finally {
            transport.close()
            runCatching { native.close() }
        }
    }

    override fun close() {
        vault.close()
    }

    private companion object {
        const val GENERIC_SYNC_FAILURE = "Sync failed; try again"
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
