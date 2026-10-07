import AAWCore
import AppKit
import SwiftUI

/// The pattern editor: the pattern of the clip last selected, with a row for
/// each pad of the clip's track. A pad whose hits are steps has a grid of
/// them to turn on and off; a pad that is held, or that has a pitch, has its
/// events as bars to add, move and stretch, by note where it has a pitch.
/// The person edits here and each edit is a command to the host, as an
/// agent's `daw pattern steps` and `daw pattern event` are. Every clip that
/// plays the pattern plays the change.
final class PatternEditor: NSView {
    private enum Drag {
        /// Steps being set along a row from the first one pressed: on where
        /// that one was off, else off. `was` is that step's level before.
        case paint(row: Int, level: Int, steps: Set<Int>, origin: CGPoint, was: Int)
        /// A step's level being dragged up or down.
        case level(row: Int, step: Int, from: Int, originY: CGFloat, level: Int)
        /// The selected events being moved, or with Option copied, by the
        /// one grabbed. `collapse` is the event to select alone if the press
        /// turns out to be a click.
        case move(events: [EventView], grabbed: EventView, origin: CGPoint, steps: Int, by: Double, semitones: Int, moved: Bool, collapse: UInt64?)
        /// An event's end being dragged to a line of the grid.
        case stretch(event: EventView, row: Int, line: Int)
        /// A rectangle that selects the events it touches, with those
        /// selected before it when Shift was held.
        case marquee(origin: CGPoint, now: CGPoint, base: Set<UInt64>)
    }

    /// An event shown ahead of the host, where a drag has it.
    private struct HeldEvent {
        var at: Double
        var pitch: Int?
        var duration: Double?
    }

    private let model: SongModel
    private var context: PatternContext?
    private var layout = PatternLayout()
    private var color = Theme.palette[0]
    /// The pattern the zoom was last fitted to.
    private var fitted: String?
    private var drag: Drag?
    /// Steps of a pad shown ahead of the host: each one's level.
    private var heldSteps: (pad: String, levels: [Int: Int])?
    /// Events shown ahead of the host, by key.
    private var held: [UInt64: HeldEvent] = [:]
    /// Where an Option-drag puts its copies: each one's row and place.
    private var ghosts: [(row: Int, event: HeldEvent)] = []
    private var holds = 0
    private var link: CADisplayLink?
    private let playhead = NSView()

    init(model: SongModel) {
        self.model = model
        super.init(frame: .zero)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
        playhead.wantsLayer = true
        playhead.layer?.backgroundColor = Theme.playhead.cgColor
        playhead.isHidden = true
        addSubview(playhead)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var isFlipped: Bool { true }
    override var isOpaque: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        link?.invalidate()
        link = nil
        guard window != nil else { return }
        let link = displayLink(target: self, selector: #selector(tick(_:)))
        link.add(to: .main, forMode: .common)
        link.isPaused = !model.transport.playing
        self.link = link
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        relayout()
        needsDisplay = true
    }

    // MARK: What is shown

    /// Takes the pattern to show, as the host has it now.
    func show(_ context: PatternContext, color: NSColor, playing: Bool) {
        let changed = context.pattern != self.context?.pattern || context.track.pads != self.context?.track.pads
        // A drag holds its row and its pattern by their places: when the rows
        // or the pattern are others, as after an agent's edit, it is dropped.
        if context.pattern.name != self.context?.pattern.name || context.track.pads.map(\.name) != self.context?.track.pads.map(\.name) {
            drag = nil
        }
        self.context = context
        self.color = color
        // What was shown ahead of the host has arrived, or was refused.
        if changed, drag == nil {
            heldSteps = nil
            held = [:]
            ghosts = []
        }
        relayout()
        link?.isPaused = !playing
        if !playing { playhead.isHidden = true }
        needsDisplay = true
    }

    private func relayout() {
        guard let context else { return }
        layout.size = bounds.size
        layout.lengthBeats = context.pattern.lengthBeats
        layout.grid = context.pattern.grid
        layout.setPads(context.track.pads.map { pad in
            PatternLayout.Pad(
                name: pad.name, gate: pad.gate, root: pad.root.map { Int($0) },
                pitches: context.pattern.events.filter { $0.pad == pad.name }.compactMap { $0.pitch.map { Int($0) } }
            )
        })
        if fitted != context.pattern.name, layout.lanesWidth > 0 {
            fitted = context.pattern.name
            layout.fit()
        }
    }

    /// A step's level in a pad's row: what the person holds it at, or the song's.
    private func level(_ pad: String, _ step: Int) -> Int {
        if let held = heldSteps, held.pad == pad, let level = held.levels[step] { return level }
        guard let cells = context?.pattern.rows.first(where: { $0.pad == pad })?.cells, step < cells.count else { return 0 }
        return Int(cells[cells.startIndex + step])
    }

    /// The pattern's events with where each is drawn: the song's place, or
    /// the person's while they hold one.
    private func events() -> [(event: EventView, row: Int, rect: CGRect)] {
        guard let context else { return [] }
        return context.pattern.events.compactMap { event in
            guard let row = layout.rows.firstIndex(where: { $0.pad.name == event.pad }) else { return nil }
            let held = self.held[event.key]
            let rect = layout.event(row: row, at: held?.at ?? event.at, duration: held.map(\.duration) ?? event.duration,
                                    pitch: held.map(\.pitch) ?? event.pitch.map { Int($0) })
            return (event, row, rect)
        }
    }

    /// The event at a point: the one drawn on top, where events overlap.
    private func event(at p: CGPoint) -> (event: EventView, row: Int, rect: CGRect)? {
        events().last { $0.rect.insetBy(dx: -2, dy: -1).contains(p) }
    }

    // MARK: Frames

    @objc private func tick(_ link: CADisplayLink) {
        // The playhead, while the clip whose pattern this is plays.
        guard let context, let head = model.playhead(), head.playing else {
            playhead.isHidden = true
            if !model.transport.playing { link.isPaused = true }
            return
        }
        let clip = context.clip
        let into = head.beat - clip.at
        guard clip.patternBeats > 0, into >= 0, into < clip.patternBeats * Double(clip.repeats) else {
            playhead.isHidden = true
            return
        }
        let x = layout.x(into.truncatingRemainder(dividingBy: clip.patternBeats)).rounded()
        playhead.isHidden = x < PatternLayout.gutter || x > bounds.width
        playhead.frame = CGRect(x: x, y: 0, width: 1, height: bounds.height)
    }

    // MARK: Zoom and scroll

    override func scrollWheel(with event: NSEvent) {
        if event.modifierFlags.contains(.command) || event.modifierFlags.contains(.option) {
            let x = convert(event.locationInWindow, from: nil).x
            layout.zoom(by: exp(event.scrollingDeltaY * 0.01), anchorX: max(x, PatternLayout.gutter))
        } else {
            layout.scroll.x -= event.scrollingDeltaX
            layout.scroll.y -= event.scrollingDeltaY
            layout.clamp()
        }
        needsDisplay = true
    }

    override func magnify(with event: NSEvent) {
        let x = convert(event.locationInWindow, from: nil).x
        layout.zoom(by: 1 + event.magnification, anchorX: max(x, PatternLayout.gutter))
        needsDisplay = true
    }

    // MARK: Mouse

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        guard let context else { return }
        let p = convert(event.locationInWindow, from: nil)
        guard p.x >= PatternLayout.gutter, p.y >= PatternLayout.rulerHeight, let index = layout.row(atY: p.y),
              layout.beat(atX: p.x) < layout.lengthBeats else {
            model.select(events: [])
            return
        }
        let row = layout.rows[index]
        let name = context.pattern.name
        let shift = event.modifierFlags.contains(.shift)
        if let hit = self.event(at: p) {
            let key = hit.event.key
            if event.clickCount == 2 {
                model.edit(.eventsRemove(events: [key]))
                return
            }
            var selection = model.selectedEvents
            var collapse: UInt64?
            if shift {
                // Shift adds an event to the selection, or takes it out.
                if selection.remove(key) == nil { selection.insert(key) }
                guard selection.contains(key) else {
                    model.select(events: selection)
                    needsDisplay = true
                    return
                }
            } else if !selection.contains(key) {
                selection = [key]
            } else if selection.count > 1 {
                collapse = key
            }
            // With Option held the press copies, whichever part of the event it is on.
            let copy = event.modifierFlags.contains(.option)
            if !copy, hit.event.duration != nil || layout.rows[hit.row].pad.gate, hit.rect.width >= 10, p.x >= hit.rect.maxX - 5 {
                // An end is of one event, which it selects alone.
                model.select(events: [key])
                drag = .stretch(event: hit.event, row: hit.row, line: layout.line(atX: hit.rect.maxX))
            } else {
                model.select(events: selection)
                let chosen = context.pattern.events.filter { selection.contains($0.key) }
                drag = .move(events: chosen, grabbed: hit.event, origin: p, steps: 0, by: 0, semitones: 0, moved: false, collapse: collapse)
            }
            needsDisplay = true
            return
        }
        let free = model.free(event.modifierFlags)
        if row.kind == .steps, !free {
            model.select(events: [])
            // A step goes on or off at the press, and a drag takes the steps
            // it passes with it.
            let step = layout.step(atX: p.x)
            let was = level(row.pad.name, step)
            drag = .paint(row: index, level: was == 0 ? 10 : 0, steps: [step], origin: p, was: was)
            heldSteps = (row.pad.name, [step: was == 0 ? 10 : 0])
            needsDisplay = true
        } else if free || event.clickCount == 2 {
            // An event: where the pointer is with ⌘, else on the step
            // under it; at the note under it in a row of notes; and held for
            // a step on a pad that is held.
            let at = free ? layout.beat(atX: p.x) : Double(layout.step(atX: p.x)) * layout.grid
            let pitch = layout.pitch(atY: p.y, row: index).map { Int32($0) }
            model.edit(.eventAdd(pattern: name, pad: row.pad.name, at: at, free: free, pitch: pitch, steps: row.pad.gate ? 1 : 0)) { [weak self] made in
                self?.model.select(events: Set(made))
            }
        } else {
            // A click in the clear of a row of events: where pasted events
            // go, and a rectangle to select events with.
            model.eventInsert = Double(layout.step(atX: p.x)) * layout.grid
            if !shift { model.select(events: []) }
            drag = .marquee(origin: p, now: p, base: shift ? model.selectedEvents : [])
            needsDisplay = true
        }
    }

    override func mouseDragged(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        switch drag {
        case .paint(let index, let level, _, let origin, let was):
            let pad = layout.rows[index].pad.name
            let (dx, dy) = (p.x - origin.x, p.y - origin.y)
            let first = layout.step(atX: origin.x)
            if was != 0, abs(dy) >= 5, abs(dy) > abs(dx), layout.step(atX: p.x) == first {
                // Up or down on a step that was on: how hard it plays.
                let level = PatternLayout.level(from: was, draggedBy: dy)
                drag = .level(row: index, step: first, from: was, originY: origin.y, level: level)
                heldSteps = (pad, [first: level])
            } else {
                let now = layout.step(atX: p.x)
                let steps = Set(min(first, now)...max(first, now))
                drag = .paint(row: index, level: level, steps: steps, origin: origin, was: was)
                heldSteps = (pad, Dictionary(uniqueKeysWithValues: steps.map { ($0, level) }))
            }
        case .level(let index, let step, let from, let originY, _):
            let level = PatternLayout.level(from: from, draggedBy: p.y - originY)
            drag = .level(row: index, step: step, from: from, originY: originY, level: level)
            heldSteps = (layout.rows[index].pad.name, [step: level])
        case .move(let events, let grabbed, let origin, _, _, _, let moved, let collapse):
            let (dx, dy) = (p.x - origin.x, p.y - origin.y)
            if !moved, hypot(dx, dy) < 3 { return }
            // By whole steps, so that an event off the grid stays as far off
            // it; with ⌘, by thousandths of a beat. They stay in the pattern.
            let ats = events.map(\.at)
            var (steps, by) = (0, 0.0)
            if model.free(event.modifierFlags) {
                let range = layout.beatRange(of: ats)
                by = min(max((Double(dx / layout.pixelsPerBeat) * 1000).rounded() / 1000, range.lowerBound), range.upperBound)
            } else {
                let range = layout.stepRange(of: ats)
                steps = min(max(layout.steps(forDrag: dx), range.lowerBound), range.upperBound)
            }
            // In rows of notes, up and down by notes, as the grabbed one's row has them.
            let rowOf = { (e: EventView) in self.layout.rows.firstIndex { $0.pad.name == e.pad } }
            let pitchOf = { (e: EventView, row: Int) -> Int? in
                guard case .notes = self.layout.rows[row].kind else { return nil }
                return e.pitch.map { Int($0) } ?? self.layout.rows[row].pad.root
            }
            var semitones = 0
            if let row = rowOf(grabbed), case .notes = layout.rows[row].kind {
                let pitched = events.compactMap { e in rowOf(e).flatMap { r in pitchOf(e, r).map { (row: r, pitch: $0) } } }
                let range = layout.semitoneRange(of: pitched)
                semitones = min(max(-Int((dy / layout.rows[row].semitone).rounded()), range.lowerBound), range.upperBound)
            }
            let shift = Double(steps) * layout.grid + by
            let placed: [(key: UInt64, row: Int, event: HeldEvent)] = events.compactMap { e in
                guard let row = rowOf(e) else { return nil }
                let pitch = pitchOf(e, row).map { $0 + semitones } ?? e.pitch.map { Int($0) }
                return (e.key, row, HeldEvent(at: e.at + shift, pitch: semitones == 0 ? e.pitch.map { Int($0) } : pitch, duration: e.duration))
            }
            // With Option, copies go there and the events stay where they are.
            if event.modifierFlags.contains(.option) {
                held = [:]
                ghosts = placed.map { ($0.row, $0.event) }
            } else {
                ghosts = []
                held = Dictionary(uniqueKeysWithValues: placed.map { ($0.key, $0.event) })
            }
            drag = .move(events: events, grabbed: grabbed, origin: origin, steps: steps, by: by, semitones: semitones, moved: true, collapse: collapse)
        case .stretch(let e, let index, _):
            let first = Int((e.at / layout.grid + 1e-9).rounded(.down)) + 1
            let line = max(first, layout.line(atX: p.x))
            drag = .stretch(event: e, row: index, line: line)
            held = [e.key: HeldEvent(at: e.at, pitch: e.pitch.map { Int($0) }, duration: Double(line) * layout.grid - e.at)]
        case .marquee(let origin, _, let base):
            drag = .marquee(origin: origin, now: p, base: base)
            let box = CGRect(x: min(origin.x, p.x), y: min(origin.y, p.y), width: abs(p.x - origin.x), height: abs(p.y - origin.y))
            model.select(events: base.union(events().filter { $0.rect.intersects(box) }.map(\.event.key)))
        case nil:
            return
        }
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard let context, let ended = drag else { return }
        drag = nil
        let name = context.pattern.name
        switch ended {
        case .paint(let index, let level, let steps, _, _):
            model.edit(.steps(pattern: name, pad: layout.rows[index].pad.name, steps: steps.sorted().map { UInt32($0) }, level: UInt32(level)))
            hold()
        case .level(let index, let step, let from, _, let level):
            if level != from {
                model.edit(.steps(pattern: name, pad: layout.rows[index].pad.name, steps: [UInt32(step)], level: UInt32(level)))
                hold()
            } else {
                heldSteps = nil
            }
        case .move(let events, _, _, let steps, let by, let semitones, let moved, let collapse):
            if moved, steps != 0 || by != 0 || semitones != 0 {
                let keys = events.map(\.key)
                if event.modifierFlags.contains(.option) {
                    model.edit(.eventsCopy(events: keys, steps: Int32(steps), by: by, semitones: Int32(semitones))) { [weak self] made in
                        self?.model.select(events: Set(made))
                    }
                } else {
                    model.edit(.eventsMove(events: keys, steps: Int32(steps), by: by, semitones: Int32(semitones)))
                }
                hold()
            } else {
                held = [:]
                ghosts = []
                if !moved, let collapse { model.select(events: [collapse]) }
            }
        case .stretch(let e, _, let line):
            if !held.isEmpty {
                model.edit(.eventEnd(event: e.key, step: UInt32(line)))
                hold()
            }
        case .marquee:
            break
        }
        needsDisplay = true
    }

    /// Keeps what a drag left shown until the host's song has it, or a second
    /// passes, as when the host refuses it.
    private func hold() {
        holds += 1
        let mine = holds
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            MainActor.assumeIsolated {
                guard let self, self.holds == mine, self.drag == nil else { return }
                self.heldSteps = nil
                self.held = [:]
                self.ghosts = []
                self.needsDisplay = true
            }
        }
    }

    // MARK: Keys

    override func keyDown(with event: NSEvent) {
        guard event.modifierFlags.intersection([.command, .control]).isEmpty else {
            super.keyDown(with: event)
            return
        }
        // The menus have most of these keys; this is for when they do not act.
        let octave = event.modifierFlags.contains(.shift)
        switch event.specialKey {
        case .delete?, .backspace?, .deleteForward?:
            // Here, the selected events and nothing else: not the clip being edited.
            if model.selectedEvents.isEmpty { NSSound.beep() } else { model.deleteSelection() }
        case .leftArrow?: nudge(steps: -1, semitones: 0)
        case .rightArrow?: nudge(steps: 1, semitones: 0)
        case .upArrow?: nudge(steps: 0, semitones: octave ? 12 : 1)
        case .downArrow?: nudge(steps: 0, semitones: octave ? -12 : -1)
        case .carriageReturn?, .enter?: model.returnToStart()
        default:
            switch event.charactersIgnoringModifiers {
            case " ": model.togglePlay()
            case "l": model.toggleLoop()
            case "\u{1b}": model.select(events: [])
            default: super.keyDown(with: event)
            }
        }
    }

    override func selectAll(_ sender: Any?) {
        model.select(events: Set(context?.pattern.events.map(\.key) ?? []))
        needsDisplay = true
    }

    /// Moves the selected events a step earlier or later, or a note up or down.
    private func nudge(steps: Int, semitones: Int) {
        let events = model.selectedEvents
        guard !events.isEmpty else {
            NSSound.beep()
            return
        }
        model.edit(.eventsMove(events: events.sorted(), steps: Int32(steps), by: 0, semitones: Int32(semitones)))
    }

    // MARK: Drawing

    private static let nameFont = NSFont.systemFont(ofSize: 11, weight: .semibold)
    private static let smallFont = NSFont.systemFont(ofSize: 9, weight: .regular)
    private static let numberFont = NSFont.monospacedDigitSystemFont(ofSize: 9, weight: .regular)
    private static let noteFont = NSFont.systemFont(ofSize: 8, weight: .medium)

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
        if TextLines.shared.draw(string, in: rect, font: font, color: color, align: align) { return }
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

    /// Whether a note is one of a piano's black keys.
    private static func black(_ note: Int) -> Bool {
        [1, 3, 6, 8, 10].contains(((note % 12) + 12) % 12)
    }

    override func draw(_ dirtyRect: NSRect) {
        let gutter = PatternLayout.gutter
        let ruler = PatternLayout.rulerHeight
        fill(bounds, Theme.gray(0.13))
        guard let context else { return }
        if layout.rows.isEmpty {
            text("\(context.track.id) has no pads to play. Drag a sample onto the track from the browser, or ask the agent for one.",
                 in: CGRect(x: gutter, y: ruler + 10, width: bounds.width - gutter - 12, height: 30),
                 font: NSFont.systemFont(ofSize: 11), color: Theme.dimText)
        }
        let lanes = CGRect(x: gutter, y: ruler, width: bounds.width - gutter, height: bounds.height - ruler)
        clipped(to: lanes) {
            drawRows()
            drawGrid(top: ruler)
            drawSteps()
            drawEvents()
            drawGhosts()
            drawInsert()
            drawMarquee()
            let end = layout.x(layout.lengthBeats)
            if end < bounds.width {
                fill(CGRect(x: end, y: ruler, width: bounds.width - end, height: bounds.height - ruler), Theme.gray(0.13))
            }
        }
        clipped(to: CGRect(x: 0, y: ruler, width: gutter, height: bounds.height - ruler)) { drawGutter() }
        clipped(to: CGRect(x: gutter, y: 0, width: bounds.width - gutter, height: ruler)) { drawRuler() }
        fill(CGRect(x: 0, y: 0, width: gutter, height: ruler), Theme.ruler)
        fill(CGRect(x: gutter - 1, y: 0, width: 1, height: bounds.height), Theme.separator)
        fill(CGRect(x: 0, y: ruler - 1, width: bounds.width, height: 1), Theme.separator)
    }

    private func drawRows() {
        for index in layout.rows.indices {
            let row = layout.rows[index]
            let frame = layout.frame(ofRow: index)
            guard frame.maxY >= PatternLayout.rulerHeight, frame.minY <= bounds.height else { continue }
            fill(CGRect(x: frame.minX, y: frame.minY, width: frame.width, height: frame.height - 1), Theme.lane)
            guard case .notes(let low, let high) = row.kind else { continue }
            // As a piano's keys: the black ones darker, and a line under each C.
            for note in low...high {
                let y = frame.minY + CGFloat(high - note) * row.semitone
                if Self.black(note) {
                    fill(CGRect(x: frame.minX, y: y, width: frame.width, height: row.semitone), Theme.gray(0, 0.16))
                }
                if note % 12 == 0 {
                    fill(CGRect(x: frame.minX, y: (y + row.semitone).rounded() - 1, width: frame.width, height: 1), Theme.gray(1, 0.1))
                }
            }
        }
    }

    /// A line at each step once they have room, at each beat and each bar.
    private func drawGrid(top: CGFloat) {
        let stepWidth = CGFloat(layout.grid) * layout.pixelsPerBeat
        let beatsPerStep = layout.grid
        let first = max(0, Int((layout.beat(atX: PatternLayout.gutter) / beatsPerStep).rounded(.down)))
        let last = min(layout.steps, Int((layout.beat(atX: bounds.width) / beatsPerStep).rounded(.up)))
        guard last >= first else { return }
        for step in first...last {
            let beat = Double(step) * beatsPerStep
            let onBeat = abs(beat - beat.rounded()) < 1e-9
            let onBar = onBeat && Int(beat.rounded()) % 4 == 0
            guard onBar || (onBeat && layout.pixelsPerBeat >= 6) || stepWidth >= 6 else { continue }
            fill(CGRect(x: layout.x(beat).rounded(), y: top, width: 1, height: bounds.height - top),
                 onBar ? Theme.gray(1, 0.2) : onBeat ? Theme.gray(1, 0.1) : Theme.gridLine)
        }
    }

    /// The steps of each row of steps: a cell for each, lit by how hard it plays.
    private func drawSteps() {
        let stepWidth = CGFloat(layout.grid) * layout.pixelsPerBeat
        let first = layout.step(atX: PatternLayout.gutter)
        let last = layout.step(atX: bounds.width)
        for index in layout.rows.indices where layout.rows[index].kind == .steps {
            let pad = layout.rows[index].pad.name
            let frame = layout.frame(ofRow: index)
            guard frame.maxY >= PatternLayout.rulerHeight, frame.minY <= bounds.height, last >= first else { continue }
            for step in first...last {
                let level = level(pad, step)
                let cell = layout.cell(row: index, step: step).insetBy(dx: min(1.5, stepWidth / 6), dy: 4)
                let shape = NSBezierPath(roundedRect: cell, xRadius: 2, yRadius: 2)
                guard level > 0 else {
                    if stepWidth >= 6 {
                        Theme.gray(1, 0.05).setFill()
                        shape.fill()
                    }
                    continue
                }
                color.withAlphaComponent(0.3 + 0.7 * CGFloat(PatternLayout.velocity(ofLevel: level)) / 127).setFill()
                shape.fill()
                // A digit says how hard, where an `x` does not.
                if level < 10, cell.width >= 12 {
                    text("\(level)", in: CGRect(x: cell.minX, y: cell.midY - 6, width: cell.width, height: 12),
                         font: Self.numberFont, color: Theme.gray(0.05, 0.85), align: .center)
                }
            }
        }
    }

    private func drawEvents() {
        for (event, index, rect) in events() {
            guard rect.maxX >= PatternLayout.gutter, rect.minX <= bounds.width,
                  rect.maxY >= PatternLayout.rulerHeight, rect.minY <= bounds.height else { continue }
            let row = layout.rows[index]
            let selected = model.selectedEvents.contains(event.key)
            let bar = rect.insetBy(dx: 0, dy: rect.height > 6 ? 0.5 : 0)
            let shape = NSBezierPath(roundedRect: bar, xRadius: 2, yRadius: 2)
            let lit = color.blended(withFraction: 0.25, of: .white) ?? color
            lit.withAlphaComponent(0.4 + 0.6 * CGFloat(event.velocity) / 127).setFill()
            shape.fill()
            (selected ? Theme.selectedClip : Theme.gray(0, 0.5)).setStroke()
            shape.lineWidth = selected ? 1.5 : 0.5
            shape.stroke()
            if case .notes = row.kind, bar.height >= 9, bar.width >= 22 {
                let held = self.held[event.key]?.pitch
                let name = held.map { noteName(midi: Int32($0)) } ?? event.note ?? ""
                text(name, in: CGRect(x: bar.minX + 3, y: bar.midY - 5.5, width: bar.width - 4, height: 11),
                     font: Self.noteFont, color: Theme.gray(0.05, 0.9))
            }
        }
    }

    /// Where an Option-drag puts its copies: outlined, over the events.
    private func drawGhosts() {
        for (row, g) in ghosts where layout.rows.indices.contains(row) {
            let bar = layout.event(row: row, at: g.at, duration: g.duration, pitch: g.pitch)
            let shape = NSBezierPath(roundedRect: bar.insetBy(dx: 0, dy: bar.height > 6 ? 0.5 : 0), xRadius: 2, yRadius: 2)
            color.withAlphaComponent(0.35).setFill()
            shape.fill()
            Theme.selectedClip.setStroke()
            shape.lineWidth = 1.5
            shape.stroke()
        }
    }

    /// Where pasted events go.
    private func drawInsert() {
        guard let at = model.eventInsert, model.selectedEvents.isEmpty, at < layout.lengthBeats else { return }
        fill(CGRect(x: layout.x(at).rounded(), y: PatternLayout.rulerHeight, width: 1, height: bounds.height), Theme.cue.withAlphaComponent(0.8))
    }

    private func drawMarquee() {
        guard case .marquee(let origin, let now, _) = drag, origin != now else { return }
        let box = CGRect(x: min(origin.x, now.x), y: min(origin.y, now.y), width: abs(now.x - origin.x), height: abs(now.y - origin.y))
        fill(box, Theme.gray(1, 0.08))
        Theme.gray(1, 0.6).setStroke()
        NSBezierPath(rect: box.insetBy(dx: 0.5, dy: 0.5)).stroke()
    }

    /// Each row's pad beside it, and beside a row of notes its Cs and its root.
    private func drawGutter() {
        for index in layout.rows.indices {
            let row = layout.rows[index]
            let frame = layout.frame(ofRow: index)
            guard frame.maxY >= PatternLayout.rulerHeight, frame.minY <= bounds.height else { continue }
            fill(CGRect(x: 0, y: frame.minY, width: PatternLayout.gutter - 1, height: frame.height - 1), Theme.header)
            fill(CGRect(x: 0, y: frame.minY, width: 4, height: frame.height - 1), color)
            let width = PatternLayout.gutter - 14
            text(row.pad.name, in: CGRect(x: 10, y: frame.minY + 4, width: width - (row.pad.gate ? 26 : 0), height: 14),
                 font: Self.nameFont, color: Theme.text)
            if row.pad.gate {
                text("held", in: CGRect(x: PatternLayout.gutter - 32, y: frame.minY + 6, width: 26, height: 11),
                     font: Self.smallFont, color: Theme.faintText, align: .right)
            }
            guard case .notes(let low, let high) = row.kind else { continue }
            for note in low...high where note % 12 == 0 || note == row.pad.root {
                let y = frame.minY + CGFloat(high - note) * row.semitone
                // Clear of the pad's name.
                guard y + row.semitone / 2 - 5.5 >= frame.minY + 18 else { continue }
                text(noteName(midi: Int32(note)), in: CGRect(x: 10, y: y + row.semitone / 2 - 5.5, width: width, height: 11),
                     font: Self.numberFont, color: note == row.pad.root ? color : Theme.dimText, align: .right)
            }
        }
    }

    /// Beats from the pattern's start, as bars and beats counted from one.
    private func drawRuler() {
        let gutter = PatternLayout.gutter
        let ruler = PatternLayout.rulerHeight
        fill(CGRect(x: gutter, y: 0, width: bounds.width - gutter, height: ruler), Theme.ruler)
        var every = 1
        while CGFloat(every) * layout.pixelsPerBeat < 26 { every *= 2 }
        let first = max(0, Int(layout.beat(atX: gutter).rounded(.down)))
        let last = min(Int(layout.lengthBeats.rounded(.up)), Int(layout.beat(atX: bounds.width).rounded(.up)))
        guard last >= first else { return }
        for beat in first...last where beat % every == 0 {
            let x = layout.x(Double(beat)).rounded()
            let onBar = beat % 4 == 0
            fill(CGRect(x: x, y: onBar ? 2 : ruler - 5, width: 1, height: onBar ? ruler - 2 : 5), Theme.gray(1, onBar ? 0.3 : 0.16))
            guard beat < Int(layout.lengthBeats.rounded(.up)) else { continue }
            let label = onBar ? "\(beat / 4 + 1)" : "\(beat / 4 + 1).\(beat % 4 + 1)"
            text(label, in: CGRect(x: x + 3, y: 2, width: 40, height: 12), font: Self.numberFont, color: onBar ? Theme.dimText : Theme.faintText)
        }
    }
}

/// The pattern editor in SwiftUI.
struct PatternPane: NSViewRepresentable {
    let model: SongModel
    let context: PatternContext
    let color: NSColor
    let playing: Bool
    /// The selected events, which the editor outlines.
    let selected: Set<UInt64>

    func makeNSView(context: Context) -> PatternEditor {
        PatternEditor(model: model)
    }

    func updateNSView(_ view: PatternEditor, context: Context) {
        view.show(self.context, color: color, playing: playing)
    }
}

/// Beside the pattern editor: the pattern's name, length, step and swing, and
/// the selected event's fields.
struct PatternHeader: View {
    let model: SongModel
    let context: PatternContext

    /// The steps a pattern can have, in beats, as the song writes them.
    static let grids = Grid.patternValues

    private var pattern: PatternView { context.pattern }

    private func row<Content: View>(_ label: String, @ViewBuilder _ content: () -> Content) -> some View {
        HStack(spacing: 4) {
            Text(label)
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .frame(width: 50, alignment: .leading)
            content()
        }
        .frame(height: 19)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                VStack(alignment: .leading, spacing: 1) {
                    Text(pattern.name).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                    Text(pattern.clips == 1 ? "Pattern of 1 clip on \(context.track.id)" : "Pattern of \(pattern.clips) clips")
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                if pattern.clips > 1 {
                    Button("Own Copy") {
                        model.edit(.clipOwnPattern(clip: context.clip.key))
                    }
                    .controlSize(.mini)
                    .help("Give this clip a copy of the pattern, so that editing it leaves the other clips as they are")
                }
            }
            row("Length") {
                TypedValue(text: pattern.lengthText, unit: "beats") { typed in
                    model.edit(.patternLength(pattern: pattern.name, beats: typed))
                } done: {
                    model.onFocus?()
                }
            }
            row("Step") {
                Picker("", selection: Binding(
                    get: { pattern.gridText },
                    set: { model.edit(.patternGrid(pattern: pattern.name, grid: $0)) }
                )) {
                    ForEach(Self.grids.contains(pattern.gridText) ? Self.grids : Self.grids + [pattern.gridText], id: \.self) {
                        Text(PianoRollLayout.noteValue($0)).tag($0)
                    }
                }
                .labelsHidden()
                .controlSize(.mini)
                .help("The pattern's steps, as note values: 1/16 is a sixteenth note and 1/8T an eighth-note triplet. ⌘1 and ⌘2 make them finer and coarser, ⌘3 triplets; with ⌘, an event goes off the grid")
            }
            row("Swing") {
                KnobBar(model: model, spec: BarSpec(value: pattern.swing * 100, min: 50, max: 75, unit: "%", initial: 50)) { [name = pattern.name] value in
                    .patternSwing(pattern: name, swing: (value * 10).rounded() / 1000)
                }
            }
            Divider().padding(.vertical, 2)
            let events = model.selectedEventViews
            if events.count == 1, let event = events.first {
                EventFields(model: model, event: event, pitched: context.track.pads.first { $0.name == event.pad }?.root != nil)
            } else if let first = events.first {
                Text("\(events.count) events").font(.system(size: 10, weight: .medium))
                row("Velocity") {
                    KnobBar(model: model, spec: BarSpec(value: Double(first.velocity), min: 1, max: 127, initial: 100, live: false, whole: true)) { [keys = events.map(\.key)] value in
                        .eventsSet(events: keys, at: nil, duration: nil, velocity: UInt32(value), transpose: nil, note: nil)
                    }
                }
            } else {
                Text("Click a step to turn it on or off. Double-click a row of held hits or of notes to add one on the step, or with ⌘ off the grid, a little behind the beat; drag around events to select them, and with Option to copy them. Step is the grid, as a note value.")
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
    }
}

/// The fields of the selected event.
private struct EventFields: View {
    let model: SongModel
    let event: EventView
    /// Whether the event's pad plays a sample with a root note.
    let pitched: Bool

    private func row<Content: View>(_ label: String, @ViewBuilder _ content: () -> Content) -> some View {
        HStack(spacing: 4) {
            Text(label)
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .frame(width: 50, alignment: .leading)
            content()
        }
        .frame(height: 19)
    }

    private func set(at: String? = nil, duration: String? = nil, velocity: UInt32? = nil, note: String? = nil) -> Edit {
        .eventsSet(events: [event.key], at: at, duration: duration, velocity: velocity, transpose: nil, note: note)
    }

    var body: some View {
        row("Velocity") {
            KnobBar(model: model, spec: BarSpec(value: Double(event.velocity), min: 1, max: 127, initial: 100, whole: true)) { [event] value in
                .eventsSet(events: [event.key], at: nil, duration: nil, velocity: UInt32(value), transpose: nil, note: nil)
            }
        }
        // Where it starts in the pattern, and how long a held pad is held, in beats.
        row("At") {
            TypedValue(text: event.atText, unit: "") { model.edit(set(at: $0)) } done: { model.onFocus?() }
            Text("Held").font(.system(size: 10)).foregroundStyle(.secondary).padding(.leading, 4)
            TypedValue(text: event.durationText ?? "", unit: "", clears: true) { model.edit(set(duration: $0)) } done: { model.onFocus?() }
        }
        if pitched {
            row("Note") {
                TypedValue(text: event.note ?? "", unit: "", clears: true) { model.edit(set(note: $0)) } done: { model.onFocus?() }
            }
        }
    }
}
