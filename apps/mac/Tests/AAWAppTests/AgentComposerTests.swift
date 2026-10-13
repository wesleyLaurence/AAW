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

    /// While the list shows, its keys go to it; the chosen skill is a chip
    /// that the caret stays after, that Backspace takes whole, and that
    /// goes with its name.
    func testTheListsKeysAndTheChip() throws {
        let (_, text) = ComposerTextView.make()
        text.frame = NSRect(x: 0, y: 0, width: 240, height: 40)
        var listKeys: [ComposerTextView.ListKey] = []
        var sent = 0
        var left = 0
        text.onSend = { sent += 1 }
        text.onEscape = { left += 1 }
        text.onListKey = { key in
            listKeys.append(key)
            return key != .close
        }
        text.keyDown(with: key(126, [.function, .numericPad], "\u{f700}"))
        text.keyDown(with: key(125, [.function, .numericPad], "\u{f701}"))
        text.keyDown(with: key(48, [], "\t"))
        text.keyDown(with: key(36))
        XCTAssertEqual(listKeys, [.up, .down, .choose, .choose])
        XCTAssertEqual(sent, 0, "Return chose rather than sent")
        text.keyDown(with: key(53, [], "\u{1b}"))
        XCTAssertEqual(listKeys.last, .close)
        XCTAssertEqual(left, 1, "a close the list did not take leaves the field")
        XCTAssertNil(ComposerTextView.listKey(for: key(36, .shift)), "Shift-Return is the field's")
        text.onListKey = nil
        text.keyDown(with: key(36))
        XCTAssertEqual(sent, 1)

        text.chip = "roar-edit"
        text.hint = "[times]"
        text.set("/roar-edit ")
        XCTAssertEqual(text.chipRange, NSRange(location: 0, length: 10))
        XCTAssertTrue(text.showsHint)
        XCTAssertNotNil(text.chipRect)
        XCTAssertEqual(text.selectedRange(), NSRange(location: 11, length: 0), "the caret is after the chip and its space")
        text.setSelectedRange(NSRange(location: 3, length: 0))
        XCTAssertEqual(text.selectedRange(), NSRange(location: 10, length: 0), "a caret put inside the chip lands after it")
        text.insertText("from :41", replacementRange: NSRange(location: 11, length: 0))
        XCTAssertEqual(text.string, "/roar-edit from :41")
        XCTAssertFalse(text.showsHint)
        XCTAssertEqual(text.chipRange, NSRange(location: 0, length: 10))

        text.set("/roar-edit ")
        text.setSelectedRange(NSRange(location: 10, length: 0))
        text.deleteBackward(nil)
        XCTAssertEqual(text.string, " ", "Backspace at the chip's end took it whole")
        XCTAssertNil(text.chipRange)

        text.set("/roar")
        XCTAssertNil(text.chipRange, "a chip whose name the text no longer starts with is not drawn")
        text.set("/roar-edits")
        XCTAssertNil(text.chipRange)
        text.set("/roar-edit\nmore")
        XCTAssertEqual(text.chipRange, NSRange(location: 0, length: 10))
    }
}
