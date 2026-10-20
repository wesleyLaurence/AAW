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

    @MainActor func testTheSynthPanelIsWiderByAStripBeforeEachEffectAndAfterTheLast() {
        let column = SynthPanel.column + SynthPanel.gap
        let strip = SynthPanel.strip + SynthPanel.gap
        XCTAssertEqual(SynthPanel.width(columns: 5, effects: 0), 5 * column + SynthPanel.gap, "no strips without effects")
        XCTAssertEqual(SynthPanel.width(columns: 5, effects: 1), 6 * column + 2 * strip + SynthPanel.gap)
        XCTAssertEqual(SynthPanel.width(columns: 5, effects: 3), 8 * column + 4 * strip + SynthPanel.gap)
    }
}
