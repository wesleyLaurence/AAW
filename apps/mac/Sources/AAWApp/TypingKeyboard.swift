import Foundation

/// The typing keyboard: the Mac's keys as a piano, Ableton's layout. The
/// white keys from C are A S D F G H J K L ; ' and the black keys above them
/// W E T Y U O P, so A to K is one octave. Z and X move the octave down and
/// up, C and V the velocity down and up by twenty. A is C4, middle C, until
/// the octave is moved.
public enum TypingKeyboard {
    /// What a key does while the keyboard is on.
    public enum Action: Equatable {
        /// A note, as a MIDI number.
        case note(Int)
        case octaveDown
        case octaveUp
        case velocityDown
        case velocityUp
    }

    /// The octave the keys start at and how hard they play, which Z, X, C
    /// and V move.
    public struct State: Equatable {
        /// The octave of the A key's C, 4 for middle C.
        public var octave = 4
        public var velocity = 100

        public init(octave: Int = 4, velocity: Int = 100) {
            self.octave = octave
            self.velocity = velocity
        }

        /// The name of the A key's note: C4.
        public var octaveName: String { "C\(octave)" }

        /// The octave moved by `steps`, kept where every key is a MIDI note.
        public mutating func move(octave steps: Int) {
            octave = min(max(octave + steps, Self.lowestOctave), Self.highestOctave)
        }

        /// The velocity moved by `steps` of twenty, 1 to 127.
        public mutating func move(velocity steps: Int) {
            velocity = min(max(velocity + steps * Self.velocityStep, 1), 127)
        }

        /// C-1 is 0; the ' key, 17 above A, must stay under 128: C8 + 17 is 125.
        public static let lowestOctave = -1
        public static let highestOctave = 8
        public static let velocityStep = 20
    }

    /// Semitones above the A key's C for each note key.
    static let notes: [Character: Int] = [
        "a": 0, "w": 1, "s": 2, "e": 3, "d": 4, "f": 5, "t": 6, "g": 7, "y": 8, "h": 9, "u": 10, "j": 11,
        "k": 12, "o": 13, "l": 14, "p": 15, ";": 16, "'": 17,
    ]

    /// What a key does at `state`, by the character it types with no
    /// modifiers, or nil for a key that is not the keyboard's.
    public static func action(for characters: String, state: State) -> Action? {
        guard characters.count == 1, let c = characters.lowercased().first else { return nil }
        switch c {
        case "z": return .octaveDown
        case "x": return .octaveUp
        case "c": return .velocityDown
        case "v": return .velocityUp
        default:
            guard let offset = notes[c] else { return nil }
            let pitch = (state.octave + 1) * 12 + offset
            return (0...127).contains(pitch) ? .note(pitch) : nil
        }
    }

    /// The transport bar's help for the keyboard's icon.
    public static func help(on: Bool, state: State) -> String {
        let keys = "A to ' play the white keys from \(state.octaveName), W to P the black keys; Z and X move the octave, C and V the velocity (\(state.velocity))."
        if on {
            return "Typing keyboard: on (⇧⌘K). \(keys) The keys play the armed MIDI tracks, or the selected one. L, M and A are notes while it is on."
        }
        return "Typing keyboard: off (⇧⌘K). On, \(keys) It turns on when a MIDI track is armed."
    }
}
