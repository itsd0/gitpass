import Foundation
import LocalAuthentication
import Security

/// Keychain storage for gitpass iOS.
///
/// Two kinds of secrets live here, and nothing else:
///  - the unlock credentials (master password + Secret Key) used by
///    Face ID unlock, gated by a biometry access control;
///  - the GitHub OAuth token from device flow, stored without biometry
///    so background sync does not need a Face ID prompt.
///
/// The master password itself is never written to a file, UserDefaults
/// or a log. No access group is set on purpose: the app and the
/// AutoFill extension do not share Keychain items. The extension asks
/// the user for the password itself (see Sources/AutoFill), so a
/// compromised extension process never inherits stored credentials.
enum KeychainError: Error, LocalizedError {
    case unavailable(OSStatus)
    case notFound
    case unexpectedData

    var errorDescription: String? {
        switch self {
        case .unavailable(let status): return "Keychain error (\(status))."
        case .notFound: return "No saved credentials."
        case .unexpectedData: return "Saved credentials could not be read."
        }
    }
}

struct UnlockCredentials: Codable {
    var password: String
    var secretKey: String
}

final class KeychainStore {
    static let shared = KeychainStore()
    private init() {}

    private let unlockAccount = "gitpass-unlock"
    private let tokenAccount = "gitpass-github-token"
    private let service = "dev.gitpass.ios"

    // MARK: - Unlock credentials (Face ID)

    func saveUnlockCredentials(_ credentials: UnlockCredentials, requireBiometry: Bool) throws {
        let data = try JSONEncoder().encode(credentials)
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: unlockAccount,
            kSecValueData as String: data,
        ]
        if requireBiometry {
            var error: Unmanaged<CFError>?
            guard let access = SecAccessControlCreateWithFlags(
                nil,
                kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
                .biometryCurrentSet,
                &error
            ) else {
                throw KeychainError.unavailable(errSecParam)
            }
            query[kSecAttrAccessControl as String] = access
        } else {
            query[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        }
        var status = SecItemAdd(query as CFDictionary, nil)
        if status == errSecDuplicateItem {
            let updateQuery: [String: Any] = [
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: service,
                kSecAttrAccount as String: unlockAccount,
            ]
            let attributes: [String: Any] = [kSecValueData as String: data]
            status = SecItemUpdate(updateQuery as CFDictionary, attributes as CFDictionary)
        }
        guard status == errSecSuccess else { throw KeychainError.unavailable(status) }
    }

    func loadUnlockCredentials() throws -> UnlockCredentials {
        // Presenting a Face ID prompt is the caller's job (LAContext);
        // the biometry access control on the item itself is the second
        // gate if the item was saved with requireBiometry.
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: unlockAccount,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        guard status == errSecSuccess else {
            if status == errSecItemNotFound { throw KeychainError.notFound }
            throw KeychainError.unavailable(status)
        }
        guard let data = item as? Data,
              let credentials = try? JSONDecoder().decode(UnlockCredentials.self, from: data)
        else { throw KeychainError.unexpectedData }
        return credentials
    }

    func deleteUnlockCredentials() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: unlockAccount,
        ]
        SecItemDelete(query as CFDictionary)
    }

    // MARK: - GitHub token (device flow)

    func saveGitHubToken(_ token: String) throws {
        let data = Data(token.utf8)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: tokenAccount,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            kSecValueData as String: data,
        ]
        var status = SecItemAdd(query as CFDictionary, nil)
        if status == errSecDuplicateItem {
            let updateQuery: [String: Any] = [
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: service,
                kSecAttrAccount as String: tokenAccount,
            ]
            status = SecItemUpdate(updateQuery as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        }
        guard status == errSecSuccess else { throw KeychainError.unavailable(status) }
    }

    func loadGitHubToken() -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: tokenAccount,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data
        else { return nil }
        return String(data: data, encoding: .utf8)
    }

    func deleteGitHubToken() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: tokenAccount,
        ]
        SecItemDelete(query as CFDictionary)
    }

    // MARK: - Face ID

    /// True when this device can evaluate a biometric policy. On the
    /// simulator this is usually false, which is why Face ID unlock is
    /// a device-only feature (see the PR description).
    static func biometricsAvailable() -> Bool {
        let context = LAContext()
        var error: NSError?
        return context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error)
    }

    static func evaluateBiometrics(reason: String) async throws -> Bool {
        let context = LAContext()
        context.localizedCancelTitle = String(localized: "Enter password instead")
        return try await context.evaluatePolicy(
            .deviceOwnerAuthenticationWithBiometrics,
            localizedReason: reason
        )
    }
}
