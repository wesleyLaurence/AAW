import CoreGraphics
import Foundation

/// The height of each row the person dragged, kept for the project in
/// `.daw/rows.json` by the row's path in the song (`tracks.drums`,
/// `groups.keys`, `returns.verb`). It is a view setting and not a song
/// field, so that the agent's text of the song does not change when the
/// person resizes a row. A row not in it has its kind's default height.
struct RowHeights {
    private struct File: Codable {
        var heights: [String: Double]
    }

    /// The least height of a row's own line, collapsed to its name, mute
    /// and solo, and the most.
    static let least: CGFloat = 22
    static let most: CGFloat = 400

    /// `.daw/rows.json` in the project's folder.
    private(set) var file: URL
    private(set) var heights: [String: CGFloat] = [:]
    /// Each row's path by its key in the arrangement last followed, so that
    /// a row renamed, by the person or the agent, keeps its height.
    private var paths: [UInt64: String] = [:]

    static func file(project folder: URL) -> URL {
        folder.appendingPathComponent(".daw", isDirectory: true).appendingPathComponent("rows.json")
    }

    /// Reads the heights. A file that is missing or does not read has none.
    init(file: URL) {
        self.file = file
        if let data = try? Data(contentsOf: file), let read = try? JSONDecoder().decode(File.self, from: data) {
            heights = read.heights.mapValues { Self.clamped(CGFloat($0)) }
        }
    }

    init(project folder: URL) {
        self.init(file: Self.file(project: folder))
    }

    /// A height kept in the range a row can have, in whole points.
    static func clamped(_ height: CGFloat) -> CGFloat {
        min(max(height.rounded(), least), most)
    }

    subscript(path: String) -> CGFloat? {
        heights[path]
    }

    /// Gives the rows at `paths` a height, or their default with nil.
    /// Whether anything changed.
    @discardableResult
    mutating func set(_ height: CGFloat?, for paths: [String]) -> Bool {
        let value = height.map(Self.clamped)
        var changed = false
        for path in paths where heights[path] != value {
            heights[path] = value
            changed = true
        }
        return changed
    }

    /// Takes the rows' paths by key from a new arrangement: a row whose path
    /// changed takes its height to the new path. Whether anything changed.
    @discardableResult
    mutating func follow(_ current: [UInt64: String]) -> Bool {
        var changed = false
        var moved: [String: CGFloat] = [:]
        for (key, path) in current {
            guard let old = paths[key], old != path, let height = heights[old] else { continue }
            moved[path] = height
            // The old path is free, unless another row has taken it.
            if !current.values.contains(old) { heights[old] = nil }
            changed = true
        }
        heights.merge(moved) { _, new in new }
        paths = current
        return changed
    }

    /// The heights go with the project to a new folder.
    mutating func move(to folder: URL) {
        file = Self.file(project: folder)
    }

    func save() throws {
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(File(heights: heights.mapValues(Double.init))).write(to: file, options: .atomic)
    }
}
