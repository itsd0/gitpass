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
                    SyncStatusLine(sync: store.sync)
                    Text(String(localized: "Two-way sync with a GitHub repository you choose, on the same engine as the desktop app: field-level merge, deletes in both directions, conflicts kept fail-safe and listed."))
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

/// One-line sync status (last success, syncing, failure, conflicts).
struct SyncStatusLine: View {
    @ObservedObject var sync: SyncCoordinator

    var body: some View {
        Text(Self.text(phase: sync.phase, lastSuccess: sync.lastSuccess, conflicts: sync.lastConflicts.count))
            .font(.footnote)
            .foregroundStyle(Color.gitpassSecondary)
            .accessibilityIdentifier("sync-status")
    }

    static func text(phase: SyncCoordinator.Phase, lastSuccess: Date?, conflicts: Int) -> String {
        switch phase {
        case .notConfigured:
            return String(localized: "Sync is not set up.")
        case .syncing:
            return String(localized: "Syncing…")
        case .failed(let message):
            return message
        case .idle:
            guard let lastSuccess else { return String(localized: "Not synced yet.") }
            let when = lastSuccess.formatted(date: .abbreviated, time: .shortened)
            if conflicts > 0 {
                return String(localized: "Last synced \(when) · \(conflicts) conflict(s), local kept")
            }
            return String(localized: "Last synced \(when)")
        }
    }
}

/// GitHub sync settings, status and first pull. The repository, branch and
/// PAT are always entered by the user; there is no default repository and
/// no app-registration flow. The token field is never prefilled.
struct SyncSetupView: View {
    @EnvironmentObject private var store: VaultStore
    @Environment(\.dismiss) private var dismiss
    @State private var owner = ""
    @State private var repo = ""
    @State private var branch = ""
    @State private var token = ""
    @State private var tokenSaved = false
    @State private var configured = false
    @State private var status = ""
    @State private var isWorking = false
    // First pull only (no vault on this device yet).
    @State private var pullPassword = ""
    @State private var pullSecretKey = ""
    @State private var pullCancellation: SyncCancellation?

    var body: some View {
        NavigationStack {
            Form {
                Section(String(localized: "Repository")) {
                    TextField(String(localized: "Owner"), text: $owner)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    TextField(String(localized: "Repository"), text: $repo)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    TextField(String(localized: "Branch"), text: $branch)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    SecureField(
                        tokenSaved
                            ? String(localized: "Token saved (leave empty to keep it)")
                            : String(localized: "Fine-grained token (github_pat_…)"),
                        text: $token
                    )
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    Button(String(localized: "Save")) { saveSettings() }
                        .disabled(isWorking)
                    if configured {
                        Button(role: .destructive) {
                            store.sync.settings.clear()
                            store.sync.settingsChanged()
                            load()
                            status = String(localized: "Sync settings and token removed from this device.")
                        } label: {
                            Text(String(localized: "Disconnect GitHub"))
                        }
                        .disabled(isWorking)
                    }
                    Text(String(localized: "Create a fine-grained personal access token on GitHub limited to only your vault repository with Contents: Read and write. It is stored in the Keychain on this device only and is never shown again."))
                        .font(.footnote)
                        .foregroundStyle(Color.gitpassSecondary)
                }
                if store.isUnlocked {
                    Section(String(localized: "Sync")) {
                        SyncStatusLine(sync: store.sync)
                        SyncNowButton(sync: store.sync, enabled: configured && !isWorking)
                    }
                    SyncConflictsSection(sync: store.sync)
                } else if !store.vaultExists {
                    Section(String(localized: "First pull (new device)")) {
                        SecureField(String(localized: "Master password"), text: $pullPassword)
                        TextField(String(localized: "Secret Key (A3-…)"), text: $pullSecretKey)
                            .textInputAutocapitalization(.characters)
                            .autocorrectionDisabled()
                        if pullCancellation == nil {
                            Button(String(localized: "Pull vault and unlock")) {
                                Task { await runFirstPull() }
                            }
                            .disabled(!configured || pullPassword.isEmpty || pullSecretKey.isEmpty)
                        } else {
                            Button(String(localized: "Cancel")) { pullCancellation?.cancel() }
                        }
                        Text(String(localized: "Downloads the vault from the repository above, checks it with your master password and Secret Key, and only then saves it on this device. Nothing is pushed."))
                            .font(.footnote)
                            .foregroundStyle(Color.gitpassSecondary)
                    }
                }
                if !status.isEmpty {
                    Section { Text(status).font(.footnote).foregroundStyle(Color.gitpassSecondary) }
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
            .onAppear(perform: load)
            .onDisappear {
                pullCancellation?.cancel()
                token = ""
                pullPassword = ""
                pullSecretKey = ""
            }
        }
    }

    private func load() {
        let settings = store.sync.settings
        let target = settings.target
        owner = target?.owner ?? ""
        repo = target?.repo ?? ""
        branch = target?.branch ?? ""
        token = ""
        tokenSaved = settings.hasToken
        configured = settings.isConfigured
    }

    private func saveSettings() {
        guard let target = SyncTarget.validated(owner: owner, repo: repo, branch: branch) else {
            status = SyncSettingsError.invalidTarget.errorDescription ?? ""
            return
        }
        do {
            try store.sync.settings.save(target: target, token: token)
            store.sync.settingsChanged()
            load()
            status = String(localized: "Saved.")
        } catch {
            token = ""
            status = syncErrorMessage(error)
        }
    }

    private func runFirstPull() async {
        let settings = store.sync.settings
        guard let target = settings.target, let savedToken = settings.token() else {
            status = String(localized: "Save the repository and token first.")
            return
        }
        let password = pullPassword
        let secretKey = pullSecretKey.trimmingCharacters(in: .whitespacesAndNewlines)
        let root = store.vaultRoot
        let cancellation = SyncCancellation()
        pullCancellation = cancellation
        status = String(localized: "Pulling…")
        let result: Result<Void, Error> = await withCheckedContinuation { cont in
            DispatchQueue.global(qos: .userInitiated).async {
                cont.resume(returning: Result {
                    try GitHubSyncService().firstPull(
                        localRoot: root, target: target, token: savedToken,
                        password: password, secretKey: secretKey, cancellation: cancellation
                    )
                })
            }
        }
        pullCancellation = nil
        switch result {
        case .success:
            do {
                try store.unlock(password: password, secretKey: secretKey)
                pullPassword = ""
                pullSecretKey = ""
                dismiss()
            } catch {
                status = error.localizedDescription
            }
        case .failure(let error):
            status = syncErrorMessage(error)
        }
    }
}

struct SyncNowButton: View {
    @ObservedObject var sync: SyncCoordinator
    let enabled: Bool

    var body: some View {
        Button(String(localized: "Sync now")) {
            Task { await sync.syncNow() }
        }
        .disabled(!enabled || sync.phase == .syncing)
        .accessibilityIdentifier("sync-now")
    }
}

/// Conflicts from the last sync (already resolved fail-safe: local kept).
/// One line each with ids and field names only, never item data.
struct SyncConflictsSection: View {
    @ObservedObject var sync: SyncCoordinator

    var body: some View {
        if !sync.lastConflicts.isEmpty {
            Section(String(localized: "Conflicts in the last sync")) {
                ForEach(sync.lastConflicts, id: \.self) { line in
                    Text(line).font(.footnote.monospaced())
                }
                Text(String(localized: "Each conflict was resolved without losing data (the local value was kept); review these items."))
                    .font(.footnote)
                    .foregroundStyle(Color.gitpassSecondary)
            }
        }
    }
}
