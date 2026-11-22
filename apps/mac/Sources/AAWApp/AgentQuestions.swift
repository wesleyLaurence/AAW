import Foundation

/// A question the agent asks the person with choices, through Claude Code's
/// `AskUserQuestion` tool: its words, a short header, two to four options
/// and whether several may be chosen. The person may also answer in their
/// own words, as Other. The answer goes back in the tool's input as
/// `answers`, keyed by each question's words (D196).
struct AgentQuestion: Equatable {
    struct Option: Equatable {
        var label: String
        var description: String
        /// What the option would look like, drawn in a monospaced box.
        var preview: String?
    }

    var question: String
    var header: String
    var multiSelect: Bool
    var options: [Option]

    /// What the person chose for one question: options by label, and their
    /// own words when Other is chosen; nil `other` when it is not.
    struct Choice: Equatable {
        var labels: Set<String> = []
        var other: String?
    }

    /// Where a question stands: waiting for the person, answered with the
    /// words that went back, or not answered (refused, stopped, or left
    /// when the conversation ended).
    enum Outcome: Equatable {
        case waiting
        case answered([String: String])
        case unanswered
    }

    init(question: String, header: String = "", multiSelect: Bool = false, options: [Option]) {
        self.question = question
        self.header = header
        self.multiSelect = multiSelect
        self.options = options
    }

    /// A question from the tool's input; nil without words or options.
    init?(_ object: [String: Any]) {
        guard let question = object["question"] as? String, !question.isEmpty,
              let options = object["options"] as? [[String: Any]] else { return nil }
        let parsed = options.compactMap { option -> Option? in
            guard let label = option["label"] as? String, !label.isEmpty else { return nil }
            let preview = (option["preview"] as? String).flatMap { $0.isEmpty ? nil : $0 }
            return Option(label: label, description: option["description"] as? String ?? "", preview: preview)
        }
        guard !parsed.isEmpty else { return nil }
        self.init(question: question, header: object["header"] as? String ?? "",
                  multiSelect: object["multiSelect"] as? Bool ?? false, options: parsed)
    }

    /// The questions of the tool's input; none when any of them cannot be
    /// read, so that the request is shown as it came instead.
    static func list(from input: [String: Any]) -> [AgentQuestion] {
        guard let objects = input["questions"] as? [[String: Any]], !objects.isEmpty else { return [] }
        let questions = objects.compactMap(AgentQuestion.init)
        return questions.count == objects.count ? questions : []
    }

    /// The person's choice after clicking an option: one option replaces
    /// the last, or Other, for a single choice; options toggle, beside
    /// Other, for several.
    func choosing(_ label: String, in choice: Choice) -> Choice {
        var choice = choice
        if multiSelect {
            if choice.labels.contains(label) { choice.labels.remove(label) } else { choice.labels.insert(label) }
        } else {
            choice.labels = [label]
            choice.other = nil
        }
        return choice
    }

    /// The person's choice after clicking Other, or typing in its field:
    /// Other replaces the options for a single choice, and toggles beside
    /// them for several; typing keeps it chosen.
    func choosingOther(_ text: String?, in choice: Choice) -> Choice {
        var choice = choice
        choice.other = text
        if !multiSelect, text != nil { choice.labels = [] }
        return choice
    }

    /// The answer as the tool takes it: the chosen labels in the options'
    /// order, then Other's words, joined by commas; nil while nothing is
    /// chosen, or Other is chosen with no words.
    func answer(_ choice: Choice) -> String? {
        let other = choice.other?.trimmingCharacters(in: .whitespacesAndNewlines)
        if other?.isEmpty == true { return nil }
        let parts = options.map(\.label).filter { choice.labels.contains($0) } + (other.map { [$0] } ?? [])
        return parts.isEmpty ? nil : parts.joined(separator: ", ")
    }

    /// Every question's answer keyed by its words; nil until each one has
    /// an answer.
    static func answers(_ questions: [AgentQuestion], _ choices: [Choice]) -> [String: String]? {
        var answers: [String: String] = [:]
        for (i, question) in questions.enumerated() {
            guard i < choices.count, let answer = question.answer(choices[i]) else { return nil }
            answers[question.question] = answer
        }
        return answers
    }

    /// The choice an answer that went back stands for, to mark it again:
    /// the options it names, and what is left as Other's words.
    func chosen(by answer: String) -> Choice {
        if let option = options.first(where: { $0.label == answer }) { return Choice(labels: [option.label]) }
        guard multiSelect else { return Choice(other: answer) }
        var labels: Set<String> = []
        var rest: [String] = []
        for part in answer.components(separatedBy: ", ") {
            if options.contains(where: { $0.label == part }) { labels.insert(part) } else { rest.append(part) }
        }
        return Choice(labels: labels, other: rest.isEmpty ? nil : rest.joined(separator: ", "))
    }
}
