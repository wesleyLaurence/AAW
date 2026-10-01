import AAWCore
import Foundation
import Observation

/// A song open in the app. It shows what the session host reports and sends
/// the transport's commands; the song and every rule about it live in the
/// host, which the agent's `daw` commands reach as well.
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
    /// A transport command the host refused.
    public private(set) var refusal: String?
    /// True for a moment after each change by the agent.
    public private(set) var agentWorking = false
    /// True for a moment after the tempo, title or length changed.
    public private(set) var sessionChanged = false
    /// The host shut down, as after `daw close`.
    public private(set) var closed = false
    /// The position the transport bar shows.
    public var position: Double = 0
    public var showsActivity = true

    /// Called with each new revision, for the arrangement view to animate.
    @ObservationIgnored var onUpdate: ((Update) -> Void)?
    @ObservationIgnored var onTransport: (() -> Void)?
    @ObservationIgnored var onClosed: (() -> Void)?
    @ObservationIgnored var onZoom: ((Zoom) -> Void)?

    @ObservationIgnored private let song: Song
    /// Commands go to the host in order, off the main thread: the host may be
    /// busy compiling the song.
    @ObservationIgnored private let commands = DispatchQueue(label: "aaw.commands")
    /// The last loop, to turn it back on.
    @ObservationIgnored private var lastLoop: LoopRegion?
    @ObservationIgnored private var agentTimer: Task<Void, Never>?
    @ObservationIgnored private var sessionTimer: Task<Void, Never>?
    @ObservationIgnored private var refusalTimer: Task<Void, Never>?

    static let activityLimit = 200

    /// Opens the song and becomes its host. Throws when the song does not
    /// load or another host has it open.
    public init(url: URL) throws {
        let relay = Relay()
        song = try Song.open(path: url.path, observer: relay)
        self.url = url
        arrangement = song.arrangement()
        relay.model = self
    }

    // MARK: What the host reports

    fileprivate func apply(_ update: Update) {
        arrangement = update.arrangement
        activity.insert(update.change, at: 0)
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

    /// Turns the loop off, or back on over its last region. With no earlier
    /// loop, it covers the section at the start position, else that bar.
    public func toggleLoop() {
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
                let message = error.localizedDescription
                DispatchQueue.main.async {
                    MainActor.assumeIsolated { self?.refuse(message) }
                }
            }
        }
    }

    private func refuse(_ message: String) {
        refusal = message
        refusalTimer?.cancel()
        refusalTimer = after(4) { $0.refusal = nil }
    }

    func zoom(_ zoom: Zoom) {
        onZoom?(zoom)
    }

    /// Saves and stops hosting the song; `daw` commands then run headless.
    public func close() {
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
