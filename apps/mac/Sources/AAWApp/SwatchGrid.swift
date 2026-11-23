import AppKit

/// The swatches of Color ▸ as one item of a menu: a grid of the palette's
/// colors, as Ableton's, the one in force ringed. A click chooses a color
/// and closes the menu; the color under the pointer is named in the
/// grid's tooltip.
final class SwatchGrid: NSView {
    typealias Swatch = (name: String, color: NSColor, checked: Bool, choice: NSObject)

    static let columns = 12
    static let side: CGFloat = 18
    static let gap: CGFloat = 4
    static let inset = NSEdgeInsets(top: 4, left: 14, bottom: 6, right: 14)

    private let swatches: [Swatch]
    private let choose: (NSObject) -> Void
    private var hovered: Int? {
        didSet {
            guard hovered != oldValue else { return }
            toolTip = hovered.map { swatches[$0].name.capitalized }
            needsDisplay = true
        }
    }

    init(swatches: [Swatch], choose: @escaping (NSObject) -> Void) {
        self.swatches = swatches
        self.choose = choose
        let rows = (swatches.count + Self.columns - 1) / Self.columns
        let width = Self.inset.left + Self.inset.right + CGFloat(Self.columns) * Self.side + CGFloat(Self.columns - 1) * Self.gap
        let height = Self.inset.top + Self.inset.bottom + CGFloat(rows) * Self.side + CGFloat(max(rows - 1, 0)) * Self.gap
        super.init(frame: NSRect(x: 0, y: 0, width: width, height: height))
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeAlways, .inVisibleRect],
                                       owner: self, userInfo: nil))
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    override var isFlipped: Bool { true }

    /// Where the swatch at `index` is drawn: across, then down.
    func rect(of index: Int) -> NSRect {
        let column = index % Self.columns, row = index / Self.columns
        return NSRect(x: Self.inset.left + CGFloat(column) * (Self.side + Self.gap),
                      y: Self.inset.top + CGFloat(row) * (Self.side + Self.gap), width: Self.side, height: Self.side)
    }

    /// The swatch under a point, with half a gap around each.
    func index(at p: NSPoint) -> Int? {
        swatches.indices.first { rect(of: $0).insetBy(dx: -Self.gap / 2, dy: -Self.gap / 2).contains(p) }
    }

    override func draw(_ dirtyRect: NSRect) {
        for (i, swatch) in swatches.enumerated() {
            let r = rect(of: i)
            swatch.color.setFill()
            NSBezierPath(roundedRect: r, xRadius: 3, yRadius: 3).fill()
            if swatch.checked || hovered == i {
                let ring = NSBezierPath(roundedRect: r.insetBy(dx: -2, dy: -2), xRadius: 4, yRadius: 4)
                ring.lineWidth = swatch.checked ? 2 : 1.5
                (swatch.checked ? NSColor.white : NSColor.white.withAlphaComponent(0.6)).setStroke()
                ring.stroke()
            }
        }
    }

    override func mouseMoved(with event: NSEvent) {
        hovered = index(at: convert(event.locationInWindow, from: nil))
    }

    override func mouseExited(with event: NSEvent) {
        hovered = nil
    }

    override func mouseUp(with event: NSEvent) {
        guard let i = index(at: convert(event.locationInWindow, from: nil)) else { return }
        // The whole menu closes, the submenu and the one it opened from.
        var menu = enclosingMenuItem?.menu
        while let parent = menu?.supermenu { menu = parent }
        menu?.cancelTracking()
        choose(swatches[i].choice)
    }
}
