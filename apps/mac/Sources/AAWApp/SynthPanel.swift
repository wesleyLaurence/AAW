import AAWCore
import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// A MIDI track's Synth, first in its chain: every part of the patch as a
/// column of controls drawn from its fields as an effect's panel is, with a
/// drawing at the head of each: the wave over one cycle, the filter's
/// response with its corner to drag, the envelope with its four handles, the
/// LFO's shape. The Synth's own column ends in an octave of keys that play a
/// note now through the track. Each envelope, LFO and macro, and velocity,
/// note and random, is a tab dragged onto a control to modulate it; a
/// control a matrix entry moves shows how far under its bar. + adds a part
/// and × takes one off. A control's change is an edit to the host, as `daw
/// synth set` is; the header above, `SynthHeader`, loads and saves patches.
struct SynthPanel: View {
    let model: SongModel
    let track: TrackView
    let synth: SynthView
    /// The panel's height, under its header: the detail panel's less its edges.
    let height: CGFloat

    /// A column's width, and the gap between columns.
    static let column: CGFloat = 188
    static let gap: CGFloat = 6
    /// A drawing's width and the heights of each kind.
    static let drawingWidth: CGFloat = column - 12
    static let waveHeight: CGFloat = 34
    static let curveHeight: CGFloat = 56
    static let keysHeight: CGFloat = 44
    /// A source dragged out of a column's title or the matrix's chips; the
    /// data is the source's name, `env2`, `lfo1`, `macros.tone`, `velocity`.
    static let sourceType = "org.aaw.synth-source"

    /// How wide the panel is: a column for each part, and one for each of
    /// the patch's effects.
    static func width(_ synth: SynthView) -> CGFloat {
        let columns = 2 + synth.oscillators.count + synth.envelopes.count + synth.lfos.count + (synth.macros.isEmpty ? 0 : 1) + 1 + synth.effects.count
        return CGFloat(columns) * (column + gap) + gap
    }

    private func set(_ field: String) -> (FieldValue) -> Edit {
        { [key = track.key] value in .synthSet(track: key, field: field, value: value) }
    }

    private func value(_ fields: [FieldView], _ name: String) -> Double {
        if case .number(let x)? = fields.first(where: { $0.name.hasSuffix(name) })?.value { return x }
        return 0
    }

    private func text(_ fields: [FieldView], _ name: String) -> String {
        if case .text(let s)? = fields.first(where: { $0.name.hasSuffix(name) })?.value { return s }
        return ""
    }

    /// A field's row: its label, its control with the reach of the matrix
    /// entries that move it drawn under the bar, and its lane mark. A source
    /// dropped on it adds an entry to the field's target.
    private func row(_ field: FieldView) -> some View {
        let entries = synth.modulation.filter { $0.field == field.name }
        return SynthFieldRow(model: model, track: track, field: field, entries: entries, edit: set(field.name))
    }

    private func title(_ text: String, source: String? = nil, remove: (() -> Void)? = nil, help: String? = nil) -> some View {
        HStack(spacing: 4) {
            if let source {
                SourceTab(name: text, source: source)
                    .help("Drag onto a control to modulate it with \(source)")
            } else {
                Text(text)
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if let remove {
                Button(action: remove) {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.borderless)
                .font(.system(size: 8))
                .help(help ?? "Remove")
            }
        }
        .frame(height: 14)
    }

    private func column<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            content()
            Spacer(minLength: 0)
        }
        .frame(width: Self.column, alignment: .top)
    }

    private func removePart(_ part: String, _ name: String) {
        model.edit(.synthPartRemove(track: track.key, part: part, name: name))
    }

    var body: some View {
        ScrollView(.vertical) {
            HStack(alignment: .top, spacing: Self.gap) {
                column {
                    HStack(spacing: 4) {
                        Text("Synth").font(.system(size: 10, weight: .semibold)).foregroundStyle(.secondary)
                        Spacer(minLength: 0)
                        Menu {
                            Button("Add Oscillator") { model.edit(.synthPartAdd(track: track.key, part: "oscillators")) }
                                .disabled(synth.oscillators.count >= 4)
                            Button("Add Envelope") { model.edit(.synthPartAdd(track: track.key, part: "envelopes")) }
                                .disabled(synth.envelopes.count >= 4)
                            Button("Add LFO") { model.edit(.synthPartAdd(track: track.key, part: "lfos")) }
                                .disabled(synth.lfos.count >= 4)
                            Button("Add Macro") { model.edit(.synthPartAdd(track: track.key, part: "macros")) }
                                .disabled(synth.macros.count >= 8)
                            Menu("Add Effect") {
                                ForEach(DeviceChain.kinds, id: \.self) { kind in
                                    Button(readable(kind)) { model.edit(.synthEffectAdd(track: track.key, kind: kind, index: nil)) }
                                }
                            }
                            .disabled(synth.effects.count >= 32)
                        } label: {
                            Image(systemName: "plus").font(.system(size: 9))
                        }
                        .menuStyle(.borderlessButton)
                        .menuIndicator(.hidden)
                        .fixedSize()
                        .help("Add an oscillator, an envelope, an LFO, a macro or an effect of the patch's own")
                    }
                    .frame(height: 14)
                    ForEach(synth.fields, id: \.name) { field in row(field) }
                    SynthKeys(model: model, track: track)
                        .padding(.top, 4)
                }
                ForEach(synth.oscillators, id: \.name) { group in
                    column {
                        title("Osc \(group.name)", remove: synth.oscillators.count > 1 ? { removePart("oscillators", group.name) } : nil,
                              help: "Remove oscillator \(group.name)")
                        WaveShape(wave: text(group.fields, ".wave"), pulseWidth: value(group.fields, ".pulse_width"), cycle: group.cycle)
                            .frame(width: Self.drawingWidth, height: Self.waveHeight)
                            .help(group.cycle.isEmpty ? "One cycle of the wave" : "One cycle of the table, as it is read at low pitches")
                        ForEach(group.fields, id: \.name) { field in row(field) }
                    }
                }
                column {
                    title("Filter")
                    FilterCurve(model: model, track: track, fields: synth.filter)
                        .frame(width: Self.drawingWidth, height: Self.curveHeight)
                        .help("The filter's response. Drag the corner: across for the cutoff, up and down for the resonance")
                    ForEach(synth.filter, id: \.name) { field in row(field) }
                }
                ForEach(synth.envelopes, id: \.name) { group in
                    column {
                        title("Env \(group.name)", source: group.name,
                              remove: group.name == "amp" ? nil : { removePart("envelopes", group.name) },
                              help: "Remove envelope \(group.name)")
                        EnvelopeShape(model: model, track: track, name: group.name, fields: group.fields)
                            .frame(width: Self.drawingWidth, height: Self.curveHeight)
                            .help("The envelope. Drag the end of the attack, of the decay or of the release")
                        ForEach(group.fields, id: \.name) { field in row(field) }
                    }
                }
                ForEach(synth.lfos, id: \.name) { group in
                    column {
                        title("LFO \(group.name)", source: group.name, remove: { removePart("lfos", group.name) },
                              help: "Remove LFO \(group.name)")
                        LfoShape(shape: text(group.fields, ".shape"), phasePercent: value(group.fields, ".phase_percent"))
                            .frame(width: Self.drawingWidth, height: Self.waveHeight)
                            .help("One cycle of the LFO")
                        ForEach(group.fields, id: \.name) { field in row(field) }
                    }
                }
                if !synth.macros.isEmpty {
                    column {
                        title("Macros")
                        ForEach(synth.macros, id: \.name) { field in
                            HStack(spacing: 4) {
                                SourceTab(name: field.label, source: field.name)
                                    .frame(width: 58, alignment: .leading)
                                    .help("Drag onto a control to have the macro move it")
                                FieldControl(model: model, field: field, edit: set(field.name))
                                SynthLaneMark(model: model, track: track, field: field)
                                Button {
                                    removePart("macros", field.label)
                                } label: {
                                    Image(systemName: "xmark")
                                }
                                .buttonStyle(.borderless)
                                .font(.system(size: 8))
                                .help("Remove macro \(field.label)")
                            }
                            .frame(height: 18)
                        }
                    }
                }
                column {
                    title("Matrix")
                    HStack(spacing: 4) {
                        ForEach(["velocity", "note", "random"], id: \.self) { source in
                            SourceTab(name: source, source: source)
                                .help("Drag onto a control to modulate it with the note's \(source)")
                        }
                        Spacer(minLength: 0)
                    }
                    .frame(height: 16)
                    if synth.modulation.isEmpty {
                        Text("No modulation. Drag an envelope's, an LFO's or a macro's tab, or one of these, onto a control; the agent uses `daw synth mod`.")
                            .font(.system(size: 9))
                            .foregroundStyle(.tertiary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    ForEach(Array(synth.modulation.enumerated()), id: \.offset) { index, entry in
                        MatrixRow(model: model, track: track, index: index, entry: entry)
                    }
                }
                ForEach(Array(synth.effects.enumerated()), id: \.element.key) { index, effect in
                    column {
                        SynthEffectColumn(model: model, track: track, effect: effect, index: index, count: synth.effects.count)
                    }
                }
            }
            .padding(.horizontal, Self.gap)
            .padding(.vertical, 5)
        }
        .frame(height: height)
    }
}

/// One of the patch's own effects as a column: its name with the marks that
/// bypass, move and remove it, and a row for each of its fields, each with
/// the lane mark of the track, as an effect's panel has them. An equalizer
/// lists its bands, each a group of rows with a mark to take it out.
private struct SynthEffectColumn: View {
    let model: SongModel
    let track: TrackView
    let effect: EffectView
    let index: Int
    let count: Int

    private var title: String {
        let kind = readable(effect.kind)
        return effect.id.map { "\(kind) · \($0)" } ?? kind
    }

    private func row(_ field: FieldView, label: String) -> some View {
        HStack(spacing: 4) {
            Text(label)
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .frame(width: 58, alignment: .leading)
                .lineLimit(1)
            FieldControl(model: model, effect: effect.key, field: field)
            SynthLaneMark(model: model, track: track, field: field)
        }
        .frame(height: 18)
    }

    var body: some View {
        HStack(spacing: 3) {
            Button {
                model.edit(.effectBypass(effect: effect.key, on: !effect.bypass))
            } label: {
                Image(systemName: "power").foregroundStyle(effect.bypass ? Color.secondary : Color.green)
            }
            .help(effect.bypass ? "Bypassed: the effect does not process" : "Bypass")
            Text(title).font(.system(size: 10, weight: .semibold)).foregroundStyle(.secondary).lineLimit(1)
            Spacer(minLength: 0)
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
            .help("Move earlier in the patch's chain")
            .disabled(index == 0)
            Button {
                model.edit(.effectMove(effect: effect.key, index: UInt32(index + 1)))
            } label: {
                Image(systemName: "chevron.right")
            }
            .help("Move later in the patch's chain")
            .disabled(index == count - 1)
            Button {
                model.edit(.effectRemove(effect: effect.key))
            } label: {
                Image(systemName: "xmark")
            }
            .help("Remove the effect from the patch, with the lanes that automate it")
        }
        .buttonStyle(.borderless)
        .font(.system(size: 8))
        .frame(height: 14)
        Group {
            if effect.kind == "eq" {
                ForEach(0..<Int(effect.bands), id: \.self) { band in
                    HStack(spacing: 4) {
                        Text("Band \(band + 1)").font(.system(size: 9, weight: .semibold)).foregroundStyle(.tertiary)
                        Spacer(minLength: 0)
                        Button {
                            model.edit(.bandRemove(effect: effect.key, band: UInt32(band)))
                        } label: {
                            Image(systemName: "minus.circle")
                        }
                        .buttonStyle(.borderless)
                        .font(.system(size: 8))
                        .help("Remove the band")
                        .disabled(effect.bands <= 1)
                    }
                    .frame(height: 14)
                    ForEach(effect.fields.filter { $0.band == UInt32(band) }, id: \.name) { field in
                        row(field, label: field.label)
                    }
                }
            } else {
                ForEach(effect.fields, id: \.name) { field in
                    row(field, label: field.label)
                }
            }
        }
        .opacity(effect.bypass ? 0.5 : 1)
    }
}

/// A field of the Synth as a row: label, control, lane mark; under the
/// control, a line for each matrix entry that moves the field, from its value
/// to where full modulation takes it; and a drop of a source onto it.
private struct SynthFieldRow: View {
    let model: SongModel
    let track: TrackView
    let field: FieldView
    let entries: [ModulationView]
    let edit: (FieldValue) -> Edit
    @State private var targeted = false

    private var target: String? { synthModTarget(field: field.name) }

    private var reaches: [(from: Double, to: Double)] {
        guard case .number(let value) = field.value, field.kind == .number else { return [] }
        return entries.map { SynthLayout.reach(value: value, min: field.min, max: field.max, log: field.log, amount: $0.amount, unit: $0.unit) }
    }

    var body: some View {
        HStack(spacing: 4) {
            Text(field.label)
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .frame(width: 58, alignment: .leading)
                .lineLimit(1)
            FieldControl(model: model, field: field, edit: edit)
                .overlay(alignment: .bottom) {
                    if !reaches.isEmpty {
                        GeometryReader { geometry in
                            let w = geometry.size.width
                            ForEach(Array(reaches.enumerated()), id: \.offset) { i, r in
                                let (a, b) = (min(r.from, r.to), max(r.from, r.to))
                                Rectangle()
                                    .fill(Color(nsColor: Theme.modulation))
                                    .frame(width: max(2, w * CGFloat(b - a)), height: 2)
                                    .offset(x: w * CGFloat(a), y: geometry.size.height - 2 - CGFloat(i) * 3)
                            }
                        }
                        .allowsHitTesting(false)
                    }
                }
                .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(Color(nsColor: Theme.modulation), lineWidth: targeted ? 2 : 0))
            SynthLaneMark(model: model, track: track, field: field)
        }
        .frame(height: 18)
        .onDrop(of: target == nil ? [] : [UTType(SynthPanel.sourceType) ?? .data], isTargeted: $targeted) { providers in
            guard let target, let provider = providers.first else { return false }
            let (key, unit) = (track.key, entries.first?.unit)
            provider.loadDataRepresentation(forTypeIdentifier: SynthPanel.sourceType) { data, _ in
                guard let data, let source = String(data: data, encoding: .utf8) else { return }
                let amount = SynthLayout.startingAmount(unit: unit ?? SynthLayout.unit(ofTarget: target))
                DispatchQueue.main.async {
                    MainActor.assumeIsolated { model.edit(.synthModAdd(track: key, source: source, target: target, amount: amount)) }
                }
            }
            return true
        }
        .help(entries.isEmpty ? "" : entries.map { "\($0.source) moves it by \(ValueScale.plain($0.amount)) \($0.unit)" }.joined(separator: "\n"))
    }
}

extension SynthLayout {
    /// The unit of a target's amount, from its path, as `describe synth` has
    /// them: for a source dropped on a control with no entry yet.
    static func unit(ofTarget target: String) -> String {
        if target == "pitch" || target.hasSuffix(".pitch") { return "semitones" }
        if target.hasSuffix(".level_db") || target.hasSuffix(".drive_db") { return "dB" }
        if target.hasSuffix(".pan") { return "pan" }
        if target.hasSuffix(".unison_detune_cents") { return "cents" }
        if target.hasSuffix("_percent") || target.hasSuffix(".pulse_width") { return "points" }
        return "octaves"
    }
}

/// A modulation source as a tab to drag onto a control: an envelope's or an
/// LFO's name in its column's title, a macro's name, or velocity, note and
/// random in the matrix's column.
private struct SourceTab: View {
    let name: String
    let source: String

    var body: some View {
        HStack(spacing: 2) {
            Image(systemName: "arrow.right.circle").font(.system(size: 8))
            Text(name).font(.system(size: 10, weight: .semibold)).lineLimit(1)
        }
        .foregroundStyle(Color(nsColor: Theme.modulation))
        .padding(.horizontal, 3)
        .frame(height: 14)
        .background(RoundedRectangle(cornerRadius: 3).fill(Color(nsColor: Theme.modulation).opacity(0.14)))
        .onDrag {
            let provider = NSItemProvider()
            provider.registerDataRepresentation(forTypeIdentifier: SynthPanel.sourceType, visibility: .all) { completion in
                completion(Data(source.utf8), nil)
                return nil
            }
            return provider
        }
    }
}

/// An entry of the matrix: source and target, its amount as a bar to drag in
/// the target's unit, and × to take it out.
private struct MatrixRow: View {
    let model: SongModel
    let track: TrackView
    let index: Int
    let entry: ModulationView

    /// The span an amount is dragged over, by its unit.
    private var range: (min: Double, max: Double) {
        switch entry.unit {
        case "octaves": (-8, 8)
        case "semitones": (-48, 48)
        case "dB": (-48, 48)
        case "pan": (-2, 2)
        default: (-100, 100)
        }
    }

    var body: some View {
        HStack(spacing: 3) {
            Text("\(entry.source) → \(entry.target)")
                .font(.system(size: 9))
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(width: 92, alignment: .leading)
                .help("\(entry.source) moves \(entry.target) by the amount, in \(entry.unit)")
            KnobBar(model: model, spec: BarSpec(value: entry.amount, min: range.min, max: range.max, unit: "", initial: 0)) { [key = track.key, index] value in
                .synthModSet(track: key, index: UInt32(index), amount: (value * 100).rounded() / 100)
            }
            Button {
                model.edit(.synthModRemove(track: track.key, index: UInt32(index)))
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.borderless)
            .font(.system(size: 8))
            .help("Remove this entry")
        }
        .frame(height: 16)
    }
}

/// The mark beside a Synth field that automation can move, as `LaneMark`
/// is for an effect's: filled while a lane moves it, and a click adds the
/// lane under the track, or removes it.
struct SynthLaneMark: View {
    let model: SongModel
    let track: TrackView
    let field: FieldView

    var body: some View {
        if let param = field.param {
            Button {
                if let lane = field.lane {
                    model.edit(.laneRemove(lane: lane))
                } else {
                    model.edit(.laneAdd(row: .track(key: track.key), param: param))
                    model.onShowLanes?(.track(track.key))
                }
            } label: {
                Image(systemName: field.lane == nil ? "diamond" : "diamond.fill")
                    .font(.system(size: 8))
                    .foregroundStyle(field.lane == nil ? Color.secondary : Color(nsColor: Theme.automation))
            }
            .buttonStyle(.borderless)
            .frame(width: 12)
            .help(field.lane == nil ? "Automate \(field.label): adds a lane under the track" : "Remove the lane that automates \(field.label)")
        } else {
            Color.clear.frame(width: 12, height: 1)
        }
    }
}

// MARK: The header

/// The Synth's title strip: the patch's name, ◂ ▸ through the patches the
/// browser lists, a menu with Save…, Save As… and the patches to load, and ×
/// to take the instrument off. Save… writes over the patch the sound came
/// from when that is one of the person's own; otherwise it asks for a name,
/// as Save As… does.
struct SynthHeader: View {
    let model: SongModel
    let track: TrackView
    let synth: SynthView
    @State private var saving: PatchSave?

    /// What Save As… asks for.
    struct PatchSave: Identifiable {
        var id = UUID()
        var name: String
        var description = ""
        var tags = ""
    }

    private var patches: [PatchInfo] { model.browser.patches }
    private var mine: PatchInfo? { patches.first { !$0.factory && $0.name == synth.patch } }

    /// The place of the current patch in the list, if it is there.
    private var index: Int? {
        guard let name = synth.patch else { return nil }
        return patches.firstIndex { $0.name == name }
    }

    private func load(_ patch: PatchInfo) {
        model.addBrowserDevice("synth", to: .track(track.key), patch: patch.name)
    }

    private func step(_ by: Int) {
        guard !patches.isEmpty else { return }
        let next = index.map { ($0 + by + patches.count) % patches.count } ?? (by > 0 ? 0 : patches.count - 1)
        load(patches[next])
    }

    private func save() {
        if let mine {
            model.savePatch(track: track.key, name: mine.name, description: nil, tags: [], replace: true)
        } else {
            saving = PatchSave(name: synth.patch.map { "\($0) 2" } ?? "")
        }
    }

    var body: some View {
        HStack(spacing: 5) {
            Image(systemName: "pianokeys").foregroundStyle(.secondary)
            Button { step(-1) } label: { Image(systemName: "chevron.left") }
                .disabled(patches.isEmpty)
                .help("The patch before this one in the browser's list")
            Text(synth.patch ?? "Synth").font(.system(size: 11, weight: .semibold)).lineLimit(1)
                .help(synth.patch.map { "The sound came from the patch \($0)" } ?? "The plain saw, or a sound of this song's own")
            Button { step(1) } label: { Image(systemName: "chevron.right") }
                .disabled(patches.isEmpty)
                .help("The next patch in the browser's list")
            Menu {
                Button(mine == nil ? "Save…" : "Save \(mine?.name ?? "")") { save() }
                Button("Save As…") { saving = PatchSave(name: synth.patch ?? "") }
                Divider()
                let factory = patches.filter(\.factory)
                let own = patches.filter { !$0.factory }
                if !factory.isEmpty {
                    Menu("Factory") {
                        ForEach(factory, id: \.slug) { patch in Button(patch.name) { load(patch) } }
                    }
                }
                if !own.isEmpty {
                    Menu("Mine") {
                        ForEach(own, id: \.slug) { patch in Button(patch.name) { load(patch) } }
                    }
                }
            } label: {
                Image(systemName: "chevron.down.circle")
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("Save the patch, or load one")
            Spacer(minLength: 2)
            Button {
                model.edit(.instrumentRemove(track: track.key))
            } label: {
                Image(systemName: "xmark")
            }
            .help("Take the Synth off; the notes are kept")
        }
        .buttonStyle(.borderless)
        .font(.system(size: 10))
        .padding(.horizontal, 7)
        .frame(height: 24)
        .background(Color(nsColor: Theme.gray(0.24)))
        .onAppear { model.browser.refreshPatches() }
        .sheet(item: $saving) { save in
            PatchSaveSheet(model: model, track: track, save: save) { saving = nil }
        }
    }
}

/// Save As…: a name, a description and tags for the patch, written to the
/// library over a patch of the name only when asked.
private struct PatchSaveSheet: View {
    let model: SongModel
    let track: TrackView
    @State var save: SynthHeader.PatchSave
    let done: () -> Void
    @State private var replace = false

    private var taken: Bool {
        let slug = save.name.lowercased().split(whereSeparator: { !$0.isLetter && !$0.isNumber }).joined(separator: "-")
        return model.browser.patches.contains { !$0.factory && $0.slug == slug }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Save the Synth on \(track.id) as a patch").font(.headline)
            Text("A patch is a file in the library, ~/Music/AAW/library/patches, that any song loads; the song's sound is then named after it.")
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Form {
                TextField("Name", text: $save.name)
                TextField("Description", text: $save.description)
                TextField("Tags", text: $save.tags, prompt: Text("bass, sub"))
                if taken {
                    Toggle("Write over the patch saved under this name", isOn: $replace)
                }
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { done() }.keyboardShortcut(.cancelAction)
                Button("Save") {
                    let tags = save.tags.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
                    model.savePatch(track: track.key, name: save.name, description: save.description, tags: tags, replace: replace)
                    done()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(save.name.trimmingCharacters(in: .whitespaces).isEmpty || (taken && !replace))
            }
        }
        .padding(16)
        .frame(width: 380)
    }
}

// MARK: Drawings

/// One cycle of an oscillator's wave, or of its table.
private struct WaveShape: View {
    let wave: String
    let pulseWidth: Double
    var cycle: [Double] = []

    var body: some View {
        Canvas { context, size in
            let box = CGRect(origin: .zero, size: size).insetBy(dx: 0, dy: 3)
            context.fill(Path(roundedRect: CGRect(origin: .zero, size: size), cornerRadius: 3), with: .color(Color(nsColor: Theme.control)))
            context.stroke(Path { p in
                p.move(to: CGPoint(x: 0, y: size.height / 2))
                p.addLine(to: CGPoint(x: size.width, y: size.height / 2))
            }, with: .color(Color(nsColor: Theme.gridLine)), lineWidth: 1)
            var path = Path()
            let n = Int(size.width)
            for i in 0...n {
                let t = Double(i) / Double(max(n, 1))
                let value = cycle.isEmpty ? SynthLayout.wave(wave, at: t, pulseWidth: pulseWidth) : SynthLayout.table(cycle, at: t)
                let y = box.midY - CGFloat(value) * box.height / 2
                if i == 0 { path.move(to: CGPoint(x: CGFloat(i), y: y)) } else { path.addLine(to: CGPoint(x: CGFloat(i), y: y)) }
            }
            context.stroke(path, with: .color(Color(nsColor: Theme.knob).opacity(0.95)), lineWidth: 1.5)
        }
    }
}

/// One cycle of an LFO's shape, from its starting phase.
private struct LfoShape: View {
    let shape: String
    let phasePercent: Double

    var body: some View {
        Canvas { context, size in
            let box = CGRect(origin: .zero, size: size).insetBy(dx: 0, dy: 3)
            context.fill(Path(roundedRect: CGRect(origin: .zero, size: size), cornerRadius: 3), with: .color(Color(nsColor: Theme.control)))
            context.stroke(Path { p in
                p.move(to: CGPoint(x: 0, y: size.height / 2))
                p.addLine(to: CGPoint(x: size.width, y: size.height / 2))
            }, with: .color(Color(nsColor: Theme.gridLine)), lineWidth: 1)
            var path = Path()
            let n = Int(size.width)
            for i in 0...n {
                let t = Double(i) / Double(max(n, 1)) + phasePercent / 100
                let y = box.midY - CGFloat(SynthLayout.lfo(shape, at: t)) * box.height / 2
                if i == 0 { path.move(to: CGPoint(x: CGFloat(i), y: y)) } else { path.addLine(to: CGPoint(x: CGFloat(i), y: y)) }
            }
            context.stroke(path, with: .color(Color(nsColor: Theme.modulation).opacity(0.9)), lineWidth: 1.5)
        }
    }
}

/// The filter's response in SwiftUI.
private struct FilterCurve: NSViewRepresentable {
    let model: SongModel
    let track: TrackView
    let fields: [FieldView]

    func makeNSView(context: Context) -> FilterCurveView {
        FilterCurveView(model: model, track: track.key, fields: fields)
    }

    func updateNSView(_ view: FilterCurveView, context: Context) {
        view.track = track.key
        view.fields = fields
    }
}

/// The filter's magnitude over 10 Hz to 20 kHz, with its corner at the
/// cutoff. A press takes hold of the corner: dragging across moves the
/// cutoff and, when it began on the corner, up and down moves the resonance,
/// both heard as they move and as one undo step.
final class FilterCurveView: NSView {
    private let model: SongModel
    var track: UInt64
    var fields: [FieldView] {
        didSet {
            if let shown, drag == nil, abs(shown.cutoff - value("cutoff_hz")) < 1e-9, abs(shown.resonance - value("resonance_percent")) < 1e-9 {
                self.shown = nil
            }
            needsDisplay = true
        }
    }

    private struct Drag {
        var gesture: String
        /// Whether the press was on the corner, so that the height counts.
        var resonates: Bool
        var moved = false
    }

    private var drag: Drag?
    /// The values as dragged, ahead of the host.
    private var shown: (cutoff: Double, resonance: Double)?
    private var release = 0

    init(model: SongModel, track: UInt64, fields: [FieldView]) {
        self.model = model
        self.track = track
        self.fields = fields
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    private func value(_ name: String) -> Double {
        if case .number(let x)? = fields.first(where: { $0.name == "filter.\(name)" })?.value { return x }
        return 0
    }

    private var mode: String {
        if case .text(let s)? = fields.first(where: { $0.name == "filter.mode" })?.value { return s }
        return "lowpass"
    }

    private var enabled: Bool {
        if case .flag(let on)? = fields.first(where: { $0.name == "filter.enabled" })?.value { return on }
        return true
    }

    private var sections: Int { value("slope_db_per_octave") >= 24 ? 2 : 1 }
    private var cutoff: Double { shown?.cutoff ?? value("cutoff_hz") }
    private var resonance: Double { shown?.resonance ?? value("resonance_percent") }
    private var box: CGRect { bounds.insetBy(dx: 4, dy: 4) }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        let corner = SynthLayout.corner(mode: mode, sections: sections, cutoffHz: cutoff, resonancePercent: resonance, in: box)
        drag = Drag(gesture: model.newGesture(), resonates: hypot(corner.x - p.x, corner.y - p.y) <= SynthLayout.grab)
        mouseDragged(with: event)
    }

    override func mouseDragged(with event: NSEvent) {
        guard var d = drag else { return }
        let p = convert(event.locationInWindow, from: nil)
        let moved = SynthLayout.filter(at: p, mode: mode, sections: sections, in: box)
        let next = (cutoff: moved.cutoffHz, resonance: d.resonates ? moved.resonancePercent : resonance)
        if next != (cutoff, resonance) {
            d.moved = true
            shown = next
            model.drag(.synthSetFields(track: track, fields: [
                SynthFieldValue(field: "filter.cutoff_hz", value: .number(value: next.cutoff)),
                SynthFieldValue(field: "filter.resonance_percent", value: .number(value: next.resonance)),
            ]), gesture: d.gesture)
            needsDisplay = true
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

    /// Keeps the values as dragged until the host's song has them, or a
    /// second passes.
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
        // Decades and 0 dB.
        context.setStrokeColor(Theme.gridLine.cgColor)
        context.setLineWidth(1)
        for hz in [100.0, 1000.0, 10000.0] {
            let x = SynthLayout.x(hz: hz, in: box).rounded() + 0.5
            context.move(to: CGPoint(x: x, y: box.minY))
            context.addLine(to: CGPoint(x: x, y: box.maxY))
        }
        let zero = SynthLayout.y(db: 0, in: box).rounded() + 0.5
        context.move(to: CGPoint(x: box.minX, y: zero))
        context.addLine(to: CGPoint(x: box.maxX, y: zero))
        context.strokePath()
        // The response.
        let path = CGMutablePath()
        let n = Int(box.width)
        for i in 0...n {
            let x = box.minX + CGFloat(i)
            let hz = SynthLayout.hz(x: x, in: box)
            let db = enabled ? SynthLayout.responseDb(mode: mode, sections: sections, cutoffHz: cutoff, resonancePercent: resonance, hz: hz) : 0
            let point = CGPoint(x: x, y: SynthLayout.y(db: db, in: box))
            if i == 0 { path.move(to: point) } else { path.addLine(to: point) }
        }
        context.setStrokeColor(Theme.knob.withAlphaComponent(enabled ? 0.95 : 0.4).cgColor)
        context.setLineWidth(1.5)
        context.addPath(path)
        context.strokePath()
        // The corner.
        let corner = SynthLayout.corner(mode: mode, sections: sections, cutoffHz: cutoff, resonancePercent: resonance, in: box)
        context.setFillColor(Theme.cue.cgColor)
        context.fillEllipse(in: CGRect(x: corner.x - 3.5, y: corner.y - 3.5, width: 7, height: 7))
    }
}

/// An envelope in SwiftUI.
private struct EnvelopeShape: NSViewRepresentable {
    let model: SongModel
    let track: TrackView
    let name: String
    let fields: [FieldView]

    func makeNSView(context: Context) -> EnvelopeView {
        EnvelopeView(model: model, track: track.key, name: name, fields: fields)
    }

    func updateNSView(_ view: EnvelopeView, context: Context) {
        view.track = track.key
        view.name = name
        view.fields = fields
    }
}

/// An envelope drawn as its attack, decay, sustain and release, with a
/// handle at the end of the attack, of the decay and of the release. A
/// handle dragged sets the times, and the decay's the sustain too, heard as
/// they move and as one undo step.
final class EnvelopeView: NSView {
    private let model: SongModel
    var track: UInt64
    var name: String
    var fields: [FieldView] {
        didSet {
            if let shown, drag == nil, shown.allSatisfy({ abs($0.value - value($0.key)) < 1e-9 }) {
                self.shown = nil
            }
            needsDisplay = true
            window?.invalidateCursorRects(for: self)
        }
    }

    private struct Drag {
        var handle: SynthLayout.Handle
        var corners: [CGPoint]
        var gesture: String
        var moved = false
    }

    private var drag: Drag?
    /// The fields as dragged, ahead of the host.
    private var shown: [String: Double]?
    private var release = 0

    init(model: SongModel, track: UInt64, name: String, fields: [FieldView]) {
        self.model = model
        self.track = track
        self.name = name
        self.fields = fields
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    private func value(_ field: String) -> Double {
        if let x = shown?[field] { return x }
        if case .number(let x)? = fields.first(where: { $0.name.hasSuffix(".\(field)") })?.value { return x }
        return 0
    }

    private var box: CGRect { bounds.insetBy(dx: 5, dy: 5) }

    private var corners: [CGPoint] {
        SynthLayout.envelope(attackMs: value("attack_ms"), decayMs: value("decay_ms"), sustainPercent: value("sustain_percent"), releaseMs: value("release_ms"), in: box)
    }

    override func resetCursorRects() {
        for (i, c) in corners.enumerated() where [1, 2, 4].contains(i) {
            addCursorRect(CGRect(x: c.x - SynthLayout.grab, y: c.y - SynthLayout.grab, width: 2 * SynthLayout.grab, height: 2 * SynthLayout.grab), cursor: .openHand)
        }
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        let corners = self.corners
        guard let handle = SynthLayout.handle(at: p, corners: corners) else { return }
        drag = Drag(handle: handle, corners: corners, gesture: model.newGesture())
    }

    override func mouseDragged(with event: NSEvent) {
        guard var d = drag else { return }
        let p = convert(event.locationInWindow, from: nil)
        let set = SynthLayout.envelope(d.handle, at: p, corners: d.corners, in: box)
        var next = shown ?? [:]
        var changed = false
        for (field, v) in set where abs(v - value(field)) > 1e-9 {
            next[field] = v
            changed = true
        }
        if changed {
            d.moved = true
            shown = next
            model.drag(.synthSetFields(track: track, fields: set.map {
                SynthFieldValue(field: "envelopes.\(name).\($0.field)", value: .number(value: $0.value))
            }), gesture: d.gesture)
            needsDisplay = true
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
        let corners = self.corners
        // The quarters the segments may fill.
        context.setStrokeColor(Theme.gridLine.cgColor)
        context.setLineWidth(1)
        for i in 1..<4 {
            let x = (box.minX + box.width * CGFloat(i) / 4).rounded() + 0.5
            context.move(to: CGPoint(x: x, y: box.minY))
            context.addLine(to: CGPoint(x: x, y: box.maxY))
        }
        context.strokePath()
        let path = CGMutablePath()
        path.addLines(between: corners)
        context.setStrokeColor(Theme.knob.withAlphaComponent(0.95).cgColor)
        context.setLineWidth(1.5)
        context.addPath(path)
        context.strokePath()
        // A fill under the shape, faint.
        let fill = CGMutablePath()
        fill.addLines(between: corners)
        fill.addLine(to: CGPoint(x: corners[4].x, y: box.maxY))
        fill.closeSubpath()
        context.setFillColor(Theme.knob.withAlphaComponent(0.18).cgColor)
        context.addPath(fill)
        context.fillPath()
        context.setFillColor(Theme.cue.cgColor)
        for i in [1, 2, 4] {
            context.fillEllipse(in: CGRect(x: corners[i].x - 3.5, y: corners[i].y - 3.5, width: 7, height: 7))
        }
    }
}

// MARK: Keys

/// An octave of keys under the Synth's fields, with the octave stepped
/// beside it. A key pressed plays its note now through the track, a beat
/// long, softly at the top of the key and hard at the bottom.
private struct SynthKeys: View {
    let model: SongModel
    let track: TrackView
    @State private var low = 48

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 3) {
                Text("Keys").font(.system(size: 10)).foregroundStyle(.secondary)
                Spacer(minLength: 0)
                Button { low = max(0, low - 12) } label: { Image(systemName: "chevron.left") }
                    .disabled(low < 12)
                    .help("An octave down")
                Text("\(SynthLayout.octaveName(low: low))–\(SynthLayout.octaveName(low: low + 12))")
                    .font(.system(size: 10).monospacedDigit())
                    .frame(width: 52)
                Button { low = min(108, low + 12) } label: { Image(systemName: "chevron.right") }
                    .disabled(low > 96)
                    .help("An octave up")
            }
            .buttonStyle(.borderless)
            .font(.system(size: 9))
            .frame(height: 14)
            Keys(model: model, track: track.key, low: low)
                .frame(width: SynthPanel.drawingWidth, height: SynthPanel.keysHeight)
                .help("Play a note through the track: softly at the top of a key, hard at the bottom")
        }
    }
}

private struct Keys: NSViewRepresentable {
    let model: SongModel
    let track: UInt64
    let low: Int

    func makeNSView(context: Context) -> KeysView {
        KeysView(model: model, track: track, low: low)
    }

    func updateNSView(_ view: KeysView, context: Context) {
        view.track = track
        view.low = low
    }
}

/// The keys, an AppKit view: a press plays the note under the pointer, and a
/// drag onto another key plays that one.
final class KeysView: NSView {
    private let model: SongModel
    var track: UInt64
    var low: Int {
        didSet { needsDisplay = true }
    }

    /// The note held down, while the mouse is.
    private var pressed: Int?
    /// How long a key's note is held, in beats.
    static let lengthBeats = 1.0

    init(model: SongModel, track: UInt64, low: Int) {
        self.model = model
        self.track = track
        self.low = low
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    private var keys: SynthLayout.Keys {
        SynthLayout.Keys(box: bounds.insetBy(dx: 0.5, dy: 0.5), low: low)
    }

    private func play(at event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        guard let (pitch, velocity) = keys.note(at: p), pitch != pressed else { return }
        pressed = pitch
        model.previewNote(track: track, pitch: pitch, velocity: velocity, lengthBeats: Self.lengthBeats)
        needsDisplay = true
    }

    override func mouseDown(with event: NSEvent) {
        play(at: event)
    }

    override func mouseDragged(with event: NSEvent) {
        play(at: event)
    }

    override func mouseUp(with event: NSEvent) {
        pressed = nil
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        let keys = self.keys
        for semitone in 0...12 where !SynthLayout.Keys.black.contains(semitone) {
            let f = keys.frame(semitone: semitone)
            let down = pressed == low + semitone
            context.setFillColor((down ? Theme.cue : Theme.gray(0.86)).cgColor)
            context.fill(f.insetBy(dx: 0.5, dy: 0))
        }
        for semitone in 0...12 where SynthLayout.Keys.black.contains(semitone) {
            let f = keys.frame(semitone: semitone)
            let down = pressed == low + semitone
            context.setFillColor((down ? Theme.cue : Theme.gray(0.12)).cgColor)
            context.fill(f)
        }
        // The low C's name, at its foot.
        let style = NSMutableParagraphStyle()
        style.alignment = .center
        let first = keys.frame(semitone: 0)
        (SynthLayout.octaveName(low: low) as NSString).draw(
            in: CGRect(x: first.minX, y: first.maxY - 12, width: first.width, height: 11),
            withAttributes: [.font: NSFont.systemFont(ofSize: 8), .foregroundColor: Theme.gray(0.3), .paragraphStyle: style]
        )
    }
}
