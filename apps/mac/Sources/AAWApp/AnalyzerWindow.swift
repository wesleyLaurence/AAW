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
            contentRect: CGRect(x: 0, y: 0, width: 1100, height: 580),
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
/// spectrum with its peaks held, the stereo field, and the last twenty
/// seconds scrolling as a waveform of each channel and as a spectrogram,
/// each a texture a column is added to as the song plays. A line between
/// two panes is dragged to size them, the mark at a pane's top right fills
/// the window with it and gives it back, a right click shows or hides each
/// pane, and a click on the levels or the loudness starts the held peaks,
/// the integrated loudness and the range again.
@MainActor
final class AnalyzerView: NSView {
    private let model: SongModel
    private let effect: UInt64
    private var link: CADisplayLink?
    /// The spectrogram's texture, made at the first reading for its rows,
    /// and the waveform's for each channel; how many of the host's columns
    /// each has taken.
    private var spectrogram: ScrollingTexture?
    private let waveforms = [ScrollingTexture(width: AnalyzerLayout.waveformColumns, height: AnalyzerLayout.waveformRows),
                             ScrollingTexture(width: AnalyzerLayout.waveformColumns, height: AnalyzerLayout.waveformRows)]
    private var seenSpectrogram: UInt64 = 0
    private var seenWaveform: UInt64 = 0
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
        if let reading = model.analysis(effect: effect, frame: link.targetTimestamp) {
            self.reading = reading
            for c in 0..<2 {
                shownPeak[c] = AnalyzerLayout.fallen(shownPeak[c], toward: Double(reading.peak_db(c)), seconds: dt)
                shownTruePeak[c] = AnalyzerLayout.fallen(shownTruePeak[c], toward: Double(reading.true_peak_db(c)), seconds: dt)
            }
            scroll(reading)
        } else {
            reading = nil
        }

        (window.windowController as? AnalyzerWindowController)?.refreshTitle()
        needsDisplay = true
    }

    /// The spectrogram's color ramp, from the floor to 0 dB: near black,
    /// then a deep blue, the knob's blue, the modulation color and white.
    private static let ramp: [UInt32] = {
        let stops: [(Double, NSColor)] = [(0, Theme.gray(0.06)), (0.3, Theme.rgb(0x1a2b4d)), (0.55, Theme.knob), (0.8, Theme.modulation), (1, Theme.rgb(0xfff6dc))]
        return (0..<256).map { i in
            let t = Double(i) / 255
            let after = stops.firstIndex { $0.0 >= t } ?? stops.count - 1
            let (a, b) = (stops[max(after - 1, 0)], stops[after])
            let f = b.0 > a.0 ? CGFloat((t - a.0) / (b.0 - a.0)) : 0
            return (a.1.usingColorSpace(.sRGB)!.blended(withFraction: f, of: b.1.usingColorSpace(.sRGB)!) ?? b.1).pixel
        }
    }()

    private static let wavePixel = Theme.modulation.pixel

    /// Adds the reading's new columns to the textures: the ones this view
    /// has not taken, blank columns for any it missed.
    private func scroll(_ reading: Analysis) {
        let rows = Int(reading.spectrogramRows)
        if rows > 0 {
            if spectrogram == nil || spectrogram?.height != rows {
                spectrogram = ScrollingTexture(width: AnalyzerLayout.spectrogramColumns, height: rows)
                seenSpectrogram = 0
            }
            let levels = reading.spectrogram.floats
            if let texture = spectrogram, levels.count % rows == 0 {
                let held = levels.count / rows
                let new = AnalyzerLayout.newColumns(seen: seenSpectrogram, total: reading.spectrogramColumns, held: held)
                if new.clear { texture.clear() }
                for _ in 0..<min(new.blank, texture.width) { texture.appendBlank() }
                for i in (held - new.take)..<held {
                    let column = levels[(i * rows)..<((i + 1) * rows)]
                    let base = column.startIndex
                    texture.append { row in Self.ramp[AnalyzerLayout.colorIndex(column[base + rows - 1 - row], steps: 256)] }
                }
                seenSpectrogram = new.seen
            }
        }
        let waveform = reading.waveform.floats
        if waveform.count % 4 == 0 {
            let held = waveform.count / 4
            let new = AnalyzerLayout.newColumns(seen: seenWaveform, total: reading.waveformColumns, held: held)
            for texture in waveforms {
                if new.clear { texture.clear() }
                for _ in 0..<min(new.blank, texture.width) { texture.appendBlank() }
            }
            for i in (held - new.take)..<held {
                for c in 0..<2 {
                    let (low, high) = (waveform[i * 4 + c * 2], waveform[i * 4 + c * 2 + 1])
                    let rows = waveforms[c].height
                    waveforms[c].append(from: AnalyzerLayout.waveformRow(high, rows: rows), to: AnalyzerLayout.waveformRow(low, rows: rows), color: Self.wavePixel)
                }
            }
            seenWaveform = new.seen
        }
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
        for divider in AnalyzerLayout.dividers(arrangement, in: bounds) {
            addCursorRect(divider.grab, cursor: divider.vertical ? .resizeLeftRight : .resizeUpDown)
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

    /// A label, laid out once and drawn each frame: the labels and the
    /// scales are the same every frame, and the readings change by a digit.
    private func text(_ s: String, at p: CGPoint, font: NSFont, color: NSColor = Theme.dimText, right: Bool = false) {
        let rect = right ? CGRect(x: p.x - 400, y: p.y, width: 400, height: 20) : CGRect(x: p.x, y: p.y, width: 400, height: 20)
        _ = TextLines.shared.draw(s, in: rect, font: font, color: color, align: right ? .right : .left, cuts: true)
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
            case .waveform: drawWaveform(in: box, context: context)
            case .spectrogram: drawSpectrogram(in: box, context: context)
            }
        }
        NSGraphicsContext.restoreGraphicsState()
    }

    /// The seconds a time pane shows: the texture's columns at the host's pace.
    private func seconds(columns: Int, perSecond: UInt32) -> Double {
        Double(columns) / Double(max(perSecond, 1))
    }

    /// Marks of whole seconds across a time pane, lines up the graph and
    /// labels in the footer under it.
    private func drawSeconds(_ seconds: Double, graph: CGRect, footer: CGRect, context: CGContext) {
        let every = seconds > 30 ? 10.0 : 5.0
        context.setStrokeColor(Theme.beatLine.cgColor)
        context.setLineWidth(1)
        for mark in AnalyzerLayout.secondMarks(seconds: seconds, every: every, in: graph) {
            let x = mark.x.rounded() + 0.5
            context.move(to: CGPoint(x: x, y: graph.minY))
            context.addLine(to: CGPoint(x: x, y: graph.maxY))
            context.strokePath()
            text(mark.label, at: CGPoint(x: x + 2, y: footer.minY), font: Self.smallFont, color: Theme.faintText)
        }
        text("now", at: CGPoint(x: graph.maxX, y: footer.minY), font: Self.smallFont, color: Theme.faintText, right: true)
    }

    /// The last seconds of each channel, the left over the right, as the
    /// lowest and highest sample of each hundredth of a second, newest at
    /// the right.
    private func drawWaveform(in box: CGRect, context: CGContext) {
        let footer: CGFloat = 14
        let graph = CGRect(x: box.minX, y: box.minY, width: box.width, height: max(box.height - footer, 10))
        let channel = (graph.height - 4) / 2
        for c in 0..<2 {
            let lane = CGRect(x: graph.minX, y: graph.minY + CGFloat(c) * (channel + 4), width: graph.width, height: channel)
            context.setFillColor(Theme.control.cgColor)
            context.fill(lane)
            let mid = lane.midY.rounded() + 0.5
            context.setStrokeColor(Theme.gridLine.cgColor)
            context.setLineWidth(1)
            context.move(to: CGPoint(x: lane.minX, y: mid))
            context.addLine(to: CGPoint(x: lane.maxX, y: mid))
            context.strokePath()
            waveforms[c].draw(in: lane, context: context)
            text(c == 0 ? "L" : "R", at: CGPoint(x: lane.minX + 3, y: lane.minY + 1), font: Self.smallFont, color: Theme.faintText)
        }
        let seconds = seconds(columns: AnalyzerLayout.waveformColumns, perSecond: reading?.waveformPerSecond ?? 100)
        drawSeconds(seconds, graph: graph, footer: CGRect(x: box.minX, y: graph.maxY + 1, width: box.width, height: footer), context: context)
    }

    /// The last seconds as a spectrogram: frequency up in equal steps of
    /// pitch, time across with the newest at the right, and the level as
    /// the color, with the frequencies the spectrum marks.
    private func drawSpectrogram(in box: CGRect, context: CGContext) {
        let footer: CGFloat = 14
        let graph = CGRect(x: box.minX, y: box.minY, width: box.width, height: max(box.height - footer, 10))
        context.setFillColor(Theme.gray(0.06).cgColor)
        context.fill(graph)
        spectrogram?.draw(in: graph, context: context)
        context.setLineWidth(1)
        for hz in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
            let y = AnalyzerLayout.spectrogramY(hz: hz, in: graph).rounded() + 0.5
            context.setStrokeColor((hz == 100 || hz == 1000 || hz == 10000 ? Theme.gray(1, 0.18) : Theme.gray(1, 0.08)).cgColor)
            context.move(to: CGPoint(x: graph.minX, y: y))
            context.addLine(to: CGPoint(x: graph.maxX, y: y))
            context.strokePath()
            let label = hz >= 1000 ? "\(Int(hz / 1000))k" : "\(Int(hz))"
            text(label, at: CGPoint(x: graph.minX + 3, y: y - 12), font: Self.smallFont, color: Theme.dimText)
        }
        let seconds = seconds(columns: AnalyzerLayout.spectrogramColumns, perSecond: reading?.spectrogramPerSecond ?? 50)
        drawSeconds(seconds, graph: graph, footer: CGRect(x: box.minX, y: graph.maxY + 1, width: box.width, height: footer), context: context)
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
            let points = AnalyzerLayout.historyPoints(reading.history.floats, seconds: seconds, perSecond: 10, in: graph)
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
        let columns = EqLayout.spectrumColumns(levels: reading.spectrum.floats, sampleRate: rate, in: box)
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
        let hold = EqLayout.spectrumColumns(levels: reading.spectrumHold.floats, sampleRate: rate, in: box)
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
        let samples = reading?.scope.floats ?? []
        if reading != nil {
            let peak = samples.reduce(Float(0)) { max($0, abs($1)) }
            text("peak \(AnalyzerLayout.text(peak > 0 ? 20 * log10(peak) : nil, unit: "dB")) at the edge", at: CGPoint(x: square.midX, y: square.minY + 2), font: Self.smallFont, color: Theme.faintText, right: false)
        }
        if samples.count >= 2 {
            var rects: [CGRect] = []
            rects.reserveCapacity(samples.count / 2)
            let gain = AnalyzerLayout.scopeGain(samples)
            for i in stride(from: 0, to: samples.count - 1, by: 2) {
                let p = AnalyzerLayout.scopePoint(left: Double(samples[i]), right: Double(samples[i + 1]), gain: gain, in: square)
                guard square.contains(p) else { continue }
                rects.append(CGRect(x: p.x - 0.75, y: p.y - 0.75, width: 1.5, height: 1.5))
            }
            context.setFillColor(Theme.modulation.withAlphaComponent(0.7).cgColor)
            context.fill(rects)
        }
        // Correlation from −1 to +1, and the balance from L to R.
        let rows = [("Correlation", "−1", "+1", reading.map { AnalyzerLayout.correlationX(Double($0.correlation), in: CGRect(x: box.minX + 92, y: 0, width: box.width - 170, height: 1)) },
                     reading.map { String(format: "%+.2f", $0.correlation) } ?? "–"),
                    ("Balance", "L", "R", reading.map { AnalyzerLayout.balanceX(Double($0.balanceDb), in: CGRect(x: box.minX + 92, y: 0, width: box.width - 170, height: 1)) },
                     reading.map { String(format: "%+.1f dB", $0.balanceDb) } ?? "–")]
        for (i, (label, low, high, x, value)) in rows.enumerated() {
            let y = scope.maxY + 6 + CGFloat(i) * 20
            let bar = CGRect(x: box.minX + 92, y: y + 4, width: box.width - 170, height: 6)
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

extension Data {
    /// The bytes as the little-endian 32-bit floats a reading carries its
    /// long lists as, in one copy.
    var floats: [Float] {
        let n = count / 4
        return [Float](unsafeUninitializedCapacity: n) { buffer, made in
            made = copyBytes(to: UnsafeMutableRawBufferPointer(buffer), count: n * 4) / 4
        }
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
    private static let font = NSFont.monospacedDigitSystemFont(ofSize: 9, weight: .regular)

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
        reading = model.analysis(effect: effect, frame: link.targetTimestamp)
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
        let columns = EqLayout.spectrumColumns(levels: reading.spectrum.floats, sampleRate: Double(reading.sampleRate), in: spectrum)
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
        _ = TextLines.shared.draw(label, in: CGRect(x: spectrum.minX + 2, y: spectrum.minY + 1, width: spectrum.width - 4, height: 12), font: Self.font, color: Theme.dimText, cuts: true)
    }
}
