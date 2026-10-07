import XCTest
@testable import AAWApp

final class PianoRollLayoutTests: XCTestCase {
    /// A four-beat clip of sixteenths in a view 640 points wide and 198 high,
    /// with the velocity lane under 142 points of notes.
    private func layout(length: Double = 4, notes: [(at: Double, end: Double)] = [], pitches: [Int] = []) -> PianoRollLayout {
        var l = PianoRollLayout()
        l.size = CGSize(width: PianoRollLayout.gutter + 640, height: PianoRollLayout.rulerHeight + 198)
        l.grid = 0.25
        l.setSpan(length: length, notes: notes)
        l.fit(pitches: pitches)
        return l
    }

    func testEveryNoteHasARowWhateverTheInstrument() {
        let l = layout()
        XCTAssertEqual(l.contentHeight, 128 * PianoRollLayout.defaultRow)
        // 127 at the top, 0 at the bottom, a row each.
        var top = l
        top.scroll.y = 0
        XCTAssertEqual(top.y(127), PianoRollLayout.rulerHeight)
        XCTAssertEqual(top.y(0), PianoRollLayout.rulerHeight + 127 * PianoRollLayout.defaultRow)
        XCTAssertEqual(top.pitch(atY: PianoRollLayout.rulerHeight + 1), 127)
        XCTAssertEqual(top.pitch(atY: PianoRollLayout.rulerHeight + 10.5), 126)
        // Past either end of the keys, the nearest note.
        XCTAssertEqual(top.pitch(atY: -50), 127)
        XCTAssertEqual(top.pitch(atY: 5000), 0)
    }

    func testAClipOpensFittedWithItsNotesInTheMiddle() {
        let l = layout(notes: [(0, 1), (1, 2)], pitches: [60, 72])
        XCTAssertEqual(l.pixelsPerBeat, 160, "four beats in 640 points")
        XCTAssertEqual(l.x(0), PianoRollLayout.gutter)
        XCTAssertEqual(l.beat(atX: PianoRollLayout.gutter + 320), 2)
        // The middle of 60 and 72 is in the middle of the rows shown.
        XCTAssertEqual(l.notesBottom, PianoRollLayout.rulerHeight + 142)
        let middle = PianoRollLayout.rulerHeight + 142 / 2
        XCTAssertEqual(l.pitch(atY: middle), 66)
        // With no notes, middle C's octave.
        XCTAssertTrue((60...72).contains(layout().pitch(atY: middle)))
    }

    func testTheViewTakesInNotesOutsideTheClip() {
        // A note half a beat before the clip and one past its end.
        let l = layout(notes: [(-0.5, 0.5), (3.5, 5.25)], pitches: [60])
        XCTAssertEqual(l.first, -1)
        XCTAssertEqual(l.last, 6)
        // It opens on the clip's start, and scrolls back to see before it.
        XCTAssertEqual(l.x(0), PianoRollLayout.gutter)
        var back = l
        back.scroll.x = 0
        XCTAssertEqual(back.x(-1), PianoRollLayout.gutter)
        XCTAssertEqual(back.beat(atX: PianoRollLayout.gutter), -1)
    }

    func testANoteIsDrawnAtItsPlaceAndPitch() {
        var l = layout()
        l.scroll = .zero
        let r = l.rect(at: 1.975, duration: 0.5, pitch: 62)
        XCTAssertEqual(r.minX, PianoRollLayout.gutter + 1.975 * 160, accuracy: 1e-9)
        XCTAssertEqual(r.width, 80, accuracy: 1e-9)
        XCTAssertEqual(r.minY, l.y(62))
        XCTAssertEqual(r.height, PianoRollLayout.defaultRow)
        // A very short note is still wide enough to take hold of.
        XCTAssertEqual(l.rect(at: 0, duration: 0.001, pitch: 60).width, 4)
        // The end of a note stretches it; a narrow note is only moved.
        XCTAssertTrue(PianoRollLayout.onEnd(CGPoint(x: r.maxX - 2, y: r.midY), of: r))
        XCTAssertFalse(PianoRollLayout.onEnd(CGPoint(x: r.midX, y: r.midY), of: r))
        XCTAssertFalse(PianoRollLayout.onEnd(CGPoint(x: 99, y: 0), of: CGRect(x: 96, y: 0, width: 6, height: 10)))
        // Its start moves the start; a short note keeps room to be moved by,
        // and has its end only.
        XCTAssertTrue(PianoRollLayout.onStart(CGPoint(x: r.minX + 2, y: r.midY), of: r))
        XCTAssertFalse(PianoRollLayout.onStart(CGPoint(x: r.midX, y: r.midY), of: r))
        let short = CGRect(x: 96, y: 0, width: 12, height: 10)
        XCTAssertFalse(PianoRollLayout.onStart(CGPoint(x: 97, y: 5), of: short))
        XCTAssertTrue(PianoRollLayout.onEnd(CGPoint(x: 107, y: 5), of: short))
    }

    func testTheGridSnapsAndDragsMoveByStepsBeatsAndNotes() {
        let l = layout()
        XCTAssertEqual(l.step(at: 1.3), 1.25)
        XCTAssertEqual(l.step(at: 0.25), 0.25, "a beat on a line is its own step")
        XCTAssertEqual(l.line(at: 1.38), 1.5)
        // 160 points a beat: 40 a sixteenth.
        XCTAssertEqual(l.steps(forDrag: 41), 1)
        XCTAssertEqual(l.steps(forDrag: -95), -2)
        // Off the grid, to a thousandth of a beat: 4 points is 0.025 beats.
        XCTAssertEqual(l.beats(forDrag: 4), 0.025)
        // Up is higher.
        XCTAssertEqual(l.semitones(forDrag: -21), 2)
        XCTAssertEqual(l.semitones(forDrag: 9), -1)
        var thirds = l
        thirds.grid = 1.0 / 3
        XCTAssertEqual(thirds.step(at: 1.4), 4.0 / 3, accuracy: 1e-12)
    }

    func testZoomKeepsTheBeatUnderThePointerAndRevealScrolls() {
        var l = layout(length: 16)
        let x = PianoRollLayout.gutter + 200
        let before = l.beat(atX: x)
        l.zoom(by: 2, anchorX: x)
        XCTAssertEqual(l.beat(atX: x), before, accuracy: 1e-9)
        l.reveal(12)
        XCTAssertTrue(l.x(12) >= PianoRollLayout.gutter && l.x(12) <= l.size.width)
        l.reveal(pitch: 120)
        XCTAssertTrue(l.y(120) >= PianoRollLayout.rulerHeight)
        l.reveal(pitch: 2)
        XCTAssertTrue(l.y(2) + l.row <= l.notesBottom, "above the velocity lane")
    }

    func testRowsZoomUpAndDownAroundThePointer() {
        var l = layout(pitches: [60])
        let y = PianoRollLayout.rulerHeight + 50
        let before = l.pitch(atY: y)
        l.zoomRows(by: 2, anchorY: y)
        XCTAssertEqual(l.row, 20)
        XCTAssertEqual(l.pitch(atY: y), before)
        XCTAssertEqual(l.semitones(forDrag: -41), 2, "a drag moves by the rows as they are drawn")
        // As tall as the most, and as short as every note in view.
        l.zoomRows(by: 100, anchorY: y)
        XCTAssertEqual(l.row, PianoRollLayout.maxRow)
        l.zoomRows(by: 0.01, anchorY: y)
        XCTAssertEqual(l.row, PianoRollLayout.minRow)
        var tall = l
        tall.size.height = PianoRollLayout.rulerHeight + PianoRollLayout.velocityHeight + 128 * 8
        tall.zoomRows(by: 0.01, anchorY: y)
        XCTAssertEqual(tall.row, 8, "all 128 notes fit")
        XCTAssertEqual(tall.scroll.y, 0)
        // A clip opened again starts at the usual height.
        l.fit(pitches: [60])
        XCTAssertEqual(l.row, PianoRollLayout.defaultRow)
    }

    func testVelocityIsALaneUnderTheNotes() {
        let l = layout()
        XCTAssertEqual(l.velocityHeight, PianoRollLayout.velocityHeight)
        let bottom = l.size.height - PianoRollLayout.velocityInset
        XCTAssertEqual(l.velocityY(0), bottom)
        XCTAssertEqual(l.velocityY(127), l.notesBottom + PianoRollLayout.velocityInset)
        // 46 points of reach for 127 steps: up is louder.
        XCTAssertEqual(l.velocities(forDrag: -23), 64)
        XCTAssertEqual(l.velocities(forDrag: 46), -127)
        XCTAssertTrue(l.inVelocity(CGPoint(x: PianoRollLayout.gutter + 10, y: l.notesBottom + 1)))
        XCTAssertFalse(l.inVelocity(CGPoint(x: 10, y: l.notesBottom + 1)), "the keys' column")
        XCTAssertFalse(l.inVelocity(CGPoint(x: PianoRollLayout.gutter + 10, y: l.notesBottom - 1)))
        // A short panel keeps its height for the notes.
        var short = l
        short.size.height = 140
        XCTAssertEqual(short.velocityHeight, 0)
        XCTAssertEqual(short.notesBottom, 140)
        XCTAssertFalse(short.inVelocity(CGPoint(x: PianoRollLayout.gutter + 10, y: 139)))
    }

    func testGridsAreNamedAsNoteValues() {
        let names = PianoRollLayout.grids.map(PianoRollLayout.noteValue)
        XCTAssertEqual(names, ["1 Bar", "1/2", "1/2T", "1/4", "1/4T", "1/8", "1/8T", "1/16", "1/16T", "1/32", "1/32T", "1/64"])
        // A step the menus do not list keeps its beats; whole bars are bars.
        XCTAssertEqual(PianoRollLayout.noteValue("3/4"), "3/4 beats")
        XCTAssertEqual(PianoRollLayout.noteValue("8"), "2 Bars")
        XCTAssertEqual(PianoRollLayout.noteValue("x"), "x")
    }

    func testBeatsAreReadAsTheSongWritesThem() {
        XCTAssertEqual(PianoRollLayout.beats("1/4"), 0.25)
        XCTAssertEqual(PianoRollLayout.beats("1/3")!, 1.0 / 3, accuracy: 1e-12)
        XCTAssertEqual(PianoRollLayout.beats("-1.5"), -1.5)
        XCTAssertNil(PianoRollLayout.beats("1/0"))
        XCTAssertNil(PianoRollLayout.beats("x"))
    }
}
