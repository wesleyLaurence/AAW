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
            .rename, nil, .mute, .solo, .automation, nil, .addTrack, .addMIDITrack, nil, .delete,
        ])
        XCTAssertEqual(titles(items), [
            "Rename", "Mute", "Solo", "Show Automation", "Add Track", "Add MIDI Track", "Delete",
        ])
        XCTAssertTrue(items.allSatisfy(\.enabled))
        // The keys the main menu has for the same things, shown beside them.
        XCTAssertEqual(items.first { $0.action == .rename }?.key, "r")
        XCTAssertEqual(items.first { $0.action == .delete }?.key, "\u{8}")
        XCTAssertEqual(items.first { $0.action == .delete }?.command, false)
    }

    func testTheTitlesFollowWhatTheRowIsSetTo() {
        let items = ContextMenu.row(kind: .track, muted: true, soloed: true, lanesShown: true)
        XCTAssertEqual(titles(items), [
            "Rename", "Unmute", "Unsolo", "Hide Automation", "Add Track", "Add MIDI Track", "Delete",
        ])
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

    func testAClipsMenuNamesItsEditorAndSplitsOnlyWhereItCan() {
        let items = ContextMenu.clip(editor: "Notes", canSplit: false)
        XCTAssertEqual(actions(items), [
            .edit, nil, .cut, .copy, .duplicate, .split, nil, .loop, nil, .delete,
        ])
        XCTAssertEqual(items.first?.title, "Edit Notes")
        XCTAssertEqual(items.first { $0.action == .split }?.enabled, false)
        XCTAssertEqual(ContextMenu.clip(editor: "Audio Clip", canSplit: true).first { $0.action == .split }?.enabled, true)
        XCTAssertEqual(ContextMenu.clip(editor: "Pattern", canSplit: false).first?.title, "Edit Pattern")
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
}
