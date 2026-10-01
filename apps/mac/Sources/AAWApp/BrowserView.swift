import AAWCore
import AppKit
import Observation
import SwiftUI

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
        let (mine, query, category, kind) = (searches, query, category, kind)
        pending = Task { [weak self] in
            if delay > 0 { try? await Task.sleep(for: .seconds(delay)) }
            guard !Task.isCancelled, let self else { return }
            self.searching = true
            self.queue.async {
                let found = Result { try librarySearch(db: library, query: query, category: category, kind: kind, limit: Self.limit) }
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

    static let width: CGFloat = 250

    private var browser: Browser { model.browser }

    /// The track the mark adds a sample to: the selected one, if any.
    private var target: TrackView? {
        guard case .track(let key) = model.selectedRow else { return nil }
        return model.arrangement.tracks.first { $0.key == key }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Text("Samples")
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
            if browser.library == nil {
                note("No sample library was found for this song. Index one with `daw samples scan DIRECTORY` from the folder that holds your songs, or set AAW_LIBRARY to an index.")
            } else {
                filters
                Divider()
                results
            }
        }
        .frame(width: Self.width)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(Color(nsColor: Theme.gray(0.14)))
        .onAppear {
            if !browser.searched { browser.search() }
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
            TextField("Search", text: Binding(get: { browser.query }, set: { browser.query = $0 }))
                .textFieldStyle(.plain)
                .font(.system(size: 12))
                .padding(.horizontal, 7)
                .frame(height: 22)
                .background(RoundedRectangle(cornerRadius: 4).fill(Color(nsColor: Theme.control)))
                .onSubmit { model.onFocus?() }
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
            note(browser.searched ? "No samples match." : "Searching…")
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
            Text(target.map { "+ adds a pad to \($0.id). Drag onto a track for a pad, or under the tracks for a new track." }
                ?? "+ adds a new track. Drag onto a track for a pad, or under the tracks for a new track.")
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
        .onTapGesture { browser.audition(sample) }
        .onDrag {
            browser.dragged = sample
            return NSItemProvider(object: sample.path as NSString)
        }
    }
}
