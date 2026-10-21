import AppKit
import SwiftUI

/// The text view the person types to the agent in. Return sends,
/// Shift-Return starts a new line and Escape gives the keys back; it grows
/// with its text to `mostLines` and scrolls past that. An AppKit view so
/// that a chip and a picture can be put in it. A skill chosen under `/` is
/// a chip: its `/name` at the start drawn on a rounded fill in the person's
/// color, taken whole by Backspace, the caret kept after it, and the
/// skill's argument hint after it until the person types. A thing of the
/// song named under `@` is a chip the same way, `@drums`, wherever it
/// stands in the text.
final class ComposerTextView: NSTextView {
    var onSend: (() -> Void)?
    var onEscape: (() -> Void)?
    var onChange: ((String) -> Void)?
    /// Where the caret is, in UTF-16 units, after each change and move.
    var onCaret: ((Int) -> Void)?
    /// A key for the list of skills over the field; true when the list
    /// took it.
    var onListKey: ((ListKey) -> Bool)?
    /// ⌘V with the pasteboard; true when what is on it was attached
    /// rather than typed: files, or an image.
    var onPaste: ((NSPasteboard) -> Bool)?
    /// The height the text wants, within one to `mostLines` lines; told
    /// once when set, then at each change.
    var onHeight: ((CGFloat) -> Void)? {
        didSet {
            lastHeight = 0
            reportHeight()
        }
    }
    var placeholder = "Create or edit this session"
    /// The chosen skill's name, drawn as a chip while the text starts with
    /// `/name`, and what its words are, drawn after the chip until typed.
    var chip: String? {
        didSet {
            guard chip != oldValue else { return }
            colorText()
            needsDisplay = true
        }
    }
    var hint = ""
    /// The names of the song's things the text mentions with `@`, each
    /// `@name` drawn as a chip.
    var mentions: Set<String> = [] {
        didSet {
            guard mentions != oldValue else { return }
            colorText()
            needsDisplay = true
        }
    }
    /// The characters still uncertain while the person speaks, drawn in
    /// the faint color until the recognizer firms them up.
    var faintRange: NSRange? {
        didSet {
            guard faintRange != oldValue else { return }
            colorText()
            needsDisplay = true
        }
    }

    static let fontSize: CGFloat = 13
    static let mostLines = 8
    static let inset = NSSize(width: 4, height: 6)
    /// The chip's fill, and its text.
    static let chipFill = Theme.color(of: .user)
    static let chipText = NSColor.white

    private var lastHeight: CGFloat = 0

    /// What a key does in the field.
    enum Key: Equatable {
        case send, newline, escape, other
    }

    /// What a key does in the list of skills.
    enum ListKey: Equatable {
        case up, down, choose, close
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

    /// While the list shows: ↑ and ↓ move, Return and Tab choose, Escape
    /// closes it.
    static func listKey(for event: NSEvent) -> ListKey? {
        switch event.keyCode {
        case 126: .up
        case 125: .down
        case 36, 76, 48: event.modifierFlags.contains(.shift) ? nil : .choose
        case 53: .close
        default: nil
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
        if let key = Self.listKey(for: event), onListKey?(key) == true { return }
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
        onCaret?(selectedRange().location)
        colorText()
        reportHeight()
    }

    /// Files and images pasted attach to the message; text pastes as text.
    override func paste(_ sender: Any?) {
        if onPaste?(NSPasteboard.general) == true { return }
        super.paste(sender)
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        reportHeight()
    }

    /// Sets the text from outside, as when a sent message clears it or a
    /// skill is chosen, with the caret at its end, or where `caret` says.
    func set(_ text: String, caret: Int? = nil) {
        guard string != text else {
            if let caret { setSelectedRange(NSRange(location: min(caret, (text as NSString).length), length: 0)) }
            return
        }
        string = text
        setSelectedRange(NSRange(location: min(caret ?? Int.max, (text as NSString).length), length: 0))
        colorText()
        reportHeight()
    }

    // MARK: The chips

    /// The characters of the skill's chip: `/name` at the start, while the
    /// text goes on with a space, a line or nothing.
    var chipRange: NSRange? {
        guard let chip, AgentCommands.split(string)?.name == chip else { return nil }
        return NSRange(location: 0, length: ("/" + chip as NSString).length)
    }

    /// The characters of every `@name` chip.
    var mentionRanges: [NSRange] {
        AgentMentions.found(in: string, among: mentions).map(\.range)
    }

    /// Every chip's characters: the skill's, then the mentions in order.
    var chipRanges: [NSRange] {
        (chipRange.map { [$0] } ?? []) + mentionRanges
    }

    /// Backspace at a chip's end takes the whole chip.
    override func deleteBackward(_ sender: Any?) {
        if let range = chipRanges.first(where: { selectedRange() == NSRange(location: $0.upperBound, length: 0) }) {
            setSelectedRange(range)
        }
        super.deleteBackward(sender)
    }

    /// A caret put inside a chip lands after it.
    override func setSelectedRanges(_ ranges: [NSValue], affinity: NSSelectionAffinity, stillSelecting: Bool) {
        var ranges = ranges
        if ranges.count == 1, let caret = ranges.first?.rangeValue, caret.length == 0,
           let range = chipRanges.first(where: { caret.location > $0.location && caret.location < $0.upperBound }) {
            ranges = [NSValue(range: NSRange(location: range.upperBound, length: 0))]
        }
        super.setSelectedRanges(ranges, affinity: affinity, stillSelecting: stillSelecting)
        if !stillSelecting { onCaret?(selectedRange().location) }
    }

    /// The chips' text in white and the uncertain spoken tail in the faint
    /// color, as temporary attributes the storage does not keep.
    private func colorText() {
        guard let layoutManager else { return }
        let length = (string as NSString).length
        let whole = NSRange(location: 0, length: length)
        layoutManager.removeTemporaryAttribute(.foregroundColor, forCharacterRange: whole)
        for range in chipRanges {
            layoutManager.addTemporaryAttribute(.foregroundColor, value: Self.chipText, forCharacterRange: range)
        }
        if let range = faintRange {
            let clipped = NSIntersectionRange(range, whole)
            if clipped.length > 0 {
                layoutManager.addTemporaryAttribute(.foregroundColor, value: Theme.faintText, forCharacterRange: clipped)
            }
        }
    }

    /// The color the characters are drawn in, for tests: the text's, the
    /// chip's or the faint one.
    func color(at index: Int) -> NSColor? {
        guard let layoutManager, index < (string as NSString).length else { return nil }
        return layoutManager.temporaryAttribute(.foregroundColor, atCharacterIndex: index, effectiveRange: nil) as? NSColor ?? textColor
    }

    /// Where the skill's chip is drawn, in the view.
    var chipRect: NSRect? {
        chipRange.map(rect)
    }

    /// Where every chip is drawn, in the view.
    var chipRects: [NSRect] {
        chipRanges.map(rect)
    }

    private func rect(_ range: NSRange) -> NSRect {
        guard let layoutManager, let textContainer else { return .zero }
        let glyphs = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        var rect = layoutManager.boundingRect(forGlyphRange: glyphs, in: textContainer)
        rect.origin.x += textContainerOrigin.x
        rect.origin.y += textContainerOrigin.y
        return rect.insetBy(dx: -3, dy: 0.5)
    }

    /// Whether the hint still shows: nothing typed after the chip.
    var showsHint: Bool {
        guard chip != nil, !hint.isEmpty, let split = AgentCommands.split(string) else { return false }
        return split.rest.trimmingCharacters(in: .whitespaces).isEmpty
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

    /// The chips' fills go under the text, which the view draws over
    /// nothing of its own.
    override func draw(_ dirtyRect: NSRect) {
        Self.chipFill.setFill()
        for rect in chipRects {
            NSBezierPath(roundedRect: rect, xRadius: 4, yRadius: 4).fill()
        }
        super.draw(dirtyRect)
        guard let font else { return }
        if string.isEmpty {
            let origin = textContainerOrigin
            let padding = textContainer?.lineFragmentPadding ?? 0
            placeholder.draw(at: NSPoint(x: origin.x + padding, y: origin.y),
                             withAttributes: [.font: font, .foregroundColor: Theme.faintText])
        } else if showsHint, let rect = chipRect {
            hint.draw(at: NSPoint(x: rect.maxX + 6, y: rect.minY + 0.5),
                      withAttributes: [.font: font, .foregroundColor: Theme.faintText])
        }
    }
}

/// The field in SwiftUI: the text view bound to the panel's draft and its
/// chip, with its height as the text wants it, and the list's keys handed
/// to the panel.
struct ComposerField: NSViewRepresentable {
    let agent: AgentPanel
    let model: SongModel
    @Binding var height: CGFloat

    func makeNSView(context: Context) -> NSScrollView {
        let (scroll, text) = ComposerTextView.make()
        text.onSend = { [weak agent] in agent?.send() }
        text.onEscape = { [weak agent, weak text, weak model] in
            // While the person speaks, Escape takes the spoken words out.
            if agent?.cancelListening() == true { return }
            text?.window?.makeFirstResponder(nil)
            model?.onFocus?()
        }
        text.onChange = { [weak agent] string in agent?.draft = string }
        text.onCaret = { [weak agent] caret in agent?.caret = caret }
        text.onListKey = { [weak agent] key in
            guard let agent else { return false }
            switch key {
            case .up: return agent.moveSelection(by: -1)
            case .down: return agent.moveSelection(by: 1)
            case .choose: return agent.chooseSelected()
            case .close: return agent.closeList()
            }
        }
        text.onPaste = { [weak agent] board in agent?.attach(pasteboard: board) ?? false }
        text.onHeight = { height in
            DispatchQueue.main.async { self.height = height }
        }
        apply(to: text, context.coordinator)
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        if let text = scroll.documentView as? ComposerTextView { apply(to: text, context.coordinator) }
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    /// Remembers which caret request the field has put, so that one is
    /// put once.
    final class Coordinator {
        var appliedCaret = 0
    }

    private func apply(to text: ComposerTextView, _ coordinator: Coordinator) {
        text.hint = agent.chip?.hint ?? ""
        text.chip = agent.chip?.name
        var caret: Int?
        if let request = agent.caretRequest, request.id != coordinator.appliedCaret {
            caret = request.caret
            coordinator.appliedCaret = request.id
        }
        text.set(agent.draft, caret: caret)
        text.mentions = agent.mentionNames
        text.faintRange = agent.uncertainRange
    }
}
