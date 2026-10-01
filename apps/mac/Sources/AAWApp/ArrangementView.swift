import AAWCore
import AppKit
import SwiftUI

/// A number that eases to each new value instead of jumping.
struct Animated {
    private var from: Double
    private(set) var to: Double
    private var start: CFTimeInterval = 0
    private var duration: CFTimeInterval = 0

    init(_ value: Double) {
        from = value
        to = value
    }

    func value(at now: CFTimeInterval) -> Double {
        guard isRunning(at: now) else { return to }
        let t = max(0, (now - start) / duration)
        return from + (to - from) * (1 - pow(1 - t, 3))
    }

    func isRunning(at now: CFTimeInterval) -> Bool {
        duration > 0 && now < start + duration
    }

    mutating func move(to target: Double, at now: CFTimeInterval, over duration: CFTimeInterval) {
        guard target != to else { return }
        from = value(at: now)
        to = target
        start = now
        self.duration = duration
    }
}

enum RowID: Hashable {
    case track(UInt64)
    case bus(UInt64)
    case master
}

/// What can light up when a change touches it.
private enum GlowKey: Hashable {
    case row(RowID)
    case clip(UInt64)
    case section(UInt64)
}

private struct Glow {
    var start: CFTimeInterval
    var color: NSColor
}

private struct RowVisual {
    var name: String
    var detail: String
    var color: NSColor?
    var mute = false
    var solo = false
    var isTrack: Bool
    var hasPan: Bool
    var height: CGFloat
    var y: Animated
    var gain: Animated
    var pan: Animated
    var alpha: Animated
    var removing = false
}

private struct ClipVisual {
    var pattern: String
    var repeats: Int
    var color: NSColor
    var muted: Bool
    var at: Animated
    var length: Animated
    var y: Animated
    var alpha: Animated
    var removing = false
}

/// The arrangement: a ruler with the loop brace, sections and bars; a header
/// for each track, return and the master; and the tracks' clips on a timeline.
/// It draws the host's arrangement and eases to each new revision, lighting
/// up what an agent's or an external change touched. Headers are display only.
final class ArrangementView: NSView {
    private static let moveTime: CFTimeInterval = 0.3
    private static let glowTime: CFTimeInterval = 1.8

    private let model: SongModel
    private var layout = TimelineLayout()
    private var rows: [RowID: RowVisual] = [:]
    private var clips: [UInt64: ClipVisual] = [:]
    private var glows: [GlowKey: Glow] = [:]
    private var colors: [UInt64: Int] = [:]
    private var link: CADisplayLink?
    private let playheadView = PlayheadView()
    private var fitted = false
    /// A loop being dragged out in the ruler.
    private var loopDrag: (anchor: CGFloat, region: (start: Double, length: Double)?)?

    init(model: SongModel) {
        self.model = model
        super.init(frame: .zero)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
        addSubview(playheadView)
        playheadView.isHidden = true
        show(model.arrangement, animated: false)
        model.onUpdate = { [weak self] update in self?.apply(update) }
        model.onTransport = { [weak self] in self?.transportChanged() }
        model.onZoom = { [weak self] zoom in self?.zoom(zoom) }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var isFlipped: Bool { true }
    override var isOpaque: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        link?.invalidate()
        link = nil
        guard let window else { return }
        let link = displayLink(target: self, selector: #selector(tick(_:)))
        link.add(to: .main, forMode: .common)
        self.link = link
        window.makeFirstResponder(self)
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        layout.size = newSize
        if !fitted, newSize.width > TimelineLayout.headerWidth {
            fitted = true
            layout.fit()
        }
        layout.clamp()
        needsDisplay = true
    }

    // MARK: Revisions

    private func color(for track: UInt64) -> NSColor {
        let index = colors[track] ?? colors.count
        colors[track] = index
        return Theme.palette[index % Theme.palette.count]
    }

    private static func chain(_ effects: [EffectView]) -> String {
        effects.filter { !$0.bypass }.map(\.kind).joined(separator: " · ")
    }

    /// Takes a revision's arrangement as the target every row and clip eases to.
    private func show(_ a: Arrangement, animated: Bool) {
        let now = CACurrentMediaTime()
        let time = animated ? Self.moveTime : 0
        var liveRows = Set<RowID>()
        var liveClips = Set<UInt64>()
        var y: CGFloat = 0

        func place(_ id: RowID, name: String, detail: String, color: NSColor?, mute: Bool, solo: Bool,
                   gain: Double, pan: Double?, height: CGFloat) {
            liveRows.insert(id)
            if var row = rows[id], !row.removing {
                row.name = name
                row.detail = detail
                row.mute = mute
                row.solo = solo
                row.y.move(to: y, at: now, over: time)
                row.gain.move(to: gain, at: now, over: time)
                row.pan.move(to: pan ?? 0, at: now, over: time)
                rows[id] = row
            } else {
                var alpha = Animated(animated ? 0 : 1)
                alpha.move(to: 1, at: now, over: time)
                rows[id] = RowVisual(
                    name: name, detail: detail, color: color, mute: mute, solo: solo,
                    isTrack: color != nil, hasPan: pan != nil, height: height,
                    y: Animated(y), gain: Animated(gain), pan: Animated(pan ?? 0), alpha: alpha
                )
            }
        }

        for track in a.tracks {
            let color = color(for: track.key)
            place(.track(track.key), name: track.id, detail: Self.chain(track.effects), color: color,
                  mute: track.mute, solo: track.solo, gain: track.gainDb, pan: track.pan,
                  height: TimelineLayout.trackHeight)
            for clip in track.clips {
                liveClips.insert(clip.key)
                let length = clip.patternBeats * Double(clip.repeats)
                if var v = clips[clip.key], !v.removing {
                    v.pattern = clip.pattern
                    v.repeats = Int(clip.repeats)
                    v.color = color
                    v.muted = track.mute
                    v.at.move(to: clip.at, at: now, over: time)
                    v.length.move(to: length, at: now, over: time)
                    v.y.move(to: y, at: now, over: time)
                    clips[clip.key] = v
                } else {
                    var alpha = Animated(animated ? 0 : 1)
                    alpha.move(to: 1, at: now, over: time)
                    clips[clip.key] = ClipVisual(
                        pattern: clip.pattern, repeats: Int(clip.repeats), color: color, muted: track.mute,
                        at: Animated(clip.at), length: Animated(length), y: Animated(y), alpha: alpha
                    )
                }
            }
            y += TimelineLayout.trackHeight
        }
        y += TimelineLayout.busGap
        for bus in a.returns {
            place(.bus(bus.key), name: bus.id, detail: Self.chain(bus.effects), color: nil,
                  mute: bus.mute, solo: false, gain: bus.gainDb, pan: bus.pan, height: TimelineLayout.busHeight)
            y += TimelineLayout.busHeight
        }
        place(.master, name: "Master", detail: Self.chain(a.master.effects), color: nil,
              mute: false, solo: false, gain: a.master.gainDb, pan: nil, height: TimelineLayout.busHeight)
        y += TimelineLayout.busHeight

        // What is gone fades out, then is dropped.
        for id in rows.keys where !liveRows.contains(id) && rows[id]?.removing == false {
            rows[id]?.removing = true
            rows[id]?.alpha.move(to: 0, at: now, over: time)
        }
        for key in clips.keys where !liveClips.contains(key) && clips[key]?.removing == false {
            clips[key]?.removing = true
            clips[key]?.alpha.move(to: 0, at: now, over: time)
        }
        if !animated { dropRemoved(at: now) }

        layout.contentHeight = y
        layout.beatsPerBar = Double(a.beatsPerBar)
        layout.lengthBeats = a.lengthBeats
        layout.clamp()
        needsDisplay = true
    }

    private func apply(_ update: Update) {
        show(update.arrangement, animated: true)
        // The person watches their own edits happen; other changes are lit.
        if update.change.origin != .user {
            let glow = Glow(start: CACurrentMediaTime(), color: Theme.color(of: update.change.origin))
            for touch in update.touched {
                switch touch.part {
                case .track: glows[.row(.track(touch.key))] = glow
                case .return: glows[.row(.bus(touch.key))] = glow
                case .master: glows[.row(.master)] = glow
                case .clip: glows[.clip(touch.key)] = glow
                case .section: glows[.section(touch.key)] = glow
                case .session: break
                }
            }
        }
        link?.isPaused = false
    }

    private func transportChanged() {
        needsDisplay = true
        link?.isPaused = false
    }

    private func dropRemoved(at now: CFTimeInterval) {
        rows = rows.filter { !($0.value.removing && !$0.value.alpha.isRunning(at: now)) }
        clips = clips.filter { !($0.value.removing && !$0.value.alpha.isRunning(at: now)) }
    }

    // MARK: Frames

    @objc private func tick(_ link: CADisplayLink) {
        let now = CACurrentMediaTime()
        dropRemoved(at: now)
        glows = glows.filter { now - $0.value.start < Self.glowTime }
        var busy = !glows.isEmpty
            || rows.values.contains { $0.y.isRunning(at: now) || $0.gain.isRunning(at: now) || $0.pan.isRunning(at: now) || $0.alpha.isRunning(at: now) }
            || clips.values.contains { $0.at.isRunning(at: now) || $0.length.isRunning(at: now) || $0.y.isRunning(at: now) || $0.alpha.isRunning(at: now) }
        if busy { needsDisplay = true }

        let playhead = model.playhead()
        if let playhead, playhead.playing {
            busy = true
            let before = layout.scroll
            layout.reveal(playhead.beat)
            if layout.scroll != before { needsDisplay = true }
            placePlayhead(at: playhead.beat)
            let shown = (playhead.beat * 4).rounded(.down) / 4
            if model.position != shown { model.position = shown }
        } else {
            playheadView.isHidden = true
            // Play was asked for and the audio thread has not started yet.
            if model.transport.playing { busy = true }
        }
        if !busy {
            needsDisplay = true
            link.isPaused = true
        }
    }

    private func placePlayhead(at beat: Double) {
        let x = layout.x(beat)
        let top = TimelineLayout.loopStrip + TimelineLayout.sectionStrip
        playheadView.isHidden = x < TimelineLayout.headerWidth || x > bounds.width
        playheadView.frame = CGRect(
            x: x.rounded() - PlayheadView.halfWidth, y: top,
            width: PlayheadView.halfWidth * 2 + 1, height: bounds.height - top
        )
    }

    // MARK: Zoom and scroll

    func zoom(_ zoom: Zoom) {
        let center = TimelineLayout.headerWidth + layout.lanesWidth / 2
        switch zoom {
        case .in: layout.zoom(by: 1.4, anchorX: center)
        case .out: layout.zoom(by: 1 / 1.4, anchorX: center)
        case .fit: layout.fit()
        }
        needsDisplay = true
    }

    override func scrollWheel(with event: NSEvent) {
        if event.modifierFlags.contains(.command) || event.modifierFlags.contains(.option) {
            let x = convert(event.locationInWindow, from: nil).x
            layout.zoom(by: exp(event.scrollingDeltaY * 0.01), anchorX: max(x, TimelineLayout.headerWidth))
        } else {
            layout.scroll.x -= event.scrollingDeltaX
            layout.scroll.y -= event.scrollingDeltaY
            layout.clamp()
        }
        needsDisplay = true
    }

    override func magnify(with event: NSEvent) {
        let x = convert(event.locationInWindow, from: nil).x
        layout.zoom(by: 1 + event.magnification, anchorX: max(x, TimelineLayout.headerWidth))
        needsDisplay = true
    }

    // MARK: Mouse and keys

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        let p = convert(event.locationInWindow, from: nil)
        guard p.x >= TimelineLayout.headerWidth else { return }
        if p.y < TimelineLayout.loopStrip {
            loopDrag = (p.x, nil)
        } else {
            model.locate(layout.target(atX: p.x, free: event.modifierFlags.contains(.option)))
        }
    }

    override func mouseDragged(with event: NSEvent) {
        guard let drag = loopDrag else { return }
        let x = max(convert(event.locationInWindow, from: nil).x, TimelineLayout.headerWidth)
        if abs(x - drag.anchor) >= 3 || drag.region != nil {
            loopDrag?.region = layout.loop(fromX: drag.anchor, toX: x)
            needsDisplay = true
        }
    }

    override func mouseUp(with event: NSEvent) {
        guard let drag = loopDrag else { return }
        loopDrag = nil
        if let region = drag.region {
            model.setLoop(start: region.start, length: region.length)
        } else {
            // A click in the loop strip sets the start position like any other.
            model.locate(layout.target(atX: drag.anchor, free: event.modifierFlags.contains(.option)))
        }
        needsDisplay = true
    }

    override func keyDown(with event: NSEvent) {
        // The Transport menu has these keys; this is for when it does not act.
        switch event.charactersIgnoringModifiers {
        case " ": model.togglePlay()
        case "\r": model.returnToStart()
        case "l": model.toggleLoop()
        default: super.keyDown(with: event)
        }
    }

    // MARK: Drawing

    private static let nameFont = NSFont.systemFont(ofSize: 12, weight: .semibold)
    private static let smallFont = NSFont.systemFont(ofSize: 10, weight: .regular)
    private static let numberFont = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
    private static let clipFont = NSFont.systemFont(ofSize: 10, weight: .medium)
    private static let badgeFont = NSFont.systemFont(ofSize: 9, weight: .bold)

    private static func paragraph(_ alignment: NSTextAlignment) -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        style.alignment = alignment
        return style
    }

    private static let styles: [NSTextAlignment: NSParagraphStyle] = [
        .left: paragraph(.left), .right: paragraph(.right), .center: paragraph(.center),
    ]

    private func text(_ string: String, in rect: CGRect, font: NSFont, color: NSColor, align: NSTextAlignment = .left) {
        guard rect.width > 4 else { return }
        (string as NSString).draw(
            with: rect, options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine],
            attributes: [.font: font, .foregroundColor: color, .paragraphStyle: Self.styles[align]!]
        )
    }

    private func fill(_ rect: CGRect, _ color: NSColor) {
        color.setFill()
        rect.fill(using: .sourceOver)
    }

    private func clipped(to rect: CGRect, _ body: () -> Void) {
        NSGraphicsContext.saveGraphicsState()
        NSBezierPath(rect: rect).addClip()
        body()
        NSGraphicsContext.restoreGraphicsState()
    }

    /// How lit a touched object still is, from 1 down to 0.
    private func glow(_ key: GlowKey, at now: CFTimeInterval) -> (level: CGFloat, color: NSColor)? {
        guard let g = glows[key] else { return nil }
        let level = 1 - (now - g.start) / Self.glowTime
        return level > 0 ? (CGFloat(level), g.color) : nil
    }

    private var loopShown: (region: LoopRegion, active: Bool)? {
        if let region = loopDrag?.region {
            return (LoopRegion(start: region.start, length: region.length), true)
        }
        if let region = model.transport.loopRegion { return (region, true) }
        return model.idleLoop.map { ($0, false) }
    }

    override func draw(_ dirtyRect: NSRect) {
        let now = CACurrentMediaTime()
        let header = TimelineLayout.headerWidth
        let ruler = TimelineLayout.rulerHeight
        fill(bounds, Theme.background)
        let order = rows.sorted { $0.value.y.to < $1.value.y.to }

        clipped(to: CGRect(x: header, y: ruler, width: bounds.width - header, height: bounds.height - ruler)) {
            drawLanes(order, at: now)
        }
        clipped(to: CGRect(x: 0, y: ruler, width: header, height: bounds.height - ruler)) {
            for (id, row) in order { drawHeader(id, row, at: now) }
        }
        clipped(to: CGRect(x: header, y: 0, width: bounds.width - header, height: ruler)) {
            drawRuler(at: now)
        }
        fill(CGRect(x: 0, y: 0, width: header, height: ruler), Theme.ruler)
        text("\(model.arrangement.tracks.count) tracks",
             in: CGRect(x: 12, y: ruler - 17, width: header - 24, height: 14), font: Self.smallFont, color: Theme.faintText)
        fill(CGRect(x: header - 1, y: 0, width: 1, height: bounds.height), Theme.separator)
        fill(CGRect(x: 0, y: ruler - 1, width: bounds.width, height: 1), Theme.separator)
    }

    private func drawLanes(_ order: [(key: RowID, value: RowVisual)], at now: CFTimeInterval) {
        let header = TimelineLayout.headerWidth
        let ruler = TimelineLayout.rulerHeight
        let width = bounds.width - header
        for (_, row) in order {
            let y = layout.y(CGFloat(row.y.value(at: now)))
            let alpha = CGFloat(row.alpha.value(at: now))
            fill(CGRect(x: header, y: y, width: width, height: row.height - 1),
                 (row.isTrack ? Theme.lane : Theme.busLane).withAlphaComponent(alpha))
        }

        // Grid: every bar, and finer lines once they have room.
        let grid = layout.grid
        let bar = layout.beatsPerBar
        let first = max(0, (layout.beat(atX: header) / grid).rounded(.down))
        let last = (layout.beat(atX: bounds.width) / grid).rounded(.up)
        if last >= first {
            for k in Int(first)...Int(last) {
                let beat = Double(k) * grid
                let onBar = beat.truncatingRemainder(dividingBy: bar) == 0
                fill(CGRect(x: layout.x(beat).rounded(), y: ruler, width: 1, height: bounds.height - ruler),
                     onBar ? Theme.barLine : Theme.gridLine)
            }
        }
        if let (region, active) = loopShown, active {
            fill(CGRect(x: layout.x(region.start), y: ruler,
                        width: CGFloat(region.length) * layout.pixelsPerBeat, height: bounds.height - ruler),
                 Theme.gray(1, 0.035))
        }

        for (key, clip) in clips.sorted(by: { $0.key < $1.key }) {
            drawClip(key, clip, at: now)
        }

        let end = layout.x(layout.lengthBeats)
        if end < bounds.width {
            fill(CGRect(x: end, y: ruler, width: bounds.width - end, height: bounds.height - ruler), Theme.pastEnd)
            fill(CGRect(x: end.rounded(), y: ruler, width: 1, height: bounds.height - ruler), Theme.gray(1, 0.25))
        }
        fill(CGRect(x: layout.x(model.transport.cue).rounded(), y: ruler, width: 1, height: bounds.height - ruler),
             Theme.cue.withAlphaComponent(0.85))
    }

    private func drawClip(_ key: UInt64, _ clip: ClipVisual, at now: CFTimeInterval) {
        let at = clip.at.value(at: now)
        let length = clip.length.value(at: now)
        let alpha = CGFloat(clip.alpha.value(at: now))
        let rect = CGRect(
            x: layout.x(at), y: layout.y(CGFloat(clip.y.value(at: now))) + 2,
            width: max(2, CGFloat(length) * layout.pixelsPerBeat - 1), height: TimelineLayout.trackHeight - 5
        )
        guard rect.maxX >= TimelineLayout.headerWidth, rect.minX <= bounds.width else { return }
        let color = clip.muted ? Theme.gray(0.45) : clip.color
        let shape = NSBezierPath(roundedRect: rect, xRadius: 3, yRadius: 3)
        color.withAlphaComponent(0.42 * alpha).setFill()
        shape.fill()
        clipped(to: rect) {
            // A title strip in the track's color over a dimmer body.
            let title = CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: 14)
            NSGraphicsContext.saveGraphicsState()
            shape.addClip()
            fill(title, color.withAlphaComponent(alpha))
            // Each repeat of the pattern.
            if clip.repeats > 1, rect.width / CGFloat(clip.repeats) >= 4 {
                for i in 1..<clip.repeats {
                    let x = rect.minX + rect.width * CGFloat(i) / CGFloat(clip.repeats)
                    fill(CGRect(x: x.rounded(), y: title.maxY, width: 1, height: rect.height - title.height),
                         Theme.gray(0, 0.35 * alpha))
                }
            }
            NSGraphicsContext.restoreGraphicsState()
            let label = clip.repeats > 1 ? "\(clip.pattern) ×\(clip.repeats)" : clip.pattern
            text(label, in: CGRect(x: rect.minX + 4, y: rect.minY, width: rect.width - 6, height: 13),
                 font: Self.clipFont, color: Theme.gray(0.08, alpha))
        }
        if let (level, tint) = glow(.clip(key), at: now) {
            tint.withAlphaComponent(0.3 * level).setFill()
            shape.fill()
            tint.withAlphaComponent(level).setStroke()
            let outline = NSBezierPath(roundedRect: rect.insetBy(dx: 1, dy: 1), xRadius: 3, yRadius: 3)
            outline.lineWidth = 2
            outline.stroke()
        }
    }

    private func drawHeader(_ id: RowID, _ row: RowVisual, at now: CFTimeInterval) {
        let width = TimelineLayout.headerWidth - 1
        let y = layout.y(CGFloat(row.y.value(at: now)))
        let alpha = CGFloat(row.alpha.value(at: now))
        let rect = CGRect(x: 0, y: y, width: width, height: row.height - 1)
        guard rect.maxY >= TimelineLayout.rulerHeight, rect.minY <= bounds.height else { return }
        fill(rect, (row.isTrack ? Theme.header : Theme.busHeader).withAlphaComponent(alpha))
        if let (level, tint) = glow(.row(id), at: now) {
            fill(rect, tint.withAlphaComponent(0.35 * level))
        }
        if let color = row.color {
            fill(CGRect(x: 0, y: y, width: 5, height: row.height - 1), (row.mute ? Theme.gray(0.45) : color).withAlphaComponent(alpha))
        }
        let left: CGFloat = 14
        let gain = row.gain.value(at: now)
        let gainText = String(format: "%+.1f dB", gain).replacingOccurrences(of: "-", with: "−")
        let textColor = (row.mute ? Theme.dimText : Theme.text).withAlphaComponent(alpha)
        let dim = Theme.dimText.withAlphaComponent(alpha)

        // The name, then the effect chain for as far as there is room.
        func title(width: CGFloat, y: CGFloat) {
            let name = NSMutableAttributedString(
                string: row.name, attributes: [.font: Self.nameFont, .foregroundColor: textColor]
            )
            if !row.detail.isEmpty {
                name.append(NSAttributedString(
                    string: "  " + row.detail, attributes: [.font: Self.smallFont, .foregroundColor: dim]
                ))
            }
            name.addAttribute(.paragraphStyle, value: Self.styles[.left]!, range: NSRange(location: 0, length: name.length))
            name.draw(with: CGRect(x: left, y: y, width: width, height: 16),
                      options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        }

        guard row.isTrack else {
            // Returns and the master: one line.
            title(width: width - left - 8 - 64, y: y + 7)
            text(gainText, in: CGRect(x: width - 8 - 60, y: y + 9, width: 60, height: 13),
                 font: Self.numberFont, color: dim, align: .right)
            return
        }

        // Mute and solo, lit when on.
        let badges: [(String, Bool, NSColor)] = [("M", row.mute, Theme.mute), ("S", row.solo, Theme.solo)]
        var badgeX = width - 8 - 18 * CGFloat(badges.count) - 2
        let nameWidth = badgeX - left - 6
        for (letter, on, color) in badges {
            let box = CGRect(x: badgeX, y: y + 6, width: 16, height: 14)
            let path = NSBezierPath(roundedRect: box, xRadius: 3, yRadius: 3)
            (on ? color : Theme.gray(1)).withAlphaComponent((on ? 1 : 0.07) * alpha).setFill()
            path.fill()
            text(letter, in: CGRect(x: box.minX, y: box.minY + 1.5, width: box.width, height: 11),
                 font: Self.badgeFont, color: (on ? Theme.gray(0.08) : Theme.faintText).withAlphaComponent(alpha), align: .center)
            badgeX += 18
        }
        title(width: nameWidth, y: y + 5)

        // Volume as a bar from −60 to +6 dB, with the value and the pan.
        let bar = CGRect(x: left, y: y + 30, width: 84, height: 4)
        fill(bar, Theme.gray(0, 0.4 * alpha))
        let level = CGFloat(min(max((gain + 60) / 66, 0), 1))
        fill(CGRect(x: bar.minX, y: bar.minY, width: bar.width * level, height: bar.height),
             (row.mute ? Theme.gray(0.5) : Theme.gray(0.82)).withAlphaComponent(alpha))
        text(gainText, in: CGRect(x: bar.maxX + 6, y: y + 25, width: 56, height: 13), font: Self.numberFont, color: dim)
        if row.hasPan {
            let pan = row.pan.value(at: now)
            let percent = Int((abs(pan) * 100).rounded())
            let panText = percent == 0 ? "C" : (pan < 0 ? "L\(percent)" : "R\(percent)")
            text(panText, in: CGRect(x: width - 8 - 34, y: y + 25, width: 34, height: 13),
                 font: Self.numberFont, color: dim, align: .right)
        }
    }

    private func drawRuler(at now: CFTimeInterval) {
        let header = TimelineLayout.headerWidth
        let loopStrip = TimelineLayout.loopStrip
        let sectionTop = loopStrip
        let barTop = loopStrip + TimelineLayout.sectionStrip
        let ruler = TimelineLayout.rulerHeight
        fill(CGRect(x: header, y: 0, width: bounds.width - header, height: ruler), Theme.ruler)
        fill(CGRect(x: header, y: 0, width: bounds.width - header, height: loopStrip), Theme.gray(0, 0.22))

        if let (region, active) = loopShown {
            let rect = CGRect(x: layout.x(region.start), y: 3,
                              width: max(3, CGFloat(region.length) * layout.pixelsPerBeat), height: loopStrip - 6)
            Theme.loop.withAlphaComponent(active ? 0.95 : 0.25).setFill()
            NSBezierPath(roundedRect: rect, xRadius: 2, yRadius: 2).fill()
        }

        for section in model.arrangement.sections {
            let rect = CGRect(x: layout.x(section.at) + 1, y: sectionTop + 2,
                              width: CGFloat(section.lengthBeats) * layout.pixelsPerBeat - 2,
                              height: TimelineLayout.sectionStrip - 4)
            guard rect.maxX >= header, rect.minX <= bounds.width, rect.width > 1 else { continue }
            // Opaque, so a section that overlaps another covers its name.
            let path = NSBezierPath(roundedRect: rect, xRadius: 2, yRadius: 2)
            Theme.section.setFill()
            path.fill()
            if let (level, tint) = glow(.section(section.key), at: now) {
                tint.withAlphaComponent(0.8 * level).setFill()
                path.fill()
            }
            clipped(to: rect) {
                // The name stays in view while its section is.
                let x = max(rect.minX, header) + 4
                text(section.id, in: CGRect(x: x, y: rect.minY, width: rect.maxX - x - 2, height: 13),
                     font: Self.smallFont, color: Theme.text)
            }
        }

        let bar = layout.beatsPerBar
        let step = layout.barLabelStep
        let firstBar = max(0, Int((layout.beat(atX: header) / bar).rounded(.down)))
        let lastBar = Int((layout.beat(atX: bounds.width) / bar).rounded(.up))
        if lastBar >= firstBar {
            for n in firstBar...lastBar {
                let x = layout.x(Double(n) * bar).rounded()
                let labeled = n % step == 0
                if labeled || CGFloat(bar) * layout.pixelsPerBeat >= 6 {
                    fill(CGRect(x: x, y: labeled ? barTop : ruler - 5, width: 1, height: labeled ? ruler - barTop : 5),
                         Theme.gray(1, labeled ? 0.3 : 0.16))
                }
                if labeled {
                    text("\(n + 1)", in: CGRect(x: x + 4, y: barTop + 2, width: 40, height: 13),
                         font: Self.numberFont, color: Theme.dimText)
                }
            }
        }

        // The start position: where space plays from.
        let cue = layout.x(model.transport.cue).rounded() + 0.5
        let flag = NSBezierPath()
        flag.move(to: CGPoint(x: cue - 5, y: barTop))
        flag.line(to: CGPoint(x: cue + 5, y: barTop))
        flag.line(to: CGPoint(x: cue, y: barTop + 7))
        flag.close()
        Theme.cue.setFill()
        flag.fill()
        fill(CGRect(x: cue - 0.5, y: barTop, width: 1, height: ruler - barTop), Theme.cue)
    }
}

/// The playhead: a line over the lanes with a marker in the ruler. A view of
/// its own, so moving it each frame redraws nothing else.
private final class PlayheadView: NSView {
    static let halfWidth: CGFloat = 5

    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func draw(_ dirtyRect: NSRect) {
        let x = Self.halfWidth
        Theme.playhead.setFill()
        let head = NSBezierPath()
        head.move(to: CGPoint(x: 0, y: 0))
        head.line(to: CGPoint(x: x * 2 + 1, y: 0))
        head.line(to: CGPoint(x: x + 0.5, y: 8))
        head.close()
        head.fill()
        CGRect(x: x, y: 0, width: 1, height: bounds.height).fill()
    }
}

enum Zoom {
    case `in`, out, fit
}

/// The arrangement view in SwiftUI.
struct ArrangementPane: NSViewRepresentable {
    let model: SongModel

    func makeNSView(context: Context) -> ArrangementView {
        ArrangementView(model: model)
    }

    func updateNSView(_ view: ArrangementView, context: Context) {}
}
