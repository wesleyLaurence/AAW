@testable import AAWApp
import XCTest

final class AnalyzerLayoutTests: XCTestCase {
    typealias Layout = AnalyzerLayout
    let bounds = CGRect(x: 0, y: 0, width: 1000, height: 500)

    func testSixPanesInThreeColumnsWithDividersBetween() {
        let a = Layout.Arrangement.standard
        let panes = Layout.panes(a, in: bounds)
        XCTAssertEqual(Set(panes.keys), Set(Layout.Pane.allCases))
        // The columns take the width in their shares, 0.26, 0.34 and 0.4,
        // with the gap between neighbors.
        let (levels, loudness, spectrum, stereo, waveform, spectrogram) = (panes[.levels]!, panes[.loudness]!, panes[.spectrum]!, panes[.stereo]!, panes[.waveform]!, panes[.spectrogram]!)
        XCTAssertEqual(levels.minX, 0)
        XCTAssertEqual(levels.maxX, 260 - Layout.gap / 2)
        XCTAssertEqual(spectrum.minX, 260 + Layout.gap / 2)
        XCTAssertEqual(spectrum.maxX, 600 - Layout.gap / 2)
        XCTAssertEqual(waveform.minX, 600 + Layout.gap / 2)
        XCTAssertEqual(spectrogram.maxX, 1000)
        // Each column's divider down it is its own: 0.5, 0.58 and 0.4.
        XCTAssertEqual(levels.maxY, 250 - Layout.gap / 2)
        XCTAssertEqual(loudness.minY, 250 + Layout.gap / 2)
        XCTAssertEqual(spectrum.maxY, 290 - Layout.gap / 2)
        XCTAssertEqual(stereo.minY, 290 + Layout.gap / 2)
        XCTAssertEqual(waveform.maxY, 200 - Layout.gap / 2)
        XCTAssertEqual(spectrogram.minY, 200 + Layout.gap / 2)
        XCTAssertEqual(loudness.maxY, 500)
        // The dividers are where the panes meet, and nowhere else.
        XCTAssertEqual(Layout.divider(at: CGPoint(x: 261, y: 100), a, in: bounds), .between(0))
        XCTAssertEqual(Layout.divider(at: CGPoint(x: 598, y: 400), a, in: bounds), .between(1))
        XCTAssertEqual(Layout.divider(at: CGPoint(x: 100, y: 252), a, in: bounds), .down(0))
        XCTAssertEqual(Layout.divider(at: CGPoint(x: 400, y: 288), a, in: bounds), .down(1))
        XCTAssertEqual(Layout.divider(at: CGPoint(x: 800, y: 203), a, in: bounds), .down(2))
        XCTAssertNil(Layout.divider(at: CGPoint(x: 800, y: 252), a, in: bounds))
        XCTAssertNil(Layout.divider(at: CGPoint(x: 100, y: 100), a, in: bounds))
        // The dividers' strips, for the cursors: two up and three across.
        let dividers = Layout.dividers(a, in: bounds)
        XCTAssertEqual(dividers.map(\.divider), [.between(0), .down(0), .between(1), .down(1), .down(2)])
        XCTAssertEqual(dividers.map(\.vertical), [true, false, true, false, false])
        XCTAssertEqual(dividers[0].grab, CGRect(x: 260 - Layout.dividerGrab, y: 0, width: Layout.dividerGrab * 2, height: 500))
        XCTAssertEqual(dividers[4].grab, CGRect(x: 600 + Layout.gap / 2, y: 200 - Layout.dividerGrab, width: 400 - Layout.gap / 2, height: Layout.dividerGrab * 2))
    }

    func testADraggedDividerMovesAndStopsAtTheLeastShare() {
        let a = Layout.Arrangement.standard
        // A column divider moves width between its two neighbors and leaves
        // the third column where it was.
        let moved = Layout.dragged(a, .between(0), to: CGPoint(x: 400, y: 0), in: bounds)
        XCTAssertEqual(moved.widths[0], 0.4, accuracy: 1e-9)
        XCTAssertEqual(moved.widths[1], 0.2, accuracy: 1e-9)
        XCTAssertEqual(moved.widths[2], 0.4, accuracy: 1e-9)
        XCTAssertEqual(Layout.panes(moved, in: bounds)[.levels]!.maxX, 400 - Layout.gap / 2)
        XCTAssertEqual(Layout.panes(moved, in: bounds)[.waveform]!.minX, 600 + Layout.gap / 2)
        XCTAssertEqual(Layout.dragged(a, .between(0), to: CGPoint(x: 10, y: 0), in: bounds).widths[0], Layout.leastShare, accuracy: 1e-9)
        let far = Layout.dragged(a, .between(1), to: CGPoint(x: 990, y: 0), in: bounds)
        XCTAssertEqual(far.widths[2], Layout.leastShare, accuracy: 1e-9)
        XCTAssertEqual(far.widths[1], 0.74 - Layout.leastShare, accuracy: 1e-9)
        // A divider down a column.
        XCTAssertEqual(Layout.dragged(a, .down(0), to: CGPoint(x: 0, y: 490), in: bounds).splits[0], 1 - Layout.leastShare)
        XCTAssertEqual(Layout.dragged(a, .down(2), to: CGPoint(x: 0, y: 125), in: bounds).splits[2], 0.25)
        XCTAssertEqual(moved.splits, a.splits)
        // With the middle column hidden, the one divider between the left
        // and the right moves width between those two.
        var two = a
        two.toggle(.spectrum)
        two.toggle(.stereo)
        XCTAssertEqual(Layout.shownColumns(two), [0, 2])
        XCTAssertEqual(Layout.panes(two, in: bounds)[.levels]!.maxX, (1000 * 0.26 / 0.66).rounded() - Layout.gap / 2)
        let dragged = Layout.dragged(two, .between(0), to: CGPoint(x: 500, y: 0), in: bounds)
        XCTAssertEqual(dragged.widths[0], 0.33, accuracy: 1e-9)
        XCTAssertEqual(dragged.widths[2], 0.33, accuracy: 1e-9)
        XCTAssertEqual(dragged.widths[1], 0.34, accuracy: 1e-9, "the hidden column keeps its share")
    }

    func testAPaneFillsTheWindowAndHiddenPanesGiveTheirRoom() {
        var a = Layout.Arrangement.standard
        a.toggleFill(.spectrogram)
        XCTAssertEqual(Layout.panes(a, in: bounds), [.spectrogram: bounds])
        XCTAssertEqual(a.shown, [.spectrogram])
        XCTAssertNil(Layout.divider(at: CGPoint(x: 260, y: 100), a, in: bounds), "no dividers while one pane fills the window")
        XCTAssertTrue(Layout.dividers(a, in: bounds).isEmpty)
        a.toggleFill(.spectrogram)
        XCTAssertEqual(Layout.panes(a, in: bounds).count, 6)
        // A hidden pane's column mate takes the column; a hidden column
        // gives the others the width.
        a.toggle(.loudness)
        var panes = Layout.panes(a, in: bounds)
        XCTAssertEqual(panes[.levels], CGRect(x: 0, y: 0, width: 260 - Layout.gap / 2, height: 500))
        XCTAssertNil(panes[.loudness])
        XCTAssertNil(Layout.divider(at: CGPoint(x: 100, y: 250), a, in: bounds))
        a.toggle(.levels)
        panes = Layout.panes(a, in: bounds)
        XCTAssertEqual(panes[.spectrum]!.minX, 0)
        XCTAssertEqual(panes[.spectrum]!.maxX, (1000 * 0.34 / 0.74).rounded() - Layout.gap / 2)
        XCTAssertEqual(panes[.spectrogram]!.maxX, 1000)
        XCTAssertEqual(Layout.divider(at: CGPoint(x: 100, y: 290), a, in: bounds), .down(1))
        // The last pane cannot hide, and showing a pane again brings it back.
        for pane in [Layout.Pane.spectrum, .stereo, .waveform] { a.toggle(pane) }
        XCTAssertEqual(a.shown, [.spectrogram])
        a.toggle(.spectrogram)
        XCTAssertEqual(a.shown, [.spectrogram], "the last pane stays")
        a.toggle(.levels)
        XCTAssertEqual(Set(a.shown), [.levels, .spectrogram])
        // The fill mark is at a pane's top right; the rest of the bar is not it.
        let levelsRect = Layout.panes(a, in: bounds)[.levels]!
        let at = Layout.pane(at: CGPoint(x: levelsRect.maxX - 5, y: 5), a, in: bounds)
        XCTAssertEqual(at?.pane, .levels)
        XCTAssertEqual(at?.fillMark, true)
        XCTAssertEqual(Layout.pane(at: CGPoint(x: 100, y: 5), a, in: bounds)?.fillMark, false)
        // Kept as JSON and read back; nothing read, nonsense and the first
        // half's shape with two columns give the standard arrangement.
        XCTAssertEqual(Layout.Arrangement.read(a.data), a)
        XCTAssertEqual(Layout.Arrangement.read(nil), .standard)
        XCTAssertEqual(Layout.Arrangement.read(Data("nonsense".utf8)), .standard)
        XCTAssertEqual(Layout.Arrangement.read(Data(#"{"hidden":[],"split":0.36,"leftSplit":0.5,"rightSplit":0.58}"#.utf8)), .standard)
        XCTAssertEqual(Layout.Arrangement.read(Data(#"{"hidden":[],"widths":[0.5,0.5],"splits":[0.5,0.5,0.5]}"#.utf8)), .standard, "a column short")
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

    func testTheTexturesScrollAndTakeTheColumnsNotSeen() {
        // The frequency axis: 20 Hz at the foot, 20 kHz at the top, in
        // equal steps of pitch, so 632 Hz (the geometric middle) is halfway.
        let box = CGRect(x: 0, y: 0, width: 100, height: 200)
        XCTAssertEqual(Layout.spectrogramY(hz: 20, in: box), 200)
        XCTAssertEqual(Layout.spectrogramY(hz: 20000, in: box), 0)
        XCTAssertEqual(Layout.spectrogramY(hz: 632.456, in: box), 100, accuracy: 0.01)
        XCTAssertEqual(Layout.spectrogramY(hz: 5, in: box), 200, "held inside")
        // The color ramp: the floor and below at 0, 0 dB and above at the top.
        XCTAssertEqual(Layout.colorIndex(-90, steps: 256), 0)
        XCTAssertEqual(Layout.colorIndex(-200, steps: 256), 0)
        XCTAssertEqual(Layout.colorIndex(0, steps: 256), 255)
        XCTAssertEqual(Layout.colorIndex(3, steps: 256), 255)
        XCTAssertEqual(Layout.colorIndex(-45, steps: 256), 128)
        // A sample's row: +1 at the top, −1 at the bottom, 0 in the middle.
        XCTAssertEqual(Layout.waveformRow(1, rows: 101), 0)
        XCTAssertEqual(Layout.waveformRow(-1, rows: 101), 100)
        XCTAssertEqual(Layout.waveformRow(0, rows: 101), 50)
        XCTAssertEqual(Layout.waveformRow(2, rows: 101), 0, "held inside")
        // The columns a reading adds: all of a first reading's, then the
        // new ones, blank columns for the ones a slow watcher lost, and a
        // fresh start when the meter started again.
        var new = Layout.newColumns(seen: 0, total: 10, held: 10)
        XCTAssertEqual([new.clear ? 1 : 0, new.blank, new.take, Int(new.seen)], [0, 0, 10, 10])
        new = Layout.newColumns(seen: 0, total: 100, held: 32)
        XCTAssertEqual([new.blank, new.take, Int(new.seen)], [68, 32, 100])
        new = Layout.newColumns(seen: 100, total: 101, held: 32)
        XCTAssertEqual([new.blank, new.take, Int(new.seen)], [0, 1, 101])
        new = Layout.newColumns(seen: 101, total: 101, held: 32)
        XCTAssertEqual([new.blank, new.take], [0, 0])
        new = Layout.newColumns(seen: 101, total: 200, held: 32)
        XCTAssertEqual([new.blank, new.take], [67, 32])
        new = Layout.newColumns(seen: 500, total: 3, held: 32)
        XCTAssertTrue(new.clear)
        XCTAssertEqual([new.blank, new.take, Int(new.seen)], [0, 3, 3])
        // The slices: a texture not yet full draws its columns at the right;
        // a full one wraps, the oldest column at the left edge.
        let wide = CGRect(x: 0, y: 10, width: 500, height: 100)
        var slices = Layout.slices(written: 100, width: 1000, in: wide)
        XCTAssertEqual(slices.count, 1)
        XCTAssertEqual(slices[0].columns, 0..<100)
        XCTAssertEqual(slices[0].rect, CGRect(x: 450, y: 10, width: 50, height: 100))
        slices = Layout.slices(written: 1000, width: 1000, in: wide)
        XCTAssertEqual(slices.count, 1)
        XCTAssertEqual(slices[0].columns, 0..<1000)
        XCTAssertEqual(slices[0].rect, wide)
        slices = Layout.slices(written: 1250, width: 1000, in: wide)
        XCTAssertEqual(slices.map(\.columns), [250..<1000, 0..<250])
        XCTAssertEqual(slices.map(\.rect), [CGRect(x: 0, y: 10, width: 375, height: 100), CGRect(x: 375, y: 10, width: 125, height: 100)])
        XCTAssertTrue(Layout.slices(written: 0, width: 1000, in: wide).isEmpty)
        // Second marks every five seconds back from the right edge.
        let marks = Layout.secondMarks(seconds: 20, every: 5, in: wide)
        XCTAssertEqual(marks.map(\.x), [375, 250, 125])
        XCTAssertEqual(marks.map(\.label), ["−5 s", "−10 s", "−15 s"])
        // The texture itself: columns written wrap around its width, and a
        // waveform column fills the rows between its two samples.
        let texture = ScrollingTexture(width: 4, height: 3)
        texture.append(from: 0, to: 1, color: 0xff00ff00)
        texture.appendBlank()
        XCTAssertEqual(texture.written, 2)
        for _ in 0..<5 { texture.append { UInt32($0) } }
        XCTAssertEqual(texture.written, 7)
        texture.clear()
        XCTAssertEqual(texture.written, 0)
        // A reading's long lists arrive as float bytes.
        var bytes = Data()
        for v: Float in [-200, 0.5, 1e-3] { withUnsafeBytes(of: v.bitPattern.littleEndian) { bytes.append(contentsOf: $0) } }
        XCTAssertEqual(bytes.floats, [-200, 0.5, 1e-3])
        XCTAssertEqual(Data().floats, [])
        XCTAssertEqual(NSColor(srgbRed: 1, green: 0.5, blue: 0, alpha: 1).pixel, 0xffff8000)
        XCTAssertEqual(NSColor(srgbRed: 1, green: 1, blue: 1, alpha: 0.5).pixel, 0x80808080, "premultiplied")
    }
}
