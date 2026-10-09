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
        let items = ContextMenu.row(kind: .track, muted: false, soloed: false, lanesShown: false)
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

    func testTheTitlesFollowWhatTheRowIsSetTo() {
        let items = ContextMenu.row(kind: .track, muted: true, soloed: true, lanesShown: true)
        XCTAssertEqual(titles(items), [
            "Rename", "Unmute", "Unsolo", "Hide Automation", "Add Track", "Add MIDI Track", "Group Tracks", "Delete",
        ])
    }

    func testAGroupedTrackLeavesItsGroupAndAGroupUngroups() {
        let grouped = ContextMenu.row(kind: .track, muted: false, soloed: false, lanesShown: false, grouped: true)
        XCTAssertEqual(actions(grouped), [
            .rename, nil, .mute, .solo, .automation, nil, .addTrack, .addMIDITrack, nil, .leaveGroup, nil, .delete,
        ])
        XCTAssertEqual(grouped.first { $0.action == .leaveGroup }?.title, "Remove from Group")
        let group = ContextMenu.row(kind: .group, muted: false, soloed: true, lanesShown: false)
        XCTAssertEqual(actions(group), [
            .rename, nil, .mute, .solo, .automation, nil, .addTrack, .addMIDITrack, nil, .ungroup, nil, .delete,
        ])
        XCTAssertEqual(titles(group), [
            "Rename", "Mute", "Unsolo", "Show Automation", "Add Track", "Add MIDI Track", "Ungroup", "Delete",
        ])
        XCTAssertEqual(group.first { $0.action == .ungroup }?.key, "G")
    }

    func testAReturnHasNoSoloAndAddsAReturn() {
        let items = ContextMenu.row(kind: .bus, muted: false, soloed: false, lanesShown: false)
        XCTAssertEqual(actions(items), [
            .rename, nil, .mute, .automation, nil, .addTrack, .addMIDITrack, .addReturn, nil, .delete,
        ])
    }

    func testTheMasterIsNeitherRenamedNorMutedNorDeleted() {
        let items = ContextMenu.row(kind: .master, muted: false, soloed: false, lanesShown: false)
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
