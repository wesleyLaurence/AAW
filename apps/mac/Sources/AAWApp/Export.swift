import AppKit
import UniformTypeIdentifiers

/// File › Export Audio…: the mix as a named WAV, AAC or MP3 file at a stated
/// level. The app runs the bundle's `daw export` with the options the panel
/// chose, so there is one export, the agent's and the person's: the file, the
/// level policies, the dither, the ceiling and the record beside the file are
/// the command's, and a change to `daw export` shows up here by itself.
struct ExportOptions: Equatable {
    /// The file's format, as `--to NAME.EXT` and `--bits` name it.
    enum Format: String, CaseIterable {
        case wav24, wav16, aac, mp3

        var name: String {
            switch self {
            case .wav24: "WAV 24-bit"
            case .wav16: "WAV 16-bit"
            case .aac: "AAC"
            case .mp3: "MP3"
            }
        }

        var fileExtension: String {
            switch self {
            case .wav24, .wav16: "wav"
            case .aac: "m4a"
            case .mp3: "mp3"
            }
        }

        var contentType: UTType {
            switch self {
            case .wav24, .wav16: .wav
            // Apple's own M4A type, whose extension is `m4a`; `.mpeg4Audio`
            // would have the panel write `.mp4`.
            case .aac: UTType(filenameExtension: "m4a") ?? .mpeg4Audio
            case .mp3: .mp3
            }
        }

        /// The format a file's extension names, 24-bit for a WAV.
        init?(fileExtension: String) {
            switch fileExtension.lowercased() {
            case "wav": self = .wav24
            case "m4a": self = .aac
            case "mp3": self = .mp3
            default: return nil
            }
        }
    }

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

    var format = Format.wav24
    var level = Level.asRendered
    /// The highest sample peak a gain may reach, in dBFS: the command's -0.1
    /// unless changed.
    var ceiling = defaultCeiling

    static let defaultCeiling = -0.1

    /// The arguments of the `daw export` that writes `to` from `project`
    /// with these options. `replace` writes over a file that is there,
    /// which the panel asked about.
    func arguments(project: String, to: String, replace: Bool) -> [String] {
        var arguments = ["export", project, "--to", to]
        if format == .wav16 { arguments += ["--bits", "16"] }
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

    /// The name the panel offers: the song's title under the format's extension.
    func fileName(title: String) -> String {
        let name = title.trimmingCharacters(in: .whitespaces)
        return "\(name.isEmpty ? "Untitled" : name).\(format.fileExtension)"
    }

    // MARK: Remembered between exports

    private static let formatKey = "export.format"
    private static let levelKey = "export.level"
    private static let ceilingKey = "export.ceiling"

    init() {}

    /// The options chosen last time, or the defaults.
    init(defaults: UserDefaults) {
        if let format = defaults.string(forKey: Self.formatKey).flatMap(Format.init(rawValue:)) { self.format = format }
        if let level = defaults.string(forKey: Self.levelKey).flatMap(Level.parse) { self.level = level }
        if defaults.object(forKey: Self.ceilingKey) != nil { ceiling = defaults.double(forKey: Self.ceilingKey) }
    }

    func remember(in defaults: UserDefaults) {
        defaults.set(format.rawValue, forKey: Self.formatKey)
        defaults.set(level.text, forKey: Self.levelKey)
        defaults.set(ceiling, forKey: Self.ceilingKey)
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

    var name: String { (file as NSString).lastPathComponent }

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

    /// The measurements in a line: the length, the loudness, the true peak,
    /// and when a gain was applied, how much, and how much the ceiling held
    /// back.
    var measurements: String {
        var parts = [Self.length(durationSeconds)]
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
    private final class Read: @unchecked Sendable {
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
