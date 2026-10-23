import AAWCore
import AppKit
import Observation
import SwiftUI
import UniformTypeIdentifiers

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
        didSet { UserDefaults.standard.set(section, forKey: "browser.section") }
    }
    var folders: [LibraryFolder] = []
    var selectedFolders: Set<String> = [] { didSet { search() } }
    var scanning = false
    var folderError: String?
    @ObservationIgnored private var folderOperations = 0
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
    nonisolated static let instruments = ["sampler", "synth"]
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
        instrumentRacks = libraryInstrumentRacks(query: "")
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
    /// since it is what the person keeps at hand.
    nonisolated static let sections = ["Favorites", "Samples", "Instruments", "MIDI Effects", "Audio Effects", "Folders"]

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
        do {
            favorites = try libraryFavorites()
            favoriteError = nil
        } catch {
            favoriteError = SongModel.reason(error)
        }
        resolveFavoriteSamples()
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
        patches = libraryPatches(query: "")
    }

    /// The person's effect racks, as `daw rack list` lists them; read again
    /// when the browser is shown.
    private(set) var racks: [RackInfo] = []

    func refreshRacks() {
        racks = libraryRacks(query: "")
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
            TextField("Search \(browser.section)", text: Binding(get: { browser.query }, set: { browser.query = $0 }))
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
                    } else if browser.section == "Instruments" || browser.section == "MIDI Effects" || browser.section == "Audio Effects" {
                        devices
                    } else {
                        filters
                        Divider()
                        results
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
        // Plugins: effects under Audio Effects, instruments under Instruments.
        let plugins = midiEffects ? [] : model.plugins.filter { $0.kind == (instruments ? "instrument" : "effect") && Browser.matches($0, browser.query) }
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
                if midiEffects && browser.query.isEmpty {
                    Text("A MIDI effect goes on a MIDI track before its instrument and changes the notes as they play; the clips keep the notes as written. Drop one on a MIDI track's header or its device panel.")
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
            .help(target.map { "Load \(rack.name) onto \($0.id)" } ?? "Add a new MIDI track playing \(rack.name)")
        }
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .frame(height: 34)
        .contentShape(Rectangle())
        .help(rack.description.isEmpty ? line : rack.description)
        .contextMenu { favoriteItem("instrument_rack", rack.slug, name: rack.name) }
        .onTapGesture(count: 2) { model.addBrowserDevice(Browser.instrumentRackKind, to: model.selectedRow, rack: rack.name) }
        .onDrag {
            browser.dragged = nil
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
            browser.dragged = nil
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
                ? (target.map { "Play \(plugin.name) on \($0.id), as daw instrument set does" } ?? "Add a new MIDI track playing \(plugin.name)")
                : "Add \(plugin.name) to the selected row's chain, as daw effect add --type \(plugin.slug) does")
        }
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .frame(height: 34)
        .contentShape(Rectangle())
        .help(plugin.description.isEmpty ? "\(plugin.slug), version \(plugin.version), from the \(plugin.from)" : plugin.description)
        .onTapGesture(count: 2) { model.addBrowserDevice(plugin.slug, to: model.selectedRow) }
        .onDrag {
            browser.dragged = nil
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
            browser.dragged = nil
            let provider = NSItemProvider()
            provider.registerDataRepresentation(forTypeIdentifier: Browser.deviceType + "." + kind, visibility: .all) { completion in
                completion(Data(kind.utf8), nil)
                return nil
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
            browser.dragged = nil
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

    private func row(_ sample: SampleInfo) -> some View {
        let emphasized = browser.selected == sample.id && browser.hasKeys
        return HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 1) {
                Text(sample.name)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(emphasized ? Color.white : Color.primary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text("\(Browser.detail(of: sample)) · \(sample.pack)")
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
            .help(target.map { $0.midi ? "Load into a Sampler on \($0.id)" : "Add to \($0.id) as an audio clip at the start position" }
                ?? "Add as a new MIDI track with a Sampler")
        }
        .padding(.leading, 10)
        .padding(.trailing, 6)
        .frame(height: 34)
        // Selected as a Mac list shows it: in the accent color while the list
        // has the keys, gray once something else does.
        .background(browser.selected == sample.id
            ? Color(nsColor: browser.hasKeys ? .selectedContentBackgroundColor : .unemphasizedSelectedContentBackgroundColor)
            : Color.clear)
        .contentShape(Rectangle())
        .contextMenu { favoriteItem("sample", sample.path, name: sample.path) }
        .onTapGesture(count: 2) {
            model.addSample(path: sample.path, name: Browser.padName(of: sample), note: sample.rootNote)
        }
        .onTapGesture {
            samplesHaveKeys = true
            browser.audition(sample)
        }
        .onDrag {
            browser.dragged = sample
            return NSItemProvider(object: sample.path as NSString)
        }
    }
}
