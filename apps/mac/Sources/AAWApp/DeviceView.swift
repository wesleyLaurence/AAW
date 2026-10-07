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

/// The strip between two devices: an effect from the browser dropped here
/// goes in at this place, and so does a copy of an effect dragged by its
/// title with Option held.
private struct DeviceInsertion: View {
    let model: SongModel
    let row: RowID
    let index: UInt32
    @State private var targeted = false

    var body: some View {
        RoundedRectangle(cornerRadius: 2)
            .fill(targeted ? Color.accentColor : Color.secondary.opacity(0.18))
            .frame(width: 12, height: model.detailHeight - 54)
            .onDrop(of: DeviceChain.kinds.map { Browser.deviceType + "." + $0 } + [DeviceChain.effectType],
                    delegate: InsertionDrop(model: model, row: row, index: index, targeted: $targeted))
            .help("Drop an effect here; with Option, a copy of one dragged from its title")
    }
}

/// What lands on an insertion strip: a kind from the browser, or with
/// Option a copy of an effect dragged from the device panel. Without Option
/// that drag is refused, so that the strip does not light up for a move
/// nothing makes.
private struct InsertionDrop: DropDelegate {
    let model: SongModel
    let row: RowID
    let index: UInt32
    @Binding var targeted: Bool

    private func kind(of info: DropInfo) -> String? {
        DeviceChain.kinds.first { info.hasItemsConforming(to: [Browser.deviceType + "." + $0]) }
    }

    private var copying: Bool {
        NSEvent.modifierFlags.contains(.option)
    }

    private func accepts(_ info: DropInfo) -> Bool {
        kind(of: info) != nil || (info.hasItemsConforming(to: [DeviceChain.effectType]) && copying)
    }

    func validateDrop(info: DropInfo) -> Bool {
        kind(of: info) != nil || info.hasItemsConforming(to: [DeviceChain.effectType])
    }

    func dropEntered(info: DropInfo) {
        targeted = accepts(info)
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        let ok = accepts(info)
        if targeted != ok { targeted = ok }
        return DropProposal(operation: ok ? .copy : .cancel)
    }

    func dropExited(info: DropInfo) {
        targeted = false
    }

    func performDrop(info: DropInfo) -> Bool {
        targeted = false
        if let kind = kind(of: info) {
            model.addBrowserDevice(kind, to: row, index: index)
            return true
        }
        guard copying, let provider = info.itemProviders(for: [DeviceChain.effectType]).first else { return false }
        let (model, row, index) = (model, row, index)
        provider.loadDataRepresentation(forTypeIdentifier: DeviceChain.effectType) { data, _ in
            guard let data, let key = String(data: data, encoding: .utf8).flatMap(UInt64.init) else { return }
            DispatchQueue.main.async { model.copyEffect(key, to: row, index: index) }
        }
        return true
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
        case .group: "Group"
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
/// control for each of its fields. A click on its title selects it, for
/// Copy, Duplicate and Delete; a drag from the title with Option copies it
/// onto a header or an insertion strip; a right click offers the same.
private struct DevicePanel: View {
    let model: SongModel
    let chain: DeviceChain
    let effect: EffectView
    let index: Int

    private var title: String {
        let kind = readable(effect.kind)
        return effect.id.map { "\(kind) · \($0)" } ?? kind
    }

    private var selected: Bool {
        model.selectedEffect == effect.key
    }

    /// Does what an item of the effect's menu says, to this effect.
    private func act(_ action: ContextMenu.EffectAction) {
        model.select(effect: effect.key)
        switch action {
        case .cut: model.cutSelection(in: .clips)
        case .copy: model.copySelection(in: .clips)
        case .paste: model.paste(in: .clips)
        case .duplicate: model.duplicateSelection()
        case .bypass: model.edit(.effectBypass(effect: effect.key, on: !effect.bypass))
        case .delete: model.deleteSelection()
        }
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
                        model.edit(.bandAdd(effect: effect.key, freqHz: nil, gainDb: nil))
                    } label: {
                        Image(systemName: "plus")
                    }
                    .help("Add a band; a double-click on the curve adds one where it is")
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
            .background(Color(nsColor: selected ? Theme.gray(0.3) : Theme.gray(0.24)))
            .contentShape(Rectangle())
            .onTapGesture { model.select(effect: effect.key) }
            .onDrag {
                let provider = NSItemProvider()
                let key = effect.key
                provider.registerDataRepresentation(forTypeIdentifier: DeviceChain.effectType, visibility: .ownProcess) { completion in
                    completion(Data(String(key).utf8), nil)
                    return nil
                }
                return provider
            }
            .contextMenu {
                ForEach(Array(ContextMenu.effect(bypassed: effect.bypass, copied: model.copiedEffectKind.map(readable)).enumerated()), id: \.offset) { _, item in
                    if let action = item.action {
                        Button(item.title) { act(action) }.disabled(!item.enabled)
                    } else {
                        Divider()
                    }
                }
            }
            .help("Click to select the effect; drag with Option to copy it onto a header or between devices")

            Group {
                if effect.kind == "eq" {
                    EqPanel(model: model, chain: chain, effect: effect)
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
        .frame(width: effect.kind == "eq" ? EqPanel.width : 216, height: model.detailHeight - 16, alignment: .top)
        .background(Color(nsColor: Theme.gray(0.19)))
        .clipShape(RoundedRectangle(cornerRadius: 4))
        .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(Color.accentColor, lineWidth: selected ? 2 : 0))
    }
}

/// The parametric equalizer: its bands as one curve over the playing
/// spectrum, each a point dragged for its frequency and gain, and under the
/// curve the selected band's fields, a bell's or a shelf's gain and q or a
/// pass's slope and resonance, with the marks that automate them.
private struct EqPanel: View {
    let model: SongModel
    let chain: DeviceChain
    let effect: EffectView
    @State private var selected = 0

    static let width: CGFloat = 400

    private func field(_ band: Int, _ name: String) -> FieldView? {
        effect.fields.first { $0.name == "bands.\(band).\(name)" }
    }

    var body: some View {
        let count = Int(effect.bands)
        let band = min(selected, max(count - 1, 0))
        let bands = EqLayout.Band.bands(of: effect.fields)
        let pass = bands.indices.contains(band) && bands[band].isPass
        VStack(spacing: 4) {
            EqCurve(model: model, effect: effect, sampleRate: Double(model.arrangement.sampleRate), selected: band) { selected = $0 }
                .frame(maxHeight: .infinity)
            HStack(spacing: 3) {
                ForEach(0..<count, id: \.self) { i in
                    Button("\(i + 1)") { selected = i }
                        .buttonStyle(.plain)
                        .font(.system(size: 9, weight: i == band ? .bold : .regular).monospacedDigit())
                        .foregroundStyle(i == band ? Color.primary : Color.secondary)
                        .frame(width: 16, height: 14)
                        .background(RoundedRectangle(cornerRadius: 3).fill(Color.white.opacity(i == band ? 0.16 : 0.05)))
                        .help("Band \(i + 1)")
                }
                Spacer(minLength: 0)
                Button {
                    model.edit(.bandRemove(effect: effect.key, band: UInt32(band)))
                } label: {
                    Image(systemName: "minus.circle")
                }
                .buttonStyle(.borderless)
                .font(.system(size: 10))
                .help("Remove band \(band + 1); a double-click on its point does too")
                .disabled(count <= 1)
            }
            .frame(height: 16)
            if let shape = field(band, "shape"), let freq = field(band, "freq_hz"), let gain = field(band, "gain_db"),
               let q = field(band, "q"), let slope = field(band, "slope_db_per_octave") {
                HStack(spacing: 3) {
                    FieldControl(model: model, effect: effect.key, field: shape).frame(width: 84).help("Shape")
                    FieldControl(model: model, effect: effect.key, field: freq).frame(width: 70).help("Frequency")
                    LaneMark(model: model, row: chain.row, field: freq)
                    if pass {
                        FieldControl(model: model, effect: effect.key, field: slope).frame(width: 70).help("Slope")
                        Color.clear.frame(width: 12, height: 1)
                    } else {
                        FieldControl(model: model, effect: effect.key, field: gain).frame(width: 70).help("Gain")
                        LaneMark(model: model, row: chain.row, field: gain)
                    }
                    FieldControl(model: model, effect: effect.key, field: q).frame(width: 50)
                        .help(pass ? "Resonance at the corner: 0.71 is flat" : "Q: how narrow the band is")
                    LaneMark(model: model, row: chain.row, field: q)
                }
                .frame(height: 19)
            }
        }
        .padding(6)
    }
}

/// The equalizer's curve in SwiftUI.
private struct EqCurve: NSViewRepresentable {
    let model: SongModel
    let effect: EffectView
    let sampleRate: Double
    let selected: Int
    let onSelect: (Int) -> Void

    func makeNSView(context: Context) -> EqCurveView {
        EqCurveView(model: model, effect: effect.key, fields: effect.fields, sampleRate: sampleRate, selected: selected, onSelect: onSelect)
    }

    func updateNSView(_ view: EqCurveView, context: Context) {
        view.effect = effect.key
        view.sampleRate = sampleRate
        view.selected = selected
        view.onSelect = onSelect
        view.fields = effect.fields
    }
}

/// The bands' response together over 20 Hz to 20 kHz, with a point for
/// each band, over the spectrum of what the equalizer puts out while the
/// song plays. A press on a point selects its band and takes hold of it:
/// dragging across moves the frequency and up and down the gain, or for a
/// pass the resonance at its corner; with Option, up and down narrow and
/// widen the band instead. Each is heard as it moves, as one undo step. A
/// double-click in the clear adds a bell there, and on a point removes its
/// band. The spectrum is read from the audio thread each frame drawn and
/// falls away once nothing plays.
final class EqCurveView: NSView {
    private let model: SongModel
    var effect: UInt64
    var sampleRate: Double
    var selected: Int {
        didSet { if selected != oldValue { needsDisplay = true } }
    }

    var onSelect: (Int) -> Void
    var fields: [FieldView] {
        didSet {
            let bands = EqLayout.Band.bands(of: fields)
            if let shown, drag == nil, bands.indices.contains(shown.band), bands[shown.band] == shown.value {
                self.shown = nil
            }
            needsDisplay = true
        }
    }

    private struct Drag {
        var gesture: String
        var band: Int
        /// Whether Option was down at the press: the drag sets the q.
        var widens: Bool
        var startQ: Double
        var startY: CGFloat
        var moved = false
    }

    private var drag: Drag?
    /// A band as dragged, ahead of the host.
    private var shown: (band: Int, value: EqLayout.Band)?
    private var release = 0
    private var link: CADisplayLink?
    /// The spectrum's top at each column, and the count it was read at.
    private var columns: [CGFloat] = []
    private var written: UInt64?
    private var idle = 0

    init(model: SongModel, effect: UInt64, fields: [FieldView], sampleRate: Double, selected: Int, onSelect: @escaping (Int) -> Void) {
        self.model = model
        self.effect = effect
        self.fields = fields
        self.sampleRate = sampleRate
        self.selected = selected
        self.onSelect = onSelect
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        link?.invalidate()
        link = nil
        guard window != nil else { return }
        let link = displayLink(target: self, selector: #selector(tick(_:)))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    private var box: CGRect { bounds.insetBy(dx: 4, dy: 4) }

    private var bands: [EqLayout.Band] {
        var bands = EqLayout.Band.bands(of: fields)
        if let shown, bands.indices.contains(shown.band) { bands[shown.band] = shown.value }
        return bands
    }

    /// Reads the spectrum when the equalizer has put out new frames, and
    /// lets the drawn one fall once it has not: 1.5 dB a frame.
    @objc private func tick(_ link: CADisplayLink) {
        let box = self.box
        guard box.width >= 1 else { return }
        // Nothing has played for a while: look less often.
        idle += 1
        if idle > 120, idle % 10 != 0 { return }
        if let spectrum = model.spectrum(effect: effect), spectrum.written != written {
            written = spectrum.written
            idle = 0
            columns = EqLayout.spectrumColumns(levels: spectrum.levels, sampleRate: Double(spectrum.sampleRate), in: box)
            needsDisplay = true
        } else if columns.contains(where: { $0 < box.maxY }) {
            let step = box.height * 1.5 / CGFloat(-EqLayout.spectrumFloorDb)
            columns = columns.map { min($0 + step, box.maxY) }
            needsDisplay = true
        }
    }

    private func set(_ band: Int, _ value: EqLayout.Band, gesture: String) {
        shown = (band, value)
        model.drag(.bandSet(effect: effect, band: UInt32(band), freqHz: value.freqHz, gainDb: value.gainDb, q: value.q), gesture: gesture)
        needsDisplay = true
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        let bands = self.bands
        let held = EqLayout.grab(at: p, bands: bands, in: box)
        if event.clickCount == 2 {
            drag = nil
            if let held {
                if bands.count > 1 { model.edit(.bandRemove(effect: effect, band: UInt32(held))) }
            } else if bands.count < 16 {
                let (hz, db) = (EqLayout.hz(x: p.x, in: box), EqLayout.db(y: p.y, in: box))
                model.edit(.bandAdd(effect: effect, freqHz: (hz * 10).rounded() / 10, gainDb: min(max((db * 10).rounded() / 10, -24), 24)))
                onSelect(bands.count)
            }
            return
        }
        guard let held else { return }
        onSelect(held)
        drag = Drag(gesture: model.newGesture(), band: held, widens: event.modifierFlags.contains(.option), startQ: bands[held].q, startY: p.y)
    }

    override func mouseDragged(with event: NSEvent) {
        guard var d = drag else { return }
        let p = convert(event.locationInWindow, from: nil)
        let bands = self.bands
        guard bands.indices.contains(d.band) else { return }
        let was = bands[d.band]
        var next = was
        if d.widens {
            next.q = EqLayout.widened(q: d.startQ, by: p.y - d.startY)
        } else {
            next = EqLayout.dragged(was, to: p, in: box)
        }
        if next != was {
            d.moved = true
            set(d.band, next, gesture: d.gesture)
        }
        drag = d
    }

    override func mouseUp(with event: NSEvent) {
        guard let d = drag else { return }
        drag = nil
        guard d.moved else { return }
        model.endDrag()
        hold()
    }

    /// Keeps the band as dragged until the host's song has it, or a second
    /// passes.
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
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        let shape = NSBezierPath(roundedRect: bounds, xRadius: 3, yRadius: 3)
        Theme.control.setFill()
        shape.fill()
        let box = self.box
        NSGraphicsContext.saveGraphicsState()
        shape.addClip()
        // Decades and every 12 dB, with 0 dB brighter.
        context.setLineWidth(1)
        for hz in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
            let x = EqLayout.x(hz: hz, in: box).rounded() + 0.5
            context.setStrokeColor((hz == 100 || hz == 1000 || hz == 10000 ? Theme.beatLine : Theme.gridLine).cgColor)
            context.move(to: CGPoint(x: x, y: box.minY))
            context.addLine(to: CGPoint(x: x, y: box.maxY))
            context.strokePath()
        }
        for db in [-24.0, -12.0, 0.0, 12.0, 24.0] {
            let y = EqLayout.y(db: db, in: box).rounded() + 0.5
            context.setStrokeColor((db == 0 ? Theme.barLine : Theme.gridLine).cgColor)
            context.move(to: CGPoint(x: box.minX, y: y))
            context.addLine(to: CGPoint(x: box.maxX, y: y))
            context.strokePath()
        }
        // The spectrum, filled to the floor.
        if columns.contains(where: { $0 < box.maxY }) {
            let path = CGMutablePath()
            path.move(to: CGPoint(x: box.minX, y: box.maxY))
            for (i, y) in columns.enumerated() {
                path.addLine(to: CGPoint(x: box.minX + CGFloat(i), y: y))
            }
            path.addLine(to: CGPoint(x: box.minX + CGFloat(columns.count), y: box.maxY))
            path.closeSubpath()
            context.setFillColor(Theme.modulation.withAlphaComponent(0.22).cgColor)
            context.addPath(path)
            context.fillPath()
        }
        // The curve.
        let bands = self.bands
        let path = CGMutablePath()
        let n = Int(box.width)
        for i in 0...n {
            let x = box.minX + CGFloat(i)
            let db = EqLayout.curveDb(bands, hz: EqLayout.hz(x: x, in: box), rate: sampleRate)
            let point = CGPoint(x: x, y: EqLayout.y(db: db, in: box))
            if i == 0 { path.move(to: point) } else { path.addLine(to: point) }
        }
        context.setStrokeColor(Theme.knob.withAlphaComponent(0.95).cgColor)
        context.setLineWidth(1.5)
        context.addPath(path)
        context.strokePath()
        // The points, the selected band's in the cue color, each numbered.
        let font = NSFont.systemFont(ofSize: 8, weight: .semibold)
        for (i, band) in bands.enumerated() {
            let p = EqLayout.point(band, in: box)
            context.setFillColor((i == selected ? Theme.cue : Theme.knob).cgColor)
            context.fillEllipse(in: CGRect(x: p.x - 4, y: p.y - 4, width: 8, height: 8))
            context.setStrokeColor(Theme.text.withAlphaComponent(0.7).cgColor)
            context.setLineWidth(1)
            context.strokeEllipse(in: CGRect(x: p.x - 4, y: p.y - 4, width: 8, height: 8))
            ("\(i + 1)" as NSString).draw(at: CGPoint(x: p.x + 5, y: p.y - 11), withAttributes: [.font: font, .foregroundColor: Theme.dimText])
        }
        NSGraphicsContext.restoreGraphicsState()
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
    /// The value a person typed into the bar, held to its range and to whole
    /// numbers where it has them; nil for text that is not a number.
    func typed(_ text: String) -> Double? {
        guard var value = ValueScale.parse(text, unit: unit) else { return nil }
        if whole { value = value.rounded() }
        return Swift.min(Swift.max(value, min), max)
    }

    /// A value as the field opened on the bar starts: the number alone, in
    /// the bar's unit, so that what is shown can be typed back.
    func typing(_ value: Double) -> String {
        whole ? String(Int(value)) : ValueScale.plain(value)
    }

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
/// each change would be a fade through silence. A click that does not move
/// opens a field over the bar with the number selected, as the tempo's is:
/// Return and a click elsewhere apply what was typed, held to the range, and
/// Escape cancels. The bar keeps the second click of a double-click, so the
/// field does not take the reset.
final class KnobBarView: NSView, NSTextFieldDelegate {
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
    /// The field a value is typed into, while it is open, and when it opened.
    private var field: NSTextField?
    private var opened: TimeInterval = 0

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

    /// The bar's own box, 16 points high in the middle of its bounds.
    private var box: CGRect {
        bounds.insetBy(dx: 0, dy: (bounds.height - 16) / 2)
    }

    override func layout() {
        super.layout()
        field?.frame = box
    }

    /// While the field has just opened, the second click of a double-click is
    /// the bar's, not the field's.
    override func hitTest(_ point: NSPoint) -> NSView? {
        if field != nil, ProcessInfo.processInfo.systemUptime - opened < NSEvent.doubleClickInterval,
           bounds.contains(convert(point, from: superview)) {
            return self
        }
        return super.hitTest(point)
    }

    /// Opens the field over the bar, with the value selected to be typed over.
    private func beginTyping() {
        guard field == nil, window != nil else { return }
        let field = NSTextField(frame: box)
        field.isBordered = false
        field.drawsBackground = true
        field.backgroundColor = Theme.control
        field.textColor = Theme.text
        field.font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
        field.alignment = .center
        field.wantsLayer = true
        field.layer?.cornerRadius = 3
        field.layer?.masksToBounds = true
        if let cell = field.cell as? NSTextFieldCell {
            cell.usesSingleLineMode = true
            cell.wraps = false
            cell.isScrollable = true
        }
        field.stringValue = spec.typing(start)
        field.delegate = self
        addSubview(field)
        self.field = field
        opened = ProcessInfo.processInfo.systemUptime
        field.selectText(nil)
        needsDisplay = true
    }

    /// Closes the field: with `commit`, the typed value is set, held to the
    /// range; `focus` gives the keys back to the arrangement.
    private func endTyping(commit: Bool, focus: Bool) {
        guard let field else { return }
        self.field = nil
        let typed = field.stringValue
        field.abortEditing()
        field.removeFromSuperview()
        if commit, let value = spec.typed(typed), value != self.value {
            shown = value
            hold()
            model.edit(edit(value))
        }
        needsDisplay = true
        if focus { model.onFocus?() }
    }

    func controlTextDidEndEditing(_ notification: Notification) {
        // Return gives the keys back; a click elsewhere has given them to
        // what was clicked.
        let movement = (notification.userInfo?["NSTextMovement"] as? Int).map { NSTextMovement(rawValue: $0) }
        endTyping(commit: true, focus: movement == .return)
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        guard selector == #selector(NSResponder.cancelOperation(_:)) else { return false }
        endTyping(commit: false, focus: true)
        return true
    }

    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 2, let initial = spec.initial {
            endTyping(commit: false, focus: true)
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
        if !d.moved {
            beginTyping()
            return
        }
        guard let shown else { return }
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
        let box = box
        let shape = NSBezierPath(roundedRect: box, xRadius: 3, yRadius: 3)
        Theme.control.setFill()
        shape.fill()
        // The field shows the value while it is open.
        if field != nil { return }
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
