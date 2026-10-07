import AAWCore
import SwiftUI

/// A song's window: the transport bar over the arrangement and the detail
/// panel, which shows the devices of the selected row or the pattern of the
/// selected clip, with the sample browser and the activity panel beside them.
struct SongView: View {
    let model: SongModel

    var body: some View {
        VStack(spacing: 0) {
            TransportBar(model: model)
            Divider()
            if let invalid = model.invalid {
                Banner(
                    icon: "exclamationmark.triangle.fill", tint: .orange,
                    text: "song.yaml was edited outside the app and does not load. The last valid song stays open, and edits wait until the file is fixed.",
                    detail: invalid
                )
            }
            HStack(spacing: 0) {
                if model.showsBrowser {
                    BrowserView(model: model)
                    Divider()
                }
                VStack(spacing: 0) {
                    ArrangementPane(model: model)
                    if model.showsDetail {
                        // Fixed here: SwiftUI takes an NSView's intrinsic
                        // height as an ideal and would share the leftover
                        // height between the strip and the arrangement.
                        DetailResizer(model: model)
                            .frame(height: DetailResizerView.thickness)
                        DetailView(model: model)
                    }
                }
                if model.showsActivity {
                    Divider()
                    ActivityPanel(model: model)
                        .frame(width: 270)
                }
            }
            // Over the arrangement, so that a refused edit does not move what
            // the person is working on.
            .overlay(alignment: .bottom) {
                if let refusal = model.refusal {
                    Banner(icon: "xmark.octagon.fill", tint: .red, text: refusal, detail: nil)
                        .background(Color(nsColor: Theme.background))
                        .transition(.opacity)
                }
            }
            .animation(.easeOut(duration: 0.2), value: model.refusal)
        }
        .background(Color(nsColor: Theme.background))
        .preferredColorScheme(.dark)
    }
}

/// The line between the arrangement and the detail panel, which is dragged
/// to make the panel taller or shorter.
private struct DetailResizer: NSViewRepresentable {
    let model: SongModel

    func makeNSView(context: Context) -> DetailResizerView {
        DetailResizerView(model: model)
    }

    func updateNSView(_ view: DetailResizerView, context: Context) {}
}

final class DetailResizerView: NSView {
    private let model: SongModel
    private var start: (y: CGFloat, height: CGFloat)?

    /// How tall the strip is to take hold of.
    static let thickness: CGFloat = 5
    /// The least the arrangement keeps above the panel.
    static let leastArrangement: CGFloat = 160

    init(model: SongModel) {
        self.model = model
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    override var intrinsicContentSize: NSSize { NSSize(width: NSView.noIntrinsicMetric, height: Self.thickness) }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .resizeUpDown)
    }

    override func mouseDown(with event: NSEvent) {
        start = (event.locationInWindow.y, model.detailHeight)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let start, let content = window?.contentView else { return }
        // Up is a taller panel: window coordinates grow upward.
        let height = start.height + (event.locationInWindow.y - start.y)
        let most = content.bounds.height - Self.leastArrangement
        model.detailHeight = min(max(height, SongModel.leastDetailHeight), max(most, SongModel.leastDetailHeight))
    }

    override func mouseUp(with event: NSEvent) {
        start = nil
    }

    override func draw(_ dirtyRect: NSRect) {
        Theme.separator.setFill()
        CGRect(x: bounds.minX, y: bounds.midY.rounded() - 0.5, width: bounds.width, height: 1).fill()
    }
}

private struct Banner: View {
    let icon: String
    let tint: Color
    let text: String
    let detail: String?

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: icon).foregroundStyle(tint)
            VStack(alignment: .leading, spacing: 2) {
                Text(text)
                if let detail {
                    Text(detail)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .lineLimit(4)
                        .textSelection(.enabled)
                }
            }
            Spacer(minLength: 0)
        }
        .font(.callout)
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .background(tint.opacity(0.14))
    }
}

struct TransportBar: View {
    let model: SongModel

    var body: some View {
        let a = model.arrangement
        let playing = model.transport.playing
        HStack(spacing: 12) {
            Button {
                model.showsBrowser.toggle()
            } label: {
                Image(systemName: "sidebar.left").frame(width: 18, height: 18)
            }
            .help("Show or hide the sample browser")

            Button {
                model.togglePlay()
            } label: {
                Image(systemName: playing ? "stop.fill" : "play.fill")
                    .frame(width: 18, height: 18)
                    .foregroundStyle(playing ? Color.green : Color.primary)
            }
            .help(playing ? "Stop (Space)" : "Play from the start position (Space)")

            Button {
                model.toggleLoop()
            } label: {
                Image(systemName: "repeat")
                    .frame(width: 18, height: 18)
                    .foregroundStyle(model.transport.loopRegion == nil ? Color.secondary : Color.yellow)
            }
            .help("Loop (L). Drag in the top strip of the ruler to set the loop.")

            Button {
                model.toggleMetronome()
            } label: {
                Image(systemName: model.transport.metronome ? "metronome.fill" : "metronome")
                    .frame(width: 18, height: 18)
                    .foregroundStyle(model.transport.metronome ? Color.yellow : Color.secondary)
            }
            .accessibilityLabel("Metronome")
            .accessibilityValue(model.transport.metronome ? "On" : "Off")
            .help("Metronome: \(model.transport.metronome ? "on" : "off"). Clicks at the session BPM during playback.")

            Text(TimelineLayout.position(model.position, beatsPerBar: a.barBeats, beatUnit: a.beatUnit))
                .font(.system(size: 15, weight: .medium).monospacedDigit())
                .frame(minWidth: 74, alignment: .leading)
                .help("Bar, beat and sixteenth")

            Divider().frame(height: 18)

            HStack(spacing: 10) {
                TempoField(model: model)
                MeterField(model: model)
                Text("\(Int((a.lengthBeats / a.barBeats).rounded(.up))) bars").foregroundStyle(.secondary)
            }
            .font(.system(size: 12).monospacedDigit())
            .foregroundStyle(model.sessionChanged ? Who.agent.color : Color.primary)
            .animation(.easeOut(duration: 0.4), value: model.sessionChanged)

            Divider().frame(height: 18)

            GridControl(model: model)

            Spacer(minLength: 8)

            if !model.warnings.isEmpty {
                Image(systemName: "info.circle")
                    .foregroundStyle(.secondary)
                    .help(model.warnings.joined(separator: "\n"))
            }
            HStack(spacing: 6) {
                Circle().fill(Who.agent.color).frame(width: 8, height: 8)
                Text("Agent editing").font(.system(size: 12))
            }
            .opacity(model.agentWorking ? 1 : 0)
            .animation(.easeOut(duration: 0.3), value: model.agentWorking)

            Button {
                model.showsActivity.toggle()
            } label: {
                Image(systemName: "sidebar.right").frame(width: 18, height: 18)
            }
            .help("Show or hide the activity panel")
        }
        .buttonStyle(.borderless)
        .focusable(false)
        .padding(.horizontal, 14)
        .frame(height: 40)
        .background(Color(nsColor: Theme.gray(0.17)))
    }
}

/// The timeline's grid in the transport bar, named as a note value, with a
/// menu that chooses it: the zoom's grid or a fixed one, Finer, Coarser,
/// Triplets and Snap to Grid, as View › Grid has them with ⌘1 to ⌘4.
private struct GridControl: View {
    let model: SongModel

    var body: some View {
        let grid = model.grid(in: .timeline)
        let bar = model.barBeats
        Menu {
            Toggle("Follow Zoom", isOn: Binding(get: { model.timelineGrid == nil }, set: { if $0 { model.timelineGrid = nil } }))
            Divider()
            ForEach(Grid.sizes(bar: bar), id: \.self) { size in
                Toggle(Grid.name(size, bar: bar), isOn: Binding(get: { Grid.size(grid, bar: bar) == size }, set: { if $0 { model.chooseGrid(size: size, in: .timeline) } }))
            }
            Divider()
            Button("Finer") { model.stepGrid(.finer, in: .timeline) }
                .disabled(model.gridStep(.finer, in: .timeline) == nil)
            Button("Coarser") { model.stepGrid(.coarser, in: .timeline) }
                .disabled(model.gridStep(.coarser, in: .timeline) == nil)
            Toggle("Triplets", isOn: Binding(get: { Grid.isTriplet(grid) }, set: { _ in model.stepGrid(.triplets, in: .timeline) }))
                .disabled(model.gridStep(.triplets, in: .timeline) == nil)
            Divider()
            Toggle("Snap to Grid", isOn: Binding(get: { model.snapsToGrid }, set: { model.snapsToGrid = $0 }))
        } label: {
            // One Text: a menu's label shows its first text alone.
            (Text("Grid ").foregroundColor(.secondary)
                + Text(model.snapsToGrid ? Grid.name(grid, bar: bar) : "\(Grid.name(grid, bar: bar)), no snap")
                    .foregroundColor(model.timelineGrid == nil ? .secondary : .primary))
                .font(.system(size: 12).monospacedDigit())
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .accessibilityLabel("Grid")
        .accessibilityValue(Grid.name(grid))
        .help("The timeline's grid, as a note value: what the start position, clips, points and dropped files land on. Gray follows the zoom; choose one to keep it. ⌘1 and ⌘2 make the grid finer and coarser, ⌘3 triplets, ⌘4 turns snapping off; with ⌘ held, a click or a drag goes off the grid.")
    }
}

/// Recent changes with who made them, newest first.
struct ActivityPanel: View {
    let model: SongModel

    private static let clock: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss"
        return f
    }()

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Activity")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 12)
                .frame(height: TimelineLayout.rulerHeight - 1, alignment: .leading)
            Divider()
            if model.activity.isEmpty {
                Text("Changes appear here as they land, with who made them. An agent's `daw` commands for this project arrive while it is open.")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .padding(12)
                Spacer(minLength: 0)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(model.activity, id: \.revision) { change in
                            row(change)
                            Divider().opacity(0.4)
                        }
                    }
                }
            }
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .background(Color(nsColor: Theme.gray(0.14)))
    }

    private func row(_ change: ChangeInfo) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 6) {
                Text(change.origin.name)
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(Color.black.opacity(0.8))
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(change.origin.color, in: RoundedRectangle(cornerRadius: 3))
                Text(Self.clock.string(from: Date(timeIntervalSince1970: change.time)))
                Spacer(minLength: 0)
                Text("rev \(change.revision)")
            }
            .font(.system(size: 10).monospacedDigit())
            .foregroundStyle(.secondary)
            Text(change.label)
                .font(.system(size: 12))
                .fixedSize(horizontal: false, vertical: true)
            ForEach(change.also, id: \.self) { also in
                Text(also)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Keeps an unfinished entry separate from updates arriving from the host.
private struct TempoField: View {
    let model: SongModel
    @State private var typed = ""
    @FocusState private var focused: Bool

    private var current: String { String(model.arrangement.tempo) }

    var body: some View {
        HStack(spacing: 4) {
            TextField("BPM", text: $typed)
                .textFieldStyle(.plain)
                .multilineTextAlignment(.trailing)
                .frame(width: 58)
                .padding(.horizontal, 5)
                .padding(.vertical, 3)
                .background(Color(nsColor: Theme.control), in: RoundedRectangle(cornerRadius: 3))
                .focused($focused)
                .accessibilityLabel("Tempo in BPM")
                .help("Tempo: 20–400 BPM. Return applies; Escape cancels.")
                .onSubmit { focused = false; model.onFocus?() }
                .onExitCommand { typed = current; focused = false; model.onFocus?() }
            Text("BPM")
        }
        .onAppear { typed = current }
        .onChange(of: model.arrangement.tempo) {
            if !focused { typed = current }
        }
        .onChange(of: focused) {
            if !focused {
                model.setTempo(typed)
                typed = current
            }
        }
    }
}

/// The time signature in the transport bar, typed as `3/4` or `6/8`.
private struct MeterField: View {
    let model: SongModel
    @State private var typed = ""
    @FocusState private var focused: Bool

    private var current: String { model.arrangement.timeSignature }

    var body: some View {
        TextField("4/4", text: $typed)
            .textFieldStyle(.plain)
            .multilineTextAlignment(.center)
            .frame(width: 36)
            .padding(.horizontal, 5)
            .padding(.vertical, 3)
            .background(Color(nsColor: Theme.control), in: RoundedRectangle(cornerRadius: 3))
            .focused($focused)
            .accessibilityLabel("Time signature")
            .help("Time signature: 1 to 32 beats over 1, 2, 4, 8 or 16, such as 3/4 or 6/8. Return applies; Escape cancels.")
            .onSubmit { focused = false; model.onFocus?() }
            .onExitCommand { typed = current; focused = false; model.onFocus?() }
            .onAppear { typed = current }
            .onChange(of: model.arrangement.timeSignature) {
                if !focused { typed = current }
            }
            .onChange(of: focused) {
                if !focused {
                    model.setTimeSignature(typed)
                    typed = current
                }
            }
    }
}
