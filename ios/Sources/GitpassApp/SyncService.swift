import CryptoKit
import Foundation

/// GitHub sync for gitpass iOS.
///
/// Transport is the GitHub REST / Git Data API over HTTPS, not the
/// `git` CLI (iOS has no git binary). The conflict rules are the
/// desktop's: field-level merge from `gitpass-core`
/// (`merge_item_records`), where the higher field clock wins and an
/// equal clock with different values keeps the local value and is
/// recorded as a conflict. Nothing about the merge is re-implemented
/// in Swift.
///
/// Scope of this first version, stated plainly so nobody mistakes it
/// for full desktop sync:
///  - Items are merged within vaults that exist on both sides.
///  - A vault that exists on only one side aborts the sync without
///    changing local data (the desktop `sync --merge` rule for
///    local-only vaults, extended to remote-only vaults, because the
///    encrypted manifest cannot be rebuilt from the iOS side for a
///    whole new vault). Creating a vault on a new device still goes
///    through a first pull (whole-tree copy, below) or desktop git.
///  - A first sync on a device with no local vault is a whole-tree
///    pull from the remote, then a normal unlock.
///  - Uploads are a single Git Data API commit whose parent is the
///    fetched remote HEAD; the ref update is not forced, so if the
///    remote moved in between, the update fails and the user retries
///    instead of overwriting someone else's push.
///  - Local-only files (`.gitpass-state.json`, the local device
///    registry file, backups) are never uploaded, matching the
///    desktop `.gitignore` behaviour for local state.
///  - This service never touches any repository other than the one
///    the user configures in Settings. The default is
///    `itsd0/gitpass-vault`, and the token is scoped by the OAuth
///    app the user connects, not by this code.
protocol HTTPSession {
    func data(for request: URLRequest) async throws -> (Data, URLResponse)
}

extension URLSession: HTTPSession {}

struct DeviceCodeResponse: Decodable {
    var deviceCode: String
    var userCode: String
    var verificationUri: String
    var expiresIn: Int
    var interval: Int
}

struct GitHubTreeEntry: Decodable {
    var path: String
    var mode: String?
    var type: String
    var sha: String
    var url: String?
}

struct GitHubTreeResponse: Decodable {
    var sha: String
    var tree: [GitHubTreeEntry]
}

struct GitHubBlobResponse: Decodable {
    var content: String
    var encoding: String
}

struct GitHubRefResponse: Decodable {
    struct Object: Decodable { var sha: String }
    var object: Object
}

struct GitHubCommitResponse: Decodable {
    var sha: String
}

struct SyncReport: Equatable {
    var merged: Int
    var addedFromRemote: Int
    var uploadedFiles: Int
    var message: String
}

enum SyncError: Error, LocalizedError {
    case notConfigured
    case http(Int, String)
    case invalidResponse
    case vaultsDiffer(String)
    case tokenExpired

    var errorDescription: String? {
        switch self {
        case .notConfigured: return "GitHub sync is not configured. Connect it in Settings first."
        case .http(let code, let body): return "GitHub returned HTTP \(code): \(body.prefix(200))"
        case .invalidResponse: return "GitHub returned an unexpected response."
        case .vaultsDiffer(let detail): return detail
        case .tokenExpired: return "The GitHub token was rejected. Reconnect in Settings."
        }
    }
}

final class GitHubSyncService {
    private let session: HTTPSession
    private let keychain: KeychainStore

    init(session: HTTPSession = URLSession.shared, keychain: KeychainStore = .shared) {
        self.session = session
        self.keychain = keychain
    }

    // MARK: - Device flow

    /// Step 1 of GitHub device flow. The client ID is an OAuth App
    /// identifier, not a secret; the user enters it in Settings
    /// (their own app, or one they were given). It is stored in
    /// UserDefaults, never in the Keychain, and never logged.
    func requestDeviceCode(clientId: String) async throws -> DeviceCodeResponse {
        var request = URLRequest(url: URL(string: "https://github.com/login/device/code")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(
            withJSONObject: ["client_id": clientId, "scope": "repo"]
        )
        let (data, response) = try await session.data(for: request)
        try check(response: response, data: data)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(DeviceCodeResponse.self, from: data)
    }

    /// Step 2: poll until the user authorizes, the code expires, or
    /// the caller cancels the task. `authorization_pending` and
    /// `slow_down` are handled per the device-flow spec.
    func pollForToken(clientId: String, deviceCode: String, initialInterval: Int) async throws -> String {
        var interval = max(initialInterval, 1)
        let deadline = Date().addingTimeInterval(10 * 60)
        while Date() < deadline {
            try await Task.sleep(nanoseconds: UInt64(interval) * 1_000_000_000)
            var request = URLRequest(url: URL(string: "https://github.com/login/oauth/access_token")!)
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Accept")
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: [
                "client_id": clientId,
                "device_code": deviceCode,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            ])
            let (data, response) = try await session.data(for: request)
            try check(response: response, data: data)
            guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                throw SyncError.invalidResponse
            }
            if let token = json["access_token"] as? String, !token.isEmpty {
                return token
            }
            switch json["error"] as? String {
            case "authorization_pending": break
            case "slow_down": interval += 5
            case "expired_token": throw SyncError.tokenExpired
            case "access_denied": throw SyncError.tokenExpired
            default: break
            }
        }
        throw SyncError.tokenExpired
    }

    // MARK: - API helpers

    private func check(response: URLResponse, data: Data) throws {
        guard let http = response as? HTTPURLResponse else { throw SyncError.invalidResponse }
        guard (200..<300).contains(http.statusCode) else {
            if http.statusCode == 401 { throw SyncError.tokenExpired }
            throw SyncError.http(http.statusCode, String(data: data, encoding: .utf8) ?? "")
        }
    }

    private func api(
        token: String, path: String, method: String = "GET", body: [String: Any]? = nil
    ) async throws -> (Data, HTTPURLResponse) {
        var request = URLRequest(url: URL(string: "https://api.github.com\(path)")!)
        request.httpMethod = method
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        request.setValue("gitpass-ios", forHTTPHeaderField: "User-Agent")
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw SyncError.invalidResponse }
        try check(response: response, data: data)
        return (data, http)
    }

    /// SHA-1 of a git blob object (`blob <len>\0<content>`). Used to
    /// skip unchanged files on upload. This is a content-addressing
    /// hash for git, not a security hash; no secret is hashed here.
    static func gitBlobSHA(for data: Data) -> String {
        var header = Data("blob \(data.count)\0".utf8)
        header.append(data)
        return Insecure.SHA1.hash(data: header).map { String(format: "%02x", $0) }.joined()
    }

    private static let localOnlyFiles: Set<String> = [
        ".gitpass-state.json",
        ".gitpass-device.json",
    ]

    static func isSyncable(path: String) -> Bool {
        // Remote tree paths are untrusted: never let one escape the
        // vault directory (absolute paths, `..` or empty components).
        if path.isEmpty || path.hasPrefix("/") { return false }
        if path.split(separator: "/", omittingEmptySubsequences: false)
            .contains(where: { $0.isEmpty || $0 == "." || $0 == ".." }) { return false }
        if localOnlyFiles.contains(path) { return false }
        if path.contains("backup") { return false }
        if path.hasPrefix(".git/") { return false }
        return true
    }

    // MARK: - Sync

    private struct RemoteSnapshot {
        var branch: String
        var headSHA: String
        var tree: [GitHubTreeEntry]
    }

    private func fetchRemote(token: String, repo: String) async throws -> RemoteSnapshot {
        let repoPath = "/repos/\(repo)"
        let (repoData, _) = try await api(token: token, path: repoPath)
        guard let repoJson = try JSONSerialization.jsonObject(with: repoData) as? [String: Any],
              let branch = repoJson["default_branch"] as? String
        else { throw SyncError.invalidResponse }
        // A 404 on the branch ref means an empty repository (no commits yet).
        do {
            let (refData, _) = try await api(token: token, path: "\(repoPath)/git/ref/heads/\(branch)")
            let refJson = try JSONDecoder().decode(GitHubRefResponse.self, from: refData)
            let headSHA = refJson.object.sha
            let (treeData, _) = try await api(
                token: token, path: "\(repoPath)/git/trees/\(headSHA)?recursive=1"
            )
            let tree = try JSONDecoder().decode(GitHubTreeResponse.self, from: treeData).tree
                .filter { $0.type == "blob" && Self.isSyncable(path: $0.path) }
            return RemoteSnapshot(branch: branch, headSHA: headSHA, tree: tree)
        } catch SyncError.http(404, _) {
            return RemoteSnapshot(branch: branch, headSHA: "", tree: [])
        }
    }

    private func downloadTree(token: String, repo: String, entries: [GitHubTreeEntry], to staging: URL) async throws {
        let fileManager = FileManager.default
        try fileManager.createDirectory(at: staging, withIntermediateDirectories: true)
        for entry in entries {
            let (blobData, _) = try await api(token: token, path: "/repos/\(repo)/git/blobs/\(entry.sha)")
            let blob = try JSONDecoder().decode(GitHubBlobResponse.self, from: blobData)
            guard blob.encoding == "base64",
                  let content = Data(base64Encoded: blob.content.replacingOccurrences(of: "\n", with: ""))
            else { throw SyncError.invalidResponse }
            let dest = staging.appendingPathComponent(entry.path)
            try fileManager.createDirectory(at: dest.deletingLastPathComponent(), withIntermediateDirectories: true)
            try content.write(to: dest)
        }
    }

    /// First pull for a device with no local vault yet: downloads the
    /// whole remote tree into `localRoot`. The caller then unlocks
    /// with the master password + Secret Key. If the remote is empty,
    /// nothing is copied and the report says so.
    func firstPull(localRoot: URL, repo: String) async throws -> SyncReport {
        guard let token = keychain.loadGitHubToken() else { throw SyncError.notConfigured }
        let remote = try await fetchRemote(token: token, repo: repo)
        if remote.tree.isEmpty {
            return SyncReport(merged: 0, addedFromRemote: 0, uploadedFiles: 0,
                              message: "The remote repository is empty. Create an account in the app first, then sync to push it.")
        }
        let fileManager = FileManager.default
        let staging = fileManager.temporaryDirectory
            .appendingPathComponent("gitpass-sync-staging-\(UUID().uuidString)")
        defer { try? fileManager.removeItem(at: staging) }
        try await downloadTree(token: token, repo: repo, entries: remote.tree, to: staging)
        for entry in remote.tree {
            let src = staging.appendingPathComponent(entry.path)
            let dest = localRoot.appendingPathComponent(entry.path)
            try fileManager.createDirectory(at: dest.deletingLastPathComponent(), withIntermediateDirectories: true)
            if fileManager.fileExists(atPath: dest.path) { try fileManager.removeItem(at: dest) }
            try fileManager.copyItem(at: src, to: dest)
        }
        return SyncReport(merged: 0, addedFromRemote: remote.tree.count, uploadedFiles: 0,
                          message: "Pulled the remote vault. Unlock it to continue.")
    }

    /// Runs one sync against `repo` (e.g. `itsd0/gitpass-vault`).
    /// `vault` is the already-unlocked local vault, `password` /
    /// `secretKey` are only used to open the staged remote copy for
    /// record comparison; they are not stored by this service.
    func sync(
        vault: Vault, localRoot: URL, password: String, secretKey: String, repo: String
    ) async throws -> SyncReport {
        guard let token = keychain.loadGitHubToken() else { throw SyncError.notConfigured }
        let repoPath = "/repos/\(repo)"
        let remote = try await fetchRemote(token: token, repo: repo)
        let remoteTree = remote.tree
        let headSHA = remote.headSHA
        let branch = remote.branch

        let fileManager = FileManager.default
        let staging = fileManager.temporaryDirectory
            .appendingPathComponent("gitpass-sync-staging-\(UUID().uuidString)")
        defer { try? fileManager.removeItem(at: staging) }
        try await downloadTree(token: token, repo: repo, entries: remoteTree, to: staging)

        if remoteTree.isEmpty {
            // Remote is empty: upload the local tree as the first commit.
            let uploaded = try await uploadLocalTree(
                token: token, repoPath: repoPath, branch: branch, headSHA: headSHA,
                localRoot: localRoot, remoteTree: []
            )
            return SyncReport(merged: 0, addedFromRemote: 0, uploadedFiles: uploaded,
                              message: "Pushed the local vault to an empty remote.")
        }

        // Merge within shared vaults.
        let remoteVault: Vault
        do {
            remoteVault = try openVault(
                root: staging.path, password: password, secretKey: secretKey
            )
        } catch {
            throw SyncError.vaultsDiffer("The remote vault could not be opened with these credentials. Nothing was changed.")
        }
        let localVaultIds = Set(try vault.listVaults().map(\.id))
        let remoteVaultIds = Set(try remoteVault.listVaults().map(\.id))
        if localVaultIds != remoteVaultIds {
            let onlyLocal = localVaultIds.subtracting(remoteVaultIds)
            let onlyRemote = remoteVaultIds.subtracting(localVaultIds)
            throw SyncError.vaultsDiffer(
                "Vaults differ (local-only: \(onlyLocal.sorted().joined(separator: ", ")); " +
                "remote-only: \(onlyRemote.sorted().joined(separator: ", "))). " +
                "Sync does not create or delete whole vaults; use desktop git sync for vault changes. Nothing was changed."
            )
        }

        var merged = 0
        var added = 0
        for vid in localVaultIds {
            let localItems = try vault.listItems(vaultId: vid)
            let localIds = Set(localItems.map(\.id))
            let remoteItems = try remoteVault.listItems(vaultId: vid)
            let remoteIds = Set(remoteItems.map(\.id))
            for iid in remoteIds.subtracting(localIds) {
                let record = try remoteVault.recordJson(vaultId: vid, itemId: iid)
                try vault.putRecordJson(vaultId: vid, itemId: iid, recordJson: record)
                added += 1
            }
            for iid in localIds.intersection(remoteIds) {
                let localRecord = try vault.recordJson(vaultId: vid, itemId: iid)
                let remoteRecord = try remoteVault.recordJson(vaultId: vid, itemId: iid)
                if localRecord == remoteRecord { continue }
                let mergedRecord = try mergeItemRecords(localJson: localRecord, remoteJson: remoteRecord)
                if mergedRecord != localRecord {
                    try vault.putRecordJson(vaultId: vid, itemId: iid, recordJson: mergedRecord)
                }
                merged += 1
            }
        }

        let uploaded = try await uploadLocalTree(
            token: token, repoPath: repoPath, branch: branch, headSHA: headSHA,
            localRoot: localRoot, remoteTree: remoteTree
        )
        return SyncReport(merged: merged, addedFromRemote: added, uploadedFiles: uploaded,
                          message: "Merged \(merged) item(s), added \(added) from remote, uploaded \(uploaded) file(s).")
    }

    // MARK: - Upload (Git Data API)

    private func uploadLocalTree(
        token: String, repoPath: String, branch: String, headSHA: String,
        localRoot: URL, remoteTree: [GitHubTreeEntry]
    ) async throws -> Int {
        let fileManager = FileManager.default
        guard let enumerator = fileManager.enumerator(at: localRoot, includingPropertiesForKeys: nil) else {
            return 0
        }
        var localFiles: [String: Data] = [:]
        for case let url as URL in enumerator {
            var isDir: ObjCBool = false
            guard fileManager.fileExists(atPath: url.path, isDirectory: &isDir), !isDir.boolValue else { continue }
            let rel = url.path.replacingOccurrences(of: localRoot.path + "/", with: "")
            guard Self.isSyncable(path: rel) else { continue }
            localFiles[rel] = try Data(contentsOf: url)
        }
        var remoteByPath: [String: String] = [:]
        for entry in remoteTree { remoteByPath[entry.path] = entry.sha }
        var treeEntries: [[String: Any]] = []
        var changed = 0
        for (path, data) in localFiles.sorted(by: { $0.key < $1.key }) {
            if remoteByPath[path] == Self.gitBlobSHA(for: data) { continue }
            let (blobData, _) = try await api(token: token, path: "\(repoPath)/git/blobs", method: "POST", body: [
                "content": data.base64EncodedString(), "encoding": "base64",
            ])
            guard let blobJson = try JSONSerialization.jsonObject(with: blobData) as? [String: Any],
                  let blobSHA = blobJson["sha"] as? String
            else { throw SyncError.invalidResponse }
            treeEntries.append(["path": path, "mode": "100644", "type": "blob", "sha": blobSHA])
            changed += 1
        }
        for path in remoteByPath.keys where localFiles[path] == nil {
            treeEntries.append(["path": path, "mode": "100644", "type": "blob", "sha": NSNull()])
            changed += 1
        }
        if treeEntries.isEmpty { return 0 }
        var treeBody: [String: Any] = ["tree": treeEntries]
        if !headSHA.isEmpty {
            let (treeData0, _) = try await api(token: token, path: "\(repoPath)/git/trees/\(headSHA)")
            if let treeJson = try JSONSerialization.jsonObject(with: treeData0) as? [String: Any],
               let baseSHA = treeJson["sha"] as? String {
                treeBody["base_tree"] = baseSHA
            }
        }
        let (treeData, _) = try await api(token: token, path: "\(repoPath)/git/trees", method: "POST", body: treeBody)
        guard let treeJson = try JSONSerialization.jsonObject(with: treeData) as? [String: Any],
              let treeSHA = treeJson["sha"] as? String
        else { throw SyncError.invalidResponse }
        var commitBody: [String: Any] = ["message": "gitpass iOS sync", "tree": treeSHA]
        if !headSHA.isEmpty { commitBody["parents"] = [headSHA] }
        let (commitData, _) = try await api(token: token, path: "\(repoPath)/git/commits", method: "POST", body: commitBody)
        let commit = try JSONDecoder().decode(GitHubCommitResponse.self, from: commitData)
        if headSHA.isEmpty {
            _ = try await api(token: token, path: "\(repoPath)/git/refs", method: "POST", body: [
                "ref": "refs/heads/\(branch)", "sha": commit.sha,
            ])
        } else {
            // Not forced: if the remote moved since we fetched, this
            // fails and the user retries instead of overwriting.
            _ = try await api(token: token, path: "\(repoPath)/git/refs/heads/\(branch)", method: "PATCH", body: [
                "sha": commit.sha, "force": false,
            ])
        }
        return changed
    }
}
