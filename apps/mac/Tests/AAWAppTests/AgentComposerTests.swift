import AppKit
import XCTest
@testable import AAWApp

/// The field the person types to the agent in: which key sends, which
/// breaks the line, which leaves, and how tall the text makes it.
@MainActor
final class AgentComposerTests: XCTestCase {
    private func key(_ code: UInt16, _ flags: NSEvent.ModifierFlags = [], _ characters: String = "\r") -> NSEvent {
        NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0, windowNumber: 0, context: nil,
                         characters: characters, charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code)!
    }

    func testWhatEachKeyDoes() {
        XCTAssertEqual(ComposerTextView.key(for: key(36)), .send)
        XCTAssertEqual(ComposerTextView.key(for: key(76)), .send, "Enter on the keypad sends too")
        XCTAssertEqual(ComposerTextView.key(for: key(36, .shift)), .newline)
        XCTAssertEqual(ComposerTextView.key(for: key(36, .option)), .send, "Option-Return no longer breaks the line")
        XCTAssertEqual(ComposerTextView.key(for: key(53, [], "\u{1b}")), .escape)
        XCTAssertEqual(ComposerTextView.key(for: key(0, [], "a")), .other)
    }

    func testReturnSendsShiftReturnBreaksTheLineAndTheFieldGrows() throws {
        let (scroll, text) = ComposerTextView.make()
        XCTAssertTrue(scroll.documentView === text)
        scroll.frame = NSRect(x: 0, y: 0, width: 240, height: 40)
        text.frame = NSRect(x: 0, y: 0, width: 240, height: 40)
        var sent = 0
        var left = 0
        var changes: [String] = []
        var heights: [CGFloat] = []
        text.onSend = { sent += 1 }
        text.onEscape = { left += 1 }
        text.onChange = { changes.append($0) }
        text.onHeight = { heights.append($0) }

        text.insertText("hello", replacementRange: NSRange(location: 0, length: 0))
        XCTAssertEqual(changes, ["hello"])
        let one = try XCTUnwrap(heights.last)
        text.keyDown(with: key(36, .shift))
        text.insertText("there", replacementRange: text.selectedRange())
        XCTAssertEqual(text.string, "hello\nthere")
        XCTAssertEqual(sent, 0)
        let two = try XCTUnwrap(heights.last)
        XCTAssertGreaterThan(two, one)
        XCTAssertLessThan(two, one * 2, "a second line adds a line, not the insets again")

        // Eight lines is the most it grows; more scrolls.
        text.set(Array(repeating: "line", count: 12).joined(separator: "\n"))
        let most = try XCTUnwrap(heights.last)
        text.set(Array(repeating: "line", count: 8).joined(separator: "\n"))
        XCTAssertEqual(heights.last, most)

        text.keyDown(with: key(36))
        XCTAssertEqual(sent, 1)
        text.keyDown(with: key(53, [], "\u{1b}"))
        XCTAssertEqual(left, 1)

        // Cleared from outside, as after a send, it is one line again.
        text.set("")
        XCTAssertEqual(text.string, "")
        XCTAssertEqual(heights.last, one)
    }
}
