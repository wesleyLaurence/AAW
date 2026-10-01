import AAWCore
import AppKit
import SwiftUI

/// The detail panel, under the arrangement: the devices of the row last
/// selected, or the pattern of the clip last selected. A row's header shows
/// the first and a clip the second, and the two marks at the top left change
/// between them.
struct DetailView: View {
    let model: SongModel

    static let height: CGFloat = 214

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 2) {
                    tab("Devices", .devices)
                    tab("Pattern", .pattern)
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
                    if let context = model.patternContext {
                        PatternHeader(model: model, context: context)
                    } else {
                        hint("Select a clip to edit its pattern. Double-click an empty part of a track to add a clip.")
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
                if let context = model.patternContext {
                    PatternPane(model: model, context: context, color: model.color(of: context.track.key),
                                playing: model.transport.playing, selected: model.selectedEvent)
                }
            }
            Spacer(minLength: 0)
        }
        .frame(height: Self.height, alignment: .top)
        .background(Color(nsColor: Theme.gray(0.13)))
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

/// A row's effect chain, each effect a panel of controls drawn from its
/// fields as the host describes them. No panel is made by hand for an effect
/// type. A control's change is an edit to the host, as an agent's `daw set` is.
struct DeviceView: View {
    let model: SongModel
    let chain: DeviceChain

    var body: some View {
        ScrollView(.horizontal) {
            HStack(alignment: .top, spacing: 8) {
                ForEach(Array(chain.effects.enumerated()), id: \.element.key) { index, effect in
                    DevicePanel(model: model, chain: chain, effect: effect, index: index)
                }
                if chain.effects.isEmpty {
                    Text("No effects on \(chain.name). Add one at the left, or ask the agent: its `daw effect add` lands here.")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .frame(width: 320, alignment: .leading)
                        .padding(.top, 4)
                }
            }
            .padding(8)
        }
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
        case .track: "Track"
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
            if !chain.pads.isEmpty {
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
        .frame(width: effect.kind == "eq" ? 376 : 216, height: DetailView.height - 16, alignment: .top)
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

/// The control a field's kind calls for.
private struct FieldControl: View {
    let model: SongModel
    let effect: UInt64
    let field: FieldView

    private func set(_ value: FieldValue) {
        model.edit(.effectSet(effect: effect, field: field.name, value: value))
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
                KnobBar(model: model, spec: BarSpec(field)) { [effect, name = field.name] value in
                    .effectSet(effect: effect, field: name, value: .number(value: value))
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
