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

    /// A thing of the song named with `@` is a chip wherever it stands:
    /// white on the fill, taken whole by Backspace, the caret kept out of
    /// it; the field tells where the caret is, and puts it where asked.
    func testMentionsAreChips() throws {
        let (_, text) = ComposerTextView.make()
        text.frame = NSRect(x: 0, y: 0, width: 320, height: 40)
        var carets: [Int] = []
        text.onCaret = { carets.append($0) }
        text.mentions = ["drums", "hook.2", "kick.wav"]
        text.set("make @drums louder than @hook.2, not @hook")
        XCTAssertEqual(text.mentionRanges, [NSRange(location: 5, length: 6), NSRange(location: 24, length: 7)])
        XCTAssertEqual(text.chipRanges, text.mentionRanges, "no skill chip")
        XCTAssertEqual(text.chipRects.count, 2)
        XCTAssertEqual(text.color(at: 6), ComposerTextView.chipText)
        XCTAssertEqual(text.color(at: 0), text.textColor)
        XCTAssertEqual(text.color(at: 38), text.textColor, "@hook names nothing")
        XCTAssertEqual(carets.last, (text.string as NSString).length, "the caret at the end was told")

        text.setSelectedRange(NSRange(location: 8, length: 0))
        XCTAssertEqual(text.selectedRange(), NSRange(location: 11, length: 0), "a caret put inside a chip lands after it")
        XCTAssertEqual(carets.last, 11)
        text.setSelectedRange(NSRange(location: 5, length: 0))
        XCTAssertEqual(text.selectedRange(), NSRange(location: 5, length: 0), "before the chip is fine")
        text.deleteBackward(nil)
        XCTAssertEqual(text.string, "make@drums louder than @hook.2, not @hook", "Backspace before a chip takes the character before it")
        XCTAssertEqual(text.mentionRanges, [NSRange(location: 23, length: 7)], "an @ inside a word is no chip")
        text.setSelectedRange(NSRange(location: 30, length: 0))
        text.deleteBackward(nil)
        XCTAssertEqual(text.string, "make@drums louder than , not @hook", "Backspace at a chip's end took it whole")
        XCTAssertEqual(text.mentionRanges, [])

        // Both kinds of chip together, and the caret put where asked.
        text.chip = "roar-edit"
        text.set("/roar-edit @drums ", caret: 7)
        XCTAssertEqual(text.chipRanges, [NSRange(location: 0, length: 10), NSRange(location: 11, length: 6)])
        XCTAssertEqual(text.selectedRange(), NSRange(location: 10, length: 0), "asked for inside the skill's chip, lands after it")
        text.set("/roar-edit @drums ", caret: 18)
        XCTAssertEqual(text.selectedRange(), NSRange(location: 18, length: 0), "the same text, the caret moved")
        text.set("/roar-edit @drums ", caret: 99)
        XCTAssertEqual(text.selectedRange(), NSRange(location: 18, length: 0))
        text.mentions = []
        XCTAssertEqual(text.chipRanges, [NSRange(location: 0, length: 10)])
    }
}
