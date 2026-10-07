import AAWCore
import Foundation

/// Where the equalizer's panel draws its bands and what a drag on them
/// means: each band's response and the curve they make together, the point
/// each band is held by, what a point dragged to a place sets, and the
/// playing spectrum under them. Pure geometry, tested in `Tests`; the view
/// draws from it and sends what it works out to the host. The responses
/// are those of the engine's sections, so the curve is what plays.
enum EqLayout {
    /// The range the curve draws: a band's 20 Hz to 20 kHz, and ±30 dB,
    /// which holds a band's ±24 dB of gain and a resonant corner.
    static let lowHz = 20.0
    static let highHz = 20000.0
    static let lowDb = -30.0
    static let highDb = 30.0
    /// The spectrum's range, drawn in the same box: silence at the bottom
    /// and full scale at the top.
    static let spectrumFloorDb = -90.0
    /// How far from a point a press takes hold of it, in points.
    static let grab: CGFloat = 7
    /// Points of an Option-drag up or down that double or halve a band's q.
    static let widthTravel: CGFloat = 40
    static let lowQ = 0.1
    static let highQ = 18.0

    /// One band as its fields say.
    struct Band: Equatable {
        var shape: String
        var freqHz: Double
        var gainDb: Double
        var q: Double
        var slope: Int

        var isPass: Bool { shape == "highpass" || shape == "lowpass" }

        /// The sections the band runs as: one, or one each 12 dB of a pass's slope.
        var sections: Int { isPass ? max(slope / 12, 1) : 1 }

        /// The bands of an effect's fields, in order.
        static func bands(of fields: [FieldView]) -> [Band] {
            var out: [Band] = []
            for index in 0... {
                let prefix = "bands.\(index)."
                let own = fields.filter { $0.name.hasPrefix(prefix) }
                if own.isEmpty { break }
                func number(_ name: String, _ fallback: Double) -> Double {
                    if case .number(let x)? = own.first(where: { $0.name == prefix + name })?.value { return x }
                    return fallback
                }
                var shape = "bell"
                if case .text(let s)? = own.first(where: { $0.name == prefix + "shape" })?.value { shape = s }
                out.append(Band(shape: shape, freqHz: number("freq_hz", 1000), gainDb: number("gain_db", 0), q: number("q", 0.71),
                                slope: Int(number("slope_db_per_octave", 12))))
            }
            return out
        }
    }

    // MARK: Responses

    /// The Q of each section of a Butterworth filter of `order`, least
    /// resonant first, as the engine lays them out.
    static func butterworthQ(order: Int) -> [Double] {
        (0..<max(order / 2, 1)).map { 1 / (2 * sin(Double.pi * Double(2 * $0 + 1) / Double(2 * order))) }
    }

    /// The Q of each section of a pass band: the Butterworth Qs of its
    /// slope, the last scaled by the band's q over 1/√2, so 0.71 is flat.
    static func passQ(_ band: Band) -> [Double] {
        var qs = butterworthQ(order: 2 * band.sections)
        qs[qs.count - 1] *= band.q * 2.squareRoot()
        return qs
    }

    /// The RBJ cookbook coefficients of one section of a band at `q`,
    /// normalized to a0 = 1: b0, b1, b2, a1, a2.
    static func section(_ band: Band, q: Double, rate: Double) -> [Double] {
        let a = pow(10, band.gainDb / 40)
        let w = 2 * Double.pi * min(max(band.freqHz, 1), rate / 2 - 1) / rate
        let (c, alpha) = (cos(w), sin(w) / (2 * q))
        let b: [Double]
        let d: [Double]
        switch band.shape {
        case "highpass":
            b = [(1 + c) / 2, -(1 + c), (1 + c) / 2]
            d = [1 + alpha, -2 * c, 1 - alpha]
        case "lowpass":
            b = [(1 - c) / 2, 1 - c, (1 - c) / 2]
            d = [1 + alpha, -2 * c, 1 - alpha]
        case "low_shelf", "high_shelf":
            let root = 2 * a.squareRoot() * alpha
            let sign: Double = band.shape == "low_shelf" ? 1 : -1
            b = [a * ((a + 1) - sign * (a - 1) * c + root),
                 sign * 2 * a * ((a - 1) - sign * (a + 1) * c),
                 a * ((a + 1) - sign * (a - 1) * c - root)]
            d = [(a + 1) + sign * (a - 1) * c + root,
                 -sign * 2 * ((a - 1) + sign * (a + 1) * c),
                 (a + 1) + sign * (a - 1) * c - root]
        default:
            b = [1 + alpha * a, -2 * c, 1 - alpha * a]
            d = [1 + alpha / a, -2 * c, 1 - alpha / a]
        }
        return [b[0] / d[0], b[1] / d[0], b[2] / d[0], d[1] / d[0], d[2] / d[0]]
    }

    /// The magnitude of a section at `hz`, in dB.
    static func sectionDb(_ s: [Double], hz: Double, rate: Double) -> Double {
        let w = 2 * Double.pi * hz / rate
        let (c1, s1, c2, s2) = (cos(w), sin(w), cos(2 * w), sin(2 * w))
        let numerator = hypot(s[0] + s[1] * c1 + s[2] * c2, -(s[1] * s1 + s[2] * s2))
        let denominator = hypot(1 + s[3] * c1 + s[4] * c2, -(s[3] * s1 + s[4] * s2))
        return 20 * log10(max(numerator, 1e-12) / max(denominator, 1e-12))
    }

    /// A band's response at `hz`, in dB: the cookbook section of a bell or a
    /// shelf, or the sections of a pass in series.
    static func responseDb(_ band: Band, hz: Double, rate: Double) -> Double {
        if band.isPass {
            return passQ(band).reduce(0) { $0 + sectionDb(section(band, q: $1, rate: rate), hz: hz, rate: rate) }
        }
        return sectionDb(section(band, q: band.q, rate: rate), hz: hz, rate: rate)
    }

    /// The bands' response together at `hz`, in dB.
    static func curveDb(_ bands: [Band], hz: Double, rate: Double) -> Double {
        bands.reduce(0) { $0 + responseDb($1, hz: hz, rate: rate) }
    }

    // MARK: Places

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

    /// The gain at a pass band's corner: the product of its sections' Qs,
    /// which is where the curve sits at the cutoff.
    static func cornerDb(_ band: Band) -> Double {
        20 * log10(passQ(band).reduce(1, *))
    }

    /// The q of a pass band whose corner is at `db`, inverting `cornerDb`.
    static func passQ(cornerDb db: Double, slope: Int) -> Double {
        let flat = butterworthQ(order: 2 * max(slope / 12, 1)).reduce(1, *)
        return min(max(pow(10, db / 20) / flat / 2.squareRoot(), lowQ), highQ)
    }

    /// The point a band is held by: at its frequency, at its gain for a
    /// bell or a shelf and at the corner for a pass.
    static func point(_ band: Band, in box: CGRect) -> CGPoint {
        CGPoint(x: x(hz: band.freqHz, in: box), y: y(db: band.isPass ? cornerDb(band) : band.gainDb, in: box))
    }

    /// The band whose point is under `p`, the nearest within reach.
    static func grab(at p: CGPoint, bands: [Band], in box: CGRect) -> Int? {
        var best: (index: Int, distance: CGFloat)?
        for (i, band) in bands.enumerated() {
            let q = point(band, in: box)
            let d: CGFloat = hypot(q.x - p.x, q.y - p.y)
            if d <= grab, best.map({ d < $0.distance }) ?? true { best = (i, d) }
        }
        return best?.index
    }

    /// A band dragged by its point to `p`: the frequency under it, and the
    /// gain at its height, or for a pass the q whose corner is at its
    /// height, the top of the box being the most resonant.
    static func dragged(_ band: Band, to p: CGPoint, in box: CGRect) -> Band {
        var moved = band
        moved.freqHz = min(max((hz(x: p.x, in: box) * 10).rounded() / 10, lowHz), highHz)
        if band.isPass {
            let q = p.y <= box.minY ? highQ : passQ(cornerDb: db(y: p.y, in: box), slope: band.slope)
            moved.q = (q * 100).rounded() / 100
        } else {
            moved.gainDb = min(max((db(y: p.y, in: box) * 10).rounded() / 10, -24), 24)
        }
        return moved
    }

    /// A band's q after an Option-drag of `dy` points, up narrowing it: q
    /// doubles each `widthTravel` points up and halves each down.
    static func widened(q: Double, by dy: CGFloat) -> Double {
        let q = q * pow(2, -Double(dy / widthTravel))
        return (min(max(q, lowQ), highQ) * 100).rounded() / 100
    }

    // MARK: The spectrum

    /// Where the spectrum's top is at each column of the box: the loudest
    /// bin under the column, or the level between the bins either side
    /// where the bins are further apart than the columns, from
    /// `spectrumFloorDb` at the bottom to 0 dB at the top. `levels` are the
    /// bins from 0 Hz to half the rate, as the host gives them.
    static func spectrumColumns(levels: [Float], sampleRate: Double, in box: CGRect) -> [CGFloat] {
        let columns = Int(box.width.rounded(.down))
        guard columns > 0, levels.count > 1 else { return [] }
        let binHz = sampleRate / Double(2 * (levels.count - 1))
        func level(atBin k: Double) -> Double {
            let i = Int(k.rounded(.down))
            guard i >= 0, i + 1 < levels.count else { return spectrumFloorDb }
            let t = k - Double(i)
            return Double(levels[i]) * (1 - t) + Double(levels[i + 1]) * t
        }
        return (0..<columns).map { column in
            let x0 = box.minX + CGFloat(column)
            let (hz0, hz1) = (hz(x: x0, in: box), hz(x: x0 + 1, in: box))
            let (k0, k1) = (hz0 / binHz, hz1 / binHz)
            var loudest = spectrumFloorDb
            if k1 - k0 >= 1 {
                for k in Int(k0.rounded(.up))...Int(k1.rounded(.down)) where k < levels.count {
                    loudest = max(loudest, Double(levels[k]))
                }
            } else {
                loudest = level(atBin: (k0 + k1) / 2)
            }
            let f = (min(max(loudest, spectrumFloorDb), 0) - spectrumFloorDb) / -spectrumFloorDb
            return box.maxY - CGFloat(f) * box.height
        }
    }
}
