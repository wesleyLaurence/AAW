import XCTest
@testable import AAWApp

final class SamplerLayoutTests: XCTestCase {
    /// A two-second file across 200 points, played from 0.5 to 1.5 seconds.
    private func layout() -> SamplerLayout {
        SamplerLayout(seconds: 2, width: 200, start: 0.5, end: 1.5)
    }

    func testSecondsAndPointsMapAcrossTheWaveform() {
        let l = layout()
        XCTAssertEqual(l.pointsPerSecond, 100)
        XCTAssertEqual(l.x(0.5), 50)
        XCTAssertEqual(l.seconds(atX: 150), 1.5)
        XCTAssertEqual(l.seconds(atX: -20), 0, "no earlier than the file")
        XCTAssertEqual(l.seconds(atX: 900), 2, "no later than the file")
        // A file that cannot be read has no width to map.
        let none = SamplerLayout(seconds: 0, width: 200, start: 0, end: 0)
        XCTAssertEqual([none.pointsPerSecond, none.x(1)], [0, 0])
        XCTAssertEqual(none.seconds(atX: 50), 0)
    }

    func testAMarkerIsTakenHoldOfNearIt() {
        let l = layout()
        XCTAssertEqual(l.marker(atX: 52), .start)
        XCTAssertEqual(l.marker(atX: 44), .start, "within reach to the left")
        XCTAssertEqual(l.marker(atX: 146), .end)
        XCTAssertNil(l.marker(atX: 100), "between them")
        XCTAssertNil(l.marker(atX: 43), "out of reach")
        // Markers together: the start from the left, the end from the right.
        let tight = SamplerLayout(seconds: 2, width: 200, start: 1, end: 1)
        XCTAssertEqual(tight.marker(atX: 98), .start)
        XCTAssertEqual(tight.marker(atX: 102), .end)
    }

    func testADraggedMarkerStaysInTheFileAndBehindTheOther() {
        let l = layout()
        XCTAssertEqual(l.dragged(.start, toX: 20), 0.2)
        XCTAssertEqual(l.dragged(.start, toX: -50), 0, "to the file's start")
        XCTAssertEqual(l.dragged(.start, toX: 180), 1.499, "never past the end")
        XCTAssertEqual(l.dragged(.end, toX: 180), 1.8)
        XCTAssertEqual(l.dragged(.end, toX: 500), 2, "to the file's end")
        XCTAssertEqual(l.dragged(.end, toX: 10), 0.501, "never before the start")
        // To a thousandth of a second.
        XCTAssertEqual(l.dragged(.start, toX: 33.3333), 0.333)
        XCTAssertEqual(SamplerLayout.text(0.5), "0.5")
        XCTAssertEqual(SamplerLayout.text(2), "2")
        XCTAssertEqual(SamplerLayout.text(1.23456), "1.235")
    }
}
