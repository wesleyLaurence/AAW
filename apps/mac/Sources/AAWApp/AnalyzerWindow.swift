import AAWCore
import AppKit
import SwiftUI

/// An analyzer's window: one for each analyzer opened from its strip in
/// the device panel, titled for its row and the song. It holds the panes
/// `AnalyzerLayout` arranges, resizes freely, goes full screen with the
/// green button and stays open whatever is selected in the song's window.
/// It closes when its effect is gone from the song, or its project closes.
@MainActor
final class AnalyzerWindowController: NSWindowController, NSWindowDelegate {
    let model: SongModel
    let effect: UInt64
    let view: AnalyzerView
    /// Called when the window closed.
    var onClose: ((AnalyzerWindowController) -> Void)?

    init(model: SongModel, effect: UInt64) {
        self.model = model
        self.effect = effect
        view = AnalyzerView(model: model, effect: effect)
        let window = NSWindow(
            contentRect: CGRect(x: 0, y: 0, width: 900, height: 520),
            styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false
        )
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        window.appearance = NSAppearance(named: .darkAqua)
        window.minSize = CGSize(width: 480, height: 300)
        window.collectionBehavior = [.fullScreenPrimary, .managed]
        window.contentView = view
        window.setFrameAutosaveName("analyzer")
        super.init(window: window)
        window.delegate = self
        refreshTitle()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    /// The window's title: the row the analyzer is on and the song.
    func refreshTitle() {
        guard let window else { return }
        let title = model.analyzerName(effect: effect) ?? "Analyzer"
        if window.title != title { window.title = title }
        let subtitle = model.arrangement.title
        if window.subtitle != subtitle { window.subtitle = subtitle }
    }

    func windowWillClose(_ notification: Notification) {
        view.stop()
        onClose?(self)
    }
}

/// The analyzer's panes, drawn at the display's rate while the window can
/// be seen: each channel's levels, the loudness with its history, the
/// spectrum with its peaks held and the stereo field. A line between two
/// panes is dragged to size them, the mark at a pane's top right fills the
/// window with it and gives it back, a right click shows or hides each
/// pane, and a click on the levels or the loudness starts the held peaks,
/// the integrated loudness and the range again.
@MainActor
final class AnalyzerView: NSView {
    private let model: SongModel
    private let effect: UInt64
    private var link: CADisplayLink?
    private var arrangement = AnalyzerLayout.Arrangement.read(UserDefaults.standard.data(forKey: AnalyzerLayout.Arrangement.defaultsKey)) {
        didSet {
            if arrangement != oldValue {
                UserDefaults.standard.set(arrangement.data, forKey: AnalyzerLayout.Arrangement.defaultsKey)
                needsDisplay = true
            }
        }
    }

    /// The last reading, and the levels as shown, falling between readings.
    private var reading: Analysis?
    private var shownPeak = [-200.0, -200.0]
    private var shownTruePeak = [-200.0, -200.0]
    private var lastTick: CFTimeInterval?
    private var dragging: AnalyzerLayout.Divider?
    /// How long each draw took and the time between draws, the last few
    /// hundred, for `--measure`.
    private(set) var drawTimes = DrawTimes()
    private var lastDraw: CFTimeInterval?

    init(model: SongModel, effect: UInt64) {
        self.model = model
        self.effect = effect
        super.init(frame: .zero)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        stop()
        guard window != nil else { return }
        let link = displayLink(target: self, selector: #selector(tick(_:)))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func stop() {
        link?.invalidate()
        link = nil
    }

    /// Reads the analyzer when the window can be seen, and lets the shown
    /// peaks fall between readings.
    @objc private func tick(_ link: CADisplayLink) {
        guard let window, window.occlusionState.contains(.visible) else { return }
        let now = CACurrentMediaTime()
        let dt = lastTick.map { now - $0 } ?? 0
        lastTick = now
        if let reading = model.analysis(effect: effect) {
            self.reading = reading
            for c in 0..<2 {
                shownPeak[c] = AnalyzerLayout.fallen(shownPeak[c], toward: Double(reading.peak_db(c)), seconds: dt)
                shownTruePeak[c] = AnalyzerLayout.fallen(shownTruePeak[c], toward: Double(reading.true_peak_db(c)), seconds: dt)
            }
        } else {
            reading = nil
        }
        (window.windowController as? AnalyzerWindowController)?.refreshTitle()
        needsDisplay = true
    }

    // MARK: Mouse

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        if let divider = AnalyzerLayout.divider(at: p, arrangement, in: bounds) {
            dragging = divider
            return
        }
        guard let (pane, fillMark) = AnalyzerLayout.pane(at: p, arrangement, in: bounds) else { return }
        if fillMark {
            arrangement.toggleFill(pane)
        } else if pane == .levels || pane == .loudness {
            model.resetAnalysis(effect: effect)
        }
    }

    override func mouseDragged(with event: NSEvent) {
        guard let dragging else { return }
        let p = convert(event.locationInWindow, from: nil)
        arrangement = AnalyzerLayout.dragged(arrangement, dragging, to: p, in: bounds)
    }

    override func mouseUp(with event: NSEvent) {
        dragging = nil
    }

    override func resetCursorRects() {
        // Resize cursors over the dividers.
        let panes = AnalyzerLayout.panes(arrangement, in: bounds)
        guard panes.count > 1 else { return }
        let x = bounds.minX + bounds.width * arrangement.split
        if panes.keys.contains(where: { $0 == .levels || $0 == .loudness }), panes.keys.contains(where: { $0 == .spectrum || $0 == .stereo }) {
            addCursorRect(CGRect(x: x - AnalyzerLayout.dividerGrab, y: 0, width: AnalyzerLayout.dividerGrab * 2, height: bounds.height), cursor: .resizeLeftRight)
        }
        for (split, column): (CGFloat, [AnalyzerLayout.Pane]) in [(arrangement.leftSplit, [.levels, .loudness]), (arrangement.rightSplit, [.spectrum, .stereo])]
        where column.allSatisfy({ panes[$0] != nil }) {
            let y = bounds.minY + bounds.height * split
            let (minX, maxX) = (panes[column[0]]!.minX, panes[column[0]]!.maxX)
            addCursorRect(CGRect(x: minX, y: y - AnalyzerLayout.dividerGrab, width: maxX - minX, height: AnalyzerLayout.dividerGrab * 2), cursor: .resizeUpDown)
        }
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let menu = NSMenu()
        for pane in AnalyzerLayout.Pane.allCases {
            let item = NSMenuItem(title: pane.title, action: #selector(togglePane(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = pane.rawValue
            item.state = arrangement.hidden.contains(pane) ? .off : .on
            menu.addItem(item)
        }
        menu.addItem(.separator())
        let reset = NSMenuItem(title: "Reset Peaks and Loudness", action: #selector(resetMeters(_:)), keyEquivalent: "")
        reset.target = self
        menu.addItem(reset)
        return menu
    }

    @objc private func togglePane(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String, let pane = AnalyzerLayout.Pane(rawValue: raw) else { return }
        arrangement.toggle(pane)
        window?.invalidateCursorRects(for: self)
    }

    @objc private func resetMeters(_ sender: Any?) {
        model.resetAnalysis(effect: effect)
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        window?.invalidateCursorRects(for: self)
    }

    // MARK: Drawing

    private static let labelFont = NSFont.systemFont(ofSize: 10, weight: .medium)
    private static let smallFont = NSFont.monospacedDigitSystemFont(ofSize: 9, weight: .regular)
    private static let valueFont = NSFont.monospacedDigitSystemFont(ofSize: 20, weight: .medium)

    private func text(_ s: String, at p: CGPoint, font: NSFont, color: NSColor = Theme.dimText, right: Bool = false) {
        let attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color]
        let size = (s as NSString).size(withAttributes: attributes)
        (s as NSString).draw(at: CGPoint(x: right ? p.x - size.width : p.x, y: p.y), withAttributes: attributes)
    }

    override func draw(_ dirtyRect: NSRect) {
        let started = CACurrentMediaTime()
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        Theme.background.setFill()
        bounds.fill()
        let panes = AnalyzerLayout.panes(arrangement, in: bounds)
        for pane in arrangement.shown {
            guard let rect = panes[pane] else { continue }
            drawPane(pane, in: rect, context: context)
        }
        if let lastDraw { drawTimes.intervals.append((started - lastDraw) * 1000) }
        lastDraw = started
        drawTimes.draws.append((CACurrentMediaTime() - started) * 1000)
        if drawTimes.draws.count > 600 {
            drawTimes.draws.removeFirst(drawTimes.draws.count - 600)
            drawTimes.intervals.removeFirst(max(0, drawTimes.intervals.count - 600))
        }
    }

    /// A pane: its bar with the title and the fill mark, and its view under it.
    private func drawPane(_ pane: AnalyzerLayout.Pane, in rect: CGRect, context: CGContext) {
        let shape = NSBezierPath(roundedRect: rect, xRadius: 4, yRadius: 4)
        Theme.gray(0.15).setFill()
        shape.fill()
        NSGraphicsContext.saveGraphicsState()
        shape.addClip()
        text(pane.title, at: CGPoint(x: rect.minX + 7, y: rect.minY + 3), font: Self.labelFont, color: Theme.dimText)
        // The fill mark: arrows out, or in while the pane fills the window.
        let mark = arrangement.filled == pane ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right"
        if let image = NSImage(systemSymbolName: mark, accessibilityDescription: nil)?.withSymbolConfiguration(.init(pointSize: 9, weight: .medium)) {
            let tinted = image.tinted(Theme.dimText)
            tinted.draw(in: CGRect(x: rect.maxX - 14, y: rect.minY + 4, width: 9, height: 9))
        }
        let box = CGRect(x: rect.minX + 8, y: rect.minY + AnalyzerLayout.paneBar + 4, width: rect.width - 16, height: rect.height - AnalyzerLayout.paneBar - 12)
        if box.width > 20, box.height > 20 {
            switch pane {
            case .levels: drawLevels(in: box, context: context)
            case .loudness: drawLoudness(in: box, context: context)
            case .spectrum: drawSpectrum(in: box, context: context)
            case .stereo: drawStereo(in: box, context: context)
            }
        }
        NSGraphicsContext.restoreGraphicsState()
    }

    private func level(_ db: Double, in box: CGRect) -> CGFloat {
        AnalyzerLayout.y(db, floor: AnalyzerLayout.levelFloorDb, top: AnalyzerLayout.levelTopDb, in: box)
    }

    /// Two bars, left and right: the peak as the bar, falling, the RMS as a
    /// brighter bar inside it, the highest peak as a line held, with the
    /// scale between them and the numbers under them.
    private func drawLevels(in box: CGRect, context: CGContext) {
        let footer: CGFloat = 46
        let meters = CGRect(x: box.minX, y: box.minY, width: box.width, height: max(box.height - footer, 10))
        let scaleWidth: CGFloat = 30
        let barWidth = max((meters.width - scaleWidth - 12) / 2, 8)
        let bars = [
            CGRect(x: meters.minX, y: meters.minY, width: barWidth, height: meters.height),
            CGRect(x: meters.maxX - barWidth, y: meters.minY, width: barWidth, height: meters.height),
        ]
        // The scale, between the bars.
        for db in [6.0, 0, -6, -12, -18, -24, -36, -48, -60] {
            let y = level(db, in: meters).rounded() + 0.5
            context.setStrokeColor((db == 0 ? Theme.barLine : Theme.gridLine).cgColor)
            context.setLineWidth(1)
            context.move(to: CGPoint(x: bars[0].maxX + 2, y: y))
            context.addLine(to: CGPoint(x: bars[1].minX - 2, y: y))
            context.strokePath()
            let label = db == 0 ? "0" : db > 0 ? "+\(Int(db))" : "\(Int(db))"
            text(label, at: CGPoint(x: meters.midX - 8, y: y - 6), font: Self.smallFont, color: Theme.faintText)
        }
        for c in 0..<2 {
            let bar = bars[c]
            context.setFillColor(Theme.control.cgColor)
            context.fill(bar)
            let peakY = level(shownPeak[c], in: bar)
            let over = shownPeak[c] > 0
            context.setFillColor((over ? Theme.cue : Theme.knob).cgColor)
            context.fill(CGRect(x: bar.minX, y: peakY, width: bar.width, height: bar.maxY - peakY))
            if let reading {
                let rmsY = level(Double(reading.rms_db(c)), in: bar)
                context.setFillColor(Theme.modulation.withAlphaComponent(0.85).cgColor)
                context.fill(CGRect(x: bar.minX + bar.width * 0.3, y: rmsY, width: bar.width * 0.4, height: bar.maxY - rmsY))
                let hold = Double(reading.peak_hold_db(c))
                if hold > AnalyzerLayout.levelFloorDb {
                    let y = level(hold, in: bar).rounded() + 0.5
                    context.setStrokeColor((hold > 0 ? Theme.cue : Theme.text).cgColor)
                    context.setLineWidth(1)
                    context.move(to: CGPoint(x: bar.minX, y: y))
                    context.addLine(to: CGPoint(x: bar.maxX, y: y))
                    context.strokePath()
                }
            }
            text(c == 0 ? "L" : "R", at: CGPoint(x: bar.midX - 3, y: bar.maxY + 3), font: Self.labelFont, color: Theme.dimText)
        }
        // The numbers: the held peak and true peak of each channel.
        let y = box.maxY - 28
        let holds = (0..<2).map { c in AnalyzerLayout.text(reading?.peak_hold_db(c), unit: "dB") }
        let trues = (0..<2).map { c in AnalyzerLayout.text(reading?.true_peak_hold_db(c), unit: "dBTP") }
        text("Peak \(holds[0])  \(holds[1])", at: CGPoint(x: box.minX, y: y), font: Self.smallFont, color: Theme.text)
        text("True \(trues[0])  \(trues[1])", at: CGPoint(x: box.minX, y: y + 13), font: Self.smallFont, color: Theme.text)
    }

    /// The momentary, short-term and integrated loudness and the range as
    /// numbers, over the short-term loudness as a line across the last
    /// minute, with −14 LUFS marked.
    private func drawLoudness(in box: CGRect, context: CGContext) {
        let numbers = CGRect(x: box.minX, y: box.minY, width: box.width, height: 52)
        let columns = [("M", AnalyzerLayout.text(reading?.momentaryLufs, unit: "")),
                       ("S", AnalyzerLayout.text(reading?.shortTermLufs, unit: "")),
                       ("I", AnalyzerLayout.text(reading?.integratedLufs, unit: "")),
                       ("LRA", reading?.rangeLu.map { String(format: "%.1f", $0) } ?? "–")]
        let width = numbers.width / CGFloat(columns.count)
        for (i, (label, value)) in columns.enumerated() {
            let x = numbers.minX + CGFloat(i) * width
            text(label, at: CGPoint(x: x, y: numbers.minY), font: Self.labelFont, color: Theme.faintText)
            text(value, at: CGPoint(x: x, y: numbers.minY + 14), font: Self.valueFont, color: Theme.text)
        }
        let graph = CGRect(x: box.minX, y: numbers.maxY + 8, width: box.width, height: max(box.height - numbers.height - 8, 10))
        context.setFillColor(Theme.control.cgColor)
        context.fill(graph)
        text("LUFS, BS.1770-4", at: CGPoint(x: graph.maxX - 3, y: graph.minY + 2), font: Self.smallFont, color: Theme.faintText, right: true)
        for lufs in [-6.0, -14, -23, -30, -45] {
            let y = AnalyzerLayout.y(lufs, floor: AnalyzerLayout.loudnessFloor, top: AnalyzerLayout.loudnessTop, in: graph).rounded() + 0.5
            context.setStrokeColor((lufs == -14 ? Theme.barLine : Theme.gridLine).cgColor)
            context.setLineWidth(1)
            context.move(to: CGPoint(x: graph.minX, y: y))
            context.addLine(to: CGPoint(x: graph.maxX, y: y))
            context.strokePath()
            text("\(Int(lufs))", at: CGPoint(x: graph.minX + 3, y: y - 11), font: Self.smallFont, color: Theme.faintText)
        }
        if let reading {
            // Sixty seconds across, or less while the window is narrow.
            let seconds = max(10, min(60, Double(graph.width) / 4))
            let points = AnalyzerLayout.historyPoints(reading.history, seconds: seconds, perSecond: 10, in: graph)
            if points.count > 1 {
                let path = CGMutablePath()
                path.addLines(between: points)
                context.setStrokeColor(Theme.modulation.cgColor)
                context.setLineWidth(1.5)
                context.addPath(path)
                context.strokePath()
            }
            if let integrated = reading.integratedLufs, integrated > Float(AnalyzerLayout.loudnessFloor) {
                let y = AnalyzerLayout.y(Double(integrated), floor: AnalyzerLayout.loudnessFloor, top: AnalyzerLayout.loudnessTop, in: graph).rounded() + 0.5
                context.setStrokeColor(Theme.cue.withAlphaComponent(0.8).cgColor)
                context.setLineWidth(1)
                context.move(to: CGPoint(x: graph.minX, y: y))
                context.addLine(to: CGPoint(x: graph.maxX, y: y))
                context.strokePath()
            }
            text("short-term, last \(Int(seconds)) s", at: CGPoint(x: graph.maxX - 3, y: graph.maxY - 13), font: Self.smallFont, color: Theme.faintText, right: true)
        }
    }

    /// The spectrum as the equalizer's panel draws it, filled to the floor,
    /// with the highest level of each column held as a line over it.
    private func drawSpectrum(in box: CGRect, context: CGContext) {
        context.setFillColor(Theme.control.cgColor)
        context.fill(box)
        context.setLineWidth(1)
        for hz in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
            let x = EqLayout.x(hz: hz, in: box).rounded() + 0.5
            context.setStrokeColor((hz == 100 || hz == 1000 || hz == 10000 ? Theme.beatLine : Theme.gridLine).cgColor)
            context.move(to: CGPoint(x: x, y: box.minY))
            context.addLine(to: CGPoint(x: x, y: box.maxY))
            context.strokePath()
            let label = hz >= 1000 ? "\(Int(hz / 1000))k" : "\(Int(hz))"
            text(label, at: CGPoint(x: x + 2, y: box.maxY - 12), font: Self.smallFont, color: Theme.faintText)
        }
        for db in stride(from: -12.0, through: -84, by: -12) {
            let y = box.minY + box.height * CGFloat(-db / -EqLayout.spectrumFloorDb)
            context.setStrokeColor(Theme.gridLine.cgColor)
            context.move(to: CGPoint(x: box.minX, y: y.rounded() + 0.5))
            context.addLine(to: CGPoint(x: box.maxX, y: y.rounded() + 0.5))
            context.strokePath()
            text("\(Int(db))", at: CGPoint(x: box.maxX - 3, y: y - 11), font: Self.smallFont, color: Theme.faintText, right: true)
        }
        guard let reading else { return }
        let rate = Double(reading.sampleRate)
        let columns = EqLayout.spectrumColumns(levels: reading.spectrum, sampleRate: rate, in: box)
        if columns.contains(where: { $0 < box.maxY }) {
            let path = CGMutablePath()
            path.move(to: CGPoint(x: box.minX, y: box.maxY))
            for (i, y) in columns.enumerated() { path.addLine(to: CGPoint(x: box.minX + CGFloat(i), y: y)) }
            path.addLine(to: CGPoint(x: box.minX + CGFloat(columns.count), y: box.maxY))
            path.closeSubpath()
            context.setFillColor(Theme.modulation.withAlphaComponent(0.3).cgColor)
            context.addPath(path)
            context.fillPath()
        }
        let hold = EqLayout.spectrumColumns(levels: reading.spectrumHold, sampleRate: rate, in: box)
        if hold.contains(where: { $0 < box.maxY }) {
            let path = CGMutablePath()
            for (i, y) in hold.enumerated() {
                let p = CGPoint(x: box.minX + CGFloat(i), y: y)
                if i == 0 { path.move(to: p) } else { path.addLine(to: p) }
            }
            context.setStrokeColor(Theme.text.withAlphaComponent(0.6).cgColor)
            context.setLineWidth(1)
            context.addPath(path)
            context.strokePath()
        }
    }

    /// The vectorscope of the last frames, mono up the middle, over bars of
    /// the correlation and the balance.
    private func drawStereo(in box: CGRect, context: CGContext) {
        let bars: CGFloat = 44
        let scope = CGRect(x: box.minX, y: box.minY, width: box.width, height: max(box.height - bars, 10))
        let side = min(scope.width, scope.height)
        let square = CGRect(x: scope.midX - side / 2, y: scope.minY, width: side, height: side)
        context.setFillColor(Theme.control.cgColor)
        context.fill(square)
        // The axes: L and R up the diagonals, mono up the middle.
        context.setStrokeColor(Theme.gridLine.cgColor)
        context.setLineWidth(1)
        context.move(to: CGPoint(x: square.minX, y: square.maxY))
        context.addLine(to: CGPoint(x: square.midX, y: square.midY))
        context.addLine(to: CGPoint(x: square.maxX, y: square.maxY))
        context.strokePath()
        context.setStrokeColor(Theme.beatLine.cgColor)
        context.move(to: CGPoint(x: square.midX, y: square.minY))
        context.addLine(to: CGPoint(x: square.midX, y: square.maxY))
        context.strokePath()
        text("L", at: CGPoint(x: square.minX + 3, y: square.maxY - 13), font: Self.smallFont, color: Theme.faintText)
        text("R", at: CGPoint(x: square.maxX - 3, y: square.maxY - 13), font: Self.smallFont, color: Theme.faintText, right: true)
        if let reading {
            let peak = reading.scope.reduce(Float(0)) { max($0, abs($1)) }
            text("peak \(AnalyzerLayout.text(peak > 0 ? 20 * log10(peak) : nil, unit: "dB")) at the edge", at: CGPoint(x: square.midX, y: square.minY + 2), font: Self.smallFont, color: Theme.faintText, right: false)
        }
        if let reading, reading.scope.count >= 2 {
            var rects: [CGRect] = []
            rects.reserveCapacity(reading.scope.count / 2)
            let gain = AnalyzerLayout.scopeGain(reading.scope)
            for i in stride(from: 0, to: reading.scope.count - 1, by: 2) {
                let p = AnalyzerLayout.scopePoint(left: Double(reading.scope[i]), right: Double(reading.scope[i + 1]), gain: gain, in: square)
                guard square.contains(p) else { continue }
                rects.append(CGRect(x: p.x - 0.75, y: p.y - 0.75, width: 1.5, height: 1.5))
            }
            context.setFillColor(Theme.modulation.withAlphaComponent(0.7).cgColor)
            context.fill(rects)
        }
        // Correlation from −1 to +1, and the balance from L to R.
        let rows = [("Correlation", "−1", "+1", reading.map { AnalyzerLayout.correlationX(Double($0.correlation), in: CGRect(x: box.minX + 92, y: 0, width: box.width - 130, height: 1)) },
                     reading.map { String(format: "%+.2f", $0.correlation) } ?? "–"),
                    ("Balance", "L", "R", reading.map { AnalyzerLayout.balanceX(Double($0.balanceDb), in: CGRect(x: box.minX + 92, y: 0, width: box.width - 130, height: 1)) },
                     reading.map { String(format: "%+.1f dB", $0.balanceDb) } ?? "–")]
        for (i, (label, low, high, x, value)) in rows.enumerated() {
            let y = scope.maxY + 6 + CGFloat(i) * 20
            let bar = CGRect(x: box.minX + 92, y: y + 4, width: box.width - 130, height: 6)
            text(label, at: CGPoint(x: box.minX, y: y), font: Self.smallFont, color: Theme.faintText)
            context.setFillColor(Theme.control.cgColor)
            context.fill(bar)
            text(low, at: CGPoint(x: bar.minX - 5, y: y), font: Self.smallFont, color: Theme.faintText, right: true)
            text(high, at: CGPoint(x: bar.maxX + 3, y: y), font: Self.smallFont, color: Theme.faintText)
            if let x {
                context.setFillColor(Theme.modulation.cgColor)
                context.fill(CGRect(x: x - 1.5, y: bar.minY - 2, width: 3, height: bar.height + 4))
            }
            text(value, at: CGPoint(x: box.maxX, y: y), font: Self.smallFont, color: Theme.text, right: true)
        }
    }
}

extension NSImage {
    /// The image drawn in one color, for a symbol in a Core Graphics drawing.
    func tinted(_ color: NSColor) -> NSImage {
        let image = NSImage(size: size, flipped: false) { rect in
            color.set()
            rect.fill()
            self.draw(in: rect, from: .zero, operation: .destinationIn, fraction: 1)
            return true
        }
        return image
    }
}

extension Analysis {
    func peak_db(_ channel: Int) -> Float { peakDb.indices.contains(channel) ? peakDb[channel] : -200 }
    func peak_hold_db(_ channel: Int) -> Float { peakHoldDb.indices.contains(channel) ? peakHoldDb[channel] : -200 }
    func rms_db(_ channel: Int) -> Float { rmsDb.indices.contains(channel) ? rmsDb[channel] : -200 }
    func true_peak_db(_ channel: Int) -> Float { truePeakDb.indices.contains(channel) ? truePeakDb[channel] : -200 }
    func true_peak_hold_db(_ channel: Int) -> Float { truePeakHoldDb.indices.contains(channel) ? truePeakHoldDb[channel] : -200 }
}

/// The analyzer's strip in the device panel: the two channels' levels and
/// a small spectrum, drawn at the display's rate while the panel shows,
/// with the button under it that opens the analyzer's window.
struct AnalyzerStrip: View {
    let model: SongModel
    let effect: EffectView

    var body: some View {
        VStack(spacing: 4) {
            AnalyzerStripDrawing(model: model, effect: effect.key)
                .frame(maxHeight: .infinity)
            Button {
                model.openAnalyzer(effect: effect.key)
            } label: {
                Label("Open Window", systemImage: "macwindow")
                    .font(.system(size: 10))
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .help("Opens the analyzer's window, which stays open whatever is selected; View › Analyzer Window does too")
        }
        .padding(6)
    }
}

private struct AnalyzerStripDrawing: NSViewRepresentable {
    let model: SongModel
    let effect: UInt64

    func makeNSView(context: Context) -> AnalyzerStripView {
        AnalyzerStripView(model: model, effect: effect)
    }

    func updateNSView(_ view: AnalyzerStripView, context: Context) {
        view.effect = effect
    }
}

/// The strip's drawing: two level bars with their peaks held, and the
/// spectrum beside them.
@MainActor
final class AnalyzerStripView: NSView {
    private let model: SongModel
    var effect: UInt64
    private var link: CADisplayLink?
    private var reading: Analysis?
    private var shownPeak = [-200.0, -200.0]
    private var lastTick: CFTimeInterval?

    init(model: SongModel, effect: UInt64) {
        self.model = model
        self.effect = effect
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var isFlipped: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        link?.invalidate()
        link = nil
        guard window != nil else { return }
        let link = displayLink(target: self, selector: #selector(tick(_:)))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    @objc private func tick(_ link: CADisplayLink) {
        guard let window, window.occlusionState.contains(.visible) else { return }
        let now = CACurrentMediaTime()
        let dt = lastTick.map { now - $0 } ?? 0
        lastTick = now
        reading = model.analysis(effect: effect)
        for c in 0..<2 {
            shownPeak[c] = AnalyzerLayout.fallen(shownPeak[c], toward: Double(reading?.peak_db(c) ?? -200), seconds: dt)
        }
        needsDisplay = true
    }

    override func mouseDown(with event: NSEvent) {
        model.resetAnalysis(effect: effect)
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        let shape = NSBezierPath(roundedRect: bounds, xRadius: 3, yRadius: 3)
        Theme.control.setFill()
        shape.fill()
        let box = bounds.insetBy(dx: 4, dy: 4)
        guard box.width > 40, box.height > 20 else { return }
        let barWidth: CGFloat = 10
        let bars = [CGRect(x: box.minX, y: box.minY, width: barWidth, height: box.height), CGRect(x: box.minX + barWidth + 3, y: box.minY, width: barWidth, height: box.height)]
        for c in 0..<2 {
            let bar = bars[c]
            context.setFillColor(Theme.gray(0, 0.3).cgColor)
            context.fill(bar)
            let y = AnalyzerLayout.y(shownPeak[c], floor: AnalyzerLayout.levelFloorDb, top: AnalyzerLayout.levelTopDb, in: bar)
            context.setFillColor((shownPeak[c] > 0 ? Theme.cue : Theme.knob).cgColor)
            context.fill(CGRect(x: bar.minX, y: y, width: bar.width, height: bar.maxY - y))
            if let reading {
                let hold = Double(reading.peak_hold_db(c))
                if hold > AnalyzerLayout.levelFloorDb {
                    let y = AnalyzerLayout.y(hold, floor: AnalyzerLayout.levelFloorDb, top: AnalyzerLayout.levelTopDb, in: bar).rounded() + 0.5
                    context.setStrokeColor((hold > 0 ? Theme.cue : Theme.text).cgColor)
                    context.setLineWidth(1)
                    context.move(to: CGPoint(x: bar.minX, y: y))
                    context.addLine(to: CGPoint(x: bar.maxX, y: y))
                    context.strokePath()
                }
            }
        }
        let zero = AnalyzerLayout.y(0, floor: AnalyzerLayout.levelFloorDb, top: AnalyzerLayout.levelTopDb, in: bars[0]).rounded() + 0.5
        context.setStrokeColor(Theme.barLine.cgColor)
        context.setLineWidth(1)
        context.move(to: CGPoint(x: bars[0].minX, y: zero))
        context.addLine(to: CGPoint(x: bars[1].maxX, y: zero))
        context.strokePath()
        let spectrum = CGRect(x: bars[1].maxX + 6, y: box.minY, width: box.maxX - bars[1].maxX - 6, height: box.height)
        guard let reading, spectrum.width > 10 else { return }
        let columns = EqLayout.spectrumColumns(levels: reading.spectrum, sampleRate: Double(reading.sampleRate), in: spectrum)
        if columns.contains(where: { $0 < spectrum.maxY }) {
            let path = CGMutablePath()
            path.move(to: CGPoint(x: spectrum.minX, y: spectrum.maxY))
            for (i, y) in columns.enumerated() { path.addLine(to: CGPoint(x: spectrum.minX + CGFloat(i), y: y)) }
            path.addLine(to: CGPoint(x: spectrum.minX + CGFloat(columns.count), y: spectrum.maxY))
            path.closeSubpath()
            context.setFillColor(Theme.modulation.withAlphaComponent(0.35).cgColor)
            context.addPath(path)
            context.fillPath()
        }
        let label = "\(AnalyzerLayout.text(reading.momentaryLufs, unit: "LUFS"))"
        (label as NSString).draw(at: CGPoint(x: spectrum.minX + 2, y: spectrum.minY + 1), withAttributes: [.font: NSFont.monospacedDigitSystemFont(ofSize: 9, weight: .regular), .foregroundColor: Theme.dimText])
    }
}
