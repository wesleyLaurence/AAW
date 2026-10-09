import Foundation

/// What a right click, or a click with Control held, offers on a header or a
/// clip of the arrangement. The items and their order, with what the row or
/// the clip can do just now; the view makes the menu from them. Lists only,
/// so that they can be tested.
public enum ContextMenu {
    /// What an item of a header's menu does, to the row that was clicked.
    public enum RowAction: Equatable {
        case rename, mute, solo, automation, addTrack, addMIDITrack, addReturn, delete
        /// Group Tracks on a track, Ungroup on a group, and Remove from Group
        /// on a track in one.
        case group, ungroup, leaveGroup
    }

    /// What an item of a clip's menu does, to the selected clips.
    public enum ClipAction: Equatable {
        case edit, cut, copy, duplicate, split, join, loop, delete
    }

    /// What an item of an effect's menu does, to the effect that was clicked
    /// in the device panel.
    public enum EffectAction: Equatable {
        case cut, copy, paste, duplicate, bypass, delete
    }

    /// What an item of the marker strip's menu does: to the marker that was
    /// clicked, or at the place clicked in the clear.
    public enum MarkerAction: Equatable {
        case add, rename, delete, deleteAll
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
    /// go; a return has no solo. A track offers Group Tracks, or Remove from
    /// Group when it is in one (`grouped`); a group offers Ungroup.
    public static func row(kind: HeaderLayout.Kind, muted: Bool, soloed: Bool, lanesShown: Bool, grouped: Bool = false) -> [Item<RowAction>] {
        var items: [Item<RowAction>] = []
        if kind != .master {
            items.append(Item(title: "Rename", action: .rename, key: "r"))
            items.append(.separator)
            items.append(Item(title: muted ? "Unmute" : "Mute", action: .mute))
        }
        if kind == .track || kind == .group {
            items.append(Item(title: soloed ? "Unsolo" : "Solo", action: .solo))
        }
        items.append(Item(title: lanesShown ? "Hide Automation" : "Show Automation", action: .automation))
        items.append(.separator)
        switch kind {
        case .track:
            items.append(Item(title: "Add Track", action: .addTrack, key: "t"))
            items.append(Item(title: "Add MIDI Track", action: .addMIDITrack, key: "T"))
            items.append(.separator)
            if grouped {
                items.append(Item(title: "Remove from Group", action: .leaveGroup))
            } else {
                items.append(Item(title: "Group Tracks", action: .group, key: "g"))
            }
        case .group:
            items.append(Item(title: "Add Track", action: .addTrack, key: "t"))
            items.append(Item(title: "Add MIDI Track", action: .addMIDITrack, key: "T"))
            items.append(.separator)
            items.append(Item(title: "Ungroup", action: .ungroup, key: "G"))
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
    /// whether the start position is inside an audio clip of the selection,
    /// and `canJoin` whether two or more clips of one track and kind are
    /// selected.
    public static func clip(editor: String, canSplit: Bool, canJoin: Bool) -> [Item<ClipAction>] {
        [
            Item(title: "Edit \(editor)", action: .edit),
            .separator,
            Item(title: "Cut", action: .cut, key: "x"),
            Item(title: "Copy", action: .copy, key: "c"),
            Item(title: "Duplicate", action: .duplicate, key: "d"),
            Item(title: "Split at Start Position", action: .split, enabled: canSplit, key: "e"),
            Item(title: "Join", action: .join, enabled: canJoin, key: "j"),
            .separator,
            Item(title: "Loop Selection", action: .loop, key: "l", command: false),
            .separator,
            Item(title: "Delete", action: .delete, key: "\u{8}", command: false),
        ]
    }

    /// The marker strip's menu. On a marker: Rename, which lets the person
    /// type what it says, and Delete; in the clear: Add Marker Here. Delete
    /// All Markers either way, while the song has any (`count`).
    public static func marker(onMarker: Bool, count: Int) -> [Item<MarkerAction>] {
        var items: [Item<MarkerAction>] = []
        if onMarker {
            items.append(Item(title: "Rename", action: .rename, key: "r"))
            items.append(Item(title: "Delete", action: .delete, key: "\u{8}", command: false))
        } else {
            items.append(Item(title: "Add Marker Here", action: .add))
        }
        items.append(.separator)
        items.append(Item(title: "Delete All Markers", action: .deleteAll, enabled: count > 0))
        return items
    }

    /// An effect's menu, on its title in the device panel: what the Edit menu
    /// does to it, Paste after it when an effect was copied, its bypass and
    /// Delete. `copied` is the kind of the effect on the clipboard, if any.
    public static func effect(bypassed: Bool, copied: String?) -> [Item<EffectAction>] {
        [
            Item(title: "Cut", action: .cut, key: "x"),
            Item(title: "Copy", action: .copy, key: "c"),
            Item(title: copied.map { "Paste \($0) After" } ?? "Paste After", action: .paste, enabled: copied != nil, key: "v"),
            Item(title: "Duplicate", action: .duplicate, key: "d"),
            .separator,
            Item(title: bypassed ? "Enable" : "Bypass", action: .bypass),
            .separator,
            Item(title: "Delete", action: .delete, key: "\u{8}", command: false),
        ]
    }
}
