import AppKit

/// The person's own Claude Code, found as their shell finds it, and what a
/// conversation is started with: the arguments and the prompt the panel adds.
enum ClaudeCode {
    /// The program, and the PATH the person's shell has, which the agent's
    /// own commands need to find `uv`, `node` and the rest.
    struct Located: Equatable {
        var executable: URL
        var path: String
    }

    /// `AAW_AGENT` names a program to run in Claude Code's place, for a
    /// scripted run or a test.
    static let override = "AAW_AGENT"

    /// Where the person's login shell finds `claude`, asked once; nil when
    /// it finds none. An app opened from the Finder has a bare PATH, so the
    /// shell is asked for its own.
    static func locate(environment: [String: String] = ProcessInfo.processInfo.environment) -> Located? {
        if let named = environment[override], !named.isEmpty {
            return Located(executable: URL(fileURLWithPath: named), path: environment["PATH"] ?? "/usr/bin:/bin")
        }
        let shell = environment["SHELL"].flatMap { $0.isEmpty ? nil : $0 } ?? "/bin/zsh"
        // An interactive login shell: a version manager such as nvm, which
        // installs Claude Code, is usually set up in the interactive file.
        for flags in ["-lic", "-lc"] {
            guard let answer = run(shell, [flags, "printf '%s\\n' \"$(command -v claude)\" \"$PATH\""]) else { continue }
            let lines = answer.split(separator: "\n").map(String.init)
            if lines.count >= 2, let program = lines.first(where: { $0.hasPrefix("/") && $0.hasSuffix("claude") }),
               let path = lines.last(where: { $0.contains(":") }) {
                return Located(executable: URL(fileURLWithPath: program), path: path)
            }
        }
        // Without a shell's word: the places it is usually installed.
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        var usual = ["\(home)/.claude/local/bin/claude", "/opt/homebrew/bin/claude", "/usr/local/bin/claude"]
        let nvm = "\(home)/.nvm/versions/node"
        for version in (try? FileManager.default.contentsOfDirectory(atPath: nvm))?.sorted().reversed() ?? [] {
            usual.append("\(nvm)/\(version)/bin/claude")
        }
        for candidate in usual where FileManager.default.isExecutableFile(atPath: candidate) {
            let path = (environment["PATH"] ?? "") + ":" + (candidate as NSString).deletingLastPathComponent
            return Located(executable: URL(fileURLWithPath: candidate), path: path)
        }
        return nil
    }

    /// What a program printed, or nil when it could not run, failed, or
    /// took longer than `limit` seconds. It is given no input: an
    /// interactive shell with the app's input would wait on it.
    private static func run(_ program: String, _ arguments: [String], limit: Double = 15) -> String? {
        let process = Process()
        let out = Pipe()
        process.executableURL = URL(fileURLWithPath: program)
        process.arguments = arguments
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = out
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return nil }
        let done = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in done.signal() }
        // Read as it comes: a shell that prints more than a pipe holds
        // would otherwise wait for a reader.
        var data = Data()
        let reading = DispatchQueue(label: "aaw.agent.locate")
        let read = DispatchGroup()
        read.enter()
        reading.async {
            data = out.fileHandleForReading.readDataToEndOfFile()
            read.leave()
        }
        if done.wait(timeout: .now() + limit) == .timedOut {
            process.terminate()
            return nil
        }
        read.wait()
        guard process.terminationStatus == 0 else { return nil }
        return String(decoding: data, as: UTF8.self)
    }

    /// The arguments of a conversation's process: one message in and each
    /// event out as a line of JSON, the text as it streams, approvals asked
    /// over the same pipe in Claude Code's auto mode, which decides most
    /// actions by itself as in the person's terminal sessions, resumed by
    /// ID or started under one the panel chose, the workspace attached, and
    /// the panel's own instructions added to Claude Code's prompt.
    static func arguments(resume: String?, sessionID: String, workspace: URL?, prompt: String) -> [String] {
        var arguments = [
            "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose",
            "--include-partial-messages", "--permission-mode", "auto", "--permission-prompt-tool", "stdio",
        ]
        if let resume { arguments += ["--resume", resume] } else { arguments += ["--session-id", sessionID] }
        if let workspace { arguments += ["--add-dir", workspace.path] }
        arguments += ["--append-system-prompt", prompt]
        return arguments
    }

    /// What the panel tells the agent beyond what a terminal session has:
    /// where it is, that `daw` is the app's own, what is approved by itself,
    /// what each message carries, and that the person reads a panel, not a
    /// terminal. A project outside the workspace is given the workspace's
    /// instructions here, which a session started in it has by location.
    static func prompt(project: URL, dawOnPath: Bool, workspace: URL?, workspaceText: String?) -> String {
        var text = """
        You are the agent in the AAW app's panel, working on the project at \(project.path), which is open in the window beside the panel. Its folder is your working directory, so PROJECT is `.` in every `daw` command. \(dawOnPath ? "`daw` is on your PATH and is the app's own copy; `uv run daw` is the same engine in this checkout." : "Use `uv run daw`, which runs from a project's folder.") You run in Claude Code's auto mode, so most actions run without asking, and your `daw` commands on this project always do; what auto mode would still ask about is shown to the person to allow or refuse, so prefer `daw` and ask in words before reaching outside the project. Each message from the person begins with a block from the app, between <app-context> tags: what is selected, the start position and loop, whether the song plays, and what the person changed in the window since your last turn; read their edits as direction and do not undo them. Work a part at a time, so the person watches the song grow; your words and each command you run are shown in the panel, so keep replies short and say what you changed.
        """
        if let workspace {
            text += "\n\nThe workspace, with the person's instructions, taste and skills for all their music, is \(workspace.path)."
        }
        if let workspaceText, !workspaceText.isEmpty {
            text += "\n\nThis project is outside the workspace, so the workspace's instructions follow here.\n\n" + workspaceText
        }
        return text
    }

    /// The workspace's instruction files as one text, for a project that is
    /// not under the workspace: the root `AGENTS.md` and the person's two
    /// files, each under its path.
    static func workspaceText(_ workspace: URL) -> String {
        var parts: [String] = []
        for name in ["AGENTS.md", ".claude/rules/instructions.md", ".claude/rules/taste.md"] {
            let file = workspace.appendingPathComponent(name)
            if let text = try? String(contentsOf: file, encoding: .utf8), !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                parts.append("## \(file.path)\n\n\(text)")
            }
        }
        return parts.joined(separator: "\n\n")
    }

    /// The checkout that stands in for the workspace: where the root
    /// `AGENTS.md`, the person's instructions and the skills for every
    /// project are. `AAWCheckout` in the bundle's Info.plist, which
    /// build.sh writes; else the folder above the project that has an
    /// `AGENTS.md` and a `.claude` folder, for a build that is no bundle.
    static func workspace(project: URL, bundle: Bundle = .main) -> URL? {
        if let path = bundle.object(forInfoDictionaryKey: "AAWCheckout") as? String, !path.isEmpty,
           FileManager.default.fileExists(atPath: path) {
            return URL(fileURLWithPath: path).standardizedFileURL
        }
        var folder = project.standardizedFileURL
        while folder.path != "/" {
            folder = folder.deletingLastPathComponent()
            let has = { (name: String) in FileManager.default.fileExists(atPath: folder.appendingPathComponent(name).path) }
            if has("AGENTS.md"), has(".claude") { return folder }
        }
        return nil
    }

    /// Whether a project's folder is under the workspace.
    static func isUnder(_ project: URL, workspace: URL) -> Bool {
        let (p, w) = (project.standardizedFileURL.path, workspace.standardizedFileURL.path)
        return p == w || p.hasPrefix(w + "/")
    }
}

/// One running conversation: Claude Code as a process in the project's
/// folder, given messages and approvals on its input and read as lines of
/// JSON from its output, each handed to the panel on the main thread.
final class AgentProcess: @unchecked Sendable {
    private let process = Process()
    private let input: Pipe
    private let output = Pipe()
    private let errors = Pipe()
    private var buffer = Data()
    private var said = Data()
    private let lock = NSLock()
    private var ended = false

    /// Called with each line of JSON the process prints, on the main thread.
    private let onEvent: @MainActor ([String: Any]) -> Void
    /// Called when the process ends, with its status and what it wrote on
    /// stderr, on the main thread.
    private let onExit: @MainActor (Int32, String) -> Void

    var isRunning: Bool { process.isRunning }

    init(executable: URL, arguments: [String], folder: URL, environment: [String: String],
         onEvent: @escaping @MainActor ([String: Any]) -> Void, onExit: @escaping @MainActor (Int32, String) -> Void) throws {
        input = Pipe()
        self.onEvent = onEvent
        self.onExit = onExit
        process.executableURL = executable
        process.arguments = arguments
        process.currentDirectoryURL = folder
        process.environment = environment
        process.standardInput = input
        process.standardOutput = output
        process.standardError = errors
        process.terminationHandler = { [weak self] process in
            guard let self else { return }
            let said = String(decoding: self.lock.withLock { self.said }, as: UTF8.self)
            let status = process.terminationStatus
            DispatchQueue.main.async {
                MainActor.assumeIsolated { self.onExit(status, said) }
            }
        }
        try process.run()
        output.fileHandleForReading.readabilityHandler = { [weak self] handle in
            self?.took(handle.availableData)
        }
        errors.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            // At the end of the pipe the handler is called again and again
            // with nothing until it is taken off.
            guard let self, !data.isEmpty else {
                handle.readabilityHandler = nil
                return
            }
            self.lock.withLock {
                self.said.append(data)
                if self.said.count > 64 * 1024 { self.said = self.said.suffix(32 * 1024) }
            }
        }
    }

    /// Output as it arrives, cut into lines; a part of a line waits for the rest.
    private func took(_ data: Data) {
        guard !data.isEmpty else {
            output.fileHandleForReading.readabilityHandler = nil
            return
        }
        var lines: [Data] = []
        lock.withLock {
            buffer.append(data)
            while let newline = buffer.firstIndex(of: UInt8(ascii: "\n")) {
                lines.append(buffer.subdata(in: buffer.startIndex..<newline))
                buffer.removeSubrange(buffer.startIndex...newline)
            }
        }
        for line in lines where !line.isEmpty {
            // Parsed on the main thread: the parsed object is not Sendable,
            // and a line of JSON is small.
            DispatchQueue.main.async { [onEvent] in
                MainActor.assumeIsolated {
                    guard let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any] else { return }
                    onEvent(object)
                }
            }
        }
    }

    /// Writes one object as a line of JSON to the process.
    func send(_ object: [String: Any]) {
        guard var data = try? JSONSerialization.data(withJSONObject: object) else { return }
        data.append(UInt8(ascii: "\n"))
        lock.withLock {
            guard !ended else { return }
            do { try input.fileHandleForWriting.write(contentsOf: data) } catch { ended = true }
        }
    }

    /// Closes the input, after which the process ends by itself once its
    /// turn is over; one still running a while later is stopped.
    func end(grace: Double = 3) {
        lock.withLock {
            guard !ended else { return }
            ended = true
            try? input.fileHandleForWriting.close()
        }
        let process = process
        DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + grace) {
            if process.isRunning { process.terminate() }
        }
    }

    /// Stops the process now.
    func kill() {
        lock.withLock { ended = true }
        try? input.fileHandleForWriting.close()
        if process.isRunning { process.terminate() }
    }
}
