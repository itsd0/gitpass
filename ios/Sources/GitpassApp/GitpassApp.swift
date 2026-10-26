import SwiftUI

@main
struct GitpassApp: App {
    @StateObject private var store = VaultStore()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(store)
                .preferredColorScheme(.dark)
        }
    }
}

struct ContentView: View {
    @EnvironmentObject private var store: VaultStore
    @State private var showingSettings = false

    var body: some View {
        Group {
            if store.isUnlocked {
                MainView(showingSettings: $showingSettings)
            } else {
                UnlockView()
            }
        }
        .sheet(isPresented: $showingSettings) {
            SettingsView()
                .environmentObject(store)
        }
    }
}

struct MainView: View {
    @EnvironmentObject private var store: VaultStore
    @Binding var showingSettings: Bool
    @State private var query = ""
    @State private var favoritesOnly = false
    @State private var selectedVaultId: String?
    @State private var showingNewItem = false

    var body: some View {
        NavigationStack {
            ItemListView(
                query: $query,
                favoritesOnly: $favoritesOnly,
                showingNewItem: $showingNewItem
            )
            .navigationTitle(String(localized: "All Items"))
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Menu {
                        Picker(String(localized: "Vault"), selection: $selectedVaultId) {
                            ForEach(store.vaults, id: \.id) { vault in
                                Text("\(vault.name) (\(vault.itemCount))").tag(vault.id as String?)
                            }
                        }
                        .onChange(of: selectedVaultId) { _, newValue in
                            store.selectedVaultId = newValue
                            try? store.refresh()
                        }
                    } label: {
                        Label(
                            store.vaults.first { $0.id == store.selectedVaultId }?.name ?? String(localized: "Vaults"),
                            systemImage: "lock"
                        )
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showingSettings = true } label: {
                        Label(String(localized: "Settings"), systemImage: "gear")
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button { store.lock() } label: {
                        Label(String(localized: "Lock"), systemImage: "lock.fill")
                    }
                }
            }
            .onAppear {
                if selectedVaultId == nil { selectedVaultId = store.selectedVaultId }
            }
            .sheet(isPresented: $showingNewItem) {
                if let vaultId = store.selectedVaultId {
                    ItemEditView(vaultId: vaultId, itemId: nil, detail: nil)
                        .environmentObject(store)
                }
            }
        }
        .tint(.gitpassOrange)
    }
}
