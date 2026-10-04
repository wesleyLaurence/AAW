import AAWCore
import XCTest
@testable import AAWApp

/// The project index, the close decision, and a project named by Save As…
/// through the app's own model and host.
final class ProjectsTests: XCTestCase {
    private var scratch: URL!

    /// The registry of hosts in a short folder, as socket paths are short,
    /// and a data folder that is not the person's. Set once: the Rust core
    /// reads them from the environment.
    private static let folders: URL = {
        let root = URL(fileURLWithPath: "/tmp/aaw-\(UUID().uuidString.prefix(8))")
        setenv("AAW_HOST_DIR", root.appendingPathComponent("hosts").path, 1)
        setenv("AAW_DATA_DIR", root.appendingPathComponent("data").path, 1)
        return root
    }()

    override func setUpWithError() throws {
        _ = Self.folders
        scratch = SongModel.normal(FileManager.default.temporaryDirectory.appendingPathComponent("aaw projects \(UUID().uuidString)"))
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try FileManager.default.removeItem(at: scratch)
    }

    override class func tearDown() {
        try? FileManager.default.removeItem(at: folders)
    }

    /// A project's folder with a song file in it.
    private func project(_ name: String) throws -> URL {
        let song = scratch.appendingPathComponent(name).appendingPathComponent("song.yaml")
        try FileManager.default.createDirectory(at: song.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "session: {length_beats: 16}\n".write(to: song, atomically: true, encoding: .utf8)
        return song
    }

    private func index() -> ProjectIndex {
        ProjectIndex(file: scratch.appendingPathComponent("data/projects.json"))
    }

    func testTheIndexKeepsProjectsNewestFirst() throws {
        var index = index()
        XCTAssertFalse(index.existed)
        XCTAssertEqual(index.entries, [])
        let (one, two) = (try project("One"), try project("Two"))
        index.note(one, title: "One", untitled: false, at: Date(timeIntervalSince1970: 100))
        index.note(two, title: "Second", untitled: true, at: Date(timeIntervalSince1970: 200))
        XCTAssertEqual(index.entries.map(\.title), ["Second", "One"])
        XCTAssertEqual(index.entries.map(\.name), ["Two", "One"])
        // Opened again, a project comes first and takes its new title.
        index.note(one, title: "First", untitled: false, at: Date(timeIntervalSince1970: 300))
        XCTAssertEqual(index.entries.map(\.title), ["First", "Second"])
        XCTAssertEqual(index.entries.map(\.untitled), [false, true])
        // Kept is for a project that should be listed without being opened.
        index.keep(two, title: "Other", untitled: false, at: Date(timeIntervalSince1970: 400))
        XCTAssertEqual(index.entries.map(\.title), ["First", "Second"])

        // What is written is what is read, and what `daw projects --all` reads.
        try index.save()
        let read = self.index()
        XCTAssertTrue(read.existed)
        XCTAssertEqual(read.entries, index.entries)
        let json = try JSONSerialization.jsonObject(with: Data(contentsOf: index.file)) as? [String: [[String: Any]]]
        let first = try XCTUnwrap(json?["projects"]?.first)
        XCTAssertEqual(first["path"] as? String, one.path)
        XCTAssertEqual(first["title"] as? String, "First")
        XCTAssertEqual(first["untitled"] as? Bool, false)
        XCTAssertEqual(first["opened"] as? Double, 300)
        XCTAssertNotNil(first["bookmark"] as? String)

        index.remove(one)
        XCTAssertEqual(index.entries.map(\.title), ["Second"])
        // A file that does not read is an empty index, and one with only the
        // fields another writer gave is read.
        try "not json".write(to: index.file, atomically: true, encoding: .utf8)
        XCTAssertEqual(self.index().entries, [])
        try #"{"projects": [{"path": "/music/Beat/song.yaml"}]}"#.write(to: index.file, atomically: true, encoding: .utf8)
        XCTAssertEqual(self.index().entries, [ProjectIndex.Entry(path: "/music/Beat/song.yaml", title: "", untitled: false, opened: 0)])
    }

    func testAMovedProjectIsFoundAgainAndALostOneIsMarked() throws {
        var index = index()
        let (moving, staying, going) = (try project("Moving"), try project("Staying"), try project("Going"))
        for (n, song) in [moving, staying, going].enumerated() {
            index.note(song, title: ProjectIndex.name(of: song), untitled: n == 1, at: Date(timeIntervalSince1970: Double(n)))
        }
        // The Finder renames one folder into another, and one is deleted.
        let files = FileManager.default
        let renamed = scratch.appendingPathComponent("Elsewhere/Moved")
        try files.createDirectory(at: renamed.deletingLastPathComponent(), withIntermediateDirectories: true)
        try files.moveItem(at: moving.deletingLastPathComponent(), to: renamed)
        try files.removeItem(at: going.deletingLastPathComponent())

        index.resolve { $0 == renamed.appendingPathComponent("song.yaml") }
        let found = Dictionary(uniqueKeysWithValues: index.entries.map { ($0.title, $0) })
        XCTAssertEqual(found["Moving"]?.path, renamed.appendingPathComponent("song.yaml").path)
        XCTAssertEqual(found["Moving"]?.missing, false)
        // Whether it has a name is asked of where it is now.
        XCTAssertEqual(found["Moving"]?.untitled, true)
        XCTAssertEqual(found["Staying"]?.path, staying.path)
        XCTAssertEqual(found["Staying"]?.untitled, false)
        XCTAssertEqual(found["Going"]?.missing, true)
        XCTAssertEqual(found["Going"]?.path, going.path)
        // Its bookmark is the moved project's, so a second move is followed too.
        let again = scratch.appendingPathComponent("Again")
        try files.moveItem(at: renamed, to: again)
        index.resolve { _ in false }
        XCTAssertEqual(index.entries.first { $0.title == "Moving" }?.path, again.appendingPathComponent("song.yaml").path)
        // A project put back is found where it was.
        _ = try project("Going")
        index.resolve { _ in false }
        XCTAssertEqual(index.entries.first { $0.title == "Going" }?.missing, false)

        // Clear Menu keeps what is open and what has no name yet.
        index.note(staying, title: "Staying", untitled: true)
        index.clear(keeping: [again.appendingPathComponent("song.yaml").path])
        XCTAssertEqual(Set(index.entries.map(\.title)), ["Moving", "Staying"])
    }

    func testClosingAsksOnlyAboutAnUntitledProjectThatHoldsSomething() {
        XCTAssertEqual(Closing.of(untitled: false, untouched: false), .close)
        XCTAssertEqual(Closing.of(untitled: false, untouched: true), .close)
        XCTAssertEqual(Closing.of(untitled: true, untouched: true), .delete)
        XCTAssertEqual(Closing.of(untitled: true, untouched: false), .ask)
    }

    /// Lets what the host reported reach the model, until `done`.
    @MainActor
    private func wait(for what: String, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(10)
        while !done(), Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        }
        XCTAssertTrue(done(), what)
    }

    @MainActor
    func testBrowserAddsDevicesAndRefusesIncompatibleRows() throws {
        let model = try SongModel(url: project("Browser"))
        defer { model.close() }
        XCTAssertFalse(model.canAddBrowserDevice("reverb", to: nil))
        XCTAssertFalse(model.canAddBrowserDevice("sampler", to: .master))
        model.addBrowserDevice("sampler", to: nil)
        wait(for: "new sampler track") { model.arrangement.tracks.count == 1 }
        let track = model.arrangement.tracks[0]
        XCTAssertTrue(track.midi)
        XCTAssertEqual(track.instrument, "sampler")
        model.addBrowserDevice("delay", to: .track(track.key))
        wait(for: "delay") { model.arrangement.tracks[0].effects.count == 1 }
        model.addBrowserDevice("filter", to: .track(track.key), index: 0)
        wait(for: "inserted filter") { model.arrangement.tracks[0].effects.count == 2 }
        XCTAssertEqual(model.arrangement.tracks[0].effects.map(\.kind), ["filter", "delay"])
        model.addBrowserDevice("reverb", to: .master)
        wait(for: "master reverb") { model.arrangement.master.effects.count == 1 }
        model.addTrack()
        wait(for: "audio track") { model.arrangement.tracks.count == 2 }
        let audio = try XCTUnwrap(model.arrangement.tracks.first { !$0.midi })
        XCTAssertFalse(model.canAddBrowserDevice("sampler", to: .track(audio.key)))
        XCTAssertEqual(model.browser.library, libraryPath(song: "/another/project/song.yaml"))
    }

    @MainActor
    func testSaveAsMovesAnUntitledProjectAndCopiesANamedOne() throws {
        let made = SongModel.normal(URL(fileURLWithPath: try projectNewUntitled()))
        XCTAssertEqual(ProjectIndex.name(of: made), "Untitled")
        let model = try SongModel(url: made)
        XCTAssertEqual(model.arrangement.title, "Untitled")
        XCTAssertEqual(model.arrangement.tempo, 120)
        XCTAssertEqual(model.arrangement.lengthBeats, 128)
        XCTAssertTrue(model.untitled && model.untouched)
        XCTAssertEqual(Closing.of(untitled: model.untitled, untouched: model.untouched), .delete)

        model.addTrack()
        wait(for: "the track") { model.arrangement.tracks.count == 1 }
        XCTAssertFalse(model.untouched)
        XCTAssertEqual(Closing.of(untitled: model.untitled, untouched: model.untouched), .ask)

        // The model knows its new place when Save As… returns.
        var moves: [URL] = []
        model.onMoved = { moves.append($0) }
        let named = scratch.appendingPathComponent("My Beat")
        try model.saveAs(named)
        XCTAssertEqual(model.url, named.appendingPathComponent("song.yaml"))
        XCTAssertFalse(model.untitled)
        XCTAssertEqual(moves, [made])
        XCTAssertEqual(model.formerURLs, [made])
        XCTAssertFalse(FileManager.default.fileExists(atPath: made.deletingLastPathComponent().path))
        wait(for: "the title") { model.arrangement.title == "My Beat" }
        XCTAssertEqual(model.activity.first?.label, "Saved as My Beat")
        // The host's own report of the move is not a second one.
        XCTAssertEqual(moves, [made])
        // Undo is still the track.
        XCTAssertEqual(model.undoStep?.label, "Add track track-1")
        XCTAssertEqual(Closing.of(untitled: model.untitled, untouched: model.untouched), .close)

        // With a name, Save As… is a copy that the model carries on in.
        let copy = scratch.appendingPathComponent("My Beat copy")
        try model.saveAs(copy)
        XCTAssertEqual(model.url, copy.appendingPathComponent("song.yaml"))
        XCTAssertEqual(model.formerURLs, [made, named.appendingPathComponent("song.yaml")])
        XCTAssertTrue(FileManager.default.fileExists(atPath: named.appendingPathComponent("song.yaml").path))
        wait(for: "the copy's title") { model.arrangement.title == "My Beat copy" }
        // A name that is taken is refused, in the host's words.
        XCTAssertThrowsError(try model.saveAs(named)) { error in
            XCTAssertTrue(SongModel.reason(error).contains("already there"), SongModel.reason(error))
        }
        XCTAssertEqual(model.url, copy.appendingPathComponent("song.yaml"))

        // The original opens once the copy stops answering for its path.
        XCTAssertThrowsError(try SongModel(url: named.appendingPathComponent("song.yaml")))
        model.release(named.appendingPathComponent("song.yaml"))
        XCTAssertEqual(model.formerURLs, [made])
        let original = try SongModel(url: named.appendingPathComponent("song.yaml"))
        XCTAssertEqual(original.arrangement.title, "My Beat")
        original.close()
        model.close()
    }
}
