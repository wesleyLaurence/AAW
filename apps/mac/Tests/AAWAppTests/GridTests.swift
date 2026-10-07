import XCTest
@testable import AAWApp

final class GridTests: XCTestCase {
    func testTheMenuListsSizesAndTellsTriplets() {
        XCTAssertEqual(Grid.sizes, ["4", "2", "1", "1/2", "1/4", "1/8", "1/16"])
        XCTAssertEqual(Grid.sizes.map(Grid.name), ["1 Bar", "1/2", "1/4", "1/8", "1/16", "1/32", "1/64"])
        XCTAssertTrue(Grid.isTriplet("1/3"))
        XCTAssertFalse(Grid.isTriplet("1/4"))
        XCTAssertFalse(Grid.isTriplet("8"), "two bars")
    }

    func testFinerAndCoarserKeepTheKind() {
        XCTAssertEqual(Grid.finer("1/4"), "1/8")
        XCTAssertEqual(Grid.coarser("1/4"), "1/2")
        XCTAssertEqual(Grid.finer("1/3"), "1/6", "a triplet stays a triplet")
        XCTAssertEqual(Grid.coarser("1/3"), "2/3")
        XCTAssertNil(Grid.finer("1/16"), "the finest")
        XCTAssertNil(Grid.coarser("4"), "a bar is the coarsest")
        // The timeline's zoom can give a grid of two bars, which the list lacks.
        XCTAssertEqual(Grid.finer("8"), "4")
        XCTAssertNil(Grid.coarser("8"))
        // A pattern's steps go from a beat to a sixty-fourth note.
        XCTAssertNil(Grid.coarser("1", in: Grid.patternValues))
        XCTAssertEqual(Grid.finer("1", in: Grid.patternValues), "1/2")
        XCTAssertNil(Grid.finer("x"))
    }

    func testTripletsGoBothWays() {
        XCTAssertEqual(Grid.triplets("1/4", true), "1/6")
        XCTAssertEqual(Grid.triplets("1/6", false), "1/4")
        XCTAssertEqual(Grid.triplets("1/4", false), "1/4", "already straight")
        XCTAssertNil(Grid.triplets("4", true), "a bar has no triplet in the list")
        XCTAssertNil(Grid.triplets("8", true))
        XCTAssertEqual(Grid.size("1/6"), "1/4")
        XCTAssertEqual(Grid.size("1/4"), "1/4")
        XCTAssertEqual(Grid.size("8"), "8")
    }

    func testChoosingASizeKeepsTriplets() {
        XCTAssertEqual(Grid.choose("1/8", keeping: "1/4"), "1/8")
        XCTAssertEqual(Grid.choose("1/8", keeping: "1/6"), "1/12")
        XCTAssertEqual(Grid.choose("4", keeping: "1/6"), "4", "a bar has no triplet, so the bar")
    }

    func testAGridIsWrittenAsTheSongWritesIt() {
        XCTAssertEqual(Grid.text(0.25), "1/4")
        XCTAssertEqual(Grid.text(1.0 / 3), "1/3")
        XCTAssertEqual(Grid.text(4), "4")
        XCTAssertEqual(Grid.text(16), "16")
    }
}
