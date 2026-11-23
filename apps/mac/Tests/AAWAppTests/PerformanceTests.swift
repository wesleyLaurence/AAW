import AAWCore
import AVFoundation
import XCTest
@testable import AAWApp

/// What makes the person wait: a large song opened as the app opens one,
/// its newest conversation a long one, and the browser's lists read, while
/// the main thread is watched. No piece of work on it may take longer than
/// the budget, a tenth of a second, past which the window stops answering.
@MainActor
final class PerformanceTests: XCTestCase {
    private var scratch: URL!

    /// Main-thread work longer than this fails the test, in milliseconds.
    static let budget = 100.0

    private nonisolated static let folders: URL = {
        let root = URL(fileURLWithPath: "/tmp/aaw-perf-\(UUID().uuidString.prefix(8))")
        setenv("AAW_HOST_DIR", root.appendingPathComponent("hosts").path, 1)
        setenv("AAW_DATA_DIR", root.appendingPathComponent("data").path, 1)
        setenv("AAW_WORKSPACE", root.appendingPathComponent("workspace").path, 1)
        return root
    }()

    override func setUp() async throws {
        _ = Self.folders
        scratch = SongModel.normal(FileManager.default.temporaryDirectory.appendingPathComponent("aaw perf \(UUID().uuidString)"))
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
        // No test runs the person's Claude Code, or reads their record of it.
        setenv(ClaudeCode.override, "/usr/bin/true", 1)
        setenv("CLAUDE_CONFIG_DIR", scratch.appendingPathComponent("claude").path, 1)
    }

    override func tearDown() async throws {
        unsetenv(ClaudeCode.override)
        unsetenv("CLAUDE_CONFIG_DIR")
        try FileManager.default.removeItem(at: scratch)
    }

    override class func tearDown() {
        try? FileManager.default.removeItem(at: folders)
    }

    func testALargeSongOpensWithoutHoldingTheMainThread() throws {
        let song = try largeSong()
        try longConversation(in: song.deletingLastPathComponent())
        Stalls.shared.start()
        // The setup's own work ends before the watch counts.
        spin(0.3)
        let mark = Stalls.shared.elapsed

        final class Opened { var result: Result<OpenedSong, Error>? }
        let opened = Opened()
        SongModel.open(url: song) { opened.result = $0 }
        wait("the song open", timeout: 60) { opened.result != nil }
        let model = SongModel(opened: try XCTUnwrap(opened.result).get())
        defer { model.close() }
        XCTAssertEqual(model.arrangement.tracks.count, 30)
        wait("its waveforms", timeout: 60) { model.waveformDelay.open != nil }
        wait("the conversation's history") { model.agent.entries.count > 1000 }
        model.browser.refreshPatches()
        model.browser.refreshRacks()
        model.browser.refreshFavorites()
        model.refreshPlugins()
        wait("the patches") { !model.browser.patches.isEmpty }
        spin(0.3)

        let over = Stalls.shared.found(after: mark).filter { $0.ms > Self.budget }
        XCTAssertEqual(over.map(\.ms), [], over.map { stall in
            "\(stall.ms) ms: " + (stall.stacks.first ?? []).prefix(16).joined(separator: " < ")
        }.joined(separator: "\n"))
    }

    // MARK: What is opened

    /// A song of 30 tracks, five minutes long: 20 audio tracks of eight
    /// clips each from four files, and 10 Synth tracks of eight note clips
    /// of 128 notes each.
    private func largeSong() throws -> URL {
        let folder = scratch.appendingPathComponent("Large Song")
        let samples = folder.appendingPathComponent("samples")
        try FileManager.default.createDirectory(at: samples, withIntermediateDirectories: true)
        var yaml = "schema_version: 2\nsession: {tempo: 100}\nsamples:\n"
        for f in 0..<4 {
            try tone(samples.appendingPathComponent("loop\(f).wav"), hz: 110 * Double(f + 1), seconds: 15)
            yaml += "  loop\(f): {path: samples/loop\(f).wav}\n"
        }
        yaml += "tracks:\n"
        for t in 0..<20 {
            yaml += "- id: audio\(t)\n  audio:\n"
            for c in 0..<8 {
                yaml += "  - {sample: loop\((t + c) % 4), at: \(c * 64), source_end_seconds: 14}\n"
            }
        }
        for t in 0..<10 {
            yaml += "- id: synth\(t)\n  type: midi\n  instrument: {synth: {oscillators: {a: {}}}}\n  clips:\n"
            for c in 0..<8 {
                let notes = (0..<128).map { n in
                    "{id: n\(n + 1), pitch: \(48 + (n * 7 + t) % 24), at: \(Double(n) / 2), duration: 0.5}"
                }
                yaml += "  - {id: s\(t)c\(c), at: \(c * 64), length_beats: 64, notes: [\(notes.joined(separator: ", "))]}\n"
            }
        }
        let song = folder.appendingPathComponent("song.yaml")
        try yaml.write(to: song, atomically: true, encoding: .utf8)
        return song
    }

    /// A stereo sine.
    private func tone(_ url: URL, hz: Double, seconds: Double) throws {
        let rate = 44100.0
        let format = try XCTUnwrap(AVAudioFormat(standardFormatWithSampleRate: rate, channels: 2))
        let frames = AVAudioFrameCount(rate * seconds)
        let buffer = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames))
        buffer.frameLength = frames
        for channel in 0..<2 {
            let data = buffer.floatChannelData![channel]
            for i in 0..<Int(frames) { data[i] = Float(0.3 * sin(2 * .pi * hz * Double(i) / rate)) }
        }
        let file = try AVAudioFile(forWriting: url, settings: format.settings)
        try file.write(from: buffer)
    }

    /// Claude Code's record of a conversation in the project's folder of
    /// 2,000 messages, about eight megabytes, as a long session leaves.
    private func longConversation(in project: URL) throws {
        let folder = ClaudeTranscripts.folder(for: project)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let words = String(repeating: "The kick sits under the bass at bar 41. ", count: 100)
        var lines: [String] = []
        for n in 0..<1000 {
            let person: [String: Any] = ["type": "user", "message": ["role": "user", "content": "Message \(n): \(words)"]]
            let agent: [String: Any] = ["type": "assistant", "message": ["role": "assistant", "content": [["type": "text", "text": "Answer \(n): \(words)"]]]]
            for object in [person, agent] {
                lines.append(String(decoding: try JSONSerialization.data(withJSONObject: object), as: UTF8.self))
            }
        }
        try (lines.joined(separator: "\n") + "\n").write(to: folder.appendingPathComponent("\(UUID().uuidString.lowercased()).jsonl"), atomically: true, encoding: .utf8)
    }

    // MARK: Waiting

    private func spin(_ seconds: Double) {
        RunLoop.main.run(until: Date().addingTimeInterval(seconds))
    }

    private func wait(_ what: String, timeout: Double = 15, until done: () -> Bool) {
        let end = Date().addingTimeInterval(timeout)
        while !done() {
            if Date() > end { return XCTFail("Waited \(timeout) s for \(what)") }
            RunLoop.main.run(until: Date().addingTimeInterval(0.02))
        }
    }
}
