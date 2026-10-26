import Foundation
import SwiftUI
import UIKit

/// App state for the gitpass iOS app.
///
/// The Rust `Vault` object is the only holder of unlocked state. When
/// the user locks, it is dropped, which drops the `Session` and its
/// account key from memory. The master password and Secret Key are
/// kept only for the duration of a sync (to open the staged remote
/// copy) and are otherwise held in these two properties while the
/// app is unlocked — the same trade-off the desktop app makes with
/// its in-memory session. They are never written to disk by this
/// class; only the Keychain copy (when Face ID is enabled) persists,
/// under a biometry access control.
@MainActor
final class VaultStore: ObservableObject {
    @Published var vault: Vault?
    @Published var vaults: [VaultInfo] = []
    @Published var items: [ItemSummary] = []
    @Published var selectedVaultId: String?
    @Published var errorMessage: String?
    @Published var isBusy = false
    @Published var lastSyncMessage: String?

    /// Unlock credentials for the current session (see class doc).
    var sessionPassword: String?
    var sessionSecretKey: String?

    static let appGroup = "group.dev.gitpass.ios"

    /// The vault lives in the shared app-group container so the
    /// AutoFill extension can open the same vault files (with its own
    /// password prompt). On the simulator, where the group container
    /// may not exist, fall back to Documents.
    var vaultRoot: URL {
        if let shared = FileManager.default.containerURL(
            forSecurityApplicationGroupIdentifier: Self.appGroup
        ) {
            return shared.appendingPathComponent("vault")
        }
        return FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("vault")
    }

    var vaultExists: Bool {
        FileManager.default.fileExists(atPath: vaultRoot.appendingPathComponent("format.json").path)
    }

    var isUnlocked: Bool { vault != nil }

    // MARK: - Unlock / lock

    func unlock(password: String, secretKey: String) throws {
        let opened = try openVault(root: vaultRoot.path, password: password, secretKey: secretKey)
        vault = opened
        sessionPassword = password
        sessionSecretKey = secretKey
        errorMessage = nil
        try refresh()
    }

    func createAccount(password: String) throws -> String {
        guard password.count >= 10 else {
            throw GitpassError.Generic("Use at least 10 characters for the master password."))
        }
        let secret = try createAccount(root: vaultRoot.path, password: password)
        try unlock(password: password, secretKey: secret)
        // Create a first vault so the item list is never empty on a
        // fresh account; the user can rename it on desktop later
        // (renaming vaults is not in this version, matching the
        // desktop's own "renaming not available" note).
        if vaults.isEmpty {
            _ = try vault?.createVault(name: "Personal", description: nil)
            try refresh()
        }
        return secret
    }

    func unlockWithBiometrics() async throws {
        guard KeychainStore.biometricsAvailable() else {
            throw KeychainError.notFound
        }
        _ = try await KeychainStore.evaluateBiometrics(reason: "Unlock gitpass")
        let credentials = try KeychainStore.shared.loadUnlockCredentials()
        try unlock(password: credentials.password, secretKey: credentials.secretKey)
    }

    func enableBiometricUnlock() throws {
        guard let password = sessionPassword, let secretKey = sessionSecretKey else {
            throw KeychainError.notFound
        }
        try KeychainStore.shared.saveUnlockCredentials(
            UnlockCredentials(password: password, secretKey: secretKey),
            requireBiometry: true
        )
    }

    func lock() {
        vault = nil
        vaults = []
        items = []
        selectedVaultId = nil
        sessionPassword = nil
        sessionSecretKey = nil
        clearClipboardIfOurs()
    }

    // MARK: - Vaults / items

    func refresh() throws {
        guard let vault else { return }
        vaults = try vault.listVaults()
        if selectedVaultId == nil { selectedVaultId = vaults.first?.id }
        if let selectedVaultId {
            items = try vault.listItems(vaultId: selectedVaultId)
                .filter { !$0.deleted && !$0.archived }
        }
    }

    func filteredItems(query: String, favoritesOnly: Bool) -> [ItemSummary] {
        var list = items
        if favoritesOnly { list = list.filter(\.favorite) }
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !q.isEmpty else { return list }
        // Simple substring search (desktop uses fuzzy ranking; the
        // iOS list uses containment, which is honest about what it
        // does and fast enough for a phone vault).
        return list.filter {
            $0.title.lowercased().contains(q)
            || $0.subtitle.lowercased().contains(q)
            || ($0.url ?? "").lowercased().contains(q)
            || $0.tags.contains { $0.lowercased().contains(q) }
        }
    }

    func itemDetail(vaultId: String, itemId: String) throws -> ItemDetail {
        guard let vault else { throw KeychainError.notFound }
        return try vault.getItem(vaultId: vaultId, itemId: itemId)
    }

    func saveItem(vaultId: String, itemId: String?, input: ItemInput) throws {
        guard let vault else { throw KeychainError.notFound }
        if let itemId {
            try vault.updateItem(vaultId: vaultId, itemId: itemId, input: input)
        } else {
            _ = try vault.createItem(vaultId: vaultId, input: input)
        }
        try refresh()
    }

    func deleteItem(vaultId: String, itemId: String) throws {
        guard let vault else { throw KeychainError.notFound }
        try vault.deleteItem(vaultId: vaultId, itemId: itemId)
        try refresh()
    }

    func toggleFavorite(vaultId: String, item: ItemSummary) throws {
        guard let vault else { throw KeychainError.notFound }
        try vault.setFavorite(vaultId: vaultId, itemId: item.id, favorite: !item.favorite)
        try refresh()
    }

    func totpCode(for secret: String) throws -> TotpCode {
        try totpCode(secret: secret)
    }

    // MARK: - Clipboard (auto-clearing, like desktop)

    private var clipboardToken: String?
    private var clipboardClearWork: DispatchWorkItem?

    /// Copies `text` and clears the clipboard after `seconds` — but
    /// only if the clipboard still holds exactly what we copied, so a
    /// newer copy by the user (or another app) is never wiped.
    func copyToClipboard(_ text: String, clearAfter seconds: TimeInterval = 30) {
        UIPasteboard.general.string = text
        clipboardToken = text
        clipboardClearWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self, UIPasteboard.general.string == self.clipboardToken else { return }
            UIPasteboard.general.string = ""
            self.clipboardToken = nil
        }
        clipboardClearWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: work)
    }

    private func clearClipboardIfOurs() {
        if let token = clipboardToken, UIPasteboard.general.string == token {
            UIPasteboard.general.string = ""
        }
        clipboardToken = nil
        clipboardClearWork?.cancel()
    }
}
