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
        let name = body.prefix { $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_" || $0 == ":" }
        guard !name.isEmpty else { return nil }
        let after = body[name.endIndex...]
        guard after.isEmpty || after.first!.isWhitespace else { return nil }
        return (String(name), String(after))
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
