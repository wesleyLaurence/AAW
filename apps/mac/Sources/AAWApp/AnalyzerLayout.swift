import AAWCore
import Foundation

/// Where the analyzer's window puts its panes and what its meters draw: the
/// arrangement of panes, which is the app's and kept between projects, the
/// rects the panes get from a window's size, the dividers between them and
/// what a drag on one means, and the scales the levels, loudness and stereo
/// field are drawn on. Pure geometry, tested in `Tests`; the views draw
/// from it.
enum AnalyzerLayout {
    /// The views the window holds. The spectrogram and the waveform are not
    /// built yet.
    enum Pane: String, CaseIterable, Codable {
        case levels, loudness, spectrum, stereo

        var title: String {
            switch self {
            case .levels: "Levels"
            case .loudness: "Loudness"
            case .spectrum: "Spectrum"
            case .stereo: "Stereo Field"
            }
        }
    }

    /// Which panes show and how large each is: two columns, the levels over
    /// the loudness at the left and the spectrum over the stereo field at the
    /// right, with a divider between the columns and one down each. A pane
    /// can fill the window for a while, and any pane can be hidden. Kept as
    /// JSON in the app's defaults.
    struct Arrangement: Equatable, Codable {
        var hidden: Set<Pane> = []
        var filled: Pane?
        /// The columns' divider across the window, as a share of its width.
        var split: CGFloat = 0.36
        /// The left column's divider down it, and the right column's, as a
        /// share of the height.
        var leftSplit: CGFloat = 0.5
        var rightSplit: CGFloat = 0.58

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

        static func read(_ data: Data?) -> Arrangement {
            guard let data, let read = try? JSONDecoder().decode(Arrangement.self, from: data) else { return .standard }
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

    private static func column(left: Bool, _ a: Arrangement) -> [Pane] {
        (left ? [Pane.levels, .loudness] : [.spectrum, .stereo]).filter { !a.hidden.contains($0) }
    }

    /// Each shown pane's rect in `bounds`.
    static func panes(_ a: Arrangement, in bounds: CGRect) -> [Pane: CGRect] {
        if let filled = a.filled, !a.hidden.contains(filled) {
            return [filled: bounds]
        }
        let (left, right) = (column(left: true, a), column(left: false, a))
        var out: [Pane: CGRect] = [:]
        func stack(_ panes: [Pane], in rect: CGRect, split: CGFloat) {
            switch panes.count {
            case 1: out[panes[0]] = rect
            case 2:
                let y = (rect.minY + rect.height * split).rounded()
                out[panes[0]] = CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: y - gap / 2 - rect.minY)
                out[panes[1]] = CGRect(x: rect.minX, y: y + gap / 2, width: rect.width, height: rect.maxY - y - gap / 2)
            default: break
            }
        }
        switch (left.isEmpty, right.isEmpty) {
        case (true, true): break
        case (false, true): stack(left, in: bounds, split: a.leftSplit)
        case (true, false): stack(right, in: bounds, split: a.rightSplit)
        case (false, false):
            let x = (bounds.minX + bounds.width * a.split).rounded()
            stack(left, in: CGRect(x: bounds.minX, y: bounds.minY, width: x - gap / 2 - bounds.minX, height: bounds.height), split: a.leftSplit)
            stack(right, in: CGRect(x: x + gap / 2, y: bounds.minY, width: bounds.maxX - x - gap / 2, height: bounds.height), split: a.rightSplit)
        }
        return out
    }

    enum Divider: Equatable {
        case split, leftSplit, rightSplit
    }

    /// Which divider a point takes hold of, if any: a divider is there only
    /// between two panes that show.
    static func divider(at p: CGPoint, _ a: Arrangement, in bounds: CGRect) -> Divider? {
        guard a.filled == nil || a.hidden.contains(a.filled!) else { return nil }
        let (left, right) = (column(left: true, a), column(left: false, a))
        let x = bounds.minX + bounds.width * a.split
        if !left.isEmpty, !right.isEmpty, abs(p.x - x) <= dividerGrab { return .split }
        let inLeft = left.isEmpty ? false : right.isEmpty || p.x < x
        let (panes, split) = inLeft ? (left, a.leftSplit) : (right, a.rightSplit)
        guard panes.count == 2 else { return nil }
        let y = bounds.minY + bounds.height * split
        return abs(p.y - y) <= dividerGrab ? (inLeft ? .leftSplit : .rightSplit) : nil
    }

    /// The arrangement with a divider dragged to a point, each pane kept at
    /// least `leastShare` of the window.
    static func dragged(_ a: Arrangement, _ d: Divider, to p: CGPoint, in bounds: CGRect) -> Arrangement {
        var out = a
        let share = { (value: CGFloat) in min(max(value, leastShare), 1 - leastShare) }
        switch d {
        case .split: out.split = share((p.x - bounds.minX) / max(bounds.width, 1))
        case .leftSplit: out.leftSplit = share((p.y - bounds.minY) / max(bounds.height, 1))
        case .rightSplit: out.rightSplit = share((p.y - bounds.minY) / max(bounds.height, 1))
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
}
