import AppKit
import SwiftUI

/// What the Export Audio sheet is choosing: the name, the range as bars and
/// beats, and the options `daw export` takes. The range starts as the
/// selected clips' span, or the whole song.
@MainActor
@Observable
final class ExportSetup {
    var options: ExportOptions
    var name: String
    var startBar: Int
    var startBeat: Double
    var endBar: Int
    var endBeat: Double

    /// A bar and the note the meter counts, in quarter-note beats.
    let barBeats: Double
    let beatUnit: Double
    let lengthBeats: Double
    let tempo: Double
    let timeSignature: String
    /// The song's sample rate, which the file is at unless another is chosen.
    let songRate: Int
    /// The range came from the selected clips.
    let fromSelection: Bool
    /// The project has a folder of the person's, whose `exports/` the file
    /// goes in; an Untitled project's is the app's, so it asks where.
    let hasFolder: Bool

    init(options: ExportOptions, title: String, range: ExportRange, fromSelection: Bool, barBeats: Double,
         beatUnit: Double, lengthBeats: Double, tempo: Double, timeSignature: String, songRate: Int, hasFolder: Bool) {
        self.options = options
        name = ExportOptions.name(title: title)
        self.barBeats = barBeats
        self.beatUnit = beatUnit
        self.lengthBeats = lengthBeats
        self.tempo = tempo
        self.timeSignature = timeSignature
        self.songRate = songRate
        self.fromSelection = fromSelection
        self.hasFolder = hasFolder
        let start = ExportRange.place(range.start, bar: barBeats, unit: beatUnit)
        let end = ExportRange.place(range.end, bar: barBeats, unit: beatUnit)
        (startBar, startBeat, endBar, endBeat) = (start.bar, start.beat, end.bar, end.beat)
    }

    /// The range the fields say.
    var range: ExportRange {
        ExportRange(
            start: ExportRange.beats(bar: startBar, beat: startBeat, barBeats: barBeats, unit: beatUnit),
            end: ExportRange.beats(bar: endBar, beat: endBeat, barBeats: barBeats, unit: beatUnit)
        )
    }

    /// The beats a bar of the meter counts: 4 in 4/4, 6 in 6/8.
    var beatsInBar: Double { barBeats / beatUnit }

    /// Why Export cannot be pressed, or nil.
    var problem: String? {
        for beat in [startBeat, endBeat] where beat < 1 || beat >= beatsInBar + 1 {
            return "A bar of \(timeSignature) has beats 1 to \(ExportOptions.number(beatsInBar))."
        }
        if startBar < 1 || endBar < 1 { return "Bars count from 1." }
        if name.trimmingCharacters(in: .whitespaces).isEmpty { return "The file needs a name." }
        return range.problem(length: lengthBeats, bar: barBeats, unit: beatUnit)
    }

    /// The range passed to `daw export`, or nil for the whole song.
    var exportedRange: ExportRange? {
        range.isWhole(length: lengthBeats) ? nil : range
    }

    func wholeSong() {
        let end = ExportRange.place(lengthBeats, bar: barBeats, unit: beatUnit)
        (startBar, startBeat, endBar, endBeat) = (1, 1, end.bar, end.beat)
    }

    /// What the range is, in words: the bars it covers and how long it lasts.
    var summary: String {
        let r = range
        guard r.end > r.start else { return "" }
        let seconds = (r.end - r.start) * 60 / tempo
        var what: String
        if r.isWhole(length: lengthBeats) {
            what = "The whole song"
        } else if startBeat == 1, endBeat == 1, endBar > startBar {
            what = endBar - startBar == 1 ? "Bar \(startBar)" : "Bars \(startBar)–\(endBar - 1)"
        } else {
            what = "\(ExportRange.text(r.start, bar: barBeats, unit: beatUnit)) to \(ExportRange.text(r.end, bar: barBeats, unit: beatUnit))"
        }
        if fromSelection && !r.isWhole(length: lengthBeats) { what += ", the selected clips" }
        return "\(what) · \(Self.duration(seconds))"
    }

    /// Seconds as `m:ss`, or `m:ss.s` under a minute where not whole.
    static func duration(_ seconds: Double) -> String {
        let tenths = (seconds * 10).rounded() / 10
        let whole = Int(tenths.rounded(.down))
        let text = String(format: "%d:%02d", whole / 60, whole % 60)
        return seconds < 60 && tenths != tenths.rounded(.down) ? text + String(format: ".%d", Int((tenths * 10).rounded()) % 10) : text
    }

    static func depthName(_ bits: Int) -> String {
        bits == 32 ? "32-bit float" : "\(bits)-bit"
    }

    static func kHz(_ rate: Int) -> String {
        let k = Double(rate) / 1000
        return "\(k == k.rounded() ? String(Int(k)) : String(k)) kHz"
    }

    /// The rates offered: the usual ones and the song's, at most 48 kHz for
    /// AAC and MP3.
    var rates: [Int] {
        let all = Set(ExportOptions.rates + [songRate]).sorted()
        return options.format.compressed ? all.filter { $0 <= ExportOptions.compressedRate } : all
    }

    /// The rate shown: the one chosen, or the song's.
    var rate: Binding<Int> {
        Binding(
            get: { self.options.rate ?? min(self.songRate, self.options.format.compressed ? ExportOptions.compressedRate : self.songRate) },
            set: { self.options.sampleRate = $0 == self.songRate ? nil : $0 }
        )
    }

    var depth: Binding<Int> {
        Binding(get: { self.options.depth ?? 24 }, set: { self.options.bits = $0 })
    }

    var bitrate: Binding<Int> {
        Binding(
            get: { self.options.bitrate ?? (self.options.format == .aac ? 256 : 320) },
            set: { self.options.bitrate = $0 }
        )
    }
}

/// File › Export Audio… (⇧⌘R): a sheet over the song window with the name,
/// the range as start and end bar and beat, the format, the sample rate, the
/// bit depth or bitrate, the level and the ceiling. Export writes into the
/// project's `exports/` folder.
struct ExportSheet: View {
    @Bindable var setup: ExportSetup
    let cancel: () -> Void
    let export: () -> Void

    /// The level policies the menu offers, in order.
    static let policies: [(String, String)] = [
        ("rendered", "As rendered"), ("peak", "True peak at"), ("lufs", "Loudness"), ("gain", "Gain"),
    ]

    private var policy: Binding<String> {
        Binding(
            get: { setup.options.level.policy },
            set: { policy in
                // The value kept when the policy changes between those with
                // one, so a number typed is not lost.
                let value = setup.options.level.value
                let same = policy == setup.options.level.policy
                if let level = ExportOptions.Level.make(policy, same ? value : nil) { setup.options.level = level }
            }
        )
    }

    private var value: Binding<Double> {
        Binding(
            get: { setup.options.level.value ?? 0 },
            set: { value in
                if let level = ExportOptions.Level.make(setup.options.level.policy, value) { setup.options.level = level }
            }
        )
    }

    private static let number: NumberFormatter = {
        let f = NumberFormatter()
        f.numberStyle = .decimal
        f.maximumFractionDigits = 3
        f.minimumFractionDigits = 0
        return f
    }()

    private static let whole: NumberFormatter = {
        let f = NumberFormatter()
        f.numberStyle = .none
        f.allowsFloats = false
        return f
    }()

    private func place(_ label: String, bar: Binding<Int>, beat: Binding<Double>) -> some View {
        HStack(spacing: 3) {
            TextField("Bar", value: bar, formatter: Self.whole)
                .frame(width: 48)
                .multilineTextAlignment(.trailing)
                .accessibilityLabel("\(label) bar")
            Text(".").foregroundStyle(.secondary)
            TextField("Beat", value: beat, formatter: Self.number)
                .frame(width: 40)
                .multilineTextAlignment(.trailing)
                .accessibilityLabel("\(label) beat")
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Export Audio").font(.headline)
            // The app's own `Grid` is the timeline's.
            SwiftUI.Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 10) {
                GridRow {
                    Text("Name:").gridColumnAlignment(.trailing)
                    HStack(spacing: 4) {
                        TextField("Name", text: $setup.name)
                            .frame(width: 260)
                            .accessibilityLabel("Name")
                        Text(".\(setup.options.format.fileExtension)").foregroundStyle(.secondary)
                    }
                }
                GridRow {
                    Text("Start:").gridColumnAlignment(.trailing)
                    HStack(spacing: 14) {
                        place("Start", bar: $setup.startBar, beat: $setup.startBeat)
                        Text("End:")
                        place("End", bar: $setup.endBar, beat: $setup.endBeat)
                        Button("Whole Song") { setup.wholeSong() }
                            .disabled(setup.range.isWhole(length: setup.lengthBeats))
                    }
                }
                GridRow {
                    Color.clear.gridCellUnsizedAxes([.horizontal, .vertical])
                    Text(setup.summary)
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
                Divider().gridCellColumns(2)
                GridRow {
                    Text("Format:").gridColumnAlignment(.trailing)
                    Picker("Format", selection: $setup.options.format) {
                        ForEach(ExportOptions.Format.allCases, id: \.self) { format in
                            Text(format.name).tag(format)
                        }
                    }
                    .labelsHidden()
                    .frame(width: 150)
                    .accessibilityLabel("Format")
                }
                GridRow {
                    Text("Sample Rate:").gridColumnAlignment(.trailing)
                    Picker("Sample Rate", selection: setup.rate) {
                        ForEach(setup.rates, id: \.self) { rate in
                            Text(rate == setup.songRate ? "\(ExportSetup.kHz(rate)) (the song's)" : ExportSetup.kHz(rate)).tag(rate)
                        }
                    }
                    .labelsHidden()
                    .frame(width: 150)
                    .accessibilityLabel("Sample rate")
                }
                GridRow {
                    if setup.options.format.compressed {
                        Text("Bitrate:").gridColumnAlignment(.trailing)
                        Picker("Bitrate", selection: setup.bitrate) {
                            ForEach(ExportOptions.bitrates, id: \.self) { kbps in
                                Text("\(kbps) kb/s").tag(kbps)
                            }
                        }
                        .labelsHidden()
                        .frame(width: 150)
                        .accessibilityLabel("Bitrate")
                    } else {
                        Text("Bit Depth:").gridColumnAlignment(.trailing)
                        Picker("Bit Depth", selection: setup.depth) {
                            ForEach(setup.options.format.depths, id: \.self) { bits in
                                Text(ExportSetup.depthName(bits)).tag(bits)
                            }
                        }
                        .labelsHidden()
                        .frame(width: 150)
                        .accessibilityLabel("Bit depth")
                    }
                }
                GridRow {
                    Text("Level:").gridColumnAlignment(.trailing)
                    HStack(spacing: 6) {
                        Picker("Level", selection: policy) {
                            ForEach(Self.policies, id: \.0) { policy, name in
                                Text(name).tag(policy)
                            }
                        }
                        .labelsHidden()
                        .frame(width: 150)
                        .accessibilityLabel("Level")
                        if setup.options.level != .asRendered {
                            TextField("Value", value: value, formatter: Self.number)
                                .frame(width: 64)
                                .multilineTextAlignment(.trailing)
                                .accessibilityLabel("Level value")
                            Text(setup.options.level.unit).foregroundStyle(.secondary)
                        }
                    }
                }
                if setup.options.level != .asRendered {
                    GridRow {
                        Text("Ceiling:").gridColumnAlignment(.trailing)
                        HStack(spacing: 6) {
                            TextField("Ceiling", value: $setup.options.ceiling, formatter: Self.number)
                                .frame(width: 64)
                                .multilineTextAlignment(.trailing)
                                .accessibilityLabel("Ceiling")
                            Text("dBFS: the highest sample peak the gain may reach; the export does not limit.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
            Text(setup.hasFolder
                 ? "Saved in the project's exports folder. What is heard is exported: muted tracks and deactivated clips are left out."
                 : "This project has no folder of yours yet, so Export asks where to save. What is heard is exported: muted tracks and deactivated clips are left out.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(3)
                .frame(width: 480, alignment: .leading)
            HStack {
                if let problem = setup.problem {
                    Label(problem, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption)
                        .foregroundStyle(.orange)
                        .lineLimit(2)
                }
                Spacer()
                Button("Cancel", action: cancel)
                    .keyboardShortcut(.cancelAction)
                Button(setup.hasFolder ? "Export" : "Export…", action: export)
                    .keyboardShortcut(.defaultAction)
                    .disabled(setup.problem != nil)
            }
        }
        .controlSize(.regular)
        .padding(20)
        .frame(width: 560, alignment: .leading)
    }
}
