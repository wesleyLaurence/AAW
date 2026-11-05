import AAWCore
import XCTest
@testable import AAWApp

/// The loop L turns on with nothing selected and no earlier loop.
final class LoopTests: XCTestCase {
    /// A 16-bar song in 4/4 with a verse over bars 5–8.
    private let verse = SectionView(key: 1, id: "verse", at: 16, lengthBeats: 16)

    private func loop(cue: Double, sections: [SectionView] = [], end: Double = 64) -> LoopRegion? {
        SongModel.defaultLoop(cue: cue, sections: sections, barBeats: 4, lengthBeats: end)
    }

    func testTheSectionAtTheCueIsLooped() {
        XCTAssertEqual(loop(cue: 21, sections: [verse]), LoopRegion(start: 16, length: 16))
    }

    func testOutsideASectionTheBarAtTheCueIsLooped() {
        XCTAssertEqual(loop(cue: 9.5, sections: [verse]), LoopRegion(start: 8, length: 4))
    }

    func testABarCutByTheEndIsLoopedToTheEnd() {
        XCTAssertEqual(loop(cue: 61, end: 62), LoopRegion(start: 60, length: 2))
    }

    func testPastTheEndTheLastBarIsLooped() {
        XCTAssertEqual(loop(cue: 64), LoopRegion(start: 60, length: 4), "at the end")
        XCTAssertEqual(loop(cue: 150), LoopRegion(start: 60, length: 4), "far past it")
        XCTAssertEqual(loop(cue: 70, end: 62), LoopRegion(start: 60, length: 2), "an end inside a bar")
    }

    func testTheLoopIsAlwaysInsideTheSong() {
        for cue in stride(from: 0.0, through: 200, by: 0.75) {
            guard let r = loop(cue: cue, sections: [verse], end: 62) else { return XCTFail("none at \(cue)") }
            XCTAssertGreaterThan(r.length, 0, "at \(cue)")
            XCTAssertGreaterThanOrEqual(r.start, 0, "at \(cue)")
            XCTAssertLessThanOrEqual(r.start + r.length, 62, "at \(cue)")
        }
    }

    func testASongWithNoLengthHasNoLoop() {
        XCTAssertNil(loop(cue: 0, end: 0))
    }
}
