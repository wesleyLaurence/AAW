import AAWCore
import XCTest
@testable import AAWApp

@MainActor
final class AudioSettingsTests: XCTestCase {
    private func fresh() -> UserDefaults {
        let suite = "aaw.tests.audio.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defaults.removePersistentDomain(forName: suite)
        return defaults
    }

    /// The choices are kept in the defaults and read back by the next
    /// launch; the usual buffer and the system's default devices otherwise.
    func testTheChoicesAreKeptAcrossLaunches() {
        let defaults = fresh()
        let first = AudioPreferences(defaults: defaults)
        XCTAssertNil(first.output)
        XCTAssertNil(first.input)
        XCTAssertEqual(first.buffer, 128)
        XCTAssertEqual(first.settings, AudioSettings(output: nil, input: nil, buffer: 128))
        var told: [AudioSettings] = []
        first.onChange = { told.append($0) }
        first.output = "AirPods Pro"
        first.buffer = 512
        first.input = "Scarlett 2i2"
        XCTAssertEqual(told.count, 3)
        XCTAssertEqual(told.last, AudioSettings(output: "AirPods Pro", input: "Scarlett 2i2", buffer: 512))
        let next = AudioPreferences(defaults: defaults)
        XCTAssertEqual(next.settings, AudioSettings(output: "AirPods Pro", input: "Scarlett 2i2", buffer: 512))
        next.output = nil
        XCTAssertNil(AudioPreferences(defaults: defaults).output)
        // A buffer the menu does not offer falls back to the usual one.
        defaults.set(100, forKey: AudioPreferences.bufferKey)
        XCTAssertEqual(AudioPreferences(defaults: defaults).buffer, 128)
    }

    /// The menu lists the system's default first, the devices, and the
    /// chosen one even while it is not connected.
    func testTheMenuKeepsAChosenDeviceThatIsAway() {
        let speakers = AudioDevice(name: "MacBook Air Speakers", inputChannels: 0, outputChannels: 2, sampleRates: [44100, 48000],
                                   currentRate: 48000, minBuffer: 15, maxBuffer: 4096, defaultInput: false, defaultOutput: true)
        XCTAssertEqual(AudioPreferences.choices([speakers], chosen: nil), [nil, "MacBook Air Speakers"])
        XCTAssertEqual(AudioPreferences.choices([speakers], chosen: "AirPods Pro"), [nil, "MacBook Air Speakers", "AirPods Pro"])
        XCTAssertEqual(AudioPreferences.choices([speakers], chosen: "MacBook Air Speakers"), [nil, "MacBook Air Speakers"])
        XCTAssertEqual(AudioWords.name(nil), "System Default")
    }

    /// What the tab says under the menus.
    func testTheWordsUnderTheMenus() {
        XCTAssertEqual(AudioWords.kHz(44100), "44.1 kHz")
        XCTAssertEqual(AudioWords.kHz(48000), "48 kHz")
        XCTAssertEqual(AudioWords.latency(buffer: 128, rate: 48000), "2.7 ms")
        XCTAssertEqual(AudioWords.latency(buffer: 256, rate: 44100), "5.8 ms")
        XCTAssertEqual(AudioWords.status(nil, songRate: 44100), "Nothing has played yet. The song is at 44.1 kHz.")
        let resampled = AudioStatus(device: "AirPods Pro", songSampleRate: 44100, deviceSampleRate: 48000, resampled: true,
                                    bufferFrames: 128, outputLatencyMs: 9.5, fallback: nil)
        XCTAssertEqual(AudioWords.status(resampled, songRate: 44100),
                       "Playing through AirPods Pro at 48 kHz; the song at 44.1 kHz is resampled to it. Output latency 9.5 ms.")
        let own = AudioStatus(device: "MacBook Air Speakers", songSampleRate: 48000, deviceSampleRate: 48000, resampled: false,
                              bufferFrames: 128, outputLatencyMs: nil, fallback: nil)
        XCTAssertEqual(AudioWords.status(own, songRate: 48000), "Playing through MacBook Air Speakers at 48 kHz, the song's rate.")
        let away = AudioStatus(device: "MacBook Air Speakers", songSampleRate: 48000, deviceSampleRate: 48000, resampled: false,
                               bufferFrames: 128, outputLatencyMs: 2.7, fallback: "Scarlett 2i2")
        XCTAssertEqual(AudioWords.status(away, songRate: 48000),
                       "Scarlett 2i2 is not connected; playing through MacBook Air Speakers at 48 kHz. Output latency 2.7 ms.")
    }

    func testTheLaunchFlagOpensSettings() {
        XCTAssertTrue(Launch(arguments: ["--settings"]).settings)
        XCTAssertFalse(Launch(arguments: ["song.yaml"]).settings)
    }
}
