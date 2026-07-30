import SwiftUI
import AppKit

final class SettingsWindowController: NSWindowController {
    private let store: SettingsStore
    private let onSave: (AppSettings) -> Void

    init(store: SettingsStore, settings: AppSettings, onSave: @escaping (AppSettings) -> Void) {
        self.store = store
        self.onSave = onSave
        let model = SettingsViewModel(settings: settings)
        let view = SettingsView(model: model, configPath: store.path) { newSettings in
            onSave(newSettings)
        }
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 560, height: 560),
            styleMask: [.titled, .closable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = "VoicePaste Settings"
        window.contentView = NSHostingView(rootView: view)
        window.center()
        super.init(window: window)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

final class SettingsViewModel: ObservableObject {
    @Published var hotkey: MacHotkey
    @Published var model: String
    @Published var languageMode: LanguageMode
    @Published var initialPrompt: String
    @Published var pasteDelayMs: Int
    @Published var preloadModelOnLaunch: Bool
    @Published var launchAtLogin: Bool
    @Published var debugLogging: Bool
    @Published var microphoneStatus: PermissionStatus = .unknown
    @Published var accessibilityStatus: PermissionStatus = .unknown

    init(settings: AppSettings) {
        hotkey = settings.hotkey
        model = settings.model
        languageMode = settings.languageMode
        initialPrompt = settings.initialPrompt
        pasteDelayMs = settings.pasteDelayMs
        preloadModelOnLaunch = settings.preloadModelOnLaunch
        launchAtLogin = settings.launchAtLogin
        debugLogging = settings.debugLogging
        refreshPermissions()
    }

    var settings: AppSettings {
        AppSettings(
            hotkey: hotkey,
            model: model,
            languageMode: languageMode,
            initialPrompt: initialPrompt,
            pasteDelayMs: pasteDelayMs,
            preloadModelOnLaunch: preloadModelOnLaunch,
            launchAtLogin: launchAtLogin,
            debugLogging: debugLogging
        )
    }

    func refreshPermissions() {
        microphoneStatus = Permissions.microphoneStatus
        accessibilityStatus = Permissions.accessibilityStatus
    }
}

struct SettingsView: View {
    @ObservedObject var model: SettingsViewModel
    let configPath: String
    let onSave: (AppSettings) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("VoicePaste Mac")
                .font(.title2.weight(.semibold))

            Form {
                Picker("Hotkey", selection: $model.hotkey) {
                    ForEach(MacHotkey.allCases) { key in
                        Text(key.title).tag(key)
                    }
                }

                TextField("MLX model", text: $model.model)

                Picker("Language", selection: $model.languageMode) {
                    ForEach(LanguageMode.allCases) { mode in
                        Text(mode.title).tag(mode)
                    }
                }

                TextField("Initial prompt", text: $model.initialPrompt, axis: .vertical)
                    .lineLimit(3...5)

                Stepper("Paste delay: \(model.pasteDelayMs) ms", value: $model.pasteDelayMs, in: 0...2000, step: 20)

                Toggle("Preload model on launch", isOn: $model.preloadModelOnLaunch)
                Toggle("Launch at login", isOn: $model.launchAtLogin)
                Toggle("Debug logging", isOn: $model.debugLogging)
            }

            Divider()

            VStack(alignment: .leading, spacing: 6) {
                Text("Config: \(configPath)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("Microphone: \(model.microphoneStatus.rawValue)   Accessibility: \(model.accessibilityStatus.rawValue)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            HStack {
                Button("Open Privacy Settings") {
                    Permissions.openPrivacySettings()
                }
                Button("Refresh") {
                    model.refreshPermissions()
                }
                Spacer()
                Button("Save") {
                    onSave(model.settings)
                    NSApp.keyWindow?.close()
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(24)
        .frame(minWidth: 540, minHeight: 520)
        .onAppear {
            model.refreshPermissions()
        }
    }
}
