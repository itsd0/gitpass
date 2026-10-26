import SwiftUI

/// Category icons for gitpass iOS.
///
/// These are the desktop app's nine categories, rendered with system
/// symbols in the gitpass orange tint — not copied from any other
/// password manager. The desktop glyphs are custom SVG; the iOS app
/// uses the platform's symbol set for the same categories so the
/// icons stay sharp at every size and respect accessibility settings.
enum GitpassCategory: String, CaseIterable, Identifiable {
    case login, creditCard = "credit_card", secureNote = "secure_note",
         identity, apiCredential = "api_credential", document,
         sshKey = "ssh_key", wifi, passkey

    var id: String { rawValue }

    var title: String {
        switch self {
        case .login: return String(localized: "Login")
        case .creditCard: return String(localized: "Credit Card")
        case .secureNote: return String(localized: "Secure Note")
        case .identity: return String(localized: "Identity")
        case .apiCredential: return String(localized: "API Credential")
        case .document: return String(localized: "Document")
        case .sshKey: return String(localized: "SSH Key")
        case .wifi: return String(localized: "Wi-Fi")
        case .passkey: return String(localized: "Passkey")
        }
    }

    var systemImage: String {
        switch self {
        case .login: return "key"
        case .creditCard: return "creditcard"
        case .secureNote: return "note.text"
        case .identity: return "person.text.rectangle"
        case .apiCredential: return "chevron.left.forwardslash.chevron.right"
        case .document: return "doc.text"
        case .sshKey: return "terminal"
        case .wifi: return "wifi"
        case .passkey: return "person.badge.key"
        }
    }

    static func from(raw: String) -> GitpassCategory {
        GitpassCategory(rawValue: raw) ?? .secureNote
    }
}

extension Color {
    /// gitpass orange accent, matching the desktop app (#F28C28).
    static let gitpassOrange = Color(red: 0.95, green: 0.55, blue: 0.16)
}

struct CategoryIconView: View {
    var category: GitpassCategory
    var size: CGFloat = 36

    var body: some View {
        ZStack {
            Circle()
                .fill(Color.gitpassOrange.opacity(0.14))
                .overlay(Circle().stroke(Color.gitpassOrange.opacity(0.3), lineWidth: 1))
            Image(systemName: category.systemImage)
                .font(.system(size: size * 0.45, weight: .semibold))
                .foregroundStyle(Color.gitpassOrange)
        }
        .frame(width: size, height: size)
        .accessibilityLabel(category.title)
    }
}
