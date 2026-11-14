import Foundation
import Network

/// Phone side of pairing v2 (#491). Swift moves opaque bytes only.
/// gitpass-core owns the key exchange, the code and the grant.
/// The Master Password is never passed in here.
///
/// The socket is raw TCP, not HTTP, so App Transport Security is unchanged.
/// There is no TLS and no Bonjour: the QR already carries numeric addresses.

enum PairingLinkError: Error, Equatable, CustomStringConvertible, CustomDebugStringConvertible, LocalizedError {
    case connect
    case timeout
    case closed
    case expired
    case failed

    var message: String {
        switch self {
        case .connect: return "pairing could not connect"
        case .timeout: return "pairing timed out"
        case .closed: return "pairing connection closed"
        case .expired: return "pairing session expired"
        case .failed: return "pairing failed"
        }
    }

    var description: String { message }
    var debugDescription: String { message }
    var errorDescription: String? { message }

    /// Keeps a known sentence. Anything else, including an address, becomes "pairing failed".
    static func from(raw: String) -> PairingLinkError {
        switch allowlisted(raw) {
        case "pairing session expired": return .expired
        case "pairing could not connect": return .connect
        case "pairing timed out": return .timeout
        case "pairing connection closed": return .closed
        default: return .failed
        }
    }
}

func allowlisted(_ raw: String) -> String {
    let known = [
        "pairing session expired",
        "pairing session already used",
        "pairing payload failed authentication",
        "unsupported pairing protocol version",
        "unrecognized pairing message",
        "pairing key rejected",
        "pairing payload field rejected",
        "pairing state unavailable",
        "no pairing in progress",
        "too many wrong pairing codes",
        "pairing message too large",
        "pairing message truncated",
        "pairing session does not match",
        "pairing verification code does not match",
        "pairing could not connect",
        "pairing timed out",
        "pairing connection closed",
        "pairing failed",
    ]
    return known.first { raw.contains($0) } ?? "pairing failed"
}

enum PairingNotice: Equatable {
    case link(PairingLinkError)
    case pull
    case camera
}

func pairingNoticeText(_ notice: PairingNotice, locale: Locale = .autoupdatingCurrent) -> String {
    switch notice {
    case .link(.expired):
        return String(localized: "That QR code expired. Start again on the computer.", locale: locale)
    case .link(.connect):
        return String(
            localized: "The phone could not connect. Use the same Wi-Fi, and not a guest network that isolates devices.",
            locale: locale
        )
    case .link(.timeout):
        return String(localized: "Pairing timed out. Start again on the computer.", locale: locale)
    case .link(.closed):
        return String(localized: "The connection closed. Start again.", locale: locale)
    case .link(.failed):
        return String(localized: "Pairing failed. Start again.", locale: locale)
    case .pull:
        return String(localized: "The vault could not be downloaded. Existing data was kept.", locale: locale)
    case .camera:
        return String(localized: "The camera is not available. Pairing needs it to scan the QR code.", locale: locale)
    }
}

func pairingTimeLeft(_ seconds: Int64, locale: Locale = .autoupdatingCurrent) -> String {
    let format = String(localized: "Time left %lld s", locale: locale)
    return String(format: format, locale: locale, seconds)
}

func pairingRepoLine(owner: String, repo: String, branch: String, locale: Locale = .autoupdatingCurrent) -> String {
    let format = String(localized: "This phone can sync %@ (%@).", locale: locale)
    return String(format: format, locale: locale, owner + "/" + repo, branch)
}

func isDesktopQr(_ text: String) -> Bool {
    let prefix = "gitpass-pair:2:qr:"
    guard text.hasPrefix(prefix), text.count > prefix.count, text.count <= 4096 else { return false }
    return text.unicodeScalars.allSatisfy { (0x21...0x7E).contains($0.value) }
}

/// A numeric address from the QR. Names are rejected so nothing here can start a DNS lookup.
/// The host is the numeric string itself. Network treats that string as an address.
func pairingNumericHost(_ host: String) -> NWEndpoint.Host? {
    if host.isEmpty || host.count > 64 { return nil }
    if pairingIPv4(host) != nil { return NWEndpoint.Host(host) }
    let hex = Set("0123456789abcdefABCDEF:")
    guard host.contains(":"), host.allSatisfy({ hex.contains($0) }), let v6 = IPv6Address(host) else { return nil }
    let bytes = pairingAddressBytes(v6.rawValue)
    if bytes.allSatisfy({ $0 == 0 }) || bytes.first == 0xFF { return nil }
    return NWEndpoint.Host(host)
}

private func pairingIPv4(_ host: String) -> IPv4Address? {
    let parts = host.split(separator: ".", omittingEmptySubsequences: false)
    guard parts.count == 4 else { return nil }
    var nums = [UInt8]()
    for part in parts {
        if part.isEmpty || part.count > 3 { return nil }
        if part.count > 1 && part.first == "0" { return nil }
        guard part.allSatisfy(\.isNumber), let value = UInt8(part) else { return nil }
        nums.append(value)
    }
    if nums.allSatisfy({ $0 == 0 }) || nums.allSatisfy({ $0 == 255 }) { return nil }
    if nums[0] >= 224 && nums[0] <= 239 { return nil }
    return IPv4Address("\(nums[0]).\(nums[1]).\(nums[2]).\(nums[3])")
}

private func pairingAddressBytes<T>(_ value: T) -> [UInt8] {
    withUnsafeBytes(of: value) { Array($0) }
}

protocol PairingLink: AnyObject {
    func connect(addresses: [String], port: UInt16, timeout: TimeInterval) throws
    func write(_ bytes: Data) throws
    func read(max: Int, slice: TimeInterval) throws -> Data
    func close()
}

protocol PairingJoiner: AnyObject {
    func start(qr: String) throws -> Data
    func endpoint() throws -> PairingEndpoint
    func push(_ chunk: Data) throws -> PairingDrive
    func phase() -> String
    func code() throws -> String
    func finish() throws -> PairedSync
    func cancel()
}

final class FfiPairingJoiner: PairingJoiner {
    private let stateDir: String

    init(stateDir: URL) {
        self.stateDir = stateDir.path
    }

    func start(qr: String) throws -> Data {
        try pairingJoinerStart(qr: qr, stateDir: stateDir)
    }

    func endpoint() throws -> PairingEndpoint {
        try pairingJoinerEndpoint()
    }

    func push(_ chunk: Data) throws -> PairingDrive {
        try pairingJoinerPush(chunk: chunk)
    }

    func phase() -> String {
        pairingJoinerPhase()
    }

    func code() throws -> String {
        try pairingJoinerCode()
    }

    func finish() throws -> PairedSync {
        try pairingJoinerFinish(stateDir: stateDir)
    }

    func cancel() {
        pairingJoinerCancel()
    }
}

struct PairingConnected: CustomStringConvertible, CustomDebugStringConvertible {
    let code: String
    let expiresAt: Int64
    var description: String { "PairingConnected([redacted])" }
    var debugDescription: String { description }
}

/// Grant fields kept until the Master Password is typed. Description is redacted.
final class HeldGrant: CustomStringConvertible, CustomDebugStringConvertible {
    let owner: String
    let repo: String
    let branch: String
    let token: String
    let secret: String

    init(owner: String, repo: String, branch: String, token: String, secret: String) {
        self.owner = owner
        self.repo = repo
        self.branch = branch
        self.token = token
        self.secret = secret
    }

    var description: String { "HeldGrant([redacted])" }
    var debugDescription: String { description }
}

/// Where the screen looks for a replay record. Next to the vault, not inside it.
enum PairingPaths {
    static func stateDirectory() -> URL {
        let base: URL
        if let shared = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: "group.dev.gitpass.ios") {
            base = shared
        } else {
            base = FileManager.default.temporaryDirectory
        }
        let dir = base.appendingPathComponent("pairing", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }
}

/// Set by a test before the screen appears. Production leaves both at their defaults.
enum PairingScanInjector {
    static var cameraEnabled = true
    static var pending: String?
    static func take() -> String? {
        let value = pending
        pending = nil
        return value
    }
    static func reset() {
        cameraEnabled = true
        pending = nil
    }
}

final class PairingSession {
    private let joiner: PairingJoiner
    private let link: PairingLink
    private let maxWait: TimeInterval
    private let state = NSLock()
    private var stopped = false
    private var didCancel = false
    private var waitLimit: TimeInterval = 120

    init(stateDir: URL, joiner: PairingJoiner? = nil, link: PairingLink? = nil, maxWait: TimeInterval = 120) {
        try? FileManager.default.createDirectory(at: stateDir, withIntermediateDirectories: true)
        self.joiner = joiner ?? FfiPairingJoiner(stateDir: stateDir)
        self.link = link ?? NWPairingLink()
        self.maxWait = maxWait
    }

    func connectUntilCode(_ qr: String) throws -> PairingConnected {
        let commit = try guarded { try joiner.start(qr: qr) }
        let ep = try guarded { try joiner.endpoint() }
        let now = Int64(Date().timeIntervalSince1970)
        if ep.expiresAt <= now {
            close()
            throw PairingLinkError.expired
        }
        let remaining = TimeInterval(ep.expiresAt - now)
        waitLimit = max(min(remaining, maxWait), 0.2)
        do {
            try link.connect(addresses: ep.addresses, port: ep.port, timeout: 5)
        } catch {
            close()
            throw (error as? PairingLinkError) ?? PairingLinkError.connect
        }
        do {
            try pump(commit, target: "code")
            let code = try guarded { try joiner.code() }
            return PairingConnected(code: code, expiresAt: ep.expiresAt)
        } catch {
            close()
            throw error
        }
    }

    func receiveGrant() throws -> PairedSync {
        do {
            try pump(Data(), target: "done")
            let paired = try guarded { try joiner.finish() }
            link.close()
            markStopped()
            return paired
        } catch {
            close()
            throw error
        }
    }

    func close() {
        markStopped()
        link.close()
        cancelJoinerOnce()
    }

    private func pump(_ initial: Data, target: String) throws {
        var outbound = initial
        let deadline = Date().addingTimeInterval(waitLimit)
        while Date() < deadline {
            if isStopped() { throw PairingLinkError.failed }
            if !outbound.isEmpty {
                try writeOrStop(outbound)
                outbound = Data()
            }
            let phase = joiner.phase()
            if phase == "failed" { throw PairingLinkError.failed }
            if phase == target { return }
            let chunk = try readOrStop(8192, 0.2)
            if chunk.isEmpty { continue }
            let drive = try guarded { try joiner.push(chunk) }
            if !drive.error.isEmpty {
                if !drive.outbound.isEmpty { try? link.write(drive.outbound) }
                throw PairingLinkError.from(raw: drive.error)
            }
            outbound = drive.outbound
            if drive.phase == "failed" { throw PairingLinkError.failed }
            if drive.phase == target {
                if !outbound.isEmpty { try writeOrStop(outbound) }
                return
            }
        }
        throw PairingLinkError.timeout
    }

    private func writeOrStop(_ bytes: Data) throws {
        do {
            try link.write(bytes)
        } catch let error as PairingLinkError {
            if isStopped() { throw PairingLinkError.failed }
            throw error
        }
    }

    private func readOrStop(_ max: Int, _ slice: TimeInterval) throws -> Data {
        do {
            return try link.read(max: max, slice: slice)
        } catch let error as PairingLinkError {
            if isStopped() { throw PairingLinkError.failed }
            throw error
        }
    }

    private func guarded<T>(_ body: () throws -> T) throws -> T {
        do {
            return try body()
        } catch let error as PairingLinkError {
            throw error
        } catch {
            let described = String(describing: error)
            let localized = error.localizedDescription
            let raw = described == localized ? described : described + "\n" + localized
            throw PairingLinkError.from(raw: raw)
        }
    }

    private func markStopped() {
        state.lock()
        stopped = true
        state.unlock()
    }

    private func isStopped() -> Bool {
        state.lock()
        defer { state.unlock() }
        return stopped
    }

    private func cancelJoinerOnce() {
        state.lock()
        let run = !didCancel
        didCancel = true
        state.unlock()
        if run { joiner.cancel() }
    }
}

/// Production transport. Network.framework errors are never copied into the thrown value.
final class NWPairingLink: PairingLink {
    private let queue = DispatchQueue(label: "dev.gitpass.pairing")
    private let condition = NSCondition()
    private var connection: NWConnection?
    private var inbox = Data()
    private var closed = false
    private var dead = false
    private var epoch = 0

    func connect(addresses: [String], port: UInt16, timeout: TimeInterval) throws {
        if port == 0 { throw PairingLinkError.connect }
        var last = PairingLinkError.connect
        var tried = false
        for host in addresses {
            guard let endpoint = pairingNumericHost(host) else { continue }
            tried = true
            do {
                try open(endpoint, port, timeout)
                return
            } catch let error as PairingLinkError {
                last = error
            }
        }
        throw tried ? last : PairingLinkError.connect
    }

    func write(_ bytes: Data) throws {
        if bytes.isEmpty { return }
        condition.lock()
        let conn = connection
        let isClosed = closed
        condition.unlock()
        if isClosed || conn == nil { throw PairingLinkError.closed }
        let done = DispatchSemaphore(value: 0)
        let box = PairingOnce()
        conn?.send(content: bytes, isComplete: false, completion: .contentProcessed { error in
            box.finish(error == nil)
            done.signal()
        })
        if done.wait(timeout: .now() + 5) == .timedOut { throw PairingLinkError.closed }
        if box.value != true { throw PairingLinkError.closed }
    }

    func read(max: Int, slice: TimeInterval) throws -> Data {
        condition.lock()
        defer { condition.unlock() }
        let deadline = Date().addingTimeInterval(slice)
        while inbox.isEmpty && !dead && !closed {
            if !condition.wait(until: deadline) { return Data() }
        }
        if inbox.isEmpty { throw PairingLinkError.closed }
        let n = min(max, inbox.count)
        let out = inbox.prefix(n)
        inbox.removeFirst(n)
        return Data(out)
    }

    func close() {
        condition.lock()
        closed = true
        dead = true
        epoch += 1
        let conn = connection
        connection = nil
        condition.signal()
        condition.unlock()
        conn?.stateUpdateHandler = nil
        conn?.cancel()
    }

    private func open(_ host: NWEndpoint.Host, _ port: UInt16, _ timeout: TimeInterval) throws {
        guard let nwPort = NWEndpoint.Port(rawValue: port) else { throw PairingLinkError.connect }
        let tcp = NWProtocolTCP.Options()
        tcp.noDelay = true
        let params = NWParameters(tls: nil, tcp: tcp)
        let conn = NWConnection(host: host, port: nwPort, using: params)
        condition.lock()
        let ticket = epoch
        let alreadyClosed = closed
        condition.unlock()
        if alreadyClosed {
            conn.cancel()
            throw PairingLinkError.failed
        }
        let done = DispatchSemaphore(value: 0)
        let box = PairingOnce()
        conn.stateUpdateHandler = { state in
            switch state {
            case .ready:
                if box.finish(true) {
                    conn.stateUpdateHandler = nil
                    done.signal()
                }
            case .failed, .cancelled:
                if box.finish(false) {
                    conn.stateUpdateHandler = nil
                    done.signal()
                }
            default:
                break
            }
        }
        conn.start(queue: queue)
        if done.wait(timeout: .now() + timeout) == .timedOut {
            conn.stateUpdateHandler = nil
            conn.cancel()
            throw PairingLinkError.timeout
        }
        guard box.value == true else {
            conn.cancel()
            throw PairingLinkError.connect
        }
        condition.lock()
        if closed || epoch != ticket {
            condition.unlock()
            conn.cancel()
            throw PairingLinkError.failed
        }
        connection = conn
        dead = false
        inbox.removeAll(keepingCapacity: false)
        condition.unlock()
        startReceive(conn)
    }

    private func startReceive(_ conn: NWConnection) {
        conn.receive(minimumIncompleteLength: 1, maximumLength: 8192) { [weak self] data, _, isComplete, error in
            guard let self else { return }
            self.condition.lock()
            if let data, !data.isEmpty {
                if self.inbox.count < 65_536 {
                    self.inbox.append(data)
                } else {
                    self.dead = true
                }
            }
            if isComplete || error != nil { self.dead = true }
            let again = !self.dead && !self.closed
            self.condition.signal()
            self.condition.unlock()
            if again { self.startReceive(conn) }
        }
    }
}

private final class PairingOnce {
    private let lock = NSLock()
    private(set) var value: Bool?
    func finish(_ ok: Bool) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if value != nil { return false }
        value = ok
        return true
    }
}
