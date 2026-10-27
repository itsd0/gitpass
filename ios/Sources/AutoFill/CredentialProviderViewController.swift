import AuthenticationServices
import UIKit

/// gitpass AutoFill credential provider (passwords only in v1).
///
/// How it works, and what it deliberately does not do:
///  - The extension opens the same vault files as the app (the shared
///    app-group container), but it does NOT reuse the app's stored
///    Face ID credentials: the Keychain items are not shared between
///    the app and the extension, so the user types the master password
///    and Secret Key in the extension UI. The unlocked Rust session
///    lives only for the duration of this credential request and is
///    dropped when the view controller deallocates.
///  - Items are matched by domain: an item's URL host (or title, as
///    a fallback) containing the requested service identifier, the
///    same containment rule the app's search uses.
///  - Only a username + password pair is returned
///    (`ASPasswordCredential`). Passkey registration and assertion
///    are not implemented here: they require associated domains, an
///    RP ID and a real device, and are listed as device-only work in
///    the PR description. Passkey items in the vault are still listed
///    in the app, with their stored fields.
///  - If no vault is on the device, the extension tells the user to
///    open the app first and cancels — it never creates a vault.
final class CredentialProviderViewController: ASCredentialProviderViewController {

    private var vault: Vault?
    private var items: [ItemSummary] = []
    private var serviceIdentifier: String = ""

    private let tableView = UITableView(frame: .zero, style: .insetGrouped)
    private let passwordField = UITextField()
    private let secretField = UITextField()
    private let statusLabel = UILabel()

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        title = "gitpass"

        let stack = UIStackView()
        stack.axis = .vertical
        stack.spacing = 12
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 12),
            stack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 16),
            stack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -16),
        ])

        statusLabel.numberOfLines = 0
        statusLabel.font = .preferredFont(forTextStyle: .footnote)
        statusLabel.textColor = .secondaryLabel
        statusLabel.text = "Enter your master password and Secret Key to fill a password from gitpass."
        stack.addArrangedSubview(statusLabel)

        passwordField.placeholder = "Master password"
        passwordField.isSecureTextEntry = true
        passwordField.borderStyle = .roundedRect
        stack.addArrangedSubview(passwordField)

        secretField.placeholder = "Secret Key (A3-…)"
        secretField.isSecureTextEntry = true
        secretField.borderStyle = .roundedRect
        secretField.autocapitalizationType = .none
        stack.addArrangedSubview(secretField)

        let unlockButton = UIButton(type: .system)
        unlockButton.setTitle("Unlock", for: .normal)
        unlockButton.addTarget(self, action: #selector(unlockTapped), for: .touchUpInside)
        stack.addArrangedSubview(unlockButton)

        tableView.delegate = self
        tableView.dataSource = self
        tableView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(tableView)
        NSLayoutConstraint.activate([
            tableView.topAnchor.constraint(equalTo: stack.bottomAnchor, constant: 12),
            tableView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            tableView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            tableView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
    }

    override func prepareInterfaceToProvideCredential(for credentialIdentity: ASPasswordCredentialIdentity) {
        serviceIdentifier = credentialIdentity.serviceIdentifier.identifier
    }

    override func prepareCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
        serviceIdentifier = serviceIdentifiers.first?.identifier ?? ""
    }

    @objc private func unlockTapped() {
        guard let password = passwordField.text, !password.isEmpty,
              let secret = secretField.text, !secret.isEmpty
        else {
            statusLabel.text = "Enter both the master password and the Secret Key."
            return
        }
        let root: URL
        if let shared = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: "group.dev.gitpass.ios") {
            root = shared.appendingPathComponent("vault")
        } else {
            statusLabel.text = "No shared vault container is available. Open the gitpass app first."
            return
        }
        guard FileManager.default.fileExists(atPath: root.appendingPathComponent("format.json").path) else {
            statusLabel.text = "No gitpass vault is on this device yet. Open the gitpass app first."
            return
        }
        do {
            let opened = try openVault(root: root.path, password: password, secretKey: secret)
            vault = opened
            let vaults = try opened.listVaults()
            var all: [ItemSummary] = []
            for vaultInfo in vaults {
                all.append(contentsOf: try opened.listItems(vaultId: vaultInfo.id))
            }
            items = all.filter { item in
                guard !item.deleted && !item.archived else { return false }
                let host = URL(string: item.url ?? "")?.host?.lowercased() ?? ""
                let needle = serviceIdentifier.lowercased()
                if needle.isEmpty { return true }
                return host.contains(needle)
                    || item.url?.lowercased().contains(needle) == true
                    || item.title.lowercased().contains(needle)
            }
            statusLabel.text = items.isEmpty
                ? "Unlocked, but no gitpass item matches \(serviceIdentifier)."
                : "Choose an item to fill."
            tableView.reloadData()
        } catch {
            statusLabel.text = "Unlock failed: \(error.localizedDescription)"
        }
    }

    private func provide(item: ItemSummary) {
        guard let vault else { return }
        do {
            let detail = try vault.getItem(vaultId: item.vaultId, itemId: item.id)
            guard let username = detail.username, let password = detail.password else {
                statusLabel.text = "That item has no username and password to fill."
                return
            }
            let credential = ASPasswordCredential(user: username, password: password)
            extensionContext.completeRequest(withSelectedCredential: credential)
        } catch {
            statusLabel.text = "Could not read that item: \(error.localizedDescription)"
        }
    }

    deinit {
        // Dropping the Vault drops the Rust Session and its keys.
        vault = nil
    }
}

extension CredentialProviderViewController: UITableViewDataSource, UITableViewDelegate {
    func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
        items.count
    }

    func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = tableView.dequeueReusableCell(withIdentifier: "item")
            ?? UITableViewCell(style: .subtitle, reuseIdentifier: "item")
        let item = items[indexPath.row]
        cell.textLabel?.text = item.title
        cell.textLabel?.textColor = .label
        cell.detailTextLabel?.text = item.subtitle
        cell.detailTextLabel?.textColor = .secondaryLabel
        return cell
    }

    func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath, animated: true)
        provide(item: items[indexPath.row])
    }
}
