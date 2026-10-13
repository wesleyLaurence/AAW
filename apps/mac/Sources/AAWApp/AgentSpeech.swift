import AVFoundation
import Foundation
import Speech

/// Something that turns the person's speech into text as it is said: the
/// Mac's own recognizer, or a stand-in that reads a file's words for a
/// scripted run. The panel owns one and shows what it hears in the field.
@MainActor
protocol Transcriber: AnyObject {
    /// Whether it can listen, and if not, why, in a line for the button's
    /// help and the refusal.
    var availability: Transcriber.Availability { get }
    /// Starts listening. `heard` is given the whole text so far each time
    /// it changes, and whether that is the last word; `level` the input's
    /// level from 0 to 1 as it goes; `ended` once, with what went wrong or
    /// nil, after which nothing more is said. Asks the permissions first,
    /// where they are not yet answered.
    func start(heard: @escaping @MainActor (String, Bool) -> Void,
               level: @escaping @MainActor (Float) -> Void,
               ended: @escaping @MainActor (String?) -> Void)
    /// Stops; `ended` is still called, once.
    func stop()
}

extension Transcriber {
    /// Whether it can listen now.
    var available: Bool {
        if case .ready = availability { return true }
        return false
    }
}

enum TranscriberAvailability: Equatable {
    case ready
    /// The Mac cannot recognize the system's locale on the device, and
    /// nothing is sent off it. The language comes down with Dictation in
    /// System Settings › Keyboard; with Dictation off, no locale has it.
    case noOnDeviceRecognition(locale: String)
    /// The person refused the microphone or speech recognition in System
    /// Settings › Privacy.
    case refused(String)
    /// The system has no recognizer at all.
    case unsupported
}

extension Transcriber {
    typealias Availability = TranscriberAvailability
}

/// How the panel chooses a transcriber: `AAW_SPEECH=FILE` names a file
/// whose words a stand-in yields, for scripted runs and tests, as
/// `AAW_AGENT` names a program; else the Mac's recognizer.
enum Transcribers {
    static let override = "AAW_SPEECH"

    @MainActor
    static func make(environment: [String: String] = ProcessInfo.processInfo.environment) -> any Transcriber {
        if let path = environment[override], !path.isEmpty {
            return FileTranscriber(file: URL(fileURLWithPath: path))
        }
        return SpeechTranscriber()
    }

    /// The words of the hypothesis so far that have firmed up: those in the
    /// common prefix of two hypotheses in a row, by word, so that what
    /// the recognizer has not changed since it last spoke is shown in the
    /// text's color and the rest in the faint one. Returns how many
    /// characters of `current` are firm, the space after the last firm
    /// word among them.
    static func firmLength(previous: String, current: String) -> Int {
        let a = previous.split(separator: " ", omittingEmptySubsequences: false)
        let b = current.split(separator: " ", omittingEmptySubsequences: false)
        var words = 0
        while words < a.count, words < b.count, a[words] == b[words] { words += 1 }
        // The last word of a hypothesis may still grow, so a word firms
        // only when one follows it.
        words = min(words, b.count - 1)
        guard words > 0 else { return 0 }
        return b[..<words].map(String.init).joined(separator: " ").utf16.count + 1
    }
}

// MARK: The Mac's recognizer

/// `SFSpeechRecognizer` over the input node of an `AVAudioEngine`, on the
/// device only, for the system's locale. The app's own engine plays the
/// song through Core Audio apart from this one, so the song keeps playing.
@MainActor
final class SpeechTranscriber: Transcriber {
    private let recognizer: SFSpeechRecognizer?
    private var engine: AVAudioEngine?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var ended: (@MainActor (String?) -> Void)?

    init(locale: Locale = .current) {
        recognizer = SFSpeechRecognizer(locale: locale) ?? SFSpeechRecognizer()
    }

    var availability: Availability {
        guard let recognizer else { return .unsupported }
        if !recognizer.supportsOnDeviceRecognition {
            return .noOnDeviceRecognition(locale: recognizer.locale.identifier)
        }
        if SFSpeechRecognizer.authorizationStatus() == .denied || SFSpeechRecognizer.authorizationStatus() == .restricted {
            return .refused("Speech recognition is off for AAW in System Settings › Privacy & Security › Speech Recognition")
        }
        if AVCaptureDevice.authorizationStatus(for: .audio) == .denied || AVCaptureDevice.authorizationStatus(for: .audio) == .restricted {
            return .refused("The microphone is off for AAW in System Settings › Privacy & Security › Microphone")
        }
        return .ready
    }

    func start(heard: @escaping @MainActor (String, Bool) -> Void,
               level: @escaping @MainActor (Float) -> Void,
               ended: @escaping @MainActor (String?) -> Void) {
        self.ended = ended
        guard let recognizer, available else {
            finish(Self.reason(availability))
            return
        }
        // The two permissions, each asked once by the system and kept.
        SFSpeechRecognizer.requestAuthorization { status in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard status == .authorized else {
                        self.finish("Speech recognition was not allowed for AAW")
                        return
                    }
                    AVCaptureDevice.requestAccess(for: .audio) { allowed in
                        DispatchQueue.main.async {
                            MainActor.assumeIsolated {
                                guard allowed else {
                                    self.finish("The microphone was not allowed for AAW")
                                    return
                                }
                                self.listen(recognizer, heard: heard, level: level)
                            }
                        }
                    }
                }
            }
        }
    }

    private func listen(_ recognizer: SFSpeechRecognizer,
                        heard: @escaping @MainActor (String, Bool) -> Void,
                        level: @escaping @MainActor (Float) -> Void) {
        guard ended != nil else { return }
        let engine = AVAudioEngine()
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.requiresOnDeviceRecognition = true
        request.addsPunctuation = true
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else {
            finish("No microphone is available")
            return
        }
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
            request.append(buffer)
            let heard = Self.level(of: buffer)
            DispatchQueue.main.async { MainActor.assumeIsolated { level(heard) } }
        }
        engine.prepare()
        do {
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            finish("The microphone could not be opened: \(error.localizedDescription)")
            return
        }
        self.engine = engine
        self.request = request
        task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self, self.ended != nil else { return }
                    if let result {
                        heard(result.bestTranscription.formattedString, result.isFinal)
                        if result.isFinal { self.finish(nil) }
                    }
                    if let error {
                        // Stopping ends the task with a cancellation, which is no error.
                        let code = (error as NSError).code
                        let cancelled = (error as NSError).domain == "kLSRErrorDomain" && code == 301
                            || (error as NSError).domain == "kAFAssistantErrorDomain" && (code == 216 || code == 1101)
                        self.finish(cancelled ? nil : error.localizedDescription)
                    }
                }
            }
        }
    }

    func stop() {
        request?.endAudio()
        task?.finish()
        // The recognizer's last result, if any, arrives with `isFinal`
        // and ends it; if nothing comes, end it here.
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { [weak self] in
            MainActor.assumeIsolated { self?.finish(nil) }
        }
    }

    private func finish(_ reason: String?) {
        guard let ended else { return }
        self.ended = nil
        engine?.inputNode.removeTap(onBus: 0)
        engine?.stop()
        engine = nil
        request = nil
        task?.cancel()
        task = nil
        ended(reason)
    }

    static func reason(_ availability: Availability) -> String {
        switch availability {
        case .ready: ""
        case .noOnDeviceRecognition(let locale):
            "This Mac cannot yet recognize \(locale) speech on the device, and nothing is sent off it: turn on Dictation in System Settings › Keyboard, which downloads the language"
        case .refused(let why): why
        case .unsupported: "This Mac has no speech recognizer"
        }
    }

    /// The buffer's level, 0 to 1: its RMS over the first channel, with
    /// −50 dB as silence and full scale as 1.
    nonisolated static func level(of buffer: AVAudioPCMBuffer) -> Float {
        guard let data = buffer.floatChannelData, buffer.frameLength > 0 else { return 0 }
        let samples = UnsafeBufferPointer(start: data[0], count: Int(buffer.frameLength))
        let rms = (samples.reduce(0) { $0 + $1 * $1 } / Float(samples.count)).squareRoot()
        let db = 20 * log10(max(rms, 1e-6))
        return min(max((db + 50) / 50, 0), 1)
    }
}

// MARK: The stand-in

/// Yields a file's words one at a time, a tenth of a second apart, each
/// time with the whole text so far, then the whole text as final; the
/// level rises and falls with each word. For scripted runs and tests.
@MainActor
final class FileTranscriber: Transcriber {
    let file: URL
    /// Seconds between words.
    var interval: TimeInterval = 0.1
    private var words: [String] = []
    private var given = 0
    private var timer: Timer?
    private var heard: (@MainActor (String, Bool) -> Void)?
    private var level: (@MainActor (Float) -> Void)?
    private var ended: (@MainActor (String?) -> Void)?

    init(file: URL) {
        self.file = file
    }

    var availability: Availability {
        FileManager.default.fileExists(atPath: file.path) ? .ready : .unsupported
    }

    func start(heard: @escaping @MainActor (String, Bool) -> Void,
               level: @escaping @MainActor (Float) -> Void,
               ended: @escaping @MainActor (String?) -> Void) {
        self.heard = heard
        self.level = level
        self.ended = ended
        guard let text = try? String(contentsOf: file, encoding: .utf8) else {
            finish("The speech file \(file.lastPathComponent) could not be read")
            return
        }
        words = text.split(whereSeparator: \.isWhitespace).map(String.init)
        given = 0
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.tick() } }
        }
    }

    private func tick() {
        guard ended != nil else { return }
        if given < words.count {
            given += 1
            level?(given % 2 == 0 ? 0.3 : 0.8)
            heard?(words[..<given].joined(separator: " "), false)
        } else {
            level?(0)
            heard?(words.joined(separator: " "), true)
            finish(nil)
        }
    }

    func stop() {
        guard ended != nil else { return }
        heard?(words[..<given].joined(separator: " "), true)
        finish(nil)
    }

    private func finish(_ reason: String?) {
        timer?.invalidate()
        timer = nil
        guard let ended else { return }
        self.ended = nil
        heard = nil
        level = nil
        ended(reason)
    }
}
