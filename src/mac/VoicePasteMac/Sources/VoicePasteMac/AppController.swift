import AppKit
import ServiceManagement

final class AppController {
    private let settingsStore = SettingsStore()
    private var settings = AppSettings()

    private let hotkeyManager = HotkeyManager()
    private let audioRecorder = AudioRecorder()
    private let transcriptionService = TranscriptionService()
    private let pasteService = PasteService()
    private let overlay = OverlayWindowController()

    private var statusItem: NSStatusItem?
    private var settingsWindow: SettingsWindowController?
    private var state: AppState = .idle {
        didSet { updateUI() }
    }
    private var lastHotkeyAt = Date.distantPast

    func start() {
        settings = settingsStore.load()
        configureStatusItem()
        configureServices()
        applySettings(settings)

        if settings.preloadModelOnLaunch {
            transcriptionService.preload(settings: settings)
        }

        requestStartupPermissionsIfNeeded()
        updateUI()
        Logger.shared.info("VoicePaste Mac ready. Config: \(settingsStore.path)")
    }

    func shutdown() {
        hotkeyManager.unregister()
        if audioRecorder.isRecording {
            _ = try? audioRecorder.stop()
        }
    }

    private func configureServices() {
        hotkeyManager.onPressed = { [weak self] in
            self?.handleHotkey()
        }
        audioRecorder.onLevelChanged = { [weak self] level in
            self?.overlay.updateLevel(level)
        }
    }

    private func configureStatusItem() {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.title = "VP"
        item.button?.toolTip = "VoicePaste"
        statusItem = item
        updateMenu()
    }

    private func applySettings(_ newSettings: AppSettings) {
        settings = newSettings
        do {
            try settingsStore.save(settings)
            try hotkeyManager.register(settings.hotkey)
            setLaunchAtLogin(settings.launchAtLogin)
            Logger.shared.info("Settings applied")
        } catch {
            NSAlert.showError("Failed to apply settings", informative: error.localizedDescription)
            Logger.shared.error("Failed to apply settings: \(error.localizedDescription)")
        }
        updateUI()
    }

    private func handleHotkey() {
        let now = Date()
        guard now.timeIntervalSince(lastHotkeyAt) > 0.3 else { return }
        lastHotkeyAt = now

        switch state {
        case .idle:
            startRecording()
        case .recording:
            stopRecordingAndTranscribe()
        case .transcribing:
            Logger.shared.info("Ignoring hotkey while transcribing")
        }
    }

    @objc private func startStopFromMenu() {
        handleHotkey()
    }

    private func startRecording() {
        Permissions.requestMicrophone { [weak self] granted in
            guard let self else { return }
            guard granted else {
                NSAlert.showError("Microphone permission is required")
                return
            }

            do {
                pasteService.captureFocusedApplication()
                state = .recording
                overlay.showRecording()
                try audioRecorder.start()
            } catch {
                state = .idle
                overlay.hideOverlay()
                NSAlert.showError("Failed to start recording", informative: error.localizedDescription)
                Logger.shared.error("Failed to start recording: \(error.localizedDescription)")
            }
        }
    }

    private func stopRecordingAndTranscribe() {
        do {
            let audioURL = try audioRecorder.stop()
            state = .transcribing
            overlay.showTranscribing()

            Task { [settings, transcriptionService, pasteService, overlay] in
                do {
                    let text = try await transcriptionService.transcribe(audioURL: audioURL, settings: settings)
                    try? FileManager.default.removeItem(at: audioURL)

                    if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        await MainActor.run {
                            NSAlert.showError("No speech detected")
                        }
                    } else {
                        let pasted = await pasteService.paste(text, delayMs: settings.pasteDelayMs)
                        if !pasted {
                            await MainActor.run {
                                NSAlert.showError("Paste failed", informative: "Grant Accessibility permission and try again. The transcript is already in the clipboard.")
                            }
                        }
                    }

                    await MainActor.run {
                        overlay.hideOverlay()
                        self.state = .idle
                    }
                } catch {
                    try? FileManager.default.removeItem(at: audioURL)
                    await MainActor.run {
                        overlay.hideOverlay()
                        self.state = .idle
                        NSAlert.showError("Transcription failed", informative: error.localizedDescription)
                    }
                    Logger.shared.error("Transcription failed: \(error.localizedDescription)")
                }
            }
        } catch {
            state = .idle
            overlay.hideOverlay()
            NSAlert.showError("Failed to stop recording", informative: error.localizedDescription)
        }
    }

    @objc private func openSettings() {
        let controller = SettingsWindowController(store: settingsStore, settings: settings) { [weak self] newSettings in
            self?.applySettings(newSettings)
        }
        settingsWindow = controller
        controller.window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc private func checkPermissions() {
        Permissions.requestAccessibilityPrompt()
        Permissions.requestMicrophone { _ in }
        let alert = NSAlert()
        alert.messageText = "VoicePaste Permissions"
        alert.informativeText = "Microphone: \(Permissions.microphoneStatus.rawValue)\nAccessibility: \(Permissions.accessibilityStatus.rawValue)\n\nAccessibility is required to paste with Cmd+V."
        alert.addButton(withTitle: "Open Accessibility")
        alert.addButton(withTitle: "Open Microphone")
        alert.addButton(withTitle: "OK")
        let result = alert.runModal()
        if result == .alertFirstButtonReturn {
            Permissions.openPrivacySettings()
        } else if result == .alertSecondButtonReturn {
            Permissions.openMicrophoneSettings()
        }
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }

    private func requestStartupPermissionsIfNeeded() {
        if Permissions.accessibilityStatus != .granted {
            Permissions.requestAccessibilityPrompt()
        }
        if Permissions.microphoneStatus == .unknown {
            Permissions.requestMicrophone { _ in }
        }
    }

    private func updateUI() {
        statusItem?.button?.title = state == .recording ? "VP REC" : state == .transcribing ? "VP ..." : "VP"
        statusItem?.button?.toolTip = "VoicePaste - \(state.rawValue) (\(settings.hotkey.rawValue))"
        updateMenu()
    }

    private func updateMenu() {
        let menu = NSMenu()
        let startTitle = state == .recording ? "Stop Recording" : "Start Recording"
        let startItem = NSMenuItem(title: startTitle, action: #selector(startStopFromMenu), keyEquivalent: "")
        startItem.target = self
        startItem.isEnabled = state != .transcribing
        menu.addItem(startItem)
        menu.addItem(.separator())

        let settingsItem = NSMenuItem(title: "Settings...", action: #selector(openSettings), keyEquivalent: ",")
        settingsItem.target = self
        menu.addItem(settingsItem)

        let permissionItem = NSMenuItem(title: "Check Permissions", action: #selector(checkPermissions), keyEquivalent: "")
        permissionItem.target = self
        menu.addItem(permissionItem)
        menu.addItem(.separator())

        let quitItem = NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)
        statusItem?.menu = menu
    }

    private func setLaunchAtLogin(_ enabled: Bool) {
        if #available(macOS 13.0, *) {
            do {
                if enabled {
                    try SMAppService.mainApp.register()
                } else if SMAppService.mainApp.status == .enabled {
                    try SMAppService.mainApp.unregister()
                }
            } catch {
                Logger.shared.warning("Launch at login update failed: \(error.localizedDescription)")
            }
        }
    }
}
