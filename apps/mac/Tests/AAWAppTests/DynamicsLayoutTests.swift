@testable import AAWApp
import XCTest

final class DynamicsLayoutTests: XCTestCase {
    let box = CGRect(x: 10, y: 20, width: 200, height: 120)

    func testLevelsSitOnTheDisplaysScaleAndReductionHangsFromTheTop() {
        XCTAssertEqual(DynamicsLayout.y(db: 0, in: box), 20)
        XCTAssertEqual(DynamicsLayout.y(db: -60, in: box), 140)
        XCTAssertEqual(DynamicsLayout.y(db: -30, in: box), 80)
        // Past either end, clamped; silence at the floor.
        XCTAssertEqual(DynamicsLayout.y(db: 3, in: box), 20)
        XCTAssertEqual(DynamicsLayout.y(db: -200, in: box), 140)
        XCTAssertEqual(DynamicsLayout.db(y: 80, in: box), -30, accuracy: 1e-9)
        // A meter's scale reaches over full scale.
        XCTAssertEqual(DynamicsLayout.y(db: 0, top: DynamicsLayout.meterTopDb, floor: -60, in: box), 20 + 120 * 6 / 66, accuracy: 1e-9)
        // 6 dB taken off reaches a tenth down the display, and a quarter of a 24 dB meter.
        XCTAssertEqual(DynamicsLayout.depth(reductionDb: 6, in: box), 12, accuracy: 1e-9)
        XCTAssertEqual(DynamicsLayout.depth(reductionDb: 6, range: 24, in: box), 30, accuracy: 1e-9)
        XCTAssertEqual(DynamicsLayout.depth(reductionDb: 40, range: 24, in: box), 120)
        XCTAssertEqual(DynamicsLayout.depth(reductionDb: -1, range: 24, in: box), 0)
        XCTAssertEqual(DynamicsLayout.meterRange(kind: "compressor"), 24)
        XCTAssertEqual(DynamicsLayout.meterRange(kind: "limiter"), 24)
        XCTAssertEqual(DynamicsLayout.meterRange(kind: "clipper"), 36)
    }

    func testTheThresholdLineIsHeldNearItAndADragSetsWhereThePointerIs() {
        // The line at −24 dB is at 20 + 120 × 24/60 = 68.
        XCTAssertTrue(DynamicsLayout.grabs(y: 68, thresholdDb: -24, in: box))
        XCTAssertTrue(DynamicsLayout.grabs(y: 73, thresholdDb: -24, in: box))
        XCTAssertFalse(DynamicsLayout.grabs(y: 76, thresholdDb: -24, in: box))
        // A drag to a point sets the level there, to a tenth of a dB, within −60 to 0.
        XCTAssertEqual(DynamicsLayout.threshold(at: 68, in: box), -24)
        XCTAssertEqual(DynamicsLayout.threshold(at: 81, in: box), -30.5)
        XCTAssertEqual(DynamicsLayout.threshold(at: 80.9, in: box), -30.5)
        XCTAssertEqual(DynamicsLayout.threshold(at: 0, in: box), 0)
        XCTAssertEqual(DynamicsLayout.threshold(at: 500, in: box), -60)
    }

    func testHopsFoldIntoColumnsByTheirMost() {
        // Eight hops into four columns: each the louder of two.
        let hops: [Float] = [-30, -10, -40, -50, -20, -21, -60, -5]
        XCTAssertEqual(DynamicsLayout.columns(hops, width: 4, empty: -200), [-10, -40, -20, -5])
        // Fewer hops than columns: a hop spans several, the newest at the right.
        XCTAssertEqual(DynamicsLayout.columns([-1, -2], width: 4, empty: -200), [-1, -1, -2, -2])
        XCTAssertEqual(DynamicsLayout.columns([], width: 3, empty: 0), [0, 0, 0])
        XCTAssertEqual(DynamicsLayout.columns(hops, width: 0, empty: 0), [])
        // Four seconds at 48 kHz is 750 hops; at 96 kHz the ring's 1024.
        XCTAssertEqual(DynamicsLayout.hopsShown(sampleRate: 48000), 750)
        XCTAssertEqual(DynamicsLayout.hopsShown(sampleRate: 96000), 1024)
    }

    func testTheMeterHoldsTheMostUntilAClickOrAPlay() {
        var held = DynamicsLayout.Held()
        // The first reading counts nothing it did not see arrive.
        held.update(written: 100, reductions: [9, 9, 9], playing: true)
        XCTAssertEqual(held.most, 0)
        // Two new hops: only they count.
        held.update(written: 102, reductions: [9, 3, 4], playing: true)
        XCTAssertEqual(held.most, 4)
        held.update(written: 103, reductions: [3, 4, 2], playing: true)
        XCTAssertEqual(held.most, 4)
        // Stopped, it holds; played again from a stop, it starts again.
        held.update(written: 103, reductions: [3, 4, 2], playing: false)
        XCTAssertEqual(held.most, 4)
        held.update(written: 104, reductions: [4, 2, 1.5], playing: true)
        XCTAssertEqual(held.most, 1.5)
        // A click starts it again too.
        held.reset()
        XCTAssertEqual(held.most, 0)
        // A gauge made again counts from there.
        held.update(written: 5, reductions: [8, 8, 8], playing: true)
        XCTAssertEqual(held.most, 0)
        held.update(written: 6, reductions: [8, 8, 7], playing: true)
        XCTAssertEqual(held.most, 7)
    }

    func testTheBarFollowsTheLastHopAndFallsOnceThereAreNoMore() {
        XCTAssertEqual(DynamicsLayout.fallen(2, toward: 6, fresh: true, seconds: 0.1), 6)
        XCTAssertEqual(DynamicsLayout.fallen(6, toward: 6, fresh: false, seconds: 0.125), 3, accuracy: 1e-9)
        XCTAssertEqual(DynamicsLayout.fallen(1, toward: 6, fresh: false, seconds: 1), 0)
        XCTAssertEqual(DynamicsLayout.text(reductionDb: 0), "0.0")
        XCTAssertEqual(DynamicsLayout.text(reductionDb: 3.26), "−3.3")
        XCTAssertEqual(DynamicsLayout.text(reductionDb: -1), "0.0")
    }
}
