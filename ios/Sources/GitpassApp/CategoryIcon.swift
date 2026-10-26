import SwiftUI

/// Category icons for gitpass iOS.
///
/// These are the desktop app's nine categories, rendered with system
/// symbols in the monochrome gitpass palette — not copied from any other
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

/// gitpass iOS palette: black and white only. Pure black background,
/// white primary text and symbols, gray secondary text.
extension Color {
    static let gitpassBackground = Color.black
    static let gitpassPrimary = Color.white
    static let gitpassSecondary = Color(white: 0.6)
    static let gitpassSurface = Color(white: 0.1)
    static let gitpassStroke = Color(white: 0.25)
}

extension View {
    /// Pure black behind lists, forms and scroll views (instead of the
    /// system's dark gray grouped background).
    func gitpassBackground() -> some View {
        scrollContentBackground(.hidden)
            .background(Color.gitpassBackground.ignoresSafeArea())
    }
}

struct CategoryIconView: View {
    var category: GitpassCategory
    var size: CGFloat = 36

    var body: some View {
        ZStack {
            Circle()
                .fill(Color.gitpassSurface)
                .overlay(Circle().stroke(Color.gitpassStroke, lineWidth: 1))
            Image(systemName: category.systemImage)
                .font(.system(size: size * 0.45, weight: .semibold))
                .foregroundStyle(Color.gitpassPrimary)
        }
        .frame(width: size, height: size)
        .accessibilityLabel(category.title)
    }
}
