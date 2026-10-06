import CoreGraphics
import Foundation

/// Where a pattern's rows, steps and events fall in the pattern editor: a row
/// for each pad of the track that plays it, time left to right from the
/// pattern's start, and for a pad with a pitch its notes top to bottom.
/// Geometry only; the pattern itself lives in the host.
public struct PatternLayout: Equatable {
    /// The pad names, left of the rows.
    public static let gutter: CGFloat = 96
    public static let rulerHeight: CGFloat = 16
    /// A row of steps or of hits.
    public static let rowHeight: CGFloat = 24
    /// The least height of a semitone and of a row of notes.
    public static let minSemitone: CGFloat = 6
    public static let minNotesHeight: CGFloat = 72
    /// A row of notes shows at least this many semitones.
    public static let minSpan = 13
    public static let maxPixelsPerBeat: CGFloat = 320
    /// The least width of a step at the zoom a pattern opens with.
    public static let minStep: CGFloat = 9

    public enum Kind: Equatable {
        /// A pad whose hits are steps on the grid.
        case steps
        /// A held pad without a pitch: its hits are events with a length.
        case hits
        /// A pad whose sample has a pitch: events by note, from `low` to `high`.
        case notes(low: Int, high: Int)
    }

    public struct Pad: Equatable {
        public var name: String
        /// Whether events hold the sample for their duration.
        public var gate: Bool
        /// The sample's root note as a MIDI number.
        public var root: Int?
        /// The notes the pattern's events play on the pad.
        public var pitches: [Int]

        public init(name: String, gate: Bool, root: Int?, pitches: [Int] = []) {
            self.name = name
            self.gate = gate
            self.root = root
            self.pitches = pitches
        }
    }

    public struct Row: Equatable {
        public var pad: Pad
        public var kind: Kind
        /// Among the rows, from the top of the first.
        public var top: CGFloat
        public var height: CGFloat

        /// The height of one note of a row of notes.
        public var semitone: CGFloat {
            if case .notes(let low, let high) = kind { return height / CGFloat(high - low + 1) }
            return height
        }
    }

    public var size: CGSize = .zero
    public var lengthBeats: Double = 4
    /// The length of a step, in beats.
    public var grid: Double = 0.25
    public var pixelsPerBeat: CGFloat = 72
    public var scroll: CGPoint = .zero
    public private(set) var rows: [Row] = []

    public init() {}

    public var lanesWidth: CGFloat { max(0, size.width - Self.gutter) }
    public var lanesHeight: CGFloat { max(0, size.height - Self.rulerHeight) }
    public var contentWidth: CGFloat { CGFloat(lengthBeats) * pixelsPerBeat }
    public var contentHeight: CGFloat { rows.last.map { $0.top + $0.height } ?? 0 }
    public var steps: Int { max(0, Int((lengthBeats / grid).rounded(.up))) }

    /// The notes a row of a pad with a pitch shows: what its events play and
    /// its root, with room to move up and down, and an octave at the least.
    public static func span(of pad: Pad) -> (low: Int, high: Int) {
        let played = pad.pitches + (pad.root.map { [$0] } ?? [])
        var low = (played.min() ?? 60) - 2
        var high = (played.max() ?? 60) + 2
        while high - low + 1 < minSpan {
            high += 1
            if high - low + 1 < minSpan { low -= 1 }
        }
        // MIDI has notes 0 to 127; a span against an end keeps its size.
        if low < 0 { (low, high) = (0, min(127, high - low)) }
        if high > 127 { (low, high) = (max(0, low - (high - 127)), 127) }
        return (low, high)
    }

    /// Lays the pads out as rows: steps and hits a line each, and the rows of
    /// notes sharing what is left of the view, or more when their notes need it.
    public mutating func setPads(_ pads: [Pad]) {
        let pitched = pads.filter { $0.root != nil }.count
        let fixed = CGFloat(pads.count - pitched) * Self.rowHeight
        let share = pitched > 0 ? (lanesHeight - fixed) / CGFloat(pitched) : 0
        var top: CGFloat = 0
        rows = pads.map { pad in
            let kind: Kind
            var height = Self.rowHeight
            if pad.root != nil {
                let (low, high) = Self.span(of: pad)
                kind = .notes(low: low, high: high)
                height = max(share, Self.minNotesHeight, CGFloat(high - low + 1) * Self.minSemitone).rounded(.down)
            } else {
                kind = pad.gate ? .hits : .steps
            }
            defer { top += height }
            return Row(pad: pad, kind: kind, top: top, height: height)
        }
        clamp()
    }

    public func x(_ beat: Double) -> CGFloat {
        Self.gutter + CGFloat(beat) * pixelsPerBeat - scroll.x
    }

    public func beat(atX x: CGFloat) -> Double {
        Double((x - Self.gutter + scroll.x) / pixelsPerBeat)
    }

    /// The view's y for a y among the rows.
    public func y(_ contentY: CGFloat) -> CGFloat {
        Self.rulerHeight + contentY - scroll.y
    }

    /// The step under `x`, among the pattern's steps.
    public func step(atX x: CGFloat) -> Int {
        min(max(0, Int((beat(atX: x) / grid).rounded(.down))), max(0, steps - 1))
    }

    /// The line of the grid nearest `x`, from 0 at the pattern's start to its
    /// number of steps at its end.
    public func line(atX x: CGFloat) -> Int {
        min(max(0, Int((beat(atX: x) / grid).rounded())), steps)
    }

    /// The row at a height of the view.
    public func row(atY y: CGFloat) -> Int? {
        let at = y - Self.rulerHeight + scroll.y
        return rows.firstIndex { at >= $0.top && at < $0.top + $0.height }
    }

    /// The top of a row in the view, with its height.
    public func frame(ofRow index: Int) -> CGRect {
        let row = rows[index]
        return CGRect(x: Self.gutter, y: y(row.top), width: lanesWidth, height: row.height)
    }

    /// The note at a height of a row of notes.
    public func pitch(atY y: CGFloat, row index: Int) -> Int? {
        let row = rows[index]
        guard case .notes(let low, let high) = row.kind else { return nil }
        let down = Int(((y - self.y(row.top)) / row.semitone).rounded(.down))
        return min(max(high - down, low), high)
    }

    /// Where a step of a row is.
    public func cell(row index: Int, step: Int) -> CGRect {
        let row = rows[index]
        let left = x(Double(step) * grid)
        return CGRect(x: left, y: y(row.top), width: CGFloat(grid) * pixelsPerBeat, height: row.height)
    }

    /// Where an event of a row is: as long as it is held, or a short mark, and
    /// in a row of notes at its note, or at the root when it has none.
    public func event(row index: Int, at: Double, duration: Double?, pitch: Int?) -> CGRect {
        let row = rows[index]
        let width = duration.map { max(4, CGFloat($0) * pixelsPerBeat) }
            ?? min(max(4, CGFloat(grid) * pixelsPerBeat), 14)
        switch row.kind {
        case .notes(let low, let high):
            let note = min(max(pitch ?? row.pad.root ?? low, low), high)
            let top = y(row.top) + CGFloat(high - note) * row.semitone
            return CGRect(x: x(at), y: top, width: width, height: row.semitone)
        case .steps, .hits:
            return CGRect(x: x(at), y: y(row.top) + 4, width: width, height: row.height - 8)
        }
    }

    /// The zoom at which the whole pattern fits the rows.
    public var fitPixelsPerBeat: CGFloat {
        guard lanesWidth > 0, lengthBeats > 0 else { return pixelsPerBeat }
        return lanesWidth / CGFloat(lengthBeats)
    }

    public var minPixelsPerBeat: CGFloat {
        min(fitPixelsPerBeat, Self.maxPixelsPerBeat)
    }

    /// The zoom a pattern opens with: all of it, unless its steps would be
    /// too narrow to click, and no wider than it needs.
    public mutating func fit() {
        let readable = Self.minStep / CGFloat(grid)
        pixelsPerBeat = min(max(fitPixelsPerBeat, readable), Self.maxPixelsPerBeat)
        scroll = .zero
        clamp()
    }

    public mutating func clamp() {
        pixelsPerBeat = min(max(pixelsPerBeat, minPixelsPerBeat), Self.maxPixelsPerBeat)
        scroll.x = min(max(0, scroll.x), max(0, contentWidth - lanesWidth))
        scroll.y = min(max(0, scroll.y), max(0, contentHeight - lanesHeight))
    }

    /// Zooms by `factor`, keeping the beat under `anchorX` where it is.
    public mutating func zoom(by factor: CGFloat, anchorX: CGFloat) {
        let anchor = beat(atX: anchorX)
        pixelsPerBeat = min(max(pixelsPerBeat * factor, minPixelsPerBeat), Self.maxPixelsPerBeat)
        scroll.x = CGFloat(anchor) * pixelsPerBeat - (anchorX - Self.gutter)
        clamp()
    }

    /// Scrolls so that `beat` is in view, as when the playhead runs off the right.
    public mutating func reveal(_ beat: Double) {
        let at = x(beat)
        if at < Self.gutter || at > size.width - 12 {
            scroll.x = CGFloat(beat) * pixelsPerBeat - lanesWidth * 0.1
            clamp()
        }
    }

    /// How far a drag of `dx` points moves an event, in steps of the grid.
    public func steps(forDrag dx: CGFloat) -> Int {
        Int((Double(dx / pixelsPerBeat) / grid).rounded())
    }

    /// How many steps of the grid events that start at `ats` can move
    /// together and each still start inside the pattern: by whole steps, so
    /// that one off the grid stays as far off it.
    public func stepRange(of ats: [Double]) -> ClosedRange<Int> {
        let least = ats.map { -Int(($0 / grid + 1e-9).rounded(.down)) }.max() ?? 0
        let most = ats.map { Int(((lengthBeats - $0) / grid - 1e-9).rounded(.up)) - 1 }.min() ?? 0
        return least...max(least, most)
    }

    /// How many beats events that start at `ats` can move together off the
    /// grid and each still start inside the pattern.
    public func beatRange(of ats: [Double]) -> ClosedRange<Double> {
        let least = -(ats.min() ?? 0)
        return least...max(least, lengthBeats - (ats.max() ?? 0) - 0.001)
    }

    /// How many notes events of rows of notes can move up or down together,
    /// each from its pitch, and stay inside its row: `pitched` pairs each
    /// one's row with its pitch. Nothing when none has a pitch.
    public func semitoneRange(of pitched: [(row: Int, pitch: Int)]) -> ClosedRange<Int> {
        var least = Int.min
        var most = Int.max
        for (index, pitch) in pitched where rows.indices.contains(index) {
            guard case .notes(let low, let high) = rows[index].kind else { continue }
            least = max(least, low - pitch)
            most = min(most, high - pitch)
        }
        guard least != Int.min, least <= 0, most >= 0 else { return 0...0 }
        return least...most
    }

    /// The velocity a step's level plays at: 1 to 9 from soft to hard, and 10,
    /// an `x`, at 100.
    public static func velocity(ofLevel level: Int) -> Int {
        level >= 10 ? 100 : [0, 14, 28, 42, 56, 71, 85, 99, 113, 127][max(0, level)]
    }

    /// The level a drag up or down takes a step to, from 1 to 9. An `x` is
    /// between 7 and 8, and goes from 7.
    public static func level(from level: Int, draggedBy dy: CGFloat) -> Int {
        let from = level >= 10 ? 7 : level
        return min(max(from - Int((dy / 5).rounded()), 1), 9)
    }
}
