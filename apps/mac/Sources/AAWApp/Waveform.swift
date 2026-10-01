import AAWCore
import CoreGraphics
import Foundation

/// What a track's clips play, as the host worked it out: the least and
/// greatest sample of each stretch of frames, at several resolutions. A clip
/// draws the part of it under its span.
public final class Waveform {
    public struct Level {
        public var framesPerBucket: Double
        /// Two signed bytes a bucket: its least and its greatest sample, with
        /// full scale at 127.
        public var data: Data

        public var buckets: Int { data.count / 2 }
    }

    /// Equal for tracks whose audio is the same.
    public let identity: UInt64
    public let framesPerBeat: Double
    /// From the finest to the coarsest.
    public let levels: [Level]

    public init(identity: UInt64, framesPerBeat: Double, levels: [Level]) {
        self.identity = identity
        self.framesPerBeat = framesPerBeat
        self.levels = levels
    }

    convenience init(_ peaks: TrackPeaks) {
        self.init(identity: peaks.identity, framesPerBeat: peaks.framesPerBeat, levels: peaks.levels.map {
            Level(framesPerBucket: Double($0.framesPerBucket), data: $0.data)
        })
    }

    /// The coarsest level with a bucket for each column `frames` wide, or the
    /// finest when columns are narrower than its buckets.
    public func level(forColumn frames: Double) -> Level? {
        levels.last { $0.framesPerBucket <= frames } ?? levels.first
    }

    /// The least and greatest sample from one frame to another among a
    /// level's buckets, from -1 to 1: silence before the song, and after what
    /// the peaks cover.
    private static func range(_ samples: UnsafeBufferPointer<Int8>, framesPerBucket: Double, from: Double,
                              to: Double) -> (lo: Double, hi: Double) {
        let first = max(0, Int((from / framesPerBucket).rounded(.down)))
        let last = min(samples.count / 2, max(first + 1, Int((to / framesPerBucket).rounded(.up))))
        guard first < last else { return (0, 0) }
        var lo = Int8.max
        var hi = Int8.min
        for bucket in first..<last {
            lo = min(lo, samples[2 * bucket])
            hi = max(hi, samples[2 * bucket + 1])
        }
        return (Double(lo) / 127, Double(hi) / 127)
    }

    public static func range(_ level: Level, from: Double, to: Double) -> (lo: Double, hi: Double) {
        level.data.withUnsafeBytes { range($0.bindMemory(to: Int8.self), framesPerBucket: level.framesPerBucket, from: from, to: to) }
    }

    /// The waveform from `beat` on as columns `step` points wide filling
    /// `rect` from its left edge: only the columns inside `clip`, each from
    /// its least sample to its greatest and at least `step` tall, so that
    /// silence is a line.
    public func columns(in rect: CGRect, clippedTo clip: CGRect, fromBeat beat: Double, pixelsPerBeat: CGFloat,
                        step: CGFloat) -> [CGRect] {
        guard pixelsPerBeat > 0, step > 0, rect.height > 0 else { return [] }
        let framesPerColumn = Double(step / pixelsPerBeat) * framesPerBeat
        guard let level = level(forColumn: framesPerColumn) else { return [] }
        let left = max(rect.minX, clip.minX)
        let right = min(rect.maxX, clip.maxX)
        guard right > left else { return [] }
        // Columns are counted from the clip's start, so they hold the same
        // audio wherever the view cuts the clip.
        let first = Int(((left - rect.minX) / step).rounded(.down))
        let last = Int(((right - rect.minX) / step).rounded(.up))
        let start = beat * framesPerBeat
        let half = rect.height / 2
        var columns: [CGRect] = []
        columns.reserveCapacity(last - first)
        level.data.withUnsafeBytes { bytes in
            let samples = bytes.bindMemory(to: Int8.self)
            for column in first..<last {
                let from = start + Double(column) * framesPerColumn
                let (lo, hi) = Self.range(samples, framesPerBucket: level.framesPerBucket, from: from, to: from + framesPerColumn)
                let height = max(step, CGFloat(hi - lo) * half)
                columns.append(CGRect(x: rect.minX + CGFloat(column) * step, y: rect.midY - CGFloat(hi) * half,
                                      width: step, height: height))
            }
        }
        return columns
    }
}

/// The waveforms the host has sent: which audio each track has at a revision,
/// and the peaks of that audio.
struct WaveformStore {
    /// The revision the identities below are of.
    private(set) var revision: UInt64?
    /// Each track's audio, by the track's key.
    private(set) var identity: [UInt64: UInt64] = [:]
    private(set) var peaks: [UInt64: Waveform] = [:]

    /// Takes an update, and forgets peaks no track has any more, which the
    /// host sends again if a track comes back to them.
    mutating func apply(_ update: Waveforms) {
        revision = update.revision
        identity = Dictionary(update.tracks.map { ($0.track, $0.identity) }, uniquingKeysWith: { $1 })
        for peaks in update.peaks { self.peaks[peaks.identity] = Waveform(peaks) }
        let used = Set(identity.values)
        peaks = peaks.filter { used.contains($0.key) }
    }

    /// Whether every track's peaks have arrived.
    var complete: Bool {
        identity.values.allSatisfy { peaks[$0] != nil }
    }

    /// A track's waveform at `revision`, once it has arrived.
    func waveform(of track: UInt64, at revision: UInt64) -> Waveform? {
        guard self.revision == revision, let identity = identity[track] else { return nil }
        return peaks[identity]
    }
}
