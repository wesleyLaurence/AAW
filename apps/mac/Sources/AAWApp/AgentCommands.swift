import Foundation

/// A skill or command the agent runs by name when a message starts with
/// `/name`: the engine's handshake lists them, and the panel shows them
/// under `/` in the field.
struct AgentCommand: Equatable, Identifiable, Hashable {
    var name: String
    var description: String
    /// What the words after the name are, as the engine says it: `[name]`.
    var hint: String

    var id: String { name }

    /// From one entry of the handshake's `commands`.
    init?(_ entry: [String: Any]) {
        guard let name = entry["name"] as? String, !name.isEmpty else { return nil }
        self.name = name
        description = entry["description"] as? String ?? ""
        hint = entry["argumentHint"] as? String ?? ""
    }

    init(name: String, description: String = "", hint: String = "") {
        (self.name, self.description, self.hint) = (name, description, hint)
    }
}

/// The commands as the list shows them: in three groups, filtered by what
/// the person typed after `/`.
enum AgentCommands {
    /// Where a command comes from, in the order the list shows.
    enum Group: Int, CaseIterable, Comparable {
        case project, workspace, other

        var title: String {
            switch self {
            case .project: "This project"
            case .workspace: "For all my music"
            case .other: "Mine and Claude Code's"
            }
        }

        static func < (a: Group, b: Group) -> Bool { a.rawValue < b.rawValue }
    }

    struct Section: Equatable {
        var group: Group
        var commands: [AgentCommand]
    }

    /// Claude Code's own commands that mean nothing in the panel, or would
    /// undo what it keeps: the session's shape, the terminal's look, its
    /// settings and its reports. `compact` and `reload-skills` stay.
    static let hidden: Set<String> = [
        "clear", "model", "effort", "fast", "config", "output-style", "context", "focus", "heapdump", "init",
        "mcp", "import", "color", "autocompact", "rename", "reload-plugins", "usage", "usage-credits",
        "extra-usage", "insights", "recap", "skill-doctor", "team-onboarding", "list-agents", "agents",
        "auto-mode-setup", "doctor", "debug", "advisor", "goal", "ultrareview", "design", "design-consent",
        "design-revoke", "workflow-launch-exec", "loop", "schedule", "batch",
    ]

    /// The names of the skills in a folder: each `SKILL.md` under its
    /// `.claude/skills/` and `.agents/skills/`.
    static func skillNames(in folder: URL) -> Set<String> {
        var names: Set<String> = []
        for skills in [".claude/skills", ".agents/skills"] {
            let dir = folder.appendingPathComponent(skills, isDirectory: true)
            for name in (try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []
            where FileManager.default.fileExists(atPath: dir.appendingPathComponent(name).appendingPathComponent("SKILL.md").path) {
                names.insert(name)
            }
        }
        return names
    }

    /// The commands in their groups, told by the skill folders of the
    /// project and the workspace, with the hidden ones and the engine's
    /// own `__` commands left out, and the "(project)" the engine adds to
    /// a folder's skills taken off the description.
    static func grouped(_ commands: [AgentCommand], project: Set<String>, workspace: Set<String>) -> [Section] {
        var groups: [Group: [AgentCommand]] = [:]
        for var command in commands where !hidden.contains(command.name) && !command.name.hasPrefix("__") {
            let group: Group = project.contains(command.name) ? .project : workspace.contains(command.name) ? .workspace : .other
            if command.description.hasSuffix(" (project)") { command.description = String(command.description.dropLast(10)) }
            groups[group, default: []].append(command)
        }
        return Group.allCases.compactMap { group in groups[group].map { Section(group: group, commands: $0) } }
    }

    /// The sections with only the commands that match what was typed: a
    /// name that starts with it first, then a name that has it, then a
    /// description that has it; an empty query keeps all.
    static func filter(_ sections: [Section], query: String) -> [Section] {
        let q = query.lowercased()
        guard !q.isEmpty else { return sections }
        return sections.compactMap { section in
            func rank(_ c: AgentCommand) -> Int {
                let name = c.name.lowercased()
                if name.hasPrefix(q) { return 0 }
                if name.contains(q) { return 1 }
                if c.description.lowercased().contains(q) { return 2 }
                return 3
            }
            let kept = section.commands.map { ($0, rank($0)) }.filter { $0.1 < 3 }
            guard !kept.isEmpty else { return nil }
            // A stable sort: the ranks, then the list's own order.
            return Section(group: section.group, commands: kept.enumerated().sorted { ($0.element.1, $0.offset) < ($1.element.1, $1.offset) }.map(\.element.0))
        }
    }

    /// The command a message starts with, as `/name`, and the words after
    /// it; nil when the message is no command. A name is letters, digits,
    /// `-`, `_` and `:`.
    static func split(_ text: String) -> (name: String, rest: String)? {
        guard text.hasPrefix("/") else { return nil }
        let body = text.dropFirst()
        let name = body.prefix(while: isNameCharacter)
        guard !name.isEmpty else { return nil }
        let after = body[name.endIndex...]
        guard after.isEmpty || after.first!.isWhitespace else { return nil }
        return (String(name), String(after))
    }

    // MARK: `/` anywhere in a message

    /// Whether a character can be part of a command's name.
    private static func isNameCharacter(_ c: Character) -> Bool {
        c.isLetter || c.isNumber || c == "-" || c == "_" || c == ":"
    }

    /// The `/` the caret is in, as the range of `/` and what was typed
    /// after it: the `/` at the text's start or after a space, with no
    /// space between it and the caret, so that "1/4" and "and/or" are
    /// text. Nil when the caret is in none. In UTF-16 units.
    static func query(in text: String, caret: Int) -> NSRange? {
        AgentMentions.query(in: text, caret: caret, trigger: 0x2F)
    }

    /// Where `/name` stands in a text as a skill: the first at the start
    /// or after a space, followed by a space, a line or the end. In UTF-16
    /// units.
    static func range(of name: String, in text: String) -> NSRange? {
        let token = "/" + name
        var from = text.startIndex
        while let found = text.range(of: token, range: from..<text.endIndex) {
            let before = found.lowerBound == text.startIndex || text[text.index(before: found.lowerBound)].isWhitespace
            let after = found.upperBound == text.endIndex || text[found.upperBound].isWhitespace
            if before && after { return NSRange(found, in: text) }
            from = text.index(after: found.lowerBound)
        }
        return nil
    }

    /// The first `/name` in a text, at the start or after a space and
    /// with a space or a line after it, whose name is among `names`: a
    /// skill typed out whole rather than chosen.
    static func named(in text: String, among names: Set<String>) -> String? {
        guard !names.isEmpty else { return nil }
        var index = text.startIndex
        var previous: Character?
        while index < text.endIndex {
            let character = text[index]
            index = text.index(after: index)
            guard character == "/", previous?.isWhitespace ?? true else {
                previous = character
                continue
            }
            let end = text[index...].firstIndex { !isNameCharacter($0) } ?? text.endIndex
            if end < text.endIndex, text[end].isWhitespace, names.contains(String(text[index..<end])) {
                return String(text[index..<end])
            }
            previous = character
        }
        return nil
    }

    /// The message as it is sent for a skill: as typed when it starts with
    /// `/name`; otherwise `/name` first and then the words as typed, the
    /// chip's `/name` kept in its place, since Claude Code runs a skill
    /// only from a message's start and gives it the rest as its words.
    static func message(_ text: String, skill: String) -> String {
        if range(of: skill, in: text)?.location == 0 { return text }
        return "/\(skill) \(text)"
    }

    /// A message that runs a skill as the person's row draws it: the
    /// skill, the words after `/name`, and where in them the person put
    /// the chip, when it was in the middle; nil for a message that runs
    /// none.
    static func shown(_ text: String) -> (name: String, words: String, chip: NSRange?)? {
        guard let (name, rest) = split(text) else { return nil }
        let words = rest.trimmingCharacters(in: .whitespacesAndNewlines)
        return (name, words, range(of: name, in: words))
    }

    /// What the person typed, for a thread's title: the words with the
    /// chip in its place, or the message as it is.
    static func typed(_ text: String) -> String {
        guard let shown = shown(text), shown.chip != nil else { return text }
        return shown.words
    }

    /// A command's record in Claude Code's transcript, as the person's
    /// line: `<command-name>/name</command-name>` and
    /// `<command-args>words</command-args>` read as `/name words`. Nil for
    /// a line that is no command's, or a local command's, which has no
    /// words for the model.
    static func recorded(_ text: String) -> String? {
        guard let name = tagged("command-name", in: text), name.hasPrefix("/") else { return nil }
        guard text.contains("<command-message>") else { return nil }
        let args = tagged("command-args", in: text).map(AppContext.strip)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return args.isEmpty ? name : "\(name) \(args)"
    }

    private static func tagged(_ tag: String, in text: String) -> String? {
        guard let open = text.range(of: "<\(tag)>"), let close = text.range(of: "</\(tag)>", range: open.upperBound..<text.endIndex) else { return nil }
        return String(text[open.upperBound..<close.lowerBound])
    }
}
