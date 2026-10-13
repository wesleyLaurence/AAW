import Foundation

/// What the agent may do without asking: `daw` commands on the open project.
/// Anything else it asks for is shown in the panel to allow or refuse,
/// Claude Code having first applied its own rules, which allow reads in the
/// project's folder and its safe commands by itself.
enum Approval {
    enum Decision: Equatable {
        case allow
        /// Asked in the panel, with the request in a line.
        case ask(String)
    }

    /// Decides a tool request. `project` is the project's folder, the
    /// agent's working folder.
    static func decide(tool: String, input: [String: Any], project: URL) -> Decision {
        let summary = ClaudeTranscripts.toolSummary(name: tool, input: input)
        guard tool == "Bash", let command = input["command"] as? String else {
            return .ask(summary.isEmpty ? tool : "\(tool) \(summary)")
        }
        return isDaw(command: command, project: project) ? .allow : .ask(command)
    }

    /// Programs a `daw` command's output may be piped through: they read
    /// and print, and write nothing.
    static let readers: Set<String> = ["head", "tail", "grep", "jq", "wc", "sort", "uniq", "cut", "cat", "tr", "column", "less", "more"]

    /// Whether a shell line is `daw`, or `uv run daw`, on this project: each
    /// command is `daw` with no path that leads outside the project's
    /// folder, or one of the readers its output goes through.
    static func isDaw(command: String, project: URL) -> Bool {
        let simple = split(command)
        guard simple.contains(where: { isDaw(words: $0, project: project) }) else { return false }
        return simple.allSatisfy { isDaw(words: $0, project: project) || isReader(words: $0, project: project) }
    }

    private static func isReader(words: [String], project: URL) -> Bool {
        guard let first = words.first, readers.contains(first) else { return false }
        return words.dropFirst().allSatisfy { !leadsOutside($0, project: project) }
    }

    private static func isDaw(words: [String], project: URL) -> Bool {
        var words = words[...]
        // VAR=value ahead of the command.
        while let first = words.first, first.contains("="), !first.hasPrefix("-"),
              first.prefix(while: { $0 != "=" }).allSatisfy({ $0.isLetter || $0.isNumber || $0 == "_" }) {
            words.removeFirst()
        }
        if words.first == "uv", words.dropFirst().first == "run" { words.removeFirst(2) }
        guard words.first == "daw" else { return false }
        return words.dropFirst().allSatisfy { !leadsOutside($0, project: project) }
    }

    /// Whether a word names a place outside the project: an absolute path
    /// elsewhere, a home path, or a relative one that climbs out.
    static func leadsOutside(_ word: String, project: URL) -> Bool {
        let root = project.standardizedFileURL.path
        if word.hasPrefix("/") {
            let path = URL(fileURLWithPath: word).standardizedFileURL.path
            return !(path == root || path.hasPrefix(root + "/"))
        }
        if word.hasPrefix("~") { return true }
        // A relative path, or an option's value such as `--to=../x`.
        let value = word.hasPrefix("-") ? String(word.drop { $0 != "=" }.dropFirst()) : word
        guard !value.isEmpty else { return false }
        if value.hasPrefix("/") || value.hasPrefix("~") { return leadsOutside(value, project: project) }
        var depth = 0
        for part in value.split(separator: "/", omittingEmptySubsequences: true) {
            switch part {
            case "..": depth -= 1
            case ".": break
            default: depth += 1
            }
            if depth < 0 { return true }
        }
        return false
    }

    /// The simple commands of a shell line, each as its words: split at
    /// `&&`, `||`, `|`, `;` and newlines outside quotes, with quotes taken
    /// off the words. A line the splitter cannot read, such as one with a
    /// subshell or a redirection, is one command of no words, which is not
    /// `daw`.
    static func split(_ line: String) -> [[String]] {
        var commands: [[String]] = []
        var words: [String] = []
        var word = ""
        var inWord = false
        var quote: Character?
        var escaped = false
        func endWord() {
            if inWord { words.append(word) }
            word = ""
            inWord = false
        }
        func endCommand() {
            endWord()
            commands.append(words)
            words = []
        }
        let characters = Array(line)
        var i = 0
        while i < characters.count {
            let c = characters[i]
            defer { i += 1 }
            if escaped {
                word.append(c)
                escaped = false
                continue
            }
            if let q = quote {
                if c == q { quote = nil } else if c == "\\", q == "\"" { escaped = true } else { word.append(c) }
                continue
            }
            switch c {
            case "'", "\"":
                quote = c
                inWord = true
            case "\\":
                escaped = true
                inWord = true
            case " ", "\t":
                endWord()
            case "\n", ";":
                endCommand()
            case "&", "|":
                endCommand()
                if i + 1 < characters.count, characters[i + 1] == c { i += 1 }
            case "(", ")", "<", ">", "`", "$":
                // Beyond this splitter: a command that is not read as daw.
                return [[]]
            default:
                word.append(c)
                inWord = true
            }
        }
        if quote != nil || escaped { return [[]] }
        endCommand()
        return commands.filter { !$0.isEmpty }
    }
}
