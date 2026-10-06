import AAWCore
import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// The detail panel, under the arrangement: the devices of the row last
/// selected, or the clip last selected, which is a pattern clip's pattern, an
/// audio clip's settings or a note clip's piano roll. A row's header shows the first and a clip the
/// second, and the two marks at the top left change between them.
struct DetailView: View {
    let model: SongModel

    /// The panel's height until the person drags its top edge, and its least.
    static let height: CGFloat = SongModel.leastDetailHeight

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 2) {
                    tab("Devices", .devices)
                    tab(Self.clipTitle(model), .pattern)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 10)
                .padding(.top, 6)
                switch model.detail {
                case .devices:
                    if let chain = model.deviceChain {
                        ChainHeader(model: model, chain: chain)
                    } else {
                        hint("Select a track, a return or the master to see its effects.")
                    }
                case .pattern:
                    if let context = model.audioContext {
                        AudioClipHeader(model: model, context: context)
                    } else if let context = model.noteContext {
                        NoteClipHeader(model: model, context: context)
                    } else if let context = model.patternContext {
                        PatternHeader(model: model, context: context)
                    } else {
                        hint("Select a clip to edit it. Double-click an empty part of a track to add a clip, or drop an audio file on it.")
                    }
                }
                Spacer(minLength: 0)
            }
            .frame(width: TimelineLayout.headerWidth - 1)
            Divider()
            switch model.detail {
            case .devices:
                if let chain = model.deviceChain { DeviceView(model: model, chain: chain) }
            case .pattern:
                if let context = model.audioContext {
                    AudioClipInfo(context: context, tempo: model.arrangement.tempo)
                } else if let context = model.noteContext {
                    NotePane(model: model, context: context, color: model.color(of: context.track.key),
                             playing: model.transport.playing, selected: model.selectedNotes, grid: model.noteGrid)
                } else if let context = model.patternContext {
                    PatternPane(model: model, context: context, color: model.color(of: context.track.key),
                                playing: model.transport.playing, selected: model.selectedEvents)
                }
            }
            Spacer(minLength: 0)
        }
        .frame(height: model.detailHeight, alignment: .top)
        .background(Color(nsColor: Theme.gray(0.13)))
    }

    /// What the clip's tab is called: after the kind of clip last selected.
    static func clipTitle(_ model: SongModel) -> String {
        if model.audioContext != nil { return "Audio Clip" }
        if model.noteContext != nil { return "Notes" }
        return "Pattern"
    }

    private func tab(_ title: String, _ detail: Detail) -> some View {
        Button {
            model.detail = detail
        } label: {
            Text(title)
                .font(.system(size: 11, weight: model.detail == detail ? .semibold : .regular))
                .foregroundStyle(model.detail == detail ? Color.primary : Color.secondary)
                .padding(.horizontal, 8)
                .frame(height: 18)
                .background(RoundedRectangle(cornerRadius: 4).fill(Color.white.opacity(model.detail == detail ? 0.12 : 0)))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focusable(false)
    }

    private func hint(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 11))
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 12)
            .padding(.top, 8)
    }
}

/// An audio clip's settings: its level, its fades and their curve, and the
/// tempo it follows the song's from, with how it is stretched. Each is an edit
/// to the host, as an agent's `daw set` is.
private struct AudioClipHeader: View {
    let model: SongModel
    let context: AudioContext

    private var clip: AudioClipView { context.clip }

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

    private func set(_ field: String, _ value: FieldValue) {
        model.edit(.audioSet(clip: clip.key, field: field, value: value))
    }

    /// A number as it is typed: without a fraction where it has none.
    private func text(_ value: Double) -> String {
        value == value.rounded() ? String(Int(value)) : String(value)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            VStack(alignment: .leading, spacing: 1) {
                Text(clip.sample).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                Text("Audio clip on \(context.track.id)").font(.system(size: 10)).foregroundStyle(.secondary).lineLimit(1)
            }
            row("Gain") {
                KnobBar(model: model, spec: BarSpec(value: clip.gainDb, min: -36, max: 24, unit: "dB", initial: 0, live: false)) { [key = clip.key] value in
                    .audioSet(clip: key, field: "gain_db", value: .number(value: (value * 10).rounded() / 10))
                }
            }
            row("Fade in") {
                TypedValue(text: text(clip.fadeInMs), unit: "ms") { typed in
                    if let ms = Double(typed) { model.edit(.audioFade(clip: clip.key, fadeInMs: ms, fadeOutMs: nil)) }
                } done: {
                    model.onFocus?()
                }
            }
            row("Fade out") {
                TypedValue(text: text(clip.fadeOutMs), unit: "ms") { typed in
                    if let ms = Double(typed) { model.edit(.audioFade(clip: clip.key, fadeInMs: nil, fadeOutMs: ms)) }
                } done: {
                    model.onFocus?()
                }
            }
            row("Curve") {
                Picker("", selection: Binding(get: { clip.fadeCurve }, set: { set("fade_curve", .text(value: $0)) })) {
                    Text("Equal power").tag("equal_power")
                    Text("Linear").tag("linear")
                }
                .labelsHidden()
                .controlSize(.mini)
                .help("Equal power keeps the level across two different sounds; linear across the same one")
            }
            row("Tempo") {
                // Empty, the clip plays at its file's own tempo; with one, it follows the song's.
                TypedValue(text: clip.sourceBpm.map(text) ?? "", unit: "BPM", clears: true) { typed in
                    if typed.isEmpty {
                        set("source_bpm", .absent)
                    } else if let bpm = Double(typed) {
                        set("source_bpm", .number(value: bpm))
                    }
                } done: {
                    model.onFocus?()
                }
                .help("The file's tempo. With one, the clip follows the song's tempo; empty, it plays as it is")
            }
            row("Stretch") {
                Picker("", selection: Binding(get: { clip.stretch }, set: { set("stretch", .text(value: $0)) })) {
                    Text("Repitch").tag("repitch")
                    Text("Keep pitch").tag("preserve_pitch")
                }
                .labelsHidden()
                .controlSize(.mini)
                .disabled(clip.sourceBpm == nil)
                .help("How the clip follows the song's tempo: faster and higher, or at its own pitch")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
    }
}

/// What an audio clip plays, in words: the part of its file, and the file's
/// beat map when it has one.
private struct AudioClipInfo: View {
    let context: AudioContext
    let tempo: Double

    /// Seconds as minutes and seconds to a hundredth, as `daw` takes them.
    static func time(_ seconds: Double) -> String {
        let s = max(0, seconds)
        return String(format: "%d:%05.2f", Int(s / 60), s.truncatingRemainder(dividingBy: 60))
    }

    private var lines: [String] {
        let clip = context.clip
        var lines = ["Plays \(Self.time(clip.sourceStartSeconds)) to \(Self.time(clip.sourceEndSeconds)) of its file, from beat \(ValueScale.plain(clip.at))."]
        guard let file = context.file else {
            return lines + ["Its file cannot be read."]
        }
        lines.append("The file is \(Self.time(file.seconds)) long.")
        if let bpm = file.bpm {
            lines.append("Its beat map has \(file.beats.count) beats at \(String(format: "%.2f", bpm)) BPM; the ticks under the waveform are its beats.")
            if clip.sourceBpm == nil, abs(bpm - tempo) > 0.005 {
                lines.append("The song is at \(ValueScale.plain(tempo)) BPM, so the file's beats leave the grid. Set the song's tempo to the file's, or give the clip its tempo to follow the song's.")
            }
        } else {
            lines.append("It has no beat map: `daw samples beats` measures one, and its beats then show under the waveform.")
        }
        return lines
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            ForEach(lines, id: \.self) { line in
                Text(line).font(.system(size: 11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Text("Drag the clip to move it, an edge to trim it and a corner's handle to fade it. ⌘E splits it at the start position.")
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 2)
        }
        .frame(maxWidth: 460, alignment: .leading)
        .padding(12)
    }
}

/// A row's effect chain, each effect a panel of controls drawn from its
/// fields as the host describes them. No panel is made by hand for an effect
/// type. A control's change is an edit to the host, as an agent's `daw set` is.
struct DeviceView: View {
    let model: SongModel
    let chain: DeviceChain

    var body: some View {
        ScrollView(.horizontal) {
            HStack(alignment: .top, spacing: 8) {
                if let track = chain.track, track.midi {
                    InstrumentPanel(model: model, track: track)
                }
                ForEach(Array(chain.effects.enumerated()), id: \.element.key) { index, effect in
                    DeviceInsertion(model: model, row: chain.row, index: UInt32(index))
                    DevicePanel(model: model, chain: chain, effect: effect, index: index)
                }
                DeviceInsertion(model: model, row: chain.row, index: UInt32(chain.effects.count))
                if chain.effects.isEmpty {
                    Text("No effects on \(chain.name). Drag an effect here, or use Add Effect.")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .frame(width: 320, alignment: .leading)
                        .padding(.top, 4)
                }
            }
            .padding(8)
        }
        .onDrop(of: (DeviceChain.kinds + (chain.track?.midi == true ? Browser.instruments : [])).map { Browser.deviceType + "." + $0 }
                    + (chain.track?.midi == true ? [Browser.patchType] : []), isTargeted: nil) { providers in
            guard let provider = providers.first else { return false }
            if provider.hasItemConformingToTypeIdentifier(Browser.patchType) {
                // A Synth patch: loaded into the track's Synth, or a Synth attached with it.
                guard model.canAddBrowserDevice("synth", to: chain.row) else { return false }
                let row = chain.row
                provider.loadDataRepresentation(forTypeIdentifier: Browser.patchType) { data, _ in
                    guard let data, let name = String(data: data, encoding: .utf8) else { return }
                    DispatchQueue.main.async { model.addBrowserDevice("synth", to: row, patch: name) }
                }
                return true
            }
            guard let kind = (DeviceChain.kinds + Browser.instruments).first(where: { provider.hasItemConformingToTypeIdentifier(Browser.deviceType + "." + $0) }),
                  model.canAddBrowserDevice(kind, to: chain.row) else { return false }
            model.addBrowserDevice(kind, to: chain.row)
            return true
        }
    }
}

private struct DeviceInsertion: View {
    let model: SongModel
    let row: RowID
    let index: UInt32
    @State private var targeted = false

    var body: some View {
        RoundedRectangle(cornerRadius: 2)
            .fill(targeted ? Color.accentColor : Color.secondary.opacity(0.18))
            .frame(width: 12, height: model.detailHeight - 54)
            .onDrop(of: DeviceChain.kinds.map { Browser.deviceType + "." + $0 }, isTargeted: $targeted) { providers in
                guard let provider = providers.first,
                      let kind = DeviceChain.kinds.first(where: { provider.hasItemConformingToTypeIdentifier(Browser.deviceType + "." + $0) }) else { return false }
                model.addBrowserDevice(kind, to: row, index: index)
                return true
            }
            .help("Drop an effect here")
    }
}

/// An effect type or a choice as a person reads it: `low_shelf` is "Low shelf".
func readable(_ name: String) -> String {
    let words = name.replacingOccurrences(of: "_", with: " ")
    return words.prefix(1).uppercased() + words.dropFirst()
}

/// The row's name, the menu that adds an effect, and a track's pads.
private struct ChainHeader: View {
    let model: SongModel
    let chain: DeviceChain

    private var kind: String {
        switch chain.row {
        case .track: chain.track?.midi == true ? "MIDI track" : "Track"
        case .bus: "Return"
        case .master: "Master"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            VStack(alignment: .leading, spacing: 1) {
                Text(chain.name).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                Text(chain.row == .master ? "Effects" : "\(kind) effects").font(.system(size: 10)).foregroundStyle(.secondary)
            }
            Menu {
                ForEach(DeviceChain.kinds, id: \.self) { kind in
                    Button(readable(kind)) {
                        model.edit(.effectAdd(row: chain.row.row, kind: kind, index: nil))
                    }
                }
            } label: {
                Label("Add Effect", systemImage: "plus").font(.system(size: 11))
            }
            .menuStyle(.borderlessButton)
            .controlSize(.small)
            .fixedSize()
            if !chain.pads.isEmpty, chain.track?.midi != true {
                Divider()
                Text("Pads").font(.system(size: 10, weight: .semibold)).foregroundStyle(.secondary)
                ScrollView {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(chain.pads, id: \.name) { pad in
                            HStack(spacing: 4) {
                                Text(pad.name).font(.system(size: 10, weight: .medium))
                                Text(pad.sample).font(.system(size: 10)).foregroundStyle(.secondary).lineLimit(1)
                                Spacer(minLength: 0)
                                if pad.gate {
                                    Text("gate").font(.system(size: 9)).foregroundStyle(.tertiary)
                                }
                            }
                        }
                    }
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
    }
}

/// A MIDI track's instrument, first in its chain, and the mark that takes it
/// off. A sampler that is empty or one pad on every note is the Sampler
/// device, drawn by `SamplerPanel`; the Synth is drawn by `SynthPanel`; a
/// sampler of several pads lists them and the notes that play each. The
/// notes are kept whatever is done here. A sample dropped here, or on the
/// track's header, becomes the instrument's, in place of the one it had.
private struct InstrumentPanel: View {
    let model: SongModel
    let track: TrackView
    @State private var targeted = false

    /// Notes as a range of names: `C-1–G9`, or one note, `C2`.
    private func notes(_ m: NoteMapView) -> String {
        m.low == m.high ? noteName(midi: m.low) : "\(noteName(midi: m.low))–\(noteName(midi: m.high))"
    }

    private var title: String {
        if track.sampler != nil { return "Sampler" }
        if let synth = track.synth { return synth.patch.map { "Synth · \($0)" } ?? "Synth" }
        return track.instrument.map(readable) ?? "No instrument"
    }

    private var width: CGFloat {
        if track.sampler != nil { return SamplerPanel.width }
        if let synth = track.synth { return SynthPanel.width(synth) }
        return 216
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let synth = track.synth {
                SynthHeader(model: model, track: track, synth: synth)
            } else {
                HStack(spacing: 5) {
                    Image(systemName: "pianokeys").foregroundStyle(.secondary)
                    Text(title).font(.system(size: 11, weight: .semibold)).lineLimit(1)
                    Spacer(minLength: 2)
                    if track.instrument != nil {
                        Button {
                            model.edit(.instrumentRemove(track: track.key))
                        } label: {
                            Image(systemName: "xmark")
                        }
                        .help("Take the instrument off; the notes are kept")
                    }
                }
                .buttonStyle(.borderless)
                .font(.system(size: 10))
                .padding(.horizontal, 7)
                .frame(height: 24)
                .background(Color(nsColor: Theme.gray(0.24)))
            }
            if let sampler = track.sampler {
                SamplerPanel(model: model, track: track, sampler: sampler)
            } else if let synth = track.synth {
                SynthPanel(model: model, track: track, synth: synth, height: model.detailHeight - 16 - 24)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 3) {
                        if track.instrument == nil {
                            Text("The notes play nothing. Drop a sample here or on \(track.id)'s header, or add one with + in the samples, and a Sampler plays it on every note. Ask the agent for a drum kit.")
                                .font(.system(size: 10))
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        ForEach(Array(track.map.enumerated()), id: \.offset) { _, m in
                            HStack(spacing: 4) {
                                Text(notes(m)).font(.system(size: 10).monospacedDigit()).frame(width: 64, alignment: .leading)
                                Text(m.pad).font(.system(size: 10, weight: .medium)).lineLimit(1)
                                Spacer(minLength: 0)
                                Text(m.pitched ? "pitched" : "as it is").font(.system(size: 9)).foregroundStyle(.tertiary)
                            }
                            .help(track.pads.first { $0.name == m.pad }.map { "Pad \(m.pad) plays sample \($0.sample)" } ?? m.pad)
                        }
                        if track.instrument != nil, !track.pads.isEmpty, track.map.isEmpty {
                            Text("No note plays a pad yet: `daw instrument map` gives the pads notes.")
                                .font(.system(size: 10))
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    .padding(6)
                }
            }
            Spacer(minLength: 0)
        }
        .frame(width: width, height: model.detailHeight - 16, alignment: .top)
        .background(Color(nsColor: Theme.gray(0.19)))
        .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(Color.accentColor, lineWidth: targeted ? 2 : 0))
        .clipShape(RoundedRectangle(cornerRadius: 4))
        .onDrop(of: SamplerDrop.types, isTargeted: $targeted) { providers in
            SamplerDrop.land(providers, model: model, track: track)
        }
    }
}

/// One effect: its name, the marks that bypass, move and remove it, and a
/// control for each of its fields.
private struct DevicePanel: View {
    let model: SongModel
    let chain: DeviceChain
    let effect: EffectView
    let index: Int

    private var title: String {
        let kind = readable(effect.kind)
        return effect.id.map { "\(kind) · \($0)" } ?? kind
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 5) {
                Button {
                    model.edit(.effectBypass(effect: effect.key, on: !effect.bypass))
                } label: {
                    Image(systemName: "power").foregroundStyle(effect.bypass ? Color.secondary : Color.green)
                }
                .help(effect.bypass ? "Bypassed: the effect does not process" : "Bypass")
                Text(title).font(.system(size: 11, weight: .semibold)).lineLimit(1)
                Spacer(minLength: 2)
                if effect.kind == "eq" {
                    Button {
                        model.edit(.bandAdd(effect: effect.key))
                    } label: {
                        Image(systemName: "plus")
                    }
                    .help("Add a band")
                    .disabled(effect.bands >= 16)
                }
                Button {
                    model.edit(.effectMove(effect: effect.key, index: UInt32(index - 1)))
                } label: {
                    Image(systemName: "chevron.left")
                }
                .help("Move earlier in the chain")
                .disabled(index == 0)
                Button {
                    model.edit(.effectMove(effect: effect.key, index: UInt32(index + 1)))
                } label: {
                    Image(systemName: "chevron.right")
                }
                .help("Move later in the chain")
                .disabled(index == chain.effects.count - 1)
                Button {
                    model.edit(.effectRemove(effect: effect.key))
                } label: {
                    Image(systemName: "xmark")
                }
                .help("Remove the effect, with the lanes that automate it")
            }
            .buttonStyle(.borderless)
            .font(.system(size: 10))
            .padding(.horizontal, 7)
            .frame(height: 24)
            .background(Color(nsColor: Theme.gray(0.24)))

            Group {
                if effect.kind == "eq" {
                    ScrollView {
                        VStack(spacing: 3) {
                            ForEach(0..<Int(effect.bands), id: \.self) { band in
                                BandRow(model: model, row: chain.row, effect: effect, band: band)
                            }
                        }
                        .padding(6)
                    }
                } else {
                    VStack(spacing: 3) {
                        ForEach(effect.fields, id: \.name) { field in
                            HStack(spacing: 4) {
                                Text(field.label)
                                    .font(.system(size: 10))
                                    .foregroundStyle(.secondary)
                                    .frame(width: 62, alignment: .leading)
                                FieldControl(model: model, effect: effect.key, field: field)
                                LaneMark(model: model, row: chain.row, field: field)
                            }
                            .frame(height: 19)
                        }
                    }
                    .padding(6)
                }
            }
            .opacity(effect.bypass ? 0.5 : 1)
            Spacer(minLength: 0)
        }
        .frame(width: effect.kind == "eq" ? 376 : 216, height: model.detailHeight - 16, alignment: .top)
        .background(Color(nsColor: Theme.gray(0.19)))
        .clipShape(RoundedRectangle(cornerRadius: 4))
    }
}

/// An equalizer's band: its shape, frequency, gain and q on one line.
private struct BandRow: View {
    let model: SongModel
    let row: RowID
    let effect: EffectView
    let band: Int

    private func field(_ name: String) -> FieldView? {
        effect.fields.first { $0.name == "bands.\(band).\(name)" }
    }

    var body: some View {
        HStack(spacing: 3) {
            ForEach(["shape", "freq_hz", "gain_db", "q"], id: \.self) { name in
                if let field = field(name) {
                    FieldControl(model: model, effect: effect.key, field: field)
                        .frame(width: name == "shape" ? 88 : name == "q" ? 50 : 76)
                        .help(field.label)
                    if name != "shape" { LaneMark(model: model, row: row, field: field) }
                }
            }
            Button {
                model.edit(.bandRemove(effect: effect.key, band: UInt32(band)))
            } label: {
                Image(systemName: "minus.circle")
            }
            .buttonStyle(.borderless)
            .font(.system(size: 10))
            .help("Remove the band")
            .disabled(effect.bands <= 1)
        }
        .frame(height: 19)
    }
}

/// The mark beside a field that automation can move: filled while a lane
/// moves it. A click adds the lane, or removes it.
private struct LaneMark: View {
    let model: SongModel
    let row: RowID
    let field: FieldView

    var body: some View {
        if let param = field.param {
            Button {
                if let lane = field.lane {
                    model.edit(.laneRemove(lane: lane))
                } else {
                    model.edit(.laneAdd(row: row.row, param: param))
                    model.onShowLanes?(row)
                }
            } label: {
                Image(systemName: field.lane == nil ? "diamond" : "diamond.fill")
                    .font(.system(size: 8))
                    .foregroundStyle(field.lane == nil ? Color.secondary : Color(nsColor: Theme.automation))
            }
            .buttonStyle(.borderless)
            .frame(width: 12)
            .help(field.lane == nil ? "Automate \(field.label): adds a lane under the row" : "Remove the lane that automates \(field.label)")
        } else {
            Color.clear.frame(width: 12, height: 1)
        }
    }
}

/// The control a field's kind calls for. `edit` is the edit that sets the
/// field to a value: an effect's, or a Sampler's pad's.
struct FieldControl: View {
    let model: SongModel
    let field: FieldView
    let edit: (FieldValue) -> Edit

    /// A control for a field of an effect.
    init(model: SongModel, effect: UInt64, field: FieldView) {
        self.init(model: model, field: field) { .effectSet(effect: effect, field: field.name, value: $0) }
    }

    init(model: SongModel, field: FieldView, edit: @escaping (FieldValue) -> Edit) {
        self.model = model
        self.field = field
        self.edit = edit
    }

    private func set(_ value: FieldValue) {
        model.edit(edit(value))
    }

    private var text: String {
        switch field.value {
        case .text(let value): value
        case .number(let value): value == value.rounded() ? String(Int(value)) : String(value)
        case .flag, .absent: ""
        }
    }

    var body: some View {
        switch field.kind {
        case .number:
            HStack(spacing: 3) {
                if field.optional {
                    Toggle("", isOn: Binding(
                        get: { field.value != .absent },
                        set: { set($0 ? field.initial : .absent) }
                    ))
                    .toggleStyle(.checkbox)
                    .controlSize(.mini)
                    .labelsHidden()
                    .help(field.value == .absent ? "Off" : "On")
                }
                KnobBar(model: model, spec: BarSpec(field)) { [edit] value in
                    edit(.number(value: value))
                }
            }
        case .integer where !field.choices.isEmpty, .choice:
            Picker("", selection: Binding(
                get: { text },
                set: { choice in set(field.kind == .integer ? .number(value: Double(choice) ?? 0) : .text(value: choice)) }
            )) {
                ForEach(field.choices, id: \.self) { choice in
                    Text(field.kind == .integer ? "\(choice) \(field.unit)" : readable(choice)).tag(choice)
                }
            }
            .labelsHidden()
            .controlSize(.mini)
        case .track:
            Picker("", selection: Binding(
                get: { text },
                set: { set($0.isEmpty ? .absent : .text(value: $0)) }
            )) {
                Text("None").tag("")
                ForEach(field.choices, id: \.self) { Text($0).tag($0) }
            }
            .labelsHidden()
            .controlSize(.mini)
            .disabled(field.choices.isEmpty)
        case .flag:
            HStack {
                Toggle("", isOn: Binding(
                    get: { field.value == .flag(value: true) },
                    set: { set(.flag(value: $0)) }
                ))
                .toggleStyle(.checkbox)
                .controlSize(.mini)
                .labelsHidden()
                Spacer(minLength: 0)
            }
        case .beats, .integer:
            TypedValue(text: text, unit: field.unit) { typed in
                if field.kind == .beats {
                    set(.text(value: typed))
                } else if let number = Double(typed) {
                    set(.number(value: number.rounded()))
                }
            } done: {
                model.onFocus?()
            }
        }
    }
}

/// A value that is typed: a beat such as `3/4`, a note or a whole number.
struct TypedValue: View {
    let text: String
    let unit: String
    /// Whether an empty value is kept, as one that takes a field away.
    var clears = false
    let commit: (String) -> Void
    let done: () -> Void

    @State private var typed = ""
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 3) {
            TextField("", text: $typed)
                .textFieldStyle(.plain)
                .font(.system(size: 10).monospacedDigit())
                .focused($focused)
                .onSubmit {
                    let value = typed.trimmingCharacters(in: .whitespaces)
                    if clears || !value.isEmpty, value != text { commit(value) }
                    typed = text
                    focused = false
                    done()
                }
                .padding(.horizontal, 5)
                .frame(height: 16)
                .background(RoundedRectangle(cornerRadius: 3).fill(Color(nsColor: Theme.control)))
            if !unit.isEmpty {
                Text(unit).font(.system(size: 9)).foregroundStyle(.tertiary)
            }
        }
        .onAppear { typed = text }
        .onChange(of: text) { typed = text }
    }
}

/// What a bar shows and how it is dragged: a number in a range.
struct BarSpec: Equatable {
    /// Nil for an optional number that is off.
    var value: Double?
    var min: Double
    var max: Double
    /// Whether the value moves in equal ratios, as a frequency does.
    var log = false
    var unit = ""
    /// What a double-click puts back, and where an off value starts.
    var initial: Double?
    /// Whether the song glides to each value of a drag; otherwise the value
    /// is sent when the drag ends.
    var live = true
    /// Dimmed, as a knob that a lane moves is.
    var dimmed = false
    /// Whole numbers only.
    var whole = false
    /// The text for a value, where the unit's usual one does not do.
    var text: String?
}

extension BarSpec {
    /// A numeric field of an effect.
    init(_ field: FieldView) {
        var value: Double?
        if case .number(let x) = field.value { value = x }
        var initial: Double?
        if case .number(let x) = field.initial { initial = x }
        self.init(value: value, min: field.min, max: field.max, log: field.log, unit: field.unit, initial: initial,
                  live: field.live, dimmed: field.lane != nil)
    }
}

/// A number as a bar to drag, in SwiftUI. `edit` is the edit that sets it.
struct KnobBar: NSViewRepresentable {
    let model: SongModel
    let spec: BarSpec
    let edit: (Double) -> Edit

    func makeNSView(context: Context) -> KnobBarView {
        KnobBarView(model: model, spec: spec, edit: edit)
    }

    func updateNSView(_ view: KnobBarView, context: Context) {
        view.edit = edit
        view.spec = spec
    }
}

/// A number as a bar filled to its place in its range, with its value. A drag
/// sideways changes it, ten times finer with Shift; a double-click puts it
/// back to its default. A value the song glides to is heard as it moves, each
/// change an edit of one gesture; any other is sent when the drag ends, since
/// each change would be a fade through silence.
final class KnobBarView: NSView {
    private let model: SongModel
    var edit: (Double) -> Edit
    var spec: BarSpec {
        didSet {
            // The host's value has arrived.
            if let value = spec.value, let shown, abs(value - shown) < 1e-9, drag == nil {
                self.shown = nil
            }
            needsDisplay = true
        }
    }

    /// Points of drag that cross the whole range.
    static let travel: CGFloat = 160

    private struct Drag {
        var x: CGFloat
        var fraction: Double
        var gesture: String
        var moved = false
    }

    private var drag: Drag?
    /// The value shown ahead of the host.
    private var shown: Double?
    private var release = 0

    init(model: SongModel, spec: BarSpec, edit: @escaping (Double) -> Edit) {
        self.model = model
        self.spec = spec
        self.edit = edit
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override var intrinsicContentSize: NSSize { NSSize(width: NSView.noIntrinsicMetric, height: 16) }

    private var scale: ValueScale {
        ValueScale(min: spec.min, max: spec.max, log: spec.log)
    }

    /// The value in effect: the dragged one or the song's; nil for a number
    /// that is off.
    private var value: Double? {
        shown ?? spec.value
    }

    private var start: Double {
        value ?? spec.initial ?? spec.min
    }

    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 2, let initial = spec.initial {
            drag = nil
            if value != initial {
                shown = initial
                hold()
                model.edit(edit(initial))
            }
            needsDisplay = true
            return
        }
        let x = convert(event.locationInWindow, from: nil).x
        drag = Drag(x: x, fraction: scale.fraction(start), gesture: model.newGesture())
    }

    override func mouseDragged(with event: NSEvent) {
        guard var d = drag else { return }
        let x = convert(event.locationInWindow, from: nil).x
        let fine = event.modifierFlags.contains(.shift)
        d.fraction = min(max(d.fraction + Double((x - d.x) / Self.travel) * (fine ? 0.1 : 1), 0), 1)
        d.x = x
        var value = scale.value(at: d.fraction)
        if spec.whole { value = value.rounded() }
        if value != self.value {
            d.moved = true
            shown = value
            if spec.live { model.drag(edit(value), gesture: d.gesture) }
            needsDisplay = true
        }
        drag = d
    }

    override func mouseUp(with event: NSEvent) {
        guard let d = drag else { return }
        drag = nil
        guard d.moved, let shown else { return }
        if spec.live {
            model.endDrag()
        } else {
            model.edit(edit(shown))
        }
        hold()
    }

    /// Keeps the value as set until the host's song has it, or a second passes,
    /// as when the host refuses it.
    private func hold() {
        release += 1
        let mine = release
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            MainActor.assumeIsolated {
                guard let self, self.release == mine, self.drag == nil else { return }
                self.shown = nil
                self.needsDisplay = true
            }
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        let box = bounds.insetBy(dx: 0, dy: (bounds.height - 16) / 2)
        let shape = NSBezierPath(roundedRect: box, xRadius: 3, yRadius: 3)
        Theme.control.setFill()
        shape.fill()
        let font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
        let style = NSMutableParagraphStyle()
        style.alignment = .center
        guard let value else {
            ("off" as NSString).draw(in: box.insetBy(dx: 3, dy: 1.5), withAttributes: [
                .font: font, .foregroundColor: Theme.faintText, .paragraphStyle: style,
            ])
            return
        }
        NSGraphicsContext.saveGraphicsState()
        shape.addClip()
        Theme.knob.withAlphaComponent(spec.dimmed ? 0.45 : 1).setFill()
        CGRect(x: box.minX, y: box.minY, width: box.width * CGFloat(scale.fraction(value)), height: box.height).fill()
        NSGraphicsContext.restoreGraphicsState()
        // A shown value is the dragged one, so its own text is out of date.
        let text = (shown == nil ? spec.text : nil)
            ?? (spec.whole ? String(Int(value)) : ValueScale.text(value, unit: spec.unit, signed: spec.min < 0))
        (text as NSString).draw(in: box.insetBy(dx: 3, dy: 1.5), withAttributes: [
            .font: font, .foregroundColor: Theme.text, .paragraphStyle: style,
        ])
    }
}
