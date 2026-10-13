import XCTest
@testable import AAWApp

final class AgentApprovalTests: XCTestCase {
    private let project = URL(fileURLWithPath: "/Users/someone/Music/AAW/projects/My Song")

    private func decide(_ command: String) -> Approval.Decision {
        Approval.decide(tool: "Bash", input: ["command": command], project: project)
    }

    /// `daw` on the open project runs by itself: as `daw` or `uv run daw`,
    /// with `.` or a path inside the project, with options, and several
    /// commands in one line.
    func testDawOnTheProjectIsAllowed() {
        for command in [
            "daw status .",
            "uv run daw status .",
            "daw clip add . drums verse --at 16",
            "daw set . tracks.bass.gain_db -- -6",
            "daw samples import . samples/kick.wav --as kick",
            "daw map . && daw check .",
            "daw inspect . | head -40",
            "daw render . --to renders/take.wav; daw listen renders/take.wav",
            "daw export . --to=exports/mix.wav",
            "daw export \"\(project.path)\" --to '\(project.path)/exports/mix.wav'",
            "AAW_WORKSPACE=lib daw patch list",
            "daw describe start",
            "daw projects --all",
            "daw",
            "daw inspect . | grep -A3 bass | head",
        ] {
            XCTAssertEqual(decide(command), .allow, command)
        }
    }

    /// Anything else asks: another program, a path outside the project, a
    /// home path, a line the splitter cannot read, and any other tool.
    func testTheRestAsks() {
        for command in [
            "ls",
            "rm -rf samples",
            "daw status ../other-song",
            "daw samples import . ~/Downloads/kick.wav",
            "daw samples import . /Users/someone/Library/kick.wav",
            "daw export . --to=/tmp/mix.wav",
            "daw status . && open .",
            "daw status $(pwd)",
            "daw status . > out.txt",
            "cd .. && daw status My\\ Song",
            "head song.yaml",
            "daw inspect . | tee ../copy.txt",
            "daw inspect . | grep x ../other/song.yaml",
            "",
        ] {
            XCTAssertEqual(decide(command), .ask(command), command)
        }
        XCTAssertEqual(Approval.decide(tool: "Edit", input: ["file_path": "/p/SONG.md", "old_string": "a"], project: project), .ask("Edit /p/SONG.md"))
        XCTAssertEqual(Approval.decide(tool: "WebSearch", input: ["query": "808"], project: project), .ask("WebSearch 808"))
        XCTAssertEqual(Approval.decide(tool: "Agent", input: [:], project: project), .ask("Agent"))
    }

    /// A path inside the project is inside however it is written.
    func testPathsInsideTheProject() {
        XCTAssertFalse(Approval.leadsOutside("samples/kick.wav", project: project))
        XCTAssertFalse(Approval.leadsOutside("./renders/../samples/kick.wav", project: project))
        XCTAssertFalse(Approval.leadsOutside(project.path + "/song.yaml", project: project))
        XCTAssertFalse(Approval.leadsOutside("--to=renders/x.wav", project: project))
        XCTAssertFalse(Approval.leadsOutside("--at", project: project))
        XCTAssertFalse(Approval.leadsOutside("-6", project: project))
        XCTAssertTrue(Approval.leadsOutside("../x", project: project))
        XCTAssertTrue(Approval.leadsOutside("a/../../x", project: project))
        XCTAssertTrue(Approval.leadsOutside("~/x", project: project))
        XCTAssertTrue(Approval.leadsOutside("/Users/someone/Music/AAW/projects/My Song Two/song.yaml", project: project))
        XCTAssertTrue(Approval.leadsOutside("--to=/tmp/x.wav", project: project))
    }

    func testTheSplitterReadsQuotesAndJoins() {
        XCTAssertEqual(Approval.split("daw set . a \"b c\" 'd e'"), [["daw", "set", ".", "a", "b c", "d e"]])
        XCTAssertEqual(Approval.split("daw a && daw b || daw c | daw d; daw e\ndaw f"),
                       [["daw", "a"], ["daw", "b"], ["daw", "c"], ["daw", "d"], ["daw", "e"], ["daw", "f"]])
        XCTAssertEqual(Approval.split("daw My\\ Song"), [["daw", "My Song"]])
        XCTAssertEqual(Approval.split("daw $(x)"), [[]])
        XCTAssertEqual(Approval.split("daw 'unclosed"), [[]])
        XCTAssertEqual(Approval.split("  "), [])
    }
}
