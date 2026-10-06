import XCTest
@testable import AAWApp

final class PatternLayoutTests: XCTestCase {
    /// A four-beat pattern of sixteenths in rows 640 points wide and 198 high.
    private func layout(beats: Double = 4, pads: [PatternLayout.Pad]) -> PatternLayout {
        var l = PatternLayout()
        l.size = CGSize(width: PatternLayout.gutter + 640, height: PatternLayout.rulerHeight + 198)
        l.lengthBeats = beats
        l.grid = 0.25
        l.setPads(pads)
        l.fit()
        return l
    }

    private let kit = [
        PatternLayout.Pad(name: "kick", gate: false, root: nil),
        PatternLayout.Pad(name: "snare", gate: false, root: nil),
        PatternLayout.Pad(name: "stab", gate: true, root: nil),
    ]

    func testAPadsRowIsStepsHitsOrNotes() {
        let bass = PatternLayout.Pad(name: "sub", gate: true, root: 33, pitches: [33, 36, 40])
        let l = layout(pads: kit + [bass])
        XCTAssertEqual(l.rows.map(\.kind), [.steps, .steps, .hits, .notes(low: 31, high: 43)])
        XCTAssertEqual(l.rows.map(\.top), [0, 24, 48, 72])
        // The rows of steps and hits are a line each; the notes have the rest.
        XCTAssertEqual(l.rows.map(\.height), [24, 24, 24, 126])
        XCTAssertEqual(l.contentHeight, 198)
        XCTAssertEqual(l.rows[3].semitone, 126.0 / 13, accuracy: 1e-9)
        XCTAssertEqual(l.row(atY: PatternLayout.rulerHeight + 30), 1)
        XCTAssertEqual(l.row(atY: PatternLayout.rulerHeight + 100), 3)
        XCTAssertNil(l.row(atY: 4), "the ruler")
        XCTAssertNil(l.row(atY: 400))
    }

    func testARowOfNotesShowsWhatIsPlayedWithRoomAround() {
        let span = PatternLayout.span
        // An octave at the least, around what there is.
        var s = span(PatternLayout.Pad(name: "p", gate: false, root: 60))
        XCTAssertEqual(s.high - s.low + 1, PatternLayout.minSpan)
        XCTAssertTrue(s.low <= 58 && s.high >= 62)
        // Two notes beyond the lowest and the highest played.
        s = span(PatternLayout.Pad(name: "p", gate: false, root: 45, pitches: [40, 64]))
        XCTAssertEqual([s.low, s.high], [38, 66])
        // Against the ends of MIDI the span keeps its size.
        s = span(PatternLayout.Pad(name: "p", gate: false, root: 1))
        XCTAssertEqual([s.low, s.high], [0, 12])
        s = span(PatternLayout.Pad(name: "p", gate: false, root: 126, pitches: [127]))
        XCTAssertEqual([s.low, s.high], [115, 127])
        // Many notes make a row taller than the view, which then scrolls.
        var l = layout(pads: [PatternLayout.Pad(name: "p", gate: false, root: 60, pitches: [30, 90])])
        XCTAssertEqual(l.rows[0].height, 65 * PatternLayout.minSemitone)
        l.scroll.y = 1000
        l.clamp()
        XCTAssertEqual(l.scroll.y, 65 * PatternLayout.minSemitone - 198)
    }

    func testStepsAndLinesAreOnTheGrid() {
        let l = layout(pads: kit)
        XCTAssertEqual(l.pixelsPerBeat, 160, "four beats fill the rows")
        XCTAssertEqual(l.steps, 16)
        let x = { (beat: Double) in l.x(beat) }
        XCTAssertEqual(l.step(atX: x(0.01)), 0)
        XCTAssertEqual(l.step(atX: x(1.3)), 5)
        XCTAssertEqual(l.step(atX: x(3.99)), 15)
        XCTAssertEqual(l.step(atX: x(9)), 15, "no step past the end")
        XCTAssertEqual(l.step(atX: 0), 0)
        XCTAssertEqual(l.line(atX: x(1.3)), 5)
        XCTAssertEqual(l.line(atX: x(1.4)), 6)
        XCTAssertEqual(l.line(atX: x(9)), 16, "the pattern's end is a line")
        XCTAssertEqual(l.cell(row: 1, step: 5), CGRect(x: x(1.25), y: PatternLayout.rulerHeight + 24, width: 40, height: 24))
        XCTAssertEqual(l.steps(forDrag: 59), 1)
        XCTAssertEqual(l.steps(forDrag: -100), -3)
    }

    func testAnEventIsAtItsBeatAndNote() {
        let l = layout(pads: kit + [PatternLayout.Pad(name: "sub", gate: true, root: 33, pitches: [36])])
        let top = PatternLayout.rulerHeight
        // In a row of hits: as long as it is held, or a short mark.
        XCTAssertEqual(l.event(row: 2, at: 1, duration: 0.5, pitch: nil), CGRect(x: l.x(1), y: top + 52, width: 80, height: 16))
        XCTAssertEqual(l.event(row: 0, at: 1, duration: nil, pitch: nil).width, 14)
        // In a row of notes: at its note from the top, or at the root without one.
        guard case .notes(let low, let high) = l.rows[3].kind else { return XCTFail("a row of notes") }
        XCTAssertEqual([low, high], [29, 41])
        let semitone = l.rows[3].semitone
        XCTAssertEqual(l.event(row: 3, at: 2, duration: 1, pitch: 36).minY, top + 72 + 5 * semitone, accuracy: 1e-9)
        XCTAssertEqual(l.event(row: 3, at: 2, duration: 1, pitch: nil).minY, top + 72 + 8 * semitone, accuracy: 1e-9)
        XCTAssertEqual(l.event(row: 3, at: 2, duration: 1, pitch: 99).minY, top + 72, accuracy: 1e-9, "a note off the row is at its edge")
        XCTAssertEqual(l.pitch(atY: top + 72 + 5.5 * semitone, row: 3), 36)
        XCTAssertEqual(l.pitch(atY: top + 72 + 0.1, row: 3), 41)
        XCTAssertEqual(l.pitch(atY: top + 197.9, row: 3), 29)
        XCTAssertNil(l.pitch(atY: top + 10, row: 0), "steps have no notes")
    }

    func testALongPatternOpensWithStepsWideEnoughToClickAndScrolls() {
        var l = layout(beats: 256, pads: kit)
        XCTAssertEqual(l.pixelsPerBeat, 36, "sixteenths nine points wide")
        XCTAssertEqual(l.contentWidth, 9216)
        l.scroll.x = 99999
        l.clamp()
        XCTAssertEqual(l.scroll.x, 9216 - 640)
        // It zooms out as far as showing the whole pattern, and in around the pointer.
        l.zoom(by: 0.0001, anchorX: 300)
        XCTAssertEqual(l.pixelsPerBeat, 2.5)
        XCTAssertEqual(l.scroll.x, 0)
        let before = l.beat(atX: 400)
        l.zoom(by: 8, anchorX: 400)
        XCTAssertEqual(l.beat(atX: 400), before, accuracy: 1e-9)
        l.zoom(by: 1000, anchorX: 400)
        XCTAssertEqual(l.pixelsPerBeat, PatternLayout.maxPixelsPerBeat)
        l.reveal(200)
        XCTAssertEqual(l.x(200), PatternLayout.gutter + 64, accuracy: 1e-9)
    }

    func testAStepsLevelIsHowHardItPlays() {
        XCTAssertEqual([1, 5, 9, 10].map(PatternLayout.velocity(ofLevel:)), [14, 71, 127, 100])
        // A drag up makes a step harder, five points to a level; an `x` goes from 7.
        XCTAssertEqual(PatternLayout.level(from: 5, draggedBy: -11), 7)
        XCTAssertEqual(PatternLayout.level(from: 5, draggedBy: 14), 2)
        XCTAssertEqual(PatternLayout.level(from: 10, draggedBy: -6), 8)
        XCTAssertEqual(PatternLayout.level(from: 10, draggedBy: 0), 7)
        XCTAssertEqual(PatternLayout.level(from: 3, draggedBy: 200), 1, "never off: a click does that")
        XCTAssertEqual(PatternLayout.level(from: 3, draggedBy: -200), 9)
    }

    func testEventsMoveTogetherAndStayInThePattern() {
        let bass = PatternLayout.Pad(name: "sub", gate: true, root: 33, pitches: [33, 36, 40])
        let l = layout(pads: kit + [bass])
        // By whole steps: the earliest to the start, the latest to the last step.
        XCTAssertEqual(l.stepRange(of: [0.5, 2]), -2...7)
        XCTAssertEqual(l.stepRange(of: [0.1, 3.9]), 0...0, "off the grid, as far off it")
        XCTAssertEqual(l.beatRange(of: [0.5, 2]), -0.5...1.999)
        // Up and down inside each one's row of notes, from 31 to 43.
        XCTAssertEqual(l.semitoneRange(of: [(row: 3, pitch: 33), (row: 3, pitch: 40)]), -2...3)
        XCTAssertEqual(l.semitoneRange(of: [(row: 0, pitch: 60)]), 0...0, "a row of steps has no notes")
        XCTAssertEqual(l.semitoneRange(of: []), 0...0)
    }
}
