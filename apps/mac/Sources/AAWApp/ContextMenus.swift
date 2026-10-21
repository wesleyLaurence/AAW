import AAWCore
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
        /// Where a track records from, chosen in the Input submenu: a device
        /// and its channels, a track, a group, the mix, or nothing.
        case input(InputChoice?)
        /// Freeze Track on a track, Unfreeze Track on a frozen one.
        case freeze
    }

    /// One choice of an audio track's Input submenu.
    public struct InputOption: Equatable {
        public var title: String
        public var choice: InputChoice?

        public init(title: String, choice: InputChoice?) {
            self.title = title
            self.choice = choice
        }
    }

    /// What an item of a clip's menu does, to the selected clips.
    public enum ClipAction: Equatable {
        case edit, cut, copy, duplicate, split, join, loop, delete
    }

    /// What an item of an effect's menu does, to the effect that was clicked
    /// in the device panel.
    public enum EffectAction: Equatable {
        case cut, copy, paste, duplicate, bypass, makeRack, unrack, delete
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
        /// Shown with a check mark: the choice in force.
        public var checked = false
        /// A submenu's items, on an item that opens one instead of acting.
        public var children: [Item<Action>]? = nil

        public static var separator: Item { Item(title: "", action: nil) }
    }

    /// The Input submenu of an audio track: No Input, each input device
    /// mono and, where it has two channels, stereo, and the song's other
    /// tracks, its groups and the mix to resample; the choice in force
    /// checked. `devices` are the Mac's input devices with their channel
    /// counts, the chosen one of Settings › Audio first as "the input
    /// device"; `tracks` and `groups` the song's other rows by key and name.
    public static func inputs(current: InputChoice?, devices: [(name: String?, channels: UInt16)],
                              tracks: [(key: UInt64, name: String)], groups: [(key: UInt64, name: String)]) -> [Item<RowAction>] {
        var items: [Item<RowAction>] = [Item(title: "No Input", action: .input(nil), checked: current == nil)]
        items.append(.separator)
        for device in devices {
            let shown = device.name ?? "Input Device"
            let mono = InputChoice.device(name: device.name, channels: [1])
            items.append(Item(title: device.channels > 1 ? "\(shown) · 1" : shown, action: .input(mono), checked: current == mono))
            if device.channels > 1 {
                let stereo = InputChoice.device(name: device.name, channels: [1, 2])
                items.append(Item(title: "\(shown) · 1+2", action: .input(stereo), checked: current == stereo))
            }
        }
        if !tracks.isEmpty || !groups.isEmpty {
            items.append(.separator)
        }
        for track in tracks {
            let choice = InputChoice.track(key: track.key)
            items.append(Item(title: "Track \(track.name)", action: .input(choice), checked: current == choice))
        }
        for group in groups {
            let choice = InputChoice.group(key: group.key)
            items.append(Item(title: "Group \(group.name)", action: .input(choice), checked: current == choice))
        }
        items.append(.separator)
        items.append(Item(title: "Master", action: .input(.master), checked: current == .master))
        return items
    }

    /// A header's menu: Rename, what a row is set to, a row added after it,
    /// and Delete. The master has no name of its own, no mute mark and cannot
    /// go; a return has no solo. A track offers Group Tracks, or Remove from
    /// Group when it is in one (`grouped`); a group offers Ungroup, and
    /// Remove from Group when it is in a group. A track offers Freeze Track,
    /// or Unfreeze Track while it is frozen (`frozen`).
    public static func row(kind: HeaderLayout.Kind, muted: Bool, soloed: Bool, automationShown: Bool, grouped: Bool = false,
                           inputs: [Item<RowAction>]? = nil, frozen: Bool = false) -> [Item<RowAction>] {
        var items: [Item<RowAction>] = []
        if kind != .master {
            items.append(Item(title: "Rename", action: .rename, key: "r"))
            items.append(.separator)
            items.append(Item(title: muted ? "Unmute" : "Mute", action: .mute))
        }
        if kind == .track || kind == .group {
            items.append(Item(title: soloed ? "Unsolo" : "Solo", action: .solo))
        }
        // An audio track's input, as a submenu; a MIDI track records no audio.
        if kind == .track, let inputs {
            items.append(Item(title: "Input", action: .input(nil), children: inputs))
        }
        if kind == .track {
            items.append(Item(title: frozen ? "Unfreeze Track" : "Freeze Track", action: .freeze, key: "F"))
        }
        items.append(Item(title: automationShown ? "Hide Automation" : "Show Automation", action: .automation))
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
            if grouped {
                items.append(Item(title: "Remove from Group", action: .leaveGroup))
            }
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
    /// does to it, Paste after it when an effect was copied, its bypass,
    /// Make Rack for an effect of the chain or Unrack for a rack, and
    /// Delete. `copied` is the kind of the effect on the clipboard, if any;
    /// `rack` whether the effect is a rack, and `inRack` whether it is
    /// inside one, where Make Rack is not offered.
    public static func effect(bypassed: Bool, copied: String?, rack: Bool = false, inRack: Bool = false) -> [Item<EffectAction>] {
        [
            Item(title: "Cut", action: .cut, key: "x"),
            Item(title: "Copy", action: .copy, key: "c"),
            Item(title: copied.map { "Paste \($0) After" } ?? "Paste After", action: .paste, enabled: copied != nil, key: "v"),
            Item(title: "Duplicate", action: .duplicate, key: "d"),
            .separator,
            Item(title: bypassed ? "Enable" : "Bypass", action: .bypass),
            rack ? Item(title: "Unrack", action: .unrack) : Item(title: "Make Rack", action: .makeRack, enabled: !inRack),
            .separator,
            Item(title: "Delete", action: .delete, key: "\u{8}", command: false),
        ]
    }
}
