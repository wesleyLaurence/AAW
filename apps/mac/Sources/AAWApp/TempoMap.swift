import AAWCore
import Foundation

/// The song's tempo through time, as the host gives it: a step from each
/// beat on, the first at beat 0, one for a song that keeps one tempo. The
/// timeline is drawn in beats, so bars keep their width; the map turns a
/// beat into its time and back, for the ruler's time strip and the tempo at
/// the playhead.
public struct TempoMap: Equatable {
    public struct Step: Equatable {
        public var beat: Double
        public var seconds: Double
        public var bpm: Double

        public init(beat: Double, seconds: Double, bpm: Double) {
            self.beat = beat
            self.seconds = seconds
            self.bpm = bpm
        }
    }

    public private(set) var steps: [Step]

    public init(_ steps: [Step]) {
        self.steps = steps.isEmpty ? [Step(beat: 0, seconds: 0, bpm: 120)] : steps
    }

    public static func constant(_ bpm: Double) -> TempoMap {
        TempoMap([Step(beat: 0, seconds: 0, bpm: bpm)])
    }

    /// The tempo of a song that keeps one; nil where it changes.
    public var constantTempo: Double? { steps.count == 1 ? steps[0].bpm : nil }

    /// The slowest tempo: where a second takes the fewest points.
    public var slowest: Double { steps.map(\.bpm).min() ?? 120 }

    private func step(atBeat beat: Double) -> Step {
        steps.last { $0.beat <= beat } ?? steps[0]
    }

    private func step(atSeconds seconds: Double) -> Step {
        steps.last { $0.seconds <= seconds } ?? steps[0]
    }

    /// The tempo playing at a beat.
    public func tempo(atBeat beat: Double) -> Double { step(atBeat: beat).bpm }

    /// The second a beat falls on: `beat * 60 / tempo` with one tempo.
    public func seconds(_ beat: Double) -> Double {
        let s = step(atBeat: beat)
        return s.seconds + (beat - s.beat) * 60 / s.bpm
    }

    /// The beat a second of the song falls on: `seconds * tempo / 60` with one tempo.
    public func beat(_ seconds: Double) -> Double {
        let s = step(atSeconds: seconds)
        return s.beat + (seconds - s.seconds) * s.bpm / 60
    }
}

extension Arrangement {
    /// The song's tempo map, from the host's steps.
    public var tempoMap: TempoMap {
        TempoMap(tempoSteps.map { TempoMap.Step(beat: $0.beat, seconds: $0.seconds, bpm: $0.bpm) })
    }
}
