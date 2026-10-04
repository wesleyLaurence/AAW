import AAWCore
import AVFoundation
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

/// A level the person can drag.
private enum Slider: Hashable {
    case gain(RowID)
    case pan(RowID)
    case send(track: UInt64, to: String)
}

/// A level shown ahead of the host: while it is dragged, and after the drag
/// until the host's song has it, or `until` passes.
private struct Held {
    var value: Double
    var until: CFTimeInterval?
}

/// A point shown ahead of the host, as a level is.
private struct HeldPoint {
    var at: Double
    var value: Double
    var until: CFTimeInterval?
}

private struct RowVisual {
    var name: String
    var detail: String
    var color: NSColor?
    var mute = false
    var solo = false
    /// Whether the row has automation lanes.
    var automated = false
    var kind: HeaderLayout.Kind
    var y: Animated
    var height: Animated
    var gain: Animated
    var pan: Animated
    /// The level of each send, by return.
    var sends: [String: Animated] = [:]
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
    /// What the clip plays: its track's waveform from the beat the clip was
    /// at when the waveform was made. It moves with the clip, and is replaced
    /// when the host has worked out what the clip plays where it is now.
    var wave: (peaks: Waveform, at: Double)?
    /// An audio clip as it is shown: the host's, or the person's while they
    /// drag a fade. It draws its file, and `pattern` is its sample's name.
    var audio: AudioClipView?
    /// A note clip as it is shown, drawing its notes; `pattern` is its ID.
    var notes: NoteClipView?
    /// The beat an audio clip's file starts on, which its waveform is drawn
    /// from, or a note clip's start, which its notes are placed from: it
    /// moves with the clip and stays while an edge is trimmed.
    var anchor = Animated(0)
}

/// A clip under the pointer.
private enum ClipHit {
    case pattern(ClipView)
    case audio(AudioClipView)
    case notes(NoteClipView)

    var key: UInt64 {
        switch self {
        case .pattern(let clip): clip.key
        case .audio(let clip): clip.key
        case .notes(let clip): clip.key
        }
    }
}

/// Where a dragged file would land: a sample in the headers as a pad, and
/// on the timeline as an audio clip at a beat; a MIDI file on the timeline
/// as a note clip.
private enum DropTarget: Equatable {
    /// As a pad of this track.
    case pad(UInt64)
    /// As a pad of a new track after the others.
    case newTrack
    /// As a clip of this track.
    case clip(track: UInt64, at: Double)
    /// As a clip of a new track after the others.
    case newClip(at: Double)
}

/// How long the arrangement took to draw each frame of a run of scrolling and
/// zooming, in milliseconds, and how far apart the frames were.
struct DrawTimes {
    var draws: [Double] = []
    var intervals: [Double] = []

    private static func ms(_ x: Double) -> Double { (x * 100).rounded() / 100 }

    private static func stats(_ values: [Double]) -> [String: Double] {
        let sorted = values.sorted()
        guard let last = sorted.last else { return [:] }
        let pick = { (q: Double) in sorted[Int((Double(sorted.count - 1) * q).rounded())] }
        return ["mean": ms(sorted.reduce(0, +) / Double(sorted.count)), "median": ms(pick(0.5)), "p95": ms(pick(0.95)), "max": ms(last)]
    }

    /// The report: what a draw took and the time between frames, with how
    /// many draws took longer than a sixtieth of a second.
    var report: [String: Any] {
        [
            "frames": draws.count,
            "draw_ms": Self.stats(draws),
            "interval_ms": Self.stats(intervals),
            "draws_over_16_7_ms": draws.filter { $0 > 1000.0 / 60 }.count,
        ]
    }
}

/// A run of frames being measured.
private struct Measuring {
    var frames: Int
    var left: Int
    var times = DrawTimes()
    var last: CFTimeInterval?
    var then: @MainActor (DrawTimes) -> Void
}

private struct SliderDrag {
    var slider: Slider
    var edit: (Double) -> Edit
    /// Where the drag has the level, before it is put on the control's steps.
    var value: Double
    /// The level shown and last sent.
    var shown: Double
    var lastX: CGFloat
    var perPoint: Double
    var range: ClosedRange<Double>
    var step: Double
    var gesture: String
    var moved = false
}

private struct ClipDrag {
    var start: CGPoint
    /// Where each dragged clip was: its beat and its track's place.
    var origin: [UInt64: (at: Double, track: Int)]
    /// How far the clips can move and stay in the song and among the tracks.
    var range: ClosedRange<Double>
    var rowRange: ClosedRange<Int>
    var by = 0.0
    var rows = 0
    var moved = false
    /// The clip to select alone if the press turns out to be a click.
    var collapse: UInt64?
}

/// An edge of an audio clip being dragged.
private struct TrimDrag {
    var clip: AudioClipView
    var layout: AudioClipLayout
    /// The start is dragged, or else the end.
    var start: Bool
    /// The beat the edge is at.
    var beat: Double
    var moved = false
}

/// An edge of a note clip being dragged: its notes stay where they are.
private struct NoteTrimDrag {
    var clip: NoteClipView
    /// The start is dragged, or else the end.
    var start: Bool
    /// The beat the edge is at.
    var beat: Double
    var moved = false
}

/// A fade's handle being dragged.
private struct FadeDrag {
    var clip: AudioClipView
    var layout: AudioClipLayout
    /// The fade in is dragged, or else the fade out.
    var fadeIn: Bool
    /// The fade's length in beats.
    var beats: Double
    var moved = false
}

private struct ReorderDrag {
    var row: RowID
    var startY: CGFloat
    /// The row's place among its kind, and the gap it would move to.
    var from: Int
    var slot: Int?
}

private struct PointDrag {
    var lane: LaneView
    var point: UInt64
    /// The lane on the timeline when the drag began.
    var rect: CGRect
    /// Where the point can go in time: no further than its neighbors.
    var range: ClosedRange<Double>
    var gesture: String
    var at: Double
    var value: Double
    var moved = false
}

private enum Drag {
    /// A loop being dragged out in the ruler.
    case loop(anchor: CGFloat, region: (start: Double, length: Double)?)
    case slider(SliderDrag)
    case clips(ClipDrag)
    case resize(clip: ClipView, repeats: Int)
    case trim(TrimDrag)
    case noteTrim(NoteTrimDrag)
    case fade(FadeDrag)
    case reorder(ReorderDrag)
    case point(PointDrag)
}

/// A parameter chosen from the menu that adds a lane.
private final class LaneChoice: NSObject {
    let row: RowID
    let param: String

    init(row: RowID, param: String) {
        self.row = row
        self.param = param
    }
}

/// The arrangement: a ruler with the loop brace, sections and bars; a header
/// for each track, return and the master; and the tracks' clips on a timeline.
/// It draws the host's arrangement and eases to each new revision, lighting
/// up what an agent's or an external change touched. The person edits here:
/// levels, mute and solo in the headers, clips on the timeline, and the points
/// of the automation lanes that fold out under a row. Each edit is shown at
/// once and sent to the host, whose song the view then follows.
final class ArrangementView: NSView, NSTextFieldDelegate {
    private static let moveTime: CFTimeInterval = 0.3
    private static let glowTime: CFTimeInterval = 1.8
    /// How near a clip's end a press resizes it, in points.
    private static let resizeEdge: CGFloat = 6

    private let model: SongModel
    private var layout = TimelineLayout()
    private var rows: [RowID: RowVisual] = [:]
    private var clips: [UInt64: ClipVisual] = [:]
    /// The clips' keys in order, oldest first: the order they are drawn in.
    /// Kept from frame to frame, since sorting a thousand clips costs more
    /// than drawing them.
    private var clipOrder: [UInt64] = []
    private var glows: [GlowKey: Glow] = [:]
    private var link: CADisplayLink?
    private let playheadView = PlayheadView()
    private var fitted = false
    private var drag: Drag?
    private var held: [Slider: Held] = [:]
    /// The tracks whose sends are shown.
    private var unfolded: Set<UInt64> = []
    /// The rows whose automation lanes are shown.
    private var lanesShown: Set<RowID> = []
    private var heldPoints: [UInt64: HeldPoint] = [:]
    private var renaming: (row: RowID, field: NSTextField)?
    private var dropTarget: DropTarget?
    /// The file being dragged over the view and its length, in seconds for an
    /// audio file and in beats for a MIDI file, read once for the outline of
    /// the clip it would make.
    private var dropFile: (path: String, seconds: Double?, beats: Double?)?
    /// The audio clip under the pointer, which shows its fades' handles.
    private var hoverClip: UInt64?
    private var measuring: Measuring?

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
        model.onRefusal = { [weak self] in self?.revert() }
        model.onSelection = { [weak self] in self?.needsDisplay = true }
        model.onRename = { [weak self] row in self?.beginRename(row) }
        model.onShowLanes = { [weak self] row in self?.showLanes(of: row) }
        model.onFocus = { [weak self] in
            guard let self else { return }
            self.window?.makeFirstResponder(self)
        }
        model.onWaveforms = { [weak self] in self?.takeWaveforms() }
        model.onMeasure = { [weak self] frames, then in self?.measure(frames: frames, then: then) }
        registerForDraggedTypes([.fileURL, .string] + (DeviceChain.kinds + ["sampler"]).map { NSPasteboard.PasteboardType(Browser.deviceType + "." + $0) })
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

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas { removeTrackingArea(area) }
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                                       owner: self))
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

    private static func chain(_ effects: [EffectView]) -> String {
        effects.filter { !$0.bypass }.map(\.kind).joined(separator: " · ")
    }

    /// The clips whose place the person holds in a drag.
    private var draggedClips: Set<UInt64> {
        switch drag {
        case .clips(let d) where d.moved: Set(d.origin.keys)
        case .resize(let clip, _): [clip.key]
        case .trim(let d): [d.clip.key]
        case .noteTrim(let d): [d.clip.key]
        case .fade(let d): [d.clip.key]
        default: []
        }
    }

    /// Takes a revision's arrangement as the target every row and clip eases to.
    private func show(_ a: Arrangement, animated: Bool) {
        let now = CACurrentMediaTime()
        let time = animated ? Self.moveTime : 0
        var liveRows = Set<RowID>()
        var liveClips = Set<UInt64>()
        let dragged = draggedClips
        var y: CGFloat = 0

        // A level eases to the host's value, unless the person holds it.
        func level(_ slider: Slider, from shown: Animated, to target: Double) -> Animated {
            if let hold = held[slider] {
                if hold.until != nil, abs(hold.value - target) < 1e-9 {
                    held[slider] = nil
                    return Animated(target)
                }
                return Animated(hold.value)
            }
            var shown = shown
            shown.move(to: target, at: now, over: time)
            return shown
        }

        // A point eases to the host's place, unless the person holds it.
        for lane in a.tracks.flatMap(\.lanes) + a.returns.flatMap(\.lanes) + a.master.lanes {
            for point in lane.points {
                if let hold = heldPoints[point.key], hold.until != nil,
                   abs(hold.at - point.at) < 1e-9, abs(hold.value - point.value) < 1e-9 {
                    heldPoints[point.key] = nil
                }
            }
        }

        /// How much taller a row is for its lanes, while they are shown.
        func lanesHeight(_ id: RowID, _ lanes: [LaneView]) -> CGFloat {
            HeaderLayout.extraHeight(lanes: lanesShown.contains(id) ? lanes.count : nil)
        }

        func place(_ id: RowID, name: String, detail: String, color: NSColor?, mute: Bool, solo: Bool,
                   gain: Double, pan: Double, kind: HeaderLayout.Kind, height: CGFloat, sends: [SendView] = [],
                   lanes: Int) {
            liveRows.insert(id)
            var row: RowVisual
            if let old = rows[id], !old.removing {
                row = old
                row.y.move(to: y, at: now, over: time)
                row.height.move(to: height, at: now, over: time)
            } else {
                var alpha = Animated(animated ? 0 : 1)
                alpha.move(to: 1, at: now, over: time)
                row = RowVisual(
                    name: name, detail: detail, color: color, kind: kind, y: Animated(y), height: Animated(height),
                    gain: Animated(gain), pan: Animated(pan), alpha: alpha
                )
            }
            row.name = name
            row.detail = detail
            row.mute = mute
            row.solo = solo
            row.automated = lanes > 0
            row.gain = level(.gain(id), from: row.gain, to: gain)
            row.pan = level(.pan(id), from: row.pan, to: pan)
            if case .track(let key) = id {
                var levels: [String: Animated] = [:]
                for send in sends {
                    // A new send rises from the bottom of its bar.
                    let shown = row.sends[send.to] ?? Animated(Fader.send.lowerBound)
                    levels[send.to] = level(.send(track: key, to: send.to), from: shown, to: send.gainDb)
                }
                // A send the person is dragging into being.
                for (slider, hold) in held {
                    if case .send(track: key, to: let to) = slider, levels[to] == nil { levels[to] = Animated(hold.value) }
                }
                row.sends = levels
            }
            rows[id] = row
        }

        for track in a.tracks {
            let color = model.color(of: track.key)
            let sends = unfolded.contains(track.key) ? a.returns.count : 0
            let height = TimelineLayout.trackHeight + HeaderLayout.extraHeight(sends: sends)
                + lanesHeight(.track(track.key), track.lanes)
            // A MIDI track's chain starts with its instrument.
            let chain = track.midi ? ([track.instrument ?? "no instrument"] + [Self.chain(track.effects)]).filter { !$0.isEmpty }.joined(separator: " · ")
                : Self.chain(track.effects)
            place(.track(track.key), name: track.id, detail: chain, color: color,
                  mute: track.mute, solo: track.solo, gain: track.gainDb, pan: track.pan,
                  kind: .track, height: height, sends: track.sends, lanes: track.lanes.count)
            for clip in track.clips {
                liveClips.insert(clip.key)
                let length = clip.patternBeats * Double(clip.repeats)
                if var v = clips[clip.key], !v.removing {
                    v.pattern = clip.pattern
                    v.color = color
                    v.muted = track.mute
                    if !dragged.contains(clip.key) {
                        v.repeats = Int(clip.repeats)
                        v.at.move(to: clip.at, at: now, over: time)
                        v.length.move(to: length, at: now, over: time)
                        v.y.move(to: y, at: now, over: time)
                    }
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
            for clip in track.noteClips {
                liveClips.insert(clip.key)
                if var v = clips[clip.key], !v.removing {
                    v.pattern = clip.id
                    v.color = color
                    v.muted = track.mute
                    v.notes = clip
                    if !dragged.contains(clip.key) {
                        v.at.move(to: clip.at, at: now, over: time)
                        v.length.move(to: clip.lengthBeats, at: now, over: time)
                        v.anchor.move(to: clip.at, at: now, over: time)
                        v.y.move(to: y, at: now, over: time)
                    }
                    clips[clip.key] = v
                } else {
                    var alpha = Animated(animated ? 0 : 1)
                    alpha.move(to: 1, at: now, over: time)
                    clips[clip.key] = ClipVisual(
                        pattern: clip.id, repeats: 1, color: color, muted: track.mute,
                        at: Animated(clip.at), length: Animated(clip.lengthBeats), y: Animated(y), alpha: alpha,
                        notes: clip, anchor: Animated(clip.at)
                    )
                }
            }
            for clip in track.audio {
                liveClips.insert(clip.key)
                let shape = audioLayout(clip, in: a)
                if var v = clips[clip.key], !v.removing {
                    v.pattern = clip.sample
                    v.color = color
                    v.muted = track.mute
                    if !dragged.contains(clip.key) {
                        v.audio = clip
                        v.at.move(to: shape.start, at: now, over: time)
                        v.length.move(to: shape.length, at: now, over: time)
                        v.anchor.move(to: shape.fileStart, at: now, over: time)
                        v.y.move(to: y, at: now, over: time)
                    }
                    clips[clip.key] = v
                } else {
                    var alpha = Animated(animated ? 0 : 1)
                    alpha.move(to: 1, at: now, over: time)
                    clips[clip.key] = ClipVisual(
                        pattern: clip.sample, repeats: 1, color: color, muted: track.mute,
                        at: Animated(shape.start), length: Animated(shape.length), y: Animated(y), alpha: alpha,
                        audio: clip, anchor: Animated(shape.fileStart)
                    )
                }
            }
            y += height
        }
        y += TimelineLayout.busGap
        for bus in a.returns {
            let height = TimelineLayout.busHeight + lanesHeight(.bus(bus.key), bus.lanes)
            place(.bus(bus.key), name: bus.id, detail: Self.chain(bus.effects), color: nil,
                  mute: bus.mute, solo: false, gain: bus.gainDb, pan: bus.pan, kind: .bus, height: height,
                  lanes: bus.lanes.count)
            y += height
        }
        let masterHeight = TimelineLayout.busHeight + lanesHeight(.master, a.master.lanes)
        place(.master, name: "Master", detail: Self.chain(a.master.effects), color: nil,
              mute: false, solo: false, gain: a.master.gainDb, pan: 0, kind: .master, height: masterHeight,
              lanes: a.master.lanes.count)
        y += masterHeight

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
        if let renaming, !liveRows.contains(renaming.row) { endRename(commit: false) }
        if clips.count != clipOrder.count || !liveClips.isSubset(of: clipOrder) { clipOrder = clips.keys.sorted() }
        takeWaveforms()

        layout.contentHeight = y
        layout.beatsPerBar = Double(a.beatsPerBar)
        layout.lengthBeats = a.lengthBeats
        layout.clamp()
        needsDisplay = true
        link?.isPaused = false
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

    /// Gives each clip the waveform of what it plays, for the tracks whose
    /// waveforms have arrived for the revision shown. Until then a clip keeps
    /// the one it had, which moves with it, and a new clip has none.
    private func takeWaveforms() {
        for track in model.arrangement.tracks {
            guard let peaks = model.waveform(of: track.key) else { continue }
            for clip in track.clips { clips[clip.key]?.wave = (peaks, clip.at) }
        }
        needsDisplay = true
    }

    /// Puts back what was shown ahead of the host, after the host refused it.
    private func revert() {
        held.removeAll()
        heldPoints.removeAll()
        switch drag {
        case .slider, .point, .trim, .noteTrim, .fade: drag = nil
        default: break
        }
        show(model.arrangement, animated: true)
    }

    /// Unfolds a row's automation lanes.
    private func showLanes(of row: RowID) {
        guard !lanesShown.contains(row) else { return }
        lanesShown.insert(row)
        show(model.arrangement, animated: true)
    }

    private func transportChanged() {
        needsDisplay = true
        link?.isPaused = false
    }

    private func dropRemoved(at now: CFTimeInterval) {
        // Most frames have nothing to drop.
        if rows.values.contains(where: { $0.removing && !$0.alpha.isRunning(at: now) }) {
            rows = rows.filter { !($0.value.removing && !$0.value.alpha.isRunning(at: now)) }
        }
        if clips.values.contains(where: { $0.removing && !$0.alpha.isRunning(at: now) }) {
            clips = clips.filter { !($0.value.removing && !$0.value.alpha.isRunning(at: now)) }
            clipOrder = clips.keys.sorted()
        }
    }

    // MARK: Frames

    @objc private func tick(_ link: CADisplayLink) {
        let now = CACurrentMediaTime()
        dropRemoved(at: now)
        glows = glows.filter { now - $0.value.start < Self.glowTime }
        // A level the host never took goes back to the host's.
        if held.values.contains(where: { $0.until.map { now > $0 } ?? false }) {
            held = held.filter { $0.value.until.map { now <= $0 } ?? true }
            show(model.arrangement, animated: true)
        }
        if heldPoints.values.contains(where: { $0.until.map { now > $0 } ?? false }) {
            heldPoints = heldPoints.filter { $0.value.until.map { now <= $0 } ?? true }
            needsDisplay = true
        }
        var busy = stepMeasure(at: now) || !glows.isEmpty || held.values.contains { $0.until != nil }
            || heldPoints.values.contains { $0.until != nil }
            || rows.values.contains {
                $0.y.isRunning(at: now) || $0.height.isRunning(at: now) || $0.gain.isRunning(at: now)
                    || $0.pan.isRunning(at: now) || $0.alpha.isRunning(at: now)
                    || $0.sends.values.contains { $0.isRunning(at: now) }
            }
            || clips.values.contains {
                $0.at.isRunning(at: now) || $0.length.isRunning(at: now) || $0.y.isRunning(at: now) || $0.alpha.isRunning(at: now)
                    || $0.anchor.isRunning(at: now)
            }
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

    // MARK: Measuring

    /// Scrolls and zooms through the song for `frames` frames, timing each
    /// draw, then reports the times.
    private func measure(frames: Int, then: @escaping @MainActor (DrawTimes) -> Void) {
        layout.fit()
        measuring = Measuring(frames: max(3, frames), left: max(3, frames), then: then)
        needsDisplay = true
        link?.isPaused = false
    }

    /// Moves the view for the next measured frame: a third of the run zooming
    /// in around the middle, a third scrolling through the song there, and a
    /// third zooming back out. True while a run goes on.
    private func stepMeasure(at now: CFTimeInterval) -> Bool {
        guard var m = measuring else { return false }
        if let last = m.last { m.times.intervals.append((now - last) * 1000) }
        m.last = now
        guard m.left > 0 else {
            measuring = nil
            layout.fit()
            needsDisplay = true
            m.then(m.times)
            return false
        }
        let third = max(1, m.frames / 3)
        let center = TimelineLayout.headerWidth + layout.lanesWidth / 2
        switch (m.frames - m.left) / third {
        case 0: layout.zoom(by: pow(8, 1 / CGFloat(third)), anchorX: center)
        case 1:
            // There and back, so that the last third zooms out from the middle.
            let span = max(0, layout.contentWidth - layout.lanesWidth)
            let step = 2 * span / CGFloat(third)
            layout.scroll.x += (m.frames - m.left) % third < third / 2 ? step : -step
            layout.scroll.y = layout.scroll.y == 0 ? 40 : 0
            layout.clamp()
        default: layout.zoom(by: pow(1 / 8, 1 / CGFloat(third)), anchorX: center)
        }
        m.left -= 1
        measuring = m
        needsDisplay = true
        return true
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
        endRename(commit: true)
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
        endRename(commit: true)
        needsDisplay = true
    }

    override func magnify(with event: NSEvent) {
        let x = convert(event.locationInWindow, from: nil).x
        layout.zoom(by: 1 + event.magnification, anchorX: max(x, TimelineLayout.headerWidth))
        needsDisplay = true
    }

    // MARK: What is where

    /// The header of a row, with its top at `top` in the view.
    private func header(_ id: RowID, _ row: RowVisual, top: CGFloat) -> HeaderLayout {
        let returns = model.arrangement.returns.count
        var sends = 0
        if case .track(let key) = id, unfolded.contains(key) { sends = returns }
        return HeaderLayout(kind: row.kind, top: top, sends: sends, folds: row.kind == .track && returns > 0,
                            lanes: lanesShown.contains(id) ? model.lanes(of: id).count : nil)
    }

    /// The lanes shown under a row whose top is at `top`, each with its
    /// rectangle on the timeline.
    private func laneRects(_ id: RowID, _ row: RowVisual, top: CGFloat) -> [(lane: LaneView, rect: CGRect)] {
        guard lanesShown.contains(id) else { return [] }
        let header = header(id, row, top: top)
        let x = TimelineLayout.headerWidth
        return model.lanes(of: id).enumerated().map { index, lane in
            (lane, CGRect(x: x, y: header.lane(index).minY, width: max(0, bounds.width - x), height: HeaderLayout.laneHeight - 1))
        }
    }

    /// The lane at a point of the timeline.
    private func lane(at p: CGPoint) -> (lane: LaneView, rect: CGRect)? {
        for (id, row) in rows {
            guard let frame = frame(of: id) else { continue }
            if let hit = laneRects(id, row, top: frame.top).first(where: { $0.rect.contains(p) }) { return hit }
        }
        return nil
    }

    /// Where a lane's points are, in the view: the host's, or the person's
    /// while they hold one.
    private func positions(_ lane: LaneView, in rect: CGRect) -> [(point: PointView, at: CGPoint)] {
        let scale = ValueScale(min: lane.min, max: lane.max, log: lane.log)
        return lane.points.map { point in
            let hold = heldPoints[point.key]
            return (point, CGPoint(x: layout.x(hold?.at ?? point.at), y: scale.y(hold?.value ?? point.value, in: rect)))
        }
    }

    /// The top and height a row is settling at, in the view.
    private func frame(of id: RowID) -> (top: CGFloat, height: CGFloat)? {
        guard let row = rows[id], !row.removing else { return nil }
        return (layout.y(CGFloat(row.y.to)), CGFloat(row.height.to))
    }

    private func row(atY y: CGFloat) -> (id: RowID, header: HeaderLayout)? {
        for (id, row) in rows {
            guard let frame = frame(of: id), y >= frame.top, y < frame.top + frame.height else { continue }
            return (id, header(id, row, top: frame.top))
        }
        return nil
    }

    /// The place among the tracks of the track at a height of the view, or of
    /// the nearest one.
    private func trackIndex(atY y: CGFloat) -> Int {
        let tracks = model.arrangement.tracks
        for (index, track) in tracks.enumerated() {
            if let frame = frame(of: .track(track.key)), y < frame.top + frame.height { return index }
        }
        return max(0, tracks.count - 1)
    }

    private func clipRect(at: Double, length: Double, top: CGFloat) -> CGRect {
        CGRect(x: layout.x(at), y: top + 2, width: max(2, CGFloat(length) * layout.pixelsPerBeat - 1),
               height: TimelineLayout.trackHeight - 5)
    }

    /// The height of a clip's title strip.
    private static let titleHeight: CGFloat = 14

    /// What a clip shows under its title: its waveform.
    private func body(of rect: CGRect) -> CGRect {
        CGRect(x: rect.minX, y: rect.minY + Self.titleHeight + 1, width: rect.width, height: rect.height - Self.titleHeight - 2)
    }

    /// An audio clip as it is drawn and dragged, in the arrangement shown or
    /// in a newer one.
    private func audioLayout(_ clip: AudioClipView, in a: Arrangement? = nil) -> AudioClipLayout {
        let a = a ?? model.arrangement
        let seconds = clip.file == 0 ? nil : a.files.first { $0.identity == clip.file }?.seconds
        return AudioClipLayout(clip, tempo: a.tempo, seconds: seconds)
    }

    /// Where a clip is in the order clips are drawn in: selected clips over
    /// the others, as a dragged clip should be, and newer clips over older.
    private func drawOrder(_ key: UInt64) -> (Int, UInt64) {
        (model.selectedClips.contains(key) ? 1 : 0, key)
    }

    /// The clip at a point: the one drawn on top, where clips overlap.
    private func clip(at p: CGPoint) -> (hit: ClipHit, rect: CGRect)? {
        var found: (hit: ClipHit, rect: CGRect)?
        func take(_ hit: ClipHit, _ rect: CGRect) {
            guard rect.contains(p), found.map({ drawOrder($0.hit.key) < drawOrder(hit.key) }) ?? true else { return }
            found = (hit, rect)
        }
        for track in model.arrangement.tracks {
            guard let frame = frame(of: .track(track.key)) else { continue }
            for clip in track.clips {
                take(.pattern(clip), clipRect(at: clip.at, length: clip.patternBeats * Double(clip.repeats), top: frame.top))
            }
            for clip in track.audio {
                let shape = audioLayout(clip)
                take(.audio(clip), clipRect(at: shape.start, length: shape.length, top: frame.top))
            }
            for clip in track.noteClips {
                take(.notes(clip), clipRect(at: clip.at, length: clip.lengthBeats, top: frame.top))
            }
        }
        return found
    }

    /// What of an audio clip a point is on: an edge, a fade's handle or the clip.
    private func part(of clip: AudioClipView, at p: CGPoint, in rect: CGRect) -> AudioClipLayout.Part {
        audioLayout(clip).part(at: p, in: body(of: rect))
    }

    /// Whether a point is on the end of a clip, where a drag changes its repeats.
    private func onResizeEdge(_ p: CGPoint, of rect: CGRect) -> Bool {
        rect.width >= 16 && p.x >= rect.maxX - Self.resizeEdge
    }

    /// Whether a point is on the start of a clip, where a drag trims a note clip.
    private func onStartEdge(_ p: CGPoint, of rect: CGRect) -> Bool {
        rect.width >= 16 && p.x <= rect.minX + Self.resizeEdge
    }

    /// The rows of a row's kind in their order: the tracks, or the returns.
    private func siblings(of row: RowID) -> [RowID] {
        switch row {
        case .track: model.arrangement.tracks.map { .track($0.key) }
        case .bus: model.arrangement.returns.map { .bus($0.key) }
        case .master: []
        }
    }

    // MARK: Mouse

    override func mouseDown(with event: NSEvent) {
        endRename(commit: true)
        window?.makeFirstResponder(self)
        let p = convert(event.locationInWindow, from: nil)
        if p.x < TimelineLayout.headerWidth {
            if p.y >= TimelineLayout.rulerHeight { headerDown(at: p, event) }
        } else if p.y < TimelineLayout.loopStrip {
            drag = .loop(anchor: p.x, region: nil)
        } else if p.y >= TimelineLayout.rulerHeight, let hit = lane(at: p) {
            laneDown(hit.lane, rect: hit.rect, at: p, event)
        } else if p.y >= TimelineLayout.rulerHeight, let hit = clip(at: p) {
            clipDown(hit.hit, rect: hit.rect, at: p, event)
        } else {
            let free = event.modifierFlags.contains(.option)
            // Clips are pasted on the track whose lane was clicked last.
            if let track = track(atLane: p) { model.cueTrack = track }
            if event.clickCount == 2, let track = track(atLane: p) {
                // Twice on an empty part of a track: a new clip in the grid
                // step under the pointer, with a new pattern to fill in, or
                // on a MIDI track an empty note clip.
                model.addClip(to: track, at: layout.step(atX: p.x, free: free))
                return
            }
            if p.y >= TimelineLayout.rulerHeight { model.select(clips: []) }
            model.locate(layout.target(atX: p.x, free: free))
        }
    }

    /// The track whose strip of clips is at a point of the timeline.
    private func track(atLane p: CGPoint) -> UInt64? {
        guard p.y >= TimelineLayout.rulerHeight, let (id, _) = row(atY: p.y), case .track(let key) = id,
              let frame = frame(of: id), p.y < frame.top + TimelineLayout.trackHeight else { return nil }
        return key
    }

    private func headerDown(at p: CGPoint, _ event: NSEvent) {
        guard let (id, header) = row(atY: p.y), let row = rows[id] else {
            model.select(row: nil)
            return
        }
        let twice = event.clickCount == 2
        switch header.part(at: p) {
        case .auto:
            // With Option, every row follows.
            let all = event.modifierFlags.contains(.option) ? rows.filter { !$0.value.removing }.map(\.key) : [id]
            if lanesShown.contains(id) { lanesShown.subtract(all) } else { lanesShown.formUnion(all) }
            show(model.arrangement, animated: true)
        case .laneRemove(let index):
            let lanes = model.lanes(of: id)
            if lanes.indices.contains(index) { model.edit(.laneRemove(lane: lanes[index].key)) }
        case .laneAdd:
            chooseLane(for: id, at: p)
        case .lane:
            model.select(row: id)
        case .mute:
            rows[id]?.mute.toggle()
            model.edit(.mute(row: id.row, on: !row.mute))
        case .solo:
            guard case .track(let key) = id else { return }
            rows[id]?.solo.toggle()
            model.edit(.solo(track: key, on: !row.solo))
        case .fold:
            guard case .track(let key) = id else { return }
            // With Option, every track follows.
            let keys = event.modifierFlags.contains(.option) ? model.arrangement.tracks.map(\.key) : [key]
            if unfolded.contains(key) { unfolded.subtract(keys) } else { unfolded.formUnion(keys) }
            show(model.arrangement, animated: true)
        case .volume:
            beginSlider(.gain(id), from: row.gain.to, range: Fader.gain, perPoint: 0.25, step: 0.1,
                        resetTo: twice ? 0 : nil, at: p) { .gain(row: id.row, db: $0) }
        case .pan:
            beginSlider(.pan(id), from: row.pan.to, range: Fader.pan, perPoint: 0.01, step: 0.01,
                        resetTo: twice ? 0 : nil, at: p) { .pan(row: id.row, pan: $0) }
        case .send(let index):
            guard case .track(let key) = id, model.arrangement.returns.indices.contains(index) else { return }
            let to = model.arrangement.returns[index].id
            if twice {
                // Back to no send.
                if row.sends[to] != nil { model.edit(.sendRemove(track: key, to: to)) }
                return
            }
            beginSlider(.send(track: key, to: to), from: row.sends[to]?.to ?? Fader.send.lowerBound, range: Fader.send,
                        perPoint: 0.5, step: 0.1, resetTo: nil, at: p) { .send(track: key, to: to, db: $0) }
        case .name, .body:
            model.select(row: id)
            guard id != .master else { return }
            if twice, header.part(at: p) == .name {
                beginRename(id)
            } else if let from = siblings(of: id).firstIndex(of: id) {
                drag = .reorder(ReorderDrag(row: id, startY: p.y, from: from))
            }
        }
        needsDisplay = true
    }

    private func beginSlider(_ slider: Slider, from value: Double, range: ClosedRange<Double>, perPoint: Double,
                             step: Double, resetTo: Double?, at p: CGPoint, edit: @escaping (Double) -> Edit) {
        if let resetTo {
            if value != resetTo { model.edit(edit(resetTo)) }
            return
        }
        drag = .slider(SliderDrag(
            slider: slider, edit: edit, value: value, shown: value, lastX: p.x, perPoint: perPoint,
            range: range, step: step, gesture: model.newGesture()
        ))
    }

    private func showLevel(_ slider: Slider, _ value: Double) {
        switch slider {
        case .gain(let id): rows[id]?.gain = Animated(value)
        case .pan(let id): rows[id]?.pan = Animated(value)
        case .send(let track, let to): rows[.track(track)]?.sends[to] = Animated(value)
        }
    }

    /// Offers the parameters of a row that have no lane yet.
    private func chooseLane(for id: RowID, at p: CGPoint) {
        let menu = NSMenu()
        let targets = model.laneTargets(of: id)
        if targets.isEmpty {
            menu.addItem(NSMenuItem(title: "Everything that can be automated here has a lane", action: nil, keyEquivalent: ""))
        }
        for target in targets {
            let item = NSMenuItem(title: target.label, action: #selector(addLane(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = LaneChoice(row: id, param: target.param)
            menu.addItem(item)
        }
        menu.popUp(positioning: nil, at: p, in: self)
    }

    @objc private func addLane(_ sender: NSMenuItem) {
        guard let choice = sender.representedObject as? LaneChoice else { return }
        model.edit(.laneAdd(row: choice.row.row, param: choice.param))
    }

    /// A press in a lane: on a point it selects it and may drag it, twice it
    /// removes it, with Option it changes whether it holds; twice on the lane
    /// it adds a point there.
    private func laneDown(_ lane: LaneView, rect: CGRect, at p: CGPoint, _ event: NSEvent) {
        let near = positions(lane, in: rect).enumerated()
            .map { (index: $0.offset, point: $0.element.point, distance: hypot($0.element.at.x - p.x, $0.element.at.y - p.y)) }
            .filter { $0.distance <= 7 }
            .min { $0.distance < $1.distance }
        guard let near else {
            if event.clickCount == 2 {
                let scale = ValueScale(min: lane.min, max: lane.max, log: lane.log)
                let at = layout.snapped(layout.beat(atX: p.x), free: event.modifierFlags.contains(.option))
                model.edit(.pointAdd(lane: lane.key, at: at, value: scale.value(atY: p.y, in: rect))) { [weak self] made in
                    self?.model.select(point: made.first)
                }
            } else {
                model.select(point: nil)
            }
            return
        }
        model.select(point: near.point.key)
        if event.clickCount == 2 {
            model.edit(.pointRemove(point: near.point.key))
        } else if event.modifierFlags.contains(.option) {
            model.edit(.pointSet(point: near.point.key, at: nil, value: nil, hold: !near.point.hold))
        } else {
            let before = near.index > 0 ? lane.points[near.index - 1].at : 0
            let after = near.index + 1 < lane.points.count ? lane.points[near.index + 1].at : layout.lengthBeats
            drag = .point(PointDrag(
                lane: lane, point: near.point.key, rect: rect, range: before...max(before, after),
                gesture: model.newGesture(), at: near.point.at, value: near.point.value
            ))
        }
    }

    /// How far right an audio clip can be dragged: the song grows with it.
    private static let farBeats = 100_000.0

    private func clipDown(_ hit: ClipHit, rect: CGRect, at p: CGPoint, _ event: NSEvent) {
        let key = hit.key
        // An edge or a fade's handle is of one clip, which it selects alone.
        var grabbed = AudioClipLayout.Part.body
        var resize = false
        // A note clip's start, or else its end.
        var edge: Bool?
        switch hit {
        case .pattern: resize = onResizeEdge(p, of: rect)
        case .audio(let clip): grabbed = part(of: clip, at: p, in: rect)
        case .notes:
            if onResizeEdge(p, of: rect) { edge = false } else if onStartEdge(p, of: rect) { edge = true }
        }
        model.cueTrack = nil
        var selection = model.selectedClips
        var collapse: UInt64?
        if event.modifierFlags.contains(.shift) {
            // Shift adds a clip to the selection, or takes it out.
            if selection.remove(key) == nil { selection.insert(key) }
            guard selection.contains(key) else {
                model.select(clips: selection)
                return
            }
        } else if !selection.contains(key) || resize || grabbed != .body || edge != nil {
            selection = [key]
        } else if selection.count > 1 {
            collapse = key
        }
        // The detail panel shows the clip that was clicked.
        model.select(clips: selection, focus: key)
        if selection.count == 1 {
            switch hit {
            case .pattern(let clip) where resize:
                drag = .resize(clip: clip, repeats: Int(clip.repeats))
                return
            case .notes(let clip) where edge != nil:
                let start = edge == true
                drag = .noteTrim(NoteTrimDrag(clip: clip, start: start, beat: start ? clip.at : clip.at + clip.lengthBeats))
                return
            case .audio(let clip) where grabbed != .body:
                let shape = audioLayout(clip)
                switch grabbed {
                case .start: drag = .trim(TrimDrag(clip: clip, layout: shape, start: true, beat: shape.start))
                case .end: drag = .trim(TrimDrag(clip: clip, layout: shape, start: false, beat: shape.end))
                case .fadeIn: drag = .fade(FadeDrag(clip: clip, layout: shape, fadeIn: true, beats: shape.fadeIn))
                case .fadeOut: drag = .fade(FadeDrag(clip: clip, layout: shape, fadeIn: false, beats: shape.fadeOut))
                case .body: break
                }
                return
            default: break
            }
        }
        let placed = model.placedClips().filter { selection.contains($0.key) }
        let tracks = placed.map(\.track)
        guard let first = placed.map(\.at).min(), let top = tracks.min(), let bottom = tracks.max() else { return }
        // Pattern clips stay inside the song; an audio clip takes the song's end with it.
        let room = placed.filter { $0.audio == nil }.map { layout.lengthBeats - $0.end }.min() ?? Self.farBeats
        drag = .clips(ClipDrag(
            start: p,
            origin: Dictionary(uniqueKeysWithValues: placed.map { ($0.key, ($0.at, $0.track)) }),
            range: min(0, -first)...max(0, room),
            rowRange: -top...max(0, model.arrangement.tracks.count - 1 - bottom),
            collapse: collapse
        ))
    }

    /// The beat a pointer at `x` means for an edge or a dropped clip: on the
    /// grid unless `free`, and not before the song. It may be past the song's
    /// end, which an audio clip takes with it.
    private func snappedBeat(atX x: CGFloat, free: Bool) -> Double {
        let raw = layout.beat(atX: x)
        let grid = layout.grid
        return max(0, free ? (raw * 1000).rounded() / 1000 : (raw / grid).rounded() * grid)
    }

    override func mouseDragged(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        switch drag {
        case .loop(let anchor, let region):
            let x = max(p.x, TimelineLayout.headerWidth)
            if abs(x - anchor) >= 3 || region != nil {
                drag = .loop(anchor: anchor, region: layout.loop(fromX: anchor, toX: x))
            }
        case .slider(var s):
            s.value = Fader.dragged(s.value, byX: p.x - s.lastX, perPoint: s.perPoint,
                                    fine: event.modifierFlags.contains(.shift), in: s.range)
            s.lastX = p.x
            let shown = Fader.stepped(s.value, step: s.step)
            if shown != s.shown {
                s.shown = shown
                s.moved = true
                held[s.slider] = Held(value: shown, until: nil)
                showLevel(s.slider, shown)
                model.drag(s.edit(shown), gesture: s.gesture)
            }
            drag = .slider(s)
        case .clips(var d):
            if !d.moved, hypot(p.x - d.start.x, p.y - d.start.y) < 3 { return }
            d.moved = true
            d.by = layout.move(byX: p.x - d.start.x, free: event.modifierFlags.contains(.option), within: d.range)
            d.rows = min(max(trackIndex(atY: p.y) - trackIndex(atY: d.start.y), d.rowRange.lowerBound), d.rowRange.upperBound)
            let tracks = model.arrangement.tracks
            for (key, origin) in d.origin {
                clips[key]?.at = Animated(origin.at + d.by)
                if tracks.indices.contains(origin.track + d.rows), let row = rows[.track(tracks[origin.track + d.rows].key)] {
                    clips[key]?.y = Animated(row.y.to)
                }
            }
            for (key, origin) in d.origin {
                // An audio clip's audio moves with it, and a note clip's notes.
                if let audio = clips[key]?.audio { clips[key]?.anchor = Animated(audioLayout(audio).fileStart + d.by) }
                if clips[key]?.notes != nil { clips[key]?.anchor = Animated(origin.at + d.by) }
            }
            drag = .clips(d)
        case .trim(var d):
            let wanted = snappedBeat(atX: p.x, free: event.modifierFlags.contains(.option))
            let beat = d.start ? d.layout.start(draggedTo: wanted) : d.layout.end(draggedTo: wanted)
            if beat != d.beat {
                d.beat = beat
                d.moved = true
                // The audio stays where it is; the clip shows more or less of it.
                let (from, to) = d.start ? (beat, d.layout.end) : (d.layout.start, beat)
                clips[d.clip.key]?.at = Animated(from)
                clips[d.clip.key]?.length = Animated(to - from)
            }
            drag = .trim(d)
        case .noteTrim(var d):
            // On the grid or, with Option, off it; a grid step long at the
            // least, and inside the song. The notes stay where they are.
            let free = event.modifierFlags.contains(.option)
            let least = free ? 0.001 : layout.grid
            let (start, end) = (d.clip.at, d.clip.at + d.clip.lengthBeats)
            let wanted = snappedBeat(atX: p.x, free: free)
            let beat = d.start ? min(wanted, end - least) : min(max(wanted, start + least), layout.lengthBeats)
            if beat != d.beat, beat >= 0 {
                d.beat = beat
                d.moved = true
                let (from, to) = d.start ? (beat, end) : (start, beat)
                clips[d.clip.key]?.at = Animated(from)
                clips[d.clip.key]?.length = Animated(to - from)
            }
            drag = .noteTrim(d)
        case .fade(var d):
            let beat = layout.beat(atX: p.x)
            let tempo = model.arrangement.tempo
            let longest = AudioClipLayout.beats(ms: 10_000, tempo: tempo)
            let beats = min(d.fadeIn ? d.layout.fadeIn(draggedTo: beat) : d.layout.fadeOut(draggedTo: beat), longest)
            if beats != d.beats {
                d.beats = beats
                d.moved = true
                let ms = (AudioClipLayout.ms(beats: beats, tempo: tempo) * 10).rounded() / 10
                if d.fadeIn { clips[d.clip.key]?.audio?.fadeInMs = ms } else { clips[d.clip.key]?.audio?.fadeOutMs = ms }
            }
            drag = .fade(d)
        case .resize(let clip, let repeats):
            let wanted = layout.repeats(atX: p.x, clipAt: clip.at, patternBeats: clip.patternBeats)
            if wanted != repeats {
                clips[clip.key]?.repeats = wanted
                clips[clip.key]?.length = Animated(clip.patternBeats * Double(wanted))
                drag = .resize(clip: clip, repeats: wanted)
            }
        case .point(var d):
            let scale = ValueScale(min: d.lane.min, max: d.lane.max, log: d.lane.log)
            let wanted = layout.snapped(layout.beat(atX: p.x), free: event.modifierFlags.contains(.option))
            let at = min(max(wanted, d.range.lowerBound), d.range.upperBound)
            let value = scale.value(atY: p.y, in: d.rect)
            if at != d.at || value != d.value {
                d.at = at
                d.value = value
                d.moved = true
                heldPoints[d.point] = HeldPoint(at: at, value: value, until: nil)
                model.drag(.pointSet(point: d.point, at: at, value: value, hold: nil), gesture: d.gesture)
            }
            drag = .point(d)
        case .reorder(var r):
            if r.slot == nil, abs(p.y - r.startY) < 4 { return }
            // The gap the pointer is nearest: above the first row whose middle is below it.
            let rows = siblings(of: r.row).compactMap { frame(of: $0) }
            r.slot = rows.filter { $0.top + $0.height / 2 < p.y }.count
            drag = .reorder(r)
        case nil:
            return
        }
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        let ended = drag
        drag = nil
        switch ended {
        case .loop(let anchor, let region):
            if let region {
                model.setLoop(start: region.start, length: region.length)
            } else {
                // A click in the loop strip sets the start position like any other.
                model.locate(layout.target(atX: anchor, free: event.modifierFlags.contains(.option)))
            }
        case .slider(let s):
            model.endDrag()
            // The level stays as dragged until the host's song has it.
            if s.moved { held[s.slider]?.until = CACurrentMediaTime() + 1 }
        case .clips(let d):
            if d.moved {
                model.edit(.clipsMove(clips: d.origin.keys.sorted(), by: d.by, rows: Int32(d.rows)))
            } else if let key = d.collapse {
                model.select(clips: [key])
            }
        case .resize(let clip, let repeats):
            if repeats != Int(clip.repeats) { model.edit(.clipRepeats(clip: clip.key, repeats: UInt32(repeats))) }
        case .trim(let d):
            // Sent when the drag ends, as a move is: each trim has the engine
            // prepare another part of the file.
            if d.moved { model.edit(.audioTrim(clip: d.clip.key, start: d.start ? d.beat : nil, end: d.start ? nil : d.beat)) }
        case .noteTrim(let d):
            if d.moved { model.edit(.clipTrim(clip: d.clip.key, start: d.start ? d.beat : nil, end: d.start ? nil : d.beat)) }
        case .fade(let d):
            if d.moved {
                let ms = (AudioClipLayout.ms(beats: d.beats, tempo: model.arrangement.tempo) * 10).rounded() / 10
                model.edit(.audioFade(clip: d.clip.key, fadeInMs: d.fadeIn ? ms : nil, fadeOutMs: d.fadeIn ? nil : ms))
            }
        case .point(let d):
            model.endDrag()
            // The point stays as dragged until the host's song has it.
            if d.moved { heldPoints[d.point]?.until = CACurrentMediaTime() + 1 }
        case .reorder(let r):
            // Taking the row out moves the gaps below it up by one.
            if let slot = r.slot {
                let index = slot > r.from ? slot - 1 : slot
                if index != r.from { model.edit(.move(row: r.row.row, index: UInt32(index))) }
            }
        case nil:
            return
        }
        needsDisplay = true
        link?.isPaused = false
    }

    override func mouseMoved(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        var resize = false
        var hover: UInt64?
        if p.x >= TimelineLayout.headerWidth, p.y >= TimelineLayout.rulerHeight, lane(at: p) == nil, let hit = clip(at: p) {
            switch hit.hit {
            case .pattern: resize = onResizeEdge(p, of: hit.rect)
            case .audio(let clip):
                hover = clip.key
                resize = part(of: clip, at: p, in: hit.rect) != .body
            case .notes: resize = onResizeEdge(p, of: hit.rect) || onStartEdge(p, of: hit.rect)
            }
        }
        (resize ? NSCursor.resizeLeftRight : NSCursor.arrow).set()
        // An audio clip under the pointer shows its fades' handles.
        if hover != hoverClip {
            hoverClip = hover
            needsDisplay = true
        }
    }

    override func mouseExited(with event: NSEvent) {
        if hoverClip != nil {
            hoverClip = nil
            needsDisplay = true
        }
    }

    /// Drops a drag that has sent nothing yet. True if there was one.
    private func cancelDrag() -> Bool {
        switch drag {
        case .clips, .resize, .trim, .noteTrim, .fade, .reorder, .loop:
            drag = nil
            show(model.arrangement, animated: true)
            return true
        case .slider, .point, nil:
            return false
        }
    }

    // MARK: Dropped samples

    /// The height of the tracks, among the rows: where a new track would go.
    private var tracksHeight: CGFloat {
        model.arrangement.tracks.last.flatMap { rows[.track($0.key)] }.map { CGFloat($0.y.to + $0.height.to) } ?? 0
    }

    /// The file a drag carries: a sample from the browser, or an audio or
    /// MIDI file from anywhere else, with its length when that is known.
    private func sample(of drag: NSDraggingInfo) -> (path: String, name: String, note: String?, seconds: Double?)? {
        if drag.draggingSource != nil, let sample = model.browser.dragged {
            return (sample.path, Browser.padName(of: sample), sample.rootNote, sample.seconds > 0 ? sample.seconds : nil)
        }
        let urls = drag.draggingPasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL]
        guard let url = urls?.first, Browser.extensions.contains(url.pathExtension.lowercased()) || Self.isMIDI(url.path) else { return nil }
        // The file is read once for a drag, not at every move of it.
        if dropFile?.path != url.path {
            if Self.isMIDI(url.path) {
                dropFile = (url.path, nil, midiFileBeats(path: url.path))
            } else {
                let file = try? AVAudioFile(forReading: url)
                let rate = file?.fileFormat.sampleRate ?? 0
                dropFile = (url.path, rate > 0 ? file.map { Double($0.length) / rate } : nil, nil)
            }
        }
        return (url.path, url.deletingPathExtension().lastPathComponent, nil, dropFile?.seconds)
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        draggingUpdated(sender)
    }

    /// Where a sample at a point would land: in the headers as a pad, and
    /// on the timeline as an audio clip at the grid line nearest the point,
    /// or with `free` off the grid.
    private func landing(at p: CGPoint, free: Bool) -> DropTarget {
        var track: UInt64?
        if p.y >= TimelineLayout.rulerHeight, let (id, _) = row(atY: p.y), case .track(let key) = id { track = key }
        if p.x < TimelineLayout.headerWidth {
            // In the headers it is a sound for patterns to play.
            return track.map { .pad($0) } ?? .newTrack
        }
        let beat = snappedBeat(atX: p.x, free: free)
        // A MIDI track holds note clips only: an audio clip dropped on one
        // lands on a new track.
        if let key = track, model.arrangement.tracks.first(where: { $0.key == key })?.midi == true { return .newClip(at: beat) }
        return track.map { .clip(track: $0, at: beat) } ?? .newClip(at: beat)
    }

    /// Where a MIDI file at a point would land: on a MIDI track's lane as a
    /// note clip there, elsewhere on the timeline on a new MIDI track, and in
    /// the headers or the ruler nowhere.
    private func midiLanding(at p: CGPoint, free: Bool) -> DropTarget? {
        guard p.x >= TimelineLayout.headerWidth, p.y >= TimelineLayout.rulerHeight else { return nil }
        let beat = snappedBeat(atX: p.x, free: free)
        if let (id, _) = row(atY: p.y), case .track(let key) = id, model.arrangement.tracks.first(where: { $0.key == key })?.midi == true {
            return .clip(track: key, at: beat)
        }
        return .newClip(at: beat)
    }

    static func isMIDI(_ path: String) -> Bool {
        ["mid", "midi"].contains(URL(fileURLWithPath: path).pathExtension.lowercased())
    }

    private func land(path: String, name: String, note: String?, on target: DropTarget) {
        if Self.isMIDI(path) {
            switch target {
            case .clip(let track, let at): model.importMIDI(path: path, to: track, at: at)
            case .newClip(let at): model.importMIDI(path: path, to: nil, at: at)
            case .pad, .newTrack: break
            }
            return
        }
        switch target {
        case .pad(let track): model.addSample(path: path, name: name, note: note, to: track)
        case .newTrack: model.addSample(path: path, name: name, note: note, to: nil)
        case .clip(let track, let at): model.addClip(path: path, name: name, note: note, to: track, at: at)
        case .newClip(let at): model.addClip(path: path, name: name, note: note, to: nil, at: at)
        }
    }

    /// Lands an audio or MIDI file at a point of the view, as a drag from
    /// the Finder that ends there does. False for another kind of file, or a
    /// MIDI file where it cannot land.
    @discardableResult
    func drop(file url: URL, at p: CGPoint) -> Bool {
        let name = url.deletingPathExtension().lastPathComponent
        if Self.isMIDI(url.path) {
            guard let target = midiLanding(at: p, free: false) else { return false }
            land(path: url.path, name: name, note: nil, on: target)
            return true
        }
        guard Browser.extensions.contains(url.pathExtension.lowercased()) else { return false }
        land(path: url.path, name: name, note: nil, on: landing(at: p, free: false))
        return true
    }

    private func deviceKind(_ sender: NSDraggingInfo) -> String? {
        (DeviceChain.kinds + ["sampler"]).first {
            sender.draggingPasteboard.types?.contains(NSPasteboard.PasteboardType(Browser.deviceType + "." + $0)) == true
        }
    }

    private func deviceLanding(_ kind: String, at point: CGPoint) -> (valid: Bool, row: RowID?) {
        guard point.y >= TimelineLayout.rulerHeight else { return (false, nil) }
        if let (row, _) = row(atY: point.y) {
            return (point.x < TimelineLayout.headerWidth && model.canAddBrowserDevice(kind, to: row), row)
        }
        return (kind == "sampler" && point.y >= TimelineLayout.rulerHeight + tracksHeight, nil)
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        if let kind = deviceKind(sender) {
            return deviceLanding(kind, at: convert(sender.draggingLocation, from: nil)).valid ? .copy : []
        }
        guard let file = sample(of: sender) else { return [] }
        let p = convert(sender.draggingLocation, from: nil)
        let free = NSEvent.modifierFlags.contains(.option)
        let target = Self.isMIDI(file.path) ? midiLanding(at: p, free: free) : landing(at: p, free: free)
        if target != dropTarget {
            dropTarget = target
            needsDisplay = true
        }
        return target == nil ? [] : .copy
    }

    override func draggingExited(_ sender: NSDraggingInfo?) {
        dropTarget = nil
        dropFile = nil
        needsDisplay = true
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        if let kind = deviceKind(sender) {
            let landing = deviceLanding(kind, at: convert(sender.draggingLocation, from: nil))
            guard landing.valid else { return false }
            model.addBrowserDevice(kind, to: landing.row)
            return true
        }
        defer {
            dropTarget = nil
            dropFile = nil
            model.browser.dragged = nil
            needsDisplay = true
        }
        guard let sample = sample(of: sender), let target = dropTarget else { return false }
        land(path: sample.path, name: sample.name, note: sample.note, on: target)
        return true
    }

    // MARK: Keys

    override func keyDown(with event: NSEvent) {
        guard event.modifierFlags.intersection([.command, .control]).isEmpty else {
            super.keyDown(with: event)
            return
        }
        // The menus have most of these keys; this is for when they do not act.
        switch event.specialKey {
        case .delete?, .backspace?, .deleteForward?: model.deleteSelection()
        case .leftArrow?: nudge(by: -layout.grid, rows: 0)
        case .rightArrow?: nudge(by: layout.grid, rows: 0)
        case .upArrow?: nudge(by: 0, rows: -1)
        case .downArrow?: nudge(by: 0, rows: 1)
        case .carriageReturn?, .enter?: model.returnToStart()
        default:
            switch event.charactersIgnoringModifiers {
            case " ": model.togglePlay()
            case "l": model.toggleLoop()
            case "\u{1b}":
                if !cancelDrag() {
                    model.select(clips: [])
                    model.select(point: nil)
                }
            default: super.keyDown(with: event)
            }
        }
    }

    override func selectAll(_ sender: Any?) {
        model.selectAllClips()
    }

    /// Moves the selected clips by a grid step or to the next track, if they
    /// stay in the song. An audio clip may pass the song's end, which grows.
    private func nudge(by: Double, rows: Int) {
        let placed = model.placedClips().filter { model.selectedClips.contains($0.key) }
        let tracks = placed.map(\.track)
        guard let first = placed.map(\.at).min(), let top = tracks.min(), let bottom = tracks.max() else { return }
        let last = placed.filter { $0.audio == nil }.map(\.end).max() ?? 0
        guard first + by >= 0, last + by <= layout.lengthBeats,
              top + rows >= 0, bottom + rows < model.arrangement.tracks.count else {
            NSSound.beep()
            return
        }
        model.edit(.clipsMove(clips: placed.map(\.key).sorted(), by: by, rows: Int32(rows)))
    }

    // MARK: Names

    /// Lets the person type a track's or a return's name where it is shown.
    private func beginRename(_ id: RowID) {
        endRename(commit: true)
        guard id != .master, let row = rows[id], let frame = frame(of: id) else { return }
        let name = header(id, row, top: frame.top).name
        let field = NSTextField(string: row.name)
        field.font = Self.nameFont
        field.isBordered = false
        field.focusRingType = .none
        field.drawsBackground = true
        field.backgroundColor = Theme.gray(0.08)
        field.textColor = Theme.text
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        field.delegate = self
        field.frame = CGRect(x: name.minX - 2, y: name.minY - 1, width: name.width + 4, height: 18)
        addSubview(field)
        renaming = (id, field)
        window?.makeFirstResponder(field)
    }

    private func endRename(commit: Bool) {
        guard let (row, field) = renaming else { return }
        renaming = nil
        let name = field.stringValue.trimmingCharacters(in: .whitespaces)
        field.delegate = nil
        field.removeFromSuperview()
        if window?.firstResponder !== self { window?.makeFirstResponder(self) }
        if commit, !name.isEmpty, name != rows[row]?.name {
            model.edit(.rename(row: row.row, to: name))
        }
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        switch selector {
        case #selector(insertNewline(_:)): endRename(commit: true)
        case #selector(cancelOperation(_:)): endRename(commit: false)
        default: return false
        }
        return true
    }

    func controlTextDidEndEditing(_ notification: Notification) {
        endRename(commit: true)
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

    /// One line of text, shortened with an ellipsis where it does not fit.
    private func text(_ string: String, in rect: CGRect, font: NSFont, color: NSColor, align: NSTextAlignment = .left) {
        guard rect.width > 4 else { return }
        // Text that fits is drawn from a line laid out once.
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

    private func fill(rounded rect: CGRect, radius: CGFloat = 3, _ color: NSColor) {
        color.setFill()
        NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius).fill()
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
        if case .loop(_, let region?) = drag {
            return (LoopRegion(start: region.start, length: region.length), true)
        }
        if let region = model.transport.loopRegion { return (region, true) }
        return model.idleLoop.map { ($0, false) }
    }

    override func draw(_ dirtyRect: NSRect) {
        let now = CACurrentMediaTime()
        defer {
            if measuring != nil { measuring?.times.draws.append((CACurrentMediaTime() - now) * 1000) }
        }
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

        // Where a dragged file would land: in a track's header as a pad, on
        // its lane as a clip, or under the tracks on a new track.
        let under = max(ruler, layout.y(tracksHeight))
        switch dropTarget {
        case .pad(let key):
            if let frame = frame(of: .track(key)) {
                let top = max(frame.top, ruler)
                fill(CGRect(x: 0, y: top, width: header - 1, height: frame.top + frame.height - 1 - top), Theme.insertion.withAlphaComponent(0.3))
            }
        case .newTrack:
            fill(CGRect(x: 0, y: under - 1.5, width: header - 1, height: 2), Theme.insertion)
        case .clip(let key, let at):
            if let frame = frame(of: .track(key)) { drawDropped(at: at, top: frame.top) }
        case .newClip(let at):
            fill(CGRect(x: 0, y: under - 1.5, width: bounds.width, height: 2), Theme.insertion)
            drawDropped(at: at, top: under)
        case nil: break
        }

        // Where a dragged row would land.
        if case .reorder(let r) = drag, let slot = r.slot {
            let frames = siblings(of: r.row).compactMap { frame(of: $0) }
            if let y = slot < frames.count ? frames[slot].top : frames.last.map({ $0.top + $0.height }), y >= ruler {
                fill(CGRect(x: 0, y: y - 1.5, width: bounds.width, height: 2), Theme.insertion)
            }
        }
    }

    /// The outline of the clip a dragged file would make: from its beat for
    /// its length, or for a bar when its length is not known.
    private func drawDropped(at beat: Double, top: CGFloat) {
        let beats = dropLength ?? layout.beatsPerBar
        let rect = clipRect(at: beat, length: beats, top: top)
        clipped(to: CGRect(x: TimelineLayout.headerWidth, y: TimelineLayout.rulerHeight,
                           width: bounds.width - TimelineLayout.headerWidth, height: bounds.height - TimelineLayout.rulerHeight)) {
            let shape = NSBezierPath(roundedRect: rect.insetBy(dx: 0.75, dy: 0.75), xRadius: 3, yRadius: 3)
            Theme.insertion.withAlphaComponent(0.25).setFill()
            shape.fill()
            Theme.insertion.setStroke()
            shape.lineWidth = 1.5
            shape.stroke()
        }
    }

    /// The beats the dragged file would cover: a MIDI file's clip, or an
    /// audio file at its own tempo.
    private var dropLength: Double? {
        if model.browser.dragged == nil, let beats = dropFile?.beats { return beats }
        let seconds = model.browser.dragged.map(\.seconds) ?? dropFile?.seconds
        return seconds.flatMap { $0 > 0 ? $0 * model.arrangement.tempo / 60 : nil }
    }

    private func drawLanes(_ order: [(key: RowID, value: RowVisual)], at now: CFTimeInterval) {
        let header = TimelineLayout.headerWidth
        let ruler = TimelineLayout.rulerHeight
        let width = bounds.width - header
        for (_, row) in order {
            let y = layout.y(CGFloat(row.y.value(at: now)))
            let alpha = CGFloat(row.alpha.value(at: now))
            fill(CGRect(x: header, y: y, width: width, height: CGFloat(row.height.value(at: now)) - 1),
                 (row.kind == .track ? Theme.lane : Theme.busLane).withAlphaComponent(alpha))
        }
        // Automation lanes are a shade darker than the row they belong to.
        var lanes: [(lane: LaneView, rect: CGRect, alpha: CGFloat)] = []
        for (id, row) in order {
            let alpha = CGFloat(row.alpha.value(at: now))
            for item in laneRects(id, row, top: layout.y(CGFloat(row.y.value(at: now)))) {
                fill(item.rect, Theme.automationLane.withAlphaComponent(alpha))
                lanes.append((item.lane, item.rect, alpha))
            }
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

        // Selected clips over the others, as a dragged clip should be, and
        // newer clips over older.
        let selected = model.selectedClips
        for chosen in [false, true] {
            for key in clipOrder where selected.contains(key) == chosen {
                if let clip = clips[key] { drawClip(key, clip, selected: chosen, at: now) }
            }
        }
        for item in lanes { drawLane(item.lane, in: item.rect, alpha: item.alpha) }

        let end = layout.x(layout.lengthBeats)
        if end < bounds.width {
            fill(CGRect(x: end, y: ruler, width: bounds.width - end, height: bounds.height - ruler), Theme.pastEnd)
            fill(CGRect(x: end.rounded(), y: ruler, width: 1, height: bounds.height - ruler), Theme.gray(1, 0.25))
        }
        fill(CGRect(x: layout.x(model.transport.cue).rounded(), y: ruler, width: 1, height: bounds.height - ruler),
             Theme.cue.withAlphaComponent(0.85))
    }

    private func drawClip(_ key: UInt64, _ clip: ClipVisual, selected: Bool, at now: CFTimeInterval) {
        let alpha = CGFloat(clip.alpha.value(at: now))
        let rect = clipRect(at: clip.at.value(at: now), length: clip.length.value(at: now),
                            top: layout.y(CGFloat(clip.y.value(at: now))))
        guard rect.maxX >= TimelineLayout.headerWidth, rect.minX <= bounds.width,
              rect.maxY >= TimelineLayout.rulerHeight, rect.minY <= bounds.height else { return }
        let color = clip.muted ? Theme.gray(0.45) : clip.color
        let shape = NSBezierPath(roundedRect: rect, xRadius: 3, yRadius: 3)
        color.withAlphaComponent((selected ? 0.62 : 0.42) * alpha).setFill()
        shape.fill()
        clipped(to: rect) {
            // A title strip in the track's color over a dimmer body.
            let title = CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: Self.titleHeight)
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
            if let audio = clip.audio {
                drawAudio(audio, of: clip, in: body(of: rect), alpha: alpha, handles: selected || hoverClip == key, at: now)
            } else if let notes = clip.notes {
                drawNotes(notes, of: clip, in: body(of: rect), alpha: alpha, at: now)
            } else {
                drawWaveform(of: clip, in: body(of: rect), alpha: alpha)
            }
            // The name is cut where the clip ends: a clip too narrow for a
            // few letters has none.
            if rect.width >= 16 {
                let label = clip.repeats > 1 ? "\(clip.pattern) ×\(clip.repeats)" : clip.pattern
                _ = TextLines.shared.draw(label, in: CGRect(x: rect.minX + 4, y: rect.minY, width: rect.width - 6, height: 13),
                                          font: Self.clipFont, color: Theme.gray(0.08, alpha), cuts: true)
            }
        }
        if selected {
            Theme.selectedClip.withAlphaComponent(alpha).setStroke()
            let outline = NSBezierPath(roundedRect: rect.insetBy(dx: 0.75, dy: 0.75), xRadius: 3, yRadius: 3)
            outline.lineWidth = 1.5
            outline.stroke()
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

    /// What a note clip shows under its title: its notes, each a bar from its
    /// start to its end or the clip's, from the lowest pitch at the bottom to
    /// the highest at the top, an octave at least. Notes outside the clip do
    /// not play and are not drawn. While an edge is dragged the notes stay
    /// where they are, and the clip shows more or fewer of them.
    private func drawNotes(_ clip: NoteClipView, of visual: ClipVisual, in body: CGRect, alpha: CGFloat,
                           at now: CFTimeInterval) {
        let origin = visual.anchor.value(at: now)
        let start = visual.at.value(at: now) - origin
        let end = start + visual.length.value(at: now)
        let playing = clip.notes.filter { $0.at >= start - 1e-9 && $0.at < end }
        guard body.height >= 6, let low = playing.map(\.pitch).min(), let high = playing.map(\.pitch).max() else { return }
        let span = max(high - low + 1, 12)
        let bottom = Int32(Double(low + high) / 2 - Double(span) / 2 + 0.5)
        let row = (body.height - 4) / CGFloat(span)
        let ink = Theme.gray(0.08, 0.8 * alpha)
        for note in playing {
            let from = layout.x(origin + note.at)
            let to = layout.x(origin + min(note.at + note.duration, end))
            let y = body.maxY - 2 - CGFloat(note.pitch - bottom + 1) * row
            fill(CGRect(x: from, y: y, width: max(to - from - 1, 1), height: max(row - 1, 1)), ink)
        }
    }

    /// What an audio clip shows under its title: its file's waveform at the
    /// clip's level, the beats of the file's beat map, and its fades, with
    /// their handles when the clip is selected or under the pointer.
    private func drawAudio(_ audio: AudioClipView, of clip: ClipVisual, in body: CGRect, alpha: CGFloat, handles: Bool,
                           at now: CFTimeInterval) {
        guard body.height >= 8, let context = NSGraphicsContext.current?.cgContext else { return }
        let start = clip.at.value(at: now)
        let length = clip.length.value(at: now)
        let anchor = clip.anchor.value(at: now)
        let visible = CGRect(x: TimelineLayout.headerWidth, y: body.minY, width: bounds.width - TimelineLayout.headerWidth, height: body.height)
        let perBeat = audio.secondsPerBeat

        // The file from where the clip starts in it, at the clip's level.
        if let wave = model.fileWaveform(audio.file), perBeat > 0 {
            let framesPerBeat = perBeat * wave.framesPerSecond
            let columns = wave.columns(
                in: body, clippedTo: visible, fromFrame: (start - anchor) * framesPerBeat,
                framesPerPoint: framesPerBeat / Double(layout.pixelsPerBeat), step: 1 / (window?.backingScaleFactor ?? 2),
                gain: pow(10, audio.gainDb / 20)
            )
            context.setFillColor(Theme.waveform.withAlphaComponent(0.78 * alpha).cgColor)
            context.fill(columns)
        } else {
            fill(CGRect(x: body.minX, y: body.midY.rounded(), width: body.width, height: 1), Theme.waveform.withAlphaComponent(0.3 * alpha))
        }

        // The file's beats, where its beat map has them: a tick at the foot of
        // the waveform, taller on a downbeat, once they are a few points apart.
        if let file = model.file(audio.file), file.beats.count > 1, perBeat > 0 {
            let seconds = { (x: CGFloat) in (self.layout.beat(atX: x) - anchor) * perBeat }
            let (from, to) = (seconds(max(body.minX, visible.minX)), seconds(min(body.maxX, visible.maxX)))
            let apart = CGFloat((file.beats[1].seconds - file.beats[0].seconds) / perBeat) * layout.pixelsPerBeat
            let every = apart >= 5
            if every || apart * 4 >= 5 {
                var ticks: [CGRect] = []
                var bars: [CGRect] = []
                // The first beat in view, by halving.
                var (low, high) = (0, file.beats.count)
                while low < high {
                    let mid = (low + high) / 2
                    if file.beats[mid].seconds < from { low = mid + 1 } else { high = mid }
                }
                for beat in file.beats[low...] {
                    guard beat.seconds <= to else { break }
                    let x = layout.x(anchor + beat.seconds / perBeat).rounded()
                    if beat.downbeat {
                        bars.append(CGRect(x: x, y: body.maxY - 9, width: 1, height: 9))
                    } else if every {
                        ticks.append(CGRect(x: x, y: body.maxY - 4, width: 1, height: 4))
                    }
                }
                context.setFillColor(Theme.gray(1, 0.45 * alpha).cgColor)
                context.fill(ticks)
                context.setFillColor(Theme.gray(1, 0.85 * alpha).cgColor)
                context.fill(bars)
            }
        }

        // The fades: what each takes away is shaded, under the curve it plays.
        let tempo = model.arrangement.tempo
        let shape = AudioClipLayout(
            start: start, end: start + length, fileStart: anchor, fileEnd: nil,
            fadeIn: min(AudioClipLayout.beats(ms: audio.fadeInMs, tempo: tempo), length),
            fadeOut: min(AudioClipLayout.beats(ms: audio.fadeOutMs, tempo: tempo), length)
        )
        let marks = shape.handles(in: body)
        let linear = audio.fadeCurve == "linear"
        for (edge, mark) in [(body.minX, marks.fadeIn.x), (body.maxX, marks.fadeOut.x)] where abs(mark - edge) >= 1.5 {
            // From silence at the clip's edge to full at the handle.
            let steps = max(2, min(48, Int(abs(mark - edge) / 3)))
            let points = (0...steps).map { i -> CGPoint in
                let t = Double(i) / Double(steps)
                return CGPoint(x: edge + (mark - edge) * CGFloat(t), y: body.maxY - body.height * CGFloat(AudioClipLayout.level(t, linear: linear)))
            }
            let taken = NSBezierPath()
            taken.move(to: CGPoint(x: edge, y: body.minY))
            for point in points { taken.line(to: point) }
            taken.close()
            Theme.gray(0, 0.4 * alpha).setFill()
            taken.fill()
            let curve = NSBezierPath()
            curve.move(to: points[0])
            for point in points.dropFirst() { curve.line(to: point) }
            curve.lineWidth = 1
            Theme.gray(1, 0.75 * alpha).setStroke()
            curve.stroke()
        }
        guard handles, body.width >= 16 else { return }
        let side = AudioClipLayout.handle
        for mark in [marks.fadeIn.x, marks.fadeOut.x] {
            // Kept inside the clip, where a fade of nothing would put it half out.
            let x = min(max(mark - side / 2, body.minX + 1), body.maxX - side - 1)
            let box = NSBezierPath(rect: CGRect(x: x, y: body.minY + 1, width: side, height: side))
            Theme.gray(0.95, alpha).setFill()
            box.fill()
            Theme.gray(0.1, alpha).setStroke()
            box.lineWidth = 1
            box.stroke()
        }
    }

    /// What a clip plays, under its title: its waveform, or while the host is
    /// still working that out, a line in its place.
    private func drawWaveform(of clip: ClipVisual, in body: CGRect, alpha: CGFloat) {
        guard body.height >= 8 else { return }
        guard let wave = clip.wave else {
            fill(CGRect(x: body.minX, y: body.midY.rounded(), width: body.width, height: 1), Theme.waveform.withAlphaComponent(0.3 * alpha))
            return
        }
        let visible = CGRect(x: TimelineLayout.headerWidth, y: body.minY, width: bounds.width - TimelineLayout.headerWidth, height: body.height)
        let columns = wave.peaks.columns(in: body, clippedTo: visible, fromBeat: wave.at, pixelsPerBeat: layout.pixelsPerBeat,
                                         step: 1 / (window?.backingScaleFactor ?? 2))
        guard !columns.isEmpty, let context = NSGraphicsContext.current?.cgContext else { return }
        context.setFillColor(Theme.waveform.withAlphaComponent(0.78 * alpha).cgColor)
        context.fill(columns)
    }

    /// A lane's values over time: a line through its points, which holds the
    /// first value before the first point and the last after the last.
    private func drawLane(_ lane: LaneView, in rect: CGRect, alpha: CGFloat) {
        guard rect.maxY >= TimelineLayout.rulerHeight, rect.minY <= bounds.height else { return }
        let placed = positions(lane, in: rect)
        guard let first = placed.first, let last = placed.last else { return }
        clipped(to: rect) {
            let line = NSBezierPath()
            line.move(to: CGPoint(x: min(rect.minX, first.at.x), y: first.at.y))
            line.line(to: first.at)
            let scale = ValueScale(min: lane.min, max: lane.max, log: lane.log)
            for (a, b) in zip(placed, placed.dropFirst()) {
                if a.point.hold {
                    // A point that holds keeps its value until the next.
                    line.line(to: CGPoint(x: b.at.x, y: a.at.y))
                } else if a.point.shape != 0, b.at.x - a.at.x >= 2 {
                    // A shaped segment is the curve it plays: bent in the
                    // parameter's own domain, as the engine bends it.
                    let from = heldPoints[a.point.key]?.value ?? a.point.value
                    let to = heldPoints[b.point.key]?.value ?? b.point.value
                    let ratio = lane.log && from > 0 && to > 0
                    let steps = max(8, min(64, Int((b.at.x - a.at.x) / 3)))
                    for i in 1..<steps {
                        let t = Double(i) / Double(steps)
                        let along = shapedProgress(t, shape: a.point.shape)
                        let value = ratio ? from * pow(to / from, along) : from + (to - from) * along
                        line.line(to: CGPoint(x: a.at.x + (b.at.x - a.at.x) * CGFloat(t), y: scale.y(value, in: rect)))
                    }
                }
                line.line(to: b.at)
            }
            line.line(to: CGPoint(x: max(rect.maxX, last.at.x), y: last.at.y))
            line.lineWidth = 1.5
            line.lineJoinStyle = .round
            Theme.automation.withAlphaComponent(alpha).setStroke()
            line.stroke()
            for (point, at) in placed {
                let selected = model.selectedPoint == point.key
                let radius: CGFloat = selected ? 4.5 : 3
                let dot = NSBezierPath(ovalIn: CGRect(x: at.x - radius, y: at.y - radius, width: 2 * radius, height: 2 * radius))
                (selected ? Theme.selectedClip : Theme.automation).withAlphaComponent(alpha).setFill()
                dot.fill()
                guard selected else { continue }
                Theme.automation.withAlphaComponent(alpha).setStroke()
                dot.lineWidth = 1.5
                dot.stroke()
                // The selected point's value, beside it.
                let value = heldPoints[point.key]?.value ?? point.value
                let x = at.x + 80 > rect.maxX ? at.x - 78 : at.x + 9
                let y = at.y - 6 < rect.minY ? rect.minY : min(at.y - 6, rect.maxY - 13)
                text(ValueScale.text(value, unit: lane.unit), in: CGRect(x: x, y: y, width: 70, height: 13),
                     font: Self.numberFont, color: Theme.text.withAlphaComponent(alpha), align: at.x + 80 > rect.maxX ? .right : .left)
            }
        }
    }

    /// The mark that shows a row's lanes: lit while they are shown, and in
    /// the lanes' color when the row has any.
    private func lanesBadge(_ row: RowVisual, in box: CGRect, shown: Bool, alpha: CGFloat) {
        fill(rounded: box, (shown ? Theme.automation : Theme.gray(1)).withAlphaComponent((shown ? 1 : 0.07) * alpha))
        let color = shown ? Theme.gray(0.08) : (row.automated ? Theme.automation : Theme.faintText)
        text("A", in: CGRect(x: box.minX, y: box.minY + 1.5, width: box.width, height: 11),
             font: Self.badgeFont, color: color.withAlphaComponent(alpha), align: .center)
    }

    /// A mute or solo badge, lit when on.
    private func badge(_ letter: String, on: Bool, color: NSColor, in box: CGRect, alpha: CGFloat) {
        fill(rounded: box, (on ? color : Theme.gray(1)).withAlphaComponent((on ? 1 : 0.07) * alpha))
        text(letter, in: CGRect(x: box.minX, y: box.minY + 1.5, width: box.width, height: 11),
             font: Self.badgeFont, color: (on ? Theme.gray(0.08) : Theme.faintText).withAlphaComponent(alpha), align: .center)
    }

    /// A level as a bar filled from the left.
    private func bar(_ rect: CGRect, filled: CGFloat, dim: Bool, alpha: CGFloat) {
        fill(rect, Theme.gray(0, 0.4 * alpha))
        fill(CGRect(x: rect.minX, y: rect.minY, width: rect.width * filled, height: rect.height),
             (dim ? Theme.gray(0.5) : Theme.gray(0.82)).withAlphaComponent(alpha))
    }

    private func drawHeader(_ id: RowID, _ row: RowVisual, at now: CFTimeInterval) {
        let top = layout.y(CGFloat(row.y.value(at: now)))
        let alpha = CGFloat(row.alpha.value(at: now))
        let rect = CGRect(x: 0, y: top, width: HeaderLayout.width, height: CGFloat(row.height.value(at: now)) - 1)
        guard rect.maxY >= TimelineLayout.rulerHeight, rect.minY <= bounds.height else { return }
        fill(rect, (row.kind == .track ? Theme.header : Theme.busHeader).withAlphaComponent(alpha))
        if model.selectedRow == id { fill(rect, Theme.selectedRow) }
        if let (level, tint) = glow(.row(id), at: now) {
            fill(rect, tint.withAlphaComponent(0.35 * level))
        }
        if let color = row.color {
            fill(CGRect(x: 0, y: top, width: 5, height: rect.height), (row.mute ? Theme.gray(0.45) : color).withAlphaComponent(alpha))
        }
        let header = header(id, row, top: top)
        let gain = row.gain.value(at: now)
        let textColor = (row.mute ? Theme.dimText : Theme.text).withAlphaComponent(alpha)
        let dim = Theme.dimText.withAlphaComponent(alpha)

        clipped(to: rect) {
            // The name, then the effect chain for as far as there is room.
            if renaming?.row != id {
                let name = NSMutableAttributedString(
                    string: row.name, attributes: [.font: Self.nameFont, .foregroundColor: textColor]
                )
                if !row.detail.isEmpty {
                    name.append(NSAttributedString(
                        string: "  " + row.detail, attributes: [.font: Self.smallFont, .foregroundColor: dim]
                    ))
                }
                name.addAttribute(.paragraphStyle, value: Self.styles[.left]!, range: NSRange(location: 0, length: name.length))
                name.draw(with: header.name, options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
            }

            lanesBadge(row, in: header.auto, shown: header.lanes != nil, alpha: alpha)
            guard row.kind == .track else {
                // Returns and the master: one line, with the volume as a value to drag.
                fill(rounded: header.volume, Theme.control.withAlphaComponent(0.28 * alpha))
                text(Fader.text(db: gain), in: header.volumeText, font: Self.numberFont, color: dim, align: .right)
                if row.kind == .bus { badge("M", on: row.mute, color: Theme.mute, in: header.mute, alpha: alpha) }
                return
            }

            badge("M", on: row.mute, color: Theme.mute, in: header.mute, alpha: alpha)
            badge("S", on: row.solo, color: Theme.solo, in: header.solo, alpha: alpha)
            if header.folds {
                // A mark that points down while the sends are shown.
                let box = header.fold.insetBy(dx: 2.5, dy: 2.5)
                let mark = NSBezierPath()
                if header.sends > 0 {
                    mark.move(to: CGPoint(x: box.minX, y: box.minY + 1))
                    mark.line(to: CGPoint(x: box.maxX, y: box.minY + 1))
                    mark.line(to: CGPoint(x: box.midX, y: box.maxY))
                } else {
                    mark.move(to: CGPoint(x: box.minX + 1, y: box.minY))
                    mark.line(to: CGPoint(x: box.minX + 1, y: box.maxY))
                    mark.line(to: CGPoint(x: box.maxX, y: box.midY))
                }
                mark.close()
                (row.sends.isEmpty ? Theme.faintText : Theme.dimText).withAlphaComponent(alpha).setFill()
                mark.fill()
            }

            // Volume as a bar from −60 to +6 dB, with the value and the pan.
            bar(header.volumeBar, filled: Fader.fraction(gain, in: Fader.gain), dim: row.mute, alpha: alpha)
            text(Fader.text(db: gain), in: header.volumeText, font: Self.numberFont, color: dim)
            fill(rounded: header.pan, Theme.control.withAlphaComponent(0.28 * alpha))
            text(Fader.text(pan: row.pan.value(at: now)), in: header.pan.insetBy(dx: 2, dy: 1),
                 font: Self.numberFont, color: dim, align: .center)

            // A level for each return; one the track does not send to is off.
            for (index, bus) in model.arrangement.returns.enumerated() where index < header.sends {
                let level = row.sends[bus.id]?.value(at: now)
                text(bus.id, in: header.sendName(index), font: Self.smallFont, color: level == nil ? Theme.faintText.withAlphaComponent(alpha) : dim)
                bar(header.sendBar(index), filled: level.map { Fader.fraction($0, in: Fader.send) } ?? 0, dim: row.mute, alpha: alpha)
                text(level.map(Fader.text(db:)) ?? "off", in: header.sendText(index), font: Self.numberFont,
                     color: level == nil ? Theme.faintText.withAlphaComponent(alpha) : dim)
            }
        }

        // Each lane's parameter and range beside it, with the mark that
        // removes it, and under them the mark that adds one.
        guard let shown = header.lanes else { return }
        clipped(to: rect) {
            for (index, lane) in model.lanes(of: id).enumerated() where index < shown {
                let box = header.lane(index)
                fill(CGRect(x: 5, y: box.minY, width: box.width - 5, height: box.height - 1),
                     Theme.automationLane.withAlphaComponent(alpha))
                text(lane.label, in: header.laneName(index), font: Self.smallFont, color: Theme.text.withAlphaComponent(alpha))
                text("\(ValueScale.text(lane.min, unit: lane.unit)) to \(ValueScale.text(lane.max, unit: lane.unit))",
                     in: header.laneRange(index), font: Self.smallFont, color: Theme.faintText.withAlphaComponent(alpha))
                let mark = header.laneRemove(index)
                text("×", in: CGRect(x: mark.minX, y: mark.minY - 1, width: mark.width, height: 14),
                     font: Self.nameFont, color: dim, align: .center)
            }
            text("+ Lane", in: header.laneAdd, font: Self.smallFont, color: dim)
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
            fill(rounded: rect, radius: 2, Theme.loop.withAlphaComponent(active ? 0.95 : 0.25))
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
