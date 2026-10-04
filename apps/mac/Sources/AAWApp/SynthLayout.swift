import AAWCore
import Foundation

/// Where the Synth panel's drawings put things, and what a drag on them
/// means: the filter's response and its corner, an envelope's four handles,
/// a wave or an LFO over one cycle, the keys, and how far a matrix entry
/// reaches on a control. Pure geometry, tested in `Tests`; the views draw
/// from it and send what it works out to the host.
enum SynthLayout {
    /// The range the filter curve draws: 10 Hz to 20 kHz, -36 to +36 dB,
    /// which holds the corner of a 12 dB filter at full resonance.
    static let lowHz = 10.0
    static let highHz = 20000.0
    static let lowDb = -36.0
    static let highDb = 36.0
    /// The longest an envelope's attack, decay or release may be.
    static let longestMs = 20000.0
    /// How far from a handle a press takes hold of it, in points.
    static let grab: CGFloat = 7

    /// The magnitude of the Synth's state-variable filter at `hz`, in dB:
    /// Simper's SVF with `k = 2 - 2 * resonance / 100`, one section for 12 dB
    /// an octave and two for 24.
    static func responseDb(mode: String, sections: Int, cutoffHz: Double, resonancePercent: Double, hz: Double) -> Double {
        let k = max(2 - 2 * min(max(resonancePercent / 100, 0), 1), 0.02)
        // s = jw, with w the frequency over the cutoff: D = 1 - w² + j k w.
        let w = hz / max(cutoffHz, 1e-6)
        let (re, im) = (1 - w * w, k * w)
        let d = (re * re + im * im).squareRoot()
        let numerator: Double
        switch mode {
        case "highpass": numerator = w * w
        case "bandpass": numerator = w
        case "notch": numerator = abs(1 - w * w)
        default: numerator = 1
        }
        let one = 20 * log10(max(numerator / d, 1e-9))
        return one * Double(max(sections, 1))
    }

    /// The gain at the cutoff itself, which is where the corner sits: `1/k`
    /// for a lowpass, highpass or bandpass, and nothing for a notch.
    static func cornerDb(mode: String, sections: Int, resonancePercent: Double) -> Double {
        responseDb(mode: mode, sections: sections, cutoffHz: 1000, resonancePercent: resonancePercent, hz: 1000)
    }

    /// The resonance whose corner gain is `db`, inverting `cornerDb`, 0 to 100.
    static func resonance(cornerDb db: Double, sections: Int) -> Double {
        let k = pow(10, -db / (20 * Double(max(sections, 1))))
        return min(max((2 - k) / 2 * 100, 0), 100)
    }

    /// A frequency's place across a box, in equal ratios.
    static func x(hz: Double, in box: CGRect) -> CGFloat {
        let f = log(max(hz, lowHz) / lowHz) / log(highHz / lowHz)
        return box.minX + CGFloat(min(max(f, 0), 1)) * box.width
    }

    static func hz(x: CGFloat, in box: CGRect) -> Double {
        let f = min(max(Double((x - box.minX) / max(box.width, 1)), 0), 1)
        return lowHz * pow(highHz / lowHz, f)
    }

    static func y(db: Double, in box: CGRect) -> CGFloat {
        let f = (min(max(db, lowDb), highDb) - lowDb) / (highDb - lowDb)
        return box.maxY - CGFloat(f) * box.height
    }

    static func db(y: CGFloat, in box: CGRect) -> Double {
        let f = min(max(Double((box.maxY - y) / max(box.height, 1)), 0), 1)
        return lowDb + f * (highDb - lowDb)
    }

    /// The filter's corner: at the cutoff, at the gain the curve has there.
    static func corner(mode: String, sections: Int, cutoffHz: Double, resonancePercent: Double, in box: CGRect) -> CGPoint {
        CGPoint(x: x(hz: cutoffHz, in: box), y: y(db: cornerDb(mode: mode, sections: sections, resonancePercent: resonancePercent), in: box))
    }

    /// The cutoff and resonance a corner dragged to a point means: the
    /// frequency under it, and the resonance whose corner gain is at its
    /// height, the top of the box being full resonance where that gain
    /// would be above it. A notch has no corner gain, so its resonance is
    /// the height read straight, 0 at the bottom and 100 at the top.
    static func filter(at p: CGPoint, mode: String, sections: Int, in box: CGRect) -> (cutoffHz: Double, resonancePercent: Double) {
        let cutoff = (hz(x: p.x, in: box) * 10).rounded() / 10
        let resonance: Double
        if mode == "notch" {
            resonance = min(max(Double((box.maxY - p.y) / max(box.height, 1)), 0), 1) * 100
        } else if p.y <= box.minY {
            resonance = 100
        } else {
            resonance = self.resonance(cornerDb: db(y: p.y, in: box), sections: sections)
        }
        return (min(max(cutoff, lowHz), highHz), (resonance * 10).rounded() / 10)
    }

    // MARK: Envelopes

    /// How much of a quarter of the box a time takes: nothing for 0 ms and
    /// the whole quarter for the longest, in equal ratios of (1 + ms).
    static func stretch(ms: Double) -> Double {
        log(1 + max(ms, 0)) / log(1 + longestMs)
    }

    static func ms(stretch: Double) -> Double {
        let ms = exp(min(max(stretch, 0), 1) * log(1 + longestMs)) - 1
        return ms < 10 ? (ms * 10).rounded() / 10 : ms.rounded()
    }

    /// An envelope as it is drawn in a box: its corners from the start, up
    /// the attack, down the decay to the sustain, held for a quarter, and
    /// down the release. Each of attack, decay and release has a quarter of
    /// the width at most.
    static func envelope(attackMs: Double, decayMs: Double, sustainPercent: Double, releaseMs: Double, in box: CGRect) -> [CGPoint] {
        let q = Double(box.width) / 4
        let (xa, xd, xr) = (q * stretch(ms: attackMs), q * stretch(ms: decayMs), q * stretch(ms: releaseMs))
        let sustain = box.maxY - CGFloat(min(max(sustainPercent, 0), 100) / 100) * box.height
        return [
            CGPoint(x: box.minX, y: box.maxY),
            CGPoint(x: box.minX + CGFloat(xa), y: box.minY),
            CGPoint(x: box.minX + CGFloat(xa + xd), y: sustain),
            CGPoint(x: box.minX + CGFloat(xa + xd + q), y: sustain),
            CGPoint(x: box.minX + CGFloat(xa + xd + q + xr), y: box.maxY),
        ]
    }

    /// Which of an envelope's handles is at a point, if any: the end of the
    /// attack, of the decay, or of the release.
    enum Handle: Equatable {
        case attack, decay, release
    }

    static func handle(at p: CGPoint, corners: [CGPoint]) -> Handle? {
        guard corners.count == 5 else { return nil }
        let candidates: [(Handle, CGPoint)] = [(.decay, corners[2]), (.attack, corners[1]), (.release, corners[4])]
        return candidates.first { hypot($0.1.x - p.x, $0.1.y - p.y) <= grab }?.0
    }

    /// The fields a handle dragged to a point sets: the attack's time; the
    /// decay's time and the sustain's level; the release's time. `corners`
    /// are the envelope's as it was when the drag began.
    static func envelope(_ handle: Handle, at p: CGPoint, corners: [CGPoint], in box: CGRect) -> [(field: String, value: Double)] {
        let q = Double(box.width) / 4
        let time = { (from: CGFloat) in ms(stretch: Double(p.x - from) / max(q, 1)) }
        switch handle {
        case .attack:
            return [("attack_ms", time(box.minX))]
        case .decay:
            let sustain = min(max(Double((box.maxY - p.y) / max(box.height, 1)), 0), 1) * 100
            return [("decay_ms", time(corners[1].x)), ("sustain_percent", sustain.rounded())]
        case .release:
            return [("release_ms", time(corners[3].x))]
        }
    }

    // MARK: Waves and LFOs

    /// A wave's level at `t` of its cycle (0 to 1), -1 to 1, as the Synth
    /// makes it: `pulseWidth` is the percent of a pulse's cycle that is
    /// high, and the pulse is kept centered. Noise is a fixed scatter.
    static func wave(_ name: String, at t: Double, pulseWidth: Double = 50) -> Double {
        let t = t - floor(t)
        switch name {
        case "sine": return sin(2 * .pi * t)
        case "triangle": return t < 0.25 ? 4 * t : t < 0.75 ? 2 - 4 * t : 4 * t - 4
        case "square": return t < 0.5 ? 1 : -1
        case "pulse":
            let w = min(max(pulseWidth / 100, 0.01), 0.99)
            return (t < w ? 1 : -1) - (2 * w - 1)
        case "noise":
            // The same scatter each time, so the drawing holds still.
            var x = UInt64(t * 4096) &* 0x9E37_79B9_7F4A_7C15 ^ 0x5EED_5EED
            x ^= x >> 29
            x = x &* 0xBF58_476D_1CE4_E5B9
            x ^= x >> 32
            return Double(x & 0xFFFF) / 32767.5 - 1
        default: return 2 * t - 1
        }
    }

    /// An LFO's level at `p` of its cycle, -1 to 1, as the Synth makes it:
    /// a saw falls, and a sample-and-hold holds a value for a cycle.
    static func lfo(_ shape: String, at p: Double) -> Double {
        let p = p - floor(p)
        switch shape {
        case "triangle": return p < 0.25 ? 4 * p : p < 0.75 ? 2 - 4 * p : 4 * p - 4
        case "saw": return 1 - 2 * p
        case "square": return p < 0.5 ? 1 : -1
        case "sample_hold": return 0.6
        default: return sin(2 * .pi * p)
        }
    }

    // MARK: Keys

    /// An octave of keys, C to the C above: thirteen notes over eight white
    /// keys, the black ones between them in the upper part.
    struct Keys {
        var box: CGRect
        /// The MIDI number of the lowest key, a C.
        var low: Int

        /// Semitones above the low C that are black keys.
        static let black: Set<Int> = [1, 3, 6, 8, 10]
        /// Each semitone's white key, counted from the left.
        static let whiteIndex = [0, 0, 1, 1, 2, 3, 3, 4, 4, 5, 5, 6, 7]

        var whiteWidth: CGFloat { box.width / 8 }

        /// A key's rectangle: a white key the whole height, a black key the
        /// upper three fifths, astride the line between its neighbors.
        func frame(semitone: Int) -> CGRect {
            let w = whiteWidth
            if Self.black.contains(semitone) {
                let left = CGFloat(Self.whiteIndex[semitone] + 1) * w
                return CGRect(x: box.minX + left - w * 0.3, y: box.minY, width: w * 0.6, height: box.height * 0.6)
            }
            return CGRect(x: box.minX + CGFloat(Self.whiteIndex[semitone]) * w, y: box.minY, width: w, height: box.height)
        }

        /// Whether a point is in a rectangle, its far edges included.
        private static func holds(_ r: CGRect, _ p: CGPoint) -> Bool {
            p.x >= r.minX && p.x <= r.maxX && p.y >= r.minY && p.y <= r.maxY
        }

        /// The note under a point, black keys first since they lie over the
        /// white ones, and how hard it is pressed: softly at the top of the
        /// key, hard at the bottom, 1 to 127.
        func note(at p: CGPoint) -> (pitch: Int, velocity: Int)? {
            guard Self.holds(box, p) else { return nil }
            let order = (0...12).sorted { Self.black.contains($0) && !Self.black.contains($1) }
            guard let semitone = order.first(where: { Self.holds(frame(semitone: $0), p) }) else { return nil }
            let f = frame(semitone: semitone)
            let depth = min(max(Double((p.y - f.minY) / max(f.height, 1)), 0), 1)
            return (low + semitone, max(1, Int((40 + 87 * depth).rounded())))
        }
    }

    /// The name of the C an octave of keys starts on: `C3` for 48.
    static func octaveName(low: Int) -> String {
        noteName(midi: Int32(low))
    }

    // MARK: Modulation

    /// How far a matrix entry moves a control: where its value sits across
    /// the control, 0 to 1, and where full modulation takes it, given the
    /// control's range and whether it moves in equal ratios. An amount in
    /// octaves doubles the value so many times; any other adds to it in the
    /// control's own unit.
    static func reach(value: Double, min lo: Double, max hi: Double, log: Bool, amount: Double, unit: String) -> (from: Double, to: Double) {
        let scale = ValueScale(min: lo, max: hi, log: log)
        let moved = unit == "octaves" ? value * pow(2, amount) : value + amount
        return (scale.fraction(value), scale.fraction(min(max(moved, lo), hi)))
    }

    /// The amount a source dropped on a control starts with, in the
    /// target's unit: enough to hear.
    static func startingAmount(unit: String) -> Double {
        switch unit {
        case "octaves": 1
        case "semitones": 1
        case "dB": 6
        case "points": 25
        case "pan": 0.5
        default: 1
        }
    }
}
