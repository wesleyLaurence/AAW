import CoreGraphics

/// Where beats and rows fall in the arrangement view: zoom, scroll and the
/// grid a click snaps to. Geometry only; the song itself lives in the host.
public struct TimelineLayout: Equatable {
    public static let headerWidth: CGFloat = 212
    /// The ruler's five strips: the loop brace, the sections, the person's
    /// markers, the bar numbers and the time.
    public static let loopStrip: CGFloat = 16
    public static let sectionStrip: CGFloat = 18
    public static let markerStrip: CGFloat = 16
    public static let barStrip: CGFloat = 18
    public static let timeStrip: CGFloat = 16
    public static let rulerHeight: CGFloat = loopStrip + sectionStrip + markerStrip + barStrip + timeStrip
    /// Where the marker strip starts, from the ruler's top.
    public static let markerTop: CGFloat = loopStrip + sectionStrip
    /// Where the bar numbers start: the start position's flag and the
    /// playhead's head are drawn from here.
    public static let barTop: CGFloat = markerTop + markerStrip
    /// Where the time starts: the strip nearest the lanes.
    public static let timeTop: CGFloat = barTop + barStrip
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
    /// The session tempo, in beats a minute: what makes a beat a time.
    public var tempo: Double = 120
    public var lengthBeats: Double = 16
    /// How far play has gone past the song's end, in beats, or zero: the
    /// view scrolls a screen past it as it does past the end, so that the
    /// playhead rolling on through empty bars stays in view.
    public var playedBeats: Double = 0
    /// The furthest marker's beat, or zero. A marker past the end lies there
    /// without moving the end (D164), and the view scrolls as far as it.
    public var markersEnd: Double = 0
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
    public var contentWidth: CGFloat { CGFloat(max(lengthBeats, playedBeats, markersEnd) + tailBeats) * pixelsPerBeat }

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

    // MARK: Time

    /// Points a second at this zoom and tempo.
    public var pixelsPerSecond: CGFloat { pixelsPerBeat * CGFloat(tempo) / 60 }

    /// Where a time from the song's start falls, in the view.
    public func x(seconds: Double) -> CGFloat { x(seconds * tempo / 60) }

    /// The time from the song's start at `x`, in seconds.
    public func seconds(atX x: CGFloat) -> Double { beat(atX: x) * 60 / tempo }

    /// The spacings a time label may have, in seconds: tenths and fifths of
    /// a second to an hour, as a clock is read. Past an hour the step doubles.
    public static let timeSteps: [Double] = [0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600]
    /// The spacings a tick between the labels may have, in seconds: finer
    /// than the labels' by hundredths of a second.
    public static let timeTickSteps: [Double] = [0.01, 0.02, 0.05] + timeSteps
    /// The least distance between time labels, and between the ticks under
    /// them, in points.
    public static let timeLabelSpacing: CGFloat = 56
    public static let timeTickSpacing: CGFloat = 10

    /// Seconds between the time labels: the finest of the clock's spacings
    /// whose labels are at least `timeLabelSpacing` apart, so that zoomed in
    /// the ruler reads in tenths of a second and zoomed out in minutes.
    public var timeLabelStep: Double {
        guard pixelsPerSecond > 0 else { return Self.timeSteps.last! }
        for step in Self.timeSteps where CGFloat(step) * pixelsPerSecond >= Self.timeLabelSpacing {
            return step
        }
        var step = Self.timeSteps.last!
        while CGFloat(step) * pixelsPerSecond < Self.timeLabelSpacing { step *= 2 }
        return step
    }

    /// Seconds between the ticks under the time labels: the finest spacing
    /// that divides the labels' whole and keeps the ticks `timeTickSpacing`
    /// apart, else the labels' own.
    public var timeTickStep: Double {
        let label = timeLabelStep
        for step in Self.timeTickSteps where step < label && CGFloat(step) * pixelsPerSecond >= Self.timeTickSpacing {
            let parts = label / step
            if abs(parts - parts.rounded()) < 1e-9 { return step }
        }
        return label
    }

    /// A time from the song's start as the ruler writes it: minutes and
    /// seconds, hours before them past an hour, and after the seconds the
    /// tenths when the labels are under a second apart and the hundredths
    /// under a tenth.
    public static func clock(_ seconds: Double, step: Double) -> String {
        let places = step < 0.1 ? 2 : step < 1 ? 1 : 0
        let unit = pow(10.0, Double(places))
        let total = Int((max(0, seconds) * unit).rounded())
        let whole = total / Int(unit)
        let fraction = total % Int(unit)
        let (h, m, s) = (whole / 3600, whole % 3600 / 60, whole % 60)
        var text = h > 0 ? String(format: "%d:%02d:%02d", h, m, s) : String(format: "%d:%02d", m, s)
        if places > 0 { text += "." + String(format: "%0\(places)d", fraction) }
        return text
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

    // MARK: Bars, beats and sixteenths in the ruler

    /// The bar strip's steps, in beats.
    public struct RulerSteps: Equatable {
        /// Between the labels.
        public var label: Double
        /// Between the ticks, which divide the labels'.
        public var tick: Double
        /// What a label's third number counts: a sixteenth, or on a triplet
        /// grid a third or a sixth of the meter's beat.
        public var count: Double
    }

    /// The least distance between the bar strip's labels, and between its
    /// ticks, in points. A label is also kept wider than its text.
    public static let positionLabelSpacing: CGFloat = 36
    public static let positionTickSpacing: CGFloat = 6
    /// A digit or a point of a label's text, about, at the ruler's size.
    public static let labelCharacterWidth: CGFloat = 6.2

    /// Whether `b` divides `a` a whole number of times, within a hair.
    static func divides(_ a: Double, _ b: Double) -> Bool {
        guard b > 0 else { return false }
        let q = a / b
        return abs(q - q.rounded()) < 1e-6
    }

    /// Whether a grid in beats is a triplet one: a third of a power of two
    /// of a beat, as 1/8T is a third of a beat and 1/4T two thirds.
    public static func isTriplet(_ beats: Double) -> Bool {
        func dyadic(_ v: Double) -> Bool {
            var x = v
            for _ in 0..<12 {
                if abs(x - x.rounded()) < 1e-6 { return true }
                x *= 2
            }
            return false
        }
        return beats > 0 && !dyadic(beats) && dyadic(beats * 3)
    }

    /// The steps the bar strip labels and ticks at this zoom: from bars,
    /// 4, 2 and 1 of them and more zoomed out, down to the meter's beat, an
    /// eighth and a sixteenth, or on a triplet grid down to thirds and sixths
    /// of the beat. Labels take the finest whose labels are
    /// `positionLabelSpacing` apart and wider than the text of the last bar's,
    /// `lastBeat` being the last the ruler shows; ticks the finest that
    /// divides it with ticks `positionTickSpacing` apart.
    public static func rulerSteps(pixelsPerBeat: CGFloat, beatsPerBar: Double, beatUnit: Double,
                                  grid: Double, lastBeat: Double) -> RulerSteps {
        let bar = beatsPerBar > 0 ? beatsPerBar : 4
        let unit = beatUnit > 0 ? beatUnit : 1
        let triplet = isTriplet(grid)
        let count = triplet ? (grid < unit / 3 - 1e-9 ? unit / 6 : unit / 3) : 0.25
        var within: [Double] = []
        if triplet {
            within = [unit / 6, unit / 3].filter { $0 >= count - 1e-9 }
        } else {
            var step = 0.25
            while step < unit - 1e-9 {
                if divides(unit, step) { within.append(step) }
                step *= 2
            }
        }
        within.append(unit)
        within = within.filter { $0 < bar - 1e-9 && divides(bar, $0) }
        guard pixelsPerBeat > 0 else { return RulerSteps(label: bar, tick: bar, count: count) }

        let digits = { (n: Int) in String(max(1, n)).count }
        let bars = Int((max(lastBeat, bar) / bar).rounded(.up))
        let beats = Int((bar / unit).rounded(.up))
        let counts = Int((unit / count).rounded())
        func wide(_ step: Double) -> CGFloat {
            var characters = digits(bars)
            if step < bar - 1e-9 { characters += 1 + digits(beats) }
            if step < unit - 1e-9 { characters += 1 + digits(counts) }
            return CGFloat(characters) * labelCharacterWidth + 8
        }
        func fits(_ step: Double) -> Bool {
            CGFloat(step) * pixelsPerBeat >= max(positionLabelSpacing, wide(step))
        }
        var label = within.first(where: fits) ?? bar
        if label >= bar { while !fits(label) { label *= 2 } }

        var tick = label
        var ladder = within
        var step = bar
        while step < label - 1e-9 { ladder.append(step); step *= 2 }
        for rung in ladder where rung < label - 1e-9 && CGFloat(rung) * pixelsPerBeat >= positionTickSpacing && divides(label, rung) {
            tick = rung
            break
        }
        return RulerSteps(label: label, tick: tick, count: count)
    }

    /// The bar strip's steps at this zoom, for the song and the view past its end.
    public var rulerSteps: RulerSteps {
        Self.rulerSteps(pixelsPerBeat: pixelsPerBeat, beatsPerBar: beatsPerBar, beatUnit: beatUnit, grid: grid,
                        lastBeat: max(lengthBeats, playedBeats, markersEnd) + tailBeats)
    }

    /// A place as the ruler labels it, each part counted from one: the bar
    /// on a bar, `bar.beat` on a beat and `bar.beat.count` between, the beat
    /// as the meter counts it and `count` beats each step of the third.
    /// Before the song's start, as before a clip's, the bar is 0 and less.
    public static func rulerLabel(_ beat: Double, beatsPerBar: Double, beatUnit: Double, count: Double) -> String {
        let barIndex = (beat / beatsPerBar + 1e-6).rounded(.down)
        let inBar = max(0, beat - barIndex * beatsPerBar)
        let bar = Int(barIndex) + 1
        if inBar < 1e-6 { return "\(bar)" }
        let beatIndex = (inBar / beatUnit + 1e-6).rounded(.down)
        let inBeat = max(0, inBar - beatIndex * beatUnit)
        if inBeat < 1e-6 { return "\(bar).\(Int(beatIndex) + 1)" }
        return "\(bar).\(Int(beatIndex) + 1).\(Int((inBeat / count).rounded()) + 1)"
    }

    /// The beat a click in the bar strip means: on the grid when it falls on
    /// the labels, else on the ticks, so that a label can be clicked whatever
    /// the grid; with `free` to a thousandth; not before the song.
    public func rulerTarget(atX x: CGFloat, free: Bool) -> Double {
        let raw = beat(atX: x)
        if free { return max(0, (raw * 1000).rounded() / 1000) }
        let steps = rulerSteps
        let g = Self.divides(steps.label, grid) ? grid : steps.tick
        return max(0, (raw / g).rounded() * g)
    }

    /// The beat a click at `x` means: on the grid unless `free`, and not
    /// before the song. It may be past the end, where play rolls on through
    /// empty bars.
    public func target(atX x: CGFloat, free: Bool) -> Double {
        let raw = beat(atX: x)
        let g = grid
        let beat = free ? (raw * 1000).rounded() / 1000 : (raw / g).rounded() * g
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
