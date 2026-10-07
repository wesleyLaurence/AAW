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

    func testAChosenGridHoldsWhateverTheZoom() {
        var l = layout()
        XCTAssertEqual(l.zoomGrid, 4)
        l.fixedGrid = 1.0 / 3
        XCTAssertEqual(l.grid, 1.0 / 3, "an eighth-note triplet at 10 points a beat")
        XCTAssertEqual(l.target(atX: l.x(17.4), free: false), 17 + 1.0 / 3, accuracy: 1e-9)
        XCTAssertEqual(l.zoomGrid, 4, "the zoom's grid is still there to go back to")
        l.fixedGrid = nil
        XCTAssertEqual(l.grid, 4)
        l.fixedGrid = 0
        XCTAssertEqual(l.grid, 4, "nothing is snapped to a grid of nothing")
    }

    func testTheDrawnGridIsTheGridWithRoomOrTheZooms() {
        var l = layout()
        XCTAssertEqual(l.drawnGrid, 4, "following the zoom, as drawn")
        // Triplets at 22 points a beat: lines a third apart would be 7 points
        // apart, so every second, which still fall on the bars. At 10 points
        // only every eighth has room, and those miss the bars: the zoom's.
        l.fixedGrid = 1.0 / 3
        XCTAssertEqual(l.drawnGrid, 4)
        l.pixelsPerBeat = 22
        XCTAssertEqual(l.drawnGrid, 2.0 / 3, accuracy: 1e-9)
        l.pixelsPerBeat = 60
        XCTAssertEqual(l.drawnGrid, 1.0 / 3, accuracy: 1e-9, "room for every line")
        // Zoomed far out, no multiple of a triplet falls on the bars: the zoom's.
        l.pixelsPerBeat = 2
        XCTAssertEqual(l.drawnGrid, 8)
        // A sixty-fourth at 10 points a beat is drawn as beats.
        l.pixelsPerBeat = 10
        l.fixedGrid = 1.0 / 16
        XCTAssertEqual(l.drawnGrid, 2)
        XCTAssertEqual(l.grid, 1.0 / 16, "and still snapped to")
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

    func testANewClipStartsInTheGridStepUnderThePointer() {
        let l = layout()
        // At 10 points a beat the grid is bars: a click late in a bar is still in it.
        XCTAssertEqual(l.step(atX: l.x(19.9), free: false), 16)
        XCTAssertEqual(l.step(atX: l.x(20.1), free: false), 20)
        XCTAssertEqual(l.step(atX: l.x(19.9), free: true), 19.9, accuracy: 1e-9)
        XCTAssertEqual(l.step(atX: l.x(66), free: false), 60, "the last bar of the song")
        XCTAssertEqual(l.step(atX: 0, free: false), 0)
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
        // The beat is the one the time signature counts: a quarter in 3/4,
        // an eighth in 6/8 and 7/8.
        XCTAssertEqual(TimelineLayout.position(5, beatsPerBar: 3), "2.3.1")
        XCTAssertEqual(TimelineLayout.position(3.75, beatsPerBar: 3, beatUnit: 0.5), "2.2.2")
        XCTAssertEqual(TimelineLayout.position(7, beatsPerBar: 3.5, beatUnit: 0.5), "3.1.1")
        XCTAssertEqual(TimelineLayout.position(6.5, beatsPerBar: 3.5, beatUnit: 0.5), "2.7.1")
    }

    func testTheZoomsGridCountsTheMetersBeats() {
        var l = layout()
        l.beatsPerBar = 3
        l.beatUnit = 0.5
        l.pixelsPerBeat = 30
        XCTAssertEqual(l.zoomGrid, 0.5, "in 6/8 the beat is an eighth")
        l.pixelsPerBeat = 60
        XCTAssertEqual(l.zoomGrid, 0.25)
        l.pixelsPerBeat = 10
        XCTAssertEqual(l.zoomGrid, 3, "a bar of three beats")
        l.pixelsPerBeat = 3
        XCTAssertEqual(l.zoomGrid, 6, "two bars")
        l.beatsPerBar = 3.5
        l.pixelsPerBeat = 10
        XCTAssertEqual(l.zoomGrid, 3.5, "a bar of 7/8")
        l.fixedGrid = 1
        XCTAssertEqual(l.drawnGrid, 3.5, "a beat does not fall on the bars of 7/8, so the zoom's lines are drawn")
        l.fixedGrid = 0.5
        l.pixelsPerBeat = 40
        XCTAssertEqual(l.drawnGrid, 0.5)
    }

    func testPointsMoveTogetherNoFurtherThanThePointsThatStay() {
        let range = TimelineLayout.pointRange
        // One point: between its neighbors.
        XCTAssertEqual(range([[(0, false), (8, true), (16, false)]], 32), -8...8)
        // Two of a lane: the first's left neighbor and the second's right one.
        XCTAssertEqual(range([[(0, false), (4, true), (6, true), (10, false)]], 32), -4...4)
        // Lanes together: the tightest of each; the song's start and end.
        XCTAssertEqual(range([[(2, true), (20, true)], [(0, false), (12, true), (13, false)]], 32), -2...1)
        XCTAssertEqual(range([[(30, true)]], 32), -30...2)
        XCTAssertEqual(range([], 32), 0...0, "nothing moves")
    }

    func testPointsGoUpTogetherInsideTheirLanes() {
        XCTAssertEqual(TimelineLayout.liftRange([0.25, 0.5]), -0.25...0.5)
        XCTAssertEqual(TimelineLayout.liftRange([0, 1]), 0...0)
        XCTAssertEqual(TimelineLayout.liftRange([]), 0...0)
    }
}
