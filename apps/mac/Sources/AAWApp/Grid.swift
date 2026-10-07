import Foundation

/// The grids an editor draws and snaps to, in beats as the song writes them
/// and named as note values, a beat being a quarter note. One list for the
/// timeline, the piano roll and the pattern's step, and the steps the Grid
/// menu takes through it: finer, coarser and triplets. The list's bar is
/// the song's, from its time signature: `bar` is a bar in beats.
public enum Grid {
    /// From a bar of 4/4 to a sixty-fourth note, each triplet after its note.
    public static let values = ["4", "2", "4/3", "1", "2/3", "1/2", "1/3", "1/4", "1/6", "1/8", "1/12", "1/16"]
    /// The steps a pattern can have: a beat to a sixty-fourth note.
    public static let patternValues = ["1", "1/2", "1/3", "1/4", "1/6", "1/8", "1/12", "1/16"]

    /// The values for a song whose bar is `bar` beats: that bar in place of
    /// the 4/4 bar, from the coarsest.
    public static func list(bar: Double = 4) -> [String] {
        var out = values.filter { $0 != "4" }
        let whole = text(bar)
        if !out.contains(whole) { out.append(whole) }
        return out.sorted { (PianoRollLayout.beats($0) ?? 0) > (PianoRollLayout.beats($1) ?? 0) }
    }

    /// The sizes the menu lists; Triplets makes each its triplet.
    public static func sizes(bar: Double = 4) -> [String] { list(bar: bar).filter { !isTriplet($0) } }

    /// The name of a value: `1/16`, `1/8T`, `1 Bar`.
    public static func name(_ value: String, bar: Double = 4) -> String { PianoRollLayout.noteValue(value, bar: bar) }

    public static func isTriplet(_ value: String) -> Bool { PianoRollLayout.noteValue(value).hasSuffix("T") }

    /// A grid in beats as the song writes it: a value of the list, else a
    /// whole number of beats, else the beats as typed.
    public static func text(_ beats: Double) -> String {
        if let value = values.first(where: { near(PianoRollLayout.beats($0), beats) }) { return value }
        if abs(beats - beats.rounded()) < 1e-9 { return String(Int(beats.rounded())) }
        if abs(beats * 2 - (beats * 2).rounded()) < 1e-9 { return "\(Int((beats * 2).rounded()))/2" }
        return ValueScale.plain(beats)
    }

    private static func near(_ a: Double?, _ b: Double) -> Bool {
        guard let a else { return false }
        return abs(a - b) < 1e-9
    }

    /// Whether `beats` is a whole number of bars of `bar`.
    private static func isBars(_ beats: Double, bar: Double) -> Bool {
        let n = beats / bar
        return n >= 1 - 1e-9 && abs(n - n.rounded()) < 1e-9
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
    /// `1/4` and `1/6`. Nil when `list` lacks it. A bar, or bars, of `bar`
    /// beats has no triplet.
    public static func triplets(_ value: String, _ on: Bool, in list: [String] = values, bar: Double = 4) -> String? {
        guard let beats = PianoRollLayout.beats(value) else { return nil }
        if isBars(beats, bar: bar) { return on ? nil : (list.contains(value) ? value : nil) }
        if isTriplet(value) == on { return list.contains(value) ? value : nil }
        let wanted = on ? beats * 2 / 3 : beats * 3 / 2
        return list.first { near(PianoRollLayout.beats($0), wanted) }
    }

    /// The size a value is listed under: itself, or a triplet's straight value.
    public static func size(_ value: String, bar: Double = 4) -> String {
        triplets(value, false, bar: bar) ?? value
    }

    /// The value chosen when `size` is picked while the grid is `current`: the
    /// size, as a triplet when the grid is one and `list` has it.
    public static func choose(_ size: String, keeping current: String, in list: [String] = values, bar: Double = 4) -> String {
        isTriplet(current) ? (triplets(size, true, in: list, bar: bar) ?? size) : size
    }
}
