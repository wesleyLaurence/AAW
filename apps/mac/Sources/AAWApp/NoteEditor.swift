import AAWCore
import AppKit
import SwiftUI

/// The piano roll: the notes of the note clip last selected, by pitch and in
/// time, every MIDI note whether or not an instrument plays it. Notes are
/// added, selected, moved, stretched and removed here, on the grid or off it,
/// and each edit is a command to the host, as an agent's `daw note` is. The
/// clip owns its notes, so no other clip changes.
final class NoteEditor: NSView {
    private enum Drag {
        /// Notes being moved: by whole steps, or with Option by beats, and by
        /// notes. `collapse` is the note to select alone if the press is a click.
        case move(notes: [NoteView], origin: CGPoint, steps: Int, by: Double, semitones: Int, moved: Bool, collapse: UInt64?)
        /// Notes being made longer or shorter by the end of `grabbed`.
        case stretch(notes: [NoteView], grabbed: NoteView, end: Double, free: Bool, moved: Bool)
        /// A rectangle drawn around notes to select them, with those that
        /// were selected before when Shift was held.
        case marquee(origin: CGPoint, now: CGPoint, base: Set<UInt64>)
    }

    /// A note shown ahead of the host, where a drag has it.
    private struct Held {
        var at: Double
        var duration: Double
        var pitch: Int
    }

    private let model: SongModel
    private var context: NoteContext?
    private var layout = PianoRollLayout()
    private var color = Theme.palette[0]
    /// The clip the view was last fitted to.
    private var fitted: UInt64?
    private var drag: Drag?
    private var held: [UInt64: Held] = [:]
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

    /// Takes the clip to show, as the host has it now.
    func show(_ context: NoteContext, color: NSColor, playing: Bool) {
        let changed = context.clip != self.context?.clip
        if context.clip.key != self.context?.clip.key {
            drag = nil
            held = [:]
            model.noteInsert = nil
        }
        self.context = context
        self.color = color
        // What was shown ahead of the host has arrived, or was refused.
        if changed, drag == nil { held = [:] }
        relayout()
        link?.isPaused = !playing
        if !playing { playhead.isHidden = true }
        needsDisplay = true
    }

    private func relayout() {
        guard let context else { return }
        layout.size = bounds.size
        layout.grid = PianoRollLayout.beats(model.noteGrid) ?? 0.25
        layout.setSpan(length: context.clip.lengthBeats, notes: notes().map { ($0.at, $0.at + $0.duration) })
        if fitted != context.clip.key, layout.lanesWidth > 0, layout.lanesHeight > 0 {
            fitted = context.clip.key
            layout.fit(pitches: context.clip.notes.map { Int($0.pitch) })
        }
    }

    /// The clip's notes where they are drawn: the song's place, or the
    /// person's while they hold one. Selected notes last, so on top.
    private func notes() -> [(note: NoteView, at: Double, duration: Double, pitch: Int)] {
        guard let context else { return [] }
        let all = context.clip.notes.map { note in
            let h = held[note.key]
            return (note, h?.at ?? note.at, h?.duration ?? note.duration, h?.pitch ?? Int(note.pitch))
        }
        return all.filter { !model.selectedNotes.contains($0.0.key) } + all.filter { model.selectedNotes.contains($0.0.key) }
    }

    /// The note at a point: the one drawn on top, where notes overlap.
    private func note(at p: CGPoint) -> (note: NoteView, rect: CGRect)? {
        for n in notes().reversed() {
            let rect = layout.rect(at: n.at, duration: n.duration, pitch: n.pitch)
            if rect.insetBy(dx: -1, dy: 0).contains(p) { return (n.note, rect) }
        }
        return nil
    }

    // MARK: Frames

    @objc private func tick(_ link: CADisplayLink) {
        // The playhead, while the clip plays.
        guard let context, let head = model.playhead(), head.playing else {
            playhead.isHidden = true
            if !model.transport.playing { link.isPaused = true }
            return
        }
        let into = head.beat - context.clip.at
        guard into >= 0, into < context.clip.lengthBeats else {
            playhead.isHidden = true
            return
        }
        let x = layout.x(into).rounded()
        playhead.isHidden = x < PianoRollLayout.gutter || x > bounds.width
        playhead.frame = CGRect(x: x, y: 0, width: 1, height: bounds.height)
    }

    // MARK: Zoom and scroll

    override func scrollWheel(with event: NSEvent) {
        if event.modifierFlags.contains(.command) || event.modifierFlags.contains(.option) {
            let x = convert(event.locationInWindow, from: nil).x
            layout.zoom(by: exp(event.scrollingDeltaY * 0.01), anchorX: max(x, PianoRollLayout.gutter))
        } else {
            layout.scroll.x -= event.scrollingDeltaX
            layout.scroll.y -= event.scrollingDeltaY
            layout.clamp()
        }
        needsDisplay = true
    }

    override func magnify(with event: NSEvent) {
        let x = convert(event.locationInWindow, from: nil).x
        layout.zoom(by: 1 + event.magnification, anchorX: max(x, PianoRollLayout.gutter))
        needsDisplay = true
    }

    // MARK: Mouse

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        guard let context else { return }
        let p = convert(event.locationInWindow, from: nil)
        guard p.x >= PianoRollLayout.gutter, p.y >= PianoRollLayout.rulerHeight else { return }
        let shift = event.modifierFlags.contains(.shift)
        let free = event.modifierFlags.contains(.option)
        if let hit = note(at: p) {
            let key = hit.note.key
            if event.clickCount == 2 {
                model.edit(.notesRemove(notes: [key]))
                return
            }
            var selection = model.selectedNotes
            var collapse: UInt64?
            if shift {
                // Shift adds a note to the selection, or takes it out.
                if selection.remove(key) == nil { selection.insert(key) }
                guard selection.contains(key) else {
                    model.select(notes: selection)
                    needsDisplay = true
                    return
                }
            } else if !selection.contains(key) {
                selection = [key]
            } else if selection.count > 1 {
                collapse = key
            }
            model.select(notes: selection)
            let chosen = context.clip.notes.filter { selection.contains($0.key) }
            if PianoRollLayout.onEnd(p, of: hit.rect) {
                drag = .stretch(notes: chosen, grabbed: hit.note, end: hit.note.at + hit.note.duration, free: free, moved: false)
            } else {
                drag = .move(notes: chosen, origin: p, steps: 0, by: 0, semitones: 0, moved: false, collapse: collapse)
            }
            needsDisplay = true
            return
        }
        let beat = layout.beat(atX: p.x)
        let pitch = layout.pitch(atY: p.y)
        if event.clickCount == 2 {
            // A note on the step under the pointer, or with Option where it
            // is, at the note under it, a step of the grid long.
            guard beat >= 0, beat < context.clip.lengthBeats else {
                NSSound.beep()
                return
            }
            model.edit(.noteAdd(clip: context.clip.key, at: beat, free: free, grid: model.noteGrid, pitch: Int32(pitch))) { [weak self] made in
                self?.model.select(notes: Set(made))
            }
            return
        }
        // A click in the clear: where pasted notes go, and a rectangle to
        // select notes with.
        model.noteInsert = free ? (beat * 1000).rounded() / 1000 : layout.step(at: beat)
        if !shift { model.select(notes: []) }
        drag = .marquee(origin: p, now: p, base: shift ? model.selectedNotes : [])
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        let free = event.modifierFlags.contains(.option)
        switch drag {
        case .move(let notes, let origin, _, _, _, let moved, let collapse):
            let (dx, dy) = (p.x - origin.x, p.y - origin.y)
            if !moved, hypot(dx, dy) < 3 { return }
            // By whole steps, so that a note off the grid stays as far off
            // it; with Option, by thousandths of a beat.
            let steps = free ? 0 : layout.steps(forDrag: dx)
            let by = free ? layout.beats(forDrag: dx) : 0
            // Up and down by notes, as far as every note stays a MIDI note.
            let pitches = notes.map { Int($0.pitch) }
            let semitones = min(max(layout.semitones(forDrag: dy), -(pitches.min() ?? 0)), 127 - (pitches.max() ?? 127))
            let shift = Double(steps) * layout.grid + by
            for n in notes {
                held[n.key] = Held(at: n.at + shift, duration: n.duration, pitch: Int(n.pitch) + semitones)
            }
            drag = .move(notes: notes, origin: origin, steps: steps, by: by, semitones: semitones, moved: true, collapse: collapse)
        case .stretch(let notes, let grabbed, _, _, _):
            let wanted = layout.beat(atX: p.x)
            var end = free ? (wanted * 1000).rounded() / 1000 : layout.line(at: wanted)
            // Every note keeps a length: the grabbed one to the first line
            // after its start, or a thousandth off the grid.
            let least = free ? grabbed.at + 0.001 : layout.step(at: grabbed.at) + layout.grid
            end = max(end, least)
            var by = end - (grabbed.at + grabbed.duration)
            by = max(by, 0.001 - (notes.map(\.duration).min() ?? 1))
            end = grabbed.at + grabbed.duration + by
            for n in notes {
                held[n.key] = Held(at: n.at, duration: n.duration + by, pitch: Int(n.pitch))
            }
            drag = .stretch(notes: notes, grabbed: grabbed, end: end, free: free, moved: true)
        case .marquee(let origin, _, let base):
            drag = .marquee(origin: origin, now: p, base: base)
            let box = CGRect(x: min(origin.x, p.x), y: min(origin.y, p.y), width: abs(p.x - origin.x), height: abs(p.y - origin.y))
            let inside = notes().filter { layout.rect(at: $0.at, duration: $0.duration, pitch: $0.pitch).intersects(box) }.map(\.note.key)
            model.select(notes: base.union(inside))
        case nil:
            return
        }
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard let ended = drag else { return }
        drag = nil
        switch ended {
        case .move(let notes, _, let steps, let by, let semitones, let moved, let collapse):
            if moved, steps != 0 || by != 0 || semitones != 0 {
                model.edit(.notesMove(notes: notes.map(\.key), steps: Int32(steps), grid: model.noteGrid, by: by, semitones: Int32(semitones)))
                hold()
            } else {
                held = [:]
                if !moved, let collapse { model.select(notes: [collapse]) }
            }
        case .stretch(let notes, let grabbed, let end, let free, let moved):
            if moved, abs(end - (grabbed.at + grabbed.duration)) > 1e-9 {
                model.edit(.notesEnd(notes: notes.map(\.key), grabbed: grabbed.key, end: end, free: free, grid: model.noteGrid))
                hold()
            } else {
                held = [:]
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
                self.held = [:]
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
            // Here, the selected notes and nothing else: not the clip being edited.
            if model.selectedNotes.isEmpty { NSSound.beep() } else { model.deleteSelection() }
        case .leftArrow?: nudge(steps: -1, semitones: 0)
        case .rightArrow?: nudge(steps: 1, semitones: 0)
        case .upArrow?: nudge(steps: 0, semitones: octave ? 12 : 1)
        case .downArrow?: nudge(steps: 0, semitones: octave ? -12 : -1)
        case .carriageReturn?, .enter?: model.returnToStart()
        default:
            switch event.charactersIgnoringModifiers {
            case " ": model.togglePlay()
            case "l": model.toggleLoop()
            case "\u{1b}": model.select(notes: [])
            default: super.keyDown(with: event)
            }
        }
    }

    override func selectAll(_ sender: Any?) {
        model.select(notes: Set(context?.clip.notes.map(\.key) ?? []))
        needsDisplay = true
    }

    /// Moves the selected notes a step earlier or later, or up or down.
    private func nudge(steps: Int, semitones: Int) {
        let notes = model.selectedNoteViews
        guard !notes.isEmpty else {
            NSSound.beep()
            return
        }
        let pitches = notes.map { Int($0.pitch) }
        guard (pitches.min() ?? 0) + semitones >= 0, (pitches.max() ?? 0) + semitones <= 127 else {
            NSSound.beep()
            return
        }
        model.edit(.notesMove(notes: notes.map(\.key), steps: Int32(steps), grid: model.noteGrid, by: 0, semitones: Int32(semitones)))
        if semitones != 0, let pitch = pitches.first {
            layout.reveal(pitch: pitch + semitones)
            needsDisplay = true
        }
    }

    // MARK: Drawing

    private static let nameFont = NSFont.systemFont(ofSize: 9, weight: .semibold)
    private static let numberFont = NSFont.monospacedDigitSystemFont(ofSize: 9, weight: .regular)
    private static let noteFont = NSFont.systemFont(ofSize: 8, weight: .medium)

    private func text(_ string: String, in rect: CGRect, font: NSFont, color: NSColor, align: NSTextAlignment = .left) {
        guard rect.width > 4 else { return }
        if TextLines.shared.draw(string, in: rect, font: font, color: color, align: align) { return }
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        style.alignment = align
        (string as NSString).draw(
            with: rect, options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine],
            attributes: [.font: font, .foregroundColor: color, .paragraphStyle: style]
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

    /// The pitches whose rows are in view.
    private var visiblePitches: ClosedRange<Int> {
        let high = layout.pitch(atY: PianoRollLayout.rulerHeight)
        let low = layout.pitch(atY: bounds.height)
        return low...max(low, high)
    }

    /// Whether the track's instrument plays a note: always, without one, so
    /// that rows are shaded only where an instrument leaves notes silent.
    private func sounds(_ pitch: Int) -> Bool {
        guard let track = context?.track, track.instrument != nil else { return true }
        return track.map.contains { Int($0.low) <= pitch && pitch <= Int($0.high) }
    }

    override func draw(_ dirtyRect: NSRect) {
        let gutter = PianoRollLayout.gutter
        let ruler = PianoRollLayout.rulerHeight
        fill(bounds, Theme.gray(0.13))
        guard let context else { return }
        let lanes = CGRect(x: gutter, y: ruler, width: bounds.width - gutter, height: bounds.height - ruler)
        clipped(to: lanes) {
            drawRows()
            drawGrid(top: ruler)
            drawOutside(context.clip)
            drawNotes(context.clip)
            drawInsert()
            drawMarquee()
            let end = layout.x(layout.last)
            if end < bounds.width {
                fill(CGRect(x: end, y: ruler, width: bounds.width - end, height: bounds.height - ruler), Theme.gray(0.13))
            }
        }
        clipped(to: CGRect(x: 0, y: ruler, width: gutter, height: bounds.height - ruler)) { drawKeys() }
        clipped(to: CGRect(x: gutter, y: 0, width: bounds.width - gutter, height: ruler)) { drawRuler() }
        fill(CGRect(x: 0, y: 0, width: gutter, height: ruler), Theme.ruler)
        fill(CGRect(x: gutter - 1, y: 0, width: 1, height: bounds.height), Theme.separator)
        fill(CGRect(x: 0, y: ruler - 1, width: bounds.width, height: 1), Theme.separator)
    }

    /// A row for each note, as a piano's keys: the black ones darker, a line
    /// under each C, and darker still where the instrument plays nothing.
    private func drawRows() {
        let left = PianoRollLayout.gutter
        let width = bounds.width - left
        for pitch in visiblePitches {
            let y = layout.y(pitch)
            fill(CGRect(x: left, y: y, width: width, height: PianoRollLayout.semitone), Theme.lane)
            if PianoRollLayout.black(pitch) {
                fill(CGRect(x: left, y: y, width: width, height: PianoRollLayout.semitone), Theme.gray(0, 0.16))
            }
            if !sounds(pitch) {
                fill(CGRect(x: left, y: y, width: width, height: PianoRollLayout.semitone), Theme.gray(0, 0.22))
            }
            if pitch % 12 == 0 {
                fill(CGRect(x: left, y: (y + PianoRollLayout.semitone).rounded() - 1, width: width, height: 1), Theme.gray(1, 0.1))
            }
        }
    }

    /// A line at each step once they have room, at each beat and each bar.
    private func drawGrid(top: CGFloat) {
        let grid = layout.grid
        let stepWidth = CGFloat(grid) * layout.pixelsPerBeat
        let first = Int((max(layout.first, layout.beat(atX: PianoRollLayout.gutter)) / grid).rounded(.down))
        let last = Int((min(layout.last, layout.beat(atX: bounds.width)) / grid).rounded(.up))
        guard last >= first else { return }
        for step in first...last {
            let beat = Double(step) * grid
            let onBeat = abs(beat - beat.rounded()) < 1e-9
            let onBar = onBeat && Int(beat.rounded()) % 4 == 0
            guard onBar || (onBeat && layout.pixelsPerBeat >= 6) || stepWidth >= 6 else { continue }
            fill(CGRect(x: layout.x(beat).rounded(), y: top, width: 1, height: bounds.height - top),
                 onBar ? Theme.gray(1, 0.2) : onBeat ? Theme.gray(1, 0.1) : Theme.gridLine)
        }
    }

    /// Before the clip's start and after its end, where notes are kept and
    /// do not play: shaded, with a line at each edge.
    private func drawOutside(_ clip: NoteClipView) {
        let top = PianoRollLayout.rulerHeight
        let height = bounds.height - top
        let start = layout.x(0)
        let end = layout.x(clip.lengthBeats)
        if layout.first < 0 { fill(CGRect(x: layout.x(layout.first), y: top, width: start - layout.x(layout.first), height: height), Theme.pastEnd) }
        fill(CGRect(x: end, y: top, width: layout.x(layout.last) - end, height: height), Theme.pastEnd)
        for x in [start, end] {
            fill(CGRect(x: x.rounded() - 1, y: top, width: 2, height: height), color.withAlphaComponent(0.7))
        }
    }

    private func drawNotes(_ clip: NoteClipView) {
        let lit = color.blended(withFraction: 0.25, of: .white) ?? color
        for n in notes() {
            let rect = layout.rect(at: n.at, duration: n.duration, pitch: n.pitch)
            guard rect.maxX >= PianoRollLayout.gutter, rect.minX <= bounds.width,
                  rect.maxY >= PianoRollLayout.rulerHeight, rect.minY <= bounds.height else { continue }
            let selected = model.selectedNotes.contains(n.note.key)
            // A note outside the clip is kept and does not play: gray.
            let plays = n.at >= 0 && n.at < clip.lengthBeats
            let bar = rect.insetBy(dx: 0, dy: 0.5)
            let shape = NSBezierPath(roundedRect: bar, xRadius: 2, yRadius: 2)
            (plays ? lit : Theme.gray(0.55)).withAlphaComponent(0.4 + 0.6 * CGFloat(n.note.velocity) / 127).setFill()
            shape.fill()
            (selected ? Theme.selectedClip : Theme.gray(0, 0.5)).setStroke()
            shape.lineWidth = selected ? 1.5 : 0.5
            shape.stroke()
            if bar.width >= 22 {
                text(noteName(midi: Int32(n.pitch)), in: CGRect(x: bar.minX + 3, y: bar.midY - 5.5, width: bar.width - 4, height: 11),
                     font: Self.noteFont, color: Theme.gray(0.05, 0.9))
            }
        }
    }

    /// Where pasted notes go.
    private func drawInsert() {
        guard let at = model.noteInsert, model.selectedNotes.isEmpty else { return }
        fill(CGRect(x: layout.x(at).rounded(), y: PianoRollLayout.rulerHeight, width: 1, height: bounds.height), Theme.cue.withAlphaComponent(0.8))
    }

    private func drawMarquee() {
        guard case .marquee(let origin, let now, _) = drag, origin != now else { return }
        let box = CGRect(x: min(origin.x, now.x), y: min(origin.y, now.y), width: abs(now.x - origin.x), height: abs(now.y - origin.y))
        fill(box, Theme.gray(1, 0.08))
        Theme.gray(1, 0.6).setStroke()
        NSBezierPath(rect: box.insetBy(dx: 0.5, dy: 0.5)).stroke()
    }

    /// The keys: each C by its name, and a note that plays a pad of its own,
    /// as a drum's does, by the pad's name.
    private func drawKeys() {
        let gutter = PianoRollLayout.gutter
        let drums = Dictionary(
            (context?.track.map ?? []).filter { !$0.pitched && $0.low == $0.high }.map { (Int($0.low), $0.pad) },
            uniquingKeysWith: { first, _ in first }
        )
        for pitch in visiblePitches {
            let y = layout.y(pitch)
            let row = CGRect(x: 0, y: y, width: gutter - 1, height: PianoRollLayout.semitone)
            fill(row, PianoRollLayout.black(pitch) ? Theme.gray(0.12) : Theme.gray(0.26))
            fill(CGRect(x: 0, y: y + PianoRollLayout.semitone - 1, width: gutter - 1, height: 1), Theme.gray(0, 0.3))
            let label = CGRect(x: 4, y: y + PianoRollLayout.semitone / 2 - 5.5, width: gutter - 10, height: 11)
            if let pad = drums[pitch] {
                text(pad, in: CGRect(x: 4, y: label.minY, width: gutter - 40, height: 11), font: Self.nameFont, color: Theme.text)
            }
            if pitch % 12 == 0 || drums[pitch] != nil {
                text(noteName(midi: Int32(pitch)), in: label, font: Self.numberFont,
                     color: pitch % 12 == 0 ? Theme.text : Theme.dimText, align: .right)
            }
        }
    }

    /// Beats from the clip's start, as bars and beats counted from one.
    private func drawRuler() {
        let gutter = PianoRollLayout.gutter
        let ruler = PianoRollLayout.rulerHeight
        fill(CGRect(x: gutter, y: 0, width: bounds.width - gutter, height: ruler), Theme.ruler)
        var every = 1
        while CGFloat(every) * layout.pixelsPerBeat < 26 { every *= 2 }
        let first = Int(max(layout.first, layout.beat(atX: gutter)).rounded(.down))
        let last = Int(min(layout.last, layout.beat(atX: bounds.width)).rounded(.up))
        guard last >= first else { return }
        for beat in first...last where beat % every == 0 {
            let x = layout.x(Double(beat)).rounded()
            let onBar = beat % 4 == 0
            fill(CGRect(x: x, y: onBar ? 2 : ruler - 5, width: 1, height: onBar ? ruler - 2 : 5), Theme.gray(1, onBar ? 0.3 : 0.16))
            let bar = Int((Double(beat) / 4).rounded(.down))
            let label = onBar ? "\(bar + 1)" : "\(bar + 1).\(beat - bar * 4 + 1)"
            text(label, in: CGRect(x: x + 3, y: 2, width: 40, height: 12), font: Self.numberFont, color: onBar ? Theme.dimText : Theme.faintText)
        }
    }
}

/// The piano roll in SwiftUI.
struct NotePane: NSViewRepresentable {
    let model: SongModel
    let context: NoteContext
    let color: NSColor
    let playing: Bool
    /// What the editor draws from the model besides the clip.
    let selected: Set<UInt64>
    let grid: String

    func makeNSView(context: Context) -> NoteEditor {
        NoteEditor(model: model)
    }

    func updateNSView(_ view: NoteEditor, context: Context) {
        view.show(self.context, color: color, playing: playing)
    }
}

/// Beside the piano roll: the clip's ID, length and the grid, and the
/// selected notes' fields.
struct NoteClipHeader: View {
    let model: SongModel
    let context: NoteContext

    private var clip: NoteClipView { context.clip }

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

    private var subtitle: String {
        switch context.track.instrument {
        case nil: "Notes on \(context.track.id), no instrument"
        case let kind?: "Notes on \(context.track.id), by its \(kind)"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            VStack(alignment: .leading, spacing: 1) {
                Text(clip.id).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                Text(subtitle).font(.system(size: 10)).foregroundStyle(.secondary).lineLimit(1)
            }
            row("Length") {
                TypedValue(text: ValueScale.plain(clip.lengthBeats), unit: "beats") { typed in
                    model.edit(.clipLength(clip: clip.key, beats: typed))
                } done: {
                    model.onFocus?()
                }
            }
            row("Grid") {
                Picker("", selection: Binding(get: { model.noteGrid }, set: { model.noteGrid = $0 })) {
                    ForEach(PatternHeader.grids, id: \.self) { Text("\($0) beat").tag($0) }
                }
                .labelsHidden()
                .controlSize(.mini)
                .help("The steps notes are added and moved on. With Option, a note goes off the grid")
            }
            Divider().padding(.vertical, 2)
            let notes = model.selectedNoteViews
            if notes.count == 1, let note = notes.first {
                NoteFields(model: model, note: note)
            } else if let first = notes.first {
                Text("\(notes.count) notes").font(.system(size: 10, weight: .medium))
                row("Velocity") {
                    KnobBar(model: model, spec: BarSpec(value: Double(first.velocity), min: 1, max: 127, initial: 100, live: false, whole: true)) { [keys = notes.map(\.key)] value in
                        .notesSet(notes: keys, pitch: nil, at: nil, duration: nil, velocity: UInt32(value))
                    }
                }
            } else {
                Text("Double-click to add a note on the grid; with Option, off it. Drag a note to move it, its end to lengthen it, and around notes to select them.")
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
    }
}

/// The fields of the one selected note.
private struct NoteFields: View {
    let model: SongModel
    let note: NoteView

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

    private func set(pitch: String? = nil, at: String? = nil, duration: String? = nil) -> Edit {
        .notesSet(notes: [note.key], pitch: pitch, at: at, duration: duration, velocity: nil)
    }

    var body: some View {
        // A pitch is typed as a name or a number and kept as its number.
        row("Note") {
            TypedValue(text: noteName(midi: note.pitch), unit: "\(note.pitch)") { model.edit(set(pitch: $0)) } done: { model.onFocus?() }
        }
        // Beats of the clip, as typed: 1.975, or 1/3 for a triplet.
        row("At") {
            TypedValue(text: note.atText, unit: "") { model.edit(set(at: $0)) } done: { model.onFocus?() }
            Text("Length").font(.system(size: 10)).foregroundStyle(.secondary).padding(.leading, 4)
            TypedValue(text: note.durationText, unit: "") { model.edit(set(duration: $0)) } done: { model.onFocus?() }
        }
        row("Velocity") {
            KnobBar(model: model, spec: BarSpec(value: Double(note.velocity), min: 1, max: 127, initial: 100, live: false, whole: true)) { [key = note.key] value in
                .notesSet(notes: [key], pitch: nil, at: nil, duration: nil, velocity: UInt32(value))
            }
        }
    }
}
