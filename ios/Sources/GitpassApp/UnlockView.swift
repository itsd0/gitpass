import SwiftUI

struct UnlockView: View {
    @EnvironmentObject private var store: VaultStore
    @State private var password = ""
    @State private var secretKey = ""
    @State private var showSecretKey = false
    @State private var isCreating = false
    @State private var errorMessage: String?
    @State private var createdSecret: String?
    @State private var showingSecretAlert = false
    @State private var showingSync = false

    private var canUnlock: Bool {
        !password.isEmpty && !secretKey.isEmpty
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 24) {
                    VStack(spacing: 8) {
                        ZStack {
                            RoundedRectangle(cornerRadius: 18)
                                .fill(Color.black)
                                .overlay(RoundedRectangle(cornerRadius: 18).stroke(Color.gitpassStroke, lineWidth: 1))
                                .frame(width: 84, height: 84)
                            Text("gp")
                                .font(.system(size: 34, weight: .bold, design: .rounded))
                                .foregroundStyle(Color.gitpassPrimary)
                        }
                        Text("gitpass")
                            .font(.largeTitle.bold())
                        Text(String(localized: "End-to-end encrypted passwords in your own Git repository."))
                            .font(.subheadline)
                            .foregroundStyle(Color.gitpassSecondary)
                            .multilineTextAlignment(.center)
                    }

                    VStack(spacing: 12) {
                        SecureField(String(localized: "Master password"), text: $password)
                            .textContentType(.password)
                            .textFieldStyle(.roundedBorder)
                            .autocapitalization(.none)
                            .disableAutocorrection(true)
                        if showSecretKey {
                            TextField(String(localized: "Secret Key (A3-…)"), text: $secretKey)
                                .textFieldStyle(.roundedBorder)
                                .autocapitalization(.none)
                                .disableAutocorrection(true)
                        } else {
                            SecureField(String(localized: "Secret Key (A3-…)"), text: $secretKey)
                                .textFieldStyle(.roundedBorder)
                                .autocapitalization(.none)
                                .disableAutocorrection(true)
                        }
                        Toggle(String(localized: "Show Secret Key"), isOn: $showSecretKey)
                            .font(.footnote)
                    }

                    if let errorMessage {
                        Text(errorMessage)
                            .font(.footnote)
                            .foregroundStyle(Color.gitpassPrimary)
                            .multilineTextAlignment(.center)
                    }

                    Button(String(localized: "Unlock")) {
                        do {
                            try store.unlock(password: password, secretKey: secretKey)
                        } catch {
                            errorMessage = errorDescription(error)
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(.gitpassPrimary)
                    .foregroundStyle(.black)
                    .disabled(!canUnlock)
                    .frame(maxWidth: .infinity)

                    if KeychainStore.biometricsAvailable() {
                        Button {
                            Task {
                                do {
                                    try await store.unlockWithBiometrics()
                                } catch {
                                    errorMessage = errorDescription(error)
                                }
                            }
                        } label: {
                            Label(String(localized: "Unlock with Face ID"), systemImage: "faceid")
                                .foregroundStyle(Color.gitpassPrimary)
                        }
                    }

                    Divider()

                    VStack(spacing: 8) {
                        if store.vaultExists {
                            Text(String(localized: "A vault is already on this device. Unlock it above."))
                                .font(.footnote)
                                .foregroundStyle(Color.gitpassSecondary)
                        } else {
                            Button(String(localized: "Create a new account on this device")) {
                                isCreating = true
                            }
                            .font(.footnote)
                            Text(String(localized: "Creating an account generates a Secret Key. Save it somewhere offline — there is no recovery."))
                                .font(.footnote)
                                .foregroundStyle(Color.gitpassSecondary)
                                .multilineTextAlignment(.center)
                        }
                        Button(String(localized: "Sync from GitHub first")) {
                            showingSync = true
                        }
                        .font(.footnote)
                    }
                }
                .padding()
            }
            .gitpassBackground()
            .sheet(isPresented: $showingSync) {
                SyncSetupView()
                    .environmentObject(store)
            }
            .navigationTitle("gitpass")
            .alert(String(localized: "Your Secret Key"), isPresented: $showingSecretAlert) {
                Button(String(localized: "I saved it")) {
                    showingSecretAlert = false
                    createdSecret = nil
                }
            } message: {
                Text(createdSecret ?? "")
            }
            .alert(String(localized: "Create account"), isPresented: $isCreating) {
                Button(String(localized: "Cancel"), role: .cancel) {}
                Button(String(localized: "Create")) {
                    do {
                        let secret = try store.createAccount(password: password)
                        createdSecret = secret
                        showingSecretAlert = true
                    } catch {
                        errorMessage = errorDescription(error)
                    }
                }
            } message: {
                Text(String(localized: "This creates a new gitpass account on this device with the master password above."))
            }
        }
    }

    private func errorDescription(_ error: Error) -> String {
        if let gitpass = error as? GitpassError {
            switch gitpass {
            case .Generic(let message):
                // Core's CLI hint for a missing format.json; never show
                // a desktop command on iOS.
                if message.contains("not initialized") { return VaultStore.noVaultMessage }
                return message
            }
        }
        return error.localizedDescription
    }
}
