import AppKit

final class OverlayWindowController: NSWindowController {
    private let titleLabel = NSTextField(labelWithString: "Recording")
    private let timerLabel = NSTextField(labelWithString: "00:00")
    private let levelBar = NSProgressIndicator()
    private var timer: Timer?
    private var startedAt = Date()

    init() {
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 260, height: 96),
            styleMask: [.nonactivatingPanel, .borderless],
            backing: .buffered,
            defer: false
        )
        panel.level = .floating
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.ignoresMouseEvents = true
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        super.init(window: panel)
        setupContent()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func showRecording() {
        startedAt = Date()
        titleLabel.stringValue = "Recording"
        timerLabel.stringValue = "00:00"
        levelBar.isHidden = false
        showOverlay()
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            self?.updateTimer()
        }
    }

    func showTranscribing() {
        timer?.invalidate()
        titleLabel.stringValue = "Transcribing"
        timerLabel.stringValue = "Working..."
        levelBar.isHidden = true
        showOverlay()
    }

    func hideOverlay() {
        timer?.invalidate()
        window?.orderOut(nil)
    }

    func updateLevel(_ level: Float) {
        DispatchQueue.main.async { [levelBar] in
            levelBar.doubleValue = Double(max(0, min(level, 1)))
        }
    }

    private func setupContent() {
        guard let content = window?.contentView else { return }
        let blur = NSVisualEffectView(frame: content.bounds)
        blur.autoresizingMask = [.width, .height]
        blur.material = .hudWindow
        blur.blendingMode = .behindWindow
        blur.state = .active
        blur.wantsLayer = true
        blur.layer?.cornerRadius = 18
        content.addSubview(blur)

        titleLabel.font = .systemFont(ofSize: 15, weight: .semibold)
        titleLabel.textColor = .white
        timerLabel.font = .monospacedDigitSystemFont(ofSize: 22, weight: .medium)
        timerLabel.textColor = .white
        levelBar.minValue = 0
        levelBar.maxValue = 1
        levelBar.doubleValue = 0
        levelBar.isIndeterminate = false
        levelBar.controlSize = .small

        let stack = NSStackView(views: [titleLabel, timerLabel, levelBar])
        stack.orientation = .vertical
        stack.alignment = .centerX
        stack.spacing = 8
        stack.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(stack)

        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: content.centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: content.centerYAnchor),
            levelBar.widthAnchor.constraint(equalToConstant: 190)
        ])
    }

    private func showOverlay() {
        if let screen = NSScreen.main, let window {
            let frame = screen.visibleFrame
            window.setFrameOrigin(NSPoint(x: frame.maxX - window.frame.width - 24, y: frame.maxY - window.frame.height - 24))
        }
        window?.orderFrontRegardless()
    }

    private func updateTimer() {
        let elapsed = Int(Date().timeIntervalSince(startedAt))
        timerLabel.stringValue = String(format: "%02d:%02d", elapsed / 60, elapsed % 60)
    }
}
