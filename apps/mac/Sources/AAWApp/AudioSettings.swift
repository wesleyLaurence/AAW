import AAWCore
import Foundation
import Observation

/// The devices the app plays and records through and its output buffer,
/// kept in the app's defaults across launches, given to every song's host
/// as it opens and sent to each open one when they change. A device is
/// kept by its name; None is the system's default, which the host follows
/// as the Mac's changes.
@MainActor
@Observable
public final class AudioPreferences {
    public static let shared = AudioPreferences(defaults: .standard)

    /// The buffers the menu offers, in frames.
    public static let buffers: [UInt32] = [64, 128, 256, 512, 1024, 2048]
    public static let usualBuffer: UInt32 = 128
    static let outputKey = "audio.output"
    static let inputKey = "audio.input"
    static let bufferKey = "audio.buffer"

    @ObservationIgnored private let defaults: UserDefaults
    /// Called with the settings when one changes: the app gives them to
    /// every open song.
    @ObservationIgnored public var onChange: ((AudioSettings) -> Void)?

    public var output: String? {
        didSet { if output != oldValue { remember() } }
    }

    public var input: String? {
        didSet { if input != oldValue { remember() } }
    }

    public var buffer: UInt32 {
        didSet { if buffer != oldValue { remember() } }
    }

    /// The Mac's devices as last listed.
    public private(set) var devices: [AudioDevice] = []

    public init(defaults: UserDefaults) {
        self.defaults = defaults
        output = defaults.string(forKey: Self.outputKey).flatMap { $0.isEmpty ? nil : $0 }
        input = defaults.string(forKey: Self.inputKey).flatMap { $0.isEmpty ? nil : $0 }
        let kept = defaults.object(forKey: Self.bufferKey) as? Int
        buffer = kept.map(UInt32.init).flatMap { Self.buffers.contains($0) ? $0 : nil } ?? Self.usualBuffer
    }

    /// What the host is given.
    public var settings: AudioSettings {
        AudioSettings(output: output, input: input, buffer: buffer)
    }

    private func remember() {
        defaults.set(output ?? "", forKey: Self.outputKey)
        defaults.set(input ?? "", forKey: Self.inputKey)
        defaults.set(Int(buffer), forKey: Self.bufferKey)
        onChange?(settings)
    }

    /// Lists the Mac's devices again, for the menus.
    public func refreshDevices() {
        devices = (try? audioDevices()) ?? []
    }

    public var outputs: [AudioDevice] { devices.filter { $0.outputChannels > 0 } }
    public var inputs: [AudioDevice] { devices.filter { $0.inputChannels > 0 } }

    /// The names a menu offers for a chosen device: the system's default
    /// first, then the devices, and the chosen one kept in the list while
    /// it is not connected, so the choice is not lost by a glance.
    public static func choices(_ devices: [AudioDevice], chosen: String?) -> [String?] {
        var names: [String?] = [nil] + devices.map { $0.name }
        if let chosen, !names.contains(chosen) { names.append(chosen) }
        return names
    }
}

/// The words Settings › Audio puts under its menus.
public enum AudioWords {
    /// A rate as it is said: `44.1 kHz`, `48 kHz`.
    public static func kHz(_ rate: UInt32) -> String {
        let k = Double(rate) / 1000
        return k == k.rounded() ? "\(Int(k)) kHz" : String(format: "%.1f kHz", k)
    }

    /// A buffer's length at a rate, in milliseconds.
    public static func latency(buffer: UInt32, rate: UInt32) -> String {
        String(format: "%.1f ms", Double(buffer) / Double(max(rate, 1)) * 1000)
    }

    /// What plays through what, and at which rate.
    public static func status(_ status: AudioStatus?, songRate: UInt32) -> String {
        guard let status, let device = status.device, let rate = status.deviceSampleRate else {
            return "Nothing has played yet. The song is at \(kHz(songRate))."
        }
        var line = ""
        if let missing = status.fallback {
            line = "\(missing) is not connected; playing through \(device) at \(kHz(rate))."
        } else {
            line = "Playing through \(device) at \(kHz(rate))"
            line += status.resampled ? "; the song at \(kHz(status.songSampleRate)) is resampled to it." : ", the song's rate."
        }
        if let ms = status.outputLatencyMs {
            line += String(format: " Output latency %.1f ms.", ms)
        }
        return line
    }

    /// The name of a chosen device as the menu shows it.
    public static func name(_ device: String?) -> String {
        device ?? "System Default"
    }
}
