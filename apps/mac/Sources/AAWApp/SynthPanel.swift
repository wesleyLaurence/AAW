import AAWCore
import SwiftUI

/// A MIDI track's Synth, first in its chain: every part of the patch as a
/// column of controls drawn from its fields as an effect's panel is, the
/// Synth's own fields, each oscillator, the filter, each envelope, each LFO,
/// the macros and the matrix. No drawing of a wave, a curve or an envelope
/// yet: that is the Synth's panel item. A control's change is an edit to the
/// host, as `daw synth set` is.
struct SynthPanel: View {
    let model: SongModel
    let track: TrackView
    let synth: SynthView

    /// A column's width, and the gap between columns.
    static let column: CGFloat = 188
    static let gap: CGFloat = 6

    /// How wide the panel is: a column for each part.
    static func width(_ synth: SynthView) -> CGFloat {
        let columns = 2 + synth.oscillators.count + synth.envelopes.count + synth.lfos.count + (synth.macros.isEmpty ? 0 : 1) + 1
        return CGFloat(columns) * (column + gap) + gap
    }

    private func set(_ field: String) -> (FieldValue) -> Edit {
        { [key = track.key] value in .synthSet(track: key, field: field, value: value) }
    }

    private func row(_ field: FieldView) -> some View {
        HStack(spacing: 4) {
            Text(field.label)
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .frame(width: 58, alignment: .leading)
                .lineLimit(1)
            FieldControl(model: model, field: field, edit: set(field.name))
            SynthLaneMark(model: model, track: track, field: field)
        }
        .frame(height: 18)
    }

    private func column<Content: View>(_ title: String, @ViewBuilder _ content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(height: 14)
            content()
            Spacer(minLength: 0)
        }
        .frame(width: Self.column, alignment: .top)
    }

    private func fields(_ title: String, _ fields: [FieldView]) -> some View {
        column(title) {
            ForEach(fields, id: \.name) { field in
                row(field)
            }
        }
    }

    var body: some View {
        HStack(alignment: .top, spacing: Self.gap) {
            fields("Synth", synth.fields)
            ForEach(synth.oscillators, id: \.name) { group in
                fields("Osc \(group.name)", group.fields)
            }
            fields("Filter", synth.filter)
            ForEach(synth.envelopes, id: \.name) { group in
                fields("Env \(group.name)", group.fields)
            }
            ForEach(synth.lfos, id: \.name) { group in
                fields("LFO \(group.name)", group.fields)
            }
            if !synth.macros.isEmpty {
                fields("Macros", synth.macros)
            }
            column("Matrix") {
                if synth.modulation.isEmpty {
                    Text("No modulation. The agent adds entries with `daw synth mod`.")
                        .font(.system(size: 9))
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                ScrollView {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(Array(synth.modulation.enumerated()), id: \.offset) { index, entry in
                            HStack(spacing: 3) {
                                Text("\(entry.source) → \(entry.target)")
                                    .font(.system(size: 9))
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                                Spacer(minLength: 2)
                                Text("\(ValueScale.plain(entry.amount)) \(entry.unit)")
                                    .font(.system(size: 9).monospacedDigit())
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
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
                }
            }
        }
        .padding(.horizontal, Self.gap)
        .padding(.vertical, 5)
    }
}

/// The mark beside a Synth field that automation can move, as `LaneMark`
/// is for an effect's: filled while a lane moves it, and a click adds the
/// lane under the track, or removes it.
private struct SynthLaneMark: View {
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
