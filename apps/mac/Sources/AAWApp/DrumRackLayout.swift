import CoreGraphics
import Foundation

/// Where a Drum Rack's keys are, as Ableton's Drum Rack lays them out: a
/// grid of 4 × 4 pads showing sixteen keys from a bank's first, the lowest
/// at the bottom left, and an overview of all 128 keys, four to a row from
/// 0 at the bottom, with a frame on the sixteen the grid shows. Geometry
/// only; the pads live in the song.
public struct DrumRackLayout: Equatable {
    public static let columns = 4
    public static let rows = 4
    /// The keys the grid shows at once.
    public static let shown = columns * rows
    /// The overview's rows: 128 keys, four to a row.
    public static let overviewRows = 128 / columns
    /// The highest first key of a bank: the grid's top row is the top of
    /// the overview's.
    public static let lastBank = 128 - shown
    /// The bank the grid opens on: C2 to D♯3, the kick at the bottom left,
    /// as the General MIDI drum notes are mapped.
    public static let defaultBank = 36

    /// The first key the grid shows, a multiple of 4.
    public var bank: Int

    public init(bank: Int = Self.defaultBank) {
        self.bank = Self.clamped(bank)
    }

    /// A first key moved onto a row of the overview, and inside it.
    public static func clamped(_ bank: Int) -> Int {
        min(max(bank - ((bank % columns) + columns) % columns, 0), lastBank)
    }

    /// The bank a Drum Rack opens on: the default when a pad is there or
    /// none is anywhere, else the row of the lowest key with a pad.
    public static func opening(keys: [Int]) -> Int {
        guard let lowest = keys.min(), !keys.contains(where: { (defaultBank..<defaultBank + shown).contains($0) }) else {
            return defaultBank
        }
        return clamped(lowest)
    }

    /// The bank that shows `key`, moved as little as it must from this one.
    public func showing(_ key: Int) -> Int {
        if (bank..<bank + Self.shown).contains(key) { return bank }
        return key < bank ? Self.clamped(key) : Self.clamped(key - Self.shown + Self.columns)
    }

    /// The key of the grid's pad in `row`, counted from the top, and
    /// `column`, from the left.
    public func key(row: Int, column: Int) -> Int {
        bank + (Self.rows - 1 - row) * Self.columns + column
    }

    /// Each row of the grid from the top, its keys from the left.
    public var grid: [[Int]] {
        (0..<Self.rows).map { row in (0..<Self.columns).map { key(row: row, column: $0) } }
    }

    // MARK: The overview

    /// The overview's cell of `key` in a box of `size`: column by its place
    /// in a row of 4, row from the top for the highest keys.
    public static func cell(_ key: Int, in size: CGSize) -> CGRect {
        let w = size.width / CGFloat(columns)
        let h = size.height / CGFloat(overviewRows)
        let row = overviewRows - 1 - key / columns
        return CGRect(x: CGFloat(key % columns) * w, y: CGFloat(row) * h, width: w, height: h)
    }

    /// The frame on the keys the grid shows, in a box of `size`.
    public func frame(in size: CGSize) -> CGRect {
        let top = Self.cell(bank + Self.shown - 1, in: size)
        let bottom = Self.cell(bank, in: size)
        return CGRect(x: 0, y: top.minY, width: size.width, height: bottom.maxY - top.minY)
    }

    /// The bank a click or a drag at `y` in the overview moves to: the
    /// frame centered on the row under the pointer, as far as it goes.
    public static func bank(atY y: CGFloat, height: CGFloat) -> Int {
        guard height > 0 else { return defaultBank }
        let row = Int((y / height * CGFloat(overviewRows)).rounded(.down))
        let key = (overviewRows - 1 - min(max(row, 0), overviewRows - 1)) * columns
        // The pointer's row is the frame's second from the bottom.
        return clamped(key - columns)
    }
}
