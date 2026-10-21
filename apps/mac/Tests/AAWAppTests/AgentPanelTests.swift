import AAWCore
import XCTest
@testable import AAWApp

/// The agent panel over a program standing in for Claude Code: what a
/// conversation is started with, what each message carries, how a request
/// is answered, how threads are kept and resumed, and what being signed out
/// looks like. The stand-in speaks the stream-json protocol as Claude Code
/// does and writes what it was given to a log.
@MainActor
final class AgentPanelTests: XCTestCase {
    private var scratch: URL!
    private var log: URL!

    private nonisolated static let folders: URL = {
        let root = URL(fileURLWithPath: "/tmp/aaw-agent-\(UUID().uuidString.prefix(8))")
        setenv("AAW_HOST_DIR", root.appendingPathComponent("hosts").path, 1)
        setenv("AAW_DATA_DIR", root.appendingPathComponent("data").path, 1)
        setenv("AAW_WORKSPACE", root.appendingPathComponent("workspace").path, 1)
        return root
    }()

    /// A program that answers as Claude Code does on its pipes: a handshake
    /// answered with five commands, then for each message a streamed greeting, a `daw` command that the
    /// panel allows by itself, a `touch` that it asks about, and a result;
    /// "short" is answered with words alone, "blocks" with a message of a
    /// heading, a list and a fence, three `daw` commands in a row and a
    /// last word, and "signout" as a session with no login.
    nonisolated static let fake = """
    #!/usr/bin/env python3
    import json, os, sys
    log = open(os.environ["AAW_FAKE_LOG"], "a")
    log.write(json.dumps({"argv": sys.argv[1:], "cwd": os.getcwd(), "path": os.environ.get("PATH", "")}) + "\\n")
    log.flush()
    def out(o):
        sys.stdout.write(json.dumps(o) + "\\n")
        sys.stdout.flush()
    def ev(e):
        out({"type": "stream_event", "api_message_id": "m1", "event": e})
    stopped = False
    BLOCKS = "## What I did\\n\\nMoved the hats, then:\\n\\n- Quieter by **3 dB**\\n- Off the beat\\n\\n```sh\\ndaw status .\\n```\\n"
    pending = []
    for line in sys.stdin:
        m = json.loads(line)
        log.write(line)
        log.flush()
        t = m.get("type")
        if t == "control_request":
            sub = m["request"]["subtype"]
            answer = {}
            if sub == "initialize":
                answer = {"commands": [
                    {"name": "song-skill", "description": "Edit this song's hook (project)", "argumentHint": "[bars]"},
                    {"name": "for-all", "description": "For every song (project)", "argumentHint": ""},
                    {"name": "roar", "description": "Mine", "argumentHint": "[times]"},
                    {"name": "clear", "description": "Start a new session", "argumentHint": "[name]"},
                    {"name": "compact", "description": "Free up context", "argumentHint": "<instructions>"}]}
            out({"type": "control_response", "response": {"subtype": "success", "request_id": m["request_id"], "response": answer}})
            if sub == "interrupt":
                stopped = True
                out({"type": "result", "subtype": "error_during_execution", "is_error": True, "result": None, "session_id": "s"})
        elif t == "user":
            stopped = False
            text = m["message"]["content"]
            if isinstance(text, list):
                text = "\\n".join(b["text"] for b in text if b["type"] == "text")
            for tag in ("</attachments>", "</app-context>"):
                if text.endswith(tag):
                    text = text[:text.rfind(tag.replace("/", ""))].strip()
            if text.startswith("<attachments>"):
                text = text[text.find("</attachments>") + 14:].strip()
            out({"type": "system", "subtype": "init", "session_id": "s"})
            if text.endswith("signout"):
                out({"type": "result", "subtype": "error", "is_error": True, "result": "Not logged in · Please run /login", "session_id": "s"})
                continue
            if text.endswith("blocks"):
                ev({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}})
                for piece in BLOCKS.split(" "):
                    ev({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": piece + " "}})
                ev({"type": "content_block_stop", "index": 0})
                out({"type": "assistant", "message": {"id": "m1", "role": "assistant", "content": [
                    {"type": "text", "text": BLOCKS},
                    {"type": "tool_use", "id": "b1", "name": "Bash", "input": {"command": "daw status ."}},
                    {"type": "tool_use", "id": "b2", "name": "Bash", "input": {"command": "daw map ."}},
                    {"type": "tool_use", "id": "b3", "name": "Bash", "input": {"command": "daw check ."}}]}})
                pending = ["b1", "b2", "b3"]
                out({"type": "control_request", "request_id": "b1", "request": {"subtype": "can_use_tool", "tool_name": "Bash", "input": {"command": "daw status ."}}})
                continue
            ev({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}})
            ev({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Hello "}})
            ev({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "from the fake"}})
            ev({"type": "content_block_stop", "index": 0})
            if text.endswith("short"):
                out({"type": "assistant", "message": {"id": "m1", "role": "assistant", "content": [{"type": "text", "text": "Hello from the fake"}]}})
                out({"type": "result", "subtype": "success", "is_error": False, "result": "Hello from the fake", "session_id": "s"})
                continue
            out({"type": "assistant", "message": {"id": "m1", "role": "assistant", "content": [
                {"type": "text", "text": "Hello from the fake"},
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "daw status .", "description": "Status"}}]}})
            out({"type": "control_request", "request_id": "r1", "request": {"subtype": "can_use_tool", "tool_name": "Bash", "input": {"command": "daw status ."}}})
        elif t == "control_response":
            if stopped:
                continue
            rid = m["response"]["request_id"]
            allowed = m["response"]["response"].get("behavior") == "allow"
            if rid in pending:
                pending.remove(rid)
                out({"type": "user", "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": rid, "content": "ok " + rid, "is_error": False}]}})
                if pending:
                    cmd = {"b2": "daw map .", "b3": "daw check ."}[pending[0]]
                    out({"type": "control_request", "request_id": pending[0], "request": {"subtype": "can_use_tool", "tool_name": "Bash", "input": {"command": cmd}}})
                else:
                    out({"type": "assistant", "message": {"id": "m4", "role": "assistant", "content": [{"type": "text", "text": "Done."}]}})
                    out({"type": "result", "subtype": "success", "is_error": False, "result": "Done.", "session_id": "s"})
            elif rid == "r1":
                out({"type": "user", "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "host: true" if allowed else "refused", "is_error": not allowed}]}})
                out({"type": "assistant", "message": {"id": "m2", "role": "assistant", "content": [{"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": "touch x"}}]}})
                out({"type": "control_request", "request_id": "r2", "request": {"subtype": "can_use_tool", "tool_name": "Bash", "input": {"command": "touch x"}}})
            elif rid == "r2":
                out({"type": "user", "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t2", "content": "made" if allowed else "The person refused", "is_error": not allowed}]}})
                out({"type": "assistant", "message": {"id": "m3", "role": "assistant", "content": [{"type": "text", "text": "All done."}]}})
                out({"type": "result", "subtype": "success", "is_error": False, "result": "All done.", "session_id": "s"})
    """

    override func setUp() async throws {
        _ = Self.folders
        scratch = SongModel.normal(FileManager.default.temporaryDirectory.appendingPathComponent("aaw agent \(UUID().uuidString)"))
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
        let program = scratch.appendingPathComponent("fake-claude")
        try Self.fake.write(to: program, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: program.path)
        log = scratch.appendingPathComponent("fake.log")
        setenv(ClaudeCode.override, program.path, 1)
        setenv("AAW_FAKE_LOG", log.path, 1)
        // Claude Code's record of sessions, read for a resumed thread, in
        // the scratch folder rather than the person's own.
        setenv("CLAUDE_CONFIG_DIR", scratch.appendingPathComponent("claude").path, 1)
    }

    override func tearDown() async throws {
        unsetenv(ClaudeCode.override)
        unsetenv("AAW_FAKE_LOG")
        unsetenv("CLAUDE_CONFIG_DIR")
        try FileManager.default.removeItem(at: scratch)
    }

    override class func tearDown() {
        try? FileManager.default.removeItem(at: folders)
    }

    /// A workspace in the scratch folder, with a project under it.
    private func workspaceAndProject() throws -> URL {
        try "Use daw.\n".write(to: scratch.appendingPathComponent("AGENTS.md"), atomically: true, encoding: .utf8)
        try FileManager.default.createDirectory(at: scratch.appendingPathComponent(".claude/rules"), withIntermediateDirectories: true)
        try "Keep vocals dry.\n".write(to: scratch.appendingPathComponent(".claude/rules/instructions.md"), atomically: true, encoding: .utf8)
        let song = scratch.appendingPathComponent("projects/My Song/song.yaml")
        try FileManager.default.createDirectory(at: song.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "session: {length_beats: 16}\n".write(to: song, atomically: true, encoding: .utf8)
        return song
    }

    /// Spins the main run loop until the condition holds, or fails.
    private func wait(_ what: String, timeout: Double = 15, until done: () -> Bool) {
        let end = Date().addingTimeInterval(timeout)
        while !done() {
            if Date() > end { return XCTFail("Waited \(timeout) s for \(what)") }
            RunLoop.main.run(until: Date().addingTimeInterval(0.02))
        }
    }

    /// What the stand-in logged: each start's arguments, and each line it was given.
    private func logged() throws -> [[String: Any]] {
        try String(contentsOf: log, encoding: .utf8).split(separator: "\n").compactMap {
            try? JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any]
        }
    }

    private func starts() throws -> [[String]] {
        try logged().compactMap { $0["argv"] as? [String] }
    }

    private func messages() throws -> [String] {
        try logged().compactMap { ($0["message"] as? [String: Any])?["content"] as? String }
    }

    func testAConversationStreamsRunsDawAndAsksAboutTheRest() throws {
        let song = try workspaceAndProject()
        let model = try SongModel(url: song)
        defer { model.close() }
        let agent = model.agent
        XCTAssertNil(agent.currentID)
        XCTAssertEqual(agent.threads, [])

        agent.draft = "hello"
        agent.send()
        XCTAssertTrue(agent.working)
        wait("the request") { agent.entries.contains { if case .approval(_, _, _, nil) = $0 { true } else { false } } }
        let shown = agent.entries.map(\.description)
        XCTAssertEqual(shown, [
            "you: hello",
            "agent: Hello from the fake",
            "tool Bash daw status . → host: true",
            "tool Bash touch x",
            "ask touch x",
        ])
        XCTAssertTrue(agent.working)

        // The stand-in was started in the project's folder, under the
        // thread's ID, with the workspace attached, and given the message
        // with the app's context ahead of it.
        let started = try XCTUnwrap(try starts().first)
        let id = try XCTUnwrap(agent.currentID)
        XCTAssertTrue(started.contains("--session-id") && started.contains(id), "\(started)")
        XCTAssertFalse(started.contains("--resume"))
        XCTAssertEqual(started.firstIndex(of: "--add-dir").map { started[$0 + 1] }, scratch.path)
        XCTAssertEqual((try logged().first)?["cwd"] as? String, ClaudeTranscripts.realPath(song.deletingLastPathComponent().path))
        let prompt = try XCTUnwrap(started.firstIndex(of: "--append-system-prompt").map { started[$0 + 1] })
        XCTAssertTrue(prompt.contains(song.deletingLastPathComponent().path))
        XCTAssertFalse(prompt.contains("Keep vocals dry"), "a project under the workspace has its instructions by location")
        let message = try XCTUnwrap(try messages().first)
        XCTAssertTrue(message.hasPrefix(AppContext.open), message)
        XCTAssertTrue(message.hasSuffix("\n\nhello"), message)
        XCTAssertTrue(message.contains("Start position 1.1.1; no loop; stopped."), message)
        XCTAssertTrue(message.contains("Nothing is selected."), message)
        XCTAssertEqual(AppContext.strip(message), "hello")

        // The person refuses: the tool fails, the agent finishes, and the
        // thread is kept with the project under the first line typed.
        guard case .approval(_, let request, _, _)? = agent.entries.last else { return XCTFail("no request") }
        agent.answer(request, allow: false)
        wait("the turn's end") { !agent.working }
        XCTAssertEqual(agent.entries.map(\.description), [
            "you: hello",
            "agent: Hello from the fake",
            "tool Bash daw status . → host: true",
            "tool Bash touch x ✗ The person refused",
            "refused touch x",
            "agent: All done.",
        ])
        XCTAssertEqual(agent.threads.map(\.title), ["hello"])
        XCTAssertEqual(ThreadList(project: song.deletingLastPathComponent()).entries.map(\.id), [id])
        XCTAssertEqual(agent.status, .ready)

        // A second message goes to the same process.
        agent.send("again short")
        wait("the second turn") { !agent.working }
        XCTAssertEqual(try starts().count, 1)
        XCTAssertEqual(try messages().count, 2)
        XCTAssertEqual(agent.entries.suffix(2).map(\.description), ["you: again short", "agent: Hello from the fake"])
    }

    func testThreadsAreStartedResumedAndListed() throws {
        let song = try workspaceAndProject()
        let model = try SongModel(url: song)
        defer { model.close() }
        let agent = model.agent

        agent.send("first short")
        wait("the first turn") { !agent.working }
        let first = try XCTUnwrap(agent.currentID)

        agent.newThread()
        XCTAssertNil(agent.currentID)
        XCTAssertEqual(agent.entries, [])
        agent.send("second short")
        wait("the second turn") { !agent.working }
        let second = try XCTUnwrap(agent.currentID)
        XCTAssertNotEqual(first, second)
        XCTAssertEqual(agent.threads.map(\.title), ["second short", "first short"])
        XCTAssertEqual(try starts().count, 2)
        XCTAssertTrue(try starts()[1].contains(second))

        // Back to the first: its earlier messages are Claude Code's own
        // record, which the stand-in does not keep, and the process resumes it.
        agent.open(first)
        XCTAssertEqual(agent.currentID, first)
        XCTAssertEqual(agent.entries.count, 1)
        guard case .note? = agent.entries.first else { return XCTFail("no note") }
        agent.send("third short")
        wait("the third turn") { !agent.working }
        let resumed = try starts()[2]
        XCTAssertEqual(resumed.firstIndex(of: "--resume").map { resumed[$0 + 1] }, first)
        XCTAssertFalse(resumed.contains("--session-id"))
        XCTAssertEqual(agent.threads.map(\.title), ["first short", "second short"])

        // Opened again, a project shows its most recent thread.
        model.close()
        let again = try SongModel(url: song)
        defer { again.close() }
        XCTAssertEqual(again.agent.currentID, first)
        XCTAssertEqual(again.agent.threads.map(\.title), ["first short", "second short"])
    }

    /// A message of blocks streams into one entry, and three `daw`
    /// commands in a row, each allowed by the panel, fold into one row.
    func testBlocksStreamAndToolsFold() throws {
        let song = try workspaceAndProject()
        let model = try SongModel(url: song)
        defer { model.close() }
        let agent = model.agent
        agent.send("blocks")
        wait("the turn") { !agent.working }
        XCTAssertNil(agent.streamingID)
        guard case .agent(_, let text)? = agent.entries.dropFirst().first else { return XCTFail("\(agent.entries)") }
        XCTAssertEqual(MarkdownBlocks.parse(text), [
            .heading(level: 2, text: "What I did"),
            .paragraph("Moved the hats, then:"),
            .list([.init(depth: 0, ordered: false, number: 0, text: "Quieter by **3 dB**"), .init(depth: 0, ordered: false, number: 0, text: "Off the beat")]),
            .code(language: "sh", text: "daw status .", closed: true),
        ])
        XCTAssertEqual(agent.entries.map(\.description).suffix(4), [
            "tool Bash daw status . → ok b1",
            "tool Bash daw map . → ok b2",
            "tool Bash daw check . → ok b3",
            "agent: Done.",
        ])
        let rows = AgentPanel.fold(agent.entries)
        XCTAssertEqual(rows.count, 4)
        guard case .tools(let tools) = rows[2] else { return XCTFail("\(rows[2])") }
        XCTAssertEqual(tools.count, 3)
        XCTAssertEqual(rows[2].id, tools[0].id)
    }

    /// The handshake's commands are listed under `/` before the first
    /// message, in groups told by the skill folders, filtered as the person
    /// types; the chosen one is a chip, and sends as `/name words` with the
    /// app's context after it, through the process warmed for the thread.
    func testSkillsAreListedChosenAndSent() throws {
        let song = try workspaceAndProject()
        for (folder, name) in [(song.deletingLastPathComponent(), "song-skill"), (scratch!, "for-all")] {
            let dir = folder.appendingPathComponent(".claude/skills/\(name)")
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            try "---\nname: \(name)\n---\n".write(to: dir.appendingPathComponent("SKILL.md"), atomically: true, encoding: .utf8)
        }
        let model = try SongModel(url: song)
        defer { model.close() }
        model.showsAgent = true
        let agent = model.agent
        wait("the handshake") { agent.handshook }
        XCTAssertEqual(try starts().count, 1, "the process was started before any message")
        XCTAssertNil(agent.currentID)
        XCTAssertEqual(agent.sections.map(\.group), [.project, .workspace, .other])
        XCTAssertEqual(agent.sections.map { $0.commands.map(\.name) }, [["song-skill"], ["for-all"], ["roar", "compact"]], "clear is hidden")
        XCTAssertEqual(agent.sections[0].commands[0].description, "Edit this song's hook", "the engine's (project) comes off")
        XCTAssertEqual(agent.sections[0].commands[0].hint, "[bars]")

        // `/` lists all; typing filters by the name, then the description.
        XCTAssertNil(agent.commandQuery)
        agent.draft = "/"
        XCTAssertEqual(agent.commandQuery, "")
        XCTAssertEqual(agent.listed.flatMap(\.commands).map(\.name), ["song-skill", "for-all", "roar", "compact"])
        agent.draft = "/co"
        XCTAssertEqual(agent.listed.flatMap(\.commands).map(\.name), ["compact"])
        agent.draft = "/song"
        XCTAssertEqual(agent.listed.flatMap(\.commands).map(\.name), ["song-skill", "for-all"])
        XCTAssertTrue(agent.moveSelection(by: 1))
        XCTAssertEqual(agent.commandSelection, 1)
        XCTAssertTrue(agent.moveSelection(by: 1))
        XCTAssertEqual(agent.commandSelection, 0, "the selection wraps")
        XCTAssertTrue(agent.chooseSelected())
        XCTAssertEqual(agent.chip?.name, "song-skill")
        XCTAssertEqual(agent.draft, "/song-skill ")
        XCTAssertNil(agent.commandQuery)
        agent.draft = "/song-skill 8 bars"
        XCTAssertEqual(agent.chip?.name, "song-skill", "the chip stays while the draft starts with its name")
        agent.draft = "/song-skil"
        XCTAssertNil(agent.chip, "and goes once it does not")
        XCTAssertEqual(agent.commandQuery, "song-skil")

        // Escape closes the list and leaves the text; the next key opens it.
        XCTAssertTrue(agent.closeList())
        XCTAssertNil(agent.commandQuery)
        XCTAssertFalse(agent.closeList())
        XCTAssertFalse(agent.moveSelection(by: 1))
        XCTAssertFalse(agent.chooseSelected())
        agent.draft = "/song-skill"
        XCTAssertEqual(agent.commandQuery, "song-skill")
        agent.draft = "a /b"
        XCTAssertNil(agent.commandQuery, "a slash elsewhere is a slash")

        agent.draft = "/ro"
        XCTAssertTrue(agent.chooseSelected())
        agent.draft = "/roar 8 bars short"
        agent.send()
        XCTAssertEqual(agent.draft, "")
        XCTAssertNil(agent.chip)
        wait("the turn") { !agent.working }
        XCTAssertEqual(try starts().count, 1, "the warmed process took the message")
        XCTAssertEqual(agent.threads.map(\.title), ["/roar 8 bars short"])
        let message = try XCTUnwrap(try messages().first)
        XCTAssertTrue(message.hasPrefix("/roar 8 bars short\n\n\(AppContext.open)\n"), message)
        XCTAssertTrue(message.hasSuffix(AppContext.close), message)
        XCTAssertEqual(AppContext.strip(message), "/roar 8 bars short")
        XCTAssertEqual(agent.entries.first?.description, "you: /roar 8 bars short")

        // A new conversation is warmed too, and keeps the list meanwhile.
        agent.newThread()
        XCTAssertEqual(agent.sections.count, 3)
        wait("the second process") { (try? self.starts().count) == 2 }
        wait("its handshake") { agent.handshook }
    }

    /// Files attached go with the next message: an image as a block the
    /// engine sees, scaled and typed, and every file as a line of the
    /// list after the context; the chips are on the person's row; a file
    /// attached is one the agent may then read without a card; the
    /// selected clips attach by reference.
    func testFilesAndClipsAttachToTheNextMessage() throws {
        let song = scratch.appendingPathComponent("projects/My Song/song.yaml")
        try FileManager.default.createDirectory(at: song.deletingLastPathComponent(), withIntermediateDirectories: true)
        try """
        session: {length_beats: 16}
        tracks:
        - id: keys
          type: midi
          clips:
          - at: 4
            length_beats: 4
            id: lead1
            notes:
            - {pitch: 60, duration: 1}
            - {pitch: 64, at: 1, duration: 1}

        """.write(to: song, atomically: true, encoding: .utf8)
        let files = try AgentAttachmentsTests.files(in: scratch)
        let model = try SongModel(url: song)
        defer { model.close() }
        let agent = model.agent

        agent.attach(file: files.png)
        agent.attach(file: files.png)
        agent.attach(file: files.wav)
        agent.attach(file: files.mid)
        agent.attach(file: files.mov)
        XCTAssertEqual(agent.attachments.map(\.name), ["shot.png", "kick.wav", "two.mid", "clip.mov"], "a file attaches once")
        XCTAssertEqual(agent.attachments.map(\.kind), [.image, .sound, .midi, .video])
        XCTAssertEqual(agent.attachments[0].detail, "40×20; seen")
        XCTAssertNotNil(agent.attachments[0].picture)
        XCTAssertEqual(agent.attachments[1].detail, "0.50 s, mono, 44.1 kHz")
        XCTAssertEqual(agent.attachments[2].detail, "2 tracks: Piano, Bass; 3 notes")
        XCTAssertTrue(agent.attachments[3].detail.hasSuffix("not read"), agent.attachments[3].detail)
        agent.removeAttachment(agent.attachments[3].id)
        XCTAssertEqual(agent.attachments.count, 3)

        // The selected clip, by its reference.
        XCTAssertFalse(agent.canAttachClips)
        let clip = try XCTUnwrap(model.placedClips().first)
        model.select(clips: [clip.key])
        XCTAssertTrue(agent.canAttachClips)
        agent.attachSelectedClips()
        agent.attachSelectedClips()
        XCTAssertEqual(agent.attachments.count, 4, "a clip attaches once")
        let attachedClip = agent.attachments[3]
        XCTAssertEqual(attachedClip.kind, .clip)
        XCTAssertEqual(attachedClip.name, "lead1")
        XCTAssertEqual(attachedClip.reference, clip.notes?.reference)
        XCTAssertEqual(attachedClip.detail, "note clip lead1 on keys at 2.1.1, 4 beats, 2 notes")

        agent.draft = "use these short"
        agent.send()
        XCTAssertEqual(agent.attachments, [])
        XCTAssertEqual(agent.entries.first?.description, "you: use these short + shot.png, kick.wav, two.mid, lead1")
        wait("the turn") { !agent.working }
        XCTAssertEqual(agent.threads.map(\.title), ["use these short"])

        // What the stand-in was given: the image first, then one text block
        // with the context, the list and the words.
        let content = try XCTUnwrap(try logged().compactMap { ($0["message"] as? [String: Any])?["content"] as? [[String: Any]] }.first)
        XCTAssertEqual(content.map { $0["type"] as? String }, ["image", "text"])
        let source = try XCTUnwrap(content[0]["source"] as? [String: Any])
        XCTAssertEqual(source["type"] as? String, "base64")
        XCTAssertEqual(source["media_type"] as? String, "image/png")
        let data = try XCTUnwrap(Data(base64Encoded: source["data"] as? String ?? ""))
        let image = try XCTUnwrap(NSBitmapImageRep(data: data))
        XCTAssertEqual(image.pixelsWide, 40)
        XCTAssertEqual(image.pixelsHigh, 20)
        let text = try XCTUnwrap(content[1]["text"] as? String)
        XCTAssertTrue(text.hasPrefix(AppContext.open), text)
        XCTAssertTrue(text.hasSuffix("\n\nuse these short"), text)
        let list = try XCTUnwrap(text.range(of: "\n\n<attachments>\n").map { text[$0.upperBound...] })
        XCTAssertTrue(list.hasPrefix("""
        - image: \(files.png.path) (40×20; seen)
        - sound: \(files.wav.path) (0.50 s, mono, 44.1 kHz)
        - midi: \(files.mid.path) (2 tracks: Piano, Bass; 3 notes)
        - clip: \(clip.notes?.reference ?? "") (note clip lead1 on keys at 2.1.1, 4 beats, 2 notes)
        </attachments>
        """), String(list))
        XCTAssertEqual(AppContext.strip(text), "use these short")

        // Reading an attached file needs no card; writing to it does.
        let attached = agent.attachedPaths
        XCTAssertEqual(attached, [files.png.path, files.wav.path, files.mid.path])
        let project = song.deletingLastPathComponent()
        XCTAssertEqual(Approval.decide(tool: "Read", input: ["file_path": files.png.path], project: project, attached: attached), .allow)
        XCTAssertEqual(Approval.decide(tool: "Bash", input: ["command": "daw samples inspect \"\(files.wav.path)\""], project: project, attached: attached), .allow)
        XCTAssertEqual(Approval.decide(tool: "Bash", input: ["command": "afinfo '\(files.wav.path)'"], project: project, attached: attached), .allow)
        XCTAssertEqual(Approval.decide(tool: "Bash", input: ["command": "cp '\(files.wav.path)' samples/"], project: project, attached: attached), .ask("cp '\(files.wav.path)' samples/"))
        XCTAssertEqual(Approval.decide(tool: "Write", input: ["file_path": files.png.path], project: project, attached: attached), .ask("Write \(files.png.path)"))

        // Attachments alone send with a word for them; too many images are refused.
        agent.attach(file: files.wav)
        agent.send()
        XCTAssertEqual(agent.entries.last(where: { if case .person = $0 { true } else { false } })?.description, "you: Here is what I attached. + kick.wav")
        wait("the stand-in's request") { agent.entries.contains { if case .approval(_, _, _, nil) = $0 { true } else { false } } }
        if case .approval(_, let request, _, _)? = agent.entries.last { agent.answer(request, allow: false) }
        wait("the second turn") { !agent.working }
        for n in 0..<AgentAttachments.mostImages {
            let copy = scratch.appendingPathComponent("shot\(n).png")
            try FileManager.default.copyItem(at: files.png, to: copy)
            agent.attach(file: copy)
        }
        agent.attach(file: files.png)
        XCTAssertEqual(agent.attachments.count, AgentAttachments.mostImages)
        XCTAssertEqual(model.refusal, "Ten images go with one message at the most")

        // A resumed conversation draws the chips from the record. The
        // stand-in keeps no record, so one is written as Claude Code writes it.
        let id = try XCTUnwrap(agent.currentID)
        let record = ClaudeTranscripts.folder(for: project).appendingPathComponent("\(id).jsonl")
        try FileManager.default.createDirectory(at: record.deletingLastPathComponent(), withIntermediateDirectories: true)
        let line = try JSONSerialization.data(withJSONObject: ["type": "user", "message": ["role": "user", "content": content], "uuid": "1"])
        try (String(decoding: line, as: UTF8.self) + "\n").write(to: record, atomically: true, encoding: .utf8)
        agent.newThread()
        XCTAssertEqual(agent.attachedPaths, [])
        agent.open(id)
        XCTAssertEqual(agent.entries.map(\.description), ["you: use these short + shot.png, kick.wav, two.mid, lead1"], "the clip's chip is named from the record's line")
        XCTAssertEqual(agent.attachedPaths, [files.png.path, files.wav.path, files.mid.path])
        try FileManager.default.removeItem(at: record.deletingLastPathComponent())
    }

    /// A transcriber the test speaks through, a word at a time.
    @MainActor
    final class HandTranscriber: Transcriber {
        var availability: Availability = .ready
        var heard: (@MainActor (String, Bool) -> Void)?
        var level: (@MainActor (Float) -> Void)?
        var ended: (@MainActor (String?) -> Void)?
        var said = ""
        var starts = 0
        var stops = 0

        func start(heard: @escaping @MainActor (String, Bool) -> Void, level: @escaping @MainActor (Float) -> Void,
                   ended: @escaping @MainActor (String?) -> Void) {
            starts += 1
            said = ""
            (self.heard, self.level, self.ended) = (heard, level, ended)
        }

        /// The recognizer's next hypothesis: the whole so far.
        func say(_ text: String, final: Bool = false) {
            said = text
            level?(0.7)
            heard?(text, final)
            if final { end() }
        }

        func stop() {
            stops += 1
            heard?(said, true)
            end()
        }

        private func end() {
            guard let ended else { return }
            (heard, level, self.ended) = (nil, nil, nil)
            ended(nil)
        }
    }

    /// `@` in the field lists the song's things: the rows, the clips by
    /// their names, the samples, the sections, the markers, the library's
    /// references and the project's files, filtered as the person types;
    /// the one chosen is `@name` in the draft with the caret after it, and
    /// each thing the words name goes with the message as a line of the
    /// list with the reference `daw` takes, the chips on the person's row.
    func testTheSongsThingsAreListedUnderAtAndSent() throws {
        let song = scratch.appendingPathComponent("projects/My Song/song.yaml")
        let project = song.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: project.appendingPathComponent("samples"), withIntermediateDirectories: true)
        let files = try AgentAttachmentsTests.files(in: project.appendingPathComponent("samples"))
        try "What the song is for.\n".write(to: project.appendingPathComponent("SONG.md"), atomically: true, encoding: .utf8)
        try FileManager.default.createDirectory(at: Self.folders.appendingPathComponent("workspace/library/references/Night Drive"), withIntermediateDirectories: true)
        try """
        session: {length_beats: 16}
        samples:
          kick: {path: samples/kick.wav}
          snare: {path: samples/kick.wav, root_note: C2}
        patterns:
          hook:
            length_beats: 4
            steps: {k: x...x...x...x...}
        tracks:
        - id: drums
          pads: {k: {sample: kick}}
          clips:
          - {pattern: hook}
          - {pattern: hook, at: 4}
        - id: keys
          type: midi
          clips:
          - at: 8
            length_beats: 4
            id: lead1
            notes:
            - {pitch: 60, duration: 1}
        sections:
        - {id: verse, at: 0, length_beats: 8}
        markers:
        - {id: m1, at: 4, text: too busy}

        """.write(to: song, atomically: true, encoding: .utf8)
        let model = try SongModel(url: song)
        defer { model.close() }
        let agent = model.agent

        // Everything the song and the project offer, in the list's groups.
        let all = agent.mentionables
        XCTAssertEqual(all.filter { $0.group == .tracks }.map(\.name), ["drums", "keys"])
        XCTAssertEqual(all.filter { $0.group == .tracks }.map(\.reference), ["tracks.drums", "tracks.keys"])
        XCTAssertEqual(all.first { $0.name == "drums" }?.detail, "sample track, 2 clips")
        XCTAssertEqual(all.first { $0.name == "keys" }?.detail, "MIDI track with no instrument, 1 clip")
        let clips = all.filter { $0.group == .clips }
        XCTAssertEqual(clips.map(\.name), ["hook.1", "hook.2", "lead1"], "two clips of one pattern are told apart, in time order")
        XCTAssertEqual(clips.map(\.kind), [.clip, .clip, .clip])
        XCTAssertEqual(clips[1].reference, model.placedClips().first { $0.at == 4 }?.clip?.reference)
        XCTAssertEqual(clips[1].detail, "pattern clip of hook on drums at 2.1.1, 1 repeat")
        XCTAssertEqual(clips[2].detail, "note clip lead1 on keys at 3.1.1, 4 beats, 1 note")
        XCTAssertEqual(all.filter { $0.group == .samples }.map(\.name), ["kick", "snare"])
        XCTAssertEqual(all.first { $0.name == "kick" }?.detail, "sample at samples/kick.wav, on drums")
        XCTAssertEqual(all.first { $0.name == "snare" }?.detail, "sample at samples/kick.wav, root C2, on no track")
        XCTAssertEqual(all.first { $0.name == "verse" }?.detail, "section from 1.1.1 to 3.1.1, 8 beats")
        XCTAssertEqual(all.first { $0.name == "verse" }?.reference, "sections.verse")
        XCTAssertEqual(all.first { $0.name == "m1" }?.detail, "marker at 2.1.1: too busy")
        XCTAssertEqual(all.first { $0.name == "m1" }?.reference, "markers.m1")
        XCTAssertEqual(all.filter { $0.group == .references }.map(\.name), ["Night Drive"])
        let names = all.filter { $0.group == .files }.map(\.name)
        XCTAssertEqual(names.filter { $0 != "AGENTS.md" }, ["samples", "samples/clip.mov", "samples/kick.wav", "samples/shot.png", "samples/two.mid", "SONG.md", "song.yaml"], "the host's own files left out")
        XCTAssertEqual(all.first { $0.name == "samples/kick.wav" }?.path, files.wav.path)
        XCTAssertTrue(agent.mentionNames.contains("hook.2"))

        // `@` lists all; typing filters; the caret says which `@`.
        XCTAssertNil(agent.mentionQuery)
        agent.draft = "make "
        agent.caret = 5
        XCTAssertNil(agent.mentionQuery)
        agent.draft = "make @"
        agent.caret = 6
        XCTAssertEqual(agent.mentionQuery, "")
        XCTAssertEqual(agent.listedMentions.map(\.group), [.tracks, .clips, .samples, .sections, .markers, .references, .files])
        agent.draft = "make @ho"
        agent.caret = 8
        XCTAssertEqual(agent.mentionQuery, "ho")
        XCTAssertEqual(agent.listedMentions.flatMap(\.mentions).map(\.name), ["hook.1", "hook.2", "samples/shot.png"], "a name that starts with it, then one that has it")
        agent.draft = "make @hoo"
        agent.caret = 9
        XCTAssertEqual(agent.listedMentions.flatMap(\.mentions).map(\.name), ["hook.1", "hook.2"])
        XCTAssertTrue(agent.moveSelection(by: 1))
        XCTAssertEqual(agent.mentionSelection, 1)
        XCTAssertTrue(agent.moveSelection(by: 1))
        XCTAssertEqual(agent.mentionSelection, 0, "the selection wraps")
        XCTAssertTrue(agent.moveSelection(by: 1))
        XCTAssertTrue(agent.chooseSelected())
        XCTAssertEqual(agent.draft, "make @hook.2 ")
        XCTAssertEqual(agent.caretRequest?.caret, 13, "the caret goes after the name and its space")
        XCTAssertEqual(agent.caret, 13)
        XCTAssertNil(agent.mentionQuery)
        XCTAssertNil(agent.chip, "no skill chip")

        // Escape closes the list and leaves the text; a caret moved alone
        // does not open it; the next key does.
        agent.draft = "make @hook.2 quieter @ve"
        agent.caret = 24
        XCTAssertEqual(agent.mentionQuery, "ve")
        XCTAssertTrue(agent.closeList())
        XCTAssertNil(agent.mentionQuery)
        XCTAssertFalse(agent.closeList())
        XCTAssertFalse(agent.moveSelection(by: 1))
        XCTAssertFalse(agent.chooseSelected())
        agent.caret = 23
        XCTAssertNil(agent.mentionQuery)
        agent.draft = "make @hook.2 quieter @ver"
        agent.caret = 25
        XCTAssertEqual(agent.mentionQuery, "ver")
        XCTAssertEqual(agent.listedMentions.flatMap(\.mentions).map(\.name), ["verse"])
        XCTAssertTrue(agent.chooseSelected())
        XCTAssertEqual(agent.draft, "make @hook.2 quieter @verse ")
        agent.caret = 5
        XCTAssertNil(agent.mentionQuery, "the caret before a chip is in no mention")

        // A chip at the start, in the middle of a word nothing, and the
        // skills' list first.
        agent.draft = "@"
        agent.caret = 1
        XCTAssertEqual(agent.mentionQuery, "")
        agent.draft = "a@b"
        agent.caret = 3
        XCTAssertNil(agent.mentionQuery)
        agent.draft = "/"
        agent.caret = 1
        XCTAssertEqual(agent.commandQuery, "")
        XCTAssertNil(agent.mentionQuery)
        XCTAssertTrue(agent.closeList())

        // What is sent: the words as typed, and a line for each thing
        // named, with the reference; a file by its path and kind.
        agent.draft = "make @hook.2 quieter in @verse, as @kick and @samples/kick.wav sound, see @m1 and @SONG.md and @Night Drive short"
        agent.caret = (agent.draft as NSString).length
        agent.send()
        XCTAssertEqual(agent.draft, "")
        XCTAssertEqual(agent.attachments, [])
        wait("the turn") { !agent.working }
        // With a file among them the content is blocks; the text is one.
        func texts() throws -> [String] {
            try logged().compactMap { ($0["message"] as? [String: Any])?["content"] as? [[String: Any]] }
                .compactMap { blocks in blocks.first { $0["type"] as? String == "text" }?["text"] as? String }
        }
        let message = try XCTUnwrap(try texts().first)
        XCTAssertEqual(AppContext.strip(message), "make @hook.2 quieter in @verse, as @kick and @samples/kick.wav sound, see @m1 and @SONG.md and @Night Drive short")
        let listed = AgentAttachments.listed(in: message)
        XCTAssertEqual(listed.map(\.kind), [.clip, .section, .sample, .sound, .marker, .document, .reference])
        XCTAssertEqual(listed.map(\.name), ["hook", "verse", "kick", "kick.wav", "m1", "SONG.md", "Night Drive"])
        XCTAssertEqual(listed.map(\.reference), [clips[1].reference, "sections.verse", "samples.kick", nil, "markers.m1", nil, "Night Drive"])
        XCTAssertEqual(listed[3].path, files.wav.path)
        XCTAssertEqual(listed[3].detail, "0.50 s, mono, 44.1 kHz", "a file named goes as a dropped one does")
        XCTAssertEqual(listed[5].path, project.appendingPathComponent("SONG.md").path)
        XCTAssertEqual(listed[0].detail, "pattern clip of hook on drums at 2.1.1, 1 repeat")
        guard case .person(_, let text, let attached)? = agent.entries.first else { return XCTFail("no person row") }
        XCTAssertTrue(text.hasPrefix("make @hook.2"))
        XCTAssertEqual(attached.map(\.name), ["hook.2", "verse", "kick", "samples/kick.wav", "m1", "SONG.md", "Night Drive"], "the chips keep the names as mentioned")
        XCTAssertTrue(agent.threads.first?.title.hasPrefix("make @hook.2 quieter") == true, agent.threads.first?.title ?? "")

        // A thing attached with `+` and named too goes once; a name that
        // is nothing in the song is a word.
        model.select(clips: [try XCTUnwrap(model.placedClips().first { $0.at == 4 }?.key)])
        agent.attachSelectedClips()
        XCTAssertEqual(agent.attachments.count, 1)
        agent.draft = "@hook.2 and @nothing short"
        agent.caret = 26
        agent.send()
        wait("the second turn") { !agent.working }
        let second = try XCTUnwrap(try texts().last)
        XCTAssertEqual(AgentAttachments.listed(in: second).map(\.reference), [clips[1].reference])
    }

    /// Speaking into the field: the words go on after what was typed as
    /// they come, the tail uncertain until the next word; Return while
    /// listening stops and sends nothing, then sends; Escape takes the
    /// spoken words out; typing stops it; a pause stops it; and what
    /// cannot listen refuses with its reason.
    func testSpeakingFillsTheField() throws {
        let song = try workspaceAndProject()
        let model = try SongModel(url: song)
        defer { model.close() }
        let agent = model.agent
        let hand = HandTranscriber()
        agent.transcriber = hand
        XCTAssertNil(agent.speechTrouble)

        // After typed text, with a space between; the last word uncertain.
        agent.draft = "please"
        agent.toggleListening()
        XCTAssertTrue(agent.listening)
        XCTAssertEqual(hand.starts, 1)
        XCTAssertEqual(agent.draft, "please")
        hand.say("move")
        XCTAssertEqual(agent.draft, "please move")
        XCTAssertEqual(agent.inputLevel, 0.7)
        XCTAssertEqual(agent.uncertainRange, NSRange(location: "please ".utf16.count, length: 4))
        hand.say("move the")
        XCTAssertEqual(agent.uncertainRange, NSRange(location: "please move ".utf16.count, length: 3))
        hand.say("move the hats")
        XCTAssertEqual(agent.draft, "please move the hats")
        XCTAssertEqual(agent.uncertainRange, NSRange(location: "please move the ".utf16.count, length: 4))
        hand.say("move the hats off the beat.", final: true)
        XCTAssertFalse(agent.listening)
        XCTAssertEqual(agent.draft, "please move the hats off the beat.")
        XCTAssertNil(agent.uncertainRange)
        XCTAssertEqual(agent.inputLevel, 0)

        // Return while listening stops it and sends nothing; the next sends.
        agent.draft = ""
        agent.toggleListening()
        hand.say("move the")
        agent.send()
        XCTAssertEqual(hand.stops, 1)
        XCTAssertFalse(agent.listening)
        XCTAssertFalse(agent.working)
        XCTAssertEqual(agent.entries, [])
        XCTAssertEqual(agent.draft, "move the")
        agent.draft += " short"
        agent.send()
        XCTAssertTrue(agent.working)
        XCTAssertEqual(agent.entries.first?.description, "you: move the short")
        wait("the turn") { !agent.working }

        // Escape takes the spoken words out and leaves what was typed, and
        // what the recognizer still says is dropped.
        agent.draft = "quieter"
        agent.toggleListening()
        hand.say("move")
        XCTAssertTrue(agent.cancelListening())
        XCTAssertFalse(agent.listening)
        XCTAssertEqual(hand.stops, 2)
        XCTAssertEqual(agent.draft, "quieter")
        XCTAssertNil(agent.uncertainRange)
        XCTAssertFalse(agent.cancelListening(), "nothing to cancel")
        agent.draft = ""

        // Typing while listening stops it, keeping what was typed.
        agent.toggleListening()
        hand.say("move")
        agent.draft = "move it"
        XCTAssertFalse(agent.listening)
        XCTAssertEqual(hand.stops, 3)
        XCTAssertEqual(agent.draft, "move it")
        agent.draft = ""

        // A pause of a few seconds stops it.
        agent.pauseAfterWords = 0.1
        agent.toggleListening()
        hand.say("move")
        RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        XCTAssertTrue(agent.listening, "not yet")
        wait("the pause") { !agent.listening }
        XCTAssertEqual(hand.stops, 4)
        XCTAssertEqual(agent.draft, "move")
        agent.draft = ""

        // The microphone's button stops it too; the next click listens again.
        agent.pauseAfterWords = 3
        agent.toggleListening()
        hand.say("move")
        agent.toggleListening()
        XCTAssertFalse(agent.listening)
        XCTAssertEqual(agent.draft, "move")
        agent.toggleListening()
        XCTAssertTrue(agent.listening)
        XCTAssertEqual(hand.starts, 7)
        hand.say("on")
        XCTAssertEqual(agent.draft, "move on")
        agent.stopListening()
        agent.draft = ""

        // What cannot listen refuses, and says why.
        agent.transcriber = FileTranscriber(file: scratch.appendingPathComponent("none.txt"))
        XCTAssertEqual(agent.speechTrouble, "This Mac has no speech recognizer")
        agent.toggleListening()
        XCTAssertFalse(agent.listening)
        XCTAssertEqual(model.refusal, "This Mac has no speech recognizer")

        // The stand-in a scripted run names fills the field by itself.
        let file = scratch.appendingPathComponent("said.txt")
        try "move the hats off the beat".write(to: file, atomically: true, encoding: .utf8)
        let standIn = FileTranscriber(file: file)
        standIn.interval = 0.01
        agent.transcriber = standIn
        agent.toggleListening()
        wait("the words") { !agent.listening }
        XCTAssertEqual(agent.draft, "move the hats off the beat")
    }

    func testToolsFoldOnlyInRuns() {
        typealias E = AgentPanel.Entry
        let tool = { (id: Int) in E.tool(id: id, use: "", name: "Bash", summary: "ls", result: nil, failed: false) }
        XCTAssertEqual(AgentPanel.fold([.person(id: 1, text: "a"), tool(2), .agent(id: 3, text: "b"), tool(4), tool(5), .note(id: 6, text: "c"), tool(7), tool(8)]), [
            .entry(.person(id: 1, text: "a")), .entry(tool(2)), .entry(.agent(id: 3, text: "b")), .tools([tool(4), tool(5)]),
            .entry(.note(id: 6, text: "c")), .tools([tool(7), tool(8)]),
        ])
        XCTAssertEqual(AgentPanel.fold([]), [])
    }

    /// A thread renamed and deleted from the panel: the list is saved with
    /// the name, the deleted thread stays out when Claude Code's own list
    /// is merged again, and the one shown becomes a new conversation.
    func testAThreadIsRenamedAndDeleted() throws {
        let song = try workspaceAndProject()
        let model = try SongModel(url: song)
        defer { model.close() }
        let agent = model.agent
        agent.send("first short")
        wait("the first turn") { !agent.working }
        let first = try XCTUnwrap(agent.currentID)
        agent.newThread()
        agent.send("second short")
        wait("the second turn") { !agent.working }
        let second = try XCTUnwrap(agent.currentID)

        agent.rename(first, "  The hats \nmore")
        XCTAssertEqual(agent.threads.map(\.title), ["second short", "The hats"])
        agent.rename(first, " ")
        XCTAssertEqual(agent.threads.map(\.title), ["second short", "The hats"], "an empty name is not taken")

        agent.delete(second)
        XCTAssertNil(agent.currentID)
        XCTAssertEqual(agent.entries, [])
        XCTAssertEqual(agent.threads.map(\.id), [first])
        let folder = song.deletingLastPathComponent()
        var list = ThreadList(project: folder)
        XCTAssertEqual(list.removed, [second])
        list.merge([ClaudeTranscripts.Session(id: second, title: "second short", modified: 1)])
        XCTAssertEqual(list.entries.map(\.id), [first])
        agent.refreshThreads()
        XCTAssertEqual(agent.threads.map(\.id), [first])

        // The next message starts a new thread; deleting the one shown
        // while the agent works is refused.
        agent.send("third short")
        XCTAssertNotEqual(agent.currentID, second)
        agent.delete(try XCTUnwrap(agent.currentID))
        XCTAssertNotNil(agent.currentID, "refused while working")
        wait("the third turn") { !agent.working }
        XCTAssertEqual(agent.threads.count, 2)
    }

    func testSignedOutIsSaid() throws {
        let song = try workspaceAndProject()
        let model = try SongModel(url: song)
        defer { model.close() }
        let agent = model.agent
        agent.send("signout")
        wait("the refusal") { !agent.working }
        XCTAssertEqual(agent.status, .signedOut("Not logged in · Please run /login"))
        guard case .note(_, let text)? = agent.entries.last else { return XCTFail("no note") }
        XCTAssertTrue(text.contains("signed out"), text)
    }

    func testStopInterruptsTheTurn() throws {
        let song = try workspaceAndProject()
        let model = try SongModel(url: song)
        defer { model.close() }
        let agent = model.agent
        agent.send("hello")
        wait("the request") { agent.entries.contains { if case .approval(_, _, _, nil) = $0 { true } else { false } } }
        agent.stop()
        wait("the stop") { !agent.working }
        XCTAssertEqual(agent.entries.last?.description, "note: Stopped.")
        // The request still waiting was refused with the stop.
        XCTAssertTrue(agent.entries.contains { $0.description == "refused touch x" })
    }

    func testClaudeCodeMissingIsSaid() throws {
        setenv(ClaudeCode.override, scratch.appendingPathComponent("no such program").path, 1)
        let song = try workspaceAndProject()
        let model = try SongModel(url: song)
        defer { model.close() }
        let agent = model.agent
        agent.send("hello")
        wait("the failure") { !agent.working }
        guard case .failed = agent.status else { return XCTFail("\(agent.status)") }
        guard case .note(_, let text)? = agent.entries.last else { return XCTFail("no note") }
        XCTAssertTrue(text.contains("could not be started"), text)
    }

    // MARK: What a conversation is started with

    func testTheArguments() {
        XCTAssertEqual(ClaudeCode.arguments(resume: nil, sessionID: "id1", workspace: URL(fileURLWithPath: "/w"), prompt: "P"), [
            "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose",
            "--include-partial-messages", "--permission-mode", "auto", "--permission-prompt-tool", "stdio",
            "--session-id", "id1", "--add-dir", "/w", "--append-system-prompt", "P",
        ])
        XCTAssertEqual(ClaudeCode.arguments(resume: "id1", sessionID: "id1", workspace: nil, prompt: "P").suffix(4),
                       ["--resume", "id1", "--append-system-prompt", "P"])
    }

    func testThePrompt() {
        let project = URL(fileURLWithPath: "/p/Song")
        let plain = ClaudeCode.prompt(project: project, dawOnPath: true, workspace: nil, workspaceText: nil)
        XCTAssertTrue(plain.contains("/p/Song"))
        XCTAssertTrue(plain.contains("`daw` is on your PATH"))
        XCTAssertFalse(plain.contains("outside the workspace"))
        let outside = ClaudeCode.prompt(project: project, dawOnPath: false, workspace: URL(fileURLWithPath: "/w"), workspaceText: "## /w/AGENTS.md\n\nUse daw.")
        XCTAssertTrue(outside.contains("Use `uv run daw`"))
        XCTAssertTrue(outside.contains("is /w."))
        XCTAssertTrue(outside.hasSuffix("## /w/AGENTS.md\n\nUse daw."))
    }

    /// The workspace is the checkout: the folder above the project with an
    /// `AGENTS.md` and a `.claude` folder when the bundle names none; its
    /// instruction files are read as one text for a project outside it.
    func testTheWorkspaceIsFoundAndRead() throws {
        let song = try workspaceAndProject()
        let project = song.deletingLastPathComponent()
        XCTAssertEqual(ClaudeCode.workspace(project: project, bundle: Bundle(for: Self.self))?.path, scratch.path)
        XCTAssertNil(ClaudeCode.workspace(project: URL(fileURLWithPath: "/private/tmp"), bundle: Bundle(for: Self.self)))
        XCTAssertTrue(ClaudeCode.isUnder(project, workspace: scratch))
        XCTAssertFalse(ClaudeCode.isUnder(URL(fileURLWithPath: "/elsewhere/Song"), workspace: scratch))
        let text = ClaudeCode.workspaceText(scratch)
        XCTAssertTrue(text.hasPrefix("## \(scratch.path)/AGENTS.md\n\nUse daw."))
        XCTAssertTrue(text.contains("## \(scratch.path)/.claude/rules/instructions.md\n\nKeep vocals dry."))
        XCTAssertFalse(text.contains("taste.md"), "a file that is not there is left out")
    }

    func testLocateTakesTheOverride() {
        XCTAssertEqual(ClaudeCode.locate(environment: [ClaudeCode.override: "/x/claude", "PATH": "/a:/b"]),
                       ClaudeCode.Located(executable: URL(fileURLWithPath: "/x/claude"), path: "/a:/b"))
        XCTAssertTrue(AgentPanel.isSignedOut("Not logged in · Please run /login"))
        XCTAssertTrue(AgentPanel.isSignedOut("Invalid API key"))
        XCTAssertFalse(AgentPanel.isSignedOut("Rate limited"))
    }
}

extension AgentPanel.Entry {
    /// The row in a line, for the tests to read.
    var description: String {
        switch self {
        case .person(_, let text, let attachments): "you: \(text)" + (attachments.isEmpty ? "" : " + " + attachments.map(\.name).joined(separator: ", "))
        case .agent(_, let text): "agent: \(text)"
        case .tool(_, _, let name, let summary, let result, let failed):
            "tool \(name) \(summary)" + (result.map { failed ? " ✗ \($0)" : " → \($0)" } ?? "")
        case .approval(_, _, let summary, let allowed):
            "\(allowed == nil ? "ask" : allowed == true ? "allowed" : "refused") \(summary)"
        case .note(_, let text): "note: \(text)"
        }
    }
}
