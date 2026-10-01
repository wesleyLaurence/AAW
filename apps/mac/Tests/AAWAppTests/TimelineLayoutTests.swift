import XCTest
@testable import AAWApp

final class TimelineLayoutTests: XCTestCase {
    /// A 64-beat song in lanes 680 points wide: 10 points a beat when fitted.
    private func layout() -> TimelineLayout {
        var l = TimelineLayout()
        l.lengthBeats = 64
        l.size = CGSize(width: TimelineLayout.headerWidth + 680, height: 400)
        l.contentHeight = 300
        l.fit()
        return l
    }

    func testFitShowsTheSongAndOneBarMore() {
        let l = layout()
        XCTAssertEqual(l.pixelsPerBeat, 10)
        XCTAssertEqual(l.x(0), TimelineLayout.headerWidth)
        XCTAssertEqual(l.x(68), l.size.width)
        XCTAssertEqual(l.beat(atX: l.x(12.5)), 12.5, accuracy: 1e-9)
    }

    func testTheGridCoarsensAsTheViewZoomsOut() {
        var l = layout()
        XCTAssertEqual(l.grid, 4, "at 10 points a beat, beats are too close: bars")
        l.pixelsPerBeat = 14
        XCTAssertEqual(l.grid, 1)
        l.pixelsPerBeat = 30
        XCTAssertEqual(l.grid, 0.5)
        l.pixelsPerBeat = 60
        XCTAssertEqual(l.grid, 0.25)
        l.pixelsPerBeat = 2
        XCTAssertEqual(l.grid, 8, "two bars")
        XCTAssertEqual(l.barLabelStep, 8, "a number every eight bars")
    }

    func testAClickSnapsToTheGridAndStaysInTheSong() {
        let l = layout()
        XCTAssertEqual(l.target(atX: l.x(17.9), free: false), 16)
        XCTAssertEqual(l.target(atX: l.x(18.1), free: false), 20)
        XCTAssertEqual(l.target(atX: l.x(17.9), free: true), 17.9, accuracy: 1e-9)
        // The start position has to be before the end of the song.
        XCTAssertEqual(l.target(atX: l.x(67), free: false), 60)
        XCTAssertEqual(l.target(atX: l.x(67), free: true), 63.999, accuracy: 1e-9)
        XCTAssertEqual(l.target(atX: 0, free: false), 0)
    }

    func testADragMakesALoopOnTheGrid() {
        let l = layout()
        let forward = l.loop(fromX: l.x(9), toX: l.x(22))
        XCTAssertEqual(forward?.start, 8)
        XCTAssertEqual(forward?.length, 16)
        // Backwards is the same loop; a tiny drag is one grid step.
        XCTAssertEqual(l.loop(fromX: l.x(22), toX: l.x(9))?.start, 8)
        XCTAssertEqual(l.loop(fromX: l.x(9), toX: l.x(9.5))?.length, 4)
        // Past the end, it stops at the end; wholly outside, there is none.
        XCTAssertEqual(l.loop(fromX: l.x(58), toX: l.x(80))?.length, 8)
        XCTAssertNil(l.loop(fromX: l.x(65), toX: l.x(67)))
    }

    func testZoomKeepsTheBeatUnderThePointer() {
        var l = layout()
        let x = l.x(32)
        l.zoom(by: 3, anchorX: x)
        XCTAssertEqual(l.pixelsPerBeat, 30)
        XCTAssertEqual(l.x(32), x, accuracy: 1e-6)
        // Zoom and scroll stay within the song.
        l.zoom(by: 1000, anchorX: x)
        XCTAssertEqual(l.pixelsPerBeat, TimelineLayout.maxPixelsPerBeat)
        l.zoom(by: 1e-6, anchorX: x)
        XCTAssertEqual(l.pixelsPerBeat, 5, "half the fitted zoom")
        XCTAssertEqual(l.scroll.x, 0)
        l.scroll = CGPoint(x: 1e6, y: 1e6)
        l.clamp()
        XCTAssertEqual(l.scroll, .zero, "everything fits, so nothing scrolls")
    }

    func testThePlayheadIsKeptInView() {
        var l = layout()
        l.pixelsPerBeat = 40
        l.reveal(10)
        XCTAssertEqual(l.scroll.x, 0, "already in view")
        l.reveal(30)
        XCTAssertEqual(l.x(30), TimelineLayout.headerWidth + 68, accuracy: 1e-6)
        l.reveal(2)
        XCTAssertEqual(l.scroll.x, 12, accuracy: 1e-6)
    }

    func testPositionsCountFromOne() {
        XCTAssertEqual(TimelineLayout.position(0, beatsPerBar: 4), "1.1.1")
        XCTAssertEqual(TimelineLayout.position(50, beatsPerBar: 4), "13.3.1")
        XCTAssertEqual(TimelineLayout.position(5.75, beatsPerBar: 4), "2.2.4")
        XCTAssertEqual(TimelineLayout.position(53.25, beatsPerBar: 4), "14.2.2")
    }
}
