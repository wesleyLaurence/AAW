import AAWCore
import AppKit
import Observation
import SwiftUI
import UniformTypeIdentifiers

/// A device dragged out of the browser, as a drop reads it: its kind, "rack"
/// for an effect rack and a plugin's slug for a plugin, and the name of the
/// patch or the rack it carries.
struct DraggedDevice: Equatable {
    var kind: String
    var patch: String? = nil
    var rack: String? = nil
}

/// The sample library as the browser shows it. The index is the one `daw
/// samples scan` writes and `daw samples search` reads; a search here asks
/// the same Python, so the person and the agent find the same samples.
@MainActor
@Observable
final class Browser {
    /// The audio files a song takes: those a pad plays as they are, and
    /// compressed ones, which the import decodes into the project.
    nonisolated static let extensions: Set<String> = ["wav", "aif", "aiff", "flac", "m4a", "mp3"]
    nonisolated static let limit: UInt32 = 200

    /// The index, or nil when none was found for the song.
    let library: String?
    let categories: [String]
    var section = UserDefaults.standard.string(forKey: "browser.section") ?? "Samples" {
        didSet {
            UserDefaults.standard.set(section, forKey: "browser.section")
            readTree()
        }
    }
    var folders: [LibraryFolder] = []
    var selectedFolders: Set<String> = [] {
        didSet {
            search()
            readTree()
        }
    }
    var scanning = false
    var folderError: String?
    @ObservationIgnored private var folderOperations = 0
    /// A device dragged out of the browser: an effect, a MIDI effect or an
    /// instrument, with the data its kind. Each also has a type of its own,
    /// this one with `.KIND` after it. The app's drag types are declared in
    /// its Info.plist, as macOS needs them to be to carry them.
    static let deviceType = "org.aaw.browser-device"
    /// A Synth patch dragged out of the browser; the data is its name.
    static let patchType = "org.aaw.browser-patch"
    /// An effect rack dragged out of the browser; the data is its name.
    static let rackType = "org.aaw.browser-rack"
    /// A plugin, a device of the person's own, dragged out of the browser;
    /// the data is its slug, which the song names it by.
    static let pluginType = "org.aaw.browser-plugin"
    /// An instrument rack dragged out of the browser; the data is its name.
    static let instrumentRackType = "org.aaw.browser-instrument-rack"
    /// The kind an instrument rack goes by where a device's kind is asked.
    static let instrumentRackKind = "instrument-rack"
    /// The instruments the browser offers, by kind.
    nonisolated static let instruments = ["sampler", drumRackKind, "synth"]
    /// The kind the Drum Rack goes by in the browser and its drags: a
    /// sampler flagged `drum_rack` in the song.
    nonisolated static let drumRackKind = "drum-rack"
    /// The MIDI effects the engine offers, by kind: devices before a MIDI
    /// track's instrument that change the notes as they play.
    nonisolated static let midiEffects = browserMidiEffects()

    /// Whether a kind is a MIDI effect, which goes on a MIDI track before
    /// its instrument.
    static func isMidiEffect(_ kind: String) -> Bool {
        midiEffects.contains(kind)
    }

    /// Whether a kind is an instrument, which goes on a MIDI track: a
    /// Sampler, a Synth or an instrument rack.
    static func isInstrument(_ kind: String) -> Bool {
        instruments.contains(kind) || kind == instrumentRackKind
    }

    /// The person's instrument racks, as `daw instrument rack list` lists
    /// them; read again when the browser is shown.
    private(set) var instrumentRacks: [InstrumentRackInfo] = []

    func refreshInstrumentRacks() {
        read({ libraryInstrumentRacks(query: "") }) { $0.instrumentRacks = $1 }
    }

    /// The person's patches, racks, instrument racks and favorites are
    /// files, read off the main thread as the browser shows, so that a large
    /// library never holds the window; a list shows what it had meanwhile.
    @ObservationIgnored private let files = DispatchQueue(label: "aaw.library.files")

    private func read<T: Sendable>(_ body: @escaping @Sendable () -> T, then apply: @escaping @MainActor (Browser, T) -> Void) {
        files.async { [weak self] in
            let value = body()
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    if let self { apply(self, value) }
                }
            }
        }
    }

    /// Whether every word of `query` is in the instrument rack's name, a
    /// tag, its instrument's kind or an effect's kind.
    nonisolated static func matches(_ rack: InstrumentRackInfo, _ query: String) -> Bool {
        query.split(separator: " ").allSatisfy { word in
            rack.name.localizedCaseInsensitiveContains(word) || rack.tags.contains { $0.localizedCaseInsensitiveContains(word) }
                || rack.instrument.localizedCaseInsensitiveContains(word) || rack.kinds.contains { $0.localizedCaseInsensitiveContains(word) }
        }
    }

    /// The sections, in the order the column lists them: Favorites first,
    /// since it is what the person keeps at hand, and Generate beside the
    /// samples it makes more of.
    nonisolated static let sections = ["Favorites", "Samples", "Generate", "Instruments", "MIDI Effects", "Audio Effects", "Folders"]

    /// The favorites, as `daw favorite list` lists them, in the order they
    /// were added; read again when the browser is shown and after a change.
    private(set) var favorites: [FavoriteInfo] = []
    /// The index's rows for the favorite samples, by path: the name, the
    /// length and the tempo a row shows. A favorite whose file the index
    /// does not have is shown from its path.
    private(set) var favoriteSamples: [String: SampleInfo] = [:]
    /// Why the favorites could not be read or changed, shown in the panel.
    var favoriteError: String?

    func refreshFavorites() {
        read({ Result { try libraryFavorites() } }) { browser, read in
            switch read {
            case .success(let favorites):
                browser.favorites = favorites
                browser.favoriteError = nil
            case .failure(let error):
                browser.favoriteError = SongModel.reason(error)
            }
            browser.resolveFavoriteSamples()
        }
    }

    /// Asks the index for the favorite samples' rows, off the main thread.
    private func resolveFavoriteSamples() {
        let paths = favorites.filter { $0.kind == "sample" }.map(\.what)
        guard let library, !paths.isEmpty else {
            favoriteSamples = [:]
            return
        }
        queue.async { [weak self] in
            let found = (try? librarySamplesAt(db: library, paths: paths)) ?? []
            DispatchQueue.main.async {
                guard let self else { return }
                self.favoriteSamples = Dictionary(found.map { ($0.path, $0) }, uniquingKeysWith: { first, _ in first })
            }
        }
    }

    /// The favorite that is `what` of `kind`, if any: a sample by its path
    /// and a device by its kind, a patch, a rack or an instrument rack by
    /// its slug, as the file matches them.
    nonisolated static func favorite(in favorites: [FavoriteInfo], kind: String, _ what: String) -> FavoriteInfo? {
        favorites.first { f in
            f.kind == kind && (kind == "sample" || kind == "device" ? f.what == what : f.slug == what)
        }
    }

    /// Whether `what` of `kind` is a favorite now; `what` is a sample's
    /// path, a device's kind, or a patch's, rack's or instrument rack's slug.
    func isFavorite(_ kind: String, _ what: String) -> Bool {
        Self.favorite(in: favorites, kind: kind, what) != nil
    }

    /// Adds `what` of `kind` to the favorites, or takes it out when it is
    /// one, as `daw favorite add` and `remove` do; `name` is what add takes,
    /// a name for a patch, a rack or an instrument rack.
    func toggleFavorite(_ kind: String, _ what: String, name: String) {
        do {
            if let have = Self.favorite(in: favorites, kind: kind, what) {
                favorites = try libraryFavoriteRemove(kind: kind, what: have.what)
            } else {
                favorites = try libraryFavoriteAdd(kind: kind, what: name)
            }
            favoriteError = nil
        } catch {
            favoriteError = SongModel.reason(error)
        }
        resolveFavoriteSamples()
    }

    /// Which group of Favorites a favorite is listed under: Samples,
    /// Instruments (an instrument, a patch, an instrument rack), MIDI
    /// Effects or Audio Effects (an effect kind, a rack).
    nonisolated static func favoriteGroup(_ favorite: FavoriteInfo) -> String {
        switch favorite.kind {
        case "sample": return "Samples"
        case "patch", "instrument_rack": return "Instruments"
        case "rack": return "Audio Effects"
        default:
            if instruments.contains(favorite.what) { return "Instruments" }
            return midiEffects.contains(favorite.what) ? "MIDI Effects" : "Audio Effects"
        }
    }

    /// A row for a favorite sample the index does not have: the file by
    /// its name, in its folder, with nothing measured.
    nonisolated static func standIn(for favorite: FavoriteInfo) -> SampleInfo {
        let url = URL(fileURLWithPath: favorite.what)
        return SampleInfo(id: favorite.what, name: url.lastPathComponent, pack: url.deletingLastPathComponent().lastPathComponent, path: favorite.what,
                          seconds: 0, channels: 0, category: "", kind: "unknown", bpm: nil, key: nil, note: nil, rootNote: nil)
    }

    /// The Synth's patches, factory and the person's, as `daw patch list`
    /// lists them; read again when the browser is shown.
    private(set) var patches: [PatchInfo] = []

    func refreshPatches() {
        read({ libraryPatches(query: "") }) { $0.patches = $1 }
    }

    /// The person's effect racks, as `daw rack list` lists them; read again
    /// when the browser is shown.
    private(set) var racks: [RackInfo] = []

    func refreshRacks() {
        read({ libraryRacks(query: "") }) { $0.racks = $1 }
    }

    /// Whether every word of `query` is in the patch's name or a tag.
    nonisolated static func matches(_ patch: PatchInfo, _ query: String) -> Bool {
        query.split(separator: " ").allSatisfy { word in
            patch.name.localizedCaseInsensitiveContains(word) || patch.tags.contains { $0.localizedCaseInsensitiveContains(word) }
        }
    }

    /// Whether every word of `query` is in the plugin's name, slug,
    /// description or a field's name.
    nonisolated static func matches(_ plugin: DeviceInfo, _ query: String) -> Bool {
        query.split(separator: " ").allSatisfy { word in
            plugin.name.localizedCaseInsensitiveContains(word) || plugin.slug.localizedCaseInsensitiveContains(word)
                || plugin.description.localizedCaseInsensitiveContains(word) || plugin.fields.contains { $0.localizedCaseInsensitiveContains(word) }
        }
    }

    /// Whether every word of `query` is in the rack's name, a tag or the
    /// kind of one of its effects.
    nonisolated static func matches(_ rack: RackInfo, _ query: String) -> Bool {
        query.split(separator: " ").allSatisfy { word in
            rack.name.localizedCaseInsensitiveContains(word) || rack.tags.contains { $0.localizedCaseInsensitiveContains(word) }
                || rack.kinds.contains { $0.localizedCaseInsensitiveContains(word) }
        }
    }

    func refreshFolders(operation: String = "list", path: String? = nil) {
        guard let library else { return }
        folderOperations += 1
        scanning = true
        if operation != "list" { folderError = nil }
        queue.async { [weak self] in
            let result = Result { try libraryFolders(db: library, operation: operation, path: path) }
            DispatchQueue.main.async {
                guard let self else { return }
                self.folderOperations -= 1
                self.scanning = self.folderOperations > 0
                switch result {
                case .success(let folders):
                    self.folders = folders
                    self.selectedFolders.formIntersection(Set(folders.map(\.path)))
                    self.search()
                    self.rereadTree()
                case .failure(let error): self.folderError = SongModel.reason(error)
                }
                if operation != "list" {
                    NotificationCenter.default.post(name: .init("AAWLibraryChanged"), object: nil)
                }
            }
        }
    }

    func addFolders() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = true
        panel.prompt = "Add Folders"
        panel.begin { [weak self] response in
            guard response == .OK else { return }
            for url in panel.urls { self?.refreshFolders(operation: "add", path: url.path) }
        }
    }

    var query = "" {
        didSet {
            if query != oldValue { search(after: 0.25) }
        }
    }

    /// The category and the kind, `one-shot` or `loop`, that results are of;
    /// nil for any.
    var category: String? {
        didSet {
            if category != oldValue { search() }
        }
    }

    var kind: String? {
        didSet {
            if kind != oldValue { search() }
        }
    }

    private(set) var results: [SampleInfo] = []
    private(set) var searching = false
    private(set) var error: String?
    /// Whether a search has come back yet.
    private(set) var searched = false
    /// The sample last clicked.
    private(set) var selected: String?
    /// Whether the samples have the keys, so the selected one is drawn in
    /// the accent color, as a Mac list is, and gray once something else has
    /// them. The view keeps it, from its focus.
    var hasKeys = false
    /// Whether a click plays the sample.
    var auditions = true {
        didSet {
            if !auditions { sound?.stop() }
        }
    }

    /// The sample being dragged out of the browser.
    @ObservationIgnored var dragged: SampleInfo?
    /// The device being dragged out of the browser, set as the drag starts:
    /// the timeline reads it to say where the device would land, since the
    /// drag's own data is not readable until it is let go. Only read while
    /// a drag of a device's type is over the view.
    @ObservationIgnored var draggedDevice: DraggedDevice?

    /// A drag of a device out of the browser begins: it is the one dragged,
    /// and no sample is.
    func startDrag(_ device: DraggedDevice) {
        dragged = nil
        draggedDevice = device
    }
    @ObservationIgnored private var sound: NSSound?
    @ObservationIgnored private var pending: Task<Void, Never>?
    @ObservationIgnored private var searches = 0
    /// Searches run in Python, off the main thread and one at a time.
    @ObservationIgnored private let queue = DispatchQueue(label: "aaw.library")

    init(library: String?) {
        self.library = library
        categories = libraryCategories()
    }

    /// Searches for the query, category and kind as they are now, after a
    /// pause while the query is being typed. A later search replaces this one.
    func search(after delay: Double = 0) {
        guard let library else { return }
        pending?.cancel()
        searches += 1
        let (mine, query, category, kind, folders) = (searches, query, category, kind, Array(selectedFolders))
        pending = Task { [weak self] in
            if delay > 0 { try? await Task.sleep(for: .seconds(delay)) }
            guard !Task.isCancelled, let self else { return }
            self.searching = true
            self.queue.async {
                let found = Result { try librarySearchFolders(db: library, query: query, category: category, kind: kind, limit: Self.limit, folders: folders) }
                DispatchQueue.main.async {
                    MainActor.assumeIsolated {
                        guard self.searches == mine else { return }
                        self.searching = false
                        self.searched = true
                        switch found {
                        case .success(let rows):
                            self.results = rows
                            self.error = nil
                        case .failure(let error):
                            self.error = SongModel.reason(error)
                        }
                    }
                }
            }
        }
    }

    /// Selects a sample and plays it from its file, as it is: not through the
    /// song's engine, and at its own pitch and tempo.
    func audition(_ sample: SampleInfo) {
        selected = sample.id
        sound?.stop()
        guard auditions else { return }
        sound = NSSound(contentsOfFile: sample.path, byReference: true)
        sound?.play()
    }

    func stopAudition() {
        sound?.stop()
    }

    /// The sample `delta` places after the selected one in the results, as
    /// the arrow keys walk them: the first when none is selected or the
    /// selected one is no longer listed, and nil past either end.
    nonisolated static func neighbor(of selected: String?, in results: [SampleInfo], by delta: Int) -> SampleInfo? {
        guard let selected, let index = results.firstIndex(where: { $0.id == selected }) else { return results.first }
        let to = index + delta
        return results.indices.contains(to) ? results[to] : nil
    }

    /// Selects and plays the next (1) or the previous (-1) sample, as the
    /// arrow keys do in the Finder; at either end nothing changes.
    func step(_ delta: Int) {
        guard let sample = Self.neighbor(of: selected, in: results, by: delta) else { return }
        audition(sample)
    }

    // MARK: Folders as a tree

    /// The folders open in the Folders tree, by path, kept across launches.
    var openFolders = Set(UserDefaults.standard.stringArray(forKey: "browser.openFolders") ?? []) {
        didSet {
            UserDefaults.standard.set(openFolders.sorted(), forKey: "browser.openFolders")
            readTree()
        }
    }
    /// The folders read from the index, by path: their subfolders and the
    /// samples directly in them. Only folders shown open are read; the
    /// tests set it in place of the index.
    var trees: [String: LibraryTree] = [:]
    /// Why a folder could not be read, shown under it.
    private(set) var treeError: String?
    /// The row of the tree last clicked or reached with the keys, by its
    /// `FolderRow.key`.
    private(set) var treeSelected: String?
    @ObservationIgnored private var treeReads = 0
    @ObservationIgnored private var reading: Set<String> = []

    /// Whether the Folders section shows its folders as a tree: until a
    /// search, a category or a kind asks for matches, which are listed flat.
    var showsTree: Bool {
        section == "Folders" && query.trimmingCharacters(in: .whitespaces).isEmpty && category == nil && kind == nil
    }

    /// The folders the tree starts from: those checked, else all of them.
    var treeRoots: [String] {
        let all = folders.map(\.path)
        return selectedFolders.isEmpty ? all : all.filter(selectedFolders.contains)
    }

    /// The rows of the tree as they are drawn now.
    var folderRows: [FolderRow] {
        Self.folderRows(roots: treeRoots, open: openFolders, trees: trees)
    }

    /// Opens a folder that is closed and closes one that is open.
    func toggle(_ path: String) {
        if openFolders.contains(path) { openFolders.remove(path) } else { openFolders.insert(path) }
    }

    /// Forgets what was read, as after a folder is added, refreshed or
    /// removed, and reads what is open again.
    func rereadTree() {
        treeReads += 1
        reading = []
        trees = [:]
        readTree()
    }

    /// Reads the open folders the tree shows and has not read yet, in one
    /// run of the index, off the main thread; a folder opened inside one
    /// just read is read next.
    func readTree() {
        guard let library, section == "Folders" else { return }
        let wanted = Self.unread(roots: treeRoots, open: openFolders, trees: trees).filter { !reading.contains($0) }
        guard !wanted.isEmpty else { return }
        reading.formUnion(wanted)
        let mine = treeReads
        queue.async { [weak self] in
            let found = Result { try libraryTree(db: library, folders: wanted) }
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self, self.treeReads == mine else { return }
                    self.reading.subtract(wanted)
                    switch found {
                    case .success(let read):
                        for tree in read { self.trees[tree.path] = tree }
                        // A folder the index no longer has reads as empty, not forever.
                        for path in wanted where self.trees[path] == nil {
                            self.trees[path] = LibraryTree(path: path, folders: [], samples: [])
                        }
                        self.treeError = nil
                        self.readTree()
                    case .failure(let error):
                        self.treeError = SongModel.reason(error)
                        for path in wanted { self.trees[path] = LibraryTree(path: path, folders: [], samples: []) }
                    }
                }
            }
        }
    }

    /// Selects a row of the tree, and plays it when it is a sample.
    func selectRow(_ row: FolderRow) {
        treeSelected = row.key
        switch row.kind {
        case .sample(let sample): audition(sample)
        default: sound?.stop()
        }
    }

    /// Up (-1) and Down (1) in the tree: the row before or after the
    /// selected one, played when it is a sample, as the Finder walks a list.
    func stepTree(_ delta: Int) {
        guard let row = Self.neighborRow(of: treeSelected, in: folderRows, by: delta) else { return }
        selectRow(row)
    }

    /// Right opens the selected folder, or goes into it when it is open;
    /// Left closes it, or goes to the folder a row is in: an outline's keys.
    func stepTree(into: Bool) {
        let rows = folderRows
        guard let index = rows.firstIndex(where: { $0.key == treeSelected }) else {
            if let first = rows.first(where: { $0.selectable }) { selectRow(first) }
            return
        }
        let row = rows[index]
        if case .folder(let path, _, _, let open) = row.kind {
            if into && !open { openFolders.insert(path); return }
            if !into && open { openFolders.remove(path); return }
            if into {
                if index + 1 < rows.count, rows[index + 1].depth > row.depth, rows[index + 1].selectable { selectRow(rows[index + 1]) }
                return
            }
        }
        guard !into, let parent = rows[..<index].last(where: { $0.depth < row.depth && $0.selectable }) else { return }
        selectRow(parent)
    }

    /// The rows of a tree of `roots`: one folder's contents at the top when
    /// it is the only root, else each root as a folder; under a folder that
    /// is open its subfolders and then its samples, a row reading it while
    /// it is not read yet.
    nonisolated static func folderRows(roots: [String], open: Set<String>, trees: [String: LibraryTree]) -> [FolderRow] {
        var rows: [FolderRow] = []
        func contents(_ path: String, _ depth: Int, _ root: Int) {
            guard let tree = trees[path] else {
                rows.append(FolderRow(root: root, depth: depth, kind: .reading(path)))
                return
            }
            for folder in tree.folders {
                let isOpen = open.contains(folder.path)
                rows.append(FolderRow(root: root, depth: depth, kind: .folder(path: folder.path, name: folder.name, count: Int(folder.count), open: isOpen)))
                if isOpen { contents(folder.path, depth + 1, root) }
            }
            for sample in tree.samples { rows.append(FolderRow(root: root, depth: depth, kind: .sample(sample))) }
        }
        if roots.count == 1 {
            contents(roots[0], 0, 0)
        } else {
            for (index, root) in roots.enumerated() {
                let isOpen = open.contains(root)
                rows.append(FolderRow(root: index, depth: 0, kind: .folder(path: root, name: URL(fileURLWithPath: root).lastPathComponent, count: nil, open: isOpen)))
                if isOpen { contents(root, 1, index) }
            }
        }
        return rows
    }

    /// The folders the tree shows the contents of that are not read yet.
    nonisolated static func unread(roots: [String], open: Set<String>, trees: [String: LibraryTree]) -> [String] {
        folderRows(roots: roots, open: open, trees: trees).compactMap { row in
            if case .reading(let path) = row.kind { return path }
            return nil
        }
    }

    /// The row `delta` places from the selected one, skipping rows still
    /// being read: the first when none is selected, nil past either end.
    nonisolated static func neighborRow(of selected: String?, in rows: [FolderRow], by delta: Int) -> FolderRow? {
        let rows = rows.filter(\.selectable)
        guard let selected, let index = rows.firstIndex(where: { $0.key == selected }) else { return rows.first }
        let to = index + delta
        return rows.indices.contains(to) ? rows[to] : nil
    }

    /// The folder a sample is in, from the registered folder it is under:
    /// `samples/bass` for a file in `bass` of the folder `samples`. The
    /// longest folder that holds it names it.
    nonisolated static func folder(of path: String, in roots: [String]) -> String {
        let directory = (path as NSString).deletingLastPathComponent
        guard let root = roots.filter({ directory == $0 || directory.hasPrefix($0.hasSuffix("/") ? $0 : $0 + "/") }).max(by: { $0.count < $1.count }) else {
            return (directory as NSString).lastPathComponent
        }
        let name = (root as NSString).lastPathComponent
        let rest = String(directory.dropFirst(root.count)).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        return rest.isEmpty ? name : name + "/" + rest
    }

    /// What a pad or a track for a sample is named after: its category where
    /// the file's name gave one, else the name.
    nonisolated static func padName(of sample: SampleInfo) -> String {
        sample.category.isEmpty || sample.category == "other"
            ? (sample.name as NSString).deletingPathExtension
            : sample.category
    }

    /// A line about a sample: its length, and its tempo, key and pitch where known.
    nonisolated static func detail(of sample: SampleInfo) -> String {
        var parts = [sample.seconds < 10 ? String(format: "%.2f s", sample.seconds) : String(format: "%.0f s", sample.seconds)]
        if let bpm = sample.bpm { parts.append("\(bpm) BPM") }
        if let key = sample.key { parts.append(key) }
        if let note = sample.note { parts.append("♪ \(note)") }
        return parts.joined(separator: " · ")
    }
}

/// A row of the Folders tree: a folder with its arrow, a sample, or a
/// folder's contents while the index is being read for them.
struct FolderRow: Equatable {
    enum Kind: Equatable {
        /// `count` is how many samples are under the folder; nil for a
        /// registered folder listed as a root.
        case folder(path: String, name: String, count: Int?, open: Bool)
        case sample(SampleInfo)
        case reading(String)
    }

    /// Which of the roots it is under, so a folder registered inside
    /// another and listed twice keeps its rows apart.
    let root: Int
    let depth: Int
    let kind: Kind

    /// What the list knows it by and the selection names.
    var key: String {
        switch kind {
        case .folder(let path, _, _, _): return "\(root):folder:\(path)"
        case .sample(let sample): return "\(root):sample:\(sample.path)"
        case .reading(let path): return "\(root):reading:\(path)"
        }
    }

    var selectable: Bool {
        if case .reading = kind { return false }
        return true
    }
}

/// The browser: the library's samples by search, category and kind. A click
/// plays a sample; the mark beside it, or a drag onto the arrangement, adds
/// it to the song as a pad of a track or as a new track.
struct BrowserView: View {
    let model: SongModel

    static let width: CGFloat = 360

    private var browser: Browser { model.browser }
    /// Whether the samples have the keys: after a click on one, Up and Down
    /// walk them, as in the Finder, until a click elsewhere takes them back.
    @FocusState private var samplesHaveKeys: Bool

    /// The track the mark adds a sample to: the selected one, if any.
    private var target: TrackView? {
        guard case .track(let key) = model.selectedRow else { return nil }
        return model.arrangement.tracks.first { $0.key == key }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Text("Browser")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
                if browser.searching { ProgressView().controlSize(.mini) }
                Button {
                    browser.auditions.toggle()
                } label: {
                    Image(systemName: browser.auditions ? "speaker.wave.2.fill" : "speaker.slash")
                        .foregroundStyle(browser.auditions ? Color.primary : Color.secondary)
                }
                .buttonStyle(.borderless)
                .font(.system(size: 10))
                .help(browser.auditions ? "A click plays the sample" : "A click selects the sample without playing it")
            }
            .padding(.horizontal, 12)
            .frame(height: TimelineLayout.rulerHeight - 1)
            Divider()
            TextField(browser.section == "Generate" ? "Search Generated" : "Search \(browser.section)", text: Binding(get: { browser.query }, set: { browser.query = $0 }))
                .textFieldStyle(.roundedBorder)
                .padding(8)
            if let error = browser.folderError { note(error) }
            HStack(alignment: .top, spacing: 0) {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(Browser.sections, id: \.self) { section in
                        Button(section) { browser.section = section }
                            .buttonStyle(.plain)
                            .font(.system(size: 11, weight: browser.section == section ? .bold : .regular))
                            .foregroundStyle(browser.section == section ? Color.accentColor : .primary)
                            .padding(.vertical, 4)
                    }
                    Divider()
                    Button("Add Folder…") { browser.addFolders() }.disabled(browser.scanning)
                    Button(browser.scanning ? "Scanning…" : "Refresh") { browser.refreshFolders(operation: "refresh") }
                        .disabled(browser.scanning || browser.folders.isEmpty)
                    ScrollView {
                        VStack(alignment: .leading, spacing: 8) {
                            Button("All folders") { browser.selectedFolders = []; browser.section = "Folders" }
                                .foregroundStyle(browser.selectedFolders.isEmpty ? Color.accentColor : .primary)
                            ForEach(browser.folders, id: \.path) { folder in
                                Toggle(isOn: Binding(get: { browser.selectedFolders.contains(folder.path) }, set: { selected in
                                    if selected { browser.selectedFolders.insert(folder.path) }
                                    else { browser.selectedFolders.remove(folder.path) }
                                    browser.section = "Folders"
                                })) {
                                    Text(URL(fileURLWithPath: folder.path).lastPathComponent + (folder.available ? "" : " (offline)"))
                                        .lineLimit(2)
                                }
                                .toggleStyle(.checkbox)
                                .help(folder.path)
                                .contextMenu {
                                    Button("Remove Folder") { browser.refreshFolders(operation: "remove", path: folder.path) }
                                }
                            }
                        }
                    }
                }
                .buttonStyle(.borderless)
                .font(.system(size: 10))
                .padding(8)
                .frame(width: 120, alignment: .leading)
                Divider()
                VStack(spacing: 0) {
                    if browser.section == "Favorites" {
                        favoritesList
                    } else if browser.section == "Generate" {
                        GenerateView(model: model)
                    } else if browser.section == "Instruments" || browser.section == "MIDI Effects" || browser.section == "Audio Effects" {
                        devices
                    } else {
                        filters
                        Divider()
                        if browser.showsTree { folderTree } else { results }
                    }
                }.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }

        }
        .frame(width: Self.width)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(Color(nsColor: Theme.gray(0.14)))
        .onAppear { browser.hasKeys = false; browser.refreshFolders(); browser.refreshPatches(); browser.refreshRacks(); browser.refreshInstrumentRacks(); browser.refreshFavorites(); model.refreshPlugins() }
        .onReceive(NotificationCenter.default.publisher(for: .init("AAWLibraryChanged"))) { _ in browser.refreshFolders() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            // A patch or a rack saved from the terminal shows when the window comes back.
            if browser.section == "Instruments" { browser.refreshPatches() }
            if browser.section == "Audio Effects" || browser.section == "Instruments" { browser.refreshRacks(); model.refreshPlugins() }
            if browser.section == "Instruments" { browser.refreshInstrumentRacks() }
            // A favorite added from the terminal, or a patch it names saved, too.
            if browser.section == "Favorites" { browser.refreshPatches(); browser.refreshRacks(); browser.refreshInstrumentRacks(); browser.refreshFavorites() }
        }
        .onChange(of: browser.section) { _, section in
            if section == "Instruments" { browser.refreshPatches(); browser.refreshInstrumentRacks() }
            if section == "Audio Effects" || section == "Instruments" { browser.refreshRacks(); model.refreshPlugins() }
            if section == "Favorites" { browser.refreshPatches(); browser.refreshRacks(); browser.refreshInstrumentRacks(); browser.refreshFavorites() }
        }
    }

    /// The favorites, in the groups the other sections list them under,
    /// each row as it is there: a sample from the index, or from its path
    /// when the index does not have it; a patch, a rack or an instrument
    /// rack as the library has it now, and one it no longer has named and
    /// gray, with Remove from Favorites in its menu; a device by its kind.
    /// The search field narrows them by name.
    private var favoritesList: some View {
        let query = browser.query
        let shown = browser.favorites.filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) || readable($0.what).localizedCaseInsensitiveContains(query) }
        let groups = ["Samples", "Instruments", "MIDI Effects", "Audio Effects"].map { group in (group, shown.filter { Browser.favoriteGroup($0) == group }) }
        return ScrollView {
            VStack(spacing: 0) {
                if let error = browser.favoriteError { note(error) }
                ForEach(groups, id: \.0) { group, favorites in
                    if !favorites.isEmpty {
                        heading(group)
                        ForEach(favorites, id: \.what) { favorite in
                            favoriteRow(favorite)
                            if favorite.kind == "sample" { Divider().opacity(0.35) }
                        }
                    }
                }
                if browser.favorites.isEmpty && browser.favoriteError == nil {
                    Text("Nothing is a favorite yet. Add to Favorites, in the menu of any sample, patch, rack or device in the browser, or daw favorite add, lists it here, first among the sections.")
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(10)
                } else if shown.isEmpty && !query.isEmpty {
                    note("No favorite matches.")
                }
            }
        }
    }

    /// A favorite's row: the row its section would draw, or a gray one for
    /// what the library no longer has.
    @ViewBuilder private func favoriteRow(_ favorite: FavoriteInfo) -> some View {
        switch favorite.kind {
        case "sample":
            if let sample = browser.favoriteSamples[favorite.what] {
                row(sample)
            } else if favorite.missing {
                missingRow(favorite, "The file is gone")
            } else {
                row(Browser.standIn(for: favorite))
            }
        case "patch":
            if let patch = browser.patches.first(where: { $0.slug == favorite.slug }) { row(patch) } else { missingRow(favorite, "No patch of this name is saved") }
        case "rack":
            if let rack = browser.racks.first(where: { $0.slug == favorite.slug }) { row(rack) } else { missingRow(favorite, "No rack of this name is saved") }
        case "instrument_rack":
            if let rack = browser.instrumentRacks.first(where: { $0.slug == favorite.slug }) { row(rack) } else { missingRow(favorite, "No instrument rack of this name is saved") }
        default:
            if favorite.missing { missingRow(favorite, "No device of this kind") } else { device(favorite.what) }
        }
    }

    /// A favorite the library no longer has: its name and why, gray, with
    /// Remove from Favorites in its menu and nothing to drag.
    private func missingRow(_ favorite: FavoriteInfo, _ why: String) -> some View {
        HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 1) {
                Text(favorite.name)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                Text(why)
                    .font(.system(size: 10))
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .foregroundStyle(.tertiary)
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .frame(height: 34)
        .contentShape(Rectangle())
        .help(favorite.what)
        .contextMenu {
            Button("Remove from Favorites") { browser.toggleFavorite(favorite.kind, favorite.kind == "sample" || favorite.kind == "device" ? favorite.what : favorite.slug, name: favorite.what) }
        }
    }

    /// Add to Favorites, or Remove from Favorites when it is one, for a
    /// row's menu: `what` is how the favorite is matched, a path, a kind or
    /// a slug, and `name` what adding takes.
    private func favoriteItem(_ kind: String, _ what: String, name: String) -> some View {
        Button(browser.isFavorite(kind, what) ? "Remove from Favorites" : "Add to Favorites") {
            browser.toggleFavorite(kind, what, name: name)
        }
    }

    /// Instruments, MIDI effects or audio effects, each with + and dragged
    /// by its name; under the Synth its patches, Factory and the person's
    /// own, and under the audio effects the person's racks, searched by
    /// name and tag along with the devices.
    private var devices: some View {
        let instruments = browser.section == "Instruments"
        let midiEffects = browser.section == "MIDI Effects"
        let kinds = (instruments ? Browser.instruments : midiEffects ? Browser.midiEffects : DeviceChain.kinds)
            .filter { browser.query.isEmpty || readable($0).localizedCaseInsensitiveContains(browser.query) }
        let patches = instruments ? browser.patches.filter { Browser.matches($0, browser.query) } : []
        let factory = patches.filter(\.factory)
        let mine = patches.filter { !$0.factory }
        let racks = instruments || midiEffects ? [] : browser.racks.filter { Browser.matches($0, browser.query) }
        // Plugins: effects under Audio Effects, instruments under
        // Instruments, MIDI effects under MIDI Effects.
        let pluginKind = instruments ? "instrument" : midiEffects ? "midi" : "effect"
        let plugins = model.plugins.filter { $0.kind == pluginKind && Browser.matches($0, browser.query) }
        let instrumentRacks = instruments ? browser.instrumentRacks.filter { Browser.matches($0, browser.query) } : []
        return ScrollView {
            VStack(spacing: 0) {
                ForEach(kinds, id: \.self) { kind in
                    device(kind)
                }
                if instruments && !patches.isEmpty {
                    if !factory.isEmpty {
                        heading("Factory")
                        ForEach(factory, id: \.slug) { patch in row(patch) }
                    }
                    if !mine.isEmpty {
                        heading("Mine")
                        ForEach(mine, id: \.slug) { patch in row(patch) }
                    }
                }
                if instruments && browser.query.isEmpty && mine.isEmpty {
                    Text("Your own patches, saved with daw patch save, are listed here under Mine.")
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(10)
                }
                if instruments && !plugins.isEmpty {
                    heading("Plugins")
                    ForEach(plugins, id: \.slug) { plugin in row(plugin) }
                }
                if instruments && !instrumentRacks.isEmpty {
                    heading("Racks")
                    ForEach(instrumentRacks, id: \.slug) { rack in row(rack) }
                }
                if instruments && browser.query.isEmpty && browser.instrumentRacks.isEmpty {
                    Text("An instrument with its effects, saved with Save Instrument… in the device panel, or daw instrument rack save, is listed here under Racks and dropped on any MIDI track.")
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(10)
                }
                if midiEffects && !plugins.isEmpty {
                    heading("Plugins")
                    ForEach(plugins, id: \.slug) { plugin in row(plugin) }
                }
                if midiEffects && browser.query.isEmpty {
                    Text("A MIDI effect goes on a MIDI track before its instrument and changes the notes as they play; the clips keep the notes as written. Drop one on a MIDI track's header or its device panel. One of your own, made with daw plugin new --midi, is listed here under Plugins.")
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(10)
                }
                if !instruments && !midiEffects && !plugins.isEmpty {
                    heading("Plugins")
                    ForEach(plugins, id: \.slug) { plugin in row(plugin) }
                }
                if !instruments && !midiEffects && browser.query.isEmpty && !model.plugins.contains(where: { $0.kind == "effect" }) {
                    Text("A device of your own, made with daw plugin new under the project's or the workspace's devices folder, is listed here under Plugins and dropped on any track.")
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(10)
                }
                if !instruments && !midiEffects && !racks.isEmpty {
                    heading("Racks")
                    ForEach(racks, id: \.slug) { rack in row(rack) }
                }
                if !instruments && !midiEffects && browser.query.isEmpty && browser.racks.isEmpty {
                    Text("A chain of effects saved with Save Rack… in the device panel, or daw rack save, is listed here under Racks and dropped on any track.")
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(10)
                }
            }
        }
    }

    /// An instrument rack: its name, its instrument and the kinds of its
    /// effects, + to load it onto the selected MIDI track or a new one, and
    /// dragged by its name.
    private func row(_ rack: InstrumentRackInfo) -> some View {
        let line = ([readable(rack.instrument)] + rack.kinds.map(readable)).joined(separator: " · ")
        return HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 1) {
                Text(rack.name)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                Text(line)
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Button {
                model.addBrowserDevice(Browser.instrumentRackKind, to: model.selectedRow, rack: rack.name)
            } label: {
                Image(systemName: "plus").frame(width: 18, height: 18)
            }
            .buttonStyle(.borderless)
            .font(.system(size: 10))
            .disabled(!model.canAddBrowserDevice(Browser.instrumentRackKind, to: model.selectedRow))
            .help(target.map { "Load \(rack.name) onto \($0.name)" } ?? "Add a new MIDI track playing \(rack.name)")
        }
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .frame(height: 34)
        .contentShape(Rectangle())
        .help(rack.description.isEmpty ? line : rack.description)
        .contextMenu { favoriteItem("instrument_rack", rack.slug, name: rack.name) }
        .onTapGesture(count: 2) { model.addBrowserDevice(Browser.instrumentRackKind, to: model.selectedRow, rack: rack.name) }
        .onDrag {
            browser.startDrag(DraggedDevice(kind: Browser.instrumentRackKind, rack: rack.name))
            let provider = NSItemProvider()
            provider.registerDataRepresentation(forTypeIdentifier: Browser.instrumentRackType, visibility: .all) { completion in
                completion(Data(rack.name.utf8), nil)
                return nil
            }
            return provider
        }
    }

    /// A rack: its name and the kinds of its effects, + to add it to the
    /// selected row's chain, and dragged by its name.
    private func row(_ rack: RackInfo) -> some View {
        let kinds = rack.kinds.map(readable).joined(separator: " · ")
        return HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 1) {
                Text(rack.name)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                Text(kinds)
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Button {
                model.addBrowserDevice("rack", to: model.selectedRow, rack: rack.name)
            } label: {
                Image(systemName: "plus").frame(width: 18, height: 18)
            }
            .buttonStyle(.borderless)
            .font(.system(size: 10))
            .disabled(!model.canAddBrowserDevice("rack", to: model.selectedRow))
            .help("Add the effects of \(rack.name) to the selected row's chain")
        }
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .frame(height: 34)
        .contentShape(Rectangle())
        .help(rack.description.isEmpty ? kinds : rack.description)
        .contextMenu { favoriteItem("rack", rack.slug, name: rack.name) }
        .onTapGesture(count: 2) { model.addBrowserDevice("rack", to: model.selectedRow, rack: rack.name) }
        .onDrag {
            browser.startDrag(DraggedDevice(kind: "rack", rack: rack.name))
            let provider = NSItemProvider()
            provider.registerDataRepresentation(forTypeIdentifier: Browser.rackType, visibility: .all) { completion in
                completion(Data(rack.name.utf8), nil)
                return nil
            }
            return provider
        }
    }

    /// A plugin: its name and fields, + to add it to the selected row's
    /// chain, and dragged by its slug.
    private func row(_ plugin: DeviceInfo) -> some View {
        let line = plugin.fields.isEmpty ? plugin.description : plugin.fields.map(readable).joined(separator: " · ")
        return HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 1) {
                Text(plugin.name)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                Text(line)
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Button {
                model.addBrowserDevice(plugin.slug, to: model.selectedRow)
            } label: {
                Image(systemName: "plus").frame(width: 18, height: 18)
            }
            .buttonStyle(.borderless)
            .font(.system(size: 10))
            .disabled(!model.canAddBrowserDevice(plugin.slug, to: model.selectedRow))
            .help(plugin.kind == "instrument"
                ? (target.map { "Play \(plugin.name) on \($0.name), as daw instrument set does" } ?? "Add a new MIDI track playing \(plugin.name)")
                : plugin.kind == "midi"
                ? "Add \(plugin.name) before the selected MIDI track's instrument, as daw effect add --type \(plugin.slug) does"
                : "Add \(plugin.name) to the selected row's chain, as daw effect add --type \(plugin.slug) does")
        }
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .frame(height: 34)
        .contentShape(Rectangle())
        .help(plugin.description.isEmpty ? "\(plugin.slug), version \(plugin.version), from the \(plugin.from)" : plugin.description)
        .onTapGesture(count: 2) { model.addBrowserDevice(plugin.slug, to: model.selectedRow) }
        .onDrag {
            browser.startDrag(DraggedDevice(kind: plugin.slug))
            let provider = NSItemProvider()
            provider.registerDataRepresentation(forTypeIdentifier: Browser.pluginType, visibility: .all) { completion in
                completion(Data(plugin.slug.utf8), nil)
                return nil
            }
            return provider
        }
    }

    private func heading(_ title: String) -> some View {
        Text(title)
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 10)
            .padding(.top, 8)
            .padding(.bottom, 2)
    }

    private func device(_ kind: String) -> some View {
        HStack {
            Text(readable(kind))
            Spacer()
            Button { model.addBrowserDevice(kind, to: model.selectedRow) } label: { Image(systemName: "plus") }
                .buttonStyle(.borderless)
                .disabled(!model.canAddBrowserDevice(kind, to: model.selectedRow))
        }
        .font(.system(size: 12))
        .padding(10)
        .contentShape(Rectangle())
        .contextMenu { favoriteItem("device", kind, name: kind) }
        .onTapGesture(count: 2) { model.addBrowserDevice(kind, to: model.selectedRow) }
        .onDrag {
            browser.startDrag(DraggedDevice(kind: kind))
            let provider = NSItemProvider()
            // The kind's own type, which the device panel and the Sampler
            // take, and the one declared type every effect and instrument
            // has, which the timeline takes.
            for type in [Browser.deviceType + "." + kind, Browser.deviceType] {
                provider.registerDataRepresentation(forTypeIdentifier: type, visibility: .all) { completion in
                    completion(Data(kind.utf8), nil)
                    return nil
                }
            }
            return provider
        }
    }

    /// A patch: its name and tags, + to add a Synth with it to the selected
    /// MIDI track or a new one, and dragged by its name.
    private func row(_ patch: PatchInfo) -> some View {
        HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 1) {
                Text(patch.name)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                Text(patch.tags.isEmpty ? patch.description : patch.tags.joined(separator: " · "))
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Button {
                model.addBrowserDevice("synth", to: model.selectedRow, patch: patch.name)
            } label: {
                Image(systemName: "plus").frame(width: 18, height: 18)
            }
            .buttonStyle(.borderless)
            .font(.system(size: 10))
            .disabled(!model.canAddBrowserDevice("synth", to: model.selectedRow))
            .help(target.map { "Load \(patch.name) into a Synth on \($0.id)" } ?? "Add a new track with a Synth playing \(patch.name)")
        }
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .frame(height: 34)
        .contentShape(Rectangle())
        .help(patch.description)
        .contextMenu { favoriteItem("patch", patch.slug, name: patch.name) }
        .onTapGesture(count: 2) { model.addBrowserDevice("synth", to: model.selectedRow, patch: patch.name) }
        .onDrag {
            browser.startDrag(DraggedDevice(kind: "synth", patch: patch.name))
            let provider = NSItemProvider()
            provider.registerDataRepresentation(forTypeIdentifier: Browser.patchType, visibility: .all) { completion in
                completion(Data(patch.name.utf8), nil)
                return nil
            }
            return provider
        }
    }

    private func note(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
            .padding(12)
    }

    private var filters: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Picker("", selection: Binding(get: { browser.category ?? "" }, set: { browser.category = $0.isEmpty ? nil : $0 })) {
                    Text("Any category").tag("")
                    ForEach(browser.categories, id: \.self) { Text(readable($0)).tag($0) }
                }
                Picker("", selection: Binding(get: { browser.kind ?? "" }, set: { browser.kind = $0.isEmpty ? nil : $0 })) {
                    Text("Any kind").tag("")
                    Text("One-shots").tag("one-shot")
                    Text("Loops").tag("loop")
                }
            }
            .labelsHidden()
            .controlSize(.small)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
    }

    @ViewBuilder private var results: some View {
        if let error = browser.error {
            note(error)
        } else if browser.results.isEmpty {
            note(browser.folders.isEmpty ? "Add a folder to find samples across your projects." : (browser.searched ? "No samples match." : "Searching…"))
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(browser.results, id: \.id) { sample in
                            row(sample).id(sample.id)
                            Divider().opacity(0.35)
                        }
                        if browser.results.count >= Int(Browser.limit) {
                            Text("The first \(Browser.limit); search to narrow them.")
                                .font(.system(size: 10))
                                .foregroundStyle(.tertiary)
                                .padding(10)
                        }
                    }
                }
                // The list takes the keys when a sample is clicked, without a
                // ring around it, and Up and Down walk the samples and play
                // each; the one reached is kept in view.
                .focusable()
                .focusEffectDisabled()
                .focused($samplesHaveKeys)
                .onKeyPress(.upArrow) { browser.step(-1); return .handled }
                .onKeyPress(.downArrow) { browser.step(1); return .handled }
                .onChange(of: samplesHaveKeys) { _, has in browser.hasKeys = has }
                .onChange(of: browser.selected) { _, selected in
                    if let selected, browser.hasKeys { proxy.scrollTo(selected) }
                }
            }
            Divider()
            Text("\(model.sampleLanding) Drag onto the timeline for an audio clip, or onto a MIDI track's name for a Sampler.")
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 10)
                .padding(.vertical, 6)
        }
    }

    /// How far a row of the tree is set in for its depth, and the width of
    /// a folder's arrow.
    static func indent(_ depth: Int) -> CGFloat { 8 + CGFloat(depth) * 14 }
    static let arrowWidth: CGFloat = 14

    /// The Folders section as a tree, as the Finder's list view shows one:
    /// each folder with an arrow that opens and closes it, its subfolders
    /// and then its samples under it. A click selects a row and plays a
    /// sample; Up and Down walk the rows, Right opens a folder and Left
    /// closes it or goes to the folder a row is in.
    @ViewBuilder private var folderTree: some View {
        let rows = browser.folderRows
        if browser.folders.isEmpty {
            note("Add a folder to find samples across your projects.")
        } else if rows.isEmpty {
            note("No samples in this folder.")
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(rows, id: \.key) { row in
                            switch row.kind {
                            case .folder(let path, let name, let count, let open):
                                folderRow(row, path: path, name: name, count: count, open: open).id(row.key)
                            case .sample(let sample):
                                self.row(sample, in: row).id(row.key)
                                Divider().opacity(0.35)
                            case .reading:
                                Text("Reading…")
                                    .font(.system(size: 10))
                                    .foregroundStyle(.tertiary)
                                    .padding(.leading, Self.indent(row.depth) + Self.arrowWidth + 4)
                                    .frame(height: 22)
                            }
                        }
                    }
                }
                .focusable()
                .focusEffectDisabled()
                .focused($samplesHaveKeys)
                .onKeyPress(.upArrow) { browser.stepTree(-1); return .handled }
                .onKeyPress(.downArrow) { browser.stepTree(1); return .handled }
                .onKeyPress(.rightArrow) { browser.stepTree(into: true); return .handled }
                .onKeyPress(.leftArrow) { browser.stepTree(into: false); return .handled }
                .onChange(of: samplesHaveKeys) { _, has in browser.hasKeys = has }
                .onChange(of: browser.treeSelected) { _, selected in
                    if let selected, browser.hasKeys { proxy.scrollTo(selected) }
                }
            }
            if let error = browser.treeError { note(error) }
            Divider()
            Text("\(model.sampleLanding) Drag onto the timeline for an audio clip, or onto a MIDI track's name for a Sampler.")
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 10)
                .padding(.vertical, 6)
        }
    }

    /// A folder of the tree: its arrow, which opens and closes it, its
    /// name, and how many samples are under it.
    private func folderRow(_ row: FolderRow, path: String, name: String, count: Int?, open: Bool) -> some View {
        let isSelected = browser.treeSelected == row.key
        let emphasized = isSelected && browser.hasKeys
        return HStack(spacing: 4) {
            Button { browser.toggle(path) } label: {
                Image(systemName: open ? "chevron.down" : "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .frame(width: Self.arrowWidth, height: 18)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(emphasized ? Color.white : Color.secondary)
            .help(open ? "Close \(name)" : "Open \(name)")
            Image(systemName: "folder.fill")
                .font(.system(size: 10))
                .foregroundStyle(emphasized ? Color.white : Color.accentColor.opacity(0.8))
            Text(name)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(emphasized ? Color.white : Color.primary)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 0)
            if let count {
                Text("\(count)")
                    .font(.system(size: 10).monospacedDigit())
                    .foregroundStyle(emphasized ? Color.white.opacity(0.8) : Color.secondary)
            }
        }
        .padding(.leading, Self.indent(row.depth))
        .padding(.trailing, 8)
        .frame(height: 24)
        .background(isSelected
            ? Color(nsColor: browser.hasKeys ? .selectedContentBackgroundColor : .unemphasizedSelectedContentBackgroundColor)
            : Color.clear)
        .contentShape(Rectangle())
        .help(path)
        .onTapGesture(count: 2) { browser.toggle(path) }
        .onTapGesture {
            samplesHaveKeys = true
            browser.selectRow(row)
        }
    }

    /// A sample's row; in the tree, `row` is its place there, which the
    /// selection names, and the row is set in by its depth.
    private func row(_ sample: SampleInfo, in row: FolderRow? = nil) -> some View {
        let isSelected = row.map { browser.treeSelected == $0.key } ?? (browser.selected == sample.id)
        let emphasized = isSelected && browser.hasKeys
        return HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 1) {
                Text(sample.name)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(emphasized ? Color.white : Color.primary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                // In the tree the folder is where the row is; a search in
                // Folders names it, and Samples the pack.
                Text(row != nil ? Browser.detail(of: sample)
                    : "\(Browser.detail(of: sample)) · \(browser.section == "Folders" ? Browser.folder(of: sample.path, in: browser.folders.map(\.path)) : sample.pack)")
                    .font(.system(size: 10))
                    .foregroundStyle(emphasized ? Color.white.opacity(0.8) : Color.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Button {
                model.addSample(path: sample.path, name: Browser.padName(of: sample), note: sample.rootNote)
            } label: {
                Image(systemName: "plus").frame(width: 18, height: 18)
            }
            .buttonStyle(.borderless)
            .font(.system(size: 10))
            .help(target.map { $0.midi ? "Load into a Sampler on \($0.name)" : "Add to \($0.name) as an audio clip at the start position" }
                ?? "Add as a new MIDI track with a Sampler")
        }
        .padding(.leading, row.map { Self.indent($0.depth) + Self.arrowWidth + 4 } ?? 10)
        .padding(.trailing, 6)
        .frame(height: 34)
        // Selected as a Mac list shows it: in the accent color while the list
        // has the keys, gray once something else does.
        .background(isSelected
            ? Color(nsColor: browser.hasKeys ? .selectedContentBackgroundColor : .unemphasizedSelectedContentBackgroundColor)
            : Color.clear)
        .contentShape(Rectangle())
        .contextMenu { favoriteItem("sample", sample.path, name: sample.path) }
        .onTapGesture(count: 2) {
            model.addSample(path: sample.path, name: Browser.padName(of: sample), note: sample.rootNote)
        }
        .onTapGesture {
            samplesHaveKeys = true
            if let row { browser.selectRow(row) } else { browser.audition(sample) }
        }
        .onDrag {
            browser.dragged = sample
            return NSItemProvider(object: sample.path as NSString)
        }
    }
}
