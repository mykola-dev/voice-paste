import Foundation

enum AppState: String {
    case idle = "Idle"
    case recording = "Recording"
    case transcribing = "Transcribing"
}

enum LanguageMode: String, Codable, CaseIterable, Identifiable {
    case auto
    case en
    case ua
    case bilingual

    var id: String { rawValue }
    var title: String {
        switch self {
        case .auto: return "Auto"
        case .en: return "English"
        case .ua: return "Ukrainian"
        case .bilingual: return "English + Ukrainian"
        }
    }
}

enum MacHotkey: String, Codable, CaseIterable, Identifiable {
    case rightOption = "RightOption"
    case f13 = "F13"
    case f14 = "F14"
    case f15 = "F15"
    case controlSpace = "Ctrl+Space"
    case optionSpace = "Option+Space"

    var id: String { rawValue }

    var title: String {
        switch self {
        case .rightOption: return "Right Option"
        default: return rawValue
        }
    }
}

struct AppSettings: Codable {
    var version: Int = 1
    var hotkey: MacHotkey = .rightOption
    var model: String = "openai-whisper:large-v3-turbo"
    var languageMode: LanguageMode = .bilingual
    var initialPrompt: String = "English and Ukrainian. Transcribe in these languages. Українська мова: ґ, є, і, ї."
    var pasteDelayMs: Int = 120
    var preloadModelOnLaunch: Bool = true
    var launchAtLogin: Bool = false
    var debugLogging: Bool = false
}

final class SettingsStore {
    private let configURL: URL

    init(configURL: URL? = nil) {
        self.configURL = configURL ?? Self.defaultConfigURL
    }

    var path: String { configURL.path }

    static var defaultConfigURL: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return base.appendingPathComponent("VoicePaste", isDirectory: true).appendingPathComponent("mac.config.json")
    }

    func load() -> AppSettings {
        do {
            let data = try Data(contentsOf: configURL)
            let settings = try JSONDecoder().decode(AppSettings.self, from: data)
            return validate(settings)
        } catch {
            return AppSettings()
        }
    }

    func save(_ settings: AppSettings) throws {
        let validated = validate(settings)
        let dir = configURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(validated).write(to: configURL, options: .atomic)
    }

    private func validate(_ settings: AppSettings) -> AppSettings {
        var value = settings
        value.version = 1
        value.model = value.model.trimmingCharacters(in: .whitespacesAndNewlines)
        if value.model.isEmpty {
            value.model = AppSettings().model
        }
        value.initialPrompt = value.initialPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        value.pasteDelayMs = min(max(value.pasteDelayMs, 0), 2000)
        return value
    }
}
