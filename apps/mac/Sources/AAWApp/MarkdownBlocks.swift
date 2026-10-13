import Foundation

/// The agent's text cut into blocks, so that the panel draws each as itself:
/// paragraphs, headings, lists, fenced code, quotes, rules and tables. Only
/// the block structure is read here; the inline markdown of a paragraph, a
/// heading or a list item is read by the view with Foundation's reader,
/// which drops block structure, which is why this exists. A table is kept
/// as its lines, drawn in monospace until tables are wanted.
enum MarkdownBlock: Equatable {
    struct ListItem: Equatable {
        /// How many levels in the item is, 0 at the margin.
        var depth: Int
        var ordered: Bool
        /// The number the item was written with, for an ordered item.
        var number: Int
        var text: String
    }

    case paragraph(String)
    case heading(level: Int, text: String)
    case list([ListItem])
    /// Fenced code: the language after the fence, if any, and whether the
    /// closing fence has come, which it has not while the text streams.
    case code(language: String?, text: String, closed: Bool)
    case quote(String)
    case rule
    case table(String)

    /// The text of a block as it would be copied or searched.
    var text: String {
        switch self {
        case .paragraph(let text), .quote(let text), .table(let text): text
        case .heading(_, let text): text
        case .list(let items): items.map(\.text).joined(separator: "\n")
        case .code(_, let text, _): text
        case .rule: ""
        }
    }
}

enum MarkdownBlocks {
    /// Reads a text into its blocks. The text may stop anywhere, as a
    /// streaming message does: a fence without its close is a code block
    /// still open, and the last line is a block of its own kind.
    static func parse(_ text: String) -> [MarkdownBlock] {
        var blocks: [MarkdownBlock] = []
        var lines = text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)[...]
        // A text that ends with a newline has no empty last line of its own.
        if lines.last == "", lines.count > 1 { lines = lines.dropLast() }
        var paragraph: [String] = []
        var items: [MarkdownBlock.ListItem] = []
        var quote: [String] = []
        var table: [String] = []

        func flush() {
            if !paragraph.isEmpty { blocks.append(.paragraph(paragraph.joined(separator: "\n"))); paragraph = [] }
            if !items.isEmpty { blocks.append(.list(items)); items = [] }
            if !quote.isEmpty { blocks.append(.quote(quote.joined(separator: "\n"))); quote = [] }
            if !table.isEmpty { blocks.append(.table(table.joined(separator: "\n"))); table = [] }
        }

        while let line = lines.popFirst() {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if let fence = fence(line) {
                flush()
                var body: [String] = []
                var closed = false
                while let next = lines.popFirst() {
                    if next.trimmingCharacters(in: .whitespaces).hasPrefix(fence.marker) {
                        closed = true
                        break
                    }
                    body.append(next)
                }
                blocks.append(.code(language: fence.language, text: body.joined(separator: "\n"), closed: closed))
                continue
            }
            if trimmed.isEmpty {
                flush()
                continue
            }
            if let heading = heading(trimmed) {
                flush()
                blocks.append(.heading(level: heading.level, text: heading.text))
                continue
            }
            if isRule(trimmed) {
                flush()
                blocks.append(.rule)
                continue
            }
            if trimmed.hasPrefix(">") {
                if !paragraph.isEmpty || !items.isEmpty || !table.isEmpty { flush() }
                quote.append(String(trimmed.dropFirst().drop(while: { $0 == " " })))
                continue
            }
            if trimmed.hasPrefix("|") {
                if !paragraph.isEmpty || !items.isEmpty || !quote.isEmpty { flush() }
                table.append(trimmed)
                continue
            }
            if let item = listItem(line) {
                if !paragraph.isEmpty || !quote.isEmpty || !table.isEmpty { flush() }
                items.append(item)
                continue
            }
            if !items.isEmpty, line.first == " " || line.first == "\t" {
                // An indented line under a list item goes on with it.
                items[items.count - 1].text += "\n" + trimmed
                continue
            }
            if !quote.isEmpty {
                // A line after a quote's marker goes on with the quote.
                quote.append(trimmed)
                continue
            }
            if !items.isEmpty || !table.isEmpty { flush() }
            paragraph.append(line)
        }
        flush()
        return blocks
    }

    /// A fence opening: three or more backticks or tildes, and the
    /// language written after them.
    private static func fence(_ line: String) -> (marker: String, language: String?)? {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        for mark: Character in ["`", "~"] {
            let run = trimmed.prefix { $0 == mark }
            guard run.count >= 3 else { continue }
            let rest = trimmed.dropFirst(run.count).trimmingCharacters(in: .whitespaces)
            // Backticks in the rest make it inline code, not a fence.
            if mark == "`", rest.contains("`") { return nil }
            return (String(run), rest.isEmpty ? nil : rest)
        }
        return nil
    }

    private static func heading(_ line: String) -> (level: Int, text: String)? {
        let hashes = line.prefix { $0 == "#" }
        guard (1...6).contains(hashes.count) else { return nil }
        let rest = line.dropFirst(hashes.count)
        guard rest.first == " " || rest.first == "\t" else { return nil }
        var text = rest.trimmingCharacters(in: .whitespaces)
        // A heading may be closed with hashes of its own.
        while text.hasSuffix("#") { text = String(text.dropLast()) }
        return (hashes.count, text.trimmingCharacters(in: .whitespaces))
    }

    private static func isRule(_ line: String) -> Bool {
        let bare = line.replacingOccurrences(of: " ", with: "")
        guard bare.count >= 3, let first = bare.first, "-*_".contains(first) else { return false }
        return bare.allSatisfy { $0 == first }
    }

    /// A list item: `-`, `*` or `+`, or a number with `.` or `)`, then a
    /// space; its depth from the spaces before the marker, two to a level.
    private static func listItem(_ line: String) -> MarkdownBlock.ListItem? {
        let indent = line.prefix { $0 == " " || $0 == "\t" }
        let rest = line.dropFirst(indent.count)
        let depth = indent.reduce(0) { $0 + ($1 == "\t" ? 2 : 1) } / 2
        if let first = rest.first, "-*+".contains(first), rest.dropFirst().first == " " {
            return .init(depth: depth, ordered: false, number: 0, text: rest.dropFirst(2).trimmingCharacters(in: .whitespaces))
        }
        let digits = rest.prefix { $0.isNumber }
        guard !digits.isEmpty, digits.count <= 9, let number = Int(digits) else { return nil }
        let after = rest.dropFirst(digits.count)
        guard let mark = after.first, mark == "." || mark == ")", after.dropFirst().first == " " else { return nil }
        return .init(depth: depth, ordered: true, number: number, text: after.dropFirst(2).trimmingCharacters(in: .whitespaces))
    }
}
