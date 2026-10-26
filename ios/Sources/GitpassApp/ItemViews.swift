import SwiftUI

// The generated FFI structs carry an `id` field but do not declare
// Identifiable; the app declares it here so sheets can bind to a
// selected item.
extension ItemSummary: Identifiable {}

struct ItemListView: View {
    @EnvironmentObject private var store: VaultStore
    @Binding var query: String
    @Binding var favoritesOnly: Bool
    @Binding var showingNewItem: Bool
    @State private var selectedItem: ItemSummary?

    private var items: [ItemSummary] {
        store.filteredItems(query: query, favoritesOnly: favoritesOnly)
    }

    var body: some View {
        List {
            if items.isEmpty {
                ContentUnavailableView(
                    String(localized: "No items"),
                    systemImage: "lock",
                    description: Text(String(localized: "Add your first item with the + button, or check your search."))
                )
            } else {
                ForEach(items, id: \.id) { item in
                    Button {
                        selectedItem = item
                    } label: {
                        HStack(spacing: 12) {
                            CategoryIconView(category: GitpassCategory.from(raw: item.category))
                            VStack(alignment: .leading, spacing: 2) {
                                Text(item.title).font(.headline).foregroundStyle(.primary)
                                if !item.subtitle.isEmpty {
                                    Text(item.subtitle).font(.subheadline).foregroundStyle(.secondary)
                                }
                                if !item.tags.isEmpty {
                                    Text(item.tags.joined(separator: ", "))
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                            }
                            Spacer()
                            if item.favorite {
                                Image(systemName: "star.fill")
                                    .foregroundStyle(.yellow)
                                    .font(.caption)
                            }
                            if item.hasTotp {
                                Image(systemName: "timer")
                                    .foregroundStyle(.secondary)
                                    .font(.caption)
                            }
                        }
                    }
                    .swipeActions(edge: .trailing) {
                        Button(role: .destructive) {
                            try? store.deleteItem(vaultId: item.vaultId, itemId: item.id)
                        } label: {
                            Label(String(localized: "Delete"), systemImage: "trash")
                        }
                        Button {
                            try? store.toggleFavorite(vaultId: item.vaultId, item: item)
                        } label: {
                            Label(
                                item.favorite ? String(localized: "Unfavorite") : String(localized: "Favorite"),
                                systemImage: "star"
                            )
                        }
                        .tint(.yellow)
                    }
                }
            }
        }
        .searchable(text: $query, prompt: String(localized: "Search"))
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { favoritesOnly.toggle() } label: {
                    Label(String(localized: "Favorites"), systemImage: favoritesOnly ? "star.fill" : "star")
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button { showingNewItem = true } label: {
                    Label(String(localized: "New Item"), systemImage: "plus")
                }
            }
        }
        .sheet(item: $selectedItem) { item in
            ItemDetailSheet(item: item)
                .environmentObject(store)
        }
    }
}

struct ItemDetailSheet: View {
    @EnvironmentObject private var store: VaultStore
    var item: ItemSummary
    @State private var detail: ItemDetail?
    @State private var errorMessage: String?
    @State private var showingEdit = false
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                if let detail {
                    ItemDetailView(detail: detail)
                } else if let errorMessage {
                    Text(errorMessage).foregroundStyle(.red).padding()
                } else {
                    ProgressView(String(localized: "Loading…"))
                }
            }
            .navigationTitle(item.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button(String(localized: "Close")) { dismiss() }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button(String(localized: "Edit")) { showingEdit = true }
                }
            }
            .onAppear {
                do {
                    detail = try store.itemDetail(vaultId: item.vaultId, itemId: item.id)
                } catch {
                    errorMessage = error.localizedDescription
                }
            }
            .sheet(isPresented: $showingEdit) {
                if let detail {
                    ItemEditView(vaultId: item.vaultId, itemId: item.id, detail: detail)
                        .environmentObject(store)
                }
            }
        }
    }
}

struct ItemDetailView: View {
    @EnvironmentObject private var store: VaultStore
    var detail: ItemDetail
    @State private var revealedFields: Set<String> = []
    @State private var copiedMessage: String?

    var body: some View {
        List {
            Section {
                HStack(spacing: 12) {
                    CategoryIconView(category: GitpassCategory.from(raw: detail.category), size: 48)
                    VStack(alignment: .leading) {
                        Text(detail.title).font(.headline)
                        Text(GitpassCategory.from(raw: detail.category).title)
                            .font(.subheadline).foregroundStyle(.secondary)
                    }
                }
                if !detail.tags.isEmpty {
                    Text(detail.tags.joined(separator: ", ")).font(.footnote).foregroundStyle(.secondary)
                }
            }
            ForEach(detail.sections, id: \.id) { section in
                Section(section.title ?? String(localized: "Fields")) {
                    ForEach(section.fields, id: \.id) { field in
                        FieldRow(
                            field: field,
                            isCardNumber: detail.category == "credit_card"
                                && (field.id == "number" || field.label == "number"),
                            isRevealed: revealedFields.contains(field.id),
                            onToggleReveal: { revealedFields.formSymmetricDifference([field.id]) },
                            onCopy: { value in
                                store.copyToClipboard(value)
                                copiedMessage = String(localized: "Copied")
                            }
                        )
                    }
                }
            }
            if let notes = detail.notes, !notes.isEmpty {
                Section(String(localized: "Notes")) {
                    Text(notes)
                    Button(String(localized: "Copy")) { store.copyToClipboard(notes) }
                }
            }
            if let url = detail.url, !url.isEmpty {
                Section(String(localized: "Website")) {
                    Text(url)
                    Button(String(localized: "Copy")) { store.copyToClipboard(url) }
                    if let link = URL(string: url.hasPrefix("http") ? url : "https://\(url)") {
                        Link(String(localized: "Open"), destination: link)
                    }
                }
            }
            if let copiedMessage {
                Section { Text(copiedMessage).foregroundStyle(.secondary).font(.footnote) }
            }
        }
    }
}

struct FieldRow: View {
    @EnvironmentObject private var store: VaultStore
    var field: FfiField
    /// Credit-card number: masked as "•••• •••• •••• 7575" (like
    /// desktop) instead of dots only. Other secrets, e.g. the CVV,
    /// stay fully hidden.
    var isCardNumber: Bool = false
    var isRevealed: Bool
    var onToggleReveal: () -> Void
    var onCopy: (String) -> Void
    @State private var liveCode: TotpCode?

    private var isSecret: Bool { field.kind == "concealed" || field.kind == "totp" }

    static func cardMask(_ value: String) -> String {
        let digits = value.filter(\.isASCII).filter(\.isNumber)
        guard digits.count >= 8 else { return "••••••••" }
        return "•••• •••• •••• \(digits.suffix(4))"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(field.label).font(.caption).foregroundStyle(.secondary)
            HStack {
                if field.kind == "totp" {
                    if let liveCode {
                        Text(liveCode.code).font(.headline.monospaced())
                        Text("(\(liveCode.secondsRemaining)s)").font(.caption).foregroundStyle(.secondary)
                    } else {
                        Text("••••••").font(.headline.monospaced())
                    }
                } else if isSecret && !isRevealed {
                    Text(isCardNumber ? Self.cardMask(field.value ?? "") : "••••••••")
                        .font(.headline.monospaced())
                } else {
                    Text(field.value ?? "").font(.headline)
                }
                Spacer()
                if isSecret {
                    Button(isRevealed ? String(localized: "Hide") : String(localized: "Show")) {
                        onToggleReveal()
                    }
                    .font(.footnote)
                }
                if field.kind == "totp" {
                    Button(String(localized: "Refresh")) {
                        if let value = field.value, !value.isEmpty {
                            liveCode = try? store.totpCode(for: value)
                        }
                    }
                    .font(.footnote)
                }
                if let value = field.value, !value.isEmpty {
                    Button(String(localized: "Copy")) {
                        if field.kind == "totp" {
                            if let code = try? store.totpCode(for: value) {
                                onCopy(code.code)
                            }
                        } else {
                            onCopy(value)
                        }
                    }
                    .font(.footnote)
                }
            }
        }
        .onAppear {
            if field.kind == "totp", let value = field.value, !value.isEmpty {
                liveCode = try? store.totpCode(for: value)
            }
        }
    }
}
