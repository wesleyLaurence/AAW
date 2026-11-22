import XCTest
@testable import AAWApp

/// The agent's questions: read from the tool's input, chosen as one or
/// several options and Other, answered in the words the tool takes, and
/// marked again from an answer that went back.
final class AgentQuestionsTests: XCTestCase {
    private let one = AgentQuestion(question: "Which drums?", header: "Drums",
                                    options: [.init(label: "808", description: "Low"), .init(label: "Breaks", description: "")])
    private let several = AgentQuestion(question: "Which parts?", header: "Parts", multiSelect: true,
                                        options: [.init(label: "Bass", description: ""), .init(label: "Keys", description: ""), .init(label: "Pads", description: "")])

    func testQuestionsAreReadFromTheInput() {
        let input: [String: Any] = ["questions": [
            ["question": "Which drums?", "header": "Drums", "multiSelect": false,
             "options": [["label": "808", "description": "Low", "preview": "x..x"], ["label": "Breaks", "description": "", "preview": ""]]],
        ]]
        let questions = AgentQuestion.list(from: input)
        XCTAssertEqual(questions.count, 1)
        XCTAssertEqual(questions[0].options.map(\.preview), ["x..x", nil])
        XCTAssertFalse(questions[0].multiSelect)
        // One question that cannot be read leaves the request as it came.
        XCTAssertEqual(AgentQuestion.list(from: ["questions": [["question": "Which?", "options": []], input["questions"].map { ($0 as! [[String: Any]])[0] }!]]), [])
        XCTAssertEqual(AgentQuestion.list(from: [:]), [])
    }

    func testOneChoiceReplacesTheLast() {
        var choice = one.choosing("808", in: .init())
        XCTAssertEqual(one.answer(choice), "808")
        choice = one.choosing("Breaks", in: choice)
        XCTAssertEqual(one.answer(choice), "Breaks")
        // Other replaces the option, and an option replaces Other.
        choice = one.choosingOther("Live drums", in: choice)
        XCTAssertEqual(choice.labels, [])
        XCTAssertEqual(one.answer(choice), "Live drums")
        XCTAssertEqual(one.answer(one.choosing("808", in: choice)), "808")
        // Other with no words is no answer yet.
        XCTAssertNil(one.answer(one.choosingOther("  ", in: .init())))
        XCTAssertNil(one.answer(.init()))
    }

    func testSeveralChoicesToggleInTheOptionsOrder() {
        var choice = several.choosing("Pads", in: .init())
        choice = several.choosing("Bass", in: choice)
        XCTAssertEqual(several.answer(choice), "Bass, Pads")
        choice = several.choosingOther("Strings", in: choice)
        XCTAssertEqual(several.answer(choice), "Bass, Pads, Strings")
        choice = several.choosing("Pads", in: choice)
        XCTAssertEqual(several.answer(choice), "Bass, Strings")
        XCTAssertEqual(several.answer(several.choosingOther(nil, in: choice)), "Bass")
    }

    func testEveryQuestionNeedsAnAnswer() {
        let first = one.choosing("808", in: .init())
        XCTAssertNil(AgentQuestion.answers([one, several], [first, .init()]))
        XCTAssertNil(AgentQuestion.answers([one, several], [first]))
        XCTAssertEqual(AgentQuestion.answers([one, several], [first, several.choosing("Keys", in: .init())]),
                       ["Which drums?": "808", "Which parts?": "Keys"])
    }

    func testAnAnswerIsMarkedAgain() {
        XCTAssertEqual(one.chosen(by: "Breaks"), .init(labels: ["Breaks"]))
        XCTAssertEqual(one.chosen(by: "Live drums"), .init(other: "Live drums"))
        XCTAssertEqual(several.chosen(by: "Bass, Pads, Strings"), .init(labels: ["Bass", "Pads"], other: "Strings"))
        XCTAssertEqual(several.chosen(by: "Keys"), .init(labels: ["Keys"]))
    }
}
