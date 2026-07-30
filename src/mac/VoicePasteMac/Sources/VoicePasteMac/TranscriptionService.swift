import Foundation

final class TranscriptionService {
    private let workerURL: URL
    private var preloadProcess: Process?

    init() {
        let bundleWorker = Bundle.main.resourceURL?.appendingPathComponent("transcribe_mlx.py")
        let devWorker = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
            .deletingLastPathComponent()
            .appendingPathComponent("transcribe_mlx.py")
        if let bundleWorker, FileManager.default.fileExists(atPath: bundleWorker.path) {
            workerURL = bundleWorker
        } else {
            workerURL = devWorker
        }
    }

    func preload(settings: AppSettings) {
        guard preloadProcess == nil else { return }
        Task.detached { [weak self] in
            do {
                _ = try await self?.run(args: ["--model", settings.model, "--preload"], timeoutSeconds: 600)
                Logger.shared.info("MLX model preloaded")
            } catch {
                Logger.shared.warning("MLX preload failed: \(error.localizedDescription)")
            }
        }
    }

    func transcribe(audioURL: URL, settings: AppSettings) async throws -> String {
        var args = [
            "--input", audioURL.path,
            "--model", settings.model,
            "--language-mode", settings.languageMode.rawValue
        ]

        if !settings.initialPrompt.isEmpty {
            args.append(contentsOf: ["--initial-prompt", settings.initialPrompt])
        }

        let output = try await run(args: args, timeoutSeconds: 180)
        return output.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func run(args: [String], timeoutSeconds: TimeInterval) async throws -> String {
        let workerPath = workerURL.path
        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                let process = Process()
                let stdout = Pipe()
                let stderr = Pipe()
                let outputQueue = DispatchQueue(label: "VoicePasteMac.TranscriptionOutput")
                var outData = Data()
                var errData = Data()

                let python = Self.pythonExecutableURL()
                process.executableURL = python
                if python.path == "/usr/bin/env" {
                    process.arguments = ["python3", workerPath] + args
                } else {
                    process.arguments = [workerPath] + args
                }
                process.standardOutput = stdout
                process.standardError = stderr
                process.environment = Self.workerEnvironment()

                stdout.fileHandleForReading.readabilityHandler = { handle in
                    let data = handle.availableData
                    guard !data.isEmpty else { return }
                    outputQueue.async {
                        outData.append(data)
                    }
                }
                stderr.fileHandleForReading.readabilityHandler = { handle in
                    let data = handle.availableData
                    guard !data.isEmpty else { return }
                    outputQueue.async {
                        errData.append(data)
                    }
                }

                let timer = DispatchSource.makeTimerSource(queue: DispatchQueue.global(qos: .userInitiated))
                timer.schedule(deadline: .now() + timeoutSeconds)
                timer.setEventHandler {
                    if process.isRunning {
                        process.terminate()
                    }
                }
                timer.resume()

                do {
                    try process.run()
                    process.waitUntilExit()
                    timer.cancel()

                    stdout.fileHandleForReading.readabilityHandler = nil
                    stderr.fileHandleForReading.readabilityHandler = nil

                    let output = outputQueue.sync { (outData, errData) }
                    let out = String(data: output.0, encoding: .utf8) ?? ""
                    let err = String(data: output.1, encoding: .utf8) ?? ""

                    if !err.isEmpty {
                        Logger.shared.info("MLX stderr: \(err)")
                    }

                    guard process.terminationStatus == 0 else {
                        throw NSError.voicePaste("MLX transcription failed: \(err.isEmpty ? out : err)")
                    }

                    continuation.resume(returning: out)
                } catch {
                    timer.cancel()
                    stdout.fileHandleForReading.readabilityHandler = nil
                    stderr.fileHandleForReading.readabilityHandler = nil
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    private static func pythonExecutableURL() -> URL {
        if let embedded = Bundle.main.resourceURL?.appendingPathComponent("python/bin/python3"), FileManager.default.isExecutableFile(atPath: embedded.path) {
            return embedded
        }
        if let venv = ProcessInfo.processInfo.environment["VOICEPASTE_MAC_PYTHON"], !venv.isEmpty {
            return URL(fileURLWithPath: venv)
        }
        return URL(fileURLWithPath: "/usr/bin/env")
    }

    private static func workerEnvironment() -> [String: String] {
        var env = ProcessInfo.processInfo.environment
        env["PYTHONIOENCODING"] = "utf-8"
        env["PYTHONUTF8"] = "1"
        if pythonExecutableURL().path == "/usr/bin/env" {
            env["VOICEPASTE_USE_ENV_PYTHON"] = "1"
        }
        return env
    }
}
