import XCTest
@testable import AAWApp

final class CommandLineToolTests: XCTestCase {
    private var scratch: URL!

    override func setUpWithError() throws {
        scratch = FileManager.default.temporaryDirectory.appendingPathComponent("aaw tool's \(UUID().uuidString)")
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try FileManager.default.removeItem(at: scratch)
    }

    /// A bundle with an executable and a `daw` that prints `says`, and the
    /// tool that links it into `bin`, a folder that is not there yet.
    private func app(_ name: String, says: String = "daw") throws -> CommandLineTool {
        let contents = scratch.appendingPathComponent("\(name).app/Contents")
        for (path, text) in [("MacOS/AAW", "app"), ("Helpers/daw", says)] {
            let file = contents.appendingPathComponent(path)
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try "#!/bin/sh\necho \(text)\n".write(to: file, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: file.path)
        }
        let tool = CommandLineTool.bundled(executable: contents.appendingPathComponent("MacOS/AAW"),
                                           folder: scratch.appendingPathComponent("bin"))
        return try XCTUnwrap(tool)
    }

    /// What running the link prints.
    private func output(of link: URL) throws -> String {
        let run = Process()
        let pipe = Pipe()
        run.executableURL = link
        run.standardOutput = pipe
        try run.run()
        run.waitUntilExit()
        return String(decoding: pipe.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
    }

    func testTheToolIsTheDawInTheBundle() throws {
        let tool = try app("AAW")
        XCTAssertEqual(tool.binary.path, scratch.appendingPathComponent("AAW.app/Contents/Helpers/daw").path)
        XCTAssertEqual(tool.link.path, scratch.appendingPathComponent("bin/daw").path)
        // A build that is no bundle has none.
        let bare = scratch.appendingPathComponent("build/release/AAW")
        XCTAssertNil(CommandLineTool.bundled(executable: bare))
        XCTAssertEqual(CommandLineTool.folder.path, "/usr/local/bin")
    }

    func testInstallingMakesALinkThatRunsTheTool() throws {
        let tool = try app("AAW")
        XCTAssertEqual(tool.state, .absent)
        XCTAssertFalse(tool.needsAdministrator)
        try tool.install()
        XCTAssertEqual(tool.state, .installed)
        XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: tool.link.path), tool.binary.path)
        XCTAssertEqual(try output(of: tool.link), "daw\n")
        // Again changes nothing.
        try tool.install()
        XCTAssertEqual(tool.state, .installed)
    }

    func testALinkToAnotherAppIsReplaced() throws {
        let old = try app("Old", says: "old")
        let new = try app("New", says: "new")
        try old.install()
        XCTAssertEqual(new.state, .elsewhere(old.binary.path))
        try new.install()
        XCTAssertEqual(new.state, .installed)
        XCTAssertEqual(old.state, .elsewhere(new.binary.path))
        XCTAssertEqual(try output(of: new.link), "new\n")
    }

    func testALinkWhoseAppIsGoneIsStillSeenAndReplaced() throws {
        let old = try app("Old")
        try old.install()
        try FileManager.default.removeItem(at: scratch.appendingPathComponent("Old.app"))
        let new = try app("New", says: "new")
        XCTAssertEqual(new.state, .elsewhere(old.binary.path))
        try new.install()
        XCTAssertEqual(try output(of: new.link), "new\n")
    }

    func testAFileThatIsNoLinkIsLeftAlone() throws {
        let tool = try app("AAW")
        try FileManager.default.createDirectory(at: tool.link.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "theirs".write(to: tool.link, atomically: true, encoding: .utf8)
        XCTAssertEqual(tool.state, .occupied)
        XCTAssertThrowsError(try tool.install()) { XCTAssertEqual($0 as? CommandLineTool.Failure, .occupied) }
        XCTAssertThrowsError(try tool.remove()) { XCTAssertEqual($0 as? CommandLineTool.Failure, .occupied) }
        XCTAssertEqual(try String(contentsOf: tool.link, encoding: .utf8), "theirs")
    }

    func testRemovingTakesTheLinkAndLeavesTheTool() throws {
        let tool = try app("AAW")
        try tool.remove()
        try tool.install()
        try tool.remove()
        XCTAssertEqual(tool.state, .absent)
        XCTAssertTrue(FileManager.default.isExecutableFile(atPath: tool.binary.path))
    }

    func testAFolderThatIsNotThePersonsNeedsAnAdministrator() throws {
        let system = CommandLineTool(binary: try app("AAW").binary, link: URL(fileURLWithPath: "/usr/bin/daw"))
        XCTAssertTrue(system.needsAdministrator)
        XCTAssertEqual(system.state, .absent)
    }

    func testAnAppRunFromATemporaryCopyIsTold() throws {
        let tool = try app("AAW")
        XCTAssertFalse(tool.isTranslocated)
        let moved = CommandLineTool(
            binary: URL(fileURLWithPath: "/private/var/folders/x/T/AppTranslocation/1234/d/AAW.app/Contents/Helpers/daw"),
            link: tool.link)
        XCTAssertTrue(moved.isTranslocated)
    }

    @MainActor
    func testTheAppMenuOffersTheTool() throws {
        _ = NSApplication.shared
        let delegate = AppDelegate(launch: Launch(arguments: []))
        let menu = try XCTUnwrap(delegate.mainMenu().items.first?.submenu)
        let item = try XCTUnwrap(menu.items.first { $0.title == "Install Command Line Tool…" })
        XCTAssertTrue(delegate.responds(to: item.action))
    }

    func testCommandsQuoteTheirPaths() {
        let tool = CommandLineTool(binary: URL(fileURLWithPath: "/Apps/It's mine/AAW.app/Contents/Helpers/daw"),
                                   link: URL(fileURLWithPath: "/usr/local/bin/daw"))
        XCTAssertEqual(
            tool.installCommand,
            "/bin/mkdir -p '/usr/local/bin' && /bin/ln -sfh '/Apps/It'\\''s mine/AAW.app/Contents/Helpers/daw' '/usr/local/bin/daw'")
        XCTAssertEqual(tool.removeCommand, "/bin/rm -f '/usr/local/bin/daw'")
    }
}
