import Foundation

/// A project's conversations with the agent: their IDs and titles, kept in
/// `.daw/threads.json` beside the song, so the list goes with the folder when
/// it moves or is saved under another name. Claude Code keeps the
/// conversations themselves, by the folder they were started in; a thread
/// the person started in a terminal in the project's folder is listed too
/// when that list can be read.
struct ThreadList: Equatable {
    struct Entry: Codable, Equatable, Identifiable {
        /// Claude Code's session ID, which `--resume` takes.
        var id: String
        /// The first line the person typed, until renamed.
        var title: String
        /// Unix time in seconds.
        var started: Double
        var last: Double

        init(id: String, title: String, started: Double, last: Double? = nil) {
            (self.id, self.title, self.started, self.last) = (id, title, started, last ?? started)
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            id = try c.decode(String.self, forKey: .id)
            title = try c.decodeIfPresent(String.self, forKey: .title) ?? ""
            started = try c.decodeIfPresent(Double.self, forKey: .started) ?? 0
            last = try c.decodeIfPresent(Double.self, forKey: .last) ?? started
        }
    }

    private struct File: Codable {
        var threads: [Entry]
        /// The IDs of threads the person deleted, so that Claude Code's own
        /// record of them, which stays, does not list them again.
        var removed: [String]?
    }

    /// `.daw/threads.json` in the project's folder.
    let file: URL
    /// Most recent first.
    private(set) var entries: [Entry] = []
    /// Deleted in the panel; left out when Claude Code's list is merged.
    private(set) var removed: [String] = []

    static func file(project folder: URL) -> URL {
        folder.appendingPathComponent(".daw", isDirectory: true).appendingPathComponent("threads.json")
    }

    /// Reads the list. A file that is missing or does not read is an empty one.
    init(file: URL) {
        self.file = file
        if let data = try? Data(contentsOf: file), let read = try? JSONDecoder().decode(File.self, from: data) {
            entries = read.threads
            removed = read.removed ?? []
            sort()
        }
    }

    init(project folder: URL) {
        self.init(file: Self.file(project: folder))
    }

    private mutating func sort() {
        entries.sort { $0.last > $1.last }
    }

    subscript(id: String) -> Entry? {
        entries.first { $0.id == id }
    }

    /// Adds a thread, or brings one the list has to the top.
    mutating func note(_ id: String, title: String, at time: Date = Date()) {
        let now = time.timeIntervalSince1970
        if let i = entries.firstIndex(where: { $0.id == id }) {
            entries[i].last = now
            if entries[i].title.isEmpty { entries[i].title = title }
        } else {
            entries.append(Entry(id: id, title: title, started: now))
            removed.removeAll { $0 == id }
        }
        sort()
    }

    mutating func rename(_ id: String, _ title: String) {
        if let i = entries.firstIndex(where: { $0.id == id }) { entries[i].title = title }
    }

    /// Takes a thread out of the list, and keeps it out when Claude Code's
    /// own record of it is merged again.
    mutating func remove(_ id: String) {
        entries.removeAll { $0.id == id }
        if !removed.contains(id) { removed.append(id) }
    }

    /// Adds the threads of Claude Code's own list for the folder that this
    /// one lacks, such as one started in a terminal, titled by their first
    /// message. Nothing the list has is changed, and a thread the person
    /// deleted stays out.
    mutating func merge(_ found: [ClaudeTranscripts.Session]) {
        for session in found where self[session.id] == nil && !removed.contains(session.id) {
            entries.append(Entry(id: session.id, title: session.title, started: session.modified, last: session.modified))
        }
        sort()
    }

    func save() throws {
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(File(threads: entries, removed: removed.isEmpty ? nil : removed)).write(to: file, options: .atomic)
    }

    /// A thread's title from the person's first message: its first line, cut
    /// to a length a menu shows.
    static func title(from text: String, limit: Int = 60) -> String {
        let line = text.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }.first { !$0.isEmpty } ?? ""
        if line.count <= limit { return line }
        return String(line.prefix(limit - 1)).trimmingCharacters(in: .whitespaces) + "…"
    }
}

/// What the person's message carries from the app, ahead of their words: the
/// selection, the transport and what they changed since the agent's last
/// turn. Wrapped so that the panel can show the words alone when a
/// conversation is drawn again from its transcript.
enum AppContext {
    static let open = "<app-context>"
    static let close = "</app-context>"

    /// The message as it is sent: the context, then the person's words. A
    /// message that runs a skill, `/name words`, is read from its start, so
    /// the context follows it there instead.
    static func wrap(context: String, text: String) -> String {
        let body = context.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty else { return text }
        if AgentCommands.split(text) != nil { return "\(text)\n\n\(open)\n\(body)\n\(close)" }
        return "\(open)\n\(body)\n\(close)\n\n\(text)"
    }

    /// The person's words without the context, as the panel shows them.
    static func strip(_ message: String) -> String {
        if message.hasPrefix(open), let end = message.range(of: close) {
            return String(message[end.upperBound...]).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        if message.hasSuffix(close), let start = message.range(of: open, options: .backwards) {
            return String(message[..<start.lowerBound]).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return message
    }
}

/// Claude Code's own record of conversations: one `.jsonl` file a session,
/// under `~/.claude/projects/` in a folder named for the working folder the
/// session started in. Read to list a project's threads and to draw a
/// resumed one's earlier messages; never written.
enum ClaudeTranscripts {
    /// A session as its file shows it.
    struct Session: Equatable {
        var id: String
        var title: String
        var modified: Double
    }

    /// A message as the panel draws it.
    enum Line: Equatable {
        case person(String)
        case agent(String)
        /// A tool the agent used, with what it was given in a line.
        case tool(name: String, summary: String)
    }

    /// Claude Code's folder: `CLAUDE_CONFIG_DIR`, or `~/.claude`.
    static func configFolder(environment: [String: String] = ProcessInfo.processInfo.environment,
                             home: URL = FileManager.default.homeDirectoryForCurrentUser) -> URL {
        if let dir = environment["CLAUDE_CONFIG_DIR"], !dir.isEmpty { return URL(fileURLWithPath: dir) }
        return home.appendingPathComponent(".claude")
    }

    /// How Claude Code names a working folder's own folder of sessions:
    /// every character that is not a letter or a digit becomes `-`.
    static func folderName(for cwd: String) -> String {
        String(cwd.unicodeScalars.map { CharacterSet.alphanumerics.contains($0) ? Character($0) : "-" })
    }

    /// The folder of a working folder's sessions. The working folder is
    /// named as the agent's process sees it, with links resolved, as `/tmp`
    /// is `/private/tmp`.
    static func folder(for cwd: URL, config: URL = configFolder()) -> URL {
        config.appendingPathComponent("projects").appendingPathComponent(folderName(for: realPath(cwd.path)))
    }

    /// A path with its links resolved, as a process's working folder reads;
    /// Foundation's own resolving leaves `/private` off, which the kernel
    /// does not.
    static func realPath(_ path: String) -> String {
        guard let resolved = realpath(path, nil) else { return path }
        defer { free(resolved) }
        return String(cString: resolved)
    }

    /// The sessions recorded for a working folder, newest first; none when
    /// the folder is not there or cannot be read. A session with no message
    /// of the person's is left out.
    static func sessions(in folder: URL) -> [Session] {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: folder.path) else { return [] }
        var found: [Session] = []
        for name in names where name.hasSuffix(".jsonl") {
            let file = folder.appendingPathComponent(name)
            let id = String(name.dropLast(6))
            guard let title = firstMessage(in: file) else { continue }
            let modified = (try? FileManager.default.attributesOfItem(atPath: file.path)[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
            found.append(Session(id: id, title: ThreadList.title(from: title), modified: modified))
        }
        return found.sorted { $0.modified > $1.modified }
    }

    /// The first thing the person typed in a session, for its title.
    private static func firstMessage(in file: URL) -> String? {
        guard let handle = try? FileHandle(forReadingFrom: file) else { return nil }
        defer { try? handle.close() }
        // The first messages are near the top; a whole transcript can be megabytes.
        let head = handle.readData(ofLength: 256 * 1024)
        for line in head.split(separator: UInt8(ascii: "\n")) {
            if case .person(let text)? = parse(line: Data(line)).first { return text }
        }
        return nil
    }

    /// The messages of a session, in order, from its file.
    static func read(_ file: URL) -> [Line] {
        guard let data = try? Data(contentsOf: file) else { return [] }
        return parse(data)
    }

    static func parse(_ data: Data) -> [Line] {
        data.split(separator: UInt8(ascii: "\n")).flatMap { parse(line: Data($0)) }
    }

    /// The messages in one line of the file: a person's message, an agent's
    /// text and the tools it used. Tool results, meta lines and the lines of
    /// subagents are passed over.
    static func parse(line: Data) -> [Line] {
        guard let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
              let type = object["type"] as? String, type == "user" || type == "assistant",
              object["isMeta"] as? Bool != true, object["isSidechain"] as? Bool != true,
              let message = object["message"] as? [String: Any] else { return [] }
        if type == "user" {
            if let text = message["content"] as? String { return personLine(text).map { [$0] } ?? [] }
            guard let blocks = message["content"] as? [[String: Any]] else { return [] }
            // A message of several blocks is the person's only when it has
            // text and no tool result.
            if blocks.contains(where: { $0["type"] as? String == "tool_result" }) { return [] }
            let text = blocks.compactMap { $0["type"] as? String == "text" ? $0["text"] as? String : nil }.joined(separator: "\n")
            return personLine(text).map { [$0] } ?? []
        }
        guard let blocks = message["content"] as? [[String: Any]] else { return [] }
        return blocks.compactMap { block in
            switch block["type"] as? String {
            case "text":
                guard let text = block["text"] as? String, !text.isEmpty else { return nil }
                return .agent(text)
            case "tool_use":
                guard let name = block["name"] as? String else { return nil }
                return .tool(name: name, summary: toolSummary(name: name, input: block["input"] as? [String: Any] ?? [:]))
            default:
                return nil
            }
        }
    }

    /// The person's line: a skill they ran as `/name words`, or their words;
    /// nothing for a local command's record or an empty line.
    private static func personLine(_ text: String) -> Line? {
        if let command = AgentCommands.recorded(text) { return .person(command) }
        let shown = AppContext.strip(text)
        let trimmed = shown.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !trimmed.hasPrefix("<command-name>"), !trimmed.hasPrefix("<local-command"),
              !trimmed.hasPrefix("[Request interrupted") else { return nil }
        return .person(shown)
    }

    /// A tool call in a line: a command as typed, a file's path, or the
    /// first string it was given.
    static func toolSummary(name: String, input: [String: Any]) -> String {
        if name == "Bash", let command = input["command"] as? String { return command }
        for key in ["file_path", "path", "pattern", "query", "url", "prompt", "description", "skill"] {
            if let value = input[key] as? String, !value.isEmpty { return value }
        }
        return input.values.compactMap { $0 as? String }.first ?? ""
    }
}
