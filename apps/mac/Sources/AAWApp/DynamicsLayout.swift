import AAWCore
import Foundation

/// What the compressor's, the limiter's and the clipper's panels draw from
/// the hops the engine leaves them: where a level and a reduction sit in a
/// box, the hops folded into the columns of a scrolling display, where the
/// threshold line is held and what a drag of it sets, and the held most.
/// Pure geometry, tested in `Tests`; the views draw from it.
enum DynamicsLayout {
    /// The display's range: full scale at the top and −60 dB, a compressor's
    /// lowest threshold, at the bottom. A reduction hangs from the top on the
    /// same scale, so the output sits under the input by what was taken off.
    static let topDb = 0.0
    static let floorDb = -60.0
    /// The seconds the compressor's display shows across its width.
    static let seconds = 4.0
    /// How far from the threshold line a press takes hold of it, in points.
    static let grab: CGFloat = 6
    /// The level meter's range, over full scale so a limiter's input past
    /// its ceiling shows.
    static let meterTopDb = 6.0

    /// The reduction a meter reads down to: 24 dB for a compressor and a
    /// limiter, 36 for a clipper, whose drive reaches 36 dB.
    static func meterRange(kind: String) -> Double {
        kind == "clipper" ? 36 : 24
    }

    /// The hops the display shows, at most the ring's 1024.
    static func hopsShown(sampleRate: Double, hopFrames: Double = 256) -> Int {
        min(1024, max(1, Int((seconds * sampleRate / hopFrames).rounded())))
    }

    /// The height of a level in a box, clamped to it.
    static func y(db: Double, top: Double = topDb, floor: Double = floorDb, in box: CGRect) -> CGFloat {
        let t = (top - min(max(db, floor), top)) / (top - floor)
        return box.minY + CGFloat(t) * box.height
    }

    /// The level at a height in the box.
    static func db(y: CGFloat, top: Double = topDb, floor: Double = floorDb, in box: CGRect) -> Double {
        guard box.height > 0 else { return top }
        return top - Double((y - box.minY) / box.height) * (top - floor)
    }

    /// How far down a reduction reaches from the box's top, on the
    /// display's scale or a meter's `range`.
    static func depth(reductionDb: Double, range: Double = topDb - floorDb, in box: CGRect) -> CGFloat {
        CGFloat(min(max(reductionDb, 0), range) / range) * box.height
    }

    /// The threshold a drag to `y` sets: within a compressor's −60 to 0 dB,
    /// to a tenth of a dB.
    static func threshold(at y: CGFloat, in box: CGRect) -> Double {
        let db = min(max(db(y: y, in: box), floorDb), topDb)
        return (db * 10).rounded() / 10
    }

    /// Whether a press at `y` takes hold of the line at `thresholdDb`.
    static func grabs(y: CGFloat, thresholdDb: Double, in box: CGRect) -> Bool {
        abs(y - self.y(db: thresholdDb, in: box)) <= grab
    }

    /// The hops, oldest first, folded into `width` columns, each the most of
    /// the hops it covers; with fewer hops than columns a hop spans several.
    /// No hops at all leave every column `empty`.
    static func columns(_ values: [Float], width: Int, empty: Float) -> [Float] {
        guard width > 0 else { return [] }
        guard !values.isEmpty else { return Array(repeating: empty, count: width) }
        var out = [Float](repeating: empty, count: width)
        let n = values.count
        for c in 0..<width {
            // Column c covers hops [lo, hi) of the n, right-aligned.
            let lo = n * c / width
            let hi = max(n * (c + 1) / width, lo + 1)
            var most = -Float.infinity
            for i in lo..<min(hi, n) { most = max(most, values[i]) }
            out[c] = most
        }
        return out
    }

    /// A reduction as its meter writes it: to a tenth of a dB, as a fall.
    static func text(reductionDb: Double) -> String {
        let r = (max(reductionDb, 0) * 10).rounded() / 10
        return r == 0 ? "0.0" : String(format: "−%.1f", r)
    }

    /// The most a meter took off since it was reset: at a click, and when
    /// the song is played from a stop. Each reading gives the hops the
    /// device has made so far and its last hops' reductions, the newest
    /// last.
    struct Held: Equatable {
        private(set) var most = 0.0
        private var written: UInt64?
        private var playing = false

        mutating func update(written: UInt64, reductions: [Float], playing: Bool) {
            if playing, !self.playing { most = 0 }
            self.playing = playing
            // The first reading, or a device moved or made again: count only
            // what comes from here.
            guard let was = self.written, written >= was else {
                self.written = written
                return
            }
            let new = min(Int(written - was), reductions.count)
            for r in reductions.suffix(new) { most = max(most, Double(r)) }
            self.written = written
        }

        mutating func reset() {
            most = 0
        }
    }

    /// The reduction a meter's bar shows now: the last hop's while hops keep
    /// coming, falling at 24 dB a second once they stop, as when the song is
    /// stopped, so that the bar does not stand at what was last played.
    static func fallen(_ shown: Double, toward latest: Double, fresh: Bool, seconds: Double) -> Double {
        fresh ? latest : max(0, shown - 24 * seconds)
    }
}
