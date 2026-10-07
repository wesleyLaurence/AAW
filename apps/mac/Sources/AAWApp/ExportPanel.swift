import AppKit
import SwiftUI

/// The options the Export Audio panel is choosing, which its accessory view
/// edits and the panel reads when it closes.
@MainActor
@Observable
final class ExportChoice {
    var options: ExportOptions {
        didSet {
            if options.format != oldValue.format { onFormat?(options.format) }
        }
    }

    /// Called when the format changes, for the panel to follow it.
    @ObservationIgnored var onFormat: ((ExportOptions.Format) -> Void)?

    init(options: ExportOptions) {
        self.options = options
    }
}

/// Under the name in the Export Audio panel: the format, the level and the
/// ceiling, as `daw export` takes them.
struct ExportAccessory: View {
    @Bindable var choice: ExportChoice

    /// The level policies the menu offers, in order.
    static let policies: [(String, String)] = [
        ("rendered", "As rendered"), ("peak", "True peak at"), ("lufs", "Loudness"), ("gain", "Gain"),
    ]

    private var policy: Binding<String> {
        Binding(
            get: { choice.options.level.policy },
            set: { policy in
                // The value kept when the policy changes between those with
                // one, so a number typed is not lost.
                let value = choice.options.level.value
                let same = policy == choice.options.level.policy
                if let level = ExportOptions.Level.make(policy, same ? value : nil) { choice.options.level = level }
            }
        )
    }

    private var value: Binding<Double> {
        Binding(
            get: { choice.options.level.value ?? 0 },
            set: { value in
                if let level = ExportOptions.Level.make(choice.options.level.policy, value) { choice.options.level = level }
            }
        )
    }

    private static let number: NumberFormatter = {
        let f = NumberFormatter()
        f.numberStyle = .decimal
        f.maximumFractionDigits = 2
        f.minimumFractionDigits = 0
        return f
    }()

    var body: some View {
        // The app's own `Grid` is the timeline's.
        SwiftUI.Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 8) {
            GridRow {
                Text("Format:").gridColumnAlignment(.trailing)
                Picker("Format", selection: $choice.options.format) {
                    ForEach(ExportOptions.Format.allCases, id: \.self) { format in
                        Text(format.name).tag(format)
                    }
                }
                .labelsHidden()
                .frame(width: 150)
                .accessibilityLabel("Format")
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
                    if choice.options.level != .asRendered {
                        TextField("Value", value: value, formatter: Self.number)
                            .frame(width: 64)
                            .multilineTextAlignment(.trailing)
                            .accessibilityLabel("Level value")
                        Text(choice.options.level.unit).foregroundStyle(.secondary)
                    }
                }
            }
            if choice.options.level != .asRendered {
                GridRow {
                    Text("Ceiling:").gridColumnAlignment(.trailing)
                    HStack(spacing: 6) {
                        TextField("Ceiling", value: $choice.options.ceiling, formatter: Self.number)
                            .frame(width: 64)
                            .multilineTextAlignment(.trailing)
                            .accessibilityLabel("Ceiling")
                        Text("dBFS").foregroundStyle(.secondary)
                        Text("The highest sample peak the gain may reach; the export does not limit.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
        .controlSize(.small)
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
        .frame(width: 560, alignment: .leading)
    }
}
