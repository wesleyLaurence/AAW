import CoreGraphics

/// Where beats and rows fall in the arrangement view: zoom, scroll and the
/// grid a click snaps to. Geometry only; the song itself lives in the host.
public struct TimelineLayout: Equatable {
    public static let headerWidth: CGFloat = 212
    /// The ruler's three strips: loop brace, section markers, bar numbers.
    public static let loopStrip: CGFloat = 16
    public static let sectionStrip: CGFloat = 18
    public static let barStrip: CGFloat = 18
    public static let rulerHeight: CGFloat = loopStrip + sectionStrip + barStrip
    public static let trackHeight: CGFloat = 46
    /// Returns and the master.
    public static let busHeight: CGFloat = 32
    /// The gap between the tracks and the returns.
    public static let busGap: CGFloat = 10
    public static let maxPixelsPerBeat: CGFloat = 240

    public var pixelsPerBeat: CGFloat = 16
    public var scroll: CGPoint = .zero
    public var size: CGSize = .zero
    public var beatsPerBar: Double = 4
    public var lengthBeats: Double = 16
    /// The height of all rows.
    public var contentHeight: CGFloat = 0

    public init() {}

    public var lanesWidth: CGFloat { max(0, size.width - Self.headerWidth) }
    public var lanesHeight: CGFloat { max(0, size.height - Self.rulerHeight) }

    /// Beats shown past the end, so the last bar is not against the edge.
    public var tailBeats: Double { beatsPerBar }
    public var contentWidth: CGFloat { CGFloat(lengthBeats + tailBeats) * pixelsPerBeat }

    public func x(_ beat: Double) -> CGFloat {
        Self.headerWidth + CGFloat(beat) * pixelsPerBeat - scroll.x
    }

    public func beat(atX x: CGFloat) -> Double {
        Double((x - Self.headerWidth + scroll.x) / pixelsPerBeat)
    }

    /// The view's y for a y among the rows.
    public func y(_ contentY: CGFloat) -> CGFloat {
        Self.rulerHeight + contentY - scroll.y
    }

    /// The zoom at which the whole song fits the lanes.
    public var fitPixelsPerBeat: CGFloat {
        guard lanesWidth > 0 else { return pixelsPerBeat }
        return lanesWidth / CGFloat(lengthBeats + tailBeats)
    }

    public var minPixelsPerBeat: CGFloat { min(fitPixelsPerBeat / 2, Self.maxPixelsPerBeat) }

    /// The least distance between grid lines, in points.
    public static let gridSpacing: CGFloat = 14

    /// The grid in beats, which is drawn and which clicks snap to: the finest
    /// of sixteenths, eighths, beats, bars and multiples of bars whose lines
    /// are at least `gridSpacing` apart.
    public var grid: Double {
        for step in [0.25, 0.5, 1.0] where step < beatsPerBar && CGFloat(step) * pixelsPerBeat >= Self.gridSpacing {
            return step
        }
        var step = beatsPerBar
        while CGFloat(step) * pixelsPerBeat < Self.gridSpacing { step *= 2 }
        return step
    }

    /// Bars between bar numbers, so that the numbers do not crowd.
    public var barLabelStep: Int {
        var bars = 1
        while CGFloat(Double(bars) * beatsPerBar) * pixelsPerBeat < 44 { bars *= 2 }
        return bars
    }

    /// The beat a click at `x` means: on the grid unless `free`, and inside
    /// the song, where a start position has to be.
    public func target(atX x: CGFloat, free: Bool) -> Double {
        let raw = beat(atX: x)
        let g = grid
        var beat = free ? (raw * 1000).rounded() / 1000 : (raw / g).rounded() * g
        let last = free ? lengthBeats - 0.001 : ((lengthBeats / g).rounded(.up) - 1) * g
        beat = min(beat, last)
        return max(0, beat)
    }

    /// The start of the grid step that holds `x`, or with `free` the beat
    /// there to a thousandth, inside the song: where a new clip goes.
    public func step(atX x: CGFloat, free: Bool) -> Double {
        let raw = beat(atX: x)
        let g = grid
        let beat = free ? (raw * 1000).rounded() / 1000 : (raw / g).rounded(.down) * g
        let last = free ? lengthBeats - 0.001 : ((lengthBeats / g).rounded(.up) - 1) * g
        return max(0, min(beat, last))
    }

    /// A beat on the grid, or with `free` to a thousandth of a beat, from the
    /// song's start to its end: where an automation point can be.
    public func snapped(_ beat: Double, free: Bool) -> Double {
        let g = grid
        let on = free ? (beat * 1000).rounded() / 1000 : (beat / g).rounded() * g
        return min(max(on, 0), lengthBeats)
    }

    /// A loop over the dragged span: on the grid, at least one grid step long
    /// and inside the song. Nil when the span is outside the song.
    public func loop(fromX a: CGFloat, toX b: CGFloat) -> (start: Double, length: Double)? {
        let g = grid
        var start = (beat(atX: min(a, b)) / g).rounded(.down) * g
        var end = (beat(atX: max(a, b)) / g).rounded(.up) * g
        start = max(0, start)
        end = min(lengthBeats, end)
        if end - start < g { end = min(lengthBeats, start + g) }
        guard end > start else { return nil }
        return (start, end - start)
    }

    /// How far a drag of `dx` points moves clips, in beats: whole grid steps,
    /// or with `free` thousandths of a beat, and no further than `range`, which
    /// is how far the clips can move and stay in the song.
    /// How far automation points can move in time together: each no further
    /// than the nearest point of its lane that stays, nor before the song or
    /// past its end. `lanes` holds each lane's points in time order, with
    /// whether each one moves.
    public static func pointRange(_ lanes: [[(at: Double, moving: Bool)]], length: Double) -> ClosedRange<Double> {
        var least = -Double.infinity
        var most = Double.infinity
        for points in lanes {
            for (index, point) in points.enumerated() where point.moving {
                let before = points[..<index].last { !$0.moving }?.at ?? 0
                let after = points[(index + 1)...].first { !$0.moving }?.at ?? length
                least = max(least, before - point.at)
                most = min(most, after - point.at)
            }
        }
        guard least.isFinite, most.isFinite, least <= 0, most >= 0 else { return 0...0 }
        return least...most
    }

    /// How far up automation points can move together, as a fraction of
    /// their lanes' height, each staying inside its lane: `fractions` is how
    /// far up its lane each one is.
    public static func liftRange(_ fractions: [Double]) -> ClosedRange<Double> {
        guard let low = fractions.min(), let high = fractions.max() else { return 0...0 }
        return min(0, -low)...max(0, 1 - high)
    }

    public func move(byX dx: CGFloat, free: Bool, within range: ClosedRange<Double>) -> Double {
        let step = free ? 0.001 : grid
        let steps = (Double(dx / pixelsPerBeat) / step).rounded()
        let least = (range.lowerBound / step).rounded(.up)
        let most = (range.upperBound / step).rounded(.down)
        guard least <= most else { return 0 }
        return min(max(steps, least), most) * step
    }

    /// The repeats that bring the end of a clip nearest `x`: at least one, and
    /// no more than fit in the song.
    public func repeats(atX x: CGFloat, clipAt at: Double, patternBeats: Double) -> Int {
        guard patternBeats > 0 else { return 1 }
        let wanted = ((beat(atX: x) - at) / patternBeats).rounded()
        let fit = ((lengthBeats - at) / patternBeats + 1e-9).rounded(.down)
        return Int(max(1, min(wanted, fit)))
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
        scroll.x = CGFloat(anchor) * pixelsPerBeat - (anchorX - Self.headerWidth)
        clamp()
    }

    public mutating func fit() {
        pixelsPerBeat = fitPixelsPerBeat
        scroll.x = 0
        clamp()
    }

    /// Scrolls so that `beat` is in view, a little in from the left edge, as
    /// when the playhead runs off the right.
    public mutating func reveal(_ beat: Double) {
        let at = x(beat)
        if at < Self.headerWidth || at > size.width - 24 {
            scroll.x = CGFloat(beat) * pixelsPerBeat - lanesWidth * 0.1
            clamp()
        }
    }

    /// A position as bars, beats and sixteenths, each counted from one.
    public static func position(_ beat: Double, beatsPerBar: Double) -> String {
        let b = max(0, beat) + 1e-9
        let bar = Int(b / beatsPerBar)
        let inBar = b - Double(bar) * beatsPerBar
        let whole = Int(inBar)
        let sixteenth = Int((inBar - Double(whole)) * 4)
        return "\(bar + 1).\(whole + 1).\(sixteenth + 1)"
    }
}
