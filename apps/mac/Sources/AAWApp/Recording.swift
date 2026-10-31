import AAWCore
import Foundation
import Observation

/// What the transport bar's Record does with: the count-in, in bars, kept
/// in the app's defaults across launches and chosen under Transport ›
/// Count-In. One bar unless chosen, as `daw record start` counts.
@MainActor
@Observable
public final class RecordPreferences {
    public static let shared = RecordPreferences(defaults: .standard)

    /// The count-ins the menu offers, in bars.
    public nonisolated static let countIns: [UInt32] = [0, 1, 2, 4]
    public nonisolated static let usualCountIn: UInt32 = 1
    static let countInKey = "record.countIn"

    @ObservationIgnored private let defaults: UserDefaults

    public var countIn: UInt32 {
        didSet { if countIn != oldValue { defaults.set(Int(countIn), forKey: Self.countInKey) } }
    }

    public init(defaults: UserDefaults) {
        self.defaults = defaults
        let kept = defaults.object(forKey: Self.countInKey) as? Int
        countIn = kept.map(UInt32.init).flatMap { Self.countIns.contains($0) ? $0 : nil } ?? Self.usualCountIn
    }

    /// A count-in as the menu names it: None, 1 Bar, 2 Bars.
    public nonisolated static func name(_ bars: UInt32) -> String {
        switch bars {
        case 0: "None"
        case 1: "1 Bar"
        default: "\(bars) Bars"
        }
    }
}

/// The words of the transport bar's Record and a header's arm.
public enum RecordWords {
    /// The Record button's help, by what it would do.
    public static func record(recording: Bool, armed: Int, countIn: UInt32) -> String {
        if recording {
            return "Stop recording (R). The takes land on their tracks; the song plays on."
        }
        if armed == 0 {
            return "Record (R). Arm a track first: the dot before its M, an audio track with an input chosen in its header's menu, a MIDI track with an instrument, which records the notes played from the typing keyboard."
        }
        let tracks = armed == 1 ? "the armed track" : "the \(armed) armed tracks"
        let lead = countIn == 0 ? "at once" : "after a count-in of \(RecordPreferences.name(countIn).lowercased())"
        return "Record \(tracks) from the start position \(lead) (R)."
    }

    /// The arm's help, by the track's input.
    public static func arm(armed: Bool, input: String?) -> String {
        switch (armed, input) {
        case (_, nil): "No input. Choose one in the header's menu, or click to choose."
        case (true, let input?): "Armed to record from \(input). Click to disarm."
        case (false, let input?): "Arm to record from \(input)."
        }
    }
}
