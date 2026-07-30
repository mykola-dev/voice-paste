import Foundation

final class Logger {
    static let shared = Logger()

    let logURL: URL
    private let queue = DispatchQueue(label: "VoicePasteMac.Logger")

    private init() {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("VoicePaste", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        logURL = dir.appendingPathComponent("voicepaste-mac.log")
    }

    func info(_ message: String) {
        write("INFO", message)
    }

    func warning(_ message: String) {
        write("WARN", message)
    }

    func error(_ message: String) {
        write("ERROR", message)
    }

    private func write(_ level: String, _ message: String) {
        let line = "[\(Self.timestamp())] [\(level)] \(message)\n"
        queue.async { [logURL] in
            if let data = line.data(using: .utf8) {
                if FileManager.default.fileExists(atPath: logURL.path), let handle = try? FileHandle(forWritingTo: logURL) {
                    _ = try? handle.seekToEnd()
                    try? handle.write(contentsOf: data)
                    try? handle.close()
                } else {
                    try? data.write(to: logURL)
                }
            }
        }
        print(line, terminator: "")
    }

    private static func timestamp() -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss.SSS"
        return formatter.string(from: Date())
    }
}
