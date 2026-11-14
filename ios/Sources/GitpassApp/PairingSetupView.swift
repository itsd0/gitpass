import AVFoundation
import SwiftUI

/// Scans the computer's QR, shows the 6-digit code, then asks for the Master
/// Password on this phone. The password is used only for the local first pull.
struct PairingSetupView: View {
    @EnvironmentObject private var store: VaultStore
    @Environment(\.dismiss) private var dismiss
    @StateObject private var model = PairingModel()

    var body: some View {
        NavigationStack {
            VStack(spacing: 20) {
                switch model.step {
                case .scan:
                    Text(String(localized: "Scan the QR code on the computer."))
                        .font(.subheadline)
                        .foregroundStyle(Color.gitpassSecondary)
                        .multilineTextAlignment(.center)
                        .accessibilityIdentifier("pair-scan")
                    if PairingScanInjector.cameraEnabled {
                        PairingCamera(onCode: { model.start($0) }, onUnavailable: { model.cameraUnavailable() })
                            .frame(maxWidth: .infinity)
                            .frame(height: 280)
                            .clipShape(RoundedRectangle(cornerRadius: 12))
                            .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.gitpassStroke, lineWidth: 1))
                    }
                case .connecting:
                    ProgressView()
                        .tint(Color.gitpassPrimary)
                    Text(String(localized: "Connecting to the computer."))
                        .font(.subheadline)
                        .foregroundStyle(Color.gitpassSecondary)
                case .code(let code, let exp):
                    Text(String(localized: "Type this code on the computer. It is not a password."))
                        .font(.subheadline)
                        .foregroundStyle(Color.gitpassSecondary)
                        .multilineTextAlignment(.center)
                    Text(pairingGrouped(code))
                        .font(.system(size: 40, weight: .semibold, design: .monospaced))
                        .foregroundStyle(Color.gitpassPrimary)
                        .accessibilityIdentifier("pair-code")
                        .accessibilityLabel(pairingGrouped(code))
                    Text(String(localized: "Waiting for the computer."))
                        .font(.footnote)
                        .foregroundStyle(Color.gitpassSecondary)
                        .accessibilityIdentifier("pair-waiting")
                    Text(pairingTimeLeft(model.remaining(exp)))
                        .font(.footnote.monospacedDigit())
                        .foregroundStyle(Color.gitpassSecondary)
                case .password(let held):
                    Text(pairingRepoLine(owner: held.owner, repo: held.repo, branch: held.branch))
                        .font(.subheadline)
                        .foregroundStyle(Color.gitpassPrimary)
                        .multilineTextAlignment(.center)
                        .accessibilityIdentifier("pair-repo")
                    SecureField(String(localized: "Master password"), text: $model.password)
                        .textContentType(.password)
                        .textFieldStyle(.roundedBorder)
                        .autocapitalization(.none)
                        .disableAutocorrection(true)
                        .accessibilityIdentifier("pair-password")
                    Text(String(localized: "Type the Master Password on this phone. It stays on this phone."))
                        .font(.footnote)
                        .foregroundStyle(Color.gitpassSecondary)
                        .multilineTextAlignment(.center)
                    Button(String(localized: "Finish setup")) {
                        model.finish(store: store)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(Color.gitpassPrimary)
                    .foregroundStyle(.black)
                    .disabled(model.password.isEmpty)
                    .accessibilityIdentifier("pair-finish")
                case .working:
                    ProgressView()
                        .tint(Color.gitpassPrimary)
                    Text(String(localized: "Downloading the vault."))
                        .font(.subheadline)
                        .foregroundStyle(Color.gitpassSecondary)
                case .failed(let notice):
                    Text(pairingNoticeText(notice))
                        .font(.subheadline)
                        .foregroundStyle(Color.gitpassPrimary)
                        .multilineTextAlignment(.center)
                        .accessibilityIdentifier("pair-error")
                    Button(String(localized: "Start again")) {
                        model.restart()
                    }
                    .buttonStyle(.bordered)
                    .tint(Color.gitpassPrimary)
                    .accessibilityIdentifier("pair-restart")
                }
                Spacer(minLength: 0)
            }
            .padding()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .gitpassBackground()
            .privacySensitive()
            .navigationTitle(String(localized: "Set up this phone"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(String(localized: "Cancel")) {
                        model.cancel()
                        dismiss()
                    }
                    .accessibilityIdentifier("pair-cancel")
                }
            }
        }
        .onAppear {
            model.onUnlocked = { dismiss() }
            if let qr = PairingScanInjector.take() {
                model.start(qr)
            }
        }
        .onDisappear { model.cancel() }
        .onReceive(Timer.publish(every: 1, on: .main, in: .common).autoconnect()) { date in
            model.tick(date)
        }
    }
}

private func pairingGrouped(_ code: String) -> String {
    var parts: [String] = []
    var rest = code
    while !rest.isEmpty {
        parts.append(String(rest.prefix(3)))
        rest = String(rest.dropFirst(3))
    }
    return parts.joined(separator: " ")
}

enum PairingStep {
    case scan
    case connecting
    case code(String, Int64)
    case password(HeldGrant)
    case working
    case failed(PairingNotice)
}

final class PairingModel: ObservableObject {
    @Published var step: PairingStep = .scan
    @Published var password = ""
    @Published private(set) var now = Date()
    var onUnlocked: () -> Void = {}

    private let stateDir: URL
    private var generation = 0
    private var task: Task<Void, Never>?
    private var session: PairingSession?
    private var cancellation: SyncCancellation?

    init(stateDir: URL = PairingPaths.stateDirectory()) {
        self.stateDir = stateDir
    }

    deinit {
        session?.close()
        cancellation?.cancel()
    }

    func remaining(_ exp: Int64) -> Int64 {
        max(0, exp - Int64(now.timeIntervalSince1970))
    }

    func tick(_ date: Date) {
        now = date
        if case .code(_, let exp) = step, remaining(exp) == 0 {
            generation += 1
            session?.close()
            step = .failed(.link(.expired))
        }
    }

    func start(_ qr: String) {
        guard case .scan = step else { return }
        guard isDesktopQr(qr) else { return }
        step = .connecting
        let mine = generation
        let oldTask = task
        let oldSession = session
        task = Task.detached { [stateDir, weak self] in
            oldTask?.cancel()
            oldSession?.close()
            await oldTask?.value
            if Task.isCancelled { return }
            let next = PairingSession(stateDir: stateDir)
            let allowed = await MainActor.run { () -> Bool in
                guard let self, mine == self.generation else { return false }
                self.session = next
                return true
            }
            guard allowed else {
                next.close()
                return
            }
            await self?.drive(next, qr, mine)
        }
    }

    func cameraUnavailable() {
        guard case .scan = step else { return }
        step = .failed(.camera)
    }

    func restart() {
        generation += 1
        cancellation?.cancel()
        cancellation = nil
        task?.cancel()
        session?.close()
        session = nil
        password = ""
        step = .scan
    }

    func cancel() {
        generation += 1
        cancellation?.cancel()
        cancellation = nil
        task?.cancel()
        session?.close()
        session = nil
        password = ""
    }

    func finish(store: VaultStore) {
        guard case .password(let held) = step else { return }
        let typed = password
        password = ""
        guard !typed.isEmpty else { return }
        guard isValidSyncToken(held.token),
              let target = SyncTarget.validated(owner: held.owner, repo: held.repo, branch: held.branch)
        else {
            step = .failed(.link(.failed))
            return
        }
        step = .working
        let mine = generation
        let token = held.token
        let secret = held.secret
        let root = MainActor.assumeIsolated { store.vaultRoot }
        let cancellation = SyncCancellation()
        self.cancellation = cancellation
        Task.detached { [weak self] in
            let result: Result<Void, Error> = Result {
                try GitHubSyncService().firstPull(
                    localRoot: root, target: target, token: token,
                    password: typed, secretKey: secret, cancellation: cancellation
                )
            }
            await MainActor.run {
                guard let self, mine == self.generation else { return }
                self.cancellation = nil
                switch result {
                case .success:
                    do {
                        try store.sync.settings.save(target: target, token: token)
                        store.sync.settingsChanged()
                        try store.unlock(password: typed, secretKey: secret)
                        self.password = ""
                        self.onUnlocked()
                    } catch {
                        self.step = .failed(.link(.failed))
                    }
                case .failure:
                    self.step = .failed(.pull)
                }
            }
        }
    }

    private func drive(_ session: PairingSession, _ qr: String, _ mine: Int) async {
        let connected: PairingConnected
        do {
            connected = try session.connectUntilCode(qr)
        } catch {
            await publish((error as? PairingLinkError) ?? .failed, mine)
            return
        }
        let show = await MainActor.run { () -> Bool in
            guard mine == generation else { return false }
            step = .code(connected.code, connected.expiresAt)
            return true
        }
        guard show else { return }
        let paired: PairedSync
        do {
            paired = try session.receiveGrant()
        } catch {
            await publish((error as? PairingLinkError) ?? .failed, mine)
            return
        }
        let held = HeldGrant(
            owner: paired.repoOwner(), repo: paired.repoName(), branch: paired.branch(),
            token: paired.syncToken(), secret: paired.secretKey()
        )
        await MainActor.run {
            guard mine == generation else { return }
            step = .password(held)
        }
    }

    private func publish(_ error: PairingLinkError, _ mine: Int) async {
        await MainActor.run {
            guard mine == generation else { return }
            step = .failed(.link(error))
        }
    }
}

private struct PairingCamera: UIViewRepresentable {
    var onCode: (String) -> Void
    var onUnavailable: () -> Void

    func makeUIView(context: Context) -> PreviewView {
        let view = PreviewView()
        context.coordinator.start(in: view)
        return view
    }

    func updateUIView(_ uiView: PreviewView, context: Context) {}

    static func dismantleUIView(_ uiView: PreviewView, coordinator: Coordinator) {
        coordinator.stop()
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onCode: onCode, onUnavailable: onUnavailable)
    }

    final class PreviewView: UIView {
        override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }
        var preview: AVCaptureVideoPreviewLayer { layer as! AVCaptureVideoPreviewLayer }
    }

    final class Coordinator: NSObject, AVCaptureMetadataOutputObjectsDelegate {
        let onCode: (String) -> Void
        let onUnavailable: () -> Void
        let session = AVCaptureSession()
        private let cameraQueue = DispatchQueue(label: "dev.gitpass.pairing.camera")
        private var delivered = false

        init(onCode: @escaping (String) -> Void, onUnavailable: @escaping () -> Void) {
            self.onCode = onCode
            self.onUnavailable = onUnavailable
        }

        deinit { stop() }

        func start(in view: PreviewView) {
            guard PairingScanInjector.cameraEnabled else { return }
            view.preview.session = session
            view.preview.videoGravity = .resizeAspectFill
            switch AVCaptureDevice.authorizationStatus(for: .video) {
            case .authorized:
                configure()
            case .notDetermined:
                AVCaptureDevice.requestAccess(for: .video) { [weak self] ok in
                    DispatchQueue.main.async {
                        if ok { self?.configure() } else { self?.onUnavailable() }
                    }
                }
            default:
                onUnavailable()
            }
        }

        private func configure() {
            guard let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back),
                  let input = try? AVCaptureDeviceInput(device: device),
                  session.canAddInput(input)
            else {
                onUnavailable()
                return
            }
            session.beginConfiguration()
            session.addInput(input)
            let output = AVCaptureMetadataOutput()
            guard session.canAddOutput(output) else {
                session.commitConfiguration()
                onUnavailable()
                return
            }
            session.addOutput(output)
            output.setMetadataObjectsDelegate(self, queue: .main)
            output.metadataObjectTypes = [.qr]
            session.commitConfiguration()
            cameraQueue.async { [session] in
                if !session.isRunning { session.startRunning() }
            }
        }

        func metadataOutput(
            _ output: AVCaptureMetadataOutput,
            didOutput objects: [AVMetadataObject],
            from connection: AVCaptureConnection
        ) {
            guard !delivered,
                  let code = objects.compactMap({ $0 as? AVMetadataMachineReadableCodeObject }).first?.stringValue,
                  isDesktopQr(code)
            else { return }
            delivered = true
            stop()
            onCode(code)
        }

        func stop() {
            for output in session.outputs {
                (output as? AVCaptureMetadataOutput)?.setMetadataObjectsDelegate(nil, queue: nil)
            }
            let session = self.session
            cameraQueue.async {
                if session.isRunning { session.stopRunning() }
            }
        }
    }
}
