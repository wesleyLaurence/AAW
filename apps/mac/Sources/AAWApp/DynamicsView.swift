import AAWCore
import AppKit
import SwiftUI

/// A compressor's, a limiter's or a clipper's work drawn while the song
/// plays, in SwiftUI.
struct DynamicsDrawing: NSViewRepresentable {
    let model: SongModel
    let effect: EffectView
    let style: DynamicsView.Style

    func makeNSView(context: Context) -> DynamicsView {
        DynamicsView(model: model, effect: effect, style: style)
    }

    func updateNSView(_ view: DynamicsView, context: Context) {
        view.update(effect)
    }
}

/// What a compressor, a limiter or a clipper took off, read from the audio
/// thread each frame drawn while it shows. As a compressor's display: the
/// last seconds of the level reaching it, filled, the level leaving it over
/// that as a line, and the reduction hanging from the top, with the threshold as a
/// line dragged up and down, and a meter at the right. As a limiter's or a
/// clipper's meter: the level reaching it with its ceiling marked and the
/// reduction falling from the top beside it. As a folded device's strip: a
/// thin reduction bar. A meter's number is the most taken off since a
/// click on it or since the song was played from a stop. While stopped the
/// display holds what was last played and the bars fall away.
@MainActor
final class DynamicsView: NSView {
    enum Style {
        case display, meter, strip
    }

    private let model: SongModel
    private let style: Style
    private var effect: UInt64
    private var kind: String
    private var threshold: FieldView?
    private var link: CADisplayLink?
    private var lastTick: CFTimeInterval?
    private var idle = 0

    /// The hops last read, oldest first, and the count they were read at.
    private var input: [Float] = []
    private var output: [Float] = []
    private var reductions: [Float] = []
    private var thresholds: [Float] = []
    private var written: UInt64?
    private var held = DynamicsLayout.Held()
    /// The reduction the bar shows now.
    private var now = 0.0

    /// The threshold as dragged, ahead of the host.
    private var shown: Double?
    private var gesture: String?
    private var release = 0

    private static let font = NSFont.monospacedDigitSystemFont(ofSize: 9, weight: .regular)

    init(model: SongModel, effect: EffectView, style: Style) {
        self.model = model
        self.style = style
        self.effect = effect.key
        self.kind = effect.kind
        self.threshold = effect.fields.first { $0.name == "threshold_db" }
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    func update(_ effect: EffectView) {
        if effect.key != self.effect {
            self.effect = effect.key
            written = nil
            held.reset()
        }
        kind = effect.kind
        threshold = effect.fields.first { $0.name == "threshold_db" }
        if let shown, gesture == nil, thresholdValue == shown { self.shown = nil }
        window?.invalidateCursorRects(for: self)
        needsDisplay = true
    }

    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    /// A strip's bar lets clicks through to the strip, which opens it out.
    override func hitTest(_ point: NSPoint) -> NSView? {
        style == .strip ? nil : super.hitTest(point)
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        link?.invalidate()
        link = nil
        guard window != nil else { return }
        let link = displayLink(target: self, selector: #selector(tick(_:)))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    // MARK: Reading

    private var hops: Int {
        style == .display ? DynamicsLayout.hopsShown(sampleRate: Double(model.arrangement.sampleRate)) : 64
    }

    @objc private func tick(_ link: CADisplayLink) {
        guard let window, window.occlusionState.contains(.visible) else { return }
        let time = CACurrentMediaTime()
        let dt = lastTick.map { time - $0 } ?? 0
        lastTick = time
        // Nothing has played for a while and the bars are down: look less often.
        idle += 1
        if idle > 120, now == 0, idle % 10 != 0 { return }
        guard let reading = model.gainReduction(effect: effect, hops: hops) else {
            if written != nil || !reductions.isEmpty || now != 0 {
                (written, input, output, reductions, thresholds, now) = (nil, [], [], [], [], 0)
                needsDisplay = true
            }
            return
        }
        let fresh = reading.written != written
        let playing = model.playhead()?.playing ?? false
        let wasHeld = held.most
        if fresh {
            idle = 0
            reductions = reading.reductionDb.floats
            if style == .display {
                input = reading.inputDb.floats
                output = reading.outputDb.floats
                thresholds = reading.thresholdDb.floats
            } else if style == .meter {
                input = reading.inputDb.floats
                thresholds = reading.thresholdDb.floats
            }
        }
        held.update(written: reading.written, reductions: reductions, playing: playing)
        written = reading.written
        let was = now
        now = DynamicsLayout.fallen(now, toward: Double(reductions.last ?? 0), fresh: fresh, seconds: dt)
        if fresh || now != was || held.most != wasHeld { needsDisplay = true }
    }

    // MARK: The threshold

    private var thresholdValue: Double? {
        if case .number(let x)? = threshold?.value { return x }
        return nil
    }

    /// Whether a lane moves the threshold: it is drawn as played and not dragged.
    private var automated: Bool { threshold?.lane != nil }

    private var box: CGRect { bounds.insetBy(dx: 4, dy: 4) }

    /// The display's graph and the meter at its right.
    private var graph: CGRect {
        let b = box
        return CGRect(x: b.minX, y: b.minY, width: max(b.width - 16, 0), height: b.height)
    }

    private var meterBar: CGRect {
        let b = box
        return CGRect(x: b.maxX - 10, y: b.minY, width: 10, height: b.height)
    }

    override func resetCursorRects() {
        guard style == .display, !automated, let db = shown ?? thresholdValue else { return }
        let y = DynamicsLayout.y(db: db, in: graph)
        addCursorRect(CGRect(x: graph.minX, y: y - DynamicsLayout.grab, width: graph.width, height: 2 * DynamicsLayout.grab), cursor: .resizeUpDown)
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        if style == .display, !automated, let db = shown ?? thresholdValue, p.x <= graph.maxX,
           DynamicsLayout.grabs(y: p.y, thresholdDb: db, in: graph) {
            gesture = model.newGesture()
            return
        }
        held.reset()
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard let gesture else { return }
        let p = convert(event.locationInWindow, from: nil)
        let db = DynamicsLayout.threshold(at: p.y, in: graph)
        guard db != (shown ?? thresholdValue) else { return }
        shown = db
        model.drag(.effectSet(effect: effect, field: "threshold_db", value: .number(value: db)), gesture: gesture)
        window?.invalidateCursorRects(for: self)
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard gesture != nil else { return }
        gesture = nil
        guard shown != nil else { return }
        model.endDrag()
        // Keeps the line where it was let go until the host's song has it, or a second passes.
        release += 1
        let mine = release
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            MainActor.assumeIsolated {
                guard let self, self.release == mine, self.gesture == nil else { return }
                self.shown = nil
                self.window?.invalidateCursorRects(for: self)
                self.needsDisplay = true
            }
        }
    }

    // MARK: Drawing

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        switch style {
        case .strip:
            drawBar(context, in: bounds, range: DynamicsLayout.meterRange(kind: kind), held: false)
        case .meter:
            drawMeter(context)
        case .display:
            drawDisplay(context)
        }
    }

    /// A reduction bar falling from the top of `bar`, and the most held as
    /// a line across it.
    private func drawBar(_ context: CGContext, in bar: CGRect, range: Double, held showsHeld: Bool) {
        context.setFillColor(Theme.gray(0, 0.3).cgColor)
        context.fill(bar)
        let depth = DynamicsLayout.depth(reductionDb: now, range: range, in: bar)
        if depth > 0 {
            context.setFillColor(Theme.reduction.cgColor)
            context.fill(CGRect(x: bar.minX, y: bar.minY, width: bar.width, height: depth))
        }
        if showsHeld, held.most > 0 {
            let y = (bar.minY + DynamicsLayout.depth(reductionDb: held.most, range: range, in: bar)).rounded() + 0.5
            context.setStrokeColor(Theme.text.cgColor)
            context.setLineWidth(1)
            context.move(to: CGPoint(x: bar.minX, y: y))
            context.addLine(to: CGPoint(x: bar.maxX, y: y))
            context.strokePath()
        }
    }

    private func background() {
        let shape = NSBezierPath(roundedRect: bounds, xRadius: 3, yRadius: 3)
        Theme.control.setFill()
        shape.fill()
    }

    /// The level reaching a limiter or a clipper with its ceiling marked,
    /// the reduction beside it, and the most taken off under both.
    private func drawMeter(_ context: CGContext) {
        background()
        let b = box
        guard b.width > 20, b.height > 30 else { return }
        let bars = CGRect(x: b.minX, y: b.minY, width: b.width, height: b.height - 14)
        let width = (bars.width - 3) / 2
        let level = CGRect(x: bars.minX, y: bars.minY, width: width, height: bars.height)
        let reduction = CGRect(x: bars.maxX - width, y: bars.minY, width: width, height: bars.height)
        let (top, floor) = (DynamicsLayout.meterTopDb, DynamicsLayout.floorDb)
        context.setFillColor(Theme.gray(0, 0.3).cgColor)
        context.fill(level)
        let heard = Double(input.last ?? -200)
        let ceiling = Double(thresholds.last ?? 0)
        if now > 0 || heard > floor {
            let y = DynamicsLayout.y(db: heard, top: top, floor: floor, in: level)
            context.setFillColor((heard > ceiling ? Theme.cue : Theme.knob).cgColor)
            context.fill(CGRect(x: level.minX, y: y, width: level.width, height: level.maxY - y))
        }
        if !thresholds.isEmpty {
            let y = DynamicsLayout.y(db: ceiling, top: top, floor: floor, in: level).rounded() + 0.5
            context.setStrokeColor(Theme.cue.cgColor)
            context.setLineWidth(1)
            context.move(to: CGPoint(x: level.minX - 1, y: y))
            context.addLine(to: CGPoint(x: level.maxX + 1, y: y))
            context.strokePath()
        }
        drawBar(context, in: reduction, range: DynamicsLayout.meterRange(kind: kind), held: true)
        let number = CGRect(x: b.minX - 2, y: bars.maxY + 2, width: b.width + 4, height: 12)
        _ = TextLines.shared.draw(DynamicsLayout.text(reductionDb: held.most), in: number, font: Self.font,
                                  color: held.most > 0 ? Theme.text : Theme.dimText, align: .center, cuts: true)
    }

    /// The compressor's scrolling display and its meter.
    private func drawDisplay(_ context: CGContext) {
        background()
        let g = graph
        guard g.width > 20, g.height > 20 else { return }
        NSGraphicsContext.saveGraphicsState()
        context.clip(to: g)
        context.setLineWidth(1)
        for db in [-12.0, -24.0, -36.0, -48.0] {
            let y = DynamicsLayout.y(db: db, in: g).rounded() + 0.5
            context.setStrokeColor(Theme.gridLine.cgColor)
            context.move(to: CGPoint(x: g.minX, y: y))
            context.addLine(to: CGPoint(x: g.maxX, y: y))
            context.strokePath()
        }
        let width = Int(g.width)
        let fill = { (values: [Float], color: NSColor) in
            let columns = DynamicsLayout.columns(values, width: width, empty: -200)
            guard columns.contains(where: { Double($0) > DynamicsLayout.floorDb }) else { return }
            let path = CGMutablePath()
            path.move(to: CGPoint(x: g.minX, y: g.maxY))
            for (i, db) in columns.enumerated() {
                let y = DynamicsLayout.y(db: Double(db), in: g)
                path.addLine(to: CGPoint(x: g.minX + CGFloat(i), y: y))
                path.addLine(to: CGPoint(x: g.minX + CGFloat(i + 1), y: y))
            }
            path.addLine(to: CGPoint(x: g.maxX, y: g.maxY))
            path.closeSubpath()
            context.setFillColor(color.cgColor)
            context.addPath(path)
            context.fillPath()
        }
        fill(input, Theme.knob.withAlphaComponent(0.75))
        // What leaves, as a line over what came in.
        let leaving = DynamicsLayout.columns(output, width: width, empty: -200)
        if leaving.contains(where: { Double($0) > DynamicsLayout.floorDb }) {
            let path = CGMutablePath()
            for (i, db) in leaving.enumerated() {
                let point = CGPoint(x: g.minX + CGFloat(i) + 0.5, y: DynamicsLayout.y(db: Double(db), in: g))
                if i == 0 { path.move(to: point) } else { path.addLine(to: point) }
            }
            context.setStrokeColor(Theme.text.withAlphaComponent(0.85).cgColor)
            context.setLineWidth(1)
            context.addPath(path)
            context.strokePath()
        }
        // The reduction, from the top.
        let taken = DynamicsLayout.columns(reductions, width: width, empty: 0)
        if taken.contains(where: { $0 > 0 }) {
            context.setFillColor(Theme.reduction.withAlphaComponent(0.8).cgColor)
            for (i, r) in taken.enumerated() where r > 0 {
                context.fill(CGRect(x: g.minX + CGFloat(i), y: g.minY, width: 1, height: DynamicsLayout.depth(reductionDb: Double(r), in: g)))
            }
        }
        // The threshold: the field's, or as a lane moved it while it played.
        context.setStrokeColor(Theme.cue.cgColor)
        context.setLineWidth(1.5)
        let label: Double?
        if automated {
            let columns = DynamicsLayout.columns(thresholds, width: width, empty: 0)
            let path = CGMutablePath()
            for (i, db) in columns.enumerated() {
                let point = CGPoint(x: g.minX + CGFloat(i), y: DynamicsLayout.y(db: Double(db), in: g))
                if i == 0 { path.move(to: point) } else { path.addLine(to: point) }
            }
            context.setLineDash(phase: 0, lengths: [3, 2])
            context.addPath(path)
            context.strokePath()
            context.setLineDash(phase: 0, lengths: [])
            label = thresholds.last.map(Double.init) ?? thresholdValue
        } else if let db = shown ?? thresholdValue {
            let y = DynamicsLayout.y(db: db, in: g).rounded() + 0.5
            context.move(to: CGPoint(x: g.minX, y: y))
            context.addLine(to: CGPoint(x: g.maxX, y: y))
            context.strokePath()
            label = db
        } else {
            label = nil
        }
        if let label {
            let y = DynamicsLayout.y(db: label, in: g)
            let above = y - 13 >= g.minY
            let rect = CGRect(x: g.minX + 3, y: above ? y - 13 : y + 2, width: 60, height: 12)
            _ = TextLines.shared.draw(String(format: "%.1f dB", label), in: rect, font: Self.font, color: Theme.cue, cuts: true)
        }
        // The most taken off, in the graph's lower right.
        let number = CGRect(x: g.maxX - 44, y: g.maxY - 13, width: 42, height: 12)
        _ = TextLines.shared.draw(DynamicsLayout.text(reductionDb: held.most), in: number, font: Self.font,
                                  color: held.most > 0 ? Theme.text : Theme.dimText, align: .right, cuts: true)
        NSGraphicsContext.restoreGraphicsState()
        drawBar(context, in: meterBar, range: DynamicsLayout.meterRange(kind: kind), held: true)
    }
}
