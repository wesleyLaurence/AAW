import XCTest
@testable import AAWApp

final class GridTests: XCTestCase {
    func testTheMenuListsSizesAndTellsTriplets() {
        XCTAssertEqual(Grid.sizes(), ["4", "2", "1", "1/2", "1/4", "1/8", "1/16"])
        XCTAssertEqual(Grid.sizes().map { Grid.name($0) }, ["1 Bar", "1/2", "1/4", "1/8", "1/16", "1/32", "1/64"])
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

    func testTheListsBarIsTheSongs() {
        // In 3/4 a bar is three beats, and the half note stays a half note.
        XCTAssertEqual(Grid.sizes(bar: 3), ["3", "2", "1", "1/2", "1/4", "1/8", "1/16"])
        XCTAssertEqual(Grid.sizes(bar: 3).map { Grid.name($0, bar: 3) }, ["1 Bar", "1/2", "1/4", "1/8", "1/16", "1/32", "1/64"])
        XCTAssertEqual(Grid.name("4", bar: 3), "1/1", "a whole note is no bar of 3/4")
        XCTAssertEqual(Grid.name("6", bar: 3), "2 Bars")
        // In 7/8 a bar is three and a half beats.
        XCTAssertEqual(Grid.list(bar: 3.5).first, "7/2")
        XCTAssertEqual(Grid.name("7/2", bar: 3.5), "1 Bar")
        XCTAssertEqual(Grid.coarser("2", in: Grid.list(bar: 3.5)), "7/2")
        XCTAssertEqual(Grid.finer("7/2", in: Grid.list(bar: 3.5)), "2")
        // In 2/4 the bar and the half note are one value, listed once.
        XCTAssertEqual(Grid.sizes(bar: 2), ["2", "1", "1/2", "1/4", "1/8", "1/16"])
        XCTAssertEqual(Grid.name("2", bar: 2), "1 Bar")
        // A bar of three beats has no triplet, though two beats is listed.
        XCTAssertNil(Grid.triplets("3", true, in: Grid.list(bar: 3), bar: 3))
        XCTAssertEqual(Grid.choose("3", keeping: "1/6", in: Grid.list(bar: 3), bar: 3), "3")
        XCTAssertEqual(Grid.size("3", bar: 3), "3")
        XCTAssertEqual(Grid.choose("1/8", keeping: "1/6", in: Grid.list(bar: 3), bar: 3), "1/12")
    }

    func testAGridIsWrittenAsTheSongWritesIt() {
        XCTAssertEqual(Grid.text(0.25), "1/4")
        XCTAssertEqual(Grid.text(1.0 / 3), "1/3")
        XCTAssertEqual(Grid.text(4), "4")
        XCTAssertEqual(Grid.text(16), "16")
        XCTAssertEqual(Grid.text(3.5), "7/2", "a bar of 7/8")
    }
}
