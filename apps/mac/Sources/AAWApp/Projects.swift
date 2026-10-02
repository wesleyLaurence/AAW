import AAWCore
import AppKit

/// The projects the app knows, in `projects.json` in its data folder: each
/// one's song file, a bookmark that finds it after its folder moved, its
/// title, whether it has a name yet and when it was last opened. Open Recent
/// shows it, and `daw projects --all` reads it, so an agent in a terminal
/// finds the same projects.
struct ProjectIndex {
    struct Entry: Codable, Equatable {
        /// The song file, where it was last seen.
        var path: String
        var bookmark: Data?
        var title: String
        /// In the data folder's Untitled, not yet given a name and a place.
        var untitled: Bool
        /// Unix time in seconds.
        var opened: Double
        /// Nothing is at the path, and the bookmark finds nothing.
        var missing = false

        var url: URL { URL(fileURLWithPath: path) }

        /// A project by its folder, which is how projects are told apart.
        var name: String { ProjectIndex.name(of: url) }

        init(path: String, bookmark: Data? = nil, title: String, untitled: Bool, opened: Double, missing: Bool = false) {
            (self.path, self.bookmark, self.title) = (path, bookmark, title)
            (self.untitled, self.opened, self.missing) = (untitled, opened, missing)
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            path = try c.decode(String.self, forKey: .path)
            bookmark = try c.decodeIfPresent(Data.self, forKey: .bookmark)
            title = try c.decodeIfPresent(String.self, forKey: .title) ?? ""
            untitled = try c.decodeIfPresent(Bool.self, forKey: .untitled) ?? false
            opened = try c.decodeIfPresent(Double.self, forKey: .opened) ?? 0
            missing = try c.decodeIfPresent(Bool.self, forKey: .missing) ?? false
        }
    }

    private struct File: Codable {
        var projects: [Entry]
    }

    let file: URL
    /// Most recently opened first.
    private(set) var entries: [Entry] = []
    /// Whether the file was there: a first launch has none.
    let existed: Bool

    /// `projects.json` in the app's data folder.
    static var standard: URL {
        URL(fileURLWithPath: projectDataDir()).appendingPathComponent("projects.json")
    }

    static func name(of song: URL) -> String {
        song.deletingLastPathComponent().lastPathComponent
    }

    /// Reads the index. A file that is missing or does not read is an empty one.
    init(file: URL) {
        self.file = file
        let data = try? Data(contentsOf: file)
        existed = data != nil
        if let data, let read = try? JSONDecoder().decode(File.self, from: data) {
            entries = read.projects
            sort()
        }
    }

    private mutating func sort() {
        entries.sort { $0.opened > $1.opened }
    }

    private static func bookmark(_ song: URL) -> Data? {
        try? song.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil)
    }

    /// Adds a project, or brings what is known of it up to date, as opened now.
    mutating func note(_ song: URL, title: String, untitled: Bool, at time: Date = Date()) {
        let known = entries.first { $0.path == song.path }
        entries.removeAll { $0.path == song.path }
        entries.append(Entry(path: song.path, bookmark: Self.bookmark(song) ?? known?.bookmark, title: title,
                             untitled: untitled, opened: time.timeIntervalSince1970))
        sort()
    }

    /// A project the index should have without its having been opened, such
    /// as an Untitled one a crash left: added if it is not there.
    mutating func keep(_ song: URL, title: String, untitled: Bool, at time: Date = Date()) {
        if !entries.contains(where: { $0.path == song.path }) { note(song, title: title, untitled: untitled, at: time) }
    }

    mutating func remove(_ song: URL) {
        entries.removeAll { $0.path == song.path }
    }

    /// Finds each project again: a bookmark follows a folder that was moved
    /// or renamed, and the path is corrected. A project that is nowhere, or
    /// in the Trash, is kept and marked missing. `untitled` says whether a
    /// song file is one of the data folder's Untitled projects.
    mutating func resolve(untitled: (URL) -> Bool = { projectIsUntitled(path: $0.path) }) {
        for i in entries.indices {
            var stale = false
            let found = entries[i].bookmark.flatMap {
                try? URL(resolvingBookmarkData: $0, options: [.withoutUI, .withoutMounting], relativeTo: nil, bookmarkDataIsStale: &stale)
            }
            let files = FileManager.default
            if let found, files.fileExists(atPath: found.path) {
                let url = SongModel.normal(found)
                entries[i].missing = url.pathComponents.contains(".Trash")
                if url.path != entries[i].path || stale {
                    entries[i].path = url.path
                    entries[i].bookmark = Self.bookmark(url) ?? entries[i].bookmark
                }
            } else {
                entries[i].missing = !files.fileExists(atPath: entries[i].path)
            }
            if !entries[i].missing { entries[i].untitled = untitled(entries[i].url) }
        }
        // Two entries that turn out to be one project: the later opening stays.
        var seen = Set<String>()
        entries = entries.filter { seen.insert($0.path).inserted }
    }

    /// Clear Menu: forgets every project but those open now and the Untitled
    /// ones, which nothing else leads back to.
    mutating func clear(keeping open: Set<String>) {
        entries.removeAll { !open.contains($0.path) && !($0.untitled && !$0.missing) }
    }

    func save() throws {
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(File(projects: entries)).write(to: file, options: .atomic)
    }
}

/// What closing a project's window does.
enum Closing: Equatable {
    /// A project with a name closes; its song is saved as it is edited.
    case close
    /// An Untitled project that holds nothing is deleted without a question.
    case delete
    /// An Untitled project that holds something: Save…, Delete or Cancel.
    case ask

    static func of(untitled: Bool, untouched: Bool) -> Closing {
        if !untitled { return .close }
        return untouched ? .delete : .ask
    }

    enum Answer: Equatable {
        case save, delete, cancel
    }

    /// Asks what to do with an Untitled project that holds something.
    @MainActor
    static func ask(_ title: String) -> Answer {
        let alert = NSAlert()
        alert.messageText = "Do you want to keep “\(title)”?"
        alert.informativeText = "This project has no name yet. Save it to choose a name and a place for it, or delete it."
        alert.addButton(withTitle: "Save…")
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: "Delete").hasDestructiveAction = true
        switch alert.runModal() {
        case .alertFirstButtonReturn: return .save
        case .alertThirdButtonReturn: return .delete
        default: return .cancel
        }
    }
}
