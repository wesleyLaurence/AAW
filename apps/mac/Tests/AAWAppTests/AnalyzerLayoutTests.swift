@testable import AAWApp
import XCTest

final class AnalyzerLayoutTests: XCTestCase {
    typealias Layout = AnalyzerLayout
    let bounds = CGRect(x: 0, y: 0, width: 1000, height: 500)

    func testFourPanesInTwoColumnsWithDividersBetween() {
        let a = Layout.Arrangement.standard
        let panes = Layout.panes(a, in: bounds)
        XCTAssertEqual(Set(panes.keys), Set(Layout.Pane.allCases))
        // The left column ends at the split, the right begins after the gap.
        let (levels, loudness, spectrum, stereo) = (panes[.levels]!, panes[.loudness]!, panes[.spectrum]!, panes[.stereo]!)
        XCTAssertEqual(levels.maxX, 360 - Layout.gap / 2)
        XCTAssertEqual(spectrum.minX, 360 + Layout.gap / 2)
        XCTAssertEqual(levels.minX, 0)
        XCTAssertEqual(stereo.maxX, 1000)
        // Each column's divider is its own.
        XCTAssertEqual(levels.maxY, 250 - Layout.gap / 2)
        XCTAssertEqual(loudness.minY, 250 + Layout.gap / 2)
        XCTAssertEqual(spectrum.maxY, 290 - Layout.gap / 2)
        XCTAssertEqual(stereo.minY, 290 + Layout.gap / 2)
        XCTAssertEqual(loudness.maxY, 500)
        // The dividers are where the panes meet, and nowhere else.
        XCTAssertEqual(Layout.divider(at: CGPoint(x: 361, y: 100), a, in: bounds), .split)
        XCTAssertEqual(Layout.divider(at: CGPoint(x: 100, y: 252), a, in: bounds), .leftSplit)
        XCTAssertEqual(Layout.divider(at: CGPoint(x: 800, y: 288), a, in: bounds), .rightSplit)
        XCTAssertNil(Layout.divider(at: CGPoint(x: 800, y: 252), a, in: bounds))
        XCTAssertNil(Layout.divider(at: CGPoint(x: 100, y: 100), a, in: bounds))
    }

    func testADraggedDividerMovesAndStopsAtTheLeastShare() {
        let a = Layout.Arrangement.standard
        let moved = Layout.dragged(a, .split, to: CGPoint(x: 500, y: 0), in: bounds)
        XCTAssertEqual(moved.split, 0.5)
        XCTAssertEqual(Layout.panes(moved, in: bounds)[.levels]!.maxX, 500 - Layout.gap / 2)
        XCTAssertEqual(Layout.dragged(a, .split, to: CGPoint(x: 10, y: 0), in: bounds).split, Layout.leastShare)
        XCTAssertEqual(Layout.dragged(a, .leftSplit, to: CGPoint(x: 0, y: 490), in: bounds).leftSplit, 1 - Layout.leastShare)
        XCTAssertEqual(Layout.dragged(a, .rightSplit, to: CGPoint(x: 0, y: 125), in: bounds).rightSplit, 0.25)
        // The other dividers are left alone.
        XCTAssertEqual(moved.leftSplit, a.leftSplit)
        XCTAssertEqual(moved.rightSplit, a.rightSplit)
    }

    func testAPaneFillsTheWindowAndHiddenPanesGiveTheirRoom() {
        var a = Layout.Arrangement.standard
        a.toggleFill(.spectrum)
        XCTAssertEqual(Layout.panes(a, in: bounds), [.spectrum: bounds])
        XCTAssertEqual(a.shown, [.spectrum])
        XCTAssertNil(Layout.divider(at: CGPoint(x: 360, y: 100), a, in: bounds), "no dividers while one pane fills the window")
        a.toggleFill(.spectrum)
        XCTAssertEqual(Layout.panes(a, in: bounds).count, 4)
        // A hidden pane's column mate takes the column; a hidden column
        // gives the other the width.
        a.toggle(.loudness)
        var panes = Layout.panes(a, in: bounds)
        XCTAssertEqual(panes[.levels], CGRect(x: 0, y: 0, width: 360 - Layout.gap / 2, height: 500))
        XCTAssertNil(panes[.loudness])
        XCTAssertNil(Layout.divider(at: CGPoint(x: 100, y: 250), a, in: bounds))
        a.toggle(.levels)
        panes = Layout.panes(a, in: bounds)
        XCTAssertEqual(panes[.spectrum]!.minX, 0)
        XCTAssertEqual(panes[.stereo]!.maxX, 1000)
        XCTAssertEqual(Layout.divider(at: CGPoint(x: 100, y: 290), a, in: bounds), .rightSplit)
        // The last pane cannot hide, and showing a pane again brings it back.
        a.toggle(.spectrum)
        XCTAssertEqual(a.shown, [.stereo])
        a.toggle(.stereo)
        XCTAssertEqual(a.shown, [.stereo], "the last pane stays")
        a.toggle(.levels)
        XCTAssertEqual(Set(a.shown), [.levels, .stereo])
        // The fill mark is at a pane's top right; the rest of the bar is not it.
        let at = Layout.pane(at: CGPoint(x: 355, y: 5), a, in: bounds)
        XCTAssertEqual(at?.pane, .levels)
        XCTAssertEqual(at?.fillMark, true)
        XCTAssertEqual(Layout.pane(at: CGPoint(x: 100, y: 5), a, in: bounds)?.fillMark, false)
        // Kept as JSON and read back; nothing read gives the standard arrangement.
        XCTAssertEqual(Layout.Arrangement.read(a.data), a)
        XCTAssertEqual(Layout.Arrangement.read(nil), .standard)
        XCTAssertEqual(Layout.Arrangement.read(Data("nonsense".utf8)), .standard)
    }

    func testTheScalesAndTheBallistics() {
        let box = CGRect(x: 0, y: 100, width: 10, height: 200)
        XCTAssertEqual(Layout.y(6, floor: -60, top: 6, in: box), 100)
        XCTAssertEqual(Layout.y(-60, floor: -60, top: 6, in: box), 300)
        XCTAssertEqual(Layout.y(-27, floor: -60, top: 6, in: box), 200)
        XCTAssertEqual(Layout.y(-200, floor: -60, top: 6, in: box), 300, "held inside the box")
        // A peak jumps up and falls at 20 dB a second.
        XCTAssertEqual(Layout.fallen(-30, toward: -10, seconds: 0.016), -10)
        XCTAssertEqual(Layout.fallen(-10, toward: -60, seconds: 0.5), -20)
        XCTAssertEqual(Layout.fallen(-10, toward: -15, seconds: 1), -15, "no further than the reading")
        // The vectorscope: mono up the middle, inverted across, left to the left.
        let square = CGRect(x: 0, y: 0, width: 200, height: 200)
        XCTAssertEqual(Layout.scopePoint(left: 1, right: 1, in: square), CGPoint(x: 100, y: 0))
        XCTAssertEqual(Layout.scopePoint(left: 1, right: -1, in: square), CGPoint(x: 0, y: 100))
        XCTAssertEqual(Layout.scopePoint(left: 0.5, right: 0, in: square), CGPoint(x: 75, y: 75))
        XCTAssertEqual(Layout.scopePoint(left: 0, right: 0, in: square), CGPoint(x: 100, y: 100))
        // The cloud is drawn at the size of its loudest frame, up to 60 dB of gain.
        XCTAssertEqual(Layout.scopeGain([0.1, -0.25, 0.2]), 4)
        XCTAssertEqual(Layout.scopeGain([0, 0]), 1000)
        XCTAssertEqual(Layout.scopePoint(left: 0.25, right: 0.25, gain: 4, in: square), CGPoint(x: 100, y: 0))
        let bar = CGRect(x: 100, y: 0, width: 200, height: 6)
        XCTAssertEqual(Layout.correlationX(0, in: bar), 200)
        XCTAssertEqual(Layout.correlationX(-1, in: bar), 100)
        XCTAssertEqual(Layout.balanceX(6, in: bar), 250)
        XCTAssertEqual(Layout.balanceX(-30, in: bar), 100, "held to ±12 dB")
        // The history: newest at the right, ten a second, silence left out.
        let graph = CGRect(x: 0, y: 0, width: 600, height: 100)
        let points = Layout.historyPoints([-200, -15, -30], seconds: 60, perSecond: 10, in: graph)
        XCTAssertEqual(points.map(\.x), [599, 600])
        XCTAssertEqual(points[0].y, 25)
        XCTAssertEqual(points[1].y, 50)
        XCTAssertEqual(Layout.historyPoints(Array(repeating: -10, count: 700), seconds: 60, perSecond: 10, in: graph).count, 601, "a minute across, no more")
        XCTAssertEqual(Layout.text(-14.04, unit: "LUFS"), "−14.0 LUFS")
        XCTAssertEqual(Layout.text(2.26, unit: "dB"), "2.3 dB")
        XCTAssertEqual(Layout.text(nil, unit: "LUFS"), "−∞")
        XCTAssertEqual(Layout.text(-200, unit: "dB"), "−∞")
    }
}
