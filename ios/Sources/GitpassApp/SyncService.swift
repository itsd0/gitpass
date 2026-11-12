import Foundation

/// GitHub two-way sync for gitpass iOS on the shared core engine (#456).
///
/// The merge, delete propagation, rollback and foreign-file checks, the
/// account/KDF guard and the whole GitHub Git Data API protocol run in
/// `gitpass-core` through the bridge (`Vault.syncGithub`, the same engine the
/// desktop, CLI and Android use). First pull on a new device is
/// `pullGithubSnapshot` (core transport, allowlist and blob-id checks) followed
/// by `validateFirstPullSnapshot` (full authentication with the Master
/// Password and Secret Key) before the snapshot is moved into place. Swift
/// only supplies the HTTPS round trip (`GitHubHTTPSTransport`), stores the
/// non-secret target and the PAT (Keychain), and schedules syncs. Swift never
/// parses GitHub JSON or vault data for sync and implements no crypto.
///
/// There is no default repository and no app-registration flow: the owner, repository,
/// branch and a fine-grained PAT (`github_pat_…`, Contents read/write on that
/// one repository) are entered by the user. The token is never logged,
/// displayed, put in a URL or returned in an error.

// MARK: - Target

/// The GitHub repository and branch holding the vault. Not secret.
struct SyncTarget: Equatable {
    let owner: String
    let repo: String
    let branch: String

    /// Trims and checks the parts (core validates again). `nil` when invalid.
    static func validated(owner: String, repo: String, branch: String) -> SyncTarget? {
        let o = owner.trimmingCharacters(in: .whitespacesAndNewlines)
        let r = repo.trimmingCharacters(in: .whitespacesAndNewlines)
        let b = branch.trimmingCharacters(in: .whitespacesAndNewlines)
        guard validOwner(o), validRepo(r), validBranch(b) else { return nil }
        return SyncTarget(owner: o, repo: r, branch: b)
    }

    var ffi: GitHubSyncTarget { GitHubSyncTarget(owner: owner, repo: repo, branch: branch) }
    var display: String { "\(owner)/\(repo) (\(branch))" }

    private static func ascii(_ s: String, _ allowed: (Character) -> Bool) -> Bool {
        s.allSatisfy { $0.isASCII && allowed($0) }
    }

    static func validOwner(_ s: String) -> Bool {
        (1...39).contains(s.count) && !s.hasPrefix("-")
            && ascii(s) { $0.isLetter || $0.isNumber || $0 == "-" }
    }

    static func validRepo(_ s: String) -> Bool {
        (1...100).contains(s.count) && s != "." && s != ".."
            && ascii(s) { $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_" || $0 == "." }
    }

    static func validBranch(_ s: String) -> Bool {
        guard (1...255).contains(s.count), !s.hasPrefix("/"), !s.hasSuffix("/"),
              !s.hasPrefix("-"), !s.hasSuffix("."), !s.hasSuffix(".lock"),
              !s.contains(".."), !s.contains("//"), !s.contains("@{")
        else { return false }
        return ascii(s) { c in
            guard let v = c.asciiValue, v > 32, v < 127 else { return false }
            return !"~^:?*[\\".contains(c)
        }
    }
}

/// Fine-grained PATs only (core enforces the same); printable ASCII, bounded.
func isValidSyncToken(_ token: String) -> Bool {
    token.hasPrefix("github_pat_") && (20...512).contains(token.count)
        && token.unicodeScalars.allSatisfy { (33...126).contains($0.value) }
}

// MARK: - Settings (target in UserDefaults, token in the Keychain)

protocol SyncTokenStore: AnyObject {
    func saveGitHubToken(_ token: String) throws
    func loadGitHubToken() -> String?
    func deleteGitHubToken()
}

extension KeychainStore: SyncTokenStore {}

enum SyncSettingsError: Error, LocalizedError, Equatable {
    case invalidTarget
    case invalidToken
    case missingToken

    var errorDescription: String? {
        switch self {
        case .invalidTarget: return String(localized: "Enter a valid owner, repository and branch.")
        case .invalidToken: return String(localized: "Use a fine-grained personal access token (starts with github_pat_).")
        case .missingToken: return String(localized: "Enter a fine-grained personal access token.")
        }
    }
}

final class SyncSettingsStore {
    private let defaults: UserDefaults
    private let tokens: SyncTokenStore

    static let ownerKey = "gitpass.sync.owner"
    static let repoKey = "gitpass.sync.repo"
    static let branchKey = "gitpass.sync.branch"
    static let lastSuccessKey = "gitpass.sync.lastSuccess"

    init(defaults: UserDefaults = .standard, tokens: SyncTokenStore = KeychainStore.shared) {
        self.defaults = defaults
        self.tokens = tokens
        migrateLegacy()
    }

    /// A token saved by an earlier version that is not a fine-grained PAT is
    /// deleted: core refuses it, and the user re-enters the repository and a
    /// PAT explicitly (earlier settings keys are never read).
    private func migrateLegacy() {
        if let token = tokens.loadGitHubToken(), !isValidSyncToken(token) {
            tokens.deleteGitHubToken()
        }
    }

    var target: SyncTarget? {
        guard let o = defaults.string(forKey: Self.ownerKey),
              let r = defaults.string(forKey: Self.repoKey),
              let b = defaults.string(forKey: Self.branchKey)
        else { return nil }
        return SyncTarget.validated(owner: o, repo: r, branch: b)
    }

    var hasToken: Bool { token() != nil }

    /// The saved PAT, only for the transport. Never display or log it.
    func token() -> String? {
        guard let t = tokens.loadGitHubToken(), isValidSyncToken(t) else { return nil }
        return t
    }

    var isConfigured: Bool { target != nil && hasToken }

    /// Saves the target and, when `token` is non-empty, replaces the PAT.
    /// An empty `token` keeps the saved one (the field is never prefilled).
    func save(target: SyncTarget, token: String?) throws {
        let trimmed = token?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if trimmed.isEmpty {
            guard hasToken else { throw SyncSettingsError.missingToken }
        } else {
            guard isValidSyncToken(trimmed) else { throw SyncSettingsError.invalidToken }
            try tokens.saveGitHubToken(trimmed)
        }
        if self.target != target { defaults.removeObject(forKey: Self.lastSuccessKey) }
        defaults.set(target.owner, forKey: Self.ownerKey)
        defaults.set(target.repo, forKey: Self.repoKey)
        defaults.set(target.branch, forKey: Self.branchKey)
    }

    /// Forgets the target, the token and the last-success time.
    func clear() {
        tokens.deleteGitHubToken()
        for key in [Self.ownerKey, Self.repoKey, Self.branchKey, Self.lastSuccessKey] {
            defaults.removeObject(forKey: key)
        }
    }

    var lastSuccess: Date? {
        let t = defaults.double(forKey: Self.lastSuccessKey)
        return t > 0 ? Date(timeIntervalSince1970: t) : nil
    }

    func recordSuccess(_ date: Date = Date()) {
        defaults.set(date.timeIntervalSince1970, forKey: Self.lastSuccessKey)
    }
}

// MARK: - HTTPS transport (the only thing Swift does for sync)

/// One bounded HTTPS exchange. Production is `URLSessionExchange`; tests
/// inject a fake. Implementations must not follow redirects or cache.
protocol HTTPExchange: AnyObject {
    /// Returns status, response headers (lower-cased names) and body.
    func perform(
        _ request: URLRequest, maxBytes: Int, isCancelled: @escaping () -> Bool
    ) throws -> (status: Int, headers: [String: String], body: Data)
}

/// Transport-level failures (mapped to `HttpTransportError`).
enum ExchangeError: Error {
    case timeout, cancelled, tooLarge, redirect, transport
}

/// `HttpTransport` for core's GitHub client: fixed origin
/// `https://api.github.com`, no redirects, no cache, no cookies, bounded
/// reads, cancellation. The token is held only for the `Authorization`
/// header (it is never part of the request record from Rust) and is never
/// logged; errors carry no text from the TLS/HTTP layers.
final class GitHubHTTPSTransport: HttpTransport, @unchecked Sendable, CustomStringConvertible {
    static let origin = "https://api.github.com"
    private static let methods: Set<String> = ["GET", "POST", "PATCH", "PUT"]
    private static let maxResponse = 64 * 1024 * 1024

    private let lock = NSLock()
    private var credential: String?
    private let cancellation: SyncCancellation
    private let exchange: HTTPExchange

    /// `nil` when `token` is not a fine-grained PAT.
    init?(token: String, cancellation: SyncCancellation, exchange: HTTPExchange = URLSessionExchange()) {
        guard isValidSyncToken(token) else { return nil }
        credential = token
        self.cancellation = cancellation
        self.exchange = exchange
    }

    var description: String { "GitHubHTTPSTransport([redacted])" }

    /// Forgets the token; later requests fail as cancelled.
    func close() {
        lock.lock(); credential = nil; lock.unlock()
    }

    private func currentToken() -> String? {
        lock.lock(); defer { lock.unlock() }
        return credential
    }

    private func cancelled() -> Bool { cancellation.isCancelled() || currentToken() == nil }

    /// `/repos/...` built by core from validated parts; refuse anything that
    /// could change the origin or traverse.
    static func validPath(_ path: String) -> Bool {
        path.hasPrefix("/repos/") && path.count <= 2048
            && path.unicodeScalars.allSatisfy { (33...126).contains($0.value) }
            && !path.contains("..") && !path.contains("//") && !path.contains("@")
            && !path.contains("\\") && !path.contains("#")
    }

    private static func validHeaderName(_ name: String) -> Bool {
        (1...64).contains(name.count)
            && name.unicodeScalars.allSatisfy { CharacterSet.alphanumerics.contains($0) && $0.isASCII || $0 == "-" }
    }

    func send(request: HttpTransportRequest) throws -> HttpTransportResponse {
        if cancelled() { throw HttpTransportError.Cancelled }
        guard Self.methods.contains(request.method), Self.validPath(request.path),
              let url = URL(string: Self.origin + request.path),
              url.scheme == "https", url.host == "api.github.com", url.user == nil
        else { throw HttpTransportError.Transport }
        let limit = Int(min(request.maxResponseBytes, UInt64(Self.maxResponse)))
        let timeout = TimeInterval(min(max(request.timeoutMs, 1_000), 120_000)) / 1000
        var req = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalAndRemoteCacheData, timeoutInterval: timeout)
        req.httpMethod = request.method
        req.httpShouldHandleCookies = false
        for header in request.headers {
            if header.name.caseInsensitiveCompare("Authorization") == .orderedSame { continue }
            guard Self.validHeaderName(header.name),
                  header.value.unicodeScalars.allSatisfy({ (32...126).contains($0.value) })
            else { throw HttpTransportError.Transport }
            req.setValue(header.value, forHTTPHeaderField: header.name)
        }
        guard let token = currentToken() else { throw HttpTransportError.Cancelled }
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        if let body = request.body { req.httpBody = body }
        let result: (status: Int, headers: [String: String], body: Data)
        do {
            result = try exchange.perform(req, maxBytes: limit) { [weak self] in self?.cancelled() ?? true }
        } catch let e as ExchangeError {
            switch e {
            case .timeout: throw cancelled() ? HttpTransportError.Cancelled : HttpTransportError.Timeout
            case .cancelled: throw HttpTransportError.Cancelled
            case .tooLarge: throw HttpTransportError.TooLarge
            case .redirect: throw HttpTransportError.Redirect
            case .transport: throw cancelled() ? HttpTransportError.Cancelled : HttpTransportError.Transport
            }
        } catch {
            throw cancelled() ? HttpTransportError.Cancelled : HttpTransportError.Transport
        }
        if cancelled() { throw HttpTransportError.Cancelled }
        guard (100...599).contains(result.status) else { throw HttpTransportError.Transport }
        if result.body.count > limit { throw HttpTransportError.TooLarge }
        func number(_ name: String) -> UInt64? { result.headers[name].flatMap { UInt64($0) } }
        return HttpTransportResponse(
            status: UInt16(result.status),
            body: result.body,
            retryAfter: number("retry-after"),
            rateLimitRemaining: number("x-ratelimit-remaining"),
            rateLimitReset: number("x-ratelimit-reset")
        )
    }
}

/// Production exchange: an ephemeral `URLSession` (no cache, no cookies, no
/// credential storage) per request, redirects refused, the body bounded
/// while it streams, cancellation polled. Blocks the calling (sync) thread,
/// never the main thread.
final class URLSessionExchange: HTTPExchange {
    private final class Delegate: NSObject, URLSessionDataDelegate, @unchecked Sendable {
        let maxBytes: Int
        var data = Data()
        var response: HTTPURLResponse?
        var failure: ExchangeError?
        let done = DispatchSemaphore(value: 0)

        init(maxBytes: Int) { self.maxBytes = maxBytes }

        func urlSession(
            _ session: URLSession, task: URLSessionTask,
            willPerformHTTPRedirection response: HTTPURLResponse,
            newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void
        ) {
            // Never follow: the 3xx response itself is returned to core.
            completionHandler(nil)
        }

        func urlSession(
            _ session: URLSession, dataTask: URLSessionDataTask,
            didReceive response: URLResponse,
            completionHandler: @escaping (URLSession.ResponseDisposition) -> Void
        ) {
            if response.expectedContentLength > Int64(maxBytes) {
                failure = .tooLarge
                completionHandler(.cancel)
                return
            }
            self.response = response as? HTTPURLResponse
            completionHandler(.allow)
        }

        func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive chunk: Data) {
            if data.count + chunk.count > maxBytes {
                failure = .tooLarge
                dataTask.cancel()
                return
            }
            data.append(chunk)
        }

        func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
            if response == nil, let http = task.response as? HTTPURLResponse { response = http }
            if failure == nil, let error = error as? URLError {
                failure = error.code == .timedOut ? .timeout : (error.code == .cancelled ? .cancelled : .transport)
            } else if failure == nil, error != nil {
                failure = .transport
            }
            done.signal()
        }
    }

    func perform(
        _ request: URLRequest, maxBytes: Int, isCancelled: @escaping () -> Bool
    ) throws -> (status: Int, headers: [String: String], body: Data) {
        let config = URLSessionConfiguration.ephemeral
        config.requestCachePolicy = .reloadIgnoringLocalAndRemoteCacheData
        config.urlCache = nil
        config.httpCookieStorage = nil
        config.httpShouldSetCookies = false
        config.urlCredentialStorage = nil
        config.timeoutIntervalForRequest = request.timeoutInterval
        config.timeoutIntervalForResource = request.timeoutInterval * 4
        let delegate = Delegate(maxBytes: maxBytes)
        let queue = OperationQueue()
        queue.maxConcurrentOperationCount = 1
        let session = URLSession(configuration: config, delegate: delegate, delegateQueue: queue)
        defer { session.invalidateAndCancel() }
        let task = session.dataTask(with: request)
        task.resume()
        while delegate.done.wait(timeout: .now() + .milliseconds(100)) == .timedOut {
            if isCancelled() {
                task.cancel()
                _ = delegate.done.wait(timeout: .now() + .seconds(5))
                throw ExchangeError.cancelled
            }
        }
        // The delegate queue is serial and has finished: safe to read.
        let (failure, response, body): (ExchangeError?, HTTPURLResponse?, Data) =
            queue.addOperationAndWait { (delegate.failure, delegate.response, delegate.data) }
        if let failure { throw failure }
        guard let response else { throw ExchangeError.transport }
        var headers: [String: String] = [:]
        for (k, v) in response.allHeaderFields {
            if let k = k as? String, let v = v as? String { headers[k.lowercased()] = v }
        }
        return (response.statusCode, headers, body)
    }
}

private final class ResultBox<T>: @unchecked Sendable {
    var value: T?
}

private extension OperationQueue {
    func addOperationAndWait<T>(_ body: @escaping () -> T) -> T {
        let box = ResultBox<T>()
        addOperations([BlockOperation { box.value = body() }], waitUntilFinished: true)
        return box.value!
    }
}

// MARK: - Engine entry points

/// Runs core sync / first pull for one target. Blocking; call off the main
/// thread (the coordinator does).
protocol SyncEngine: AnyObject {
    func sync(vault: Vault, target: SyncTarget, token: String, cancellation: SyncCancellation) throws -> GitHubSyncReport
}

final class GitHubSyncService: SyncEngine {
    private let makeTransport: (String, SyncCancellation) -> HttpTransport?

    init(makeTransport: @escaping (String, SyncCancellation) -> HttpTransport? = {
        GitHubHTTPSTransport(token: $0, cancellation: $1)
    }) {
        self.makeTransport = makeTransport
    }

    func sync(vault: Vault, target: SyncTarget, token: String, cancellation: SyncCancellation) throws -> GitHubSyncReport {
        guard let transport = makeTransport(token, cancellation) else { throw SyncSettingsError.invalidToken }
        defer { (transport as? GitHubHTTPSTransport)?.close() }
        return try vault.syncGithub(target: target.ffi, token: token, transport: transport, cancellation: cancellation)
    }

    static let vaultExistsMessage = String(localized: "A vault is already on this device; first pull never replaces it.")

    /// First pull for a device with no vault: core downloads and checks the
    /// remote files into a private staging directory next to `localRoot`,
    /// core authenticates the whole snapshot with the Master Password and
    /// Secret Key, and only then is it moved into place (one rename). On any
    /// failure the staging copy is deleted and `localRoot` is untouched. An
    /// existing vault is never replaced. Read-only on the remote.
    func firstPull(
        localRoot: URL, target: SyncTarget, token: String,
        password: String, secretKey: String, cancellation: SyncCancellation
    ) throws {
        let fm = FileManager.default
        if fm.fileExists(atPath: localRoot.path) {
            let contents = (try? fm.contentsOfDirectory(atPath: localRoot.path)) ?? ["?"]
            guard contents.isEmpty else { throw GitpassError.Generic(Self.vaultExistsMessage) }
        }
        guard let transport = makeTransport(token, cancellation) else { throw SyncSettingsError.invalidToken }
        defer { (transport as? GitHubHTTPSTransport)?.close() }
        let parent = localRoot.deletingLastPathComponent()
        try fm.createDirectory(at: parent, withIntermediateDirectories: true)
        let staging = parent.appendingPathComponent(".gitpass-first-pull-\(UUID().uuidString)")
        defer { try? fm.removeItem(at: staging) }
        _ = try pullGithubSnapshot(
            root: staging.path, target: target.ffi, token: token,
            transport: transport, cancellation: cancellation
        )
        if cancellation.isCancelled() { throw HttpTransportError.Cancelled }
        try validateFirstPullSnapshot(root: staging.path, password: password, secretKey: secretKey)
        if cancellation.isCancelled() { throw HttpTransportError.Cancelled }
        if fm.fileExists(atPath: localRoot.path) {
            // Re-checked right before the rename: only an empty directory goes.
            guard (try? fm.contentsOfDirectory(atPath: localRoot.path))?.isEmpty == true else {
                throw GitpassError.Generic(Self.vaultExistsMessage)
            }
            try fm.removeItem(at: localRoot)
        }
        try fm.moveItem(at: staging, to: localRoot)
    }
}

/// Fixed text for any sync error: core's own messages are already fixed
/// text without tokens, paths or data; everything else collapses.
func syncErrorMessage(_ error: Error) -> String {
    switch error {
    case GitpassError.Generic(let message): return message
    case let e as SyncSettingsError: return e.errorDescription ?? genericSyncFailure
    case HttpTransportError.Cancelled: return String(localized: "Sync cancelled.")
    default: return genericSyncFailure
    }
}

let genericSyncFailure = String(localized: "Sync failed; check the repository settings and the connection, then try again.")

// MARK: - Coordinator (when to sync, status for the UI)

/// Decides when to sync and publishes the status. Syncs run only while the
/// vault is unlocked (the Master Password is never stored for background
/// sync): on unlock, a few seconds after a local change, periodically while
/// unlocked, and on demand. Locking cancels a running sync.
@MainActor
final class SyncCoordinator: ObservableObject {
    enum Phase: Equatable {
        case notConfigured
        case idle
        case syncing
        case failed(String)
    }

    @Published private(set) var phase: Phase = .notConfigured
    @Published private(set) var lastSuccess: Date?
    @Published private(set) var lastConflicts: [String] = []
    @Published private(set) var lastReport: GitHubSyncReport?

    let settings: SyncSettingsStore
    private let engine: SyncEngine
    private let queue: DispatchQueue
    private let debounce: TimeInterval
    private let period: TimeInterval

    /// The unlocked vault, or nil. Set by the owner (VaultStore).
    var vaultProvider: () -> Vault? = { nil }
    /// Called on the main actor when a sync changed the local vault.
    var onLocalChange: () -> Void = {}

    private var running: SyncCancellation?
    private var generation = 0
    private var debounceTask: Task<Void, Never>?
    private var periodicTask: Task<Void, Never>?

    init(
        settings: SyncSettingsStore, engine: SyncEngine,
        queue: DispatchQueue = DispatchQueue(label: "dev.gitpass.ios.sync", qos: .utility),
        debounce: TimeInterval = 5, period: TimeInterval = 15 * 60
    ) {
        self.settings = settings
        self.engine = engine
        self.queue = queue
        self.debounce = debounce
        self.period = period
        lastSuccess = settings.lastSuccess
        phase = settings.isConfigured ? .idle : .notConfigured
    }

    var isSyncing: Bool { running != nil }

    func unlocked() {
        settingsChanged()
        guard settings.isConfigured else { return }
        Task { await syncNow() }
        periodicTask?.cancel()
        let period = self.period
        periodicTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: UInt64(period * 1_000_000_000))
                if Task.isCancelled { return }
                await self?.syncNow()
            }
        }
    }

    func locked() {
        generation += 1
        running?.cancel()
        running = nil
        debounceTask?.cancel()
        debounceTask = nil
        periodicTask?.cancel()
        periodicTask = nil
        if case .syncing = phase { phase = settings.isConfigured ? .idle : .notConfigured }
    }

    /// A local edit: sync shortly after the last one.
    func localChange() {
        guard settings.isConfigured else { return }
        debounceTask?.cancel()
        let delay = debounce
        debounceTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            if Task.isCancelled { return }
            await self?.syncNow()
        }
    }

    func settingsChanged() {
        lastSuccess = settings.lastSuccess
        if !settings.isConfigured {
            phase = .notConfigured
        } else if phase == .notConfigured {
            phase = .idle
        }
    }

    /// Runs one sync now (no-op if one is already running or the vault is
    /// locked) and returns when it finished.
    func syncNow() async {
        guard running == nil else { return }
        guard let target = settings.target, let token = settings.token() else {
            phase = .notConfigured
            return
        }
        guard let vault = vaultProvider() else { return }
        let cancellation = SyncCancellation()
        running = cancellation
        phase = .syncing
        let gen = generation
        let engine = self.engine
        let result: Result<GitHubSyncReport, Error> = await withCheckedContinuation { cont in
            queue.async {
                cont.resume(returning: Result {
                    try engine.sync(vault: vault, target: target, token: token, cancellation: cancellation)
                })
            }
        }
        // Locked (or another generation) meanwhile: drop the result.
        guard gen == generation else { return }
        running = nil
        switch result {
        case .success(let report):
            settings.recordSuccess()
            lastSuccess = settings.lastSuccess
            lastReport = report
            lastConflicts = report.conflicts
            phase = .idle
            if report.localChanged { onLocalChange() }
        case .failure(let error):
            phase = .failed(syncErrorMessage(error))
        }
    }
}
