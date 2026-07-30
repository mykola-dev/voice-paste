import AppKit

final class PasteService {
    private var previousApp: NSRunningApplication?

    func captureFocusedApplication() {
        previousApp = NSWorkspace.shared.frontmostApplication
        Logger.shared.info("Captured focused app: \(previousApp?.localizedName ?? "unknown")")
    }

    func paste(_ text: String, delayMs: Int) async -> Bool {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return true
        }

        await MainActor.run {
            let pasteboard = NSPasteboard.general
            pasteboard.clearContents()
            pasteboard.setString(text, forType: .string)
        }

        if let previousApp {
            _ = previousApp.activate(options: [.activateIgnoringOtherApps])
        }

        if delayMs > 0 {
            try? await Task.sleep(nanoseconds: UInt64(delayMs) * 1_000_000)
        }

        return sendCommandV()
    }

    private func sendCommandV() -> Bool {
        guard Permissions.accessibilityStatus == .granted else {
            Permissions.requestAccessibilityPrompt()
            Logger.shared.error("Accessibility permission is required for paste")
            return false
        }

        let source = CGEventSource(stateID: .hidSystemState)
        let vKey: CGKeyCode = 9
        let flags: CGEventFlags = .maskCommand

        guard let down = CGEvent(keyboardEventSource: source, virtualKey: vKey, keyDown: true),
              let up = CGEvent(keyboardEventSource: source, virtualKey: vKey, keyDown: false) else {
            return false
        }

        down.flags = flags
        up.flags = flags
        down.post(tap: .cghidEventTap)
        up.post(tap: .cghidEventTap)
        return true
    }
}
