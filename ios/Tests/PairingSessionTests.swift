import Darwin
import XCTest

// Compiled into GitpassTests, which is the target iOS CI runs.
// The SwiftUI screen is not in this target; the app build compiles it.
// These tests drive the same session the screen uses, with a fake link
// or the production Network.framework link on 127.0.0.1.

private let pairingTestToken = "github_pat_FAKE_ios_loopback"
private let pairingTestSecret = "A3-7DPGPV-RCYDPG-PVRCY-DPGPV-RCYD3"

final class PairingSessionTests: XCTestCase {
    override func tearDown() {
        PairingScanInjector.reset()
        pairingJoinerCancel()
        super.tearDown()
    }

    func testAllowlistDropsAddresses() {
        let expired = PairingLinkError.from(raw: "GitpassError.Generic(\"pairing session expired\") at 10.1.2.3")
        XCTAssertEqual(expired, .expired)
        XCTAssertEqual(expired.description, "pairing session expired")
        XCTAssertFalse(expired.debugDescription.contains("10.1.2.3"))

        let unknown = PairingLinkError.from(raw: "nw_connection failed 192.168.1.9:1234")
        XCTAssertEqual(unknown, .failed)
        XCTAssertEqual(unknown.description, "pairing failed")
        XCTAssertFalse(String(describing: unknown).contains("192.168"))

        let wrongCode = PairingLinkError.from(raw: "pairing verification code does not match")
        XCTAssertEqual(wrongCode, .failed)
        XCTAssertEqual(wrongCode.description, "pairing failed")
    }

    func testNoticeTextIsFixedEnglish() {
        let locale = Locale(identifier: "en")
        XCTAssertEqual(
            pairingNoticeText(.link(.connect), locale: locale),
            "The phone could not connect. Use the same Wi-Fi, and not a guest network that isolates devices."
        )
        XCTAssertEqual(
            pairingNoticeText(.pull, locale: locale),
            "The vault could not be downloaded. Existing data was kept."
        )
        let line = pairingTimeLeft(12, locale: locale)
        XCTAssertTrue(line.contains("12"))
        XCTAssertFalse(line.contains("%"))
        let repo = pairingRepoLine(owner: "fake-owner", repo: "fake-vault", branch: "main", locale: locale)
        XCTAssertTrue(repo.contains("fake-owner/fake-vault"))
        XCTAssertTrue(repo.contains("main"))
        XCTAssertFalse(repo.contains("github_pat_"))
    }

    func testRedactedDescriptions() {
        let connected = PairingConnected(code: "123456", expiresAt: 0)
        XCTAssertFalse(String(describing: connected).contains("123456"), "code rendered")
        let held = HeldGrant(
            owner: "fake-owner", repo: "fake-vault", branch: "main",
            token: "SENTINEL-must-not-appear", secret: "SENTINEL-must-not-appear"
        )
        let rendered = String(describing: held)
        XCTAssertFalse(rendered.contains("SENTINEL"), "grant rendered")
    }

    func testDesktopQrShape() {
        XCTAssertTrue(isDesktopQr("gitpass-pair:2:qr:abc.1.9.e30.e30"))
        XCTAssertFalse(isDesktopQr("gitpass-pair:1:qr:abc"))
        XCTAssertFalse(isDesktopQr("gitpass-pair:2:qr:has space"))
        XCTAssertFalse(isDesktopQr("https://example.invalid"))
        XCTAssertFalse(isDesktopQr("gitpass-pair:2:qr:"))
    }

    func testNumericHostsRejectNames() {
        XCTAssertNotNil(pairingNumericHost("127.0.0.1"))
        XCTAssertNotNil(pairingNumericHost("192.168.1.20"))
        XCTAssertNotNil(pairingNumericHost("::1"))
        XCTAssertNil(pairingNumericHost("not-a-host.invalid"))
        XCTAssertNil(pairingNumericHost("127.0.0.01"))
        XCTAssertNil(pairingNumericHost("0.0.0.0"))
        XCTAssertNil(pairingNumericHost("255.255.255.255"))
        XCTAssertNil(pairingNumericHost("224.0.0.1"))
        XCTAssertNil(pairingNumericHost("::"))
        XCTAssertNil(pairingNumericHost("ff02::1"))
        XCTAssertNil(pairingNumericHost(""))
    }

    func testInjectorIsOneShot() {
        PairingScanInjector.pending = "gitpass-pair:2:qr:FAKE"
        XCTAssertEqual(PairingScanInjector.take(), "gitpass-pair:2:qr:FAKE")
        XCTAssertNil(PairingScanInjector.take())
    }

    func testExpiredDoesNotConnect() {
        let link = ScriptLink()
        let joiner = ScriptJoiner()
        joiner.endpointValue.expiresAt = 1
        let session = PairingSession(stateDir: tempDir(), joiner: joiner, link: link, maxWait: 2)
        expect(session, qr: "gitpass-pair:2:qr:script", equals: .expired)
        XCTAssertEqual(link.connects, 0)
    }

    func testTimeout() {
        let link = ScriptLink()
        let session = PairingSession(stateDir: tempDir(), joiner: ScriptJoiner(), link: link, maxWait: 0.5)
        expect(session, qr: "gitpass-pair:2:qr:script", equals: .timeout)
        XCTAssertEqual(link.connects, 1)
    }

    func testClosed() {
        let link = ScriptLink()
        link.readError = .closed
        let session = PairingSession(stateDir: tempDir(), joiner: ScriptJoiner(), link: link, maxWait: 2)
        expect(session, qr: "gitpass-pair:2:qr:script", equals: .closed)
    }

    func testProtocolErrorWritesFailFrame() {
        let link = ScriptLink()
        link.reads = [Data([9])]
        let joiner = ScriptJoiner()
        joiner.pushDrive = PairingDrive(outbound: Data([6, 1]), phase: "failed", error: "pairing payload failed authentication")
        let session = PairingSession(stateDir: tempDir(), joiner: joiner, link: link, maxWait: 2)
        expect(session, qr: "gitpass-pair:2:qr:script", equals: .failed)
        XCTAssertTrue(link.written.contains(6) && link.written.contains(1), "fail frame")
    }

    func testScriptedCode() throws {
        let link = ScriptLink()
        link.reads = [Data([9])]
        let joiner = ScriptJoiner()
        joiner.startBytes = Data([1])
        joiner.pushDrive = PairingDrive(outbound: Data([2]), phase: "code", error: "")
        let session = PairingSession(stateDir: tempDir(), joiner: joiner, link: link, maxWait: 2)
        let connected = try session.connectUntilCode("gitpass-pair:2:qr:script")
        let shape = connected.code == "123456"
        XCTAssertTrue(shape, "code shape")
        XCTAssertEqual([UInt8](link.written), [1, 2])
        session.close()
    }

    func testCancelUnblocks() {
        let link = BlockingLink()
        let session = PairingSession(stateDir: tempDir(), joiner: ScriptJoiner(), link: link, maxWait: 5)
        let done = expectation(description: "unblocked")
        DispatchQueue.global().async {
            self.expect(session, qr: "gitpass-pair:2:qr:script", equals: .failed)
            done.fulfill()
        }
        Thread.sleep(forTimeInterval: 0.3)
        session.close()
        wait(for: [done], timeout: 3)
    }

    func testBadVersionDoesNotConnect() {
        let link = ScriptLink()
        let dir = tempDir()
        let session = PairingSession(stateDir: dir, joiner: FfiPairingJoiner(stateDir: dir), link: link, maxWait: 2)
        defer { session.close() }
        do {
            _ = try session.connectUntilCode("gitpass-pair:1:qr:nope")
            XCTFail("bad version")
        } catch {
            let text = String(describing: error)
            XCTAssertFalse(text.contains("gitpass-pair:1"), "error echoed the qr")
            XCTAssertFalse(text.contains("nope"), "error echoed the qr")
        }
        XCTAssertEqual(link.connects, 0)
    }

    func testHostnameIsNotEchoed() {
        let link = NWPairingLink()
        do {
            try link.connect(addresses: ["not-a-host.invalid"], port: 9, timeout: 1)
            XCTFail("name was accepted")
        } catch {
            let text = String(describing: error)
            XCTAssertEqual(text, "pairing could not connect")
            XCTAssertFalse(text.contains("not-a-host"), "error echoed a name")
        }
    }

    func testRefusedPortIsNotEchoed() {
        let link = NWPairingLink()
        do {
            try link.connect(addresses: ["127.0.0.1"], port: 1, timeout: 2)
            XCTFail("port 1 accepted")
        } catch {
            let text = String(describing: error)
            XCTAssertTrue(text == "pairing could not connect" || text == "pairing timed out")
            XCTAssertFalse(text.contains("127.0.0.1"), "error echoed an address")
        }
    }

    func testLoopbackGrantKeepsSecretsOffTheWire() throws {
        let hostDir = tempDir()
        let joinDir = tempDir()
        let peer = try PairingHostPeer(dir: hostDir)
        defer { peer.close() }
        assertClean(peer.qr)
        XCTAssertTrue(peer.qr.hasPrefix("gitpass-pair:2:qr:"), "qr prefix")
        let client = PairingSession(
            stateDir: joinDir, joiner: FfiPairingJoiner(stateDir: joinDir), link: NWPairingLink(), maxWait: 20
        )
        defer { client.close() }
        let connected = try client.connectUntilCode(peer.qr)
        let shape = connected.code.count == 6 && connected.code.allSatisfy(\.isNumber)
        XCTAssertTrue(shape, "code shape")
        XCTAssertFalse(String(describing: connected).contains(connected.code), "code rendered")
        do {
            try peer.confirm(code: connected.code)
        } catch {
            XCTFail("confirm failed")
            return
        }
        let paired = try client.receiveGrant()
        let token = paired.syncToken()
        let secret = paired.secretKey()
        let tokenMatches = token == pairingTestToken
        XCTAssertTrue(tokenMatches, "grant token mismatch")
        XCTAssertEqual(paired.repoOwner(), "fake-owner")
        XCTAssertEqual(paired.repoName(), "fake-vault")
        XCTAssertEqual(paired.branch(), "main")
        XCTAssertEqual(paired.tokenKind(), "fine_grained_pat")
        let rendered = String(describing: paired)
        let leaked = rendered.contains(token) || rendered.contains(secret)
        XCTAssertFalse(leaked, "object rendered a secret")
        let held = HeldGrant(
            owner: paired.repoOwner(), repo: paired.repoName(), branch: paired.branch(),
            token: token, secret: secret
        )
        XCTAssertFalse(String(describing: held).contains("github_pat_"), "held grant rendered")

        let again = ScriptLink()
        let second = PairingSession(
            stateDir: joinDir, joiner: FfiPairingJoiner(stateDir: joinDir), link: again, maxWait: 2
        )
        var replayed = false
        do {
            _ = try second.connectUntilCode(peer.qr)
        } catch {
            replayed = true
            let text = String(describing: error)
            XCTAssertFalse(text.contains("github_pat_"), "replay error has a token")
        }
        XCTAssertTrue(replayed, "replay")
        XCTAssertEqual(again.connects, 0)
        second.close()

        peer.close()
        assertClean(peer.wireText())
        XCTAssertNil(peer.failure)
    }

    private func expect(_ session: PairingSession, qr: String, equals: PairingLinkError) {
        do {
            _ = try session.connectUntilCode(qr)
            XCTFail("expected \(equals.message)")
        } catch let error as PairingLinkError {
            XCTAssertEqual(error, equals)
        } catch {
            XCTFail("unexpected error")
        }
    }

    private func assertClean(_ text: String, file: StaticString = #filePath, line: UInt = #line) {
        let leaked = text.contains(pairingTestToken)
            || text.contains(pairingTestSecret)
            || text.contains("master_password")
            || text.contains("FAKE-master")
        XCTAssertFalse(leaked, "secret appeared", file: file, line: line)
    }

    private func tempDir() -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("gitpass-pair-\(UUID().uuidString)", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }
}

private final class ScriptLink: PairingLink {
    var connects = 0
    var written = Data()
    var reads: [Data] = []
    var readError: PairingLinkError?
    func connect(addresses: [String], port: UInt16, timeout: TimeInterval) throws { connects += 1 }
    func write(_ bytes: Data) throws { written.append(bytes) }
    func read(max: Int, slice: TimeInterval) throws -> Data {
        if let readError { throw readError }
        if reads.isEmpty {
            Thread.sleep(forTimeInterval: min(slice, 0.05))
            return Data()
        }
        return reads.removeFirst()
    }
    func close() {}
}

private final class BlockingLink: PairingLink {
    private let condition = NSCondition()
    private var closed = false
    func connect(addresses: [String], port: UInt16, timeout: TimeInterval) throws {}
    func write(_ bytes: Data) throws {}
    func read(max: Int, slice: TimeInterval) throws -> Data {
        condition.lock()
        defer { condition.unlock() }
        let deadline = Date().addingTimeInterval(slice)
        while !closed {
            if !condition.wait(until: deadline) { return Data() }
        }
        throw PairingLinkError.closed
    }
    func close() {
        condition.lock()
        closed = true
        condition.signal()
        condition.unlock()
    }
}

private final class ScriptJoiner: PairingJoiner {
    var phaseName = "wait"
    var startBytes = Data([1])
    var pushDrive = PairingDrive(outbound: Data(), phase: "code", error: "")
    var codeValue = "123456"
    var endpointValue = PairingEndpoint(
        port: 9, addresses: ["127.0.0.1"], expiresAt: Int64(Date().timeIntervalSince1970) + 60
    )
    func start(qr: String) throws -> Data { startBytes }
    func endpoint() throws -> PairingEndpoint { endpointValue }
    func push(_ chunk: Data) throws -> PairingDrive {
        phaseName = pushDrive.phase
        return pushDrive
    }
    func phase() -> String { phaseName }
    func code() throws -> String { codeValue }
    func finish() throws -> PairedSync { throw PairingLinkError.failed }
    func cancel() {}
}

/// Desktop side of one loopback pairing. The socket is 127.0.0.1.
/// Recorded bytes are checked so a secret never crosses the socket.
private final class PairingHostPeer {
    let qr: String
    private let dir: URL
    private var listenFD: Int32 = -1
    private let grantLock = NSCondition()
    private var grant: Data?
    private var stop = false
    private let wireLock = NSLock()
    private var wire = Data()
    private let failureLock = NSLock()
    private var failureValue: String?
    private let done = DispatchSemaphore(value: 0)
    private var finished = false

    var failure: String? {
        failureLock.lock()
        defer { failureLock.unlock() }
        return failureValue
    }

    init(dir: URL) throws {
        self.dir = dir
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        listenFD = Darwin.socket(AF_INET, SOCK_STREAM, 0)
        if listenFD < 0 { throw PairingLinkError.connect }
        let port: UInt16
        do {
            port = try bindLoopback()
        } catch {
            Darwin.close(listenFD)
            listenFD = -1
            throw PairingLinkError.connect
        }
        let view: PairingHostView
        do {
            let input = PairingHostInput(
                stateDir: dir.path,
                accountId: "00000000-0000-4000-8000-000000000001",
                repoOwner: "fake-owner",
                repoName: "fake-vault",
                branch: "main",
                secretKey: pairingTestSecret,
                syncToken: pairingTestToken,
                addressesJson: "[\"127.0.0.1\"]",
                port: port
            )
            view = try pairingHostBegin(input: input)
        } catch {
            Darwin.close(listenFD)
            listenFD = -1
            throw PairingLinkError.from(raw: String(describing: error))
        }
        qr = view.qr
        DispatchQueue.global(qos: .userInitiated).async { [self] in
            defer { self.done.signal() }
            self.serve()
        }
    }

    func confirm(code: String) throws {
        let drive = try pairingHostConfirm(stateDir: dir.path, code: code)
        if !drive.error.isEmpty { throw PairingLinkError.from(raw: drive.error) }
        if drive.outbound.isEmpty { throw PairingLinkError.failed }
        grantLock.lock()
        grant = drive.outbound
        grantLock.signal()
        grantLock.unlock()
    }

    func wireText() -> String {
        wireLock.lock()
        defer { wireLock.unlock() }
        return String(wire.map { Character(UnicodeScalar($0)) })
    }

    func close() {
        if finished { return }
        finished = true
        grantLock.lock()
        stop = true
        grantLock.signal()
        grantLock.unlock()
        done.wait(timeout: .now() + 5)
        if listenFD >= 0 {
            Darwin.close(listenFD)
            listenFD = -1
        }
        try? pairingHostCancel(stateDir: dir.path)
        pairingJoinerCancel()
    }

    private func bindLoopback() throws -> UInt16 {
        var one: Int32 = 1
        setsockopt(listenFD, SOL_SOCKET, SO_REUSEADDR, &one, socklen_t(MemoryLayout.size(ofValue: one)))
        setsockopt(listenFD, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout.size(ofValue: one)))
        var addr = sockaddr_in()
        addr.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = 0
        addr.sin_addr.s_addr = UInt32(0x7f000001).bigEndian
        let bound = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.bind(listenFD, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        if bound != 0 || Darwin.listen(listenFD, 1) != 0 { throw PairingLinkError.connect }
        let flags = fcntl(listenFD, F_GETFL)
        if flags >= 0 { _ = fcntl(listenFD, F_SETFL, flags | Int32(O_NONBLOCK)) }
        var got = sockaddr_in()
        var len = socklen_t(MemoryLayout<sockaddr_in>.size)
        let named = withUnsafeMutablePointer(to: &got) { addrPtr -> Int32 in
            addrPtr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
                withUnsafeMutablePointer(to: &len) { lenPtr in
                    getsockname(listenFD, sa, lenPtr)
                }
            }
        }
        if named != 0 { throw PairingLinkError.connect }
        let port = UInt16(bigEndian: got.sin_port)
        if port == 0 { throw PairingLinkError.connect }
        return port
    }

    private func serve() {
        let deadline = Date().addingTimeInterval(20)
        var client: Int32 = -1
        while !shouldStop(), Date() < deadline {
            var pfd = pollfd(fd: listenFD, events: Int16(POLLIN), revents: 0)
            let rc = withUnsafeMutablePointer(to: &pfd) { poll($0, nfds_t(1), 200) }
            if rc > 0 {
                client = Darwin.accept(listenFD, nil, nil)
                if client >= 0 { break }
            }
        }
        guard client >= 0 else { return }
        var one: Int32 = 1
        setsockopt(client, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout.size(ofValue: one)))
        setsockopt(client, IPPROTO_TCP, TCP_NODELAY, &one, socklen_t(MemoryLayout.size(ofValue: one)))
        let flags = fcntl(client, F_GETFL)
        if flags >= 0 { _ = fcntl(client, F_SETFL, flags | Int32(O_NONBLOCK)) }
        var buf = [UInt8](repeating: 0, count: 8192)
        var reachedCode = false
        while !shouldStop() {
            let n = readSlice(client, &buf)
            if n < 0 { break }
            if n == 0 { continue }
            record(Data(buf.prefix(n)))
            let drive: PairingDrive
            do {
                drive = try pairingHostPush(stateDir: dir.path, chunk: Data(buf.prefix(n)))
            } catch {
                note("pairing failed")
                break
            }
            if !drive.outbound.isEmpty {
                record(drive.outbound)
                if !writeAll(client, drive.outbound) { break }
            }
            if !drive.error.isEmpty {
                note(allowlisted(drive.error))
                break
            }
            if drive.phase == "code" {
                reachedCode = true
                break
            }
        }
        if reachedCode {
            grantLock.lock()
            let grantDeadline = Date().addingTimeInterval(20)
            while grant == nil && !stop {
                if !grantLock.wait(until: grantDeadline) { break }
            }
            let bytes = grant
            grantLock.unlock()
            if let bytes {
                record(bytes)
                _ = writeAll(client, bytes)
                let end = Date().addingTimeInterval(10)
                while !shouldStop(), Date() < end {
                    let n = readSlice(client, &buf)
                    if n < 0 { break }
                    if n > 0 { record(Data(buf.prefix(n))) }
                }
            }
        }
        Darwin.close(client)
    }

    private func shouldStop() -> Bool {
        grantLock.lock()
        defer { grantLock.unlock() }
        return stop
    }

    private func readSlice(_ fd: Int32, _ buf: inout [UInt8]) -> Int {
        var pfd = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
        let rc = withUnsafeMutablePointer(to: &pfd) { poll($0, nfds_t(1), 200) }
        if rc == 0 { return 0 }
        if rc < 0 { return errno == EINTR ? 0 : -1 }
        let n = buf.withUnsafeMutableBytes { raw -> Int in
            guard let base = raw.baseAddress else { return -1 }
            return Darwin.read(fd, base, buf.count)
        }
        if n == 0 { return -1 }
        if n < 0 {
            if errno == EAGAIN || errno == EWOULDBLOCK || errno == EINTR { return 0 }
            return -1
        }
        return n
    }

    private func writeAll(_ fd: Int32, _ data: Data) -> Bool {
        var sent = 0
        let bytes = [UInt8](data)
        while sent < bytes.count {
            if shouldStop() { return false }
            var pfd = pollfd(fd: fd, events: Int16(POLLOUT), revents: 0)
            let rc = withUnsafeMutablePointer(to: &pfd) { poll($0, nfds_t(1), 5_000) }
            if rc <= 0 { return false }
            let n = bytes.withUnsafeBytes { raw -> Int in
                guard let base = raw.baseAddress else { return -1 }
                return Darwin.write(fd, base.advanced(by: sent), bytes.count - sent)
            }
            if n > 0 {
                sent += n
            } else if n < 0 && (errno == EAGAIN || errno == EWOULDBLOCK) {
                continue
            } else {
                return false
            }
        }
        return true
    }

    private func record(_ data: Data) {
        wireLock.lock()
        wire.append(data)
        wireLock.unlock()
    }

    private func note(_ text: String) {
        failureLock.lock()
        failureValue = text
        failureLock.unlock()
    }
}
