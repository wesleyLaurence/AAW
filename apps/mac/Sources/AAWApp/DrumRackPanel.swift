import AAWCore
import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// The pad selected in a Drum Rack: a key, or a pad that no key plays.
enum DrumSelection: Equatable {
    case key(Int)
    case pad(String)
}

/// A MIDI track's Drum Rack, as Ableton's: from the left, an overview of
/// the 128 keys that picks the sixteen the grid shows; the grid of 4 × 4
/// pads, the lowest key at the bottom left, each a sample dropped on it with
/// its mute, play and solo; the list of the pads with each one's level, pan,
/// mute and solo; and the selected pad's sample, as the Sampler shows its
/// one pad. Every control is a field of the song.
struct DrumRackPanel: View {
    let model: SongModel
    let track: TrackView
    let rack: DrumRackView

    static let overviewWidth: CGFloat = 32
    static let padWidth: CGFloat = 66
    static let listWidth: CGFloat = 278
    static let gap: CGFloat = 3

    static var width: CGFloat {
        8 + overviewWidth + 6 + CGFloat(DrumRackLayout.columns) * (padWidth + gap) + 6 + listWidth + 6 + SamplerPanel.drumWidth
    }

    private var layout: DrumRackLayout { DrumRackLayout(bank: model.drumBank(track)) }
    private var selection: DrumSelection? { model.drumSelectionOrFirst(track) }

    /// The key's pad, if a key plays one.
    private func key(_ note: Int) -> DrumKeyView? {
        rack.keys.first { $0.note == Int32(note) }
    }

    var body: some View {
        HStack(alignment: .top, spacing: 6) {
            DrumOverview(rack: rack, layout: layout) { bank in model.drumBanks[track.key] = bank }
                .frame(width: Self.overviewWidth)
                .help("All 128 keys, a cell each; a filled one has a pad. Click or drag to show other keys in the grid")
            VStack(spacing: Self.gap) {
                ForEach(Array(layout.grid.enumerated()), id: \.offset) { _, row in
                    HStack(spacing: Self.gap) {
                        ForEach(row, id: \.self) { note in
                            DrumPadCell(model: model, track: track, note: note, key: key(note), soloed: rack.soloed,
                                        selected: selection == .key(note))
                                .frame(width: Self.padWidth)
                        }
                    }
                }
            }
            DrumPadList(model: model, track: track, rack: rack, selection: selection)
                .frame(width: Self.listWidth)
            Group {
                if let selected = model.selectedDrumKey(track) {
                    SamplerPanel(model: model, track: track, sampler: selected.pad, drum: selected)
                } else {
                    Text(rack.keys.isEmpty
                        ? "An empty Drum Rack. Drop a sample on a pad, from the browser or the Finder, and its key plays it."
                        : "Select a pad to shape its sample.")
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(6)
                }
            }
            .frame(width: SamplerPanel.drumWidth, alignment: .topLeading)
        }
        .padding(.leading, 8)
        .padding(.vertical, 5)
    }
}

/// The 128 keys, four to a row from 0 at the bottom, a filled cell where a
/// pad is and a frame on the sixteen the grid shows; a click or a drag
/// moves the frame.
private struct DrumOverview: View {
    let rack: DrumRackView
    let layout: DrumRackLayout
    let move: (Int) -> Void

    var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            let filled = Set(rack.keys.compactMap { $0.note.map(Int.init) })
            Canvas { context, _ in
                for key in 0..<128 {
                    let cell = DrumRackLayout.cell(key, in: size).insetBy(dx: 0.5, dy: 0.5)
                    let color: NSColor = filled.contains(key) ? Theme.gray(0.62) : Theme.gray(0.12)
                    context.fill(Path(cell), with: .color(Color(nsColor: color)))
                }
                let frame = layout.frame(in: size).insetBy(dx: 0.5, dy: 0.5)
                context.stroke(Path(frame), with: .color(Color(nsColor: Theme.cue)), lineWidth: 1)
            }
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0).onChanged { drag in
                let bank = DrumRackLayout.bank(atY: drag.location.y, height: size.height)
                if bank != layout.bank { move(bank) }
            })
        }
    }
}

/// A pad of the grid: its pad's name, or its key's when empty, and along
/// its foot mute, play and solo. A click selects it; a sample dropped on it
/// is played by its key; its menu takes its pad off.
private struct DrumPadCell: View {
    let model: SongModel
    let track: TrackView
    let note: Int
    let key: DrumKeyView?
    let soloed: Bool
    let selected: Bool
    @State private var targeted = false

    private var name: String { key?.pad.pad ?? noteName(midi: Int32(note)) }

    var body: some View {
        VStack(spacing: 0) {
            Text(name)
                .font(.system(size: 10, weight: key == nil ? .regular : .medium))
                .foregroundStyle(key == nil ? .secondary : .primary)
                .multilineTextAlignment(.center)
                .lineLimit(2)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(.horizontal, 3)
            if let key, let pad = key.pad.pad {
                HStack(spacing: 0) {
                    DrumToggle(label: "M", on: key.mute, color: Theme.mute, help: key.mute ? "Unmute \(pad)" : "Mute \(pad)") {
                        model.edit(.drumPadSet(track: track.key, pad: pad, field: "mute", value: .flag(value: !key.mute)))
                    }
                    Button {
                        model.previewNote(track: track.key, pitch: note, velocity: 100, lengthBeats: 1)
                    } label: {
                        Image(systemName: "play.fill").font(.system(size: 7))
                    }
                    .buttonStyle(.borderless)
                    .frame(maxWidth: .infinity)
                    .help("Play \(pad)")
                    DrumToggle(label: "S", on: key.solo, color: Theme.solo, help: key.solo ? "Unsolo \(pad)" : "Solo \(pad): only soloed pads play") {
                        model.edit(.drumPadSet(track: track.key, pad: pad, field: "solo", value: .flag(value: !key.solo)))
                    }
                }
                .frame(height: 14)
                .background(Color(nsColor: Theme.gray(0, 0.2)))
            }
        }
        .frame(maxHeight: .infinity)
        .background(Color(nsColor: key == nil ? Theme.gray(0.15) : Theme.gray(key!.sounds ? 0.31 : 0.22)))
        .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(
            targeted || selected ? Color.accentColor : Color(nsColor: Theme.gray(0.08)), lineWidth: targeted ? 2 : 1))
        .clipShape(RoundedRectangle(cornerRadius: 3))
        .contentShape(Rectangle())
        .onTapGesture { model.select(drum: .key(note), on: track) }
        .help(key.map { "\($0.pad.pad ?? ""), played by \(noteName(midi: Int32(note)))\($0.pad.sample.map { ": \($0)" } ?? "")" }
            ?? "\(noteName(midi: Int32(note))): drop a sample here and this key plays it")
        .contextMenu {
            if let pad = key?.pad.pad {
                Button("Play") { model.previewNote(track: track.key, pitch: note, velocity: 100, lengthBeats: 1) }
                Divider()
                Button("Remove \(pad)") { model.edit(.drumPadRemove(track: track.key, pad: pad)) }
            }
        }
        .onDrop(of: DrumPadCell.types, isTargeted: $targeted) { providers in
            DrumPadCell.land(providers, model: model, track: track.key, note: note)
        }
    }

    /// What a pad takes: the browser's sample, which drags as its path, and
    /// a file from the Finder.
    static let types = [UTType.fileURL.identifier, UTType.utf8PlainText.identifier, UTType.plainText.identifier]

    @MainActor
    static func land(_ providers: [NSItemProvider], model: SongModel, track: UInt64, note: Int) -> Bool {
        if let sample = model.browser.dragged {
            model.browser.dragged = nil
            model.loadDrumPad(path: sample.path, name: Browser.padName(of: sample), into: track, note: note)
            return true
        }
        guard let provider = providers.first, provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) else { return false }
        provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
            guard let data = item as? Data, let url = URL(dataRepresentation: data, relativeTo: nil),
                  Browser.extensions.contains(url.pathExtension.lowercased()) else { return }
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    model.loadDrumPad(path: url.path, name: url.deletingPathExtension().lastPathComponent, into: track, note: note)
                }
            }
        }
        return true
    }
}

/// A letter that is lit while its switch is on: a pad's mute or solo.
private struct DrumToggle: View {
    let label: String
    let on: Bool
    let color: NSColor
    let help: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(label)
                .font(.system(size: 8, weight: .bold))
                .foregroundStyle(on ? Color.black : Color.secondary)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(on ? Color(nsColor: color) : Color.clear)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity)
        .help(help)
    }
}

/// The Drum Rack's pads, a row each by key from the lowest: its key and
/// name, which select it, play, its level and pan, mute and solo.
private struct DrumPadList: View {
    let model: SongModel
    let track: TrackView
    let rack: DrumRackView
    let selection: DrumSelection?

    private func field(_ key: DrumKeyView, _ name: String) -> FieldView? {
        key.pad.fields.first { $0.name == name }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 4) {
                Text("Pad").frame(maxWidth: .infinity, alignment: .leading)
                Text("Level").frame(width: 74, alignment: .leading)
                Text("Pan").frame(width: 62, alignment: .leading)
                Spacer().frame(width: 30)
            }
            .font(.system(size: 9))
            .foregroundStyle(.tertiary)
            .padding(.horizontal, 4)
            .frame(height: 14)
            ScrollView {
                VStack(spacing: 1) {
                    ForEach(Array(rack.keys.enumerated()), id: \.offset) { _, key in
                        row(key)
                    }
                }
            }
            if rack.keys.isEmpty {
                Text("Its pads are listed here, a row each.")
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)
                    .padding(4)
            }
        }
    }

    private func row(_ key: DrumKeyView) -> some View {
        let pad = key.pad.pad ?? ""
        let mine: DrumSelection = key.note.map { .key(Int($0)) } ?? .pad(pad)
        let set = { (name: String) in SamplerPanel.setter(track: track.key, pad: pad, field: name) }
        return HStack(spacing: 4) {
            HStack(spacing: 4) {
                Text(key.note.map { noteName(midi: $0) } ?? "—")
                    .font(.system(size: 9).monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(width: 26, alignment: .leading)
                Text(pad)
                    .font(.system(size: 10, weight: .medium))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .foregroundStyle(key.sounds ? .primary : .secondary)
                Spacer(minLength: 0)
                if let note = key.note {
                    Button {
                        model.previewNote(track: track.key, pitch: Int(note), velocity: 100, lengthBeats: 1)
                    } label: {
                        Image(systemName: "play.fill").font(.system(size: 7))
                    }
                    .buttonStyle(.borderless)
                    .help("Play \(pad)")
                }
            }
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
            .onTapGesture {
                if let note = key.note { model.drumBanks[track.key] = DrumRackLayout(bank: model.drumBank(track)).showing(Int(note)) }
                model.select(drum: mine, on: track)
            }
            if let level = field(key, "gain_db") {
                FieldControl(model: model, field: level, edit: set("gain_db")).frame(width: 74)
            }
            if let pan = field(key, "pan") {
                FieldControl(model: model, field: pan, edit: set("pan")).frame(width: 62)
            }
            DrumToggle(label: "M", on: key.mute, color: Theme.mute, help: key.mute ? "Unmute \(pad)" : "Mute \(pad)") {
                model.edit(.drumPadSet(track: track.key, pad: pad, field: "mute", value: .flag(value: !key.mute)))
            }
            .frame(width: 14, height: 14)
            DrumToggle(label: "S", on: key.solo, color: Theme.solo, help: key.solo ? "Unsolo \(pad)" : "Solo \(pad)") {
                model.edit(.drumPadSet(track: track.key, pad: pad, field: "solo", value: .flag(value: !key.solo)))
            }
            .frame(width: 14, height: 14)
        }
        .padding(.horizontal, 4)
        .frame(height: 19)
        .background(Color(nsColor: selection == mine ? Theme.gray(0.32) : Theme.gray(0.22)))
        .contextMenu {
            Button("Remove \(pad)") { model.edit(.drumPadRemove(track: track.key, pad: pad)) }
        }
    }
}

/// The fields of a Drum Rack's pad that no Sampler shows: the choke group,
/// mono, the tempo its file was made at and how a key stretches it to the
/// song's.
struct DrumPadFields: View {
    let model: SongModel
    let track: UInt64
    let pad: String
    let drum: DrumKeyView

    private func row<Content: View>(_ label: String, @ViewBuilder _ content: () -> Content) -> some View {
        HStack(spacing: 4) {
            Text(label)
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .frame(width: 52, alignment: .leading)
            content()
        }
        .frame(height: 18)
    }

    private func set(_ field: String, _ value: FieldValue) {
        model.edit(.drumPadSet(track: track, pad: pad, field: field, value: value))
    }

    var body: some View {
        VStack(spacing: 2) {
            row("Choke") {
                TypedValue(text: drum.chokeGroup ?? "", unit: "", clears: true) { typed in
                    set("choke_group", .text(value: typed))
                } done: {
                    model.onFocus?()
                }
                .help("Pads with the same choke group cut each other off, as an open hat is by the closed one; empty for none")
            }
            row("Mono") {
                Toggle("", isOn: Binding(get: { drum.mono }, set: { set("mono", .flag(value: $0)) }))
                    .toggleStyle(.checkbox)
                    .controlSize(.mini)
                    .labelsHidden()
                    .help("Play the sample as one channel, panned by the pad's pan")
                Spacer(minLength: 0)
            }
            row("Tempo") {
                TypedValue(text: drum.sourceBpm.map { SamplerLayout.text($0) } ?? "", unit: "BPM", clears: true) { typed in
                    if typed.isEmpty {
                        set("source_bpm", .absent)
                    } else if let bpm = Double(typed) {
                        set("source_bpm", .number(value: bpm))
                    }
                } done: {
                    model.onFocus?()
                }
                .help("The tempo the file was made at, so a key plays it at the song's; empty plays it as it is")
            }
            row("Stretch") {
                Picker("", selection: Binding(get: { drum.stretch }, set: { set("stretch", .text(value: $0)) })) {
                    Text("Repitch").tag("repitch")
                    Text("Keep pitch").tag("preserve_pitch")
                }
                .labelsHidden()
                .controlSize(.mini)
                .disabled(drum.sourceBpm == nil)
                .help("How a file with a tempo is fitted to the song's: faster and higher, or at its own pitch")
            }
            Spacer(minLength: 0)
        }
    }
}
