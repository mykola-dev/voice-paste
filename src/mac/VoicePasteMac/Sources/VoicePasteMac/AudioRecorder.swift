import AVFoundation

final class AudioRecorder {
    var onLevelChanged: ((Float) -> Void)?

    private let engine = AVAudioEngine()
    private var outputFile: AVAudioFile?
    private var converter: AVAudioConverter?
    private var recordingURL: URL?
    private(set) var isRecording = false

    func start() throws {
        guard !isRecording else { throw NSError.voicePaste("Already recording") }
        guard Permissions.microphoneStatus == .granted else { throw NSError.voicePaste("Microphone permission is required") }

        let tempDir = FileManager.default.temporaryDirectory.appendingPathComponent("VoicePaste", isDirectory: true)
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        let url = tempDir.appendingPathComponent("recording_\(Self.timestamp()).wav")
        recordingURL = url

        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)
        guard let targetFormat = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 16_000, channels: 1, interleaved: false) else {
            throw NSError.voicePaste("Failed to create target audio format")
        }

        converter = AVAudioConverter(from: inputFormat, to: targetFormat)
        outputFile = try AVAudioFile(forWriting: url, settings: targetFormat.settings, commonFormat: .pcmFormatInt16, interleaved: false)

        input.removeTap(onBus: 0)
        input.installTap(onBus: 0, bufferSize: 4096, format: inputFormat) { [weak self] buffer, _ in
            self?.write(buffer: buffer, inputFormat: inputFormat, targetFormat: targetFormat)
            self?.onLevelChanged?(Self.rmsLevel(buffer: buffer))
        }

        engine.prepare()
        try engine.start()
        isRecording = true
        Logger.shared.info("Recording started: \(url.path)")
    }

    func stop() throws -> URL {
        guard isRecording, let recordingURL else { throw NSError.voicePaste("Not recording") }
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        outputFile = nil
        converter = nil
        isRecording = false
        Logger.shared.info("Recording stopped: \(recordingURL.path)")
        return recordingURL
    }

    private func write(buffer: AVAudioPCMBuffer, inputFormat: AVAudioFormat, targetFormat: AVAudioFormat) {
        guard let converter, let outputFile else { return }

        let ratio = targetFormat.sampleRate / inputFormat.sampleRate
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 32
        guard let converted = AVAudioPCMBuffer(pcmFormat: targetFormat, frameCapacity: capacity) else { return }

        var didProvideInput = false
        let inputBlock: AVAudioConverterInputBlock = { _, status in
            if didProvideInput {
                status.pointee = .noDataNow
                return nil
            }
            didProvideInput = true
            status.pointee = .haveData
            return buffer
        }

        var error: NSError?
        converter.convert(to: converted, error: &error, withInputFrom: inputBlock)
        if let error {
            Logger.shared.warning("Audio conversion failed: \(error.localizedDescription)")
            return
        }

        do {
            try outputFile.write(from: converted)
        } catch {
            Logger.shared.warning("Audio write failed: \(error.localizedDescription)")
        }
    }

    private static func rmsLevel(buffer: AVAudioPCMBuffer) -> Float {
        guard let channel = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return 0 }
        var sum: Float = 0
        for index in 0..<Int(buffer.frameLength) {
            let sample = channel[index]
            sum += sample * sample
        }
        let rms = sqrt(sum / Float(buffer.frameLength))
        return min(rms * 5, 1)
    }

    private static func timestamp() -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyyMMdd_HHmmss_SSS"
        return formatter.string(from: Date())
    }
}

extension NSError {
    static func voicePaste(_ message: String) -> NSError {
        NSError(domain: "VoicePasteMac", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }
}
