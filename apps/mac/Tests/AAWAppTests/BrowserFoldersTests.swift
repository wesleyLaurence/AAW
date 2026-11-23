import AAWCore
import XCTest
@testable import AAWApp

/// The Folders section as a tree: which rows an open folder makes, what is
/// read from the index, the keys that walk it and the folder a search
/// result names.
final class BrowserFoldersTests: XCTestCase {
    private func sample(_ path: String) -> SampleInfo {
        SampleInfo(id: path, name: (path as NSString).lastPathComponent, pack: "", path: path, seconds: 0.5, channels: 2,
                   category: "", kind: "one-shot", bpm: nil, key: nil, note: nil, rootNote: nil)
    }

    private func folder(_ path: String, _ count: UInt32) -> LibrarySubfolder {
        LibrarySubfolder(name: (path as NSString).lastPathComponent, path: path, count: count)
    }

    /// /lib with bass/ and drums/kicks/, and a sample of its own.
    private var trees: [String: LibraryTree] {
        [
            "/lib": LibraryTree(path: "/lib", folders: [folder("/lib/bass", 1), folder("/lib/drums", 2)], samples: [sample("/lib/top.wav")]),
            "/lib/bass": LibraryTree(path: "/lib/bass", folders: [], samples: [sample("/lib/bass/sub.wav")]),
            "/lib/drums": LibraryTree(path: "/lib/drums", folders: [folder("/lib/drums/kicks", 1)], samples: [sample("/lib/drums/snare.wav")]),
        ]
    }

    /// Each row as `depth name`, a folder with > or v for its arrow.
    private func drawn(_ rows: [FolderRow]) -> [String] {
        rows.map { row in
            let pad = String(repeating: " ", count: row.depth)
            switch row.kind {
            case .folder(_, let name, let count, let open): return "\(pad)\(open ? "v" : ">") \(name)" + (count.map { " \($0)" } ?? "")
            case .sample(let sample): return "\(pad)\(sample.name)"
            case .reading: return "\(pad)…"
            }
        }
    }

    func testOneFolderListsItsContentsWithClosedSubfolders() {
        let rows = Browser.folderRows(roots: ["/lib"], open: [], trees: trees)
        XCTAssertEqual(drawn(rows), ["> bass 1", "> drums 2", "top.wav"], "subfolders first, then the folder's own samples")
    }

    func testAnOpenFolderShowsItsContentsUnderIt() {
        let rows = Browser.folderRows(roots: ["/lib"], open: ["/lib/drums"], trees: trees)
        XCTAssertEqual(drawn(rows), ["> bass 1", "v drums 2", " > kicks 1", " snare.wav", "top.wav"])
        XCTAssertEqual(Browser.unread(roots: ["/lib"], open: ["/lib/drums"], trees: trees), [])
    }

    func testAnOpenFolderNotReadYetIsReadAndShowsSo() {
        let open: Set<String> = ["/lib/drums", "/lib/drums/kicks"]
        let rows = Browser.folderRows(roots: ["/lib"], open: open, trees: trees)
        XCTAssertEqual(drawn(rows), ["> bass 1", "v drums 2", " v kicks 1", "  …", " snare.wav", "top.wav"])
        XCTAssertEqual(Browser.unread(roots: ["/lib"], open: open, trees: trees), ["/lib/drums/kicks"])
        XCTAssertEqual(Browser.unread(roots: ["/lib"], open: [], trees: [:]), ["/lib"], "the only root is read to be shown")
    }

    func testAClosedFolderIsNotReadEvenWhenItsChildIsOpen() {
        // A subfolder left open under a folder that was closed comes back
        // open when its folder opens again, as in the Finder.
        let rows = Browser.folderRows(roots: ["/lib"], open: ["/lib/drums/kicks"], trees: trees)
        XCTAssertEqual(drawn(rows), ["> bass 1", "> drums 2", "top.wav"])
        XCTAssertEqual(Browser.unread(roots: ["/lib"], open: ["/lib/drums/kicks"], trees: trees), [])
    }

    func testSeveralRootsAreFoldersOfTheirOwn() {
        let rows = Browser.folderRows(roots: ["/lib", "/other"], open: ["/lib"], trees: trees)
        XCTAssertEqual(drawn(rows), ["v lib", " > bass 1", " > drums 2", " top.wav", "> other"])
        XCTAssertEqual(Browser.unread(roots: ["/lib", "/other"], open: [], trees: [:]), [], "closed roots are not read")
        // A folder registered inside another and listed under both keeps its rows apart.
        let nested = Browser.folderRows(roots: ["/lib", "/lib/bass"], open: ["/lib", "/lib/bass"], trees: trees)
        let keys = nested.map(\.key)
        XCTAssertEqual(Set(keys).count, keys.count)
    }

    func testTheKeysWalkTheRowsAndSkipOnesBeingRead() {
        let rows = Browser.folderRows(roots: ["/lib"], open: ["/lib/drums", "/lib/drums/kicks"], trees: trees)
        let kicks = rows[2].key
        XCTAssertEqual(Browser.neighborRow(of: kicks, in: rows, by: 1).map { drawn([$0]) }, [" snare.wav"], "past the row being read")
        XCTAssertEqual(Browser.neighborRow(of: nil, in: rows, by: 1)?.key, rows[0].key)
        XCTAssertNil(Browser.neighborRow(of: rows[0].key, in: rows, by: -1), "the first stays")
        XCTAssertNil(Browser.neighborRow(of: rows.last?.key, in: rows, by: 1), "the last stays")
    }

    @MainActor
    func testRightOpensAndLeftClosesOrGoesUp() {
        UserDefaults.standard.removeObject(forKey: "browser.openFolders")
        defer { UserDefaults.standard.removeObject(forKey: "browser.openFolders") }
        // Without an index nothing is read; the folders are given.
        let browser = Browser(library: nil)
        browser.auditions = false
        browser.folders = [LibraryFolder(path: "/lib", available: true)]
        browser.openFolders = []
        browser.trees = trees
        XCTAssertEqual(browser.treeRoots, ["/lib"])
        browser.stepTree(1)
        XCTAssertEqual(drawn(browser.folderRows.filter { $0.key == browser.treeSelected }), ["> bass 1"], "Down from nothing, the first")
        browser.stepTree(1)
        browser.stepTree(into: true)
        XCTAssertEqual(browser.openFolders, ["/lib/drums"], "Right opens")
        XCTAssertEqual(UserDefaults.standard.stringArray(forKey: "browser.openFolders"), ["/lib/drums"], "kept across launches")
        browser.stepTree(into: true)
        XCTAssertEqual(drawn(browser.folderRows.filter { $0.key == browser.treeSelected }), [" > kicks 1"], "Right again goes in")
        browser.stepTree(1)
        XCTAssertEqual(drawn(browser.folderRows.filter { $0.key == browser.treeSelected }), [" snare.wav"])
        browser.stepTree(into: false)
        XCTAssertEqual(drawn(browser.folderRows.filter { $0.key == browser.treeSelected }), ["v drums 2"], "Left from a sample goes to its folder")
        browser.stepTree(into: false)
        XCTAssertEqual(browser.openFolders, [], "Left closes")
        browser.toggle("/lib/bass")
        XCTAssertEqual(browser.openFolders, ["/lib/bass"], "the arrow opens")
    }

    func testASearchResultNamesItsFolderUnderTheRegisteredOne() {
        let roots = ["/Users/me/samples", "/Users/me/samples-old", "/Users/me/samples/drums"]
        XCTAssertEqual(Browser.folder(of: "/Users/me/samples/bass/sub.wav", in: roots), "samples/bass")
        XCTAssertEqual(Browser.folder(of: "/Users/me/samples/top.wav", in: roots), "samples")
        XCTAssertEqual(Browser.folder(of: "/Users/me/samples-old/a/b.wav", in: roots), "samples-old/a", "not samples")
        XCTAssertEqual(Browser.folder(of: "/Users/me/samples/drums/kicks/k.wav", in: roots), "drums/kicks", "the folder nearest it")
        XCTAssertEqual(Browser.folder(of: "/elsewhere/x.wav", in: roots), "elsewhere")
    }
}
