import CoreGraphics

/// Where beats and rows fall in the arrangement view: zoom, scroll and the
/// grid a click snaps to. Geometry only; the song itself lives in the host.
public struct TimelineLayout: Equatable {
    public static let headerWidth: CGFloat = 212
    /// The ruler's four strips: the loop brace, the sections, the person's
    /// markers and the bar numbers.
    public static let loopStrip: CGFloat = 16
    public static let sectionStrip: CGFloat = 18
    public static let markerStrip: CGFloat = 16
    public static let barStrip: CGFloat = 18
    public static let rulerHeight: CGFloat = loopStrip + sectionStrip + markerStrip + barStrip
    /// Where the marker strip starts, from the ruler's top.
    public static let markerTop: CGFloat = loopStrip + sectionStrip
    /// A marker's flag, which it has with or without text, and the space
    /// either side of the text in it.
    public static let markerFlag: CGFloat = 9
    public static let markerPad: CGFloat = 4
    public static let trackHeight: CGFloat = 46
    /// Returns and the master.
    public static let busHeight: CGFloat = 32
    /// The gap between the tracks and the returns.
    public static let busGap: CGFloat = 10
    public static let maxPixelsPerBeat: CGFloat = 240
    /// How far in from a clip's edge a press resizes or trims it.
    public static let resizeEdge: CGFloat = 6

    public var pixelsPerBeat: CGFloat = 16
    public var scroll: CGPoint = .zero
    public var size: CGSize = .zero
    /// A bar in beats as the song writes them: 3 in 3/4 and 6/8, 3.5 in 7/8.
    public var beatsPerBar: Double = 4
    /// The note the time signature counts, in those beats: 1 in 3/4, 0.5 in
    /// 6/8. The zoom's grid and the brighter lines are at its multiples.
    public var beatUnit: Double = 1
    public var lengthBeats: Double = 16
    /// The height of all rows.
    public var contentHeight: CGFloat = 0

    public init() {}

    public var lanesWidth: CGFloat { max(0, size.width - Self.headerWidth) }
    public var lanesHeight: CGFloat { max(0, size.height - Self.rulerHeight) }

    /// Beats the view scrolls past the end: a screen of them, and two bars
    /// at the least, so that a clip is dropped or dragged past the end, which
    /// moves to hold it (D130).
    public var tailBeats: Double {
        guard pixelsPerBeat > 0 else { return 2 * beatsPerBar }
        return max(2 * beatsPerBar, Double(lanesWidth / pixelsPerBeat))
    }
    public var contentWidth: CGFloat { CGFloat(lengthBeats + tailBeats) * pixelsPerBeat }

    public func x(_ beat: Double) -> CGFloat {
        Self.headerWidth + CGFloat(beat) * pixelsPerBeat - scroll.x
    }

    /// Where the pointer becomes the resize cursor over a clip drawn in
    /// `rect`: its start when a drag trims it, and its end, each a strip
    /// `resizeEdge` wide; none for a clip too narrow for its parts, which
    /// only moves.
    public static func edgeRects(of rect: CGRect, start: Bool, end: Bool) -> [CGRect] {
        guard rect.width >= 16 else { return [] }
        var rects: [CGRect] = []
        if start { rects.append(CGRect(x: rect.minX, y: rect.minY, width: resizeEdge, height: rect.height)) }
        if end { rects.append(CGRect(x: rect.maxX - resizeEdge, y: rect.minY, width: resizeEdge, height: rect.height)) }
        return rects
    }

    public func beat(atX x: CGFloat) -> Double {
        Double((x - Self.headerWidth + scroll.x) / pixelsPerBeat)
    }

    /// The view's y for a y among the rows.
    public func y(_ contentY: CGFloat) -> CGFloat {
        Self.rulerHeight + contentY - scroll.y
    }

    /// The zoom at which the whole song and one bar more fit the lanes.
    public var fitPixelsPerBeat: CGFloat {
        guard lanesWidth > 0 else { return pixelsPerBeat }
        return lanesWidth / CGFloat(lengthBeats + beatsPerBar)
    }

    public var minPixelsPerBeat: CGFloat { min(fitPixelsPerBeat / 2, Self.maxPixelsPerBeat) }

    /// The least distance between grid lines, in points.
    public static let gridSpacing: CGFloat = 14

    /// The grid the zoom allows, in beats: the finest of a quarter of the
    /// meter's beat, half of it, the beat, bars and multiples of bars whose
    /// lines are at least `gridSpacing` apart.
    public var zoomGrid: Double {
        for step in [beatUnit / 4, beatUnit / 2, beatUnit] where step < beatsPerBar && CGFloat(step) * pixelsPerBeat >= Self.gridSpacing {
            return step
        }
        var step = beatsPerBar
        while CGFloat(step) * pixelsPerBeat < Self.gridSpacing { step *= 2 }
        return step
    }

    /// A grid chosen in the Grid menu, in beats, in place of the zoom's; nil
    /// follows the zoom.
    public var fixedGrid: Double?

    /// The grid clicks and drags snap to: the chosen one, or the zoom's.
    public var grid: Double {
        if let fixedGrid, fixedGrid > 0 { return fixedGrid }
        return zoomGrid
    }

    /// The grid whose lines are drawn: the grid, or when its lines would be
    /// closer than `gridSpacing`, every second, fourth or eighth line of it
    /// while those still fall on the bars, and else the zoom's.
    public var drawnGrid: Double {
        var step = grid
        while CGFloat(step) * pixelsPerBeat < Self.gridSpacing { step *= 2 }
        let perBar = beatsPerBar / step
        let bars = step / beatsPerBar
        let onBars = abs(perBar - perBar.rounded()) < 1e-9 || abs(bars - bars.rounded()) < 1e-9
        return onBars ? step : zoomGrid
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
    /// there to a thousandth, not before the song: where a new clip goes. It
    /// may be past the song's end, which moves to hold the clip.
    public func step(atX x: CGFloat, free: Bool) -> Double {
        let raw = beat(atX: x)
        let g = grid
        let beat = free ? (raw * 1000).rounded() / 1000 : (raw / g).rounded(.down) * g
        return max(0, beat)
    }

    /// A beat on the grid, or with `free` to a thousandth of a beat, not
    /// before the song's start: where an automation point can be. Past the
    /// end, the end moves to it.
    public func snapped(_ beat: Double, free: Bool) -> Double {
        let g = grid
        let on = free ? (beat * 1000).rounded() / 1000 : (beat / g).rounded() * g
        return max(on, 0)
    }

    /// Where each marker's flag is drawn and pressed, from its line: as wide
    /// as its text with the space around it, no further than the next
    /// marker's line, and never less than the flag. `xs` are the markers'
    /// lines in time order and `textWidths` what each one's text takes.
    public static func markerSpans(xs: [CGFloat], textWidths: [CGFloat]) -> [ClosedRange<CGFloat>] {
        xs.indices.map { i in
            let wanted = textWidths[i] > 0 ? textWidths[i] + 2 * markerPad : markerFlag
            let room = i + 1 < xs.count ? xs[i + 1] - xs[i] - 1 : .infinity
            return xs[i]...(xs[i] + max(markerFlag, min(wanted, room)))
        }
    }

    /// The marker a press at `x` takes hold of: the one whose flag holds it,
    /// the later of two that share a place, so that each can be reached.
    public static func marker(atX x: CGFloat, spans: [ClosedRange<CGFloat>]) -> Int? {
        spans.lastIndex { $0.contains(x) }
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
    /// than the nearest point of its lane that stays, nor before the song;
    /// the last point of a lane may go as far past the end as it likes, and
    /// the end moves to it. `lanes` holds each lane's points in time order,
    /// with whether each one moves.
    public static func pointRange(_ lanes: [[(at: Double, moving: Bool)]]) -> ClosedRange<Double> {
        var least = -Double.infinity
        var most = Double.infinity
        for points in lanes {
            for (index, point) in points.enumerated() where point.moving {
                let before = points[..<index].last { !$0.moving }?.at ?? 0
                let after = points[(index + 1)...].first { !$0.moving }?.at ?? .infinity
                least = max(least, before - point.at)
                most = min(most, after - point.at)
            }
        }
        guard least.isFinite, least <= 0, most >= 0 else { return 0...0 }
        return least...most
    }

    /// The line of an automation segment from one point to the next, as the
    /// places it passes after the first: the next point alone when it is
    /// straight, else the curve it plays, bent between the two as the engine
    /// bends it in the parameter's domain, which is the lane's height.
    public static func curve(from a: CGPoint, to b: CGPoint, shape: Double) -> [CGPoint] {
        guard shape != 0, b.x - a.x >= 2 else { return [b] }
        let steps = max(8, min(64, Int((b.x - a.x) / 3)))
        return (1...steps).map { i in
            let t = Double(i) / Double(steps)
            let along = CGFloat(shapedProgress(t, shape: shape))
            return CGPoint(x: a.x + (b.x - a.x) * CGFloat(t), y: a.y + (b.y - a.y) * along)
        }
    }

    /// How far a place is from a line through points, in points.
    public static func distance(_ p: CGPoint, toLineThrough points: [CGPoint]) -> CGFloat {
        var least = CGFloat.infinity
        for (a, b) in zip(points, points.dropFirst()) {
            let (dx, dy) = (b.x - a.x, b.y - a.y)
            let length = dx * dx + dy * dy
            let t = length > 0 ? min(max(((p.x - a.x) * dx + (p.y - a.y) * dy) / length, 0), 1) : 0
            least = min(least, hypot(p.x - (a.x + dx * t), p.y - (a.y + dy * t)))
        }
        return least
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

    /// The repeats that bring the end of a clip nearest `x`: at least one.
    /// Past the song's end, the end moves to hold them.
    public func repeats(atX x: CGFloat, clipAt at: Double, patternBeats: Double) -> Int {
        guard patternBeats > 0 else { return 1 }
        let wanted = ((beat(atX: x) - at) / patternBeats).rounded()
        return Int(max(1, wanted))
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

    /// A position as bars, beats and sixteenths, each counted from one: the
    /// beat as the time signature counts it, `beatUnit` quarter notes long.
    public static func position(_ beat: Double, beatsPerBar: Double, beatUnit: Double = 1) -> String {
        let b = max(0, beat) + 1e-9
        let bar = Int(b / beatsPerBar)
        let inBar = b - Double(bar) * beatsPerBar
        let whole = Int(inBar / beatUnit)
        let sixteenth = Int((inBar - Double(whole) * beatUnit) * 4)
        return "\(bar + 1).\(whole + 1).\(sixteenth + 1)"
    }
}
