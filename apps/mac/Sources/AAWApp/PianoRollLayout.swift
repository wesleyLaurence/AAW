import CoreGraphics
import Foundation

/// Where a note clip's notes fall in the piano roll: time left to right in
/// beats of the clip, from its start, and every MIDI note from 127 at the top
/// to 0 at the bottom, whatever the instrument. The view runs from the
/// clip's start, or the earliest note before it, to its end, or the latest
/// note past it. Geometry only; the notes live in the host.
public struct PianoRollLayout: Equatable {
    /// The keys and the notes' names, left of the rows.
    public static let gutter: CGFloat = 96
    public static let rulerHeight: CGFloat = 16
    /// The height of a note's row.
    public static let semitone: CGFloat = 10
    public static let notes = 128
    public static let maxPixelsPerBeat: CGFloat = 480
    /// The least width of a step at the zoom a clip opens with.
    public static let minStep: CGFloat = 9
    /// How near a note's end a press stretches it, in points.
    public static let endGrip: CGFloat = 5

    public var size: CGSize = .zero
    /// The clip's length in beats.
    public var lengthBeats: Double = 4
    /// The beats shown: from the first, which is 0 or a note's before the
    /// clip, to the last, which is the clip's end or a note's past it.
    public var first: Double = 0
    public var last: Double = 4
    /// The length of a step, in beats.
    public var grid: Double = 0.25
    public var pixelsPerBeat: CGFloat = 72
    public var scroll: CGPoint = .zero

    public init() {}

    public var lanesWidth: CGFloat { max(0, size.width - Self.gutter) }
    public var lanesHeight: CGFloat { max(0, size.height - Self.rulerHeight) }
    public var contentWidth: CGFloat { CGFloat(last - first) * pixelsPerBeat }
    public var contentHeight: CGFloat { CGFloat(Self.notes) * Self.semitone }

    /// Takes the clip's length and the span of its notes, in beats of the
    /// clip: the view takes in what lies outside the clip, a beat at a time.
    public mutating func setSpan(length: Double, notes: [(at: Double, end: Double)]) {
        lengthBeats = length
        first = min(0, (notes.map(\.at).min() ?? 0).rounded(.down))
        last = max(length, (notes.map(\.end).max() ?? 0).rounded(.up))
        clamp()
    }

    public func x(_ beat: Double) -> CGFloat {
        Self.gutter + CGFloat(beat - first) * pixelsPerBeat - scroll.x
    }

    public func beat(atX x: CGFloat) -> Double {
        Double((x - Self.gutter + scroll.x) / pixelsPerBeat) + first
    }

    /// The top of a note's row in the view.
    public func y(_ pitch: Int) -> CGFloat {
        Self.rulerHeight + CGFloat(Self.notes - 1 - pitch) * Self.semitone - scroll.y
    }

    /// The note whose row is at a height of the view, within 0 to 127.
    public func pitch(atY y: CGFloat) -> Int {
        let down = Int(((y - Self.rulerHeight + scroll.y) / Self.semitone).rounded(.down))
        return min(max(Self.notes - 1 - down, 0), Self.notes - 1)
    }

    /// Where a note is drawn: as long as it lasts, and at least a few points.
    public func rect(at: Double, duration: Double, pitch: Int) -> CGRect {
        CGRect(x: x(at), y: y(pitch), width: max(4, CGFloat(duration) * pixelsPerBeat), height: Self.semitone)
    }

    /// Whether a point on a note's rect is on its end, where a drag stretches it.
    public static func onEnd(_ p: CGPoint, of rect: CGRect) -> Bool {
        rect.width >= 10 && p.x >= rect.maxX - endGrip
    }

    /// The start of the step a beat is in, on the grid.
    public func step(at beat: Double) -> Double {
        (beat / grid + 1e-9).rounded(.down) * grid
    }

    /// The line of the grid nearest a beat.
    public func line(at beat: Double) -> Double {
        (beat / grid).rounded() * grid
    }

    /// How far a drag of `dx` points moves notes, in steps of the grid.
    public func steps(forDrag dx: CGFloat) -> Int {
        Int((Double(dx / pixelsPerBeat) / grid).rounded())
    }

    /// How far a drag of `dx` points moves notes off the grid, in beats to a
    /// thousandth.
    public func beats(forDrag dx: CGFloat) -> Double {
        (Double(dx / pixelsPerBeat) * 1000).rounded() / 1000
    }

    /// How many notes up a drag of `dy` points moves notes.
    public func semitones(forDrag dy: CGFloat) -> Int {
        -Int((dy / Self.semitone).rounded())
    }

    /// The zoom at which the whole clip fits the width.
    public var fitPixelsPerBeat: CGFloat {
        guard lanesWidth > 0, lengthBeats > 0 else { return pixelsPerBeat }
        return lanesWidth / CGFloat(lengthBeats)
    }

    public var minPixelsPerBeat: CGFloat {
        guard lanesWidth > 0, last > first else { return pixelsPerBeat }
        return min(lanesWidth / CGFloat(last - first), Self.maxPixelsPerBeat)
    }

    /// The zoom a clip opens with: all of it, unless its steps would be too
    /// narrow to click, from its start, with its notes in the middle of the
    /// height, or middle C's octave when it has none.
    public mutating func fit(pitches: [Int]) {
        let readable = Self.minStep / CGFloat(grid)
        pixelsPerBeat = min(max(fitPixelsPerBeat, readable), Self.maxPixelsPerBeat)
        scroll.x = CGFloat(-first) * pixelsPerBeat
        let middle = pitches.isEmpty ? 66 : Double(pitches.min()! + pitches.max()!) / 2
        scroll.y = CGFloat(Double(Self.notes) - 0.5 - middle) * Self.semitone - lanesHeight / 2
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
        scroll.x = CGFloat(anchor - first) * pixelsPerBeat - (anchorX - Self.gutter)
        clamp()
    }

    /// Scrolls so that `beat` is in view, as when the playhead runs off the right.
    public mutating func reveal(_ beat: Double) {
        let at = x(beat)
        if at < Self.gutter || at > size.width - 12 {
            scroll.x = CGFloat(beat - first) * pixelsPerBeat - lanesWidth * 0.1
            clamp()
        }
    }

    /// Scrolls up or down so that a note's row is in view.
    public mutating func reveal(pitch: Int) {
        let top = y(pitch)
        if top < Self.rulerHeight {
            scroll.y -= Self.rulerHeight - top
        } else if top + Self.semitone > size.height {
            scroll.y += top + Self.semitone - size.height
        }
        clamp()
    }

    /// A beat as the song writes it, `1/4` or `0.5`, as a number.
    public static func beats(_ text: String) -> Double? {
        let parts = text.split(separator: "/").map { Double($0.trimmingCharacters(in: .whitespaces)) }
        switch parts.count {
        case 1: return parts[0]
        case 2: if let n = parts[0], let d = parts[1], d != 0 { return n / d }
        default: break
        }
        return nil
    }

    /// Whether a note is one of a piano's black keys.
    public static func black(_ pitch: Int) -> Bool {
        [1, 3, 6, 8, 10].contains(((pitch % 12) + 12) % 12)
    }
}
