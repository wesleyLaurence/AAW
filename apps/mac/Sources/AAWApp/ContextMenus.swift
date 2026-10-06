import Foundation

/// What a right click, or a click with Control held, offers on a header or a
/// clip of the arrangement. The items and their order, with what the row or
/// the clip can do just now; the view makes the menu from them. Lists only,
/// so that they can be tested.
public enum ContextMenu {
    /// What an item of a header's menu does, to the row that was clicked.
    public enum RowAction: Equatable {
        case rename, mute, solo, automation, addTrack, addMIDITrack, addReturn, delete
    }

    /// What an item of a clip's menu does, to the selected clips.
    public enum ClipAction: Equatable {
        case edit, cut, copy, duplicate, split, loop, delete
    }

    /// One line of a menu: a separator when it has no action.
    public struct Item<Action: Equatable>: Equatable {
        public var title: String
        public var action: Action?
        public var enabled = true
        /// The key the main menu has for the same thing, shown beside it.
        public var key = ""
        public var command = true

        public static var separator: Item { Item(title: "", action: nil) }
    }

    /// A header's menu: Rename, what a row is set to, a row added after it,
    /// and Delete. The master has no name of its own, no mute mark and cannot
    /// go; a return has no solo.
    public static func row(kind: HeaderLayout.Kind, muted: Bool, soloed: Bool, lanesShown: Bool) -> [Item<RowAction>] {
        var items: [Item<RowAction>] = []
        if kind != .master {
            items.append(Item(title: "Rename", action: .rename, key: "r"))
            items.append(.separator)
            items.append(Item(title: muted ? "Unmute" : "Mute", action: .mute))
        }
        if kind == .track {
            items.append(Item(title: soloed ? "Unsolo" : "Solo", action: .solo))
        }
        items.append(Item(title: lanesShown ? "Hide Automation" : "Show Automation", action: .automation))
        items.append(.separator)
        switch kind {
        case .track:
            items.append(Item(title: "Add Track", action: .addTrack, key: "t"))
            items.append(Item(title: "Add MIDI Track", action: .addMIDITrack, key: "T"))
        case .bus, .master:
            items.append(Item(title: "Add Track", action: .addTrack, key: "t"))
            items.append(Item(title: "Add MIDI Track", action: .addMIDITrack, key: "T"))
            items.append(Item(title: "Add Return", action: .addReturn, key: "t"))
        }
        if kind != .master {
            items.append(.separator)
            items.append(Item(title: "Delete", action: .delete, key: "\u{8}", command: false))
        }
        return items
    }

    /// A clip's menu: its editor by name, then what the Edit menu does to the
    /// selected clips, and the loop set over them as L sets it. `canSplit` is
    /// whether the start position is inside an audio clip of the selection.
    public static func clip(editor: String, canSplit: Bool) -> [Item<ClipAction>] {
        [
            Item(title: "Edit \(editor)", action: .edit),
            .separator,
            Item(title: "Cut", action: .cut, key: "x"),
            Item(title: "Copy", action: .copy, key: "c"),
            Item(title: "Duplicate", action: .duplicate, key: "d"),
            Item(title: "Split at Start Position", action: .split, enabled: canSplit, key: "e"),
            .separator,
            Item(title: "Loop Selection", action: .loop, key: "l", command: false),
            .separator,
            Item(title: "Delete", action: .delete, key: "\u{8}", command: false),
        ]
    }
}
