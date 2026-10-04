import AAWCore
import AppKit
import Observation
import QuartzCore

/// A row of the arrangement: a track, a return or the master.
enum RowID: Hashable {
    case track(UInt64)
    case bus(UInt64)
    case master

    /// The row as the host's edits name it.
    var row: Row {
        switch self {
        case .track(let key): .track(key: key)
        case .bus(let key): .return(key: key)
        case .master: .master
        }
    }
}

/// The channel whose devices the detail panel shows.
struct DeviceChain {
    var row: RowID
    var name: String
    var effects: [EffectView]
    /// A track's pads.
    var pads: [PadView]
    /// The track, for a MIDI track's instrument.
    var track: TrackView? = nil
    /// The song's effect types, for adding one.
    static let kinds = browserEffects()
}

/// What the detail panel shows: a row's devices, or the clip last selected,
/// which is a pattern clip's pattern, an audio clip's settings or a note
/// clip's notes.
enum Detail {
    case devices, pattern
}

/// A clip on the timeline, a pattern clip, an audio clip or a note clip, with
/// the place of its track among the tracks and the beats it covers. An audio
/// clip's end is where its sound ends.
struct PlacedClip {
    var track: Int
    var key: UInt64
    var at: Double
    var end: Double
    var clip: ClipView?
    var audio: AudioClipView?
    var notes: NoteClipView?
}

/// The note clip the detail panel edits, with its track.
struct NoteContext: Equatable {
    var clip: NoteClipView
    var track: TrackView
}

/// What Copy took, for Paste: clips, as the host wrote them down, and the
/// beats they covered; or notes, with the clip they came from.
enum Clipboard {
    case clips(copied: String, span: Double)
    case notes([NoteCopy], from: UInt64)
}

/// The audio clip the detail panel shows, with its track and its file.
struct AudioContext: Equatable {
    var clip: AudioClipView
    var track: TrackView
    var file: FileView?
}

/// The pattern the detail panel edits: a clip's, with the track whose pads it
/// plays.
struct PatternContext: Equatable {
    var clip: ClipView
    var track: TrackView
    var pattern: PatternView
}

/// A song open in the app. It shows what the session host reports and sends
/// the person's edits and the transport's commands; the song and every rule
/// about it live in the host, which the agent's `daw` commands reach as well.
@MainActor
@Observable
public final class SongModel {
    /// The song file, which changes when the project is saved under a name.
    public private(set) var url: URL
    /// Whether the project has no name yet: it is in the app's data folder
    /// until Save As… gives it a name and a place.
    public private(set) var untitled: Bool
    /// The paths the project had before it was saved under another name, at
    /// which `daw` commands still reach it.
    private(set) var formerURLs: [URL] = []
    public private(set) var arrangement: Arrangement
    public private(set) var transport = TransportView(metronome: false, playing: false, cue: 0, loopRegion: nil)
    /// Recent changes, newest first.
    public private(set) var activity: [ChangeInfo] = []
    /// Why song.yaml does not load after an edit outside the host.
    public private(set) var invalid: String?
    public private(set) var warnings: [String] = []
    /// A command the host refused.
    public private(set) var refusal: String?
    /// What undo and redo would do, and whose change each is.
    public private(set) var undoStep: HistoryStep?
    public private(set) var redoStep: HistoryStep?
    /// The selected clips, or else the selected row or automation point.
    private(set) var selectedClips: Set<UInt64> = []
    private(set) var selectedRow: RowID?
    private(set) var selectedPoint: UInt64?
    /// The row whose devices the detail panel shows: the last one selected.
    private(set) var deviceRow: RowID?
    /// What the detail panel shows: devices after a row is selected, and a
    /// pattern after a clip is.
    var detail = Detail.devices
    /// The clip whose pattern the detail panel edits: the last one selected.
    private(set) var patternClip: UInt64?
    /// The selected event of that pattern.
    private(set) var selectedEvent: UInt64?
    /// The selected notes of the note clip the detail panel shows.
    private(set) var selectedNotes: Set<UInt64> = []
    /// The steps the piano roll draws, adds and moves notes on, in beats as
    /// the song writes them.
    var noteGrid = "1/4"
    /// Where in the note clip being edited notes are pasted: the beat of the
    /// clip last clicked in the clear in the piano roll.
    @ObservationIgnored var noteInsert: Double?
    /// What Copy took.
    @ObservationIgnored private(set) var clipboard: Clipboard?
    /// The track whose lane was last clicked, where clips are pasted.
    @ObservationIgnored var cueTrack: UInt64?
    /// True for a moment after each change by the agent.
    public private(set) var agentWorking = false
    /// True for a moment after the tempo, title or length changed.
    public private(set) var sessionChanged = false
    /// The host shut down, as after `daw close`.
    public private(set) var closed = false
    /// The position the transport bar shows.
    public var position: Double = 0
    public var showsActivity = true
    public var showsDetail = true
    public var showsBrowser = false
    /// The detail panel's height, which the person drags at its top edge:
    /// `leastDetailHeight` at the least, and kept between projects.
    public var detailHeight: CGFloat = max(SongModel.leastDetailHeight, CGFloat(UserDefaults.standard.double(forKey: "detail.height"))) {
        didSet {
            if detailHeight != oldValue { UserDefaults.standard.set(Double(detailHeight), forKey: "detail.height") }
        }
    }

    /// The detail panel's least height, and its height until it is dragged.
    public static let leastDetailHeight: CGFloat = 214
    /// The sample library, for the browser.
    let browser: Browser

    /// Called with each new revision, for the arrangement view to animate.
    @ObservationIgnored var onUpdate: ((Update) -> Void)?
    @ObservationIgnored var onTransport: (() -> Void)?
    @ObservationIgnored var onClosed: (() -> Void)?
    @ObservationIgnored var onZoom: ((Zoom) -> Void)?
    /// Called when the host refuses an edit, for the view to put back what it
    /// showed ahead of the host.
    @ObservationIgnored var onRefusal: (() -> Void)?
    @ObservationIgnored var onSelection: (() -> Void)?
    /// Asks the view to let the person type a row's name.
    @ObservationIgnored var onRename: ((RowID) -> Void)?
    /// Asks the view to unfold a row's automation lanes.
    @ObservationIgnored var onShowLanes: ((RowID) -> Void)?
    /// Gives the keys back to the arrangement, after a value was typed.
    @ObservationIgnored var onFocus: (() -> Void)?
    /// Called when the project was saved under another name, with the song
    /// file's path before.
    @ObservationIgnored var onMoved: ((URL) -> Void)?
    /// Called when waveforms arrive, for the arrangement to draw them.
    @ObservationIgnored var onWaveforms: (() -> Void)?
    /// Asks the arrangement to draw so many frames of scrolling and zooming
    /// and say how long each took.
    @ObservationIgnored var onMeasure: ((Int, @escaping @MainActor (DrawTimes) -> Void) -> Void)?

    @ObservationIgnored private let song: Song
    /// Commands go to the host in order, off the main thread: the host may be
    /// busy compiling the song.
    @ObservationIgnored private let commands = DispatchQueue(label: "aaw.commands")
    /// The last loop, to turn it back on.
    @ObservationIgnored private var lastLoop: LoopRegion?
    @ObservationIgnored private var agentTimer: Task<Void, Never>?
    @ObservationIgnored private var sessionTimer: Task<Void, Never>?
    @ObservationIgnored private var refusalTimer: Task<Void, Never>?
    /// A drag's latest edit, waiting for its turn to be sent.
    @ObservationIgnored private var dragNext: (edit: Edit, gesture: String)?
    @ObservationIgnored private var dragSending = false
    @ObservationIgnored private var dragSent: CFTimeInterval = 0
    @ObservationIgnored private var dragTimer: Task<Void, Never>?
    @ObservationIgnored private var gestures = 0
    @ObservationIgnored private var waveforms = WaveformStore()
    /// When the revision shown arrived, to time its waveforms from.
    @ObservationIgnored private var revisionShown = CACurrentMediaTime()
    /// How long the waveforms took, in milliseconds: after the song opened,
    /// and after the last change of some track's audio.
    @ObservationIgnored private(set) var waveformDelay: (open: Double?, change: Double?)
    /// Track colors, given out in the order tracks are first seen.
    @ObservationIgnored private var colors: [UInt64: Int] = [:]
    /// Samples are copied into the project off the main thread, and off the
    /// queue of commands: a copy takes a moment, and a fader must not wait.
    @ObservationIgnored private let imports = DispatchQueue(label: "aaw.imports")

    /// A drag sends the host at most this many edits a second.
    static let dragRate = 30.0

    static let activityLimit = 200

    /// Opens the song and becomes its host. Throws when the song does not
    /// load or another host has it open.
    public init(url: URL) throws {
        let relay = Relay()
        song = try Song.open(path: url.path, observer: relay)
        self.url = url
        untitled = song.isUntitled()
        browser = Browser(library: libraryPath(song: url.path))
        arrangement = song.arrangement()
        deviceRow = arrangement.tracks.first.map { .track($0.key) }
        relay.model = self
    }

    // MARK: What the host reports

    fileprivate func apply(_ update: Update) {
        arrangement = update.arrangement
        revisionShown = CACurrentMediaTime()
        undoStep = update.undo
        redoStep = update.redo
        // A drag is one entry, which follows it.
        if let gesture = update.change.gesture, activity.first?.gesture == gesture, activity.first?.origin == update.change.origin {
            activity[0] = update.change
        } else {
            activity.insert(update.change, at: 0)
        }
        dropMissingSelection()
        if activity.count > Self.activityLimit {
            activity.removeLast(activity.count - Self.activityLimit)
        }
        if update.change.origin == .agent {
            agentWorking = true
            agentTimer?.cancel()
            agentTimer = after(1.6) { $0.agentWorking = false }
        }
        if update.touched.contains(where: { $0.part == .session }) {
            sessionChanged = true
            sessionTimer?.cancel()
            sessionTimer = after(1.6) { $0.sessionChanged = false }
        }
        onUpdate?(update)
    }

    fileprivate func apply(_ transport: TransportView) {
        self.transport = transport
        if let region = transport.loopRegion { lastLoop = region }
        if !transport.playing { position = transport.cue }
        onTransport?()
    }

    fileprivate func setInvalid(_ error: String?) {
        invalid = error
    }

    /// A file's URL as the app compares them: without `..` or links.
    nonisolated static func normal(_ url: URL) -> URL {
        url.standardizedFileURL.resolvingSymlinksInPath()
    }

    /// The project is at another path: the person saved it under a name, or
    /// `daw move` or `daw copy` did.
    fileprivate func moved(to path: String) {
        let (old, new) = (url, Self.normal(URL(fileURLWithPath: path)))
        guard new != old else { return }
        formerURLs.append(old)
        url = new
        untitled = song.isUntitled()
        onMoved?(old)
    }

    fileprivate func warn(_ message: String) {
        if !warnings.contains(message) { warnings.append(message) }
    }

    fileprivate func hostClosed() {
        closed = true
        onClosed?()
    }

    fileprivate func apply(_ update: Waveforms) {
        waveforms.apply(update)
        // The last of a revision's peaks: how long they took from its change.
        if !update.peaks.isEmpty, update.revision == arrangement.revision, waveforms.complete {
            let delay = (CACurrentMediaTime() - revisionShown) * 1000
            if update.revision == 0 { waveformDelay.open = delay } else { waveformDelay.change = delay }
        }
        onWaveforms?()
    }

    /// What a track's clips play at the revision shown, once the host has
    /// worked it out; until then, nil.
    func waveform(of track: UInt64) -> Waveform? {
        waveforms.waveform(of: track, at: arrangement.revision)
    }

    /// The waveform of a file an audio clip plays, once the host has sent it.
    func fileWaveform(_ identity: UInt64) -> Waveform? {
        waveforms.file(identity)
    }

    /// A file an audio clip plays: its length and its beat map.
    func file(_ identity: UInt64) -> FileView? {
        identity == 0 ? nil : arrangement.files.first { $0.identity == identity }
    }

    /// A track's color: the next of the palette for each track first seen.
    /// The song format has no color.
    func color(of track: UInt64) -> NSColor {
        let index = colors[track] ?? colors.count
        colors[track] = index
        return Theme.palette[index % Theme.palette.count]
    }

    private func after(_ seconds: Double, _ body: @escaping @MainActor (SongModel) -> Void) -> Task<Void, Never> {
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(seconds))
            if !Task.isCancelled, let self { body(self) }
        }
    }

    func setTempo(_ text: String) {
        guard let bpm = Double(text.trimmingCharacters(in: .whitespacesAndNewlines)),
              bpm.isFinite, (20...400).contains(bpm) else {
            refuse("Enter a tempo from 20 to 400 BPM.")
            return
        }
        if bpm != arrangement.tempo { edit(.tempo(bpm: bpm)) }
    }

    // MARK: Editing

    /// Makes an edit. `then` is called with the keys of what the edit made,
    /// once the arrangement shows it.
    func edit(_ edit: Edit, then: (@MainActor ([UInt64]) -> Void)? = nil) {
        flushDrag()
        commands.async { [song, weak self] in
            let result = Result { try song.edit(edit: edit, gesture: nil) }
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    switch result {
                    case .success(let made): then?(made)
                    case .failure(let error): self?.refuse(Self.reason(error))
                    }
                }
            }
        }
    }

    /// A name for a drag, for the edits it sends.
    func newGesture() -> String {
        gestures += 1
        return "app-\(ProcessInfo.processInfo.processIdentifier)-\(gestures)"
    }

    /// Sends a drag's edit: the latest one, a limited number of times a second
    /// and one at a time, so a slow host is never sent a backlog.
    func drag(_ edit: Edit, gesture: String) {
        dragNext = (edit, gesture)
        sendDrag()
    }

    /// Sends a drag's last edit now, as it ends.
    func endDrag() {
        dragTimer?.cancel()
        dragTimer = nil
        flushDrag()
    }

    private func flushDrag() {
        guard let next = dragNext else { return }
        dragNext = nil
        send { _ = try $0.edit(edit: next.edit, gesture: next.gesture) }
    }

    private func sendDrag() {
        guard !dragSending, let next = dragNext else { return }
        let wait = dragSent + 1 / Self.dragRate - CACurrentMediaTime()
        if wait > 0 {
            if dragTimer == nil {
                dragTimer = after(wait) {
                    $0.dragTimer = nil
                    $0.sendDrag()
                }
            }
            return
        }
        dragNext = nil
        dragSending = true
        dragSent = CACurrentMediaTime()
        commands.async { [song, weak self] in
            let result = Result { try song.edit(edit: next.edit, gesture: next.gesture) }
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self else { return }
                    self.dragSending = false
                    if case .failure(let error) = result { self.refuse(Self.reason(error)) }
                    self.sendDrag()
                }
            }
        }
    }

    public func undo() {
        send { try $0.undo() }
    }

    public func redo() {
        send { try $0.redo() }
    }

    // MARK: Selection

    /// Each clip, pattern clips and audio clips alike, with the place of its
    /// track among the tracks.
    func placedClips() -> [PlacedClip] {
        arrangement.tracks.enumerated().flatMap { index, track in
            track.clips.map {
                PlacedClip(track: index, key: $0.key, at: $0.at, end: $0.at + $0.patternBeats * Double($0.repeats), clip: $0)
            } + track.audio.map {
                PlacedClip(track: index, key: $0.key, at: $0.at, end: $0.at + $0.lengthBeats + $0.tailBeats, audio: $0)
            } + track.noteClips.map {
                PlacedClip(track: index, key: $0.key, at: $0.at, end: $0.at + $0.lengthBeats, notes: $0)
            }
        }
    }

    /// The beats the selected clips cover together.
    var selectedSpan: (start: Double, end: Double)? {
        let clips = placedClips().filter { selectedClips.contains($0.key) }
        guard let start = clips.map(\.at).min() else { return nil }
        return (start, clips.map(\.end).max() ?? start)
    }

    /// Selects clips. The detail panel shows `focus`, the clip that was
    /// clicked, or the one clip selected: a pattern clip's pattern, an audio
    /// clip's settings or a note clip's notes.
    func select(clips: Set<UInt64>, focus: UInt64? = nil) {
        setSelection(clips: clips, row: nil)
        if let clip = focus ?? (clips.count == 1 ? clips.first : nil), clips.contains(clip) {
            if patternClip != clip {
                patternClip = clip
                select(event: nil)
                select(notes: [])
            }
            detail = .pattern
        }
    }

    /// Selects notes of the note clip the detail panel shows.
    func select(notes: Set<UInt64>) {
        guard notes != selectedNotes else { return }
        selectedNotes = notes
        if !notes.isEmpty, selectedPoint != nil {
            selectedPoint = nil
            onSelection?()
        }
        sendSelection()
    }

    /// Selects an event of the pattern the detail panel shows.
    func select(event: UInt64?) {
        guard event != selectedEvent else { return }
        selectedEvent = event
        if event != nil, selectedPoint != nil {
            selectedPoint = nil
            onSelection?()
        }
        sendSelection()
    }

    /// The pattern the detail panel edits, with its clip and the clip's track.
    var patternContext: PatternContext? {
        guard let key = patternClip else { return nil }
        for track in arrangement.tracks {
            if let clip = track.clips.first(where: { $0.key == key }),
               let pattern = arrangement.patterns.first(where: { $0.name == clip.pattern }) {
                return PatternContext(clip: clip, track: track, pattern: pattern)
            }
        }
        return nil
    }

    /// The note clip the detail panel edits, with its track.
    var noteContext: NoteContext? {
        guard let key = patternClip else { return nil }
        for track in arrangement.tracks {
            if let clip = track.noteClips.first(where: { $0.key == key }) {
                return NoteContext(clip: clip, track: track)
            }
        }
        return nil
    }

    /// The selected notes, in the note clip the detail panel shows.
    var selectedNoteViews: [NoteView] {
        guard !selectedNotes.isEmpty else { return [] }
        return noteContext?.clip.notes.filter { selectedNotes.contains($0.key) } ?? []
    }

    /// The audio clip the detail panel shows, with its track and its file.
    var audioContext: AudioContext? {
        guard let key = patternClip else { return nil }
        for track in arrangement.tracks {
            if let clip = track.audio.first(where: { $0.key == key }) {
                return AudioContext(clip: clip, track: track, file: file(clip.file))
            }
        }
        return nil
    }

    /// The selected event, in the pattern the detail panel shows.
    var selectedEventView: EventView? {
        guard let key = selectedEvent else { return nil }
        return patternContext?.pattern.events.first { $0.key == key }
    }

    func select(row: RowID?) {
        setSelection(clips: [], row: row)
    }

    /// Selects an automation point, in place of clips, rows and events.
    func select(point: UInt64?) {
        if point != nil {
            setSelection(clips: [], row: nil)
            select(event: nil)
        }
        guard point != selectedPoint else { return }
        selectedPoint = point
        onSelection?()
    }

    /// Shows a row's devices without selecting it.
    func showDevices(of row: RowID) {
        deviceRow = row
    }

    /// The devices of the row the detail panel shows.
    var deviceChain: DeviceChain? {
        switch deviceRow {
        case .track(let key):
            return arrangement.tracks.first { $0.key == key }
                .map { DeviceChain(row: .track(key), name: $0.id, effects: $0.effects, pads: $0.pads, track: $0) }
        case .bus(let key):
            return arrangement.returns.first { $0.key == key }
                .map { DeviceChain(row: .bus(key), name: $0.id, effects: $0.effects, pads: []) }
        case .master:
            return DeviceChain(row: .master, name: "Master", effects: arrangement.master.effects, pads: [])
        case nil:
            return nil
        }
    }

    /// A row's automation lanes.
    func lanes(of row: RowID) -> [LaneView] {
        switch row {
        case .track(let key): arrangement.tracks.first { $0.key == key }?.lanes ?? []
        case .bus(let key): arrangement.returns.first { $0.key == key }?.lanes ?? []
        case .master: arrangement.master.lanes
        }
    }

    /// The parameters of a row that could have a lane and have none.
    func laneTargets(of row: RowID) -> [LaneTarget] {
        switch row {
        case .track(let key): arrangement.tracks.first { $0.key == key }?.laneTargets ?? []
        case .bus(let key): arrangement.returns.first { $0.key == key }?.laneTargets ?? []
        case .master: arrangement.master.laneTargets
        }
    }

    public func selectAllClips() {
        select(clips: Set(placedClips().map(\.key)))
    }

    private func setSelection(clips: Set<UInt64>, row: RowID?) {
        guard clips != selectedClips || row != selectedRow else { return }
        selectedClips = clips
        selectedRow = row
        selectedEvent = nil
        selectedNotes = []
        if !clips.isEmpty || row != nil { selectedPoint = nil }
        // The detail panel follows the selection, and stays on the last row.
        if let row {
            deviceRow = row
            detail = .devices
        } else if let track = placedClips().first(where: { clips.contains($0.key) })?.track {
            deviceRow = .track(arrangement.tracks[track].key)
        }
        onSelection?()
        sendSelection()
    }

    /// The host keeps the selection for `daw status`, so that an agent can be
    /// asked about "the selected clip" or "the selected note".
    private func sendSelection() {
        var keys = Array(selectedClips).sorted()
        switch selectedRow {
        case .track(let key), .bus(let key): keys.append(key)
        case .master, nil: break
        }
        if let selectedEvent { keys.append(selectedEvent) }
        keys += selectedNotes.sorted()
        commands.async { [song, keys] in try? song.select(keys: keys) }
    }

    /// Forgets selected objects that a change removed.
    private func dropMissingSelection() {
        let clips = selectedClips.intersection(placedClips().map(\.key))
        var row = selectedRow
        switch row {
        case .track(let key): if !arrangement.tracks.contains(where: { $0.key == key }) { row = nil }
        case .bus(let key): if !arrangement.returns.contains(where: { $0.key == key }) { row = nil }
        case .master, nil: break
        }
        var point = selectedPoint
        if let key = point {
            let lanes = arrangement.tracks.flatMap(\.lanes) + arrangement.returns.flatMap(\.lanes) + arrangement.master.lanes
            if !lanes.contains(where: { $0.points.contains { $0.key == key } }) { point = nil }
        }
        if clips != selectedClips || row != selectedRow || point != selectedPoint {
            selectedClips = clips
            selectedRow = row
            selectedPoint = point
            onSelection?()
        }
        switch deviceRow {
        case .track(let key): if !arrangement.tracks.contains(where: { $0.key == key }) { deviceRow = nil }
        case .bus(let key): if !arrangement.returns.contains(where: { $0.key == key }) { deviceRow = nil }
        case .master, nil: break
        }
        if deviceRow == nil { deviceRow = arrangement.tracks.first.map { .track($0.key) } }
        if patternClip != nil, patternContext == nil, audioContext == nil, noteContext == nil { patternClip = nil }
        if selectedEvent != nil, selectedEventView == nil { selectedEvent = nil }
        if !selectedNotes.isEmpty {
            let notes = selectedNotes.intersection(noteContext?.clip.notes.map(\.key) ?? [])
            if notes != selectedNotes { selectedNotes = notes }
        }
    }

    public var canDelete: Bool {
        selectedEvent != nil || !selectedNotes.isEmpty || !selectedClips.isEmpty || selectedPoint != nil
            || (selectedRow != nil && selectedRow != .master)
    }

    /// Whether the selected notes are what Delete, Duplicate and Copy act on:
    /// the piano roll shows them.
    var notesInHand: Bool {
        !selectedNotes.isEmpty && detail == .pattern && showsDetail
    }

    /// Removes the selected event or notes of the clip being edited, or else
    /// the selected clips, automation point, track or return.
    public func deleteSelection() {
        if let event = selectedEvent, detail == .pattern, showsDetail {
            edit(.eventRemove(event: event))
        } else if notesInHand {
            edit(.notesRemove(notes: selectedNotes.sorted()))
        } else if !selectedClips.isEmpty {
            edit(.clipsRemove(clips: selectedClips.sorted()))
        } else if let point = selectedPoint {
            edit(.pointRemove(point: point))
        } else if let row = selectedRow, row != .master {
            edit(.remove(row: row.row))
        }
    }

    public var canDuplicate: Bool {
        notesInHand || !selectedClips.isEmpty
    }

    /// Copies the selected notes, or else clips, to right after them and
    /// selects the copies.
    public func duplicateSelection() {
        if notesInHand {
            edit(.notesDuplicate(notes: selectedNotes.sorted())) { [weak self] made in
                self?.select(notes: Set(made))
            }
            return
        }
        guard !selectedClips.isEmpty else { return }
        edit(.clipsDuplicate(clips: selectedClips.sorted())) { [weak self] made in
            self?.select(clips: Set(made))
        }
    }

    /// Takes what is selected for Paste, as it is now: the selected notes
    /// when `notes`, else the selected clips. Paste works after they change
    /// or are gone.
    public func copySelection(notes: Bool) {
        if notes {
            let chosen = selectedNoteViews
            guard let from = noteContext?.clip.key, !chosen.isEmpty else { return }
            clipboard = .notes(chosen.map { NoteCopy(pitch: $0.pitch, at: $0.atText, duration: $0.durationText, velocity: $0.velocity) }, from: from)
        } else if let span = selectedSpan {
            do {
                clipboard = .clips(copied: try song.copyClips(clips: selectedClips.sorted()), span: span.end - span.start)
            } catch {
                refuse(Self.reason(error))
            }
        }
    }

    /// Copies what is selected, then removes it.
    public func cutSelection(notes: Bool) {
        copySelection(notes: notes)
        if notes ? notesInHand : !selectedClips.isEmpty { deleteSelection() }
    }

    /// Pastes copied notes into the note clip being edited, at the beat of
    /// the clip last clicked in the clear, or else where they were in another
    /// clip and right after themselves in their own; or copied clips at the
    /// start position, on the track whose lane was last clicked, or the
    /// selected track, or else the tracks they came from. Selects the copies,
    /// and moves the place pasted at to their end, so that the next paste
    /// follows them.
    public func paste(intoNotes: Bool) {
        switch clipboard {
        case .notes(let notes, let from) where intoNotes:
            guard let clip = noteContext?.clip.key else { return }
            let first = notes.compactMap { PianoRollLayout.beats($0.at) }.min() ?? 0
            let end = notes.compactMap { n in PianoRollLayout.beats(n.at).flatMap { at in PianoRollLayout.beats(n.duration).map { at + $0 } } }.max() ?? first
            let at = noteInsert ?? (clip == from ? nil : first)
            edit(.notesPaste(notes: notes, clip: clip, at: at)) { [weak self] made in
                self?.select(notes: Set(made))
            }
            if let at { noteInsert = at + end - first }
        case .clips(let copied, let span) where !intoNotes:
            var track = cueTrack
            if case .track(let key) = selectedRow { track = key }
            let at = transport.cue
            edit(.clipsPaste(copied: copied, at: at, track: track)) { [weak self] made in
                self?.select(clips: Set(made))
            }
            locate(at + span)
        default:
            NSSound.beep()
        }
    }

    /// Whether Paste has something to paste where the keys are.
    func canPaste(intoNotes: Bool) -> Bool {
        switch clipboard {
        case .notes?: intoNotes && noteContext != nil
        case .clips?: !intoNotes
        case nil: false
        }
    }

    /// The audio clips a split is of: the selected ones, or with a track
    /// selected and no clip, that track's.
    private var splitTargets: [AudioClipView] {
        if !selectedClips.isEmpty {
            return arrangement.tracks.flatMap(\.audio).filter { selectedClips.contains($0.key) }
        }
        if case .track(let key) = selectedRow {
            return arrangement.tracks.first { $0.key == key }?.audio ?? []
        }
        return []
    }

    /// Whether the start position is inside an audio clip a split is of.
    public var canSplit: Bool {
        let cue = transport.cue
        return splitTargets.contains { $0.at < cue && cue < $0.at + $0.lengthBeats }
    }

    /// Splits those audio clips at the start position, and selects the later
    /// halves, so that Delete takes what follows the split.
    public func splitSelection() {
        let clips = splitTargets.map(\.key).sorted()
        guard !clips.isEmpty else { return }
        edit(.audioSplit(clips: clips, at: transport.cue)) { [weak self] made in
            if let focus = made.first { self?.select(clips: Set(made), focus: focus) }
        }
    }

    /// Adds a track under the selected one, or else last, and asks for its
    /// name: a MIDI track of note clips, with no instrument, when `midi`.
    public func addTrack(midi: Bool = false) {
        var index = arrangement.tracks.count
        if case .track(let key) = selectedRow, let at = arrangement.tracks.firstIndex(where: { $0.key == key }) {
            index = at + 1
        }
        add(.trackAdd(index: UInt32(index), midi: midi)) { .track($0) }
    }

    /// Whether a device from the browser can go on a row: an effect on any
    /// row, an instrument on a MIDI track or, with no row, on a new track.
    func canAddBrowserDevice(_ kind: String, to row: RowID?) -> Bool {
        if !Browser.instruments.contains(kind) { return DeviceChain.kinds.contains(kind) && row != nil }
        guard let row else { return true }
        guard case .track(let key) = row else { return false }
        return arrangement.tracks.first { $0.key == key }?.midi == true
    }

    /// Adds a device from the browser: an effect at `index` of the row's
    /// chain or its end; an empty Sampler, or a Synth with the plain saw or
    /// the patch named, as the MIDI track's instrument or on a new track.
    func addBrowserDevice(_ kind: String, to row: RowID?, index: UInt32? = nil, patch: String? = nil) {
        guard canAddBrowserDevice(kind, to: row) else { return }
        if Browser.instruments.contains(kind) {
            let track: UInt64?
            if case .track(let key) = row { track = key } else { track = nil }
            let edit: Edit = kind == "sampler" ? .instrumentAdd(track: track) : .synthAdd(track: track, patch: patch)
            self.edit(edit) { [weak self] made in
                guard let self else { return }
                if let key = track ?? made.first { self.select(row: .track(key)) }
                self.detail = .devices
            }
        } else if let row {
            edit(.effectAdd(row: row.row, kind: kind, index: index))
            select(row: row)
            detail = .devices
        }
    }

    public func addReturn() {
        add(.returnAdd(index: UInt32(arrangement.returns.count))) { .bus($0) }
    }

    private func add(_ edit: Edit, row: @escaping @MainActor (UInt64) -> RowID) {
        self.edit(edit) { [weak self] made in
            guard let self, let key = made.first else { return }
            self.select(row: row(key))
            self.onRename?(row(key))
        }
    }

    /// Adds a clip of a new, empty pattern to a track and selects it, which
    /// shows the pattern to fill in; on a MIDI track, an empty note clip,
    /// which shows the piano roll.
    func addClip(to track: UInt64, at beat: Double) {
        edit(.clipNew(track: track, at: beat)) { [weak self] made in
            if let clip = made.first { self?.select(clips: [clip], focus: clip) }
        }
    }

    /// Adds a sample file to the song: as a pad of `track`, or with no track
    /// as a new track after the others. The file is copied into the project
    /// first; the original stays as it is. `name` is what the pad and a new
    /// track are named after, and `note` the sample's pitch, if it has one.
    func addSample(path: String, name: String, note: String?, to track: UInt64?) {
        let index = UInt32(arrangement.tracks.count)
        copy(path, note: note) { model, asset in
            model.edit(.sampleAdd(asset: asset, name: name, track: track, index: index)) { [weak model] made in
                // The row's pads show in the detail panel.
                if let key = track ?? made.first { model?.select(row: .track(key)) }
            }
        }
    }

    /// Loads a sample file into the Sampler on a MIDI track, or gives a
    /// track with no instrument a Sampler of it: the keys then play it at
    /// every note's pitch, as it is at middle C. The file is copied into the
    /// project first, and the pad is named after `name`.
    func loadSampler(path: String, name: String, into track: UInt64) {
        copy(path, note: nil) { model, asset in
            model.edit(.samplerLoad(track: track, asset: asset, name: name)) { [weak model] _ in
                model?.select(row: .track(track))
                model?.detail = .devices
            }
        }
    }

    /// Lands a file dropped on the detail panel: into the Sampler it shows,
    /// or a MIDI track with no instrument. False when the panel shows no
    /// such track, or the file is not audio.
    @discardableResult
    func dropOnDevices(file url: URL) -> Bool {
        guard detail == .devices, let track = deviceChain?.track, track.midi, track.sampler != nil || track.instrument == nil,
              Browser.extensions.contains(url.pathExtension.lowercased()) else { return false }
        loadSampler(path: url.path, name: url.deletingPathExtension().lastPathComponent, into: track.key)
        return true
    }

    /// Saves the Synth on a MIDI track to the library as a patch, as `daw
    /// patch save` does, and names the song's patch after it; over a patch
    /// already saved under the name only with `replace`. The browser's list
    /// and the panel's header then show it.
    func savePatch(track: UInt64, name: String, description: String?, tags: [String], replace: Bool) {
        edit(.patchSave(track: track, name: name, description: description, tags: tags, replace: replace)) { [weak self] _ in
            self?.browser.refreshPatches()
        }
    }

    /// Plays a note now through the Synth on a MIDI track and the track's
    /// chain, as the panel's keys do: outside the timeline and the undo
    /// history. `velocity` is 1 to 127.
    func previewNote(track: UInt64, pitch: Int, velocity: Int, lengthBeats: Double) {
        send { try $0.previewNote(track: track, pitch: Int32(pitch), velocity: UInt32(velocity), lengthBeats: lengthBeats) }
    }

    /// Sets the root note of the Sampler's sample to the pitch `daw samples
    /// analyze` measures from its file, off the main thread; a file with no
    /// one pitch is refused with the reason. `done` is called either way.
    func measureSamplerRoot(track: UInt64, done: @escaping @MainActor () -> Void) {
        guard let sampler = arrangement.tracks.first(where: { $0.key == track })?.sampler, let sample = sampler.sample else { return done() }
        guard let library = browser.library else {
            refuse("The library index was not found, so the pitch cannot be measured")
            return done()
        }
        let file = url.deletingLastPathComponent().appendingPathComponent(sampler.path).path
        imports.async { [weak self] in
            let measured = Result { try libraryPitch(db: library, path: file) }
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    defer { done() }
                    guard let self else { return }
                    switch measured {
                    case .failure(let error): self.refuse(Self.reason(error))
                    case .success(nil): self.refuse("No one pitch was measured in \(sample): it plays as it is at middle C")
                    case .success(let note?): self.edit(.samplerRoot(track: track, note: note))
                    }
                }
            }
        }
    }

    /// Adds a sample file to the song as an audio clip at a beat: on `track`,
    /// or with no track on a new one after the others, named after `name`.
    /// The file is copied into the project first, and the song grows to hold
    /// a clip that ends past its end.
    func addClip(path: String, name: String, note: String?, to track: UInt64?, at beat: Double) {
        let index = UInt32(arrangement.tracks.count)
        copy(path, note: note) { model, asset in
            model.edit(.sampleClip(asset: asset, name: name, track: track, index: index, at: beat)) { [weak model] made in
                // The clip is the last of what the edit made, after a new track.
                if let clip = made.last { model?.select(clips: [clip], focus: clip) }
            }
        }
    }

    /// Makes a note clip of a MIDI file's notes at a beat: on the MIDI track
    /// `track`, or with no track on a new MIDI track after the others, named
    /// after the file. The file is read where it is, and the song grows to
    /// hold the clip.
    func importMIDI(path: String, to track: UInt64?, at beat: Double) {
        let index = UInt32(arrangement.tracks.count)
        edit(.midiClip(path: path, track: track, index: index, at: beat)) { [weak self] made in
            // The clip is the last of what the edit made, after a new track.
            if let clip = made.last { self?.select(clips: [clip], focus: clip) }
        }
    }

    /// The note clip File › Export MIDI Clip… writes, and its track: the
    /// clip selected, when it is the only one.
    public var exportableClip: (track: TrackView, clip: NoteClipView)? {
        guard selectedClips.count == 1, let key = selectedClips.first else { return nil }
        for track in arrangement.tracks {
            if let clip = track.noteClips.first(where: { $0.key == key }) { return (track, clip) }
        }
        return nil
    }

    /// Writes the notes of a note clip that play as a MIDI file.
    public func exportMIDI(clip: UInt64, to url: URL) throws {
        _ = try song.exportMidiClip(clip: clip, path: url.path)
    }

    /// Copies a sample file into the project, off the main thread, and hands
    /// the copy on; the original stays as it is.
    private func copy(_ path: String, note: String?, then: @escaping @MainActor (SongModel, Asset) -> Void) {
        let song = url.path
        imports.async { [weak self] in
            let copied = Result { try libraryImport(song: song, source: path, rootNote: note) }
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self else { return }
                    switch copied {
                    case .failure(let error): self.refuse(Self.reason(error))
                    case .success(let asset): then(self, asset)
                    }
                }
            }
        }
    }

    public var canRename: Bool {
        selectedRow != nil && selectedRow != .master
    }

    public func renameSelection() {
        if let row = selectedRow, row != .master { onRename?(row) }
    }

    // MARK: Transport

    /// Where the song is playing, read from the audio thread.
    func playhead() -> Playhead? {
        song.playhead()
    }

    public func toggleMetronome() {
        let enabled = !transport.metronome
        send { try $0.setMetronome(enabled: enabled) }
    }

    public func togglePlay() {
        if transport.playing { stop() } else { play() }
    }

    public func play(from beat: Double? = nil) {
        send { try $0.play(from: beat) }
    }

    public func stop() {
        send { try $0.stop() }
    }

    /// Sets the start position; while playing, playback jumps there.
    public func locate(_ beat: Double) {
        send { try $0.locate(beat: beat) }
    }

    /// Jumps back to the start position while playing.
    public func returnToStart() {
        locate(transport.cue)
    }

    public func setLoop(start: Double, length: Double) {
        send { try $0.setLoop(start: start, length: length) }
    }

    /// Loops the selected clips. With none selected, turns the loop off, or
    /// back on over its last region; with no earlier loop, it covers the
    /// section at the start position, else that bar.
    public func toggleLoop() {
        if let span = selectedSpan, span.end > span.start {
            let region = LoopRegion(start: span.start, length: span.end - span.start)
            if transport.loopRegion == region {
                send { try $0.clearLoop() }
            } else {
                setLoop(start: region.start, length: region.length)
            }
            return
        }
        if transport.loopRegion != nil {
            send { try $0.clearLoop() }
            return
        }
        let region = lastLoop.flatMap(inSong) ?? defaultLoop()
        setLoop(start: region.start, length: region.length)
    }

    /// The loop the brace shows while the loop is off.
    var idleLoop: LoopRegion? {
        transport.loopRegion == nil ? lastLoop.flatMap(inSong) : nil
    }

    private func inSong(_ region: LoopRegion) -> LoopRegion? {
        region.start + region.length <= arrangement.lengthBeats ? region : nil
    }

    private func defaultLoop() -> LoopRegion {
        let cue = transport.cue
        if let s = arrangement.sections.first(where: { $0.at <= cue && cue < $0.at + $0.lengthBeats }),
           s.at + s.lengthBeats <= arrangement.lengthBeats {
            return LoopRegion(start: s.at, length: s.lengthBeats)
        }
        let bar = Double(arrangement.beatsPerBar)
        let start = (cue / bar).rounded(.down) * bar
        return LoopRegion(start: start, length: min(bar, arrangement.lengthBeats - start))
    }

    private func send(_ command: @escaping @Sendable (Song) throws -> Void) {
        commands.async { [song, weak self] in
            do {
                try command(song)
            } catch {
                let message = Self.reason(error)
                DispatchQueue.main.async {
                    MainActor.assumeIsolated { self?.refuse(message) }
                }
            }
        }
    }

    /// Why the host refused a command, in its words.
    nonisolated static func reason(_ error: Error) -> String {
        if case SongError.Failed(let message) = error { return message }
        return error.localizedDescription
    }

    private func refuse(_ message: String) {
        refusal = message
        refusalTimer?.cancel()
        refusalTimer = after(6) { $0.refusal = nil }
        onRefusal?()
    }

    func zoom(_ zoom: Zoom) {
        onZoom?(zoom)
    }

    /// Draws `frames` frames of scrolling and zooming and reports how long
    /// each took to draw.
    func measure(frames: Int, then: @escaping @MainActor (DrawTimes) -> Void) {
        onMeasure?(frames, then)
    }

    // MARK: The project

    /// Whether the project holds nothing: a blank song and no other file.
    /// Closing such an Untitled project deletes it without a question.
    var untouched: Bool {
        song.untouched()
    }

    /// Saves the project as `folder`, where nothing is yet, and names the
    /// song after it. An Untitled project moves there; a project that has a
    /// name is copied and stays as it was, and this window carries on in the
    /// copy. The history and what is playing carry on either way. Waits for
    /// the host, which reports the new path as it reports a change.
    func saveAs(_ folder: URL) throws {
        endDrag()
        let path = try commands.sync { try song.saveAs(folder: folder.path) }
        // Known here at once; the host's own report of it follows.
        moved(to: path)
    }

    /// Stops answering `daw` commands for a path the project was saved from,
    /// so that the project there can be opened.
    func release(_ former: URL) {
        commands.sync { try? song.release(path: former.path) }
        formerURLs.removeAll { $0 == former }
    }

    /// Tells the host whether this project's window is the one in front,
    /// which `daw projects` reports.
    func setFront(_ front: Bool) {
        commands.async { [song] in try? song.setFront(front: front) }
    }

    /// Saves and stops hosting the song; `daw` commands then run headless.
    public func close() {
        endDrag()
        // After queued commands, so none is sent to a closed host.
        commands.sync { song.close() }
    }
}

/// Carries the host's reports from its thread to the model on the main thread,
/// in the order they were made.
private final class Relay: SongObserver, @unchecked Sendable {
    @MainActor weak var model: SongModel?

    private func onMain(_ body: @escaping @MainActor (SongModel) -> Void) {
        DispatchQueue.main.async {
            MainActor.assumeIsolated {
                if let model = self.model { body(model) }
            }
        }
    }

    func changed(update: Update) {
        onMain { $0.apply(update) }
    }

    func transport(transport: TransportView) {
        onMain { $0.apply(transport) }
    }

    func invalid(error: String?) {
        onMain { $0.setInvalid(error) }
    }

    func moved(path: String) {
        onMain { $0.moved(to: path) }
    }

    func warning(message: String) {
        onMain { $0.warn(message) }
    }

    func waveforms(waveforms: Waveforms) {
        onMain { $0.apply(waveforms) }
    }

    func closed() {
        onMain { $0.hostClosed() }
    }
}
