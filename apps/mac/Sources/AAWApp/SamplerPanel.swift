import AAWCore
import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// A MIDI track's Sampler device, first in its chain: the file's waveform
/// with the start and the end of the part the keys play, the sample's root
/// note, and a control for each of the pad's fields, drawn from the fields as
/// an effect's panel is. Empty, it says to drop a sample on it. A sample
/// dropped on it, from the browser or the Finder, loads it in place of the
/// one it had and keeps the pad's settings.
struct SamplerPanel: View {
    let model: SongModel
    let track: TrackView
    let sampler: SamplerView

    static let width: CGFloat = 448
    /// The waveform's size, at the panel's left.
    static let waveWidth: CGFloat = 204
    static let waveHeight: CGFloat = 92

    @State private var measuring = false

    private func field(_ name: String) -> FieldView? {
        sampler.fields.first { $0.name == name }
    }

    private func set(_ name: String) -> (FieldValue) -> Edit {
        { [key = track.key] value in .samplerSet(track: key, field: name, value: value) }
    }

    /// A field's row: eight of them fit the panel's height.
    private func row<Content: View>(_ label: String, @ViewBuilder _ content: () -> Content) -> some View {
        HStack(spacing: 4) {
            Text(label)
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .frame(width: 56, alignment: .leading)
            content()
        }
        .frame(height: 18)
    }

    /// Sets the root note to the pitch `daw samples analyze` measures.
    private func measure() {
        measuring = true
        model.measureSamplerRoot(track: track.key) { measuring = false }
    }

    var body: some View {
        if sampler.pad == nil {
            Text("Empty Sampler. Drop a sample here, from the browser or the Finder, to play it on the keys.")
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(6)
        } else {
            HStack(alignment: .top, spacing: 8) {
                VStack(alignment: .leading, spacing: 3) {
                    SamplerWave(model: model, track: track.key, sampler: sampler)
                        .frame(width: Self.waveWidth, height: Self.waveHeight)
                        .help("The part of the file the keys play. Drag a marker to move the start or the end")
                    HStack(spacing: 4) {
                        Text("Start").font(.system(size: 10)).foregroundStyle(.secondary)
                        TypedValue(text: SamplerLayout.text(sampler.startSeconds), unit: "s") { typed in
                            if let seconds = Double(typed) { model.edit(set("start_seconds")(.number(value: SamplerLayout.rounded(seconds)))) }
                        } done: {
                            model.onFocus?()
                        }
                        .frame(width: 66)
                        .help("The second of the file the keys play from")
                        Spacer(minLength: 2)
                        Text("End").font(.system(size: 10)).foregroundStyle(.secondary)
                        // Empty, the pad plays to the file's end.
                        TypedValue(text: SamplerLayout.text(sampler.endSeconds), unit: "s", clears: true) { typed in
                            if typed.isEmpty {
                                model.edit(set("end_seconds")(.absent))
                            } else if let seconds = Double(typed) {
                                model.edit(set("end_seconds")(.number(value: SamplerLayout.rounded(seconds))))
                            }
                        } done: {
                            model.onFocus?()
                        }
                        .frame(width: 66)
                        .help("The second of the file the keys play to; empty plays to its end")
                    }
                    .frame(height: 18)
                    Text(sampler.file == 0
                        ? "\(sampler.sample ?? "") cannot be read."
                        : "\(sampler.sample ?? ""), \(SamplerLayout.text(sampler.seconds)) s. Played on every key, as it is at \(sampler.rootText.isEmpty ? "middle C" : sampler.rootText).")
                        .font(.system(size: 9))
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .frame(width: Self.waveWidth)
                VStack(spacing: 2) {
                    row("Root") {
                        // Empty, the sample plays as it is at middle C.
                        TypedValue(text: sampler.rootText, unit: "", clears: true) { [key = track.key] typed in
                            model.edit(.samplerRoot(track: key, note: typed.isEmpty ? nil : typed))
                        } done: {
                            model.onFocus?()
                        }
                        .help("The note the sample is at, such as C4 or 60; empty plays it as it is at middle C")
                        Button(measuring ? "Measuring…" : "Measure") { measure() }
                            .buttonStyle(.borderless)
                            .font(.system(size: 10))
                            .disabled(measuring || sampler.file == 0)
                            .help("Set the root note to the pitch measured from the file")
                    }
                    if let mode = field("mode") {
                        row(mode.label) {
                            Picker("", selection: Binding(
                                get: { if case .text(let value) = mode.value { value } else { "one_shot" } },
                                set: { model.edit(set("mode")(.text(value: $0))) }
                            )) {
                                Text("One-shot").tag("one_shot")
                                Text("Held").tag("gate")
                            }
                            .labelsHidden()
                            .controlSize(.mini)
                            .help("One-shot plays the sample to its end; Held stops at the note's end, after the release")
                        }
                    }
                    ForEach(["transpose", "attack_ms", "release_ms", "gain_db", "pan", "reverse"], id: \.self) { name in
                        if let f = field(name) {
                            row(f.label) {
                                FieldControl(model: model, field: f, edit: set(name))
                            }
                        }
                    }
                }
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 5)
        }
    }
}

/// The waveform in SwiftUI.
private struct SamplerWave: NSViewRepresentable {
    let model: SongModel
    let track: UInt64
    let sampler: SamplerView

    func makeNSView(context: Context) -> SamplerWaveView {
        SamplerWaveView(model: model, track: track, sampler: sampler)
    }

    func updateNSView(_ view: SamplerWaveView, context: Context) {
        view.track = track
        view.sampler = sampler
    }
}

/// A file's waveform, as the host sends its peaks, with the part the pad
/// plays bright and the rest dimmed, and a marker at the start and at the
/// end. Dragging a marker moves it, no further than the file or the other
/// marker, and sends the new place when the drag ends, since each change
/// gives the sampler new voices.
final class SamplerWaveView: NSView {
    private let model: SongModel
    var track: UInt64
    var sampler: SamplerView {
        didSet {
            // The host's values have arrived.
            if let shown, drag == nil, abs(shown.start - sampler.startSeconds) < 1e-9, abs(shown.end - sampler.endSeconds) < 1e-9 {
                self.shown = nil
            }
            needsDisplay = true
            window?.invalidateCursorRects(for: self)
        }
    }

    private struct Drag {
        var marker: SamplerLayout.Marker
        var moved = false
    }

    private var drag: Drag?
    /// The markers as dragged, ahead of the host.
    private var shown: (start: Double, end: Double)?
    private var release = 0

    init(model: SongModel, track: UInt64, sampler: SamplerView) {
        self.model = model
        self.track = track
        self.sampler = sampler
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    /// The markers in effect: the dragged ones or the song's.
    var layout: SamplerLayout {
        SamplerLayout(seconds: sampler.seconds, width: bounds.width,
                      start: shown?.start ?? sampler.startSeconds, end: shown?.end ?? sampler.endSeconds)
    }

    override func resetCursorRects() {
        guard sampler.seconds > 0 else { return }
        let l = layout
        for at in [l.x(l.start), l.x(l.end)] {
            addCursorRect(CGRect(x: at - SamplerLayout.grab, y: 0, width: 2 * SamplerLayout.grab, height: bounds.height), cursor: .resizeLeftRight)
        }
    }

    override func mouseDown(with event: NSEvent) {
        let x = convert(event.locationInWindow, from: nil).x
        guard let marker = layout.marker(atX: x) else { return }
        drag = Drag(marker: marker)
    }

    override func mouseDragged(with event: NSEvent) {
        guard var d = drag else { return }
        let x = convert(event.locationInWindow, from: nil).x
        let l = layout
        let at = l.dragged(d.marker, toX: x)
        var next = (start: l.start, end: l.end)
        switch d.marker {
        case .start: next.start = at
        case .end: next.end = at
        }
        if next != (l.start, l.end) {
            d.moved = true
            shown = next
            needsDisplay = true
        }
        drag = d
    }

    override func mouseUp(with event: NSEvent) {
        guard let d = drag else { return }
        drag = nil
        guard d.moved, let shown else { return }
        let field = d.marker == .start ? "start_seconds" : "end_seconds"
        let at = d.marker == .start ? shown.start : shown.end
        // An end at the file's end is the pad playing to it.
        let value: FieldValue = d.marker == .end && at >= sampler.seconds ? .absent : .number(value: at)
        model.edit(.samplerSet(track: track, field: field, value: value))
        hold()
    }

    /// Keeps the markers as dragged until the host's song has them, or a
    /// second passes, as when the host refuses them.
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
        let box = bounds
        let shape = NSBezierPath(roundedRect: box, xRadius: 3, yRadius: 3)
        Theme.control.setFill()
        shape.fill()
        guard sampler.seconds > 0 else { return }
        let l = layout
        NSGraphicsContext.saveGraphicsState()
        shape.addClip()
        let body = box.insetBy(dx: 0, dy: 4)
        if let wave = model.fileWaveform(sampler.file), box.width > 0 {
            let columns = wave.columns(
                in: body, clippedTo: body, fromFrame: 0,
                framesPerPoint: wave.framesPerSecond * sampler.seconds / Double(box.width),
                step: 1 / (window?.backingScaleFactor ?? 2)
            )
            context.setFillColor(Theme.knob.withAlphaComponent(0.9).cgColor)
            context.fill(columns)
        } else {
            context.setFillColor(Theme.knob.withAlphaComponent(0.5).cgColor)
            context.fill(CGRect(x: box.minX, y: box.midY.rounded(), width: box.width, height: 1))
        }
        // The part outside the markers is dimmed.
        let (startX, endX) = (l.x(l.start).rounded(), l.x(l.end).rounded())
        context.setFillColor(Theme.gray(0, 0.45).cgColor)
        context.fill(CGRect(x: box.minX, y: box.minY, width: startX - box.minX, height: box.height))
        context.fill(CGRect(x: endX, y: box.minY, width: box.maxX - endX, height: box.height))
        // The markers, with a handle at the top that points inward.
        context.setFillColor(Theme.cue.cgColor)
        for (at, inward) in [(startX, CGFloat(1)), (endX, CGFloat(-1))] {
            context.fill(CGRect(x: at - 0.5, y: box.minY, width: 1, height: box.height))
            let handle = CGMutablePath()
            handle.move(to: CGPoint(x: at, y: box.minY))
            handle.addLine(to: CGPoint(x: at + inward * 6, y: box.minY))
            handle.addLine(to: CGPoint(x: at, y: box.minY + 6))
            handle.closeSubpath()
            context.addPath(handle)
            context.fillPath()
        }
        NSGraphicsContext.restoreGraphicsState()
    }
}

/// What a drop on a MIDI track's instrument panel carries: a sample from the
/// browser, which the browser remembers while it is dragged, or an audio
/// file from the Finder, read from the drag.
enum SamplerDrop {
    /// The types a drop on the panel takes: the browser's Sampler, a file
    /// from the Finder, and the browser's sample, which drags as its path.
    @MainActor static let types = [Browser.deviceType + ".sampler", UTType.fileURL.identifier, UTType.utf8PlainText.identifier, UTType.plainText.identifier]

    /// Loads what was dropped into the Sampler on `track`, or attaches the
    /// browser's empty Sampler. False for anything else.
    @MainActor
    static func land(_ providers: [NSItemProvider], model: SongModel, track: TrackView) -> Bool {
        guard let provider = providers.first else { return false }
        if provider.hasItemConformingToTypeIdentifier(Browser.deviceType + ".sampler") {
            model.addBrowserDevice("sampler", to: .track(track.key))
            return true
        }
        if let sample = model.browser.dragged {
            model.browser.dragged = nil
            model.loadSampler(path: sample.path, name: Browser.padName(of: sample), into: track.key)
            return true
        }
        guard provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) else { return false }
        provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
            guard let data = item as? Data, let url = URL(dataRepresentation: data, relativeTo: nil) else { return }
            DispatchQueue.main.async {
                MainActor.assumeIsolated { _ = model.dropOnDevices(file: url) }
            }
        }
        return true
    }
}
