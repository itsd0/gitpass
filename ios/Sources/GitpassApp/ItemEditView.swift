import SwiftUI

struct EditableField: Identifiable {
    var id = UUID()
    var label: String
    var value: String
    var kind: String
}

/// Item editor.
///
/// For logins, the username / password / TOTP / URL fields map to the
/// legacy v5 login fields (the same fields desktop and the CLI read),
/// and any extra fields are stored as an additional section. For
/// every other category, all fields are stored as sections — the
/// legacy fields are left empty so a non-login item never silently
/// loses a field to the login synthesis in `view_sections`.
struct ItemEditView: View {
    @EnvironmentObject private var store: VaultStore
    @Environment(\.dismiss) private var dismiss

    var vaultId: String
    var itemId: String?
    var detail: ItemDetail?

    @State private var title = ""
    @State private var category: GitpassCategory = .login
    @State private var username = ""
    @State private var password = ""
    @State private var url = ""
    @State private var notes = ""
    @State private var totp = ""
    @State private var tagsText = ""
    @State private var favorite = false
    @State private var customFields: [EditableField] = []
    @State private var showGenerator = false
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            Form {
                Section(String(localized: "Item")) {
                    TextField(String(localized: "Title"), text: $title)
                    Picker(String(localized: "Category"), selection: $category) {
                        ForEach(GitpassCategory.allCases) { cat in
                            Text(cat.title).tag(cat)
                        }
                    }
                }
                if category == .login {
                    Section(String(localized: "Login")) {
                        TextField(String(localized: "Username"), text: $username)
                            .autocapitalization(.none)
                            .disableAutocorrection(true)
                        SecureField(String(localized: "Password"), text: $password)
                        Button(String(localized: "Generate password")) { showGenerator = true }
                        TextField(String(localized: "Website"), text: $url)
                            .autocapitalization(.none)
                            .disableAutocorrection(true)
                            .keyboardType(.URL)
                        TextField(String(localized: "One-time password secret"), text: $totp)
                            .autocapitalization(.none)
                            .disableAutocorrection(true)
                    }
                } else {
                    Section(String(localized: "Fields")) {
                        ForEach($customFields) { $field in
                            VStack(alignment: .leading) {
                                TextField(String(localized: "Label"), text: $field.label)
                                TextField(String(localized: "Value"), text: $field.value)
                                    .autocapitalization(.none)
                                    .disableAutocorrection(true)
                                Picker(String(localized: "Kind"), selection: $field.kind) {
                                    Text(String(localized: "Text")).tag("text")
                                    Text(String(localized: "Concealed")).tag("concealed")
                                    Text(String(localized: "URL")).tag("url")
                                    Text(String(localized: "Email")).tag("email")
                                    Text(String(localized: "Phone")).tag("phone")
                                    Text(String(localized: "One-time password")).tag("totp")
                                    Text(String(localized: "Multiline")).tag("multiline")
                                }
                                .pickerStyle(.menu)
                            }
                        }
                        .onDelete { customFields.remove(atOffsets: $0) }
                        Button(String(localized: "Add field")) {
                            customFields.append(EditableField(label: "", value: "", kind: "text"))
                        }
                    }
                }
                Section(String(localized: "More")) {
                    TextField(String(localized: "Notes"), text: $notes, axis: .vertical)
                    TextField(String(localized: "Tags (comma separated)"), text: $tagsText)
                        .autocapitalization(.none)
                        .disableAutocorrection(true)
                    Toggle(String(localized: "Favorite"), isOn: $favorite)
                }
                if let errorMessage {
                    Section { Text(errorMessage).foregroundStyle(.red) }
                }
            }
            .navigationTitle(itemId == nil ? String(localized: "New Item") : String(localized: "Edit Item"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button(String(localized: "Cancel")) { dismiss() }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button(String(localized: "Save")) { save() }
                        .disabled(title.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            .onAppear { loadFromDetail() }
            .sheet(isPresented: $showGenerator) {
                GeneratorView { generated in
                    password = generated
                    showGenerator = false
                }
            }
        }
    }

    private func loadFromDetail() {
        guard let detail else { return }
        title = detail.title
        category = GitpassCategory.from(raw: detail.category)
        username = detail.username ?? ""
        password = detail.password ?? ""
        url = detail.url ?? ""
        notes = detail.notes ?? ""
        totp = detail.totp ?? ""
        tagsText = detail.tags.joined(separator: ", ")
        favorite = detail.favorite
        // Extra sections only: the synthesized "login" section is
        // already represented by the fields above.
        customFields = detail.sections
            .filter { $0.id != "login" }
            .flatMap(\.fields)
            .map { EditableField(label: $0.label, value: $0.value ?? "", kind: $0.kind) }
    }

    private func save() {
        let tags = tagsText.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        let sections: [FfiSection]
        if category == .login {
            // For a new item, no sections means "no extra fields".
            // For an edit, always send the extra section (even empty)
            // so deleting every extra field actually tombstones them
            // in the core instead of silently keeping them.
            sections = (customFields.isEmpty && itemId == nil) ? [] : [
                FfiSection(
                    id: "extra",
                    title: String(localized: "Additional fields"),
                    fields: customFields.map {
                        FfiField(id: UUID().uuidString, label: $0.label, kind: $0.kind, value: $0.value.isEmpty ? nil : $0.value)
                    }
                )
            ]
        } else {
            sections = [
                FfiSection(
                    id: "main",
                    title: nil,
                    fields: customFields.map {
                        FfiField(id: UUID().uuidString, label: $0.label, kind: $0.kind, value: $0.value.isEmpty ? nil : $0.value)
                    }
                )
            ]
        }
        let input = ItemInput(
            title: title,
            username: category == .login ? (username.isEmpty ? nil : username) : nil,
            password: category == .login ? (password.isEmpty ? nil : password) : nil,
            url: category == .login ? (url.isEmpty ? nil : url) : nil,
            notes: notes.isEmpty ? nil : notes,
            totp: category == .login ? (totp.isEmpty ? nil : totp) : nil,
            category: category.rawValue,
            tags: tags,
            favorite: favorite,
            sections: sections
        )
        do {
            try store.saveItem(vaultId: vaultId, itemId: itemId, input: input)
            dismiss()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

struct GeneratorView: View {
    var onPick: (String) -> Void
    @State private var length: Double = 20
    @State private var generated = ""
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(generated.isEmpty ? String(localized: "Tap Generate") : generated)
                        .font(.headline.monospaced())
                        .textSelection(.enabled)
                    Slider(value: $length, in: 8...64, step: 1) {
                        Text(String(localized: "Length"))
                    }
                    Text("Length: \(Int(length))")
                }
                Section {
                    Text(String(localized: "Generated with gitpass-core, the same generator the CLI uses (letters, digits and symbols, uniform sampling)."))
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle(String(localized: "Password Generator"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button(String(localized: "Cancel")) { dismiss() }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button(String(localized: "Generate")) {
                        generated = generatePassword(length: UInt64(Int(length)))
                    }
                }
                ToolbarItem(placement: .bottomBar) {
                    Button(String(localized: "Use this password")) {
                        if !generated.isEmpty { onPick(generated) }
                    }
                    .disabled(generated.isEmpty)
                }
            }
            .onAppear {
                generated = generatePassword(length: UInt64(Int(length)))
            }
        }
    }
}
