import AAWCore
import Foundation

/// Where the analyzer's window puts its panes and what its meters draw: the
/// arrangement of panes, which is the app's and kept between projects, the
/// rects the panes get from a window's size, the dividers between them and
/// what a drag on one means, the scales the levels, loudness and stereo
/// field are drawn on, and how the spectrogram's and the waveform's
/// textures scroll. Pure geometry, tested in `Tests`; the views draw from it.
enum AnalyzerLayout {
    /// The views the window holds.
    enum Pane: String, CaseIterable, Codable {
        case levels, loudness, spectrum, stereo, waveform, spectrogram

        var title: String {
            switch self {
            case .levels: "Levels"
            case .loudness: "Loudness"
            case .spectrum: "Spectrum"
            case .stereo: "Stereo Field"
            case .waveform: "Waveform"
            case .spectrogram: "Spectrogram"
            }
        }
    }

    /// The panes in their columns: the meters at the left, the frequency
    /// and the field in the middle, and the two that scroll with time at the
    /// right, over each other so their seconds line up.
    static let columns: [[Pane]] = [[.levels, .loudness], [.spectrum, .stereo], [.waveform, .spectrogram]]

    /// Which panes show and how large each is: three columns of two, with a
    /// divider between each pair of columns that show and one down each
    /// column whose two panes show. A pane can fill the window for a while,
    /// and any pane can be hidden. Kept as JSON in the app's defaults.
    struct Arrangement: Equatable, Codable {
        var hidden: Set<Pane> = []
        var filled: Pane?
        /// Each column's share of the width; the columns that show divide
        /// the window in their shares' proportion.
        var widths: [CGFloat] = [0.26, 0.34, 0.4]
        /// Each column's divider down it, as a share of the height.
        var splits: [CGFloat] = [0.5, 0.58, 0.4]

        static let standard = Arrangement()
        static let defaultsKey = "analyzer.arrangement"

        /// The panes that show, in drawing order.
        var shown: [Pane] {
            if let filled, !hidden.contains(filled) { return [filled] }
            return Pane.allCases.filter { !hidden.contains($0) }
        }

        /// Shows the pane or hides it. The last pane showing cannot hide.
        mutating func toggle(_ pane: Pane) {
            if hidden.contains(pane) {
                hidden.remove(pane)
            } else if Pane.allCases.filter({ !hidden.contains($0) }).count > 1 {
                hidden.insert(pane)
                if filled == pane { filled = nil }
            }
        }

        /// Fills the window with the pane, or gives it back its place.
        mutating func toggleFill(_ pane: Pane) {
            filled = filled == pane ? nil : pane
        }

        /// An arrangement read back; anything that does not read as one,
        /// an older shape among them, is the standard arrangement.
        static func read(_ data: Data?) -> Arrangement {
            guard let data, let read = try? JSONDecoder().decode(Arrangement.self, from: data),
                  read.widths.count == columns.count, read.splits.count == columns.count,
                  read.widths.allSatisfy({ $0 > 0 }) else { return .standard }
            return read
        }

        var data: Data? { try? JSONEncoder().encode(self) }
    }

    /// Between two panes.
    static let gap: CGFloat = 6
    /// How far from a divider a press takes hold of it, in points.
    static let dividerGrab: CGFloat = 5
    /// How much of the window a divider leaves a pane at the least.
    static let leastShare: CGFloat = 0.15
    /// The strip at a pane's top with its title and its fill mark.
    static let paneBar: CGFloat = 18

    /// The panes of a column that show, in order.
    private static func column(_ c: Int, _ a: Arrangement) -> [Pane] {
        columns[c].filter { !a.hidden.contains($0) }
    }

    /// The columns with a pane showing, in order.
    static func shownColumns(_ a: Arrangement) -> [Int] {
        columns.indices.filter { !column($0, a).isEmpty }
    }

    /// Each shown column's rect in `bounds`, by column, the gap kept
    /// between neighbors; empty while a pane fills the window.
    static func columnRects(_ a: Arrangement, in bounds: CGRect) -> [Int: CGRect] {
        if let filled = a.filled, !a.hidden.contains(filled) { return [:] }
        let shown = shownColumns(a)
        let total = shown.reduce(0) { $0 + a.widths[$1] }
        guard total > 0 else { return [:] }
        var out: [Int: CGRect] = [:]
        var x = bounds.minX
        var share: CGFloat = 0
        for (i, c) in shown.enumerated() {
            share += a.widths[c]
            let end = i == shown.count - 1 ? bounds.maxX : (bounds.minX + bounds.width * share / total).rounded()
            let (left, right) = (i == 0 ? x : x + gap / 2, i == shown.count - 1 ? end : end - gap / 2)
            out[c] = CGRect(x: left, y: bounds.minY, width: right - left, height: bounds.height)
            x = end
        }
        return out
    }

    /// Each shown pane's rect in `bounds`.
    static func panes(_ a: Arrangement, in bounds: CGRect) -> [Pane: CGRect] {
        if let filled = a.filled, !a.hidden.contains(filled) {
            return [filled: bounds]
        }
        var out: [Pane: CGRect] = [:]
        for (c, rect) in columnRects(a, in: bounds) {
            let panes = column(c, a)
            switch panes.count {
            case 1: out[panes[0]] = rect
            case 2:
                let y = (rect.minY + rect.height * a.splits[c]).rounded()
                out[panes[0]] = CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: y - gap / 2 - rect.minY)
                out[panes[1]] = CGRect(x: rect.minX, y: y + gap / 2, width: rect.width, height: rect.maxY - y - gap / 2)
            default: break
            }
        }
        return out
    }

    /// A line between two panes: the one after the `n`th shown column, or
    /// the one down column `c`.
    enum Divider: Equatable {
        case between(Int)
        case down(Int)
    }

    /// Each divider that is there and the strip a press takes hold of it
    /// in, `dividerGrab` either side.
    static func dividers(_ a: Arrangement, in bounds: CGRect) -> [(divider: Divider, grab: CGRect, vertical: Bool)] {
        let rects = columnRects(a, in: bounds)
        let shown = shownColumns(a)
        var out: [(Divider, CGRect, Bool)] = []
        for (n, c) in shown.enumerated() {
            guard let rect = rects[c] else { continue }
            if n + 1 < shown.count {
                let x = rect.maxX + gap / 2
                out.append((.between(n), CGRect(x: x - dividerGrab, y: bounds.minY, width: dividerGrab * 2, height: bounds.height), true))
            }
            if column(c, a).count == 2 {
                let y = (rect.minY + rect.height * a.splits[c]).rounded()
                out.append((.down(c), CGRect(x: rect.minX, y: y - dividerGrab, width: rect.width, height: dividerGrab * 2), false))
            }
        }
        return out.map { ($0.0, $0.1, $0.2) }
    }

    /// Which divider a point takes hold of, if any.
    static func divider(at p: CGPoint, _ a: Arrangement, in bounds: CGRect) -> Divider? {
        dividers(a, in: bounds).first { $0.grab.contains(p) }?.divider
    }

    /// The arrangement with a divider dragged to a point, each pane kept at
    /// least `leastShare` of the window. A column divider moves width
    /// between the two columns either side of it and leaves the others.
    static func dragged(_ a: Arrangement, _ d: Divider, to p: CGPoint, in bounds: CGRect) -> Arrangement {
        var out = a
        switch d {
        case .down(let c):
            guard c < out.splits.count else { return a }
            out.splits[c] = min(max((p.y - bounds.minY) / max(bounds.height, 1), leastShare), 1 - leastShare)
        case .between(let n):
            let shown = shownColumns(a)
            guard n + 1 < shown.count else { return a }
            let (l, r) = (shown[n], shown[n + 1])
            let total = shown.reduce(0) { $0 + a.widths[$1] }
            let before = shown[..<n].reduce(0) { $0 + a.widths[$1] }
            let pair = a.widths[l] + a.widths[r]
            let least = leastShare * total
            let left = min(max((p.x - bounds.minX) / max(bounds.width, 1) * total - before, least), pair - least)
            out.widths[l] = left
            out.widths[r] = pair - left
        }
        return out
    }

    /// The pane under a point, and whether the point is on its fill mark at
    /// the top right of its bar.
    static func pane(at p: CGPoint, _ a: Arrangement, in bounds: CGRect) -> (pane: Pane, fillMark: Bool)? {
        for (pane, rect) in panes(a, in: bounds) where rect.contains(p) {
            let mark = CGRect(x: rect.maxX - paneBar, y: rect.minY, width: paneBar, height: paneBar)
            return (pane, mark.contains(p))
        }
        return nil
    }

    // MARK: Scales

    /// The level meters' range: −60 dB at the foot and +6 at the top, as the
    /// faders show, so a peak over full scale is seen.
    static let levelFloorDb = -60.0
    static let levelTopDb = 6.0
    /// The loudness scale: −60 LUFS at the foot and 0 at the top, so a quiet
    /// mix is seen and a mastering target at −14 sits high.
    static let loudnessFloor = -60.0
    static let loudnessTop = 0.0

    /// Where a value on a scale from `floor` to `top` falls in a box, from
    /// the top down; held inside the box.
    static func y(_ value: Double, floor: Double, top: Double, in box: CGRect) -> CGFloat {
        let share = (min(max(value, floor), top) - floor) / (top - floor)
        return box.maxY - CGFloat(share) * box.height
    }

    /// A shown peak falling toward a lower reading at 20 dB a second, or
    /// jumping up to a higher one, so a meter reads as meters do.
    static func fallen(_ shown: Double, toward value: Double, seconds: Double) -> Double {
        value >= shown ? value : max(value, shown - 20 * seconds)
    }

    /// A vectorscope point: the left channel up the left diagonal and the
    /// right up the right one, so a mono signal draws a vertical line, an
    /// inverted one a horizontal line and a wide one a cloud. `gain` scales
    /// the samples: 1 puts full scale on both channels at the box's top, and
    /// `scopeGain` puts the loudest frame shown there, so the shape is seen
    /// whatever the level.
    static func scopePoint(left: Double, right: Double, gain: Double = 1, in box: CGRect) -> CGPoint {
        let (mid, side) = ((left + right) / 2 * gain, (right - left) / 2 * gain)
        let half = min(box.width, box.height) / 2
        return CGPoint(x: box.midX + CGFloat(side) * half, y: box.midY - CGFloat(mid) * half)
    }

    /// The gain that puts the loudest sample among the frames at full scale,
    /// no more than 60 dB, so silence stays a point.
    static func scopeGain(_ samples: [Float]) -> Double {
        let peak = samples.reduce(0.0) { max($0, Double(abs($1))) }
        return 1 / max(peak, 0.001)
    }

    /// Where a correlation from −1 to +1 falls across a bar.
    static func correlationX(_ correlation: Double, in bar: CGRect) -> CGFloat {
        bar.minX + CGFloat((min(max(correlation, -1), 1) + 1) / 2) * bar.width
    }

    /// Where a balance in dB falls across a bar: ±12 dB to its ends.
    static func balanceX(_ db: Double, in bar: CGRect) -> CGFloat {
        bar.minX + CGFloat((min(max(db, -12), 12) + 12) / 24) * bar.width
    }

    /// The short-term loudness history as points across a box, newest at
    /// the right, `perSecond` values a second over `seconds` seconds of
    /// width; silence is left out.
    static func historyPoints(_ history: [Float], seconds: Double, perSecond: Double, in box: CGRect) -> [CGPoint] {
        let step = box.width / CGFloat(seconds * perSecond)
        return history.enumerated().compactMap { i, lufs in
            let x = box.maxX - CGFloat(history.count - 1 - i) * step
            guard x >= box.minX, lufs > Float(loudnessFloor) else { return nil }
            return CGPoint(x: x, y: y(Double(lufs), floor: loudnessFloor, top: loudnessTop, in: box))
        }
    }

    /// A reading for a label: one decimal, `−∞` for silence.
    static func text(_ value: Float?, unit: String) -> String {
        guard let value, value > -150 else { return "−∞" }
        let sign = value < 0 ? "−" : ""
        return "\(sign)\(String(format: "%.1f", abs(value))) \(unit)"
    }

    // MARK: The textures that scroll

    /// Columns the spectrogram's texture holds and the waveform's: at the
    /// host's 50 and 100 columns a second, 20.48 s of each, so the two
    /// panes read the same seconds.
    static let spectrogramColumns = 1024
    static let waveformColumns = 2048
    /// Rows of the waveform's texture for each channel.
    static let waveformRows = 192
    /// The spectrogram's frequency range, the host's: 20 Hz to 20 kHz, as
    /// the spectrum is drawn.
    static let spectrogramLowHz = 20.0
    static let spectrogramHighHz = 20000.0
    /// The level the spectrogram's darkest color stands for; 0 dB is its
    /// brightest, as the spectrum's floor and top are.
    static let spectrogramFloorDb = -90.0

    /// Where a frequency falls up a box, lowest at the foot, in equal steps
    /// of pitch, as the spectrogram's rows are.
    static func spectrogramY(hz: Double, in box: CGRect) -> CGFloat {
        let f = log(min(max(hz, spectrogramLowHz), spectrogramHighHz) / spectrogramLowHz) / log(spectrogramHighHz / spectrogramLowHz)
        return box.maxY - CGFloat(f) * box.height
    }

    /// A level's place on the spectrogram's color ramp, 0 at the floor to
    /// `steps - 1` at 0 dB.
    static func colorIndex(_ db: Float, steps: Int) -> Int {
        let share = (min(max(Double(db), spectrogramFloorDb), 0) - spectrogramFloorDb) / -spectrogramFloorDb
        return Int((share * Double(steps - 1)).rounded())
    }

    /// The row of a waveform texture `rows` tall a sample falls on, +1 at
    /// the top row and −1 at the bottom.
    static func waveformRow(_ sample: Float, rows: Int) -> Int {
        let share = (1 - Double(min(max(sample, -1), 1))) / 2
        return min(max(Int((share * Double(rows - 1)).rounded()), 0), rows - 1)
    }

    /// What a reading adds to a texture: the columns of a reading that a
    /// watcher who had seen `seen` columns has not, the blank ones first
    /// for columns the reading no longer holds, and the count the watcher
    /// has then seen. A count that went backwards is a meter started again,
    /// whose texture starts again too.
    static func newColumns(seen: UInt64, total: UInt64, held: Int) -> (clear: Bool, blank: Int, take: Int, seen: UInt64) {
        if total < seen { return (true, 0, min(Int(total), held), total) }
        let new = total - seen
        let take = Int(min(new, UInt64(held)))
        let blank = Int(min(new - UInt64(take), UInt64(Int32.max)))
        return (false, blank, take, total)
    }

    /// How a texture of `width` columns, `written` of them so far, newest
    /// last and wrapping around, is drawn across a box: each slice's
    /// columns in the texture and the rect it fills, the newest column at
    /// the box's right edge and a column a `width`th of the box wide.
    static func slices(written: Int, width: Int, in box: CGRect) -> [(columns: Range<Int>, rect: CGRect)] {
        guard written > 0, width > 0 else { return [] }
        let column = box.width / CGFloat(width)
        if written < width {
            return [(0..<written, CGRect(x: box.maxX - CGFloat(written) * column, y: box.minY, width: CGFloat(written) * column, height: box.height))]
        }
        let head = written % width
        var out: [(Range<Int>, CGRect)] = []
        if head < width {
            out.append((head..<width, CGRect(x: box.minX, y: box.minY, width: CGFloat(width - head) * column, height: box.height)))
        }
        if head > 0 {
            out.append((0..<head, CGRect(x: box.minX + CGFloat(width - head) * column, y: box.minY, width: CGFloat(head) * column, height: box.height)))
        }
        return out
    }

    /// Where the marks of whole seconds fall across a box that shows the
    /// last `seconds` seconds, newest at the right, every `every` seconds
    /// back from now: each mark's x and its label.
    static func secondMarks(seconds: Double, every: Double, in box: CGRect) -> [(x: CGFloat, label: String)] {
        guard every > 0, seconds > 0 else { return [] }
        return stride(from: every, to: seconds, by: every).map { back in
            (box.maxX - CGFloat(back / seconds) * box.width, "−\(Int(back)) s")
        }
    }
}
