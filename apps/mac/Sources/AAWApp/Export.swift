import AppKit
import UniformTypeIdentifiers

/// File › Export Audio…: what is heard of the song, whole or a range of it,
/// as a named WAV, AIFF, FLAC, AAC or MP3 file at a stated rate, depth and
/// level. The app runs the bundle's `daw export` with the options the sheet
/// chose, so there is one export, the agent's and the person's: the range,
/// the file, the level policies, the dither, the ceiling and the record
/// beside the file are the command's, and a change to `daw export` shows up
/// here by itself.
struct ExportOptions: Equatable {
    /// The file's format, as `--to NAME.EXT` names it.
    enum Format: String, CaseIterable {
        case wav, aiff, flac, aac, mp3

        var name: String {
            switch self {
            case .wav: "WAV"
            case .aiff: "AIFF"
            case .flac: "FLAC"
            case .aac: "AAC"
            case .mp3: "MP3"
            }
        }

        var fileExtension: String {
            switch self {
            case .wav: "wav"
            case .aiff: "aiff"
            case .flac: "flac"
            case .aac: "m4a"
            case .mp3: "mp3"
            }
        }

        var contentType: UTType {
            switch self {
            case .wav: .wav
            case .aiff: .aiff
            case .flac: UTType(filenameExtension: "flac") ?? .audio
            // Apple's own M4A type, whose extension is `m4a`; `.mpeg4Audio`
            // would have the panel write `.mp4`.
            case .aac: UTType(filenameExtension: "m4a") ?? .mpeg4Audio
            case .mp3: .mp3
            }
        }

        /// AAC and MP3 have a bitrate, not a bit depth, and are at most 48 kHz.
        var compressed: Bool { self == .aac || self == .mp3 }

        /// The bit depths the format is written at; 32 is float.
        var depths: [Int] {
            switch self {
            case .wav, .aiff: [16, 24, 32]
            case .flac: [16, 24]
            case .aac, .mp3: []
            }
        }

        /// The format a file's extension names.
        init?(fileExtension: String) {
            switch fileExtension.lowercased() {
            case "wav": self = .wav
            case "aif", "aiff": self = .aiff
            case "flac": self = .flac
            case "m4a": self = .aac
            case "mp3": self = .mp3
            default: return nil
            }
        }
    }

    /// The sample rates the sheet offers; the song's is offered too.
    static let rates = [44100, 48000, 88200, 96000]
    /// The highest rate AAC and MP3 are written at.
    static let compressedRate = 48000
    /// The bitrates the sheet offers, in kb/s.
    static let bitrates = [128, 192, 256, 320]

    /// The level policy: one gain for the whole file, or none.
    enum Level: Equatable {
        case asRendered
        /// `--peak`: the estimated true peak put here, in dBTP.
        case peak(Double)
        /// `--lufs`: this integrated loudness.
        case lufs(Double)
        /// `--gain`: this gain in dB.
        case gain(Double)

        /// The policy's name, as the panel's menu and the scripted
        /// `--export-level` spell it.
        var policy: String {
            switch self {
            case .asRendered: "rendered"
            case .peak: "peak"
            case .lufs: "lufs"
            case .gain: "gain"
            }
        }

        var value: Double? {
            switch self {
            case .asRendered: nil
            case .peak(let v), .lufs(let v), .gain(let v): v
            }
        }

        /// The value's unit, as the panel shows it.
        var unit: String {
            switch self {
            case .asRendered: ""
            case .peak: "dBTP"
            case .lufs: "LUFS"
            case .gain: "dB"
            }
        }

        /// A policy with a value: the values a person usually wants where
        /// none is given yet.
        static func make(_ policy: String, _ value: Double?) -> Level? {
            switch policy {
            case "rendered": .asRendered
            case "peak": .peak(value ?? -1)
            case "lufs": .lufs(value ?? -14)
            case "gain": .gain(value ?? 0)
            default: nil
            }
        }

        /// `peak=-1`, `lufs=-14`, `gain=-3` or `rendered`, as the command
        /// line and the defaults spell a level.
        static func parse(_ text: String) -> Level? {
            let parts = text.split(separator: "=", maxSplits: 1).map(String.init)
            guard let policy = parts.first else { return nil }
            if parts.count == 1 { return policy == "rendered" ? .asRendered : nil }
            guard let value = Double(parts[1]) else { return nil }
            return make(policy, value)
        }

        var text: String {
            value.map { "\(policy)=\($0)" } ?? policy
        }
    }

    var format = Format.wav
    /// The bit depth of a WAV, AIFF or FLAC file: 16, 24 or 32 (float).
    var bits = 24
    /// The sample rate, or nil for the song's.
    var sampleRate: Int?
    /// The bitrate of an AAC or MP3 file in kb/s, or nil for the command's:
    /// 256 for AAC, 320 for MP3.
    var bitrate: Int?
    var level = Level.asRendered
    /// The highest sample peak a gain may reach, in dBFS: the command's -0.1
    /// unless changed.
    var ceiling = defaultCeiling

    static let defaultCeiling = -0.1

    /// The bit depth the file is written at: the one chosen, or 24 where
    /// the format has no such depth; nil for AAC and MP3.
    var depth: Int? {
        format.compressed ? nil : format.depths.contains(bits) ? bits : 24
    }

    /// The rate the file is written at, or nil for the song's: one chosen
    /// above what AAC and MP3 carry is 48 kHz for them.
    var rate: Int? {
        guard let sampleRate else { return nil }
        return format.compressed ? min(sampleRate, Self.compressedRate) : sampleRate
    }

    /// The arguments of the `daw export` that writes `to` from `project`
    /// with these options, over `range` or the whole song. `replace` writes
    /// over a file that is there, which the person was asked about.
    func arguments(project: String, to: String, replace: Bool, range: ExportRange? = nil) -> [String] {
        var arguments = ["export", project, "--to", to]
        if let range { arguments += ["--start", Self.number(range.start), "--end", Self.number(range.end)] }
        if let depth, depth != 24 { arguments += ["--bits", String(depth)] }
        if format.compressed, let bitrate { arguments += ["--bitrate", String(bitrate)] }
        if let rate { arguments += ["--sample-rate", String(rate)] }
        switch level {
        case .asRendered: break
        case .peak(let v): arguments += ["--peak", Self.number(v)]
        case .lufs(let v): arguments += ["--lufs", Self.number(v)]
        case .gain(let v): arguments += ["--gain", Self.number(v)]
        }
        if level != .asRendered, ceiling != Self.defaultCeiling { arguments += ["--ceiling", Self.number(ceiling)] }
        if replace { arguments.append("--replace") }
        return arguments
    }

    /// A number as the command reads it: `-1`, not `-1.0`.
    static func number(_ value: Double) -> String {
        value == value.rounded() && abs(value) < 1e15 ? String(Int(value)) : String(value)
    }

    /// The name the sheet offers: the song's title.
    static func name(title: String) -> String {
        let name = title.trimmingCharacters(in: .whitespaces)
        return name.isEmpty ? "Untitled" : name
    }

    /// The file a name typed in the sheet is: under the format's extension,
    /// not twice, with no folder in it.
    func fileName(_ typed: String) -> String {
        var name = typed.trimmingCharacters(in: .whitespaces)
            .replacingOccurrences(of: "/", with: "-").replacingOccurrences(of: ":", with: "-")
        let suffix = ".\(format.fileExtension)"
        if name.lowercased().hasSuffix(suffix) { name = String(name.dropLast(suffix.count)) }
        return "\(name.isEmpty ? "Untitled" : name)\(suffix)"
    }

    // MARK: Remembered between exports

    private static let formatKey = "export.format"
    private static let bitsKey = "export.bits"
    private static let rateKey = "export.rate"
    private static let bitrateKey = "export.bitrate"
    private static let levelKey = "export.level"
    private static let ceilingKey = "export.ceiling"

    init() {}

    /// The options chosen last time, or the defaults. A format kept before
    /// the bit depth was its own, `wav24` or `wav16`, is a WAV at that depth.
    init(defaults: UserDefaults) {
        switch defaults.string(forKey: Self.formatKey) {
        case "wav24": format = .wav
        case "wav16": format = .wav; bits = 16
        case let name?: if let format = Format(rawValue: name) { self.format = format }
        case nil: break
        }
        if defaults.object(forKey: Self.bitsKey) != nil { bits = defaults.integer(forKey: Self.bitsKey) }
        if defaults.integer(forKey: Self.rateKey) > 0 { sampleRate = defaults.integer(forKey: Self.rateKey) }
        if defaults.integer(forKey: Self.bitrateKey) > 0 { bitrate = defaults.integer(forKey: Self.bitrateKey) }
        if let level = defaults.string(forKey: Self.levelKey).flatMap(Level.parse) { self.level = level }
        if defaults.object(forKey: Self.ceilingKey) != nil { ceiling = defaults.double(forKey: Self.ceilingKey) }
    }

    func remember(in defaults: UserDefaults) {
        defaults.set(format.rawValue, forKey: Self.formatKey)
        defaults.set(bits, forKey: Self.bitsKey)
        defaults.set(sampleRate ?? 0, forKey: Self.rateKey)
        defaults.set(bitrate ?? 0, forKey: Self.bitrateKey)
        defaults.set(level.text, forKey: Self.levelKey)
        defaults.set(ceiling, forKey: Self.ceilingKey)
    }
}

/// The beats of the song an export covers, from `start` to where it stops,
/// `end`, in quarter-note beats from the song's start as every position is.
/// The sheet shows them as bar, beat and sixteenth in the song's meter, as
/// the transport does, a sixteenth with a decimal for a place between them.
struct ExportRange: Equatable {
    var start: Double
    var end: Double

    /// A position as a bar, a beat and a sixteenth, each from 1, the beat the
    /// note the meter counts: beat 8.5 in 4/4 is bar 3, beat 1, sixteenth 3.
    /// `bar` and `unit` are in quarter-note beats, a bar of 3/4 three, the
    /// eighth of 6/8 a half. The sixteenth has a fraction off the grid.
    static func place(_ beats: Double, bar: Double, unit: Double) -> (bar: Int, beat: Int, sixteenth: Double) {
        // Rounded to a millionth, so 2.9999999 is bar 2's end, not its last beat.
        let at = (beats * 1e6).rounded() / 1e6
        let index = (at / bar + 1e-9).rounded(.down)
        let inBar = at - index * bar
        let beat = min((inBar / unit + 1e-9).rounded(.down), (bar / unit).rounded(.up) - 1)
        let sixteenth = ((inBar - beat * unit) * 4 * 1e6).rounded() / 1e6 + 1
        return (Int(index) + 1, Int(beat) + 1, sixteenth)
    }

    /// A bar, a beat and a sixteenth from 1 as beats from the song's start.
    static func beats(bar: Int, beat: Int, sixteenth: Double, barBeats: Double, unit: Double) -> Double {
        Double(bar - 1) * barBeats + Double(beat - 1) * unit + (sixteenth - 1) / 4
    }

    /// A position as the transport writes it, bar.beat.sixteenth, at the
    /// nearest sixteenth, and how far off it the place is in milliseconds
    /// when it is off the grid: `2.1.1`, `1.4.4 + 55 ms`.
    static func text(_ beats: Double, bar: Double, unit: Double, tempo: Double) -> String {
        let grid = (beats * 4).rounded() / 4
        let base = TimelineLayout.position(grid, beatsPerBar: bar, beatUnit: unit)
        let ms = ((beats - grid) * 60 / tempo * 1000).rounded()
        guard ms != 0 else { return base }
        return "\(base) \(ms > 0 ? "+" : "−") \(Int(abs(ms))) ms"
    }

    /// The whole song, or the selected clips' span inside it.
    static func initial(selection: (start: Double, end: Double)?, length: Double) -> ExportRange {
        if let s = selection {
            let range = ExportRange(start: max(0, s.start), end: min(length, s.end))
            if range.end > range.start { return range }
        }
        return ExportRange(start: 0, end: length)
    }

    func isWhole(length: Double) -> Bool {
        start <= 1e-9 && abs(end - length) <= 1e-9
    }

    /// Why the range cannot be exported, or nil.
    func problem(length: Double, bar: Double, unit: Double, tempo: Double) -> String? {
        if start < -1e-9 { return "The start is before bar 1." }
        if end <= start + 1e-9 { return "The end must be after the start." }
        if end > length + 1e-9 {
            return "The song ends at \(Self.text(length, bar: bar, unit: unit, tempo: tempo)); nothing after it is rendered."
        }
        return nil
    }
}

/// What `daw export` reported of the file it wrote: what the banner shows.
struct ExportReport: Equatable {
    var file: String
    var durationSeconds: Double
    var policy: String
    var gainDb: Double
    var heldBackDb: Double
    var integratedLufs: Double?
    var truePeakDbtp: Double?
    var warnings: [String]
    var renderedNow: Bool
    /// Where the range began and stopped in beats, when it was not the
    /// whole song.
    var range: (start: Double, end: Double)?

    var name: String { (file as NSString).lastPathComponent }

    static func == (a: ExportReport, b: ExportReport) -> Bool {
        a.file == b.file && a.durationSeconds == b.durationSeconds && a.policy == b.policy && a.gainDb == b.gainDb
            && a.heldBackDb == b.heldBackDb && a.integratedLufs == b.integratedLufs && a.truePeakDbtp == b.truePeakDbtp
            && a.warnings == b.warnings && a.renderedNow == b.renderedNow && a.range?.start == b.range?.start && a.range?.end == b.range?.end
    }

    /// The command's JSON result.
    init?(json data: Data) {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let file = object["file"] as? String,
              let level = object["level"] as? [String: Any],
              let policy = level["policy"] as? String else { return nil }
        self.file = file
        durationSeconds = (object["duration_seconds"] as? Double) ?? 0
        self.policy = policy
        gainDb = (level["gain_db"] as? Double) ?? 0
        heldBackDb = (level["held_back_db"] as? Double) ?? 0
        integratedLufs = level["integrated_lufs"] as? Double
        truePeakDbtp = level["estimated_true_peak_dbtp"] as? Double
        warnings = (object["warnings"] as? [String]) ?? []
        renderedNow = ((object["render"] as? [String: Any])?["rendered_now"] as? Bool) ?? false
        // In beats, which the app writes as the transport does.
        if let r = object["range"] as? [String: Any], r["whole_song"] as? Bool == false,
           let start = r["start_beat"] as? Double, let end = r["end_beat"] as? Double {
            range = (start, end)
        }
    }

    init(file: String, durationSeconds: Double, policy: String, gainDb: Double, heldBackDb: Double,
         integratedLufs: Double?, truePeakDbtp: Double?, warnings: [String], renderedNow: Bool) {
        self.file = file
        self.durationSeconds = durationSeconds
        self.policy = policy
        self.gainDb = gainDb
        self.heldBackDb = heldBackDb
        self.integratedLufs = integratedLufs
        self.truePeakDbtp = truePeakDbtp
        self.warnings = warnings
        self.renderedNow = renderedNow
    }

    /// Seconds as `m:ss`, or `m:ss.s` under ten seconds.
    static func length(_ seconds: Double) -> String {
        let whole = Int(seconds.rounded(.down))
        return String(format: "%d:%02d", whole / 60, whole % 60)
    }

    static func decibels(_ value: Double, _ unit: String, signed: Bool = false) -> String {
        let text = String(format: signed ? "%+.1f" : "%.1f", value).replacingOccurrences(of: "-", with: "−")
        return "\(text) \(unit)"
    }

    /// The measurements in a line: the range when it was not the whole
    /// song, its places written by `place`, the length, the loudness, the
    /// true peak, and when a gain was applied, how much, and how much the
    /// ceiling held back.
    func measurements(place: (Double) -> String) -> String {
        var parts = [Self.length(durationSeconds)]
        if let range { parts.insert("\(place(range.start)) to \(place(range.end))", at: 0) }
        if let integratedLufs { parts.append(Self.decibels(integratedLufs, "LUFS")) }
        if let truePeakDbtp { parts.append(Self.decibels(truePeakDbtp, "dBTP")) }
        if policy != "as rendered" {
            parts.append("\(Self.decibels(gainDb, "dB", signed: true)) applied")
            if heldBackDb > 0.05 { parts.append("\(Self.decibels(heldBackDb, "dB")) held back by the ceiling") }
        }
        return parts.joined(separator: " · ")
    }
}

/// What an export came to: the file and its measurements, or the command's
/// reason for writing nothing.
enum ExportOutcome: Equatable {
    case done(ExportReport)
    case failed(file: String, reason: String)
}

/// Runs `daw export` in the bundle's `daw`, off the main thread.
enum Exporter {
    /// The bundle's `daw`, or nil in a build that is not a bundle.
    @MainActor static var daw: URL? {
        Bundle.main.executableURL.flatMap { CommandLineTool.bundled(executable: $0)?.binary }
    }

    /// The command's reason, from what it wrote on stderr: the `error` of its
    /// JSON, or the text as it is.
    static func reason(stderr: String, status: Int32) -> String {
        let text = stderr.trimmingCharacters(in: .whitespacesAndNewlines)
        if let data = text.data(using: .utf8),
           let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let error = object["error"] as? String {
            return error
        }
        return text.isEmpty ? "daw export failed (status \(status))" : text
    }

    /// Runs the export and waits for it. Call off the main thread.
    nonisolated static func run(daw: URL?, arguments: [String], file: String) -> ExportOutcome {
        guard let daw else {
            return .failed(file: file, reason: "This build has no daw inside it: apps/mac/build.sh makes an app with daw in its bundle.")
        }
        let process = Process()
        let out = Pipe()
        let err = Pipe()
        process.executableURL = daw
        process.arguments = arguments
        process.standardOutput = out
        process.standardError = err
        do {
            try process.run()
        } catch {
            return .failed(file: file, reason: "daw could not be started: \(error.localizedDescription)")
        }
        // Both pipes are read as they fill: a reader that waited for one
        // while the command filled the other would wait forever.
        let errors = Read(err.fileHandleForReading)
        let output = out.fileHandleForReading.readDataToEndOfFile()
        let said = errors.wait()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            return .failed(file: file, reason: reason(stderr: String(decoding: said, as: UTF8.self), status: process.terminationStatus))
        }
        guard let report = ExportReport(json: output) else {
            return .failed(file: file, reason: "daw export printed no result")
        }
        return .done(report)
    }

    /// A pipe read to its end on another thread.
    final class Read: @unchecked Sendable {
        private var data = Data()
        private let group = DispatchGroup()

        init(_ handle: FileHandle) {
            group.enter()
            DispatchQueue.global(qos: .utility).async { [self] in
                data = handle.readDataToEndOfFile()
                group.leave()
            }
        }

        func wait() -> Data {
            group.wait()
            return data
        }
    }
}
