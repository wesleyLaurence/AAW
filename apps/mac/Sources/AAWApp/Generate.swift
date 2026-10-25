import AAWCore
import AppKit
import Observation

/// The bundle's `daw` run as a process off the main thread, as File › Export
/// Audio… runs it, for what the app asks of a service: a key kept or
/// checked, a sound or a song generated. Its result is the command's JSON;
/// its failure the command's reason. It can be stopped.
final class DawProcess: @unchecked Sendable {
    enum Outcome: Equatable {
        case done(Data)
        case failed(String)
        /// Stopped by `cancel()`.
        case cancelled
    }

    private let process = Process()
    private let lock = NSLock()
    private var stopped = false

    /// Starts `daw` with `arguments`, `input` written to its standard input
    /// and then closed, and calls `done` on the main thread when it ends.
    /// Nil `daw` fails at once, as a build that is not a bundle has none.
    @discardableResult
    static func run(_ daw: URL?, _ arguments: [String], input: Data? = nil,
                    done: @escaping @MainActor (Outcome) -> Void) -> DawProcess {
        let run = DawProcess()
        guard let daw else {
            DispatchQueue.main.async { done(.failed("This build has no daw inside it: apps/mac/build.sh makes an app with daw in its bundle.")) }
            return run
        }
        DispatchQueue.global(qos: .userInitiated).async {
            let outcome = run.wait(daw, arguments, input)
            DispatchQueue.main.async { done(outcome) }
        }
        return run
    }

    /// Stops the command; its outcome is `cancelled`.
    func cancel() {
        lock.lock()
        stopped = true
        let running = process.isRunning
        lock.unlock()
        if running { process.terminate() }
    }

    private func wait(_ daw: URL, _ arguments: [String], _ input: Data?) -> Outcome {
        let out = Pipe()
        let err = Pipe()
        let stdin = Pipe()
        process.executableURL = daw
        process.arguments = arguments
        process.standardOutput = out
        process.standardError = err
        process.standardInput = input == nil ? FileHandle.nullDevice : stdin
        lock.lock()
        if stopped {
            lock.unlock()
            return .cancelled
        }
        do {
            try process.run()
        } catch {
            lock.unlock()
            return .failed("daw could not be started: \(error.localizedDescription)")
        }
        lock.unlock()
        if let input {
            try? stdin.fileHandleForWriting.write(contentsOf: input)
            try? stdin.fileHandleForWriting.close()
        }
        // Both pipes are read as they fill, as the export reads them.
        let errors = Exporter.Read(err.fileHandleForReading)
        let output = out.fileHandleForReading.readDataToEndOfFile()
        let said = errors.wait()
        process.waitUntilExit()
        lock.lock()
        let cancelled = stopped
        lock.unlock()
        if cancelled { return .cancelled }
        guard process.terminationStatus == 0 else {
            return .failed(Exporter.reason(stderr: String(decoding: said, as: UTF8.self), status: process.terminationStatus))
        }
        return .done(output)
    }
}

/// What `daw keys status` says of the ElevenLabs key: whether one is kept,
/// the plan's tier and credits, the agent's budget, and what went wrong.
struct KeyStatus: Equatable {
    /// True when a key is kept, false when none is, nil when the Keychain
    /// could not be read.
    var kept: Bool?
    var tier: String?
    var creditsLeft: Double?
    var creditsLimit: Double?
    /// When the credits come back, as the service's `2026-11-01T00:00:00Z`.
    var creditsReset: String?
    /// Why the key could not be checked with the service, or read.
    var problem: String?
    var budget: Budget?

    /// The agent's daily budget, in seconds of audio, and today's spend.
    struct Budget: Equatable {
        var perDay: Double
        var agentToday: Double
        var personToday: Double
        var left: Double

        init?(_ object: Any?) {
            guard let object = object as? [String: Any], let perDay = object["agent_seconds_per_day"] as? Double else { return nil }
            self.perDay = perDay
            let spent = object["spent_today"] as? [String: Any]
            agentToday = (spent?["agent"] as? Double) ?? 0
            personToday = (spent?["user"] as? Double) ?? 0
            left = (object["left_today"] as? Double) ?? perDay
        }

        init(perDay: Double, agentToday: Double, personToday: Double, left: Double) {
            (self.perDay, self.agentToday, self.personToday, self.left) = (perDay, agentToday, personToday, left)
        }
    }

    init(kept: Bool?, tier: String? = nil, creditsLeft: Double? = nil, creditsLimit: Double? = nil,
         creditsReset: String? = nil, problem: String? = nil, budget: Budget? = nil) {
        self.kept = kept
        self.tier = tier
        self.creditsLeft = creditsLeft
        self.creditsLimit = creditsLimit
        self.creditsReset = creditsReset
        self.problem = problem
        self.budget = budget
    }

    /// The JSON of `daw keys status`, or of `daw keys set`, which says the
    /// key is kept and checks it the same way.
    init?(json data: Data) {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        if let stored = object["stored"] as? Bool {
            kept = stored
        } else {
            kept = object["key"] as? Bool
        }
        let subscription = object["subscription"] as? [String: Any]
        tier = (subscription?["tier"] as? String) ?? (object["tier"] as? String)
        creditsLeft = subscription?["credits_left"] as? Double
        creditsLimit = subscription?["credits_limit"] as? Double
        creditsReset = subscription?["credits_reset"] as? String
        problem = object["problem"] as? String
        budget = Budget(object["budget"])
    }

    /// The tier as the service's pages name it: `creator` is Creator.
    static func plan(_ tier: String) -> String {
        tier.split(separator: "_").map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined(separator: " ")
    }

    /// A day as the person reads it, from the service's ISO date.
    static func day(_ iso: String) -> String {
        guard let date = ISO8601DateFormatter().date(from: iso) else { return iso }
        let format = DateFormatter()
        format.dateStyle = .medium
        format.timeStyle = .none
        format.timeZone = TimeZone(identifier: "UTC")
        return format.string(from: date)
    }

    /// The plan in a line: its tier, the credits left of how many, and when
    /// they come back.
    var planLine: String? {
        guard let tier else { return nil }
        var parts = ["\(Self.plan(tier)) plan"]
        if let creditsLeft {
            let number = NumberFormatter()
            number.numberStyle = .decimal
            number.maximumFractionDigits = 0
            let left = number.string(from: NSNumber(value: creditsLeft)) ?? "\(Int(creditsLeft))"
            let limit = creditsLimit.flatMap { number.string(from: NSNumber(value: $0)) }
            parts.append(limit.map { "\(left) of \($0) credits left" } ?? "\(left) credits left")
        }
        if let creditsReset { parts.append("renews \(Self.day(creditsReset))") }
        return parts.joined(separator: " · ")
    }
}

/// The ElevenLabs key as the app knows it, through the bundle's `daw keys`:
/// the app never reads or holds the key itself. Saving pipes what was typed
/// to `daw keys set`, so the Keychain item is made by the `daw` that reads it
/// for every request, and nothing else is given it.
@MainActor
@Observable
final class ElevenLabsAccount {
    static let shared = ElevenLabsAccount(daw: Exporter.daw)

    static let service = "elevenlabs"
    /// Where a person makes a key.
    static let keysPage = URL(string: "https://elevenlabs.io/app/settings/api-keys")!

    /// What `daw keys status` said last; nil before it has.
    private(set) var status: KeyStatus?
    /// A status, a save, a removal or a budget is being asked for.
    private(set) var busy = false
    /// What the last save, removal or budget came to, or why it failed.
    private(set) var message: String?
    private(set) var failed = false

    @ObservationIgnored let daw: URL?

    init(daw: URL?) {
        self.daw = daw
    }

    /// Asks `daw keys status` again: once when the Generate tab or the
    /// Settings pane first shows, and after each change.
    func refresh() {
        busy = true
        DawProcess.run(daw, ["keys", "status"]) { [weak self] outcome in
            guard let self else { return }
            busy = false
            switch outcome {
            case .done(let data): status = KeyStatus(json: data) ?? status
            case .failed(let reason): status = KeyStatus(kept: nil, problem: reason)
            case .cancelled: break
            }
        }
    }

    /// Keeps `key` in the Keychain, through `daw keys set`, which checks it
    /// with the service once.
    func save(key: String) {
        let key = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !key.isEmpty else { return }
        change(["keys", "set", Self.service], input: Data((key + "\n").utf8)) { data in
            let said = KeyStatus(json: data)
            if let problem = said?.problem { return (true, "The key is kept, but the service did not take it: \(problem)") }
            return (false, "The key is kept in the Keychain and the service took it.")
        }
    }

    func remove() {
        change(["keys", "remove", Self.service]) { data in
            let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            return (false, (object?["removed"] as? Bool) == false ? "No key was kept." : "The key is gone from the Keychain.")
        }
    }

    /// Sets how many seconds of audio the agent may generate in a day
    /// without asking, 0 to 86,400.
    func setBudget(_ seconds: Double) {
        guard (0...86_400).contains(seconds) else {
            message = "The agent's budget is 0 to 86,400 seconds a day."
            failed = true
            return
        }
        change(["keys", "budget", "--agent-seconds", ExportOptions.number(seconds)]) { _ in
            (false, "The agent may now generate \(GenerateRequest.length(seconds)) a day without asking.")
        }
    }

    private func change(_ arguments: [String], input: Data? = nil, said: @escaping (Data) -> (Bool, String)) {
        busy = true
        message = nil
        DawProcess.run(daw, arguments, input: input) { [weak self] outcome in
            guard let self else { return }
            switch outcome {
            case .done(let data): (failed, message) = said(data)
            case .failed(let reason): (failed, message) = (true, reason)
            case .cancelled: break
            }
            refresh()
        }
    }
}

/// What the Generate tab asks for: a sound, a loop or a song from a
/// description, as `daw generate sound` or `daw generate music` takes it.
struct GenerateRequest: Equatable {
    enum Kind: String, CaseIterable {
        case sound, loop, song

        var name: String {
            switch self {
            case .sound: "Sound"
            case .loop: "Loop"
            case .song: "Song"
            }
        }
    }

    var kind = Kind.sound
    var text = ""
    /// A sound's length, nil for the model to pick: 0.5 to 30 seconds.
    var soundSeconds: Double?
    /// A loop's length in bars of the song's tempo and meter.
    var bars: Double = 2
    /// A song's length, 3 to 300 seconds.
    var songSeconds: Double = 30
    var instrumental = false

    /// What the command would refuse, said before anything is sent.
    var problem: String? {
        if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return "Describe what to make." }
        switch kind {
        case .sound:
            if let s = soundSeconds, !(0.5...30).contains(s) { return "A sound is 0.5 to 30 seconds." }
        case .loop:
            if !(bars > 0 && bars <= 64) { return "A loop is a number of bars, at most 30 seconds long." }
        case .song:
            if !(3...300).contains(songSeconds) { return "A song is 3 to 300 seconds." }
        }
        return nil
    }

    /// The arguments of the `daw` command that makes it in the song at
    /// `song`, as the person's own request: it does not count against the
    /// agent's budget.
    func arguments(song: String) -> [String] {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        var arguments = ["--origin", "user", "generate"]
        switch kind {
        case .sound:
            arguments += ["sound", song, text]
            if let soundSeconds { arguments += ["--seconds", ExportOptions.number(soundSeconds)] }
        case .loop:
            arguments += ["sound", song, text, "--loop", "--bars", ExportOptions.number(bars)]
        case .song:
            arguments += ["music", song, text, "--seconds", ExportOptions.number(songSeconds)]
            if instrumental { arguments.append("--instrumental") }
        }
        return arguments
    }

    /// The request a result was made by, to make it again.
    init(again result: GeneratedInfo) {
        text = result.prompt
        switch result.kind {
        case "music":
            kind = .song
            songSeconds = result.askedSeconds ?? result.seconds.rounded()
            instrumental = result.instrumental
        case "loop":
            kind = .loop
            if let bars = result.bars { self.bars = bars }
        default:
            kind = .sound
            soundSeconds = result.askedSeconds
        }
    }

    init() {}

    /// Seconds as `4.5 s`, or `2:30` from a minute up.
    static func length(_ seconds: Double) -> String {
        if seconds >= 60 { return ExportReport.length(seconds) }
        return "\(ExportOptions.number((seconds * 10).rounded() / 10)) s"
    }
}

/// The Generate tab's state for one song: the form, the request running,
/// what it came to, and the results it lists.
@MainActor
@Observable
final class Generator {
    var request = GenerateRequest()
    /// The request being made and when it started.
    private(set) var running: GenerateRequest?
    private(set) var started: Date?
    /// Why the last request made nothing.
    private(set) var error: String?
    /// What the last request made, in a line, and its notes.
    private(set) var made: String?
    private(set) var notes: [String] = []
    /// The song's results, newest first, then the generated folder's.
    private(set) var results: [GeneratedInfo] = []
    @ObservationIgnored private var process: DawProcess?
    @ObservationIgnored let daw: URL?
    @ObservationIgnored private let queue = DispatchQueue(label: "aaw.generated")

    init(daw: URL?) {
        self.daw = daw
    }

    /// Reads the recipes again, off the main thread.
    func refresh(song: URL) {
        let path = song.path
        queue.async {
            let found = libraryGenerated(song: path)
            DispatchQueue.main.async { MainActor.assumeIsolated { self.results = found } }
        }
    }

    /// Makes what the form describes, as the person's request. The song
    /// takes the result as an edit and keeps playing meanwhile.
    func generate(song: URL) {
        guard running == nil else { return }
        if let problem = request.problem {
            error = problem
            return
        }
        start(request, song: song)
    }

    /// Fills the form with a result's request and makes it again.
    func again(_ result: GeneratedInfo, song: URL) {
        guard running == nil, !result.fromPlan else { return }
        request = GenerateRequest(again: result)
        generate(song: song)
    }

    /// Stops the request: nothing is imported. What the service made is
    /// paid for and lost.
    func cancel() {
        process?.cancel()
    }

    private func start(_ request: GenerateRequest, song: URL) {
        running = request
        started = Date()
        error = nil
        made = nil
        notes = []
        process = DawProcess.run(daw, request.arguments(song: song.path)) { [weak self] outcome in
            guard let self else { return }
            running = nil
            started = nil
            process = nil
            switch outcome {
            case .done(let data):
                let reply = (try? JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
                made = Self.madeLine(reply)
                notes = (reply["notes"] as? [String]) ?? []
            case .failed(let reason): error = reason
            case .cancelled: error = "Stopped. Nothing was added to the song."
            }
            refresh(song: song)
            ElevenLabsAccount.shared.refresh()
        }
    }

    /// The reply of `daw generate` in a line: the sample it added and how long.
    static func madeLine(_ reply: [String: Any]) -> String {
        let id = (reply["sample_id"] as? String) ?? "a sample"
        let seconds = (reply["seconds"] as? Double).map { " of \(GenerateRequest.length($0))" } ?? ""
        let what = (reply["title"] as? String).map { "\($0) is in the song" } ?? "It is in the song"
        return "\(what) as \(id)\(seconds). Nothing is placed: drag it onto the timeline or a MIDI track's name."
    }

    /// What the tab shows while a request runs: `Making a song… 0:12`.
    static func progress(_ request: GenerateRequest, since started: Date, now: Date) -> String {
        let what = request.kind == .song ? "a song" : request.kind == .loop ? "a loop" : "a sound"
        return "Making \(what)… \(ExportReport.length(max(0, now.timeIntervalSince(started))))"
    }

    /// A result as a sample of the library, for the browser's rows: played on
    /// a click, dragged and added as one, named after its ID.
    nonisolated static func sample(_ result: GeneratedInfo) -> SampleInfo {
        SampleInfo(id: result.path, name: shownName(result), pack: "", path: result.path, seconds: result.seconds,
                   channels: 0, category: result.name, kind: result.kind == "loop" ? "loop" : "one-shot",
                   bpm: nil, key: nil, note: nil, rootNote: nil)
    }

    /// A song's title, else the description, else the ID.
    nonisolated static func shownName(_ result: GeneratedInfo) -> String {
        if let title = result.title, !title.isEmpty { return title }
        return result.prompt.isEmpty ? result.name : result.prompt
    }

    /// A result's line under its name: what it is, how long, and when, by
    /// the time alone if today.
    nonisolated static func detail(_ result: GeneratedInfo, now: Date = Date()) -> String {
        var parts = [result.kind == "music" ? "Song" : result.kind == "loop" ? "Loop" : "Sound", GenerateRequest.length(result.seconds)]
        if let date = ISO8601DateFormatter().date(from: result.created) {
            let today = Calendar.current.isDate(date, inSameDayAs: now)
            let format = DateFormatter()
            format.doesRelativeDateFormatting = true
            format.dateStyle = today ? .none : .short
            format.timeStyle = today ? .short : .none
            parts.append(format.string(from: date))
        }
        return parts.joined(separator: " · ")
    }

    /// A result's tooltip: the sample that plays it in the song, and the
    /// description in full.
    nonisolated static func help(_ result: GeneratedInfo) -> String {
        let text = result.prompt.isEmpty ? "Made from a composition plan" : result.prompt
        return result.sampleId.map { "In the song as \($0). \(text)" } ?? text
    }

    /// Whether every word of `query` is in the result's description, title or ID.
    nonisolated static func matches(_ result: GeneratedInfo, _ query: String) -> Bool {
        query.split(separator: " ").allSatisfy { word in
            result.prompt.localizedCaseInsensitiveContains(word) || (result.title ?? "").localizedCaseInsensitiveContains(word)
                || result.name.localizedCaseInsensitiveContains(word)
        }
    }
}
