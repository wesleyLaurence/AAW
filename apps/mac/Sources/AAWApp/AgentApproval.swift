import Foundation

/// What the agent may do without asking: `daw` commands on the open project,
/// and reading a file the person attached to the conversation. Anything
/// else it asks for is shown in the panel to allow or refuse, Claude Code
/// having first applied its own rules, which allow reads in the project's
/// folder and its safe commands by itself.
enum Approval {
    enum Decision: Equatable {
        case allow
        /// Asked in the panel, with the request in a line.
        case ask(String)
    }

    /// Decides a tool request. `project` is the project's folder, the
    /// agent's working folder; `attached` the paths the person attached in
    /// this conversation, which count as inside the project for a read: the
    /// Read tool, or a command that inspects a file.
    static func decide(tool: String, input: [String: Any], project: URL, attached: Set<String> = []) -> Decision {
        let summary = ClaudeTranscripts.toolSummary(name: tool, input: input)
        if tool == "Read", let path = input["file_path"] as? String, isAttached(path, attached: attached) { return .allow }
        guard tool == "Bash", let command = input["command"] as? String else {
            return .ask(summary.isEmpty ? tool : "\(tool) \(summary)")
        }
        return isDaw(command: command, project: project, attached: attached) ? .allow : .ask(command)
    }

    /// Programs a `daw` command's output may be piped through: they read
    /// and print, and write nothing.
    static let readers: Set<String> = ["head", "tail", "grep", "jq", "wc", "sort", "uniq", "cut", "cat", "tr", "column", "less", "more"]

    /// Programs that read a file and say what it is, allowed on an attached
    /// file as `daw` is.
    static let inspectors: Set<String> = ["afinfo", "file", "ffprobe", "mdls", "xxd", "strings"]

    /// Whether a shell line is `daw`, or `uv run daw`, on this project: each
    /// command is `daw` with no path that leads outside the project's
    /// folder, or one of the readers its output goes through. With files
    /// attached, a command that inspects one counts as `daw` does.
    static func isDaw(command: String, project: URL, attached: Set<String> = []) -> Bool {
        let simple = split(command)
        guard simple.contains(where: { isDaw(words: $0, project: project, attached: attached) || isInspector(words: $0, project: project, attached: attached) }) else { return false }
        return simple.allSatisfy {
            isDaw(words: $0, project: project, attached: attached) || isReader(words: $0, project: project, attached: attached)
                || isInspector(words: $0, project: project, attached: attached)
        }
    }

    private static func isReader(words: [String], project: URL, attached: Set<String>) -> Bool {
        guard let first = words.first, readers.contains(first) else { return false }
        return words.dropFirst().allSatisfy { !leadsOutside($0, project: project, attached: attached) }
    }

    /// An inspector on an attached file, and on nothing outside the project.
    private static func isInspector(words: [String], project: URL, attached: Set<String>) -> Bool {
        guard let first = words.first, inspectors.contains(first), !attached.isEmpty else { return false }
        let rest = words.dropFirst()
        return rest.contains { isAttached($0, attached: attached) } && rest.allSatisfy { !leadsOutside($0, project: project, attached: attached) }
    }

    private static func isDaw(words: [String], project: URL, attached: Set<String>) -> Bool {
        var words = words[...]
        // VAR=value ahead of the command.
        while let first = words.first, first.contains("="), !first.hasPrefix("-"),
              first.prefix(while: { $0 != "=" }).allSatisfy({ $0.isLetter || $0.isNumber || $0 == "_" }) {
            words.removeFirst()
        }
        if words.first == "uv", words.dropFirst().first == "run" { words.removeFirst(2) }
        guard words.first == "daw" else { return false }
        return words.dropFirst().allSatisfy { !leadsOutside($0, project: project, attached: attached) }
    }

    /// Whether an absolute path is one the person attached, or inside an
    /// attached folder.
    static func isAttached(_ word: String, attached: Set<String>) -> Bool {
        guard word.hasPrefix("/"), !attached.isEmpty else { return false }
        let path = URL(fileURLWithPath: word).standardizedFileURL.path
        return attached.contains { path == $0 || path.hasPrefix($0 + "/") }
    }

    /// Whether a word names a place outside the project: an absolute path
    /// elsewhere, a home path, or a relative one that climbs out. An
    /// attached path is not outside.
    static func leadsOutside(_ word: String, project: URL, attached: Set<String> = []) -> Bool {
        let root = project.standardizedFileURL.path
        if word.hasPrefix("/") {
            if isAttached(word, attached: attached) { return false }
            let path = URL(fileURLWithPath: word).standardizedFileURL.path
            return !(path == root || path.hasPrefix(root + "/"))
        }
        if word.hasPrefix("~") { return true }
        // A relative path, or an option's value such as `--to=../x`.
        let value = word.hasPrefix("-") ? String(word.drop { $0 != "=" }.dropFirst()) : word
        guard !value.isEmpty else { return false }
        if value.hasPrefix("/") || value.hasPrefix("~") { return leadsOutside(value, project: project, attached: attached) }
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
