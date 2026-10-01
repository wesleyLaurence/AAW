import AAWCore
import Foundation
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
    /// The song's effect types, for adding one.
    static let kinds = ["filter", "eq", "compressor", "limiter", "delay", "reverb"]
}

/// A song open in the app. It shows what the session host reports and sends
/// the person's edits and the transport's commands; the song and every rule
/// about it live in the host, which the agent's `daw` commands reach as well.
@MainActor
@Observable
public final class SongModel {
    public let url: URL
    public private(set) var arrangement: Arrangement
    public private(set) var transport = TransportView(playing: false, cue: 0, loopRegion: nil)
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
    /// True for a moment after each change by the agent.
    public private(set) var agentWorking = false
    /// True for a moment after the tempo, title or length changed.
    public private(set) var sessionChanged = false
    /// The host shut down, as after `daw close`.
    public private(set) var closed = false
    /// The position the transport bar shows.
    public var position: Double = 0
    public var showsActivity = true
    public var showsDevices = true

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

    /// A drag sends the host at most this many edits a second.
    static let dragRate = 30.0

    static let activityLimit = 200

    /// Opens the song and becomes its host. Throws when the song does not
    /// load or another host has it open.
    public init(url: URL) throws {
        let relay = Relay()
        song = try Song.open(path: url.path, observer: relay)
        self.url = url
        arrangement = song.arrangement()
        deviceRow = arrangement.tracks.first.map { .track($0.key) }
        relay.model = self
    }

    // MARK: What the host reports

    fileprivate func apply(_ update: Update) {
        arrangement = update.arrangement
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

    fileprivate func warn(_ message: String) {
        if !warnings.contains(message) { warnings.append(message) }
    }

    fileprivate func hostClosed() {
        closed = true
        onClosed?()
    }

    private func after(_ seconds: Double, _ body: @escaping @MainActor (SongModel) -> Void) -> Task<Void, Never> {
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(seconds))
            if !Task.isCancelled, let self { body(self) }
        }
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

    /// Each clip with the place of its track among the tracks.
    func placedClips() -> [(track: Int, clip: ClipView)] {
        arrangement.tracks.enumerated().flatMap { index, track in track.clips.map { (index, $0) } }
    }

    /// The beats the selected clips cover together.
    var selectedSpan: (start: Double, end: Double)? {
        let clips = placedClips().map(\.clip).filter { selectedClips.contains($0.key) }
        guard let start = clips.map(\.at).min() else { return nil }
        return (start, clips.map { $0.at + $0.patternBeats * Double($0.repeats) }.max() ?? start)
    }

    func select(clips: Set<UInt64>) {
        setSelection(clips: clips, row: nil)
    }

    func select(row: RowID?) {
        setSelection(clips: [], row: row)
    }

    /// Selects an automation point, in place of clips and rows.
    func select(point: UInt64?) {
        if point != nil { setSelection(clips: [], row: nil) }
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
                .map { DeviceChain(row: .track(key), name: $0.id, effects: $0.effects, pads: $0.pads) }
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
        select(clips: Set(placedClips().map(\.clip.key)))
    }

    private func setSelection(clips: Set<UInt64>, row: RowID?) {
        guard clips != selectedClips || row != selectedRow else { return }
        selectedClips = clips
        selectedRow = row
        if !clips.isEmpty || row != nil { selectedPoint = nil }
        // The detail panel follows the selection, and stays on the last row.
        if let row {
            deviceRow = row
        } else if let track = placedClips().first(where: { clips.contains($0.clip.key) })?.track {
            deviceRow = .track(arrangement.tracks[track].key)
        }
        onSelection?()
        // The host keeps the selection for `daw status`, so that an agent can
        // be asked about "the selected clip".
        var keys = Array(clips).sorted()
        switch row {
        case .track(let key), .bus(let key): keys.append(key)
        case .master, nil: break
        }
        commands.async { [song, keys] in try? song.select(keys: keys) }
    }

    /// Forgets selected objects that a change removed.
    private func dropMissingSelection() {
        let clips = selectedClips.intersection(placedClips().map(\.clip.key))
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
    }

    public var canDelete: Bool {
        !selectedClips.isEmpty || selectedPoint != nil || (selectedRow != nil && selectedRow != .master)
    }

    /// Removes the selected clips, or else the selected automation point,
    /// track or return.
    public func deleteSelection() {
        if !selectedClips.isEmpty {
            edit(.clipsRemove(clips: selectedClips.sorted()))
        } else if let point = selectedPoint {
            edit(.pointRemove(point: point))
        } else if let row = selectedRow, row != .master {
            edit(.remove(row: row.row))
        }
    }

    /// Copies the selected clips to right after them and selects the copies.
    public func duplicateSelection() {
        guard !selectedClips.isEmpty else { return }
        edit(.clipsDuplicate(clips: selectedClips.sorted())) { [weak self] made in
            self?.select(clips: Set(made))
        }
    }

    /// Adds a track under the selected one, or else last, and asks for its name.
    public func addTrack() {
        var index = arrangement.tracks.count
        if case .track(let key) = selectedRow, let at = arrangement.tracks.firstIndex(where: { $0.key == key }) {
            index = at + 1
        }
        add(.trackAdd(index: UInt32(index))) { .track($0) }
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

    func warning(message: String) {
        onMain { $0.warn(message) }
    }

    func closed() {
        onMain { $0.hostClosed() }
    }
}
