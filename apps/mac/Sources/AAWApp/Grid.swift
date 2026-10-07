import Foundation

/// The grids an editor draws and snaps to, in beats as the song writes them
/// and named as note values, a beat being a quarter note. One list for the
/// timeline, the piano roll and the pattern's step, and the steps the Grid
/// menu takes through it: finer, coarser and triplets.
public enum Grid {
    /// From a bar to a sixty-fourth note, each triplet after its note.
    public static let values = ["4", "2", "4/3", "1", "2/3", "1/2", "1/3", "1/4", "1/6", "1/8", "1/12", "1/16"]
    /// The steps a pattern can have: a beat to a sixty-fourth note.
    public static let patternValues = ["1", "1/2", "1/3", "1/4", "1/6", "1/8", "1/12", "1/16"]
    /// The sizes the menu lists; Triplets makes each its triplet.
    public static var sizes: [String] { values.filter { !isTriplet($0) } }

    /// The name of a value: `1/16`, `1/8T`, `1 Bar`.
    public static func name(_ value: String) -> String { PianoRollLayout.noteValue(value) }

    public static func isTriplet(_ value: String) -> Bool { name(value).hasSuffix("T") }

    /// A grid in beats as the song writes it: a value of the list, else a
    /// whole number of beats, else the beats as typed.
    public static func text(_ beats: Double) -> String {
        if let value = values.first(where: { near(PianoRollLayout.beats($0), beats) }) { return value }
        if abs(beats - beats.rounded()) < 1e-9 { return String(Int(beats.rounded())) }
        return ValueScale.plain(beats)
    }

    private static func near(_ a: Double?, _ b: Double) -> Bool {
        guard let a else { return false }
        return abs(a - b) < 1e-9
    }

    /// The next value of `list` finer than `value` of its kind, triplet or
    /// straight; nil at the finest.
    public static func finer(_ value: String, in list: [String] = values) -> String? {
        guard let beats = PianoRollLayout.beats(value) else { return nil }
        let triplet = isTriplet(value)
        return list.first { isTriplet($0) == triplet && (PianoRollLayout.beats($0) ?? .infinity) < beats - 1e-9 }
    }

    /// The next value of `list` coarser than `value` of its kind; nil at the
    /// coarsest.
    public static func coarser(_ value: String, in list: [String] = values) -> String? {
        guard let beats = PianoRollLayout.beats(value) else { return nil }
        let triplet = isTriplet(value)
        return list.last { isTriplet($0) == triplet && (PianoRollLayout.beats($0) ?? 0) > beats + 1e-9 }
    }

    /// The triplet of a straight value, or the straight value of a triplet:
    /// `1/4` and `1/6`. Nil when `list` lacks it.
    public static func triplets(_ value: String, _ on: Bool, in list: [String] = values) -> String? {
        guard let beats = PianoRollLayout.beats(value) else { return nil }
        if isTriplet(value) == on { return list.contains(value) ? value : nil }
        let wanted = on ? beats * 2 / 3 : beats * 3 / 2
        return list.first { near(PianoRollLayout.beats($0), wanted) }
    }

    /// The size a value is listed under: itself, or a triplet's straight value.
    public static func size(_ value: String) -> String {
        triplets(value, false) ?? value
    }

    /// The value chosen when `size` is picked while the grid is `current`: the
    /// size, as a triplet when the grid is one and `list` has it.
    public static func choose(_ size: String, keeping current: String, in list: [String] = values) -> String {
        isTriplet(current) ? (triplets(size, true, in: list) ?? size) : size
    }
}
