import XCTest
@testable import AAWApp

/// Where a header dragged among the tracks and groups lands.
final class RowDropTests: XCTestCase {
    // drums, bass in kit; keys, pad in synths, which is in music; vox alone.
    // Drawn: kit, drums, bass, music, synths, keys, pad, vox.
    private let kit: UInt64 = 100, music: UInt64 = 101, synths: UInt64 = 102
    private let drums: UInt64 = 1, bass: UInt64 = 2, keys: UInt64 = 3, pad: UInt64 = 4, vox: UInt64 = 5

    private func song(folded: Set<UInt64> = []) -> RowDrop {
        RowDrop(tracks: [(drums, kit), (bass, kit), (keys, synths), (pad, synths), (vox, nil)],
                groups: [(kit, nil), (music, nil), (synths, music)], folded: folded)
    }

    func testTheRowsAreListedAsTheArrangementDrawsThem() {
        XCTAssertEqual(song().entries.map(\.row), [
            .group(kit), .track(drums), .track(bass), .group(music), .group(synths), .track(keys), .track(pad), .track(vox),
        ])
        XCTAssertEqual(song().entries.map(\.depth), [0, 1, 1, 0, 1, 2, 2, 0])
        // Folded, a group hides what is under it.
        XCTAssertEqual(song(folded: [music]).entries.map(\.row), [.group(kit), .track(drums), .track(bass), .group(music), .track(vox)])
    }

    func testATrackDroppedAmongAGroupsTracksJoinsIt() {
        let drop = song()
        // Between drums and bass: in the kit, whatever the pointer's x.
        for depth in [0, 1, 5] {
            XCTAssertEqual(drop.landing(of: .track(vox), slot: 2, depth: depth), RowDrop.Landing(index: 1, group: kit, depth: 1, slot: 2))
        }
        // Right under the kit's header: in it, before its tracks.
        XCTAssertEqual(drop.landing(of: .track(vox), slot: 1, depth: 0), RowDrop.Landing(index: 0, group: kit, depth: 1, slot: 1))
        // Above the kit's header: on the master, first.
        XCTAssertEqual(drop.landing(of: .track(vox), slot: 0, depth: 3), RowDrop.Landing(index: 0, group: nil, depth: 0, slot: 0))
        // Between two tracks of a group in a group: in the inner one.
        XCTAssertEqual(drop.landing(of: .track(vox), slot: 6, depth: 0)?.group, synths)
    }

    func testAfterAGroupsLastRowThePointersXChooses() {
        let drop = song()
        // After bass, before music's header: the kit set in, the master at the edge.
        XCTAssertEqual(drop.landing(of: .track(vox), slot: 3, depth: 4), RowDrop.Landing(index: 2, group: kit, depth: 1, slot: 3))
        XCTAssertEqual(drop.landing(of: .track(vox), slot: 3, depth: 0), RowDrop.Landing(index: 2, group: nil, depth: 0, slot: 3))
        // After pad, before vox: synths, music or the master.
        XCTAssertEqual(drop.landing(of: .track(vox), slot: 7, depth: 2)?.group, synths)
        XCTAssertEqual(drop.landing(of: .track(vox), slot: 7, depth: 1)?.group, music)
        XCTAssertEqual(drop.landing(of: .track(vox), slot: 7, depth: 0)?.group, nil)
        // At the very end, after vox on the master: the master only.
        XCTAssertEqual(drop.landing(of: .track(drums), slot: 8, depth: 4), RowDrop.Landing(index: 4, group: nil, depth: 0, slot: 8))
    }

    func testADroppedTrackOutOfItsGroupAndBackWhereItWas() {
        let drop = song()
        // Bass dragged to the edge just under itself leaves the kit in place.
        let out = drop.landing(of: .track(bass), slot: 3, depth: 0)
        XCTAssertEqual(out, RowDrop.Landing(index: 1, group: nil, depth: 0, slot: 3))
        XCTAssertFalse(drop.isStill(.track(bass), out!))
        // Set in, it is where it was.
        XCTAssertTrue(drop.isStill(.track(bass), drop.landing(of: .track(bass), slot: 3, depth: 1)!))
        XCTAssertTrue(drop.isStill(.track(bass), drop.landing(of: .track(bass), slot: 2, depth: 1)!))
    }

    func testAGroupDraggedTakesItsRowsAndNestsButNotInItself() {
        let drop = song()
        // The kit dropped between keys and pad: into synths, with its tracks.
        XCTAssertEqual(drop.landing(of: .group(kit), slot: 6, depth: 0), RowDrop.Landing(index: 1, group: synths, depth: 2, slot: 6))
        // Dropped after vox, at the end on the master.
        XCTAssertEqual(drop.landing(of: .group(kit), slot: 8, depth: 0), RowDrop.Landing(index: 3, group: nil, depth: 0, slot: 8))
        // Music among its own rows goes nowhere; just under itself, nowhere new.
        XCTAssertNil(drop.landing(of: .group(music), slot: 5, depth: 0))
        XCTAssertNil(drop.landing(of: .group(music), slot: 6, depth: 0))
        let under = drop.landing(of: .group(music), slot: 7, depth: 9)
        XCTAssertNotEqual(under?.group, synths)
        XCTAssertTrue(drop.isStill(.group(music), drop.landing(of: .group(music), slot: 3, depth: 0)!))
        // Synths dragged out of music to the edge after the kit.
        XCTAssertEqual(drop.landing(of: .group(synths), slot: 3, depth: 0), RowDrop.Landing(index: 2, group: nil, depth: 0, slot: 3))
    }

    func testAFoldedGroupIsJoinedAtItsEndSetIn() {
        let drop = song(folded: [kit])
        // Drawn: kit, music, synths, keys, pad, vox. Under the folded kit, set
        // in, after its last track; at the edge, on the master after it.
        XCTAssertEqual(drop.landing(of: .track(vox), slot: 1, depth: 1), RowDrop.Landing(index: 2, group: kit, depth: 1, slot: 1))
        XCTAssertEqual(drop.landing(of: .track(vox), slot: 1, depth: 0), RowDrop.Landing(index: 2, group: nil, depth: 0, slot: 1))
    }

    func testAGroupWithNoTracksIsJoinedAfterTheTracksOverIt() {
        // fx is empty and in music: drawn after every track.
        let fx: UInt64 = 103
        let drop = RowDrop(tracks: [(keys, music), (vox, nil)], groups: [(music, nil), (fx, music)], folded: [])
        XCTAssertEqual(drop.entries.map(\.row), [.group(music), .track(keys), .track(vox), .group(fx)])
        // Into fx: after music's last track, so music's tracks stay together.
        XCTAssertEqual(drop.landing(of: .track(vox), slot: 4, depth: 9), RowDrop.Landing(index: 1, group: fx, depth: 2, slot: 4))
    }

    func testThePointerChoosesTheSlotAndTheDepth() {
        XCTAssertEqual(RowDrop.slot(at: 50, middles: [20, 45, 70]), 2)
        XCTAssertEqual(RowDrop.slot(at: 0, middles: [20, 45, 70]), 0)
        XCTAssertEqual(RowDrop.depth(atX: 8), 0)
        XCTAssertEqual(RowDrop.depth(atX: 24), 1)
        XCTAssertEqual(RowDrop.depth(atX: 120), 7)
    }

    func testARefusalLosesTheAdviceForTheAgent() {
        XCTAssertEqual(SongModel.forPerson("dub: the tracks of group group-1 must be next to each other; move it beside them with daw track move"),
                       "dub: the tracks of group group-1 must be next to each other")
        XCTAssertEqual(SongModel.forPerson("Nothing to group"), "Nothing to group")
    }
}
