import XCTest
@testable import AAWApp

final class ContextMenuTests: XCTestCase {
    private func actions<A: Equatable>(_ items: [ContextMenu.Item<A>]) -> [A?] {
        items.map(\.action)
    }

    private func titles<A: Equatable>(_ items: [ContextMenu.Item<A>]) -> [String] {
        items.compactMap { $0.action == nil ? nil : $0.title }
    }

    func testATracksMenuHasRenameTheSettingsTheAddsAndDelete() {
        let items = ContextMenu.row(kind: .track, muted: false, soloed: false, automationShown: false)
        XCTAssertEqual(actions(items), [
            .rename, nil, .mute, .solo, .freeze, .automation, nil, .addTrack, .addMIDITrack, nil, .group, nil, .delete,
        ])
        XCTAssertEqual(titles(items), [
            "Rename", "Mute", "Solo", "Freeze Track", "Show Automation", "Add Track", "Add MIDI Track", "Group Tracks", "Delete",
        ])
        // A frozen track offers to unfreeze; the Track menu's key is beside it.
        let frozen = ContextMenu.row(kind: .track, muted: false, soloed: false, automationShown: false, frozen: true)
        XCTAssertEqual(frozen.first { $0.action == .freeze }?.title, "Unfreeze Track")
        XCTAssertEqual(frozen.first { $0.action == .freeze }?.key, "F")
        XCTAssertNil(ContextMenu.row(kind: .group, muted: false, soloed: false, automationShown: false).first { $0.action == .freeze })
        XCTAssertEqual(items.first { $0.action == .group }?.key, "g")
        XCTAssertTrue(items.allSatisfy(\.enabled))
        // The keys the main menu has for the same things, shown beside them.
        XCTAssertEqual(items.first { $0.action == .rename }?.key, "r")
        XCTAssertEqual(items.first { $0.action == .delete }?.key, "\u{8}")
        XCTAssertEqual(items.first { $0.action == .delete }?.command, false)
    }

    /// An audio track's menu has an Input submenu: No Input, each input
    /// device mono and stereo where it has two channels, the song's other
    /// tracks, its groups and the master, the choice in force checked; a
    /// group's menu has none, and a track given no submenu, a MIDI track,
    /// has none either.
    func testATracksInputSubmenuOffersDevicesTracksGroupsAndTheMaster() {
        let devices: [(name: String?, channels: UInt16)] = [(nil, 2), ("MacBook Pro Microphone", 1), ("Scarlett 2i2", 2)]
        let tracks: [(key: UInt64, name: String)] = [(7, "bass")]
        let groups: [(key: UInt64, name: String)] = [(9, "drums")]
        let inputs = ContextMenu.inputs(current: .track(key: 7), devices: devices, tracks: tracks, groups: groups)
        XCTAssertEqual(titles(inputs), [
            "No Input", "Input Device · 1", "Input Device · 1+2", "MacBook Pro Microphone", "Scarlett 2i2 · 1", "Scarlett 2i2 · 1+2",
            "Track bass", "Group drums", "Master",
        ])
        XCTAssertEqual(actions(inputs).compactMap { $0 }, [
            .input(nil), .input(.device(name: nil, channels: [1])), .input(.device(name: nil, channels: [1, 2])),
            .input(.device(name: "MacBook Pro Microphone", channels: [1])),
            .input(.device(name: "Scarlett 2i2", channels: [1])), .input(.device(name: "Scarlett 2i2", channels: [1, 2])),
            .input(.track(key: 7)), .input(.group(key: 9)), .input(.master),
        ])
        XCTAssertEqual(inputs.filter(\.checked).map(\.title), ["Track bass"])
        XCTAssertEqual(ContextMenu.inputs(current: nil, devices: [], tracks: [], groups: []).filter(\.checked).map(\.title), ["No Input"])
        // The submenu sits after Solo in a track's menu.
        let items = ContextMenu.row(kind: .track, muted: false, soloed: false, automationShown: false, inputs: inputs)
        XCTAssertEqual(titles(items), [
            "Rename", "Mute", "Solo", "Input", "Freeze Track", "Show Automation", "Add Track", "Add MIDI Track", "Group Tracks", "Delete",
        ])
        XCTAssertEqual(items.first { $0.title == "Input" }?.children, inputs)
        XCTAssertNil(ContextMenu.row(kind: .group, muted: false, soloed: false, automationShown: false, inputs: inputs).first { $0.title == "Input" })
        XCTAssertNil(ContextMenu.row(kind: .track, muted: false, soloed: false, automationShown: false).first { $0.title == "Input" })
    }

    func testTheTitlesFollowWhatTheRowIsSetTo() {
        let items = ContextMenu.row(kind: .track, muted: true, soloed: true, automationShown: true)
        XCTAssertEqual(titles(items), [
            "Rename", "Unmute", "Unsolo", "Freeze Track", "Hide Automation", "Add Track", "Add MIDI Track", "Group Tracks", "Delete",
        ])
    }

    func testAGroupedTrackLeavesItsGroupAndAGroupUngroups() {
        let grouped = ContextMenu.row(kind: .track, muted: false, soloed: false, automationShown: false, grouped: true)
        XCTAssertEqual(actions(grouped), [
            .rename, nil, .mute, .solo, .freeze, .automation, nil, .addTrack, .addMIDITrack, nil, .leaveGroup, nil, .delete,
        ])
        XCTAssertEqual(grouped.first { $0.action == .leaveGroup }?.title, "Remove from Group")
        let group = ContextMenu.row(kind: .group, muted: false, soloed: true, automationShown: false)
        XCTAssertEqual(actions(group), [
            .rename, nil, .mute, .solo, .automation, nil, .addTrack, .addMIDITrack, nil, .ungroup, nil, .delete,
        ])
        XCTAssertEqual(titles(group), [
            "Rename", "Mute", "Unsolo", "Show Automation", "Add Track", "Add MIDI Track", "Ungroup", "Delete",
        ])
        XCTAssertEqual(group.first { $0.action == .ungroup }?.key, "G")
        // A group in a group also leaves it.
        let nested = ContextMenu.row(kind: .group, muted: false, soloed: false, automationShown: false, grouped: true)
        XCTAssertEqual(actions(nested), [
            .rename, nil, .mute, .solo, .automation, nil, .addTrack, .addMIDITrack, nil, .leaveGroup, .ungroup, nil, .delete,
        ])
        XCTAssertEqual(nested.first { $0.action == .leaveGroup }?.title, "Remove from Group")
    }

    func testMoveToGroupListsTheGroupsSetInWithNewGroup() {
        let groups = ContextMenu.groups([(key: 7, name: "music", depth: 0), (key: 8, name: "synths", depth: 1)], current: 8)
        XCTAssertEqual(actions(groups), [.moveToGroup(7), .moveToGroup(8), nil, .group])
        XCTAssertEqual(titles(groups), ["music", "synths", "New Group"])
        XCTAssertEqual(groups.map(\.indent), [0, 1, 0, 0])
        // The group it is in is checked and not chosen again.
        XCTAssertEqual(groups.map(\.checked), [false, true, false, false])
        XCTAssertEqual(groups[1].enabled, false)
        let track = ContextMenu.row(kind: .track, muted: false, soloed: false, automationShown: false, grouped: true, groups: groups)
        XCTAssertEqual(titles(track), [
            "Rename", "Mute", "Solo", "Freeze Track", "Show Automation", "Add Track", "Add MIDI Track", "Move to Group",
            "Remove from Group", "Delete",
        ])
        XCTAssertEqual(track.first { $0.title == "Move to Group" }?.children, groups)
        let group = ContextMenu.row(kind: .group, muted: false, soloed: false, automationShown: false, groups: groups)
        XCTAssertEqual(titles(group), [
            "Rename", "Mute", "Solo", "Show Automation", "Add Track", "Add MIDI Track", "Move to Group", "Ungroup", "Delete",
        ])
    }

    func testAReturnHasNoSoloAndAddsAReturn() {
        let items = ContextMenu.row(kind: .bus, muted: false, soloed: false, automationShown: false)
        XCTAssertEqual(actions(items), [
            .rename, nil, .mute, .automation, nil, .addTrack, .addMIDITrack, .addReturn, nil, .delete,
        ])
    }

    func testTheMasterIsNeitherRenamedNorMutedNorDeleted() {
        let items = ContextMenu.row(kind: .master, muted: false, soloed: false, automationShown: false)
        XCTAssertEqual(actions(items), [.automation, nil, .addTrack, .addMIDITrack, .addReturn])
        XCTAssertFalse(items.contains { $0.action == .rename || $0.action == .mute || $0.action == .delete })
    }

    func testAClipsMenuNamesItsEditorAndSplitsAndJoinsOnlyWhereItCan() {
        let items = ContextMenu.clip(editor: "Notes", canSplit: false, canJoin: false)
        XCTAssertEqual(actions(items), [
            .edit, nil, .cut, .copy, .duplicate, .split, .join, .activate, nil, .loop, nil, .delete,
        ])
        XCTAssertEqual(items.first?.title, "Edit Notes")
        XCTAssertEqual(items.first { $0.action == .split }?.enabled, false)
        XCTAssertEqual(items.first { $0.action == .join }?.enabled, false)
        XCTAssertEqual(items.first { $0.action == .join }?.key, "j")
        let several = ContextMenu.clip(editor: "Audio Clip", canSplit: true, canJoin: true)
        XCTAssertEqual(several.first { $0.action == .split }?.enabled, true)
        XCTAssertEqual(several.first { $0.action == .join }?.enabled, true)
        XCTAssertEqual(ContextMenu.clip(editor: "Audio Clip", canSplit: false, canJoin: false).first?.title, "Edit Audio Clip")
        XCTAssertEqual(items.first { $0.action == .loop }?.key, "l")
        XCTAssertEqual(items.first { $0.action == .loop }?.command, false)
    }

    func testAClipsMenuDeactivatesOrActivatesWithZero() {
        let one = ContextMenu.clip(editor: "Notes", canSplit: false, canJoin: false)
        XCTAssertEqual(one.first { $0.action == .activate }?.title, "Deactivate Clip")
        XCTAssertEqual(one.first { $0.action == .activate }?.key, "0")
        XCTAssertEqual(one.first { $0.action == .activate }?.command, false)
        let off = ContextMenu.clip(editor: "Audio Clip", canSplit: false, canJoin: true, activates: true, many: true)
        XCTAssertEqual(off.first { $0.action == .activate }?.title, "Activate Clips")
    }

    /// Color ▸ on a header and on a clip: the item that takes the own color
    /// away, then a swatch of each color of the palette in its order, the
    /// one everything colored has checked, none checked for a mix; the
    /// master has none.
    func testColorOffersTheNoneItemAndASwatchOfEachColor() {
        let palette: [(name: String, rgb: UInt32)] = [("red", 0xe5534b), ("white", 0xf2f2f2)]
        let colors = ContextMenu.colors(palette: palette, chosen: ["white", "white"], none: "Automatic") { ContextMenu.RowAction.color($0) }
        XCTAssertEqual(actions(colors), [.color(nil), .color("red"), .color("white")])
        XCTAssertEqual(colors.map(\.title), ["Automatic", "red", "white"])
        XCTAssertEqual(colors.map(\.swatch), [nil, 0xe5534b, 0xf2f2f2])
        XCTAssertEqual(colors.map(\.checked), [false, false, true])
        let mixed = ContextMenu.colors(palette: palette, chosen: ["white", nil], none: "Automatic") { ContextMenu.RowAction.color($0) }
        XCTAssertFalse(mixed.contains(where: \.checked))
        let none = ContextMenu.colors(palette: palette, chosen: [nil], none: "Track Color") { ContextMenu.ClipAction.color($0) }
        XCTAssertEqual(none.map(\.checked), [true, false, false])

        let row = ContextMenu.row(kind: .track, muted: false, soloed: false, automationShown: false, colors: colors)
        XCTAssertEqual(Array(actions(row).suffix(3)), [nil, .color(nil), .delete])
        XCTAssertEqual(row.first { $0.title == "Color" }?.children, colors)
        let master = ContextMenu.row(kind: .master, muted: false, soloed: false, automationShown: false, colors: colors)
        XCTAssertFalse(master.contains { $0.title == "Color" })
        let clip = ContextMenu.clip(editor: "Notes", canSplit: false, canJoin: false, colors: none)
        XCTAssertEqual(Array(actions(clip).suffix(2)), [.color(nil), .delete])
    }

    func testAnEffectsMenuPastesOnlyWhatWasCopiedAndNamesItsBypass() {
        let items = ContextMenu.effect(bypassed: false, copied: nil)
        XCTAssertEqual(actions(items), [.cut, .copy, .paste, .duplicate, nil, .bypass, .makeRack, nil, .delete])
        XCTAssertEqual(titles(items), ["Cut", "Copy", "Paste After", "Duplicate", "Bypass", "Make Rack", "Delete"])
        XCTAssertEqual(items.first { $0.action == .paste }?.enabled, false)
        XCTAssertEqual(items.first { $0.action == .duplicate }?.key, "d")
        XCTAssertEqual(items.first { $0.action == .delete }?.command, false)
        let copied = ContextMenu.effect(bypassed: true, copied: "Compressor")
        XCTAssertEqual(titles(copied), ["Cut", "Copy", "Paste Compressor After", "Duplicate", "Enable", "Make Rack", "Delete"])
        XCTAssertEqual(copied.first { $0.action == .paste }?.enabled, true)
        // A rack offers Unrack in its place; an effect inside a rack cannot be made one.
        let rack = ContextMenu.effect(bypassed: false, copied: nil, rack: true)
        XCTAssertEqual(actions(rack), [.cut, .copy, .paste, .duplicate, nil, .bypass, .unrack, nil, .delete])
        XCTAssertEqual(rack.first { $0.action == .unrack }?.title, "Unrack")
        let inside = ContextMenu.effect(bypassed: false, copied: nil, inRack: true)
        XCTAssertEqual(inside.first { $0.action == .makeRack }?.enabled, false)
    }

    func testTheMarkerStripsMenuActsOnAMarkerOrAddsOneInTheClear() {
        let on = ContextMenu.marker(onMarker: true, count: 3)
        XCTAssertEqual(on.map(\.action), [.rename, .delete, nil, .deleteAll])
        XCTAssertEqual(on.first { $0.action == .rename }?.key, "r")
        XCTAssertEqual(on.first { $0.action == .delete }?.command, false)
        let clear = ContextMenu.marker(onMarker: false, count: 0)
        XCTAssertEqual(clear.map(\.action), [.add, nil, .deleteAll])
        XCTAssertEqual(clear.first?.title, "Add Marker Here")
        // Nothing to delete in a song without markers.
        XCTAssertEqual(clear.last?.enabled, false)
        XCTAssertEqual(on.last?.enabled, true)
    }
}
