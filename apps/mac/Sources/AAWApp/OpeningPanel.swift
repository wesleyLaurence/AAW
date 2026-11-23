import AppKit

/// Says a project is opening while its host loads it, which takes a while
/// for a large song: its name over a moving bar, and Cancel, which leaves
/// the project closed.
@MainActor
final class OpeningPanel: NSObject {
    private let panel: NSPanel
    private let cancelled: @MainActor () -> Void

    init(name: String, cancel: @escaping @MainActor () -> Void) {
        cancelled = cancel
        panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 360, height: 104),
                        styleMask: [.titled], backing: .buffered, defer: true)
        super.init()
        panel.title = "Opening"
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false

        let label = NSTextField(labelWithString: "Opening “\(name)”…")
        label.lineBreakMode = .byTruncatingMiddle
        let bar = NSProgressIndicator()
        bar.style = .bar
        bar.isIndeterminate = true
        bar.startAnimation(nil)
        let button = NSButton(title: "Cancel", target: self, action: #selector(cancelPressed))
        button.keyEquivalent = "\u{1b}"

        let content = NSView()
        for view in [label, bar, button] as [NSView] {
            view.translatesAutoresizingMaskIntoConstraints = false
            content.addSubview(view)
        }
        NSLayoutConstraint.activate([
            label.topAnchor.constraint(equalTo: content.topAnchor, constant: 16),
            label.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 20),
            label.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -20),
            bar.topAnchor.constraint(equalTo: label.bottomAnchor, constant: 10),
            bar.leadingAnchor.constraint(equalTo: label.leadingAnchor),
            bar.trailingAnchor.constraint(equalTo: label.trailingAnchor),
            button.topAnchor.constraint(equalTo: bar.bottomAnchor, constant: 12),
            button.trailingAnchor.constraint(equalTo: label.trailingAnchor),
            button.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -16),
        ])
        panel.contentView = content
    }

    func show() {
        panel.center()
        panel.makeKeyAndOrderFront(nil)
    }

    func close() {
        panel.orderOut(nil)
    }

    @objc private func cancelPressed() {
        cancelled()
    }
}
