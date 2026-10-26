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
