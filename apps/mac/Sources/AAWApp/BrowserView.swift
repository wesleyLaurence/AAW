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
    /// The instruments the browser offers, by kind.
    static let instruments = ["sampler", "synth"]

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
                    ForEach(["Samples", "Instruments", "Audio Effects", "Folders"], id: \.self) { section in
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
                    if browser.section == "Instruments" || browser.section == "Audio Effects" {
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
        .onAppear { browser.hasKeys = false; browser.refreshFolders(); browser.refreshPatches(); browser.refreshRacks() }
        .onReceive(NotificationCenter.default.publisher(for: .init("AAWLibraryChanged"))) { _ in browser.refreshFolders() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            // A patch or a rack saved from the terminal shows when the window comes back.
            if browser.section == "Instruments" { browser.refreshPatches() }
            if browser.section == "Audio Effects" { browser.refreshRacks() }
        }
        .onChange(of: browser.section) { _, section in
            if section == "Instruments" { browser.refreshPatches() }
            if section == "Audio Effects" { browser.refreshRacks() }
        }
    }

    /// Instruments, or audio effects, each with + and dragged by its name;
    /// under the Synth its patches, Factory and the person's own, and under
    /// the effects the person's racks, searched by name and tag along with
    /// the devices.
    private var devices: some View {
        let instruments = browser.section == "Instruments"
        let kinds = (instruments ? Browser.instruments : DeviceChain.kinds)
            .filter { browser.query.isEmpty || readable($0).localizedCaseInsensitiveContains(browser.query) }
        let patches = instruments ? browser.patches.filter { Browser.matches($0, browser.query) } : []
        let factory = patches.filter(\.factory)
        let mine = patches.filter { !$0.factory }
        let racks = instruments ? [] : browser.racks.filter { Browser.matches($0, browser.query) }
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
                if !instruments && !racks.isEmpty {
                    heading("Racks")
                    ForEach(racks, id: \.slug) { rack in row(rack) }
                }
                if !instruments && browser.query.isEmpty && browser.racks.isEmpty {
                    Text("A chain of effects saved with Save Rack… in the device panel, or daw rack save, is listed here under Racks and dropped on any track.")
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(10)
                }
            }
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
