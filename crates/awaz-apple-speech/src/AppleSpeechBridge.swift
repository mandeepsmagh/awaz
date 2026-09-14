@preconcurrency import AVFoundation
import Foundation
import Speech

private struct BridgeEvent: Encodable {
    let type: String
    var text: String?
    var message: String?
}

private actor EventWriter {
    func send(_ event: BridgeEvent) {
        do {
            var data = try JSONEncoder().encode(event)
            data.append(0x0a)
            try FileHandle.standardOutput.write(contentsOf: data)
        } catch {
            FileHandle.standardError.write(Data("Apple Speech output failed: \(error)\n".utf8))
        }
    }
}

private enum BridgeError: LocalizedError {
    case unsupportedLocale(String)
    case noAudioFormat
    case malformedAudio
    case audioQueueFull
    case unknownCommand(UInt8)

    var errorDescription: String? {
        switch self {
        case .unsupportedLocale(let locale):
            "Apple Speech is not available for \(locale)."
        case .noAudioFormat:
            "Apple Speech did not provide a compatible audio format."
        case .malformedAudio:
            "The Apple Speech helper received malformed audio."
        case .audioQueueFull:
            "Apple Speech could not consume audio in real time."
        case .unknownCommand(let command):
            "The Apple Speech helper received unknown command \(command)."
        }
    }
}

private actor AppleRecognizer {
    private let locale: Locale
    private let writer: EventWriter
    private var transcriber: SpeechTranscriber?
    private var analyzer: SpeechAnalyzer?
    private var input: AsyncStream<AnalyzerInput>.Continuation?
    private var resultsTask: Task<Void, Never>?
    private var finalizedText = ""
    private var converter: AVAudioConverter?
    private var sourceFormat: AVAudioFormat?
    private var targetFormat: AVAudioFormat?

    init(language: String, writer: EventWriter) {
        locale = Locale(identifier: language)
        self.writer = writer
    }

    func prepare() async throws {
        guard SpeechTranscriber.isAvailable,
              let resolved = await SpeechTranscriber.supportedLocale(equivalentTo: locale)
        else {
            throw BridgeError.unsupportedLocale(locale.identifier)
        }
        let transcriber = Self.makeTranscriber(locale: resolved)
        try await Self.ensureModelInstalled(for: transcriber)
    }

    func start() async throws {
        guard analyzer == nil else { return }
        guard let resolved = await SpeechTranscriber.supportedLocale(equivalentTo: locale) else {
            throw BridgeError.unsupportedLocale(locale.identifier)
        }

        let transcriber = Self.makeTranscriber(locale: resolved)
        try await Self.ensureModelInstalled(for: transcriber)
        guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber]) else {
            throw BridgeError.noAudioFormat
        }

        let (stream, continuation) = AsyncStream<AnalyzerInput>.makeStream(
            bufferingPolicy: .bufferingOldest(1024)
        )
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        self.transcriber = transcriber
        self.analyzer = analyzer
        input = continuation
        targetFormat = format
        sourceFormat = nil
        converter = nil
        finalizedText = ""

        resultsTask = Task { [writer] in
            do {
                for try await result in transcriber.results {
                    if Task.isCancelled { return }
                    let snapshot = self.absorb(result)
                    await writer.send(BridgeEvent(type: "partial", text: snapshot))
                }
                if !Task.isCancelled {
                    await writer.send(BridgeEvent(type: "final", text: self.finalizedText))
                }
            } catch {
                if !Task.isCancelled {
                    await writer.send(BridgeEvent(type: "error", message: error.localizedDescription))
                }
            }
        }

        try await analyzer.start(inputSequence: stream)
    }

    func feed(samples: [Float], sampleRate: UInt32) throws {
        guard let targetFormat, let input else { return }
        guard !samples.isEmpty, sampleRate > 0,
              let incoming = AVAudioFormat(
                commonFormat: .pcmFormatFloat32,
                sampleRate: Double(sampleRate),
                channels: 1,
                interleaved: false
              ),
              let source = AVAudioPCMBuffer(
                pcmFormat: incoming,
                frameCapacity: AVAudioFrameCount(samples.count)
              ),
              let channel = source.floatChannelData?[0]
        else { throw BridgeError.malformedAudio }

        source.frameLength = AVAudioFrameCount(samples.count)
        channel.update(from: samples, count: samples.count)

        if incoming == targetFormat {
            guard case .enqueued = input.yield(AnalyzerInput(buffer: source)) else {
                throw BridgeError.audioQueueFull
            }
            return
        }

        if sourceFormat != incoming {
            sourceFormat = incoming
            converter = AVAudioConverter(from: incoming, to: targetFormat)
        }
        guard let converter else { throw BridgeError.noAudioFormat }

        let ratio = targetFormat.sampleRate / incoming.sampleRate
        let capacity = AVAudioFrameCount((Double(source.frameLength) * ratio).rounded(.up)) + 64
        guard let converted = AVAudioPCMBuffer(pcmFormat: targetFormat, frameCapacity: capacity) else {
            throw BridgeError.noAudioFormat
        }

        nonisolated(unsafe) let converterInput = source
        var supplied = false
        var conversionError: NSError?
        let status = converter.convert(to: converted, error: &conversionError) { _, outputStatus in
            if supplied {
                outputStatus.pointee = .noDataNow
                return nil
            }
            supplied = true
            outputStatus.pointee = .haveData
            return converterInput
        }
        if let conversionError { throw conversionError }
        guard status != .error else { throw BridgeError.noAudioFormat }
        if converted.frameLength > 0 {
            guard case .enqueued = input.yield(AnalyzerInput(buffer: converted)) else {
                throw BridgeError.audioQueueFull
            }
        }
    }

    func finish() async {
        input?.finish()
        input = nil
        do {
            try await analyzer?.finalizeAndFinishThroughEndOfInput()
        } catch {
            await writer.send(BridgeEvent(type: "error", message: error.localizedDescription))
            await analyzer?.cancelAndFinishNow()
        }
        await resultsTask?.value
        reset()
    }

    func cancel() async {
        input?.finish()
        input = nil
        resultsTask?.cancel()
        await analyzer?.cancelAndFinishNow()
        await resultsTask?.value
        reset()
    }

    private func reset() {
        analyzer = nil
        transcriber = nil
        resultsTask = nil
        sourceFormat = nil
        targetFormat = nil
        converter = nil
        finalizedText = ""
    }

    private func absorb(_ result: SpeechTranscriber.Result) -> String {
        let text = String(result.text.characters)
        if result.isFinal {
            finalizedText += text
            return finalizedText.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return (finalizedText + text).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func makeTranscriber(locale: Locale) -> SpeechTranscriber {
        SpeechTranscriber(
            locale: locale,
            transcriptionOptions: [],
            reportingOptions: [.volatileResults],
            attributeOptions: []
        )
    }

    private static func ensureModelInstalled(for transcriber: SpeechTranscriber) async throws {
        let installed = await SpeechTranscriber.installedLocales
        let selected = transcriber.selectedLocales
        let isInstalled = selected.allSatisfy { locale in
            installed.contains { $0.identifier(.bcp47) == locale.identifier(.bcp47) }
        }
        guard !isInstalled else { return }
        if let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
            try await request.downloadAndInstall()
        }
    }
}

private func readExactly(_ count: Int) throws -> Data? {
    var data = Data()
    while data.count < count {
        guard let part = try FileHandle.standardInput.read(upToCount: count - data.count),
              !part.isEmpty
        else {
            return data.isEmpty ? nil : data
        }
        data.append(part)
    }
    return data
}

@main
private struct AppleSpeechBridge {
    static func main() async {
        let writer = EventWriter()
        let language = CommandLine.arguments.dropFirst().first ?? "en"
        let recognizer = AppleRecognizer(language: language, writer: writer)

        do {
            try await recognizer.prepare()
            await writer.send(BridgeEvent(type: "ready"))
            var finishTask: Task<Void, Never>?

            while let header = try readExactly(5), header.count == 5 {
                let command = header[header.startIndex]
                let payloadCount = header.dropFirst().withUnsafeBytes {
                    Int(UInt32(littleEndian: $0.loadUnaligned(as: UInt32.self)))
                }
                guard let payload = try readExactly(payloadCount), payload.count == payloadCount else {
                    throw BridgeError.malformedAudio
                }

                switch command {
                case 1:
                    try await recognizer.start()
                    await writer.send(BridgeEvent(type: "started"))
                case 2:
                    guard payload.count >= 4, (payload.count - 4).isMultiple(of: 4) else {
                        throw BridgeError.malformedAudio
                    }
                    let sampleRate = payload.prefix(4).withUnsafeBytes {
                        UInt32(littleEndian: $0.loadUnaligned(as: UInt32.self))
                    }
                    var samples = [Float](repeating: 0, count: (payload.count - 4) / 4)
                    _ = samples.withUnsafeMutableBytes { destination in
                        payload.copyBytes(to: destination, from: 4..<payload.count)
                    }
                    try await recognizer.feed(samples: samples, sampleRate: sampleRate)
                case 3:
                    finishTask = Task.detached {
                        await recognizer.finish()
                        await writer.send(BridgeEvent(type: "finished"))
                    }
                case 4:
                    await recognizer.cancel()
                    await writer.send(BridgeEvent(type: "cancelled"))
                    await finishTask?.value
                    finishTask = nil
                case 5:
                    await recognizer.cancel()
                    await finishTask?.value
                    return
                default:
                    throw BridgeError.unknownCommand(command)
                }
            }
        } catch {
            await writer.send(BridgeEvent(type: "error", message: error.localizedDescription))
        }
    }
}
