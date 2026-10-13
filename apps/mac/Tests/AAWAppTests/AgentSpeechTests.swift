import AVFoundation
import XCTest
@testable import AAWApp

/// Speech into the field: what firms up between two hypotheses, the
/// stand-in that reads a file's words, the microphone's level, and the
/// composer's faint tail.
@MainActor
final class AgentSpeechTests: XCTestCase {
    private var scratch: URL!

    override func setUp() async throws {
        scratch = FileManager.default.temporaryDirectory.appendingPathComponent("aaw speech \(UUID().uuidString)")
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
    }

    override func tearDown() async throws {
        try FileManager.default.removeItem(at: scratch)
    }

    /// Spins the main run loop until the condition holds, or fails.
    private func wait(_ what: String, timeout: Double = 5, until done: () -> Bool) {
        let end = Date().addingTimeInterval(timeout)
        while !done() {
            if Date() > end { return XCTFail("Waited \(timeout) s for \(what)") }
            RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        }
    }

    /// A word firms up once the next hypothesis keeps it and a word
    /// follows it; the last word may still grow.
    func testWhatFirmsUpBetweenHypotheses() {
        XCTAssertEqual(Transcribers.firmLength(previous: "", current: "move"), 0)
        XCTAssertEqual(Transcribers.firmLength(previous: "move", current: "move the"), "move ".utf16.count)
        XCTAssertEqual(Transcribers.firmLength(previous: "move the", current: "move the hats"), "move the ".utf16.count)
        XCTAssertEqual(Transcribers.firmLength(previous: "move the", current: "moved a hat"), 0, "nothing kept")
        XCTAssertEqual(Transcribers.firmLength(previous: "move the hat", current: "move the hats off"), "move the ".utf16.count,
                       "a word that changed is not firm, nor is what follows it")
        XCTAssertEqual(Transcribers.firmLength(previous: "move the hats", current: "move the hats"), "move the ".utf16.count,
                       "the last word may still grow")
    }

    /// The stand-in yields the file's words one at a time with the whole
    /// so far, then the whole as final, and ends; a stop mid-way ends with
    /// what was given.
    func testTheStandInReadsAFile() throws {
        let file = scratch.appendingPathComponent("said.txt")
        try "move the hats\noff the beat\n".write(to: file, atomically: true, encoding: .utf8)
        let transcriber = FileTranscriber(file: file)
        transcriber.interval = 0.01
        XCTAssertEqual(transcriber.availability, .ready)
        var heard: [(String, Bool)] = []
        var levels: [Float] = []
        var ended: [String?] = []
        transcriber.start(heard: { heard.append(($0, $1)) }, level: { levels.append($0) }, ended: { ended.append($0) })
        wait("the words") { !ended.isEmpty }
        XCTAssertEqual(heard.map(\.0), ["move", "move the", "move the hats", "move the hats off", "move the hats off the",
                                        "move the hats off the beat", "move the hats off the beat"])
        XCTAssertEqual(heard.map(\.1), [false, false, false, false, false, false, true])
        XCTAssertEqual(levels.count, 7)
        XCTAssertEqual(levels.last, 0)
        XCTAssertEqual(ended, [nil])
        transcriber.stop()
        XCTAssertEqual(ended.count, 1, "ended once")

        let again = FileTranscriber(file: file)
        again.interval = 0.01
        var heardAgain: [(String, Bool)] = []
        var endedAgain = 0
        again.start(heard: { heardAgain.append(($0, $1)) }, level: { _ in }, ended: { _ in endedAgain += 1 })
        wait("two words") { heardAgain.count >= 2 }
        again.stop()
        XCTAssertEqual(endedAgain, 1)
        XCTAssertEqual(heardAgain.last?.1, true)
        XCTAssertTrue(["move", "move the", "move the hats"].contains(heardAgain.last?.0 ?? ""))
        RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        XCTAssertEqual(endedAgain, 1, "nothing more after a stop")

        let missing = FileTranscriber(file: scratch.appendingPathComponent("none.txt"))
        XCTAssertEqual(missing.availability, .unsupported)
        var reason: String??
        missing.start(heard: { _, _ in XCTFail("nothing heard") }, level: { _ in }, ended: { reason = .some($0) })
        XCTAssertEqual(reason, .some("The speech file none.txt could not be read"))
    }

    /// The environment names the stand-in, as it names the agent.
    func testTheStandInIsChosenByTheEnvironment() {
        let file = scratch.appendingPathComponent("said.txt")
        let chosen = Transcribers.make(environment: [Transcribers.override: file.path])
        XCTAssertEqual((chosen as? FileTranscriber)?.file, file)
        XCTAssertTrue(Transcribers.make(environment: [:]) is SpeechTranscriber)
    }

    /// The level: silence 0, full scale 1, and −25 dB half way.
    func testTheMicrophonesLevel() throws {
        let format = try XCTUnwrap(AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1))
        func buffer(_ amplitude: Float) -> AVAudioPCMBuffer {
            let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 480)!
            buffer.frameLength = 480
            for i in 0..<480 { buffer.floatChannelData![0][i] = amplitude }
            return buffer
        }
        XCTAssertEqual(SpeechTranscriber.level(of: buffer(0)), 0)
        XCTAssertEqual(SpeechTranscriber.level(of: buffer(1)), 1)
        XCTAssertEqual(SpeechTranscriber.level(of: buffer(pow(10, -25 / 20))), 0.5, accuracy: 0.01)
    }

    /// What cannot listen says why.
    func testTheReasons() {
        XCTAssertEqual(SpeechTranscriber.reason(.ready), "")
        XCTAssertEqual(SpeechTranscriber.reason(.noOnDeviceRecognition(locale: "fr_FR")),
                       "This Mac cannot yet recognize fr_FR speech on the device, and nothing is sent off it: turn on Dictation in System Settings › Keyboard, which downloads the language")
        XCTAssertEqual(SpeechTranscriber.reason(.refused("off")), "off")
        XCTAssertEqual(SpeechTranscriber.reason(.unsupported), "This Mac has no speech recognizer")
    }

    /// The uncertain tail is drawn in the faint color and the rest in the
    /// text's; a range past the text is clipped.
    func testTheComposersFaintTail() {
        let (_, text) = ComposerTextView.make()
        text.frame = NSRect(x: 0, y: 0, width: 240, height: 40)
        text.set("move the hats")
        XCTAssertEqual(text.color(at: 0), Theme.text)
        text.faintRange = NSRange(location: 9, length: 4)
        XCTAssertEqual(text.color(at: 8), Theme.text)
        XCTAssertEqual(text.color(at: 9), Theme.faintText)
        XCTAssertEqual(text.color(at: 12), Theme.faintText)
        text.faintRange = NSRange(location: 9, length: 40)
        XCTAssertEqual(text.color(at: 12), Theme.faintText)
        text.faintRange = nil
        XCTAssertEqual(text.color(at: 12), Theme.text)
        // A chip keeps its white with a tail after it.
        text.chip = "roar"
        text.set("/roar move the hats")
        text.faintRange = NSRange(location: 15, length: 4)
        XCTAssertEqual(text.color(at: 1), ComposerTextView.chipText)
        XCTAssertEqual(text.color(at: 6), Theme.text)
        XCTAssertEqual(text.color(at: 16), Theme.faintText)
    }
}
