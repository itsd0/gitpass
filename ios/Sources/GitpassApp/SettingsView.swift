import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var store: VaultStore
    @Environment(\.dismiss) private var dismiss
    @State private var biometricEnabled = false
    @State private var message: String?
    @State private var showingSync = false

    var body: some View {
        NavigationStack {
            Form {
                Section(String(localized: "Security")) {
                    if KeychainStore.biometricsAvailable() {
                        Toggle(String(localized: "Unlock with Face ID"), isOn: $biometricEnabled)
                            .onChange(of: biometricEnabled) { _, enabled in
                                if enabled {
                                    do {
                                        try store.enableBiometricUnlock()
                                        message = String(localized: "Face ID unlock enabled. Your unlock credentials are stored in the Keychain under a biometry access control.")
                                    } catch {
                                        biometricEnabled = false
                                        message = error.localizedDescription
                                    }
                                } else {
                                    KeychainStore.shared.deleteUnlockCredentials()
                                    message = String(localized: "Face ID unlock disabled.")
                                }
                            }
                    } else {
                        Text(String(localized: "Face ID is not available on this device or simulator. Unlock with your master password and Secret Key."))
                            .font(.footnote)
                            .foregroundStyle(Color.gitpassSecondary)
                    }
                    Button(String(localized: "Lock now")) {
                        store.lock()
                        dismiss()
                    }
                }
                Section(String(localized: "Sync")) {
                    Button(String(localized: "GitHub sync")) { showingSync = true }
                    if let lastSyncMessage = store.lastSyncMessage {
                        Text(lastSyncMessage).font(.footnote).foregroundStyle(Color.gitpassSecondary)
                    }
                    Text(String(localized: "Sync uses the GitHub API over HTTPS with the desktop's field-level merge rules. Whole-vault creation and deletion still go through desktop git sync."))
                        .font(.footnote)
                        .foregroundStyle(Color.gitpassSecondary)
                }
                Section(String(localized: "About")) {
                    Text(String(localized: "gitpass for iOS — a companion to the gitpass desktop app. All cryptography runs in gitpass-core (Rust); Swift never implements it."))
                        .font(.footnote)
                        .foregroundStyle(Color.gitpassSecondary)
                    Text(String(localized: "Vault format v5. If you lose your master password and Secret Key, there is no recovery."))
                        .font(.footnote)
                        .foregroundStyle(Color.gitpassSecondary)
                }
                if let message {
                    Section { Text(message).font(.footnote) }
                }
            }
            .gitpassBackground()
            .navigationTitle(String(localized: "Settings"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button(String(localized: "Done")) { dismiss() }
                }
            }
            .sheet(isPresented: $showingSync) {
                SyncSetupView()
                    .environmentObject(store)
            }
        }
    }
}

struct SyncSetupView: View {
    @EnvironmentObject private var store: VaultStore
    @Environment(\.dismiss) private var dismiss
    @State private var clientId = UserDefaults.standard.string(forKey: "gitpass.github.clientId") ?? ""
    @State private var repo = UserDefaults.standard.string(forKey: "gitpass.github.repo") ?? "itsd0/gitpass-vault"
    @State private var pat = ""
    @State private var authMode = 0 // 0: Fine-grained PAT, 1: OAuth Device Flow
    @State private var deviceCode: DeviceCodeResponse?
    @State private var status = ""
    @State private var isWorking = false
    @State private var tokenPresent = KeychainStore.shared.loadGitHubToken() != nil

    private let sync = GitHubSyncService()

    var body: some View {
        NavigationStack {
            Form {
                if tokenPresent {
                    // Connected: show what was saved instead of an
                    // empty form, plus a way to disconnect.
                    Section(String(localized: "GitHub")) {
                        HStack {
                            Image(systemName: "checkmark.circle.fill")
                                .foregroundStyle(Color.gitpassPrimary)
                            Text(String(localized: "Connected"))
                                .font(.headline)
                                .foregroundStyle(Color.gitpassPrimary)
                        }
                        if !clientId.isEmpty {
                            LabeledContent(String(localized: "Client ID")) {
                                Text(clientId)
                                    .font(.footnote.monospaced())
                                    .foregroundStyle(Color.gitpassSecondary)
                                    .textSelection(.enabled)
                            }
                        } else {
                            Text(String(localized: "Connected using a Personal Access Token"))
                                .font(.footnote)
                                .foregroundStyle(Color.gitpassSecondary)
                        }
                        TextField(String(localized: "Vault repository (owner/repo)"), text: $repo)
                            .autocapitalization(.none)
                            .disableAutocorrection(true)
                            .onSubmit { saveSettings() }
                        Button(role: .destructive) {
                            KeychainStore.shared.deleteGitHubToken()
                            tokenPresent = false
                            status = String(localized: "Disconnected.")
                        } label: {
                            Text(String(localized: "Disconnect GitHub"))
                                .foregroundStyle(Color.gitpassPrimary)
                        }
                    }
                } else {
                    Section {
                        Picker(String(localized: "Method"), selection: $authMode) {
                            Text(String(localized: "Fine-grained PAT")).tag(0)
                            Text(String(localized: "OAuth App")).tag(1)
                        }
                        .pickerStyle(.segmented)
                    }

                    if authMode == 0 {
                        Section(String(localized: "Fine-grained Personal Access Token")) {
                            TextField(String(localized: "Vault repository (owner/repo)"), text: $repo)
                                .autocapitalization(.none)
                                .disableAutocorrection(true)
                            SecureField(String(localized: "Token (github_pat_...)"), text: $pat)
                                .autocapitalization(.none)
                                .disableAutocorrection(true)
                            Button(String(localized: "Connect with Personal Access Token")) {
                                savePat()
                            }
                            .disabled(pat.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

                            Text(String(localized: "Recommended: create a fine-grained Personal Access Token on GitHub (github.com/settings/tokens?type=beta) scoped ONLY to your vault repository (Repository access: Only select repositories -> choose your vault repo; Permissions: Contents: Read and write). Unlike OAuth Apps with the classic 'repo' scope, a fine-grained PAT cannot access any of your other repositories."))
                                .font(.footnote)
                                .foregroundStyle(Color.gitpassSecondary)
                        }
                    } else {
                        Section(String(localized: "GitHub OAuth App")) {
                            TextField(String(localized: "OAuth App client ID"), text: $clientId)
                                .autocapitalization(.none)
                                .disableAutocorrection(true)
                            TextField(String(localized: "Vault repository (owner/repo)"), text: $repo)
                                .autocapitalization(.none)
                                .disableAutocorrection(true)
                            Text(String(localized: "The client ID is an OAuth App identifier, not a secret. Create one at github.com/settings/developers (do not enable a client secret flow for a public device app) and paste its client ID here. The token it issues is stored in the Keychain."))
                                .font(.footnote)
                                .foregroundStyle(Color.gitpassSecondary)
                        }
                        Section {
                            if let deviceCode {
                                VStack(alignment: .leading, spacing: 8) {
                                    Text(String(localized: "On another device, open:"))
                                        .font(.footnote)
                                    Text(deviceCode.verificationUri).font(.headline)
                                    Text(String(localized: "Enter this code:"))
                                        .font(.footnote)
                                    Text(deviceCode.userCode).font(.largeTitle.monospaced())
                                    Button(String(localized: "Copy code")) {
                                        store.copyToClipboard(deviceCode.userCode)
                                    }
                                }
                                Button(String(localized: "I authorized it — fetch token")) {
                                    Task { await fetchToken() }
                                }
                                .disabled(isWorking)
                            } else {
                                Button(String(localized: "Connect with device code")) {
                                    Task { await startDeviceFlow() }
                                }
                                .disabled(clientId.isEmpty || isWorking)
                            }
                        }
                    }
                }
                Section(String(localized: "Sync")) {
                    Button(String(localized: "Sync now")) {
                        Task { await runSync() }
                    }
                    .disabled(!tokenPresent || isWorking)
                    Button(String(localized: "Pull remote vault first (new device)")) {
                        Task { await runFirstPull() }
                    }
                    .disabled(!tokenPresent || isWorking)
                    if !status.isEmpty {
                        Text(status).font(.footnote).foregroundStyle(Color.gitpassSecondary)
                    }
                }
            }
            .gitpassBackground()
            .navigationTitle(String(localized: "GitHub Sync"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button(String(localized: "Done")) { dismiss() }
                }
            }
        }
    }

    private func savePat() {
        let trimmedPat = pat.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedPat.isEmpty else { return }
        saveSettings()
        do {
            try KeychainStore.shared.saveGitHubToken(trimmedPat)
            tokenPresent = true
            pat = ""
            status = String(localized: "Connected with Personal Access Token.")
        } catch {
            status = error.localizedDescription
        }
    }

    /// Client ID and repo are not secrets, so UserDefaults is fine; the
    /// token itself only ever lives in the Keychain.
    private func saveSettings() {
        clientId = clientId.trimmingCharacters(in: .whitespacesAndNewlines)
        UserDefaults.standard.set(clientId, forKey: "gitpass.github.clientId")
        UserDefaults.standard.set(repo, forKey: "gitpass.github.repo")
    }

    private func startDeviceFlow() async {
        saveSettings()
        isWorking = true
        defer { isWorking = false }
        do {
            deviceCode = try await sync.requestDeviceCode(clientId: clientId)
            status = String(localized: "Enter the code on GitHub, then tap the fetch button.")
        } catch {
            status = error.localizedDescription
        }
    }

    private func fetchToken() async {
        guard let deviceCode else { return }
        isWorking = true
        defer { isWorking = false }
        do {
            let token = try await sync.pollForToken(
                clientId: clientId, deviceCode: deviceCode.deviceCode, initialInterval: deviceCode.interval
            )
            try KeychainStore.shared.saveGitHubToken(token)
            saveSettings()
            tokenPresent = true
            self.deviceCode = nil
            status = String(localized: "GitHub connected.")
        } catch {
            status = error.localizedDescription
        }
    }

    private func runSync() async {
        saveSettings()
        guard let vault = store.vault,
              let password = store.sessionPassword,
              let secretKey = store.sessionSecretKey
        else {
            status = String(localized: "Unlock the vault first, then sync.")
            return
        }
        isWorking = true
        defer { isWorking = false }
        do {
            let report = try await sync.sync(
                vault: vault, localRoot: store.vaultRoot,
                password: password, secretKey: secretKey, repo: repo
            )
            try? store.refresh()
            store.lastSyncMessage = report.message
            status = report.message
        } catch {
            status = error.localizedDescription
        }
    }

    private func runFirstPull() async {
        saveSettings()
        isWorking = true
        defer { isWorking = false }
        do {
            let report = try await sync.firstPull(localRoot: store.vaultRoot, repo: repo)
            status = report.message
        } catch {
            status = error.localizedDescription
        }
    }
}
