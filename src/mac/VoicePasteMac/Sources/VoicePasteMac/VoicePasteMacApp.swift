import AppKit

@main
final class VoicePasteMacApp: NSObject, NSApplicationDelegate {
    private var controller: AppController?

    static func main() {
        let app = NSApplication.shared
        let delegate = VoicePasteMacApp()
        app.delegate = delegate
        app.setActivationPolicy(.accessory)
        app.run()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        guard ProcessInfo.processInfo.machineHardwareName == "arm64" else {
            NSAlert.showError("VoicePaste for macOS requires Apple Silicon. Intel Macs are not supported.")
            NSApp.terminate(nil)
            return
        }

        Logger.shared.info("=== VoicePaste Mac starting ===")
        controller = AppController()
        controller?.start()
    }

    func applicationWillTerminate(_ notification: Notification) {
        controller?.shutdown()
        Logger.shared.info("=== VoicePaste Mac stopped ===")
    }
}

private extension ProcessInfo {
    var machineHardwareName: String {
        var size = 0
        sysctlbyname("hw.machine", nil, &size, nil, 0)
        var machine = [CChar](repeating: 0, count: size)
        sysctlbyname("hw.machine", &machine, &size, nil, 0)
        return String(cString: machine)
    }
}

extension NSAlert {
    static func showError(_ message: String, informative: String = "") {
        DispatchQueue.main.async {
            let alert = NSAlert()
            alert.alertStyle = .critical
            alert.messageText = message
            alert.informativeText = informative
            alert.runModal()
        }
    }
}
