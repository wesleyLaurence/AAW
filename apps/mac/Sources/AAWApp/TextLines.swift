import AppKit
import CoreText

/// Lines of text laid out once and drawn many times. A timeline draws the
/// same few hundred names and numbers every frame it scrolls or zooms, and
/// laying a string out costs far more than drawing it.
@MainActor
final class TextLines {
    static let shared = TextLines()

    private struct Key: Hashable {
        var string: String
        var font: ObjectIdentifier
    }

    private struct Line {
        var line: CTLine
        var width: CGFloat
    }

    private var lines: [Key: Line] = [:]
    /// Values that change as they are dragged leave lines behind; past this
    /// many the lines are laid out afresh.
    private static let limit = 4000

    private func line(_ string: String, font: NSFont) -> Line {
        let key = Key(string: string, font: ObjectIdentifier(font))
        if let line = lines[key] { return line }
        // The color is the context's when the line is drawn, so one line
        // serves every color and every step of a fade.
        let attributed = NSAttributedString(string: string, attributes: [
            .font: font, NSAttributedString.Key(kCTForegroundColorFromContextAttributeName as String): true,
        ])
        let made = CTLineCreateWithAttributedString(attributed)
        let line = Line(line: made, width: CGFloat(CTLineGetTypographicBounds(made, nil, nil, nil)))
        if lines.count >= Self.limit { lines.removeAll(keepingCapacity: true) }
        lines[key] = line
        return line
    }

    /// Draws one line of text at the top of `rect` in the current context,
    /// which is a flipped view's. Text wider than `rect` is cut at its edge
    /// when `cuts`; otherwise nothing is drawn and the result is false, for
    /// the caller to draw it shortened.
    func draw(_ string: String, in rect: CGRect, font: NSFont, color: NSColor, align: NSTextAlignment = .left,
              cuts: Bool = false) -> Bool {
        guard let context = NSGraphicsContext.current?.cgContext else { return false }
        let line = line(string, font: font)
        guard cuts || line.width <= rect.width else { return false }
        var x = rect.minX
        switch align {
        case .right: x = rect.maxX - line.width
        case .center: x = rect.midX - line.width / 2
        default: break
        }
        if cuts, line.width > rect.width {
            context.saveGState()
            context.clip(to: rect.insetBy(dx: 0, dy: -4))
        }
        context.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
        context.setFillColor(color.cgColor)
        context.textPosition = CGPoint(x: max(x, rect.minX), y: rect.minY + font.ascender.rounded())
        CTLineDraw(line.line, context)
        if cuts, line.width > rect.width { context.restoreGState() }
        return true
    }
}
