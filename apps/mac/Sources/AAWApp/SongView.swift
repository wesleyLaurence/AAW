import AAWCore
import SwiftUI

/// A song's window: the transport bar over the arrangement, with the activity
/// panel beside it.
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
                ArrangementPane(model: model)
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

            Text(TimelineLayout.position(model.position, beatsPerBar: Double(a.beatsPerBar)))
                .font(.system(size: 15, weight: .medium).monospacedDigit())
                .frame(minWidth: 74, alignment: .leading)
                .help("Bar, beat and sixteenth")

            Divider().frame(height: 18)

            HStack(spacing: 10) {
                Text("\(a.tempo.formatted(.number.precision(.fractionLength(0...2)))) BPM")
                Text("\(a.beatsPerBar)/4").foregroundStyle(.secondary)
                Text("\(Int((a.lengthBeats / Double(a.beatsPerBar)).rounded(.up))) bars").foregroundStyle(.secondary)
            }
            .font(.system(size: 12).monospacedDigit())
            .foregroundStyle(model.sessionChanged ? Who.agent.color : Color.primary)
            .animation(.easeOut(duration: 0.4), value: model.sessionChanged)

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
                Text("Changes appear here as they land, with who made them. An agent's `daw` commands for this song arrive while it is open.")
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
