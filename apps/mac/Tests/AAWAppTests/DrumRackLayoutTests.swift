import XCTest
@testable import AAWApp

final class DrumRackLayoutTests: XCTestCase {
    func testTheGridShowsSixteenKeysTheLowestAtTheBottomLeft() {
        let l = DrumRackLayout()
        XCTAssertEqual(l.bank, 36)
        XCTAssertEqual(l.grid, [[48, 49, 50, 51], [44, 45, 46, 47], [40, 41, 42, 43], [36, 37, 38, 39]])
        XCTAssertEqual(l.key(row: 3, column: 0), 36, "the kick at the bottom left")
        XCTAssertEqual(l.key(row: 0, column: 3), 51)
    }

    func testABankIsARowOfTheOverviewAndStaysInsideIt() {
        XCTAssertEqual(DrumRackLayout(bank: 38).bank, 36)
        XCTAssertEqual(DrumRackLayout(bank: -5).bank, 0)
        XCTAssertEqual(DrumRackLayout(bank: 127).bank, 112)
        XCTAssertEqual(DrumRackLayout(bank: 112).grid[0], [124, 125, 126, 127])
    }

    func testItOpensWhereThePadsAre() {
        XCTAssertEqual(DrumRackLayout.opening(keys: []), 36, "an empty Drum Rack")
        XCTAssertEqual(DrumRackLayout.opening(keys: [36, 38, 42]), 36)
        XCTAssertEqual(DrumRackLayout.opening(keys: [20, 40]), 36, "a pad in the default bank keeps it")
        XCTAssertEqual(DrumRackLayout.opening(keys: [61, 70]), 60, "the row of the lowest pad")
        XCTAssertEqual(DrumRackLayout.opening(keys: [125]), 112)
    }

    func testItMovesAsLittleAsItMustToShowAKey() {
        let l = DrumRackLayout()
        XCTAssertEqual(l.showing(40), 36)
        XCTAssertEqual(l.showing(53), 40, "a key above comes in at the top row")
        XCTAssertEqual(l.showing(30), 28, "a key below comes in at the bottom row")
    }

    func testTheOverviewHasACellAKeyAndAFrameOnTheGrid() {
        let size = CGSize(width: 40, height: 320)
        XCTAssertEqual(DrumRackLayout.cell(0, in: size), CGRect(x: 0, y: 310, width: 10, height: 10))
        XCTAssertEqual(DrumRackLayout.cell(127, in: size), CGRect(x: 30, y: 0, width: 10, height: 10))
        XCTAssertEqual(DrumRackLayout(bank: 36).frame(in: size), CGRect(x: 0, y: 190, width: 40, height: 40))
        // A click centers the frame on the row under it, as far as it goes.
        XCTAssertEqual(DrumRackLayout.bank(atY: 215, height: 320), 36)
        XCTAssertEqual(DrumRackLayout.bank(atY: 2, height: 320), 112)
        XCTAssertEqual(DrumRackLayout.bank(atY: 319, height: 320), 0)
    }
}
