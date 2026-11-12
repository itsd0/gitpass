import CryptoKit
import XCTest

// iOS two-way sync on the shared core engine (#456). The app's real
// `GitHubHTTPSTransport` talks to an in-process fake of the GitHub Git Data
// API (`FakeGitHub`, an `HTTPExchange`), so the merge, delete propagation,
// rollback and first-pull checks are core's, exercised through the bridge.
// No network, never a real repository, a fake token, temp-dir vaults with
// fake data. SHA-1 below is only the fake server's git object ids.

private let fakeToken = "github_pat_FAKE_TEST_TOKEN_0123456789"
private let fakePassword = "FAKE correct horse battery staple"
private let target = SyncTarget.validated(owner: "o", repo: "vault", branch: "main")!

private func sha1Hex(_ data: Data) -> String {
    Insecure.SHA1.hash(data: data).map { String(format: "%02x", $0) }.joined()
}

private func fakeBlobId(_ data: Data) -> String {
    var d = Data("blob \(data.count)\0".utf8)
    d.append(data)
    return sha1Hex(d)
}

private func rawSHA(_ hex: String) -> Data {
    var out = Data()
    var i = hex.startIndex
    while i < hex.endIndex {
        let j = hex.index(i, offsetBy: 2)
        out.append(UInt8(hex[i..<j], radix: 16)!)
        i = j
    }
    return out
}

/// git's tree id for (path, blob id) pairs (nested directories).
private func gitTreeId(_ entries: [(String, String)]) -> String {
    final class Dir {
        var files: [String: String] = [:]
        var dirs: [String: Dir] = [:]
    }
    let root = Dir()
    for (path, sha) in entries {
        let parts = path.split(separator: "/").map(String.init)
        var d = root
        for p in parts.dropLast() {
            if d.dirs[p] == nil { d.dirs[p] = Dir() }
            d = d.dirs[p]!
        }
        d.files[parts.last!] = sha
    }
    func hash(_ d: Dir) -> Data {
        var named: [([UInt8], Data)] = []
        for (n, sha) in d.files {
            var e = Data("100644 \(n)\0".utf8)
            e.append(rawSHA(sha))
            named.append((Array(n.utf8), e))
        }
        for (n, sub) in d.dirs {
            var e = Data("40000 \(n)\0".utf8)
            e.append(hash(sub))
            named.append((Array(n.utf8) + [UInt8(ascii: "/")], e))
        }
        named.sort { $0.0.lexicographicallyPrecedes($1.0) }
        let body = named.reduce(into: Data()) { $0.append($1.1) }
        var obj = Data("tree \(body.count)\0".utf8)
        obj.append(body)
        return Data(Insecure.SHA1.hash(data: obj))
    }
    return hash(root).map { String(format: "%02x", $0) }.joined()
}

/// In-process fake of the GitHub Git Data API for repo `o/vault`, branch
/// `main`, behind the app's real transport.
final class FakeGitHub: HTTPExchange {
    private let lock = NSLock()
    var blobs: [String: Data] = [:]
    var trees: [String: [(String, String)]] = [:]
    var commits: [String: (tree: String, parents: [String])] = [:]
    var head: String?
    var counter = 0
    var requests: [String] = []
    var statusOverride: Int?
    var failure: ExchangeError?

    func withLock<T>(_ body: () -> T) -> T {
        lock.lock(); defer { lock.unlock() }
        return body()
    }

    func newCommit(tree: String, parents: [String]) -> String {
        counter += 1
        let sha = sha1Hex(Data("commit \(tree) \(parents) \(counter)".utf8))
        commits[sha] = (tree, parents)
        return sha
    }

    private func isAncestor(_ anc: String, of: String) -> Bool {
        var stack = [of]
        while let c = stack.popLast() {
            if c == anc { return true }
            stack.append(contentsOf: commits[c]?.parents ?? [])
        }
        return false
    }

    private func json(_ status: Int, _ value: Any) -> (status: Int, headers: [String: String], body: Data) {
        (status, [:], try! JSONSerialization.data(withJSONObject: value))
    }

    private func status(_ code: Int) -> (status: Int, headers: [String: String], body: Data) {
        // Noisy body: it must never reach an error message.
        json(code, ["message": "BODY_SHOULD_NOT_LEAK"])
    }

    func perform(
        _ request: URLRequest, maxBytes: Int, isCancelled: @escaping () -> Bool
    ) throws -> (status: Int, headers: [String: String], body: Data) {
        lock.lock(); defer { lock.unlock() }
        let url = request.url!
        XCTAssertEqual(url.scheme, "https")
        XCTAssertEqual(url.host, "api.github.com")
        XCTAssertFalse(url.absoluteString.contains("FAKE_TEST_TOKEN"), "token must never be in a URL")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer \(fakeToken)")
        XCTAssertEqual(request.cachePolicy, .reloadIgnoringLocalAndRemoteCacheData)
        XCTAssertFalse(request.httpShouldHandleCookies)
        XCTAssertGreaterThan(maxBytes, 0)
        let method = request.httpMethod ?? "GET"
        let full = String(url.absoluteString.dropFirst("https://api.github.com".count))
        requests.append("\(method) \(full)")
        if let failure { throw failure }
        if let code = statusOverride { return status(code) }
        guard full.hasPrefix("/repos/o/vault") else {
            XCTFail("fixed test repo only: \(full)")
            return status(404)
        }
        let path = String(full.dropFirst("/repos/o/vault".count))
        let body = request.httpBody.flatMap { try? JSONSerialization.jsonObject(with: $0) } as? [String: Any] ?? [:]
        let empty = commits.isEmpty
        switch (method, path) {
        case ("GET", ""):
            return json(200, ["full_name": "o/vault"])
        case ("GET", "/git/ref/heads/main"):
            if let head { return json(200, ["ref": "refs/heads/main", "object": ["type": "commit", "sha": head]]) }
            return status(empty ? 409 : 404)
        case ("GET", let p) where p.hasPrefix("/git/commits/"):
            let sha = String(p.dropFirst("/git/commits/".count))
            guard let c = commits[sha] else { return status(404) }
            return json(200, ["sha": sha, "tree": ["sha": c.tree], "parents": c.parents.map { ["sha": $0] }])
        case ("GET", let p) where p.hasPrefix("/git/trees/"):
            let rest = String(p.dropFirst("/git/trees/".count))
            XCTAssertTrue(rest.hasSuffix("?recursive=1"))
            let sha = String(rest.prefix { $0 != "?" })
            guard let entries = trees[sha] else { return status(404) }
            var dirs = Set<String>()
            var out: [[String: Any]] = []
            for (path, blob) in entries {
                let parts = path.split(separator: "/").map(String.init)
                var acc = ""
                for c in parts.dropLast() {
                    acc = acc.isEmpty ? c : acc + "/" + c
                    dirs.insert(acc)
                }
                out.append(["path": path, "mode": "100644", "type": "blob", "sha": blob, "size": blobs[blob]!.count])
            }
            for d in dirs.sorted() {
                out.append(["path": d, "mode": "040000", "type": "tree", "sha": fakeBlobId(Data(d.utf8))])
            }
            return json(200, ["sha": sha, "truncated": false, "tree": out])
        case ("GET", let p) where p.hasPrefix("/git/blobs/"):
            let sha = String(p.dropFirst("/git/blobs/".count))
            guard let b = blobs[sha] else { return status(404) }
            return json(200, ["sha": sha, "encoding": "base64", "size": b.count,
                              "content": b.base64EncodedString(options: [.lineLength64Characters, .endLineWithLineFeed])])
        case ("POST", "/git/blobs"):
            if empty { return status(409) }
            let bytes = Data(base64Encoded: body["content"] as! String)!
            let sha = fakeBlobId(bytes)
            blobs[sha] = bytes
            return json(201, ["sha": sha])
        case ("POST", "/git/trees"):
            var pairs: [(String, String)] = []
            for e in body["tree"] as! [[String: Any]] {
                XCTAssertEqual(e["mode"] as? String, "100644")
                let sha = e["sha"] as! String
                guard blobs[sha] != nil else { return status(422) }
                pairs.append((e["path"] as! String, sha))
            }
            let id = gitTreeId(pairs)
            trees[id] = pairs
            return json(201, ["sha": id])
        case ("POST", "/git/commits"):
            let tree = body["tree"] as! String
            let parents = body["parents"] as? [String] ?? []
            guard trees[tree] != nil, parents.allSatisfy({ commits[$0] != nil }) else { return status(422) }
            let sha = newCommit(tree: tree, parents: parents)
            return json(201, ["sha": sha, "tree": ["sha": tree], "parents": parents.map { ["sha": $0] }])
        case ("PUT", "/contents/format.json"):
            if head != nil { return status(422) }
            let bytes = Data(base64Encoded: body["content"] as! String)!
            let blob = fakeBlobId(bytes)
            blobs[blob] = bytes
            let pairs = [("format.json", blob)]
            let tree = gitTreeId(pairs)
            trees[tree] = pairs
            let c = newCommit(tree: tree, parents: [])
            head = c
            return json(201, ["content": ["sha": blob], "commit": ["sha": c]])
        case ("PATCH", "/git/refs/heads/main"):
            XCTAssertEqual(body["force"] as? Bool, false, "never force")
            let new = body["sha"] as! String
            guard let cur = head, commits[new] != nil, isAncestor(cur, of: new) else { return status(422) }
            head = new
            return json(200, ["object": ["sha": new]])
        default:
            XCTFail("unexpected request \(method) \(path)")
            return status(404)
        }
    }
}

final class SyncEngineTests: XCTestCase {
    private var dirs: [URL] = []

    override func tearDown() {
        for d in dirs { try? FileManager.default.removeItem(at: d) }
        dirs = []
        super.tearDown()
    }

    private func tempDir() -> URL {
        let d = FileManager.default.temporaryDirectory.appendingPathComponent("gitpass-sync-\(UUID().uuidString)")
        dirs.append(d)
        return d
    }

    private func service(_ gh: FakeGitHub) -> GitHubSyncService {
        GitHubSyncService { token, cancellation in
            GitHubHTTPSTransport(token: token, cancellation: cancellation, exchange: gh)
        }
    }

    private func login(_ title: String, _ password: String) -> ItemInput {
        ItemInput(title: title, username: "user", password: password, url: nil, notes: nil, totp: nil,
                  category: "login", tags: [], favorite: false, sections: [])
    }

    private func sync(_ v: Vault, _ gh: FakeGitHub) throws -> GitHubSyncReport {
        try service(gh).sync(vault: v, target: target, token: fakeToken, cancellation: SyncCancellation())
    }

    private func assertFixedText(_ error: Error, file: StaticString = #filePath, line: UInt = #line) -> String {
        let m = syncErrorMessage(error)
        XCTAssertFalse(m.contains("FAKE_TEST_TOKEN"), m, file: file, line: line)
        XCTAssertFalse(m.contains("BODY_SHOULD_NOT_LEAK"), m, file: file, line: line)
        XCTAssertFalse(m.contains(NSTemporaryDirectory()), m, file: file, line: line)
        XCTAssertFalse(m.contains("FAKE correct horse"), m, file: file, line: line)
        return m
    }

    private struct Setup {
        let gh: FakeGitHub
        let a: URL, b: URL
        let secret: String
        let va: Vault, vb: Vault
        let vid: String, one: String, two: String
    }

    /// Device A (fresh account, two items) initializes the empty fake repo;
    /// device B is a first pull of it (core download + full authentication).
    private func setup() throws -> Setup {
        let gh = FakeGitHub()
        let a = tempDir().appendingPathComponent("vault")
        let secret = try createAccount(root: a.path, password: fakePassword)
        let va = try openVault(root: a.path, password: fakePassword, secretKey: secret)
        let vid = try va.createVault(name: "Personal", description: nil)
        let one = try va.createItem(vaultId: vid, input: login("One", "pw-one"))
        let two = try va.createItem(vaultId: vid, input: login("Two", "pw-two"))
        XCTAssertTrue(try sync(va, gh).pushed, "first sync initializes the empty repository")

        let b = tempDir().appendingPathComponent("vault")
        let writes = gh.withLock { gh.requests.filter { !$0.hasPrefix("GET") }.count }
        try service(gh).firstPull(localRoot: b, target: target, token: fakeToken,
                                  password: fakePassword, secretKey: secret, cancellation: SyncCancellation())
        XCTAssertEqual(gh.withLock { gh.requests.filter { !$0.hasPrefix("GET") }.count }, writes, "first pull is read-only")
        XCTAssertTrue(FileManager.default.fileExists(atPath: b.appendingPathComponent("format.json").path))
        let staging = try FileManager.default.contentsOfDirectory(atPath: b.deletingLastPathComponent().path)
        XCTAssertEqual(staging, ["vault"], "no staging copy left behind")
        let vb = try openVault(root: b.path, password: fakePassword, secretKey: secret)
        return Setup(gh: gh, a: a, b: b, secret: secret, va: va, vb: vb, vid: vid, one: one, two: two)
    }

    func testMergeAndDeletePropagationThroughTheEngine() throws {
        let s = try setup()
        let headBefore = s.gh.withLock { s.gh.head }
        let first = try sync(s.vb, s.gh)
        XCTAssertFalse(first.pushed || first.localChanged, "a first-pulled copy is already in sync")
        XCTAssertTrue(first.conflicts.isEmpty)
        XCTAssertEqual(s.gh.withLock { s.gh.head }, headBefore)

        // Concurrent edits on both devices: A edits One; B trashes Two and adds Three.
        try s.va.updateItem(vaultId: s.vid, itemId: s.one, input: login("One", "pw-one-desk"))
        try s.vb.deleteItem(vaultId: s.vid, itemId: s.two)
        let three = try s.vb.createItem(vaultId: s.vid, input: login("Three", "pw-three"))
        XCTAssertTrue(try sync(s.va, s.gh).pushed)
        let r = try sync(s.vb, s.gh)
        XCTAssertTrue(r.pushed && r.localChanged, "\(r)")
        XCTAssertTrue(r.conflicts.isEmpty, "\(r.conflicts)")
        XCTAssertEqual(try s.vb.getItem(vaultId: s.vid, itemId: s.one).password, "pw-one-desk")

        let ra = try sync(s.va, s.gh)
        XCTAssertTrue(ra.localChanged && !ra.pushed, "\(ra)")
        XCTAssertTrue(try s.va.getItem(vaultId: s.vid, itemId: s.two).deleted, "delete propagated to A")
        XCTAssertEqual(try s.va.getItem(vaultId: s.vid, itemId: three).password, "pw-three")

        // Restore on A propagates back to B.
        try s.va.restoreItem(vaultId: s.vid, itemId: s.two)
        XCTAssertTrue(try sync(s.va, s.gh).pushed)
        XCTAssertTrue(try sync(s.vb, s.gh).localChanged)
        XCTAssertFalse(try s.vb.getItem(vaultId: s.vid, itemId: s.two).deleted)

        // Report lines carry ids/kinds only, never item data.
        for line in r.conflicts { XCTAssertFalse(line.contains("pw-")) }
    }

    func testRolledBackRemoteIsRefusedAndNothingChanges() throws {
        let s = try setup()
        _ = try sync(s.vb, s.gh)
        let old = s.gh.withLock { s.gh.head! }
        try s.va.updateItem(vaultId: s.vid, itemId: s.one, input: login("One", "pw-newer"))
        XCTAssertTrue(try sync(s.va, s.gh).pushed)
        XCTAssertTrue(try sync(s.vb, s.gh).localChanged)
        let newer = s.gh.withLock { s.gh.head! }

        // Someone resets the branch to the older commit.
        s.gh.withLock { s.gh.head = old }
        XCTAssertThrowsError(try sync(s.vb, s.gh)) { error in
            let m = assertFixedText(error)
            XCTAssertTrue(m.contains("rollback") && m.contains("refusing"), m)
        }
        XCTAssertEqual(s.gh.withLock { s.gh.head }, old, "nothing pushed over the rolled-back branch")
        XCTAssertEqual(try s.vb.getItem(vaultId: s.vid, itemId: s.one).password, "pw-newer", "local unchanged")
        XCTAssertNotEqual(old, newer)
    }

    func testForeignFileOnRemoteAndErrorsAreFixedText() throws {
        let s = try setup()
        // HTTP and transport failures: fixed text, nothing half-applied.
        for (status, needle) in [(401, "token"), (404, "access")] {
            s.gh.withLock { s.gh.statusOverride = status }
            XCTAssertThrowsError(try sync(s.vb, s.gh)) { XCTAssertTrue(assertFixedText($0).contains(needle)) }
        }
        s.gh.withLock { s.gh.statusOverride = nil; s.gh.failure = .redirect }
        XCTAssertThrowsError(try sync(s.vb, s.gh)) { XCTAssertTrue(assertFixedText($0).contains("redirect")) }
        s.gh.withLock { s.gh.failure = .transport }
        XCTAssertThrowsError(try sync(s.vb, s.gh)) { XCTAssertTrue(assertFixedText($0).contains("network")) }
        s.gh.withLock { s.gh.failure = nil }

        // A file outside the vault allowlist on the remote is refused.
        s.gh.withLock {
            let head = s.gh.head!
            var pairs = s.gh.trees[s.gh.commits[head]!.tree]!
            let blob = fakeBlobId(Data("hello".utf8))
            s.gh.blobs[blob] = Data("hello".utf8)
            pairs.append(("notes.txt", blob))
            let t = gitTreeId(pairs)
            s.gh.trees[t] = pairs
            s.gh.head = s.gh.newCommit(tree: t, parents: [head])
        }
        let before = try s.vb.listItems(vaultId: s.vid).count
        XCTAssertThrowsError(try sync(s.vb, s.gh)) { XCTAssertTrue(assertFixedText($0).contains("refusing")) }
        XCTAssertEqual(try s.vb.listItems(vaultId: s.vid).count, before)
        // First pull of that remote is refused too and writes nothing.
        let c = tempDir().appendingPathComponent("vault")
        XCTAssertThrowsError(try service(s.gh).firstPull(
            localRoot: c, target: target, token: fakeToken, password: fakePassword,
            secretKey: s.secret, cancellation: SyncCancellation()
        )) { XCTAssertTrue(assertFixedText($0).contains("refusing")) }
        XCTAssertFalse(FileManager.default.fileExists(atPath: c.path))
    }

    func testFirstPullNeverReplacesAVaultAndChecksCredentials() throws {
        let s = try setup()
        let manifest = try Data(contentsOf: s.b.appendingPathComponent("manifest.enc"))
        XCTAssertThrowsError(try service(s.gh).firstPull(
            localRoot: s.b, target: target, token: fakeToken, password: fakePassword,
            secretKey: s.secret, cancellation: SyncCancellation()
        ))
        XCTAssertEqual(try Data(contentsOf: s.b.appendingPathComponent("manifest.enc")), manifest)

        // Wrong master password: the downloaded copy is discarded.
        let c = tempDir().appendingPathComponent("vault")
        XCTAssertThrowsError(try service(s.gh).firstPull(
            localRoot: c, target: target, token: fakeToken, password: "FAKE wrong password",
            secretKey: s.secret, cancellation: SyncCancellation()
        )) { _ = assertFixedText($0) }
        XCTAssertFalse(FileManager.default.fileExists(atPath: c.path))
        let parent = c.deletingLastPathComponent()
        XCTAssertEqual((try? FileManager.default.contentsOfDirectory(atPath: parent.path)) ?? [], [])

        // Cancelled before start: no request at all.
        let n = s.gh.withLock { s.gh.requests.count }
        let cancel = SyncCancellation()
        cancel.cancel()
        XCTAssertThrowsError(try service(s.gh).firstPull(
            localRoot: c, target: target, token: fakeToken, password: fakePassword,
            secretKey: s.secret, cancellation: cancel
        ))
        XCTAssertEqual(s.gh.withLock { s.gh.requests.count }, n)
        XCTAssertFalse(FileManager.default.fileExists(atPath: c.path))
    }
}

// MARK: - Transport, settings and coordinator (no vault crypto needed)

final class RecordingExchange: HTTPExchange {
    var requests: [URLRequest] = []
    var result: Result<(status: Int, headers: [String: String], body: Data), ExchangeError> =
        .success((200, ["x-ratelimit-remaining": "42", "retry-after": "7"], Data("{}".utf8)))

    func perform(
        _ request: URLRequest, maxBytes: Int, isCancelled: @escaping () -> Bool
    ) throws -> (status: Int, headers: [String: String], body: Data) {
        requests.append(request)
        return try result.get()
    }
}

final class MemoryTokens: SyncTokenStore {
    var token: String?
    func saveGitHubToken(_ token: String) throws { self.token = token }
    func loadGitHubToken() -> String? { token }
    func deleteGitHubToken() { token = nil }
}

final class SyncTransportTests: XCTestCase {
    private func request(_ path: String, method: String = "GET", headers: [HttpHeader] = [], max: UInt64 = 1000) -> HttpTransportRequest {
        HttpTransportRequest(method: method, path: path, headers: headers, body: nil, maxResponseBytes: max, timeoutMs: 5000)
    }

    func testTokenOnlyInTheAuthorizationHeaderAtAFixedOrigin() throws {
        let ex = RecordingExchange()
        let t = try XCTUnwrap(GitHubHTTPSTransport(token: fakeToken, cancellation: SyncCancellation(), exchange: ex))
        XCTAssertFalse("\(t)".contains("FAKE_TEST_TOKEN"))
        let resp = try t.send(request: request("/repos/o/vault/git/trees/abc?recursive=1", headers: [
            HttpHeader(name: "Accept", value: "application/vnd.github+json"),
            HttpHeader(name: "authorization", value: "Bearer smuggled"),
        ]))
        XCTAssertEqual(resp.status, 200)
        XCTAssertEqual(resp.rateLimitRemaining, 42)
        XCTAssertEqual(resp.retryAfter, 7)
        let sent = try XCTUnwrap(ex.requests.first)
        XCTAssertEqual(sent.url?.absoluteString, "https://api.github.com/repos/o/vault/git/trees/abc?recursive=1")
        XCTAssertEqual(sent.value(forHTTPHeaderField: "Authorization"), "Bearer \(fakeToken)")
        XCTAssertEqual(sent.value(forHTTPHeaderField: "Accept"), "application/vnd.github+json")
        XCTAssertEqual(sent.cachePolicy, .reloadIgnoringLocalAndRemoteCacheData)
        XCTAssertFalse(sent.httpShouldHandleCookies)
    }

    func testRefusesBadInputBeforeAnyRequest() {
        XCTAssertNil(GitHubHTTPSTransport(token: "ghp_classic_token_0123456789", cancellation: SyncCancellation()))
        XCTAssertNil(GitHubHTTPSTransport(token: "github_pat_ has space 0123456789", cancellation: SyncCancellation()))
        let ex = RecordingExchange()
        let t = GitHubHTTPSTransport(token: fakeToken, cancellation: SyncCancellation(), exchange: ex)!
        for path in ["/user", "/repos/../x", "/repos/o//x", "/repos/o@evil.example/x", "https://evil.example/repos/x", "/repos/o/x#f", "/repos/o/x\\y"] {
            XCTAssertThrowsError(try t.send(request: request(path))) { XCTAssertEqual($0 as? HttpTransportError, .Transport, path) }
        }
        XCTAssertThrowsError(try t.send(request: request("/repos/o/x", method: "DELETE")))
        XCTAssertThrowsError(try t.send(request: request("/repos/o/x", headers: [HttpHeader(name: "X-Bad\r\nInjected", value: "1")])))
        XCTAssertTrue(ex.requests.isEmpty)
    }

    func testFailuresMapToFixedTransportErrors() {
        let ex = RecordingExchange()
        let cancel = SyncCancellation()
        let t = GitHubHTTPSTransport(token: fakeToken, cancellation: cancel, exchange: ex)!
        let cases: [(ExchangeError, HttpTransportError)] = [
            (.redirect, .Redirect), (.tooLarge, .TooLarge), (.timeout, .Timeout), (.transport, .Transport), (.cancelled, .Cancelled),
        ]
        for (from, to) in cases {
            ex.result = .failure(from)
            XCTAssertThrowsError(try t.send(request: request("/repos/o/x"))) { XCTAssertEqual($0 as? HttpTransportError, to) }
        }
        ex.result = .success((200, [:], Data(count: 2000)))
        XCTAssertThrowsError(try t.send(request: request("/repos/o/x", max: 1000))) { XCTAssertEqual($0 as? HttpTransportError, .TooLarge) }
        ex.result = .success((302, [:], Data()))
        XCTAssertEqual(try t.send(request: request("/repos/o/x")).status, 302, "3xx goes back to core, never followed")

        let n = ex.requests.count
        cancel.cancel()
        XCTAssertThrowsError(try t.send(request: request("/repos/o/x"))) { XCTAssertEqual($0 as? HttpTransportError, .Cancelled) }
        let t2 = GitHubHTTPSTransport(token: fakeToken, cancellation: SyncCancellation(), exchange: ex)!
        t2.close()
        XCTAssertThrowsError(try t2.send(request: request("/repos/o/x"))) { XCTAssertEqual($0 as? HttpTransportError, .Cancelled) }
        XCTAssertEqual(ex.requests.count, n)
    }

    func testTargetValidation() {
        XCTAssertEqual(SyncTarget.validated(owner: " o ", repo: "vault ", branch: "main")?.display, "o/vault (main)")
        XCTAssertNotNil(SyncTarget.validated(owner: "my-org", repo: "my.vault_1", branch: "feature/x"))
        for (o, r, b) in [("", "v", "main"), ("o/x", "v", "main"), ("o", "..", "main"), ("o", "v/x", "main"),
                          ("o", "v", ""), ("o", "v", "a..b"), ("o", "v", "a b"), ("o", "v", "x.lock"), ("-o", "v", "main")] {
            XCTAssertNil(SyncTarget.validated(owner: o, repo: r, branch: b), "\(o) \(r) \(b)")
        }
    }

    func testSettingsKeepTheTokenOutOfUserDefaults() throws {
        let suite = "gitpass-sync-test-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let tokens = MemoryTokens()
        tokens.token = "gho_legacy_style_token_0123"
        let settings = SyncSettingsStore(defaults: defaults, tokens: tokens)
        XCTAssertNil(tokens.token, "an earlier non-PAT token is dropped")
        XCTAssertNil(settings.target, "no default repository")
        XCTAssertFalse(settings.isConfigured)

        XCTAssertThrowsError(try settings.save(target: target, token: "")) { XCTAssertEqual($0 as? SyncSettingsError, .missingToken) }
        XCTAssertThrowsError(try settings.save(target: target, token: "ghp_classic_0123456789abc")) { XCTAssertEqual($0 as? SyncSettingsError, .invalidToken) }
        try settings.save(target: target, token: " \(fakeToken) ")
        XCTAssertEqual(settings.target, target)
        XCTAssertEqual(settings.token(), fakeToken)
        XCTAssertTrue(settings.isConfigured)
        let stored = defaults.persistentDomain(forName: suite) ?? [:]
        XCTAssertFalse(stored.values.contains { "\($0)".contains("FAKE_TEST_TOKEN") }, "token only in the token store")

        try settings.save(target: target, token: "")   // keeps the saved token
        XCTAssertEqual(settings.token(), fakeToken)
        settings.recordSuccess(Date(timeIntervalSince1970: 1000))
        XCTAssertEqual(settings.lastSuccess, Date(timeIntervalSince1970: 1000))
        settings.clear()
        XCTAssertNil(settings.target)
        XCTAssertNil(tokens.token)
        XCTAssertNil(settings.lastSuccess)
    }
}

final class FakeEngine: SyncEngine {
    var result: Result<GitHubSyncReport, Error> = .success(
        GitHubSyncReport(head: "h", pushed: true, localChanged: true, conflicts: ["v1/i1: edit vs delete, kept local"],
                         mergedItems: 1, localOnlyItems: 0, attempts: 1)
    )
    var calls = 0
    var gate: DispatchSemaphore?
    var sawCancel = false

    func sync(vault: Vault, target: SyncTarget, token: String, cancellation: SyncCancellation) throws -> GitHubSyncReport {
        calls += 1
        if let gate {
            gate.wait()
            sawCancel = cancellation.isCancelled()
        }
        return try result.get()
    }
}

@MainActor
final class SyncCoordinatorTests: XCTestCase {
    func testStatusConflictsFailuresAndLockCancels() async throws {
        let suite = "gitpass-sync-test-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SyncSettingsStore(defaults: defaults, tokens: MemoryTokens())
        let engine = FakeEngine()
        let coordinator = SyncCoordinator(settings: settings, engine: engine, debounce: 3600, period: 3600)
        XCTAssertEqual(coordinator.phase, .notConfigured)

        let root = FileManager.default.temporaryDirectory.appendingPathComponent("gitpass-sync-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let secret = try createAccount(root: root.appendingPathComponent("vault").path, password: fakePassword)
        let vault = try openVault(root: root.appendingPathComponent("vault").path, password: fakePassword, secretKey: secret)
        var unlocked: Vault? = vault
        var localChanges = 0
        coordinator.vaultProvider = { unlocked }
        coordinator.onLocalChange = { localChanges += 1 }

        await coordinator.syncNow()
        XCTAssertEqual(engine.calls, 0, "not configured: no sync")
        try settings.save(target: target, token: fakeToken)
        coordinator.settingsChanged()
        XCTAssertEqual(coordinator.phase, .idle)

        await coordinator.syncNow()
        XCTAssertEqual(engine.calls, 1)
        XCTAssertEqual(coordinator.phase, .idle)
        XCTAssertEqual(coordinator.lastConflicts, ["v1/i1: edit vs delete, kept local"])
        XCTAssertNotNil(coordinator.lastSuccess)
        XCTAssertEqual(localChanges, 1)

        engine.result = .failure(GitpassError.Generic("GitHub rejected the token"))
        await coordinator.syncNow()
        XCTAssertEqual(coordinator.phase, .failed("GitHub rejected the token"))
        engine.result = .failure(URLError(.notConnectedToInternet))
        await coordinator.syncNow()
        XCTAssertEqual(coordinator.phase, .failed(genericSyncFailure))

        // Lock during a sync cancels it and drops its result.
        engine.result = .success(GitHubSyncReport(head: "h2", pushed: false, localChanged: true, conflicts: [],
                                                  mergedItems: 0, localOnlyItems: 0, attempts: 1))
        let gate = DispatchSemaphore(value: 0)
        engine.gate = gate
        let running = Task { await coordinator.syncNow() }
        while !coordinator.isSyncing { await Task.yield() }
        XCTAssertEqual(coordinator.phase, .syncing)
        coordinator.locked()
        unlocked = nil
        gate.signal()
        await running.value
        XCTAssertTrue(engine.sawCancel)
        XCTAssertEqual(localChanges, 1, "a cancelled sync's result is dropped")
        XCTAssertNotEqual(coordinator.phase, .syncing)
        await coordinator.syncNow()
        XCTAssertEqual(engine.calls, 4, "locked: no further sync")
    }
}
