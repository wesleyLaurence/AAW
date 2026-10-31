@testable import AAWApp
import XCTest

/// The typing keyboard's layout and state, and the scripted key lifted.
final class TypingKeyboardTests: XCTestCase {
    func testTheKeysAreAbletonsLayoutFromMiddleC() {
        let state = TypingKeyboard.State()
        XCTAssertEqual(state.octaveName, "C4")
        // A to K is one octave of white keys, C4 to C5; W to U the black keys between.
        let white = ["a", "s", "d", "f", "g", "h", "j", "k", "l", ";", "'"].map { TypingKeyboard.action(for: $0, state: state) }
        XCTAssertEqual(white, [60, 62, 64, 65, 67, 69, 71, 72, 74, 76, 77].map(TypingKeyboard.Action.note))
        let black = ["w", "e", "t", "y", "u", "o", "p"].map { TypingKeyboard.action(for: $0, state: state) }
        XCTAssertEqual(black, [61, 63, 66, 68, 70, 73, 75].map(TypingKeyboard.Action.note))
        // A capital, with Shift, is the same key.
        XCTAssertEqual(TypingKeyboard.action(for: "A", state: state), .note(60))
        XCTAssertEqual(TypingKeyboard.action(for: "z", state: state), .octaveDown)
        XCTAssertEqual(TypingKeyboard.action(for: "x", state: state), .octaveUp)
        XCTAssertEqual(TypingKeyboard.action(for: "c", state: state), .velocityDown)
        XCTAssertEqual(TypingKeyboard.action(for: "v", state: state), .velocityUp)
        // The rest of the keyboard is not the piano's: R stays Record, Space play.
        for other in ["r", "q", "i", "n", "m", "b", " ", "\u{1b}", "", "ab"] {
            XCTAssertNil(TypingKeyboard.action(for: other, state: state), other)
        }
    }

    func testTheOctaveAndVelocityMoveWithinTheirBounds() {
        var state = TypingKeyboard.State()
        state.move(octave: -1)
        XCTAssertEqual(TypingKeyboard.action(for: "a", state: state), .note(48))
        XCTAssertEqual(state.octaveName, "C3")
        for _ in 0..<20 { state.move(octave: -1) }
        XCTAssertEqual(state.octave, -1)
        XCTAssertEqual(TypingKeyboard.action(for: "a", state: state), .note(0), "C-1 is MIDI 0")
        for _ in 0..<20 { state.move(octave: 1) }
        XCTAssertEqual(state.octave, 8)
        XCTAssertEqual(TypingKeyboard.action(for: "a", state: state), .note(108))
        XCTAssertEqual(TypingKeyboard.action(for: "'", state: state), .note(125), "every key is a note at the top")
        state.move(velocity: 1)
        XCTAssertEqual(state.velocity, 120)
        state.move(velocity: 1)
        XCTAssertEqual(state.velocity, 127)
        for _ in 0..<10 { state.move(velocity: -1) }
        XCTAssertEqual(state.velocity, 1)
    }

    func testTheHelpSaysWhereTheKeysStart() {
        var state = TypingKeyboard.State()
        state.move(octave: -2)
        let on = TypingKeyboard.help(on: true, state: state)
        XCTAssertTrue(on.hasPrefix("Typing keyboard: on"), on)
        XCTAssertTrue(on.contains("from C2") && on.contains("(100)"), on)
        XCTAssertTrue(TypingKeyboard.help(on: false, state: state).contains("turns on when a MIDI track is armed"))
    }

    func testAScriptedKeyIsLiftedWithUp() {
        let launch = Launch(arguments: ["song.yaml", "--key", "a", "--key", "up+a", "--key", ";", "--key", "up+shift+a"])
        guard case .key(let down) = launch.actions[0], case .key(let up) = launch.actions[1],
              case .key(let semicolon) = launch.actions[2], case .key(let shifted) = launch.actions[3] else {
            return XCTFail("expected keys")
        }
        XCTAssertFalse(down.up)
        XCTAssertTrue(up.up)
        XCTAssertEqual(up.characters, "a")
        XCTAssertEqual(up.code, down.code)
        XCTAssertEqual(semicolon.code, 41)
        XCTAssertTrue(shifted.up)
        XCTAssertEqual(shifted.modifiers, [.shift])
    }

    func testRecordSpeaksOfMIDITracks() {
        XCTAssertTrue(RecordWords.record(recording: false, armed: 0, countIn: 1).contains("a MIDI track with an instrument"))
    }
}
