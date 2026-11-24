import AAWCore
import AppKit
import XCTest
@testable import AAWApp

final class TimeSelectionTests: XCTestCase {
    /// Runs the main run loop until `done` or two seconds pass.
    @MainActor
    private func wait(_ done: () -> Bool) {
        let end = Date().addingTimeInterval(2)
        while !done(), Date() < end {
            RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        }
    }

    /// A group of two MIDI tracks over a third: the kick's clip covers bars
    /// 1 to 2, the hats' clips lie on beats 4 and 8 alone, and the keys play
    /// through.
    @MainActor
    private func open() throws -> (SongModel, URL) {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("aaw time \(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let song = folder.appendingPathComponent("song.yaml")
        try """
        session: {length_beats: 32, tempo: 120}
        tracks:
        - id: kick
          type: midi
          group: drums
          clips:
          - {id: k, length_beats: 8, notes: [{pitch: 36, duration: 1}, {pitch: 36, at: 4, duration: 1}]}
        - id: hats
          type: midi
          group: drums
          clips:
          - {id: h1, at: 4, length_beats: 1, notes: [{pitch: 42, duration: 1/2}]}
          - {id: h2, at: 8, length_beats: 1, notes: [{pitch: 42, duration: 1/2}]}
        - id: keys
          type: midi
          clips:
          - {id: p, length_beats: 32, notes: [{pitch: 60, duration: 32}]}
        groups:
        - id: drums

        """.write(to: song, atomically: true, encoding: .utf8)
        return (try SongModel(url: song), folder)
    }

    /// Bars 1 and 2 of a group selected: its tracks and the clips wholly
    /// inside on them; Delete, Copy, Duplicate, Loop and Export act on the
    /// beats; a clip pressed takes the selection's place.
    @MainActor
    func testABeatsSelectionOnAGroupCoversItsTracksAndTheClipsInside() throws {
        let (model, folder) = try open()
        defer {
            model.close()
            try? FileManager.default.removeItem(at: folder)
        }
        let a = model.arrangement
        let group = try XCTUnwrap(a.groups.first).key
        let (kick, hats, keys) = (a.tracks[0], a.tracks[1], a.tracks[2])
        model.select(time: TimeSelection(start: 0, length: 8, rows: [.group(group)]))
        XCTAssertEqual(model.tracks(of: try XCTUnwrap(model.timeSelection)), [kick.key, hats.key])
        XCTAssertEqual(model.selectedClips, [kick.noteClips[0].key, hats.noteClips[0].key], "h2 ends past the beats, the keys are not covered")
        XCTAssertTrue(model.canDelete)
        XCTAssertTrue(model.canDuplicate)
        XCTAssertTrue(model.canCopy(in: .clips))
        XCTAssertEqual(model.selectedSpan?.start, 0)
        XCTAssertEqual(model.selectedSpan?.end, 8)
        XCTAssertEqual(model.exportRange, ExportRange(start: 0, end: 8))
        // Every track, from the ruler.
        model.select(time: TimeSelection(start: 0, length: 4, rows: []))
        XCTAssertEqual(model.tracks(of: try XCTUnwrap(model.timeSelection)), [kick.key, hats.key, keys.key])
        // A clip selected, a row or nothing: the beats are no longer selected.
        model.select(clips: [keys.noteClips[0].key])
        XCTAssertNil(model.timeSelection)
        model.select(time: TimeSelection(start: 0, length: 4, rows: [.track(hats.key)]))
        model.select(clips: [])
        XCTAssertNil(model.timeSelection)
    }

    /// Copied, the beats are taken as they are, empty ones too; pasted at
    /// the start position they replace what is there on the same tracks,
    /// and the pasted beats are the selection.
    @MainActor
    func testCopiedBeatsArePastedOverTheirWholeLengthAndSelected() throws {
        let (model, folder) = try open()
        defer {
            model.close()
            try? FileManager.default.removeItem(at: folder)
        }
        let a = model.arrangement
        let group = try XCTUnwrap(a.groups.first).key
        let hats = a.tracks[1].key
        // Beats 0 to 4 of the drums: the kick's first bar, and nothing on the hats.
        model.select(time: TimeSelection(start: 0, length: 4, rows: [.group(group)]))
        model.copySelection(in: .clips)
        guard case .range(_, let length, let tracks)? = model.clipboard else { return XCTFail("a range was copied") }
        XCTAssertEqual(length, 4)
        XCTAssertEqual(tracks, [a.tracks[0].key, hats])
        XCTAssertTrue(model.canPaste(in: .clips))
        // At beat 4: the hats' clip there goes, since the copy is empty there.
        model.select(clips: [])
        model.locate(4)
        wait { model.transport.cue == 4 }
        model.paste(in: .clips)
        wait { model.arrangement.tracks[1].noteClips.count == 1 }
        let after = model.arrangement
        XCTAssertEqual(after.tracks[1].noteClips.map(\.at), [8], "the hats' clip at 4 was replaced by the copy's empty beats")
        XCTAssertEqual(after.tracks[0].noteClips.map(\.at).sorted(), [0, 4], "the kick's clip cut at 4 and its first bar again there")
        XCTAssertEqual(after.tracks[2].noteClips.count, 1, "the keys are not in the copy")
        wait { model.timeSelection != nil }
        XCTAssertEqual(model.timeSelection, TimeSelection(start: 4, length: 4, rows: [.track(after.tracks[0].key), .track(hats)]))

        // ⌘D: the selected beats again right after themselves, and the
        // selection moves on to them.
        model.duplicateSelection()
        wait { model.timeSelection?.start == 8 }
        XCTAssertEqual(model.timeSelection?.length, 4)
        wait { model.arrangement.tracks[1].noteClips.isEmpty }
        XCTAssertEqual(model.arrangement.tracks[1].noteClips.count, 0, "the hats' clip at 8 was replaced by empty beats")

        // Delete empties the beats and moves nothing.
        model.deleteSelection()
        wait { !model.arrangement.tracks[0].noteClips.contains { $0.at == 8 } }
        XCTAssertEqual(model.arrangement.tracks[0].noteClips.map(\.at).sorted(), [0, 4])
    }
}
