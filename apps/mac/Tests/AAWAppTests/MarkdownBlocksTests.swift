import XCTest
@testable import AAWApp

/// The agent's text cut into blocks: what the panel draws each part as.
final class MarkdownBlocksTests: XCTestCase {
    func testParagraphsHeadingsAndLists() {
        let text = """
        Here is what I did.

        ## The hats
        - Moved them off the beat
        - Quieter by 3 dB
          with a second line
        1. First
        2) Second

        A `daw` line
        and its next line.
        """
        XCTAssertEqual(MarkdownBlocks.parse(text), [
            .paragraph("Here is what I did."),
            .heading(level: 2, text: "The hats"),
            .list([
                .init(depth: 0, ordered: false, number: 0, text: "Moved them off the beat"),
                .init(depth: 0, ordered: false, number: 0, text: "Quieter by 3 dB\nwith a second line"),
                .init(depth: 0, ordered: true, number: 1, text: "First"),
                .init(depth: 0, ordered: true, number: 2, text: "Second"),
            ]),
            .paragraph("A `daw` line\nand its next line."),
        ])
    }

    func testNestedItemsKeepTheirDepth() {
        XCTAssertEqual(MarkdownBlocks.parse("- a\n  - b\n    * c\n- d"), [
            .list([
                .init(depth: 0, ordered: false, number: 0, text: "a"),
                .init(depth: 1, ordered: false, number: 0, text: "b"),
                .init(depth: 2, ordered: false, number: 0, text: "c"),
                .init(depth: 0, ordered: false, number: 0, text: "d"),
            ]),
        ])
    }

    func testFencesQuotesRulesAndTables() {
        let text = """
        Run this:
        ```sh
        daw status .

        daw map .
        ```
        > a quote
        continued
        ---
        | a | b |
        |---|---|
        | 1 | 2 |
        ~~~
        plain
        ~~~
        """
        XCTAssertEqual(MarkdownBlocks.parse(text), [
            .paragraph("Run this:"),
            .code(language: "sh", text: "daw status .\n\ndaw map .", closed: true),
            .quote("a quote\ncontinued"),
            .rule,
            .table("| a | b |\n|---|---|\n| 1 | 2 |"),
            .code(language: nil, text: "plain", closed: true),
        ])
    }

    /// A streaming text stops anywhere: an open fence is a code block not
    /// yet closed, and each piece reads as what it is so far.
    func testAStreamingTextReadsAsFarAsItGoes() {
        XCTAssertEqual(MarkdownBlocks.parse("Done. Now\n```\ndaw ad"), [
            .paragraph("Done. Now"),
            .code(language: nil, text: "daw ad", closed: false),
        ])
        XCTAssertEqual(MarkdownBlocks.parse("- one\n- tw"), [
            .list([.init(depth: 0, ordered: false, number: 0, text: "one"), .init(depth: 0, ordered: false, number: 0, text: "tw")]),
        ])
        XCTAssertEqual(MarkdownBlocks.parse("## "), [.paragraph("## ")])
        XCTAssertEqual(MarkdownBlocks.parse(""), [])
        XCTAssertEqual(MarkdownBlocks.parse("\n\n"), [])
        XCTAssertEqual(MarkdownBlocks.parse("text\n"), [.paragraph("text")])
    }

    /// What is not a block marker stays in its paragraph: a dash with no
    /// space, a number in a sentence, inline backticks, a hash in text.
    func testWhatIsNotAMarker() {
        XCTAssertEqual(MarkdownBlocks.parse("-6 dB on the bass\n2024 was the year\n#tag here\n```not a fence`"), [
            .paragraph("-6 dB on the bass\n2024 was the year\n#tag here\n```not a fence`"),
        ])
        XCTAssertEqual(MarkdownBlock.list([.init(depth: 0, ordered: false, number: 0, text: "a")]).text, "a")
        XCTAssertEqual(MarkdownBlock.code(language: "sh", text: "ls", closed: true).text, "ls")
    }
}
