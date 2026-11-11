import XCTest

// NOTE: This test target compiles the generated UniFFI bindings plus
// Keychain.swift and SyncService.swift directly (see project.yml), so
// it exercises the real Rust code without importing the app target
// as a library. The app target is only the test host (it launches the
// app binary on the simulator; the tests do not call app UI code).

final class StubHTTPSession: HTTPSession {
    var handler: (URLRequest) throws -> (Data, HTTPURLResponse) = { _ in
        throw URLError(.badServerResponse)
    }

    func data(for request: URLRequest) async throws -> (Data, URLResponse) {
        try handler(request)
    }
}

private func httpResponse(url: URL, status: Int) -> HTTPURLResponse {
    HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil)!
}

final class GitpassTests: XCTestCase {

    // MARK: - Rust FFI (real vault, real crypto)

    func testBridgeHandshakeMatchesApp() {
        let ffi = ffiVersion()
        let format = formatVersion()
        XCTAssertEqual(ffi.apiLevel, BridgeCompat.expectedApiLevel)
        XCTAssertFalse(ffi.crateVersion.isEmpty)
        XCTAssertEqual(format.currentWrite, BridgeCompat.expectedFormat)
        XCTAssertLessThanOrEqual(format.minRead, format.currentWrite)
        XCTAssertTrue(BridgeCompat.checkNative())
        XCTAssertFalse(BridgeCompat.isCompatible(apiLevel: BridgeCompat.expectedApiLevel + 1, minRead: 2, currentWrite: BridgeCompat.expectedFormat))
        XCTAssertFalse(BridgeCompat.isCompatible(apiLevel: BridgeCompat.expectedApiLevel, minRead: 2, currentWrite: BridgeCompat.expectedFormat + 1))
    }

    func testDefaultUnlockDoesNotRewriteSyncedFiles() throws {
        let options = defaultUnlockOptions()
        XCTAssertFalse(options.registerDevice)
        XCTAssertFalse(options.purgeExpiredTrash)
        XCTAssertFalse(options.upgradeKdf)
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("gitpass-test-\(UUID().uuidString)")
            .appendingPathComponent("vault")
        let password = "FAKE correct horse battery staple"
        let secret = try createAccount(root: root.path, password: password)
        _ = try openVaultWith(root: root.path, password: password, secretKey: secret, options: options)
        let synced = ["format.json", "keyset.enc", "manifest.enc", "devices.enc"]
        func read() -> [String: Data] {
            var out: [String: Data] = [:]
            for name in synced {
                out[name] = try? Data(contentsOf: root.appendingPathComponent(name))
            }
            return out
        }
        let before = read()
        _ = try openVaultWith(root: root.path, password: password, secretKey: secret, options: options)
        _ = try openVault(root: root.path, password: password, secretKey: secret)
        XCTAssertEqual(before, read())
    }

    func testVaultRoundTrip() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("gitpass-test-\(UUID().uuidString)")
            .appendingPathComponent("vault")
        let secret = try createAccount(root: root.path, password: "correct horse battery staple")
        XCTAssertTrue(secret.hasPrefix("A3-"))
        let vault = try openVault(root: root.path, password: "correct horse battery staple", secretKey: secret)
        let vaultId = try vault.createVault(name: "Personal", description: nil)
        XCTAssertFalse(vaultId.isEmpty)
        let vaults = try vault.listVaults()
        XCTAssertEqual(vaults.count, 1)
        XCTAssertEqual(vaults[0].name, "Personal")

        let input = ItemInput(
            title: "Example", username: "alice", password: "s3cret!",
            url: "https://example.com", notes: nil, totp: "JBSWY3DPEHPK3PXP",
            category: "login", tags: [], favorite: false, sections: []
        )
        let itemId = try vault.createItem(vaultId: vaultId, input: input)
        let items = try vault.listItems(vaultId: vaultId)
        XCTAssertEqual(items.count, 1)
        XCTAssertEqual(items[0].subtitle, "alice")
        XCTAssertTrue(items[0].hasTotp)
        XCTAssertFalse(items.map(\.title).joined().contains("s3cret"))

        let detail = try vault.getItem(vaultId: vaultId, itemId: itemId)
        XCTAssertEqual(detail.password, "s3cret!")
        XCTAssertEqual(detail.username, "alice")

        try vault.setFavorite(vaultId: vaultId, itemId: itemId, favorite: true)
        XCTAssertTrue(try vault.getItem(vaultId: vaultId, itemId: itemId).favorite)

        try vault.deleteItem(vaultId: vaultId, itemId: itemId)
        XCTAssertTrue(try vault.getItem(vaultId: vaultId, itemId: itemId).deleted)
        try vault.restoreItem(vaultId: vaultId, itemId: itemId)
        XCTAssertFalse(try vault.getItem(vaultId: vaultId, itemId: itemId).deleted)
    }

    func testTotpAndGenerator() throws {
        let code = try totpCode(secret: "JBSWY3DPEHPK3PXP")
        XCTAssertEqual(code.code.count, 6)
        XCTAssertTrue(code.code.allSatisfy(\.isNumber))
        XCTAssertTrue((1...30).contains(code.secondsRemaining))
        XCTAssertThrowsError(try totpCode(secret: "not base32!!!"))
        XCTAssertEqual(generatePassword(length: 20).count, 20)
    }

    func testMergeKeepsLocalOnEqualClockConflict() throws {
        // Two records with identical clocks and different titles:
        // the desktop rule keeps the local value. The JSON here is
        // the ItemRecord shape (flattened legacy item fields).
        let local = #"{"title":"Local","username":"local-user","updated_at":"1"}"#
        let remote = #"{"title":"Remote","username":"remote-user","updated_at":"1"}"#
        let merged = try mergeItemRecords(localJson: local, remoteJson: remote)
        XCTAssertTrue(merged.contains("\"title\":\"Local\""), merged)
    }

    // MARK: - Shared fixtures (issue #451)

    /// Shared allowed-path table (#452): the bridge's isAllowedPath (core's
    /// single rule) must accept/reject exactly the table's paths.
    func testSharedAllowedPathTable() throws {
        let bundle = Bundle(for: GitpassTests.self)
        let fm = FileManager.default
        let candidates = [
            bundle.url(forResource: "shared", withExtension: nil),
            bundle.resourceURL,
        ].compactMap { $0 }
        guard let dir = candidates.first(where: {
            fm.fileExists(atPath: $0.appendingPathComponent("allowed_paths.json").path)
        }) else {
            XCTFail("allowed_paths.json missing from test bundle resources")
            return
        }
        let data = try Data(contentsOf: dir.appendingPathComponent("allowed_paths.json"))
        let table = try JSONSerialization.jsonObject(with: data) as! [String: Any]
        let allowed = table["allowed"] as! [String]
        let rejected = table["rejected"] as! [String]
        XCTAssertGreaterThanOrEqual(allowed.count, 10)
        XCTAssertGreaterThanOrEqual(rejected.count, 20)
        for path in allowed {
            XCTAssertTrue(isAllowedPath(path: path), "must allow \(path.debugDescription)")
        }
        for path in rejected {
            XCTAssertFalse(isAllowedPath(path: path), "must reject \(path.debugDescription)")
        }
    }

    func testSharedFixtures() throws {
        // Fixtures ship as test-bundle resources (see project.yml).
        // XcodeGen adds the directory as a folder reference, so the
        // bundle holds a `shared/v2…v5` tree; fall back to a flat
        // layout (fixtures copied straight into the bundle root).
        let bundle = Bundle(for: GitpassTests.self)
        let fm = FileManager.default
        let candidates = [
            bundle.url(forResource: "shared", withExtension: nil),
            bundle.resourceURL,
        ].compactMap { $0 }
        guard let fixturesURL = candidates.first(where: {
            fm.fileExists(atPath: $0.appendingPathComponent("v2").path)
        }) else {
            XCTFail("shared fixtures missing from test bundle resources")
            return
        }
        for version in [2, 3, 4, 5] {
            let dir = fixturesURL.appendingPathComponent("v\(version)")
            let work = fm.temporaryDirectory
                .appendingPathComponent("gitpass-fixture-v\(version)-\(UUID().uuidString)")
            try fm.copyItem(at: dir, to: work)
            defer { try? fm.removeItem(at: work) }
            let expectedData = try Data(contentsOf: work.appendingPathComponent("expected.json"))
            let expected = try JSONSerialization.jsonObject(with: expectedData) as! [String: Any]
            // Version is read BEFORE opening: openVault is side-effect-free
            // since #450, but a normal unlock may re-wrap the keyset to the
            // production KDF and bump format.json.
            let formatData = try Data(contentsOf: work.appendingPathComponent("format.json"))
            let format = try JSONSerialization.jsonObject(with: formatData) as! [String: Any]
            XCTAssertEqual(format["version"] as? Int, version)
            let secret = try String(
                contentsOf: work.appendingPathComponent("secret.key.txt"), encoding: .utf8
            ).trimmingCharacters(in: .whitespacesAndNewlines)
            let vault = try openVault(
                root: work.path, password: "fixture-password-451", secretKey: secret
            )
            var vaults: [[String: Any]] = []
            for info in try vault.listVaults() {
                var items: [[String: Any]] = []
                for summary in try vault.listItems(vaultId: info.id) {
                    let recordJSON = try vault.recordJson(vaultId: info.id, itemId: summary.id)
                    var record = try JSONSerialization.jsonObject(
                        with: Data(recordJSON.utf8)) as! [String: Any]
                    stripSharedFixtureRecord(&record)
                    items.append(record)
                }
                items.sort { ($0["title"] as? String ?? "") < ($1["title"] as? String ?? "") }
                var v: [String: Any] = [
                    "name": info.name,
                    "description": info.description ?? NSNull(),
                    "items": items,
                ]
                if let icon = info.icon { v["icon"] = icon }
                if let color = info.color { v["color"] = color }
                vaults.append(v)
            }
            vaults.sort { ($0["name"] as? String ?? "") < ($1["name"] as? String ?? "") }
            // The bridge has no generator-history export, so that
            // section is covered by the core test only.
            var want = expected
            want.removeValue(forKey: "generator_history")
            let got: [String: Any] = ["format_version": format["version"] as Any, "vaults": vaults]
            XCTAssertEqual(
                try canonicalJSON(got), try canonicalJSON(want),
                "v\(version) decrypted content via the bridge"
            )
        }
    }

    // MARK: - Keychain (simulator-safe, no biometry)

    func testKeychainTokenRoundTrip() throws {
        let store = KeychainStore.shared
        defer { store.deleteGitHubToken() }
        try store.saveGitHubToken("test-token-123")
        XCTAssertEqual(store.loadGitHubToken(), "test-token-123")
        store.deleteGitHubToken()
        XCTAssertNil(store.loadGitHubToken())
    }

    func testKeychainCredentialsRoundTripWithoutBiometry() throws {
        let store = KeychainStore.shared
        defer { store.deleteUnlockCredentials() }
        try store.saveUnlockCredentials(
            UnlockCredentials(password: "pw", secretKey: "A3-TEST"),
            requireBiometry: false
        )
        let loaded = try store.loadUnlockCredentials()
        XCTAssertEqual(loaded.password, "pw")
        XCTAssertEqual(loaded.secretKey, "A3-TEST")
    }

    // MARK: - Sync service (stubbed network, no real GitHub calls)

    func testDeviceCodeParsing() async throws {
        let stub = StubHTTPSession()
        stub.handler = { request in
            XCTAssertEqual(request.url?.absoluteString, "https://github.com/login/device/code")
            XCTAssertEqual(request.httpMethod, "POST")
            let json = """
            {"device_code":"dev-1","user_code":"ABCD-EFGH","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}
            """
            return (Data(json.utf8), httpResponse(url: request.url!, status: 200))
        }
        let service = GitHubSyncService(session: stub)
        let code = try await service.requestDeviceCode(clientId: "test-client")
        XCTAssertEqual(code.deviceCode, "dev-1")
        XCTAssertEqual(code.userCode, "ABCD-EFGH")
        XCTAssertEqual(code.interval, 5)
    }

    func testGitBlobSHAEmpty() {
        // The well-known git hash of an empty blob.
        XCTAssertEqual(
            GitHubSyncService.gitBlobSHA(for: Data()),
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
        )
    }

    func testSyncablePaths() {
        XCTAssertFalse(GitHubSyncService.isSyncable(path: ".gitpass-state.json"))
        XCTAssertFalse(GitHubSyncService.isSyncable(path: ".gitpass-device.json"))
        XCTAssertFalse(GitHubSyncService.isSyncable(path: ".git/config"))
        XCTAssertFalse(GitHubSyncService.isSyncable(path: "../escape.json"))
        XCTAssertFalse(GitHubSyncService.isSyncable(path: "items/../../escape.json"))
        XCTAssertFalse(GitHubSyncService.isSyncable(path: "/etc/passwd"))
        XCTAssertTrue(GitHubSyncService.isSyncable(path: "format.json"))
        XCTAssertTrue(GitHubSyncService.isSyncable(path: "vaults/abc/items/xyz.enc"))
    }
}

// MARK: - Shared fixture helpers (issue #451)

/// Recursively drop every volatile field from a parsed JSON value,
/// mirroring `gitpass_core::test_fixtures::semantic_record`: wall-clock
/// timestamps, merge clocks, random attachment ids; `deleted_at`
/// becomes an explicit `deleted` boolean.
private func stripSharedFixtureVolatile(_ value: inout Any) {
    if var obj = value as? [String: Any] {
        for key in ["updated_at", "created_at", "clocks", "xclocks", "replaced_at"] {
            obj.removeValue(forKey: key)
        }
        if let deletedAt = obj.removeValue(forKey: "deleted_at") {
            obj["deleted"] = !(deletedAt is NSNull)
        }
        if var atts = obj["attachments"] as? [[String: Any]] {
            for i in atts.indices { atts[i].removeValue(forKey: "id") }
            obj["attachments"] = atts
        }
        for k in Array(obj.keys) {
            if var v = obj[k] {
                stripSharedFixtureVolatile(&v)
                obj[k] = v
            }
        }
        value = obj
    } else if var arr = value as? [Any] {
        for i in arr.indices { stripSharedFixtureVolatile(&arr[i]) }
        value = arr
    }
}

private func stripSharedFixtureRecord(_ record: inout [String: Any]) {
    var any: Any = record
    stripSharedFixtureVolatile(&any)
    record = any as! [String: Any]
    if record["deleted"] == nil { record["deleted"] = false }
}

/// Canonical JSON bytes: object keys sorted, arrays in order.
private func canonicalJSON(_ value: Any) throws -> Data {
    try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
}
