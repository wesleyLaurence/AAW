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
            .rename, nil, .mute, .solo, .automation, nil, .addTrack, .addMIDITrack, nil, .group, nil, .delete,
        ])
        XCTAssertEqual(titles(items), [
            "Rename", "Mute", "Solo", "Show Automation", "Add Track", "Add MIDI Track", "Group Tracks", "Delete",
        ])
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
            "Rename", "Mute", "Solo", "Input", "Show Automation", "Add Track", "Add MIDI Track", "Group Tracks", "Delete",
        ])
        XCTAssertEqual(items.first { $0.title == "Input" }?.children, inputs)
        XCTAssertNil(ContextMenu.row(kind: .group, muted: false, soloed: false, automationShown: false, inputs: inputs).first { $0.title == "Input" })
        XCTAssertNil(ContextMenu.row(kind: .track, muted: false, soloed: false, automationShown: false).first { $0.title == "Input" })
    }

    func testTheTitlesFollowWhatTheRowIsSetTo() {
        let items = ContextMenu.row(kind: .track, muted: true, soloed: true, automationShown: true)
        XCTAssertEqual(titles(items), [
            "Rename", "Unmute", "Unsolo", "Hide Automation", "Add Track", "Add MIDI Track", "Group Tracks", "Delete",
        ])
    }

    func testAGroupedTrackLeavesItsGroupAndAGroupUngroups() {
        let grouped = ContextMenu.row(kind: .track, muted: false, soloed: false, automationShown: false, grouped: true)
        XCTAssertEqual(actions(grouped), [
            .rename, nil, .mute, .solo, .automation, nil, .addTrack, .addMIDITrack, nil, .leaveGroup, nil, .delete,
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
            .edit, nil, .cut, .copy, .duplicate, .split, .join, nil, .loop, nil, .delete,
        ])
        XCTAssertEqual(items.first?.title, "Edit Notes")
        XCTAssertEqual(items.first { $0.action == .split }?.enabled, false)
        XCTAssertEqual(items.first { $0.action == .join }?.enabled, false)
        XCTAssertEqual(items.first { $0.action == .join }?.key, "j")
        let several = ContextMenu.clip(editor: "Audio Clip", canSplit: true, canJoin: true)
        XCTAssertEqual(several.first { $0.action == .split }?.enabled, true)
        XCTAssertEqual(several.first { $0.action == .join }?.enabled, true)
        XCTAssertEqual(ContextMenu.clip(editor: "Pattern", canSplit: false, canJoin: false).first?.title, "Edit Pattern")
        XCTAssertEqual(items.first { $0.action == .loop }?.key, "l")
        XCTAssertEqual(items.first { $0.action == .loop }?.command, false)
    }

    func testAnEffectsMenuPastesOnlyWhatWasCopiedAndNamesItsBypass() {
        let items = ContextMenu.effect(bypassed: false, copied: nil)
        XCTAssertEqual(actions(items), [.cut, .copy, .paste, .duplicate, nil, .bypass, nil, .delete])
        XCTAssertEqual(titles(items), ["Cut", "Copy", "Paste After", "Duplicate", "Bypass", "Delete"])
        XCTAssertEqual(items.first { $0.action == .paste }?.enabled, false)
        XCTAssertEqual(items.first { $0.action == .duplicate }?.key, "d")
        XCTAssertEqual(items.first { $0.action == .delete }?.command, false)
        let copied = ContextMenu.effect(bypassed: true, copied: "Compressor")
        XCTAssertEqual(titles(copied), ["Cut", "Copy", "Paste Compressor After", "Duplicate", "Enable", "Delete"])
        XCTAssertEqual(copied.first { $0.action == .paste }?.enabled, true)
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
