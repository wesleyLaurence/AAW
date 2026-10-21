import AAWCore
import XCTest
@testable import AAWApp

final class DeviceChainTests: XCTestCase {
    func testAStripSaysWhereADraggedEffectGoesAndTheTwoBesideItRefuseIt() {
        // Four effects have five strips; strip i sits before effect i, and
        // the index given is where the effect ends up.
        XCTAssertEqual(DeviceChain.moveTarget(from: 2, strip: 0), 0)
        XCTAssertEqual(DeviceChain.moveTarget(from: 2, strip: 1), 1)
        XCTAssertNil(DeviceChain.moveTarget(from: 2, strip: 2), "the strip before it leaves it where it is")
        XCTAssertNil(DeviceChain.moveTarget(from: 2, strip: 3), "and so does the one after it")
        XCTAssertEqual(DeviceChain.moveTarget(from: 2, strip: 4), 3, "after the last: the fourth moves up")
        XCTAssertEqual(DeviceChain.moveTarget(from: 0, strip: 4), 3)
        XCTAssertEqual(DeviceChain.moveTarget(from: 3, strip: 0), 0)
        XCTAssertNil(DeviceChain.moveTarget(from: 0, strip: 1), "one effect: neither strip takes it")
    }

    private func effect(_ key: UInt64, kind: String = "compressor", effects: [EffectView] = []) -> EffectView {
        EffectView(key: key, kind: kind, id: nil, bypass: false, fields: [], bands: 0, name: nil, effects: effects)
    }

    func testAnEffectIsFoundAndPlacedThroughTheRackItIsIn() {
        // A chain of a compressor, a rack of two, and a limiter.
        let chain = [effect(1), effect(2, kind: "rack", effects: [effect(21, kind: "filter"), effect(22, kind: "eq")]), effect(3, kind: "limiter")]
        XCTAssertEqual(DeviceChain.find(22, in: chain)?.kind, "eq")
        XCTAssertEqual(DeviceChain.find(2, in: chain)?.kind, "rack")
        XCTAssertNil(DeviceChain.find(9, in: chain))
        XCTAssertEqual(DeviceChain.place(of: 3, in: chain)?.index, 2)
        XCTAssertNil(DeviceChain.place(of: 3, in: chain)?.rack)
        XCTAssertEqual(DeviceChain.place(of: 21, in: chain)?.rack, 2)
        XCTAssertEqual(DeviceChain.place(of: 21, in: chain)?.index, 0)
        XCTAssertNil(DeviceChain.place(of: 9, in: chain))
        // The chain's effects cross into the rack and the rack's out, but
        // not a rack into a rack, and not within one list.
        XCTAssertTrue(DeviceChain.crosses(1, in: chain, to: 2))
        XCTAssertTrue(DeviceChain.crosses(22, in: chain, to: nil))
        XCTAssertFalse(DeviceChain.crosses(2, in: chain, to: 2), "a rack holds no rack")
        XCTAssertFalse(DeviceChain.crosses(1, in: chain, to: nil), "a move within the chain is the strip's own")
        XCTAssertFalse(DeviceChain.crosses(21, in: chain, to: 2), "and within the rack too")
        XCTAssertFalse(DeviceChain.crosses(9, in: chain, to: 2))
    }

    @MainActor func testTheSynthPanelIsWiderByAStripBeforeEachEffectAndAfterTheLast() {
        let column = SynthPanel.column + SynthPanel.gap
        let strip = SynthPanel.strip + SynthPanel.gap
        XCTAssertEqual(SynthPanel.width(columns: 5, effects: 0), 5 * column + SynthPanel.gap, "no strips without effects")
        XCTAssertEqual(SynthPanel.width(columns: 5, effects: 1), 6 * column + 2 * strip + SynthPanel.gap)
        XCTAssertEqual(SynthPanel.width(columns: 5, effects: 3), 8 * column + 4 * strip + SynthPanel.gap)
    }
}
