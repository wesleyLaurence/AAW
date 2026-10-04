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
        .onAppear { browser.refreshFolders() }
        .onReceive(NotificationCenter.default.publisher(for: .init("AAWLibraryChanged"))) { _ in browser.refreshFolders() }
    }

    private var devices: some View {
        let kinds = browser.section == "Instruments" ? ["sampler"] : DeviceChain.kinds
        return ScrollView {
            VStack(spacing: 0) {
                ForEach(kinds.filter { browser.query.isEmpty || readable($0).localizedCaseInsensitiveContains(browser.query) }, id: \.self) { kind in
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
            }
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
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(browser.results, id: \.id) { sample in
                        row(sample)
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
            Divider()
            Text(target.map { "+ adds a pad to \($0.id). Drag onto the timeline for an audio clip, or onto a track's name for a pad." }
                ?? "+ adds a new track with a pad. Drag onto the timeline for an audio clip, or onto a track's name for a pad.")
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 10)
                .padding(.vertical, 6)
        }
    }

    private func row(_ sample: SampleInfo) -> some View {
        HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 1) {
                Text(sample.name)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text("\(Browser.detail(of: sample)) · \(sample.pack)")
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Button {
                model.addSample(path: sample.path, name: Browser.padName(of: sample), note: sample.rootNote, to: target?.key)
            } label: {
                Image(systemName: "plus").frame(width: 18, height: 18)
            }
            .buttonStyle(.borderless)
            .font(.system(size: 10))
            .help(target.map { "Add as a pad of \($0.id)" } ?? "Add as a new track")
        }
        .padding(.leading, 10)
        .padding(.trailing, 6)
        .frame(height: 34)
        .background(Color.white.opacity(browser.selected == sample.id ? 0.09 : 0))
        .contentShape(Rectangle())
        .onTapGesture(count: 2) {
            model.addSample(path: sample.path, name: Browser.padName(of: sample), note: sample.rootNote, to: target?.key)
        }
        .onTapGesture { browser.audition(sample) }
        .onDrag {
            browser.dragged = sample
            return NSItemProvider(object: sample.path as NSString)
        }
    }
}
