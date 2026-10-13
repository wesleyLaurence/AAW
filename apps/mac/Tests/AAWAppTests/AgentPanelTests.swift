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

    /// A program that answers as Claude Code does on its pipes: a handshake,
    /// then for each message a streamed greeting, a `daw` command that the
    /// panel allows by itself, a `touch` that it asks about, and a result;
    /// "short" is answered with words alone, and "signout" as a session
    /// with no login.
    private nonisolated static let fake = """
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
    for line in sys.stdin:
        m = json.loads(line)
        log.write(line)
        log.flush()
        t = m.get("type")
        if t == "control_request":
            sub = m["request"]["subtype"]
            out({"type": "control_response", "response": {"subtype": "success", "request_id": m["request_id"], "response": {}}})
            if sub == "interrupt":
                stopped = True
                out({"type": "result", "subtype": "error_during_execution", "is_error": True, "result": None, "session_id": "s"})
        elif t == "user":
            stopped = False
            text = m["message"]["content"]
            out({"type": "system", "subtype": "init", "session_id": "s"})
            if text.endswith("signout"):
                out({"type": "result", "subtype": "error", "is_error": True, "result": "Not logged in · Please run /login", "session_id": "s"})
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
            if rid == "r1":
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
    }

    override func tearDown() async throws {
        unsetenv(ClaudeCode.override)
        unsetenv("AAW_FAKE_LOG")
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
            "--include-partial-messages", "--permission-prompt-tool", "stdio",
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
        case .person(_, let text): "you: \(text)"
        case .agent(_, let text): "agent: \(text)"
        case .tool(_, _, let name, let summary, let result, let failed):
            "tool \(name) \(summary)" + (result.map { failed ? " ✗ \($0)" : " → \($0)" } ?? "")
        case .approval(_, _, let summary, let allowed):
            "\(allowed == nil ? "ask" : allowed == true ? "allowed" : "refused") \(summary)"
        case .note(_, let text): "note: \(text)"
        }
    }
}
