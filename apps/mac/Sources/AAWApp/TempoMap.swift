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

    /// The beats from `from` to `to` cut where the tempo steps: a piece a
    /// step, each with its tempo; one piece where it keeps one.
    public func pieces(from: Double, to: Double) -> [(from: Double, to: Double, bpm: Double)] {
        guard to > from else { return [] }
        var out: [(from: Double, to: Double, bpm: Double)] = []
        var at = from
        for (i, s) in steps.enumerated() {
            let end = i + 1 < steps.count ? steps[i + 1].beat : .infinity
            guard end > at else { continue }
            out.append((at, min(end, to), s.bpm))
            at = end
            if at >= to { break }
        }
        return out
    }
}

/// Where an audio clip's file plays along the song's beats, from a beat
/// where so many seconds of it play: a clip that follows the song's tempo
/// (one with `source_bpm`) at its seconds a beat everywhere, and one that
/// plays its file at its own speed through the song's tempo steps, its
/// seconds a beat those of where it is.
public struct ClipTime {
    public var map: TempoMap
    /// A beat, and the second of the file played there.
    public var beat: Double
    public var seconds: Double
    /// Seconds of the file a beat at the clip's start, as the host gives it.
    public var secondsPerBeat: Double
    /// Whether the clip plays at its own speed through the tempo's steps.
    public var stepped: Bool
    /// Seconds of the file a second of the song: its repitch.
    public var speed: Double

    /// `clipBeat` is the clip's own beat, where `secondsPerBeat` is read;
    /// `follows` whether the clip has a tempo of its own that the song's
    /// stretches it to.
    public init(map: TempoMap, beat: Double, seconds: Double, secondsPerBeat: Double, clipBeat: Double, follows: Bool) {
        self.map = map
        self.beat = beat
        self.seconds = seconds
        self.secondsPerBeat = secondsPerBeat
        stepped = !follows && map.constantTempo == nil
        speed = secondsPerBeat * map.tempo(atBeat: clipBeat) / 60
    }

    /// The second of the file played at a beat.
    public func seconds(atBeat b: Double) -> Double {
        stepped ? seconds + (map.seconds(b) - map.seconds(beat)) * speed : seconds + (b - beat) * secondsPerBeat
    }

    /// The beat a second of the file plays on.
    public func beat(atSeconds s: Double) -> Double {
        guard secondsPerBeat > 0, speed > 0 else { return beat }
        return stepped ? map.beat(map.seconds(beat) + (s - seconds) / speed) : beat + (s - seconds) / secondsPerBeat
    }

    /// The beats from `from` to `to` as pieces each at one rate: a piece a
    /// tempo step for a clip at its own speed, else one; each with the
    /// second of the file at its start and the file's seconds a beat.
    public func pieces(from: Double, to: Double) -> [(from: Double, to: Double, seconds: Double, secondsPerBeat: Double)] {
        guard stepped else { return to > from ? [(from, to, seconds(atBeat: from), secondsPerBeat)] : [] }
        return map.pieces(from: from, to: to).map { ($0.from, $0.to, seconds(atBeat: $0.from), 60 / $0.bpm * speed) }
    }
}

extension Arrangement {
    /// The song's tempo map, from the host's steps.
    public var tempoMap: TempoMap {
        TempoMap(tempoSteps.map { TempoMap.Step(beat: $0.beat, seconds: $0.seconds, bpm: $0.bpm) })
    }
}
