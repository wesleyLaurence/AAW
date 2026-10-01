import AAWCore
import AppKit
import SwiftUI

/// The app's colors. The window is always dark, as an arrangement usually is.
enum Theme {
    static func gray(_ white: CGFloat, _ alpha: CGFloat = 1) -> NSColor {
        NSColor(srgbRed: white, green: white, blue: white, alpha: alpha)
    }

    static func rgb(_ hex: UInt32, _ alpha: CGFloat = 1) -> NSColor {
        NSColor(
            srgbRed: CGFloat((hex >> 16) & 0xff) / 255,
            green: CGFloat((hex >> 8) & 0xff) / 255,
            blue: CGFloat(hex & 0xff) / 255,
            alpha: alpha
        )
    }

    static let background = gray(0.11)
    static let lane = gray(0.165)
    static let busLane = gray(0.135)
    static let header = gray(0.2)
    static let busHeader = gray(0.17)
    /// Over the header of the selected row.
    static let selectedRow = gray(1, 0.1)
    /// Around a selected clip.
    static let selectedClip = gray(1)
    /// Behind a value that can be dragged.
    static let control = gray(0, 0.28)
    /// Where a dragged row would land.
    static let insertion = rgb(0x4aa8ff)
    static let ruler = gray(0.15)
    static let separator = gray(0.07)
    static let barLine = gray(1, 0.13)
    static let gridLine = gray(1, 0.05)
    static let pastEnd = gray(0, 0.32)
    static let text = gray(0.9)
    static let dimText = gray(0.58)
    static let faintText = gray(0.42)
    static let playhead = gray(1)
    static let cue = rgb(0xffa23a)
    static let loop = rgb(0xc9ced6)
    static let mute = rgb(0xf2b134)
    static let solo = rgb(0x4aa8ff)
    static let section = rgb(0x4c525d)

    /// Whose change it was, in the activity panel and on what it touched.
    static func color(of origin: Who) -> NSColor {
        switch origin {
        case .agent: rgb(0xb78cff)
        case .user: rgb(0x4aa8ff)
        case .external: rgb(0xe0a94a)
        }
    }

    /// Track colors, given out in order as tracks are first seen.
    static let palette: [NSColor] = [
        rgb(0xe8704f), rgb(0xe9b44c), rgb(0x8ecf5d), rgb(0x4fc9b0), rgb(0x54a8e8),
        rgb(0x8d86f0), rgb(0xd67fd6), rgb(0xe86a8f), rgb(0xa9b86a), rgb(0x6fc1d9),
    ]
}

extension Who {
    var name: String {
        switch self {
        case .agent: "agent"
        case .user: "you"
        case .external: "file"
        }
    }

    var color: Color { Color(nsColor: Theme.color(of: self)) }
}
