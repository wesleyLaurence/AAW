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
        // A skill is read from the message's start, so the context follows it.
        let command = AppContext.wrap(context: "Nothing is selected.", text: "/roar-edit from :41")
        XCTAssertEqual(command, "/roar-edit from :41\n\n<app-context>\nNothing is selected.\n</app-context>")
        XCTAssertEqual(AppContext.strip(command), "/roar-edit from :41")
        XCTAssertEqual(AppContext.wrap(context: "x", text: "/ not a skill").prefix(13), "<app-context>")
    }

    /// What is attached goes as a list after the context, and comes off
    /// again with it; a message's record gives the attachments back.
    func testAttachmentsAreListedAndReadBack() {
        let attachments = [
            AgentAttachment(kind: .image, name: "shot.png", path: "/Users/x/Desktop/shot.png", detail: "1024×768; seen"),
            AgentAttachment(kind: .sound, name: "kick (1).wav", path: "/Users/x/kick (1).wav", detail: "0.40 s, mono, 44.1 kHz"),
            AgentAttachment(kind: .clip, name: "lead1", reference: "@12", detail: "note clip lead1 on keys at 5.1.1"),
            AgentAttachment(kind: .patch, name: "Soft Pad"),
            AgentAttachment(kind: .folder, name: "takes", path: "/Users/x/takes"),
        ]
        let message = AppContext.wrap(context: "Nothing is selected.", text: "use these", attachments: attachments)
        XCTAssertEqual(message, """
        <app-context>
        Nothing is selected.
        </app-context>

        <attachments>
        - image: /Users/x/Desktop/shot.png (1024×768; seen)
        - sound: /Users/x/kick (1).wav (0.40 s, mono, 44.1 kHz)
        - clip: @12 (note clip lead1 on keys at 5.1.1)
        - patch: Soft Pad
        - folder: /Users/x/takes
        </attachments>

        use these
        """)
        XCTAssertEqual(AppContext.strip(message), "use these")
        let read = AgentAttachments.listed(in: message)
        XCTAssertEqual(read.map(\.kind), [.image, .sound, .clip, .patch, .folder])
        XCTAssertEqual(read.map(\.name), ["shot.png", "kick (1).wav", "@12", "Soft Pad", "takes"])
        XCTAssertEqual(read.map(\.path), ["/Users/x/Desktop/shot.png", "/Users/x/kick (1).wav", nil, nil, "/Users/x/takes"])
        XCTAssertEqual(read[2].reference, "@12")
        XCTAssertEqual(read.map(\.detail), ["1024×768; seen", "0.40 s, mono, 44.1 kHz", "note clip lead1 on keys at 5.1.1", "", ""])
        // A skill's message has both after its words; no context, the list alone.
        let command = AppContext.wrap(context: "x", text: "/roar-edit go", attachments: [attachments[0]])
        XCTAssertTrue(command.hasPrefix("/roar-edit go\n\n<app-context>\nx\n</app-context>\n\n<attachments>\n"), command)
        XCTAssertEqual(AppContext.strip(command), "/roar-edit go")
        XCTAssertEqual(AgentAttachments.listed(in: command).map(\.name), ["shot.png"])
        XCTAssertEqual(AppContext.wrap(context: "", text: "w", attachments: [attachments[3]]), "<attachments>\n- patch: Soft Pad\n</attachments>\n\nw")
        XCTAssertEqual(AgentAttachments.listed(in: "no list"), [])
        XCTAssertNil(AgentAttachment(line: "- nothing: x"))
        XCTAssertNil(AgentAttachment(line: "plain"))
        // The content the engine takes: the words alone with nothing
        // attached, else the images as blocks and the text as one.
        XCTAssertEqual(AgentAttachments.content(text: "t", attachments: []) as? String, "t")
        let blocks = AgentAttachments.content(text: "t", attachments: [attachments[1]]) as? [[String: Any]]
        XCTAssertEqual(blocks?.count, 1)
        XCTAssertEqual(blocks?.first?["type"] as? String, "text")
    }

    /// A message's first word as a skill's name, and the rest.
    func testACommandIsSplitFromItsWords() {
        XCTAssertEqual(AgentCommands.split("/roar-edit from :41")?.name, "roar-edit")
        XCTAssertEqual(AgentCommands.split("/roar-edit from :41")?.rest, " from :41")
        XCTAssertEqual(AgentCommands.split("/plugin:skill")?.name, "plugin:skill")
        XCTAssertEqual(AgentCommands.split("/compact\nkeep the drums")?.rest, "\nkeep the drums")
        XCTAssertNil(AgentCommands.split("/"))
        XCTAssertNil(AgentCommands.split("/roar,edit"))
        XCTAssertNil(AgentCommands.split("a /b"))
        XCTAssertNil(AgentCommands.split(""))
    }

    /// The handshake's commands grouped by the skill folders, with the
    /// engine's own hidden, and filtered by a query.
    func testCommandsAreGroupedAndFiltered() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("aaw skills \(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        for (skills, name) in [(".claude/skills", "one"), (".agents/skills", "two"), (".claude/skills", "no-skill-file")] {
            let dir = folder.appendingPathComponent(skills).appendingPathComponent(name)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            if name != "no-skill-file" { try "".write(to: dir.appendingPathComponent("SKILL.md"), atomically: true, encoding: .utf8) }
        }
        XCTAssertEqual(AgentCommands.skillNames(in: folder), ["one", "two"])
        XCTAssertEqual(AgentCommands.skillNames(in: folder.appendingPathComponent("nowhere")), [])

        let commands = [
            AgentCommand(name: "clear", description: "Start over"),
            AgentCommand(name: "plug:in", description: "(plug) A plugin's"),
            AgentCommand(name: "one", description: "The song's (project)", hint: "[bars]"),
            AgentCommand(name: "__remote", description: "Internal"),
            AgentCommand(name: "two", description: "Every song's (project)"),
            AgentCommand(name: "compact", description: "Free up context"),
        ]
        let sections = AgentCommands.grouped(commands, project: ["one"], workspace: ["two"])
        XCTAssertEqual(sections, [
            .init(group: .project, commands: [AgentCommand(name: "one", description: "The song's", hint: "[bars]")]),
            .init(group: .workspace, commands: [AgentCommand(name: "two", description: "Every song's")]),
            .init(group: .other, commands: [AgentCommand(name: "plug:in", description: "(plug) A plugin's"), AgentCommand(name: "compact", description: "Free up context")]),
        ])
        XCTAssertEqual(AgentCommands.grouped(commands, project: [], workspace: []).map(\.group), [.other])
        XCTAssertEqual(AgentCommands.filter(sections, query: "").count, 3)
        XCTAssertEqual(AgentCommands.filter(sections, query: "SONG").map { $0.commands.map(\.name) }, [["one"], ["two"]], "the description matches too, in any case")
        XCTAssertEqual(AgentCommands.filter(sections, query: "co").map { $0.commands.map(\.name) }, [["compact"]])
        XCTAssertEqual(AgentCommands.filter(sections, query: "zzz"), [])
        // A name that starts with the query comes before one that has it.
        let other = [AgentCommands.Section(group: .other, commands: [AgentCommand(name: "recompact"), AgentCommand(name: "compact"), AgentCommand(name: "x", description: "compact")])]
        XCTAssertEqual(AgentCommands.filter(other, query: "comp").map { $0.commands.map(\.name) }, [["compact", "recompact", "x"]])
    }

    /// A skill the person ran, as Claude Code records it.
    func testARecordedCommandIsReadAsThePersonsLine() {
        let line = "<command-message>roar-edit</command-message>\n<command-name>/roar-edit</command-name>\n<command-args>from :41\n\n<app-context>\nNothing is selected.\n</app-context></command-args>"
        XCTAssertEqual(AgentCommands.recorded(line), "/roar-edit from :41")
        XCTAssertEqual(AgentCommands.recorded("<command-message>compact</command-message>\n<command-name>/compact</command-name>\n<command-args></command-args>"), "/compact")
        XCTAssertNil(AgentCommands.recorded("<command-name>/clear</command-name>\n<local-command-stdout></local-command-stdout>"), "a local command is not the person's line")
        XCTAssertNil(AgentCommands.recorded("plain words"))
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
    {"type":"user","message":{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}},{"type":"text","text":"<app-context>\\nNothing is selected.\\n</app-context>\\n\\n<attachments>\\n- image: /x/shot.png (8×8; seen)\\n</attachments>\\n\\nthanks"}]},"uuid":"9"}
    {"type":"user","message":{"role":"user","content":"<command-message>roar-edit</command-message>\\n<command-name>/roar-edit</command-name>\\n<command-args>from :41\\n\\n<app-context>\\nNothing is selected.\\n</app-context></command-args>"},"uuid":"10"}
    {"type":"user","message":{"role":"user","content":[{"type":"text","text":"Base directory for this skill: /x\\n\\nMake the edit.\\n\\nARGUMENTS: from :41"}]},"isMeta":true,"uuid":"11"}
    not json

    """

    /// A session's file is read as the person's messages without the app's
    /// context, the agent's text and the tools it used; tool results, meta
    /// lines, commands and subagents' lines are passed over.
    func testATranscriptIsReadAsMessages() throws {
        let lines = ClaudeTranscripts.parse(Data(transcript.utf8))
        XCTAssertEqual(lines, [
            .person("Make the hats busier", []),
            .agent("I'll look at the drums first."),
            .tool(name: "Bash", summary: "daw map ."),
            .tool(name: "Read", summary: "SONG.md"),
            .agent("Done: sixteenths on the hats in the chorus."),
            .person("thanks", [AgentAttachment(kind: .image, name: "shot.png", path: "/x/shot.png", detail: "8×8; seen")]),
            .person("/roar-edit from :41", []),
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
