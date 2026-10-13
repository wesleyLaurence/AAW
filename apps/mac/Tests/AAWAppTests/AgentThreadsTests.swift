import XCTest
@testable import AAWApp

final class AgentThreadsTests: XCTestCase {
    private var scratch: URL!

    override func setUpWithError() throws {
        scratch = FileManager.default.temporaryDirectory.appendingPathComponent("aaw threads \(UUID().uuidString)")
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try FileManager.default.removeItem(at: scratch)
    }

    /// The list is kept in `.daw/threads.json`, most recent first, and
    /// read back; a thread noted again comes to the top with its title kept.
    func testTheListIsKeptWithTheProject() throws {
        var list = ThreadList(project: scratch)
        XCTAssertEqual(list.entries, [])
        list.note("a", title: "Make the hats busier", at: Date(timeIntervalSince1970: 100))
        list.note("b", title: "A bass line", at: Date(timeIntervalSince1970: 200))
        XCTAssertEqual(list.entries.map(\.id), ["b", "a"])
        list.note("a", title: "", at: Date(timeIntervalSince1970: 300))
        XCTAssertEqual(list.entries.map(\.id), ["a", "b"])
        XCTAssertEqual(list["a"]?.title, "Make the hats busier")
        XCTAssertEqual(list["a"]?.last, 300)
        XCTAssertEqual(list["a"]?.started, 100)
        try list.save()
        XCTAssertEqual(ThreadList.file(project: scratch).path, scratch.appendingPathComponent(".daw/threads.json").path)
        let read = ThreadList(project: scratch)
        XCTAssertEqual(read.entries, list.entries)
        var renamed = read
        renamed.rename("b", "Bass")
        renamed.remove("a")
        XCTAssertEqual(renamed.entries.map(\.title), ["Bass"])
    }

    /// Claude Code's own sessions for the folder that the list lacks are
    /// added under their first message; one the list has keeps its title.
    func testClaudeCodesOwnSessionsAreMerged() {
        var list = ThreadList(project: scratch)
        list.note("a", title: "Mine", at: Date(timeIntervalSince1970: 100))
        list.merge([
            ClaudeTranscripts.Session(id: "a", title: "Other", modified: 500),
            ClaudeTranscripts.Session(id: "t", title: "From a terminal", modified: 50),
        ])
        XCTAssertEqual(list.entries.map(\.id), ["a", "t"])
        XCTAssertEqual(list["a"]?.title, "Mine")
        XCTAssertEqual(list["t"]?.title, "From a terminal")
        XCTAssertEqual(list["t"]?.started, 50)
    }

    func testATitleIsTheFirstLineCut() {
        XCTAssertEqual(ThreadList.title(from: "\n  Make the hats busier \nand quieter"), "Make the hats busier")
        XCTAssertEqual(ThreadList.title(from: String(repeating: "a", count: 80)), String(repeating: "a", count: 59) + "…")
        XCTAssertEqual(ThreadList.title(from: "  \n "), "")
    }

    /// The app's context goes ahead of the person's words and comes off
    /// again when a message is shown.
    func testTheContextIsWrappedAndStripped() {
        let message = AppContext.wrap(context: "Start position 1.1.1; stopped.\nNothing is selected.", text: "make it darker")
        XCTAssertTrue(message.hasPrefix("<app-context>\nStart position 1.1.1"))
        XCTAssertTrue(message.hasSuffix("</app-context>\n\nmake it darker"))
        XCTAssertEqual(AppContext.strip(message), "make it darker")
        XCTAssertEqual(AppContext.strip("plain words"), "plain words")
        XCTAssertEqual(AppContext.wrap(context: " \n", text: "words"), "words")
    }

    /// Claude Code names a folder's sessions folder by the folder's path
    /// with every other character made `-`.
    func testTheSessionsFolderIsNamedForTheWorkingFolder() {
        XCTAssertEqual(ClaudeTranscripts.folderName(for: "/Users/wes/Dropbox/Wesley Laurence/code/AAW"), "-Users-wes-Dropbox-Wesley-Laurence-code-AAW")
        XCTAssertEqual(ClaudeTranscripts.folderName(for: "/a/b.c/d_e"), "-a-b-c-d-e")
        let config = scratch.appendingPathComponent("config")
        XCTAssertEqual(ClaudeTranscripts.folder(for: URL(fileURLWithPath: "/p/My Song"), config: config).path,
                       config.appendingPathComponent("projects/-p-My-Song").path)
        // As the agent's own process sees its folder: with links resolved,
        // `/private` included, which the system's temporary folder is under.
        XCTAssertTrue(ClaudeTranscripts.folder(for: scratch, config: config).lastPathComponent.hasPrefix("-private-var-"))
        XCTAssertEqual(ClaudeTranscripts.realPath("/no/such/folder"), "/no/such/folder")
        XCTAssertEqual(ClaudeTranscripts.configFolder(environment: ["CLAUDE_CONFIG_DIR": "/x/.cc"]).path, "/x/.cc")
        XCTAssertEqual(ClaudeTranscripts.configFolder(environment: [:], home: URL(fileURLWithPath: "/Users/me")).path, "/Users/me/.claude")
    }

    private let transcript = """
    {"type":"mode","mode":"normal","sessionId":"s1"}
    {"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":"<local-command-caveat>…</local-command-caveat>"},"isMeta":true,"uuid":"1"}
    {"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>"},"uuid":"2"}
    {"type":"user","message":{"role":"user","content":"<app-context>\\nStart position 1.1.1; stopped.\\n</app-context>\\n\\nMake the hats busier"},"uuid":"3"}
    {"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I'll look at the drums first."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"daw map .","description":"Map the song"}}]},"uuid":"4"}
    {"type":"user","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"bars…","is_error":false}]},"uuid":"5"}
    {"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"SONG.md"}}]},"uuid":"6"}
    {"type":"assistant","isSidechain":true,"message":{"role":"assistant","content":[{"type":"text","text":"a subagent's words"}]},"uuid":"7"}
    {"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done: sixteenths on the hats in the chorus."}]},"uuid":"8"}
    {"type":"user","message":{"role":"user","content":[{"type":"text","text":"thanks"}]},"uuid":"9"}
    not json

    """

    /// A session's file is read as the person's messages without the app's
    /// context, the agent's text and the tools it used; tool results, meta
    /// lines, commands and subagents' lines are passed over.
    func testATranscriptIsReadAsMessages() throws {
        let lines = ClaudeTranscripts.parse(Data(transcript.utf8))
        XCTAssertEqual(lines, [
            .person("Make the hats busier"),
            .agent("I'll look at the drums first."),
            .tool(name: "Bash", summary: "daw map ."),
            .tool(name: "Read", summary: "SONG.md"),
            .agent("Done: sixteenths on the hats in the chorus."),
            .person("thanks"),
        ])
    }

    /// The sessions in a folder are listed newest first, titled by the first
    /// message; a file with no message of the person's is left out, and a
    /// folder that is not there lists none.
    func testTheSessionsOfAFolderAreListed() throws {
        let folder = scratch.appendingPathComponent("projects/-p-Song")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try transcript.write(to: folder.appendingPathComponent("s1.jsonl"), atomically: true, encoding: .utf8)
        try "{\"type\":\"mode\",\"mode\":\"normal\"}\n".write(to: folder.appendingPathComponent("empty.jsonl"), atomically: true, encoding: .utf8)
        let second = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"A bass line, please\"}}\n"
        try second.write(to: folder.appendingPathComponent("s2.jsonl"), atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSince1970: 1000)], ofItemAtPath: folder.appendingPathComponent("s1.jsonl").path)
        try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSince1970: 2000)], ofItemAtPath: folder.appendingPathComponent("s2.jsonl").path)
        let sessions = ClaudeTranscripts.sessions(in: folder)
        XCTAssertEqual(sessions.map(\.id), ["s2", "s1"])
        XCTAssertEqual(sessions.map(\.title), ["A bass line, please", "Make the hats busier"])
        XCTAssertEqual(sessions.map(\.modified), [2000, 1000])
        XCTAssertEqual(ClaudeTranscripts.sessions(in: scratch.appendingPathComponent("none")), [])
    }

    func testAToolIsSummedUpInALine() {
        XCTAssertEqual(ClaudeTranscripts.toolSummary(name: "Bash", input: ["command": "daw map .", "description": "Map"]), "daw map .")
        XCTAssertEqual(ClaudeTranscripts.toolSummary(name: "Edit", input: ["file_path": "/p/SONG.md", "old_string": "x"]), "/p/SONG.md")
        XCTAssertEqual(ClaudeTranscripts.toolSummary(name: "Grep", input: ["pattern": "kick"]), "kick")
        XCTAssertEqual(ClaudeTranscripts.toolSummary(name: "Other", input: ["n": 3]), "")
    }
}
