import AppKit
import SwiftUI

/// The text view the person types to the agent in. Return sends,
/// Shift-Return starts a new line and Escape gives the keys back; it grows
/// with its text to `mostLines` and scrolls past that. An AppKit view so
/// that the next items can put a chip and a picture in it.
final class ComposerTextView: NSTextView {
    var onSend: (() -> Void)?
    var onEscape: (() -> Void)?
    var onChange: ((String) -> Void)?
    /// The height the text wants, within one to `mostLines` lines; told
    /// once when set, then at each change.
    var onHeight: ((CGFloat) -> Void)? {
        didSet {
            lastHeight = 0
            reportHeight()
        }
    }
    var placeholder = "Ask the agent…"

    static let fontSize: CGFloat = 13
    static let mostLines = 8
    static let inset = NSSize(width: 4, height: 6)

    private var lastHeight: CGFloat = 0

    /// What a key does in the field.
    enum Key: Equatable {
        case send, newline, escape, other
    }

    /// Return or Enter sends, with Shift they break the line, and Escape
    /// leaves the field.
    static func key(for event: NSEvent) -> Key {
        switch event.keyCode {
        case 36, 76: event.modifierFlags.contains(.shift) ? .newline : .send
        case 53: .escape
        default: .other
        }
    }

    /// The text view in a scroll view, over a layout manager of its own,
    /// whose used rect gives the height.
    static func make() -> (scroll: NSScrollView, text: ComposerTextView) {
        let storage = NSTextStorage()
        let layout = NSLayoutManager()
        let container = NSTextContainer(size: NSSize(width: 0, height: CGFloat.greatestFiniteMagnitude))
        layout.addTextContainer(container)
        storage.addLayoutManager(layout)
        let text = ComposerTextView(frame: .zero, textContainer: container)
        let scroll = NSScrollView()
        scroll.documentView = text
        scroll.drawsBackground = false
        scroll.borderType = .noBorder
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        text.minSize = .zero
        text.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        text.isRichText = false
        text.font = .systemFont(ofSize: fontSize)
        text.textColor = Theme.text
        text.insertionPointColor = Theme.text
        text.drawsBackground = false
        text.textContainerInset = inset
        text.isAutomaticQuoteSubstitutionEnabled = false
        text.isAutomaticDashSubstitutionEnabled = false
        text.isAutomaticTextReplacementEnabled = false
        text.allowsUndo = true
        text.isVerticallyResizable = true
        text.isHorizontallyResizable = false
        text.textContainer?.widthTracksTextView = true
        text.autoresizingMask = [.width]
        return (scroll, text)
    }

    override func keyDown(with event: NSEvent) {
        // A composition under way, as with an input method, takes Return itself.
        if hasMarkedText() { return super.keyDown(with: event) }
        switch Self.key(for: event) {
        case .send: onSend?()
        case .newline: insertText("\n", replacementRange: selectedRange())
        case .escape: onEscape?()
        case .other: super.keyDown(with: event)
        }
    }

    override func didChangeText() {
        super.didChangeText()
        onChange?(string)
        reportHeight()
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        reportHeight()
    }

    /// Sets the text from outside, as when a sent message clears it.
    func set(_ text: String) {
        guard string != text else { return }
        string = text
        reportHeight()
    }

    /// The height the text wants: its lines, one at the least and
    /// `mostLines` at the most, with the insets.
    var wantedHeight: CGFloat {
        guard let layoutManager, let textContainer, let font else { return 0 }
        layoutManager.ensureLayout(for: textContainer)
        let line = layoutManager.defaultLineHeight(for: font)
        let used = layoutManager.usedRect(for: textContainer).height
        let lines = min(max(used, line), line * CGFloat(Self.mostLines))
        return (lines + Self.inset.height * 2).rounded(.up)
    }

    private func reportHeight() {
        let height = wantedHeight
        guard height != lastHeight else { return }
        lastHeight = height
        onHeight?(height)
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard string.isEmpty, let font else { return }
        let origin = textContainerOrigin
        let padding = textContainer?.lineFragmentPadding ?? 0
        placeholder.draw(at: NSPoint(x: origin.x + padding, y: origin.y),
                         withAttributes: [.font: font, .foregroundColor: Theme.faintText])
    }
}

/// The field in SwiftUI: the text view bound to the panel's draft, with
/// its height as the text wants it.
struct ComposerField: NSViewRepresentable {
    let agent: AgentPanel
    let model: SongModel
    @Binding var height: CGFloat

    func makeNSView(context: Context) -> NSScrollView {
        let (scroll, text) = ComposerTextView.make()
        text.onSend = { [weak agent] in agent?.send() }
        text.onEscape = { [weak text, weak model] in
            text?.window?.makeFirstResponder(nil)
            model?.onFocus?()
        }
        text.onChange = { [weak agent] string in agent?.draft = string }
        text.onHeight = { height in
            DispatchQueue.main.async { self.height = height }
        }
        text.set(agent.draft)
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        (scroll.documentView as? ComposerTextView)?.set(agent.draft)
    }
}
