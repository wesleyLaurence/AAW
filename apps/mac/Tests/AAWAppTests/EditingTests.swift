import AAWCore
import AppKit
import XCTest
@testable import AAWApp

final class EditingTests: XCTestCase {
    /// A 64-beat song in lanes 680 points wide: 10 points a beat, a grid of bars.
    private func layout() -> TimelineLayout {
        var l = TimelineLayout()
        l.lengthBeats = 64
        l.size = CGSize(width: TimelineLayout.headerWidth + 680, height: 400)
        l.fit()
        return l
    }

    func testADraggedClipMovesByGridStepsAndStaysInTheSong() {
        var l = layout()
        let free = -8.0...40.0
        XCTAssertEqual(l.move(byX: 17, free: false, within: free), 0, "under half a bar")
        XCTAssertEqual(l.move(byX: 23, free: false, within: free), 4)
        XCTAssertEqual(l.move(byX: -70, free: false, within: free), -8)
        XCTAssertEqual(l.move(byX: -200, free: false, within: free), -8, "no earlier than the song's start")
        XCTAssertEqual(l.move(byX: 900, free: false, within: free), 40)
        // A clip two beats from the end cannot take a whole bar's step.
        XCTAssertEqual(l.move(byX: 60, free: false, within: -8...2), 0)
        // Off the grid, to a thousandth of a beat.
        XCTAssertEqual(l.move(byX: 23.456, free: true, within: free), 2.346, accuracy: 1e-9)
        XCTAssertEqual(l.move(byX: 60, free: true, within: -8...2), 2)
        // Zoomed in, the steps are finer.
        l.pixelsPerBeat = 60
        XCTAssertEqual(l.move(byX: 40, free: false, within: free), 0.75)
    }

    func testAClipsEndSnapsToItsRepeats() {
        let l = layout()
        // A clip at beat 8 of a 4-beat pattern.
        XCTAssertEqual(l.repeats(atX: l.x(19), clipAt: 8, patternBeats: 4), 3)
        XCTAssertEqual(l.repeats(atX: l.x(21), clipAt: 8, patternBeats: 4), 3)
        XCTAssertEqual(l.repeats(atX: l.x(2), clipAt: 8, patternBeats: 4), 1, "never none")
        XCTAssertEqual(l.repeats(atX: l.x(90), clipAt: 8, patternBeats: 4), 14, "no further than the song")
        XCTAssertEqual(l.repeats(atX: l.x(90), clipAt: 8, patternBeats: 24), 2)
    }

    func testAHeadersPartsAreWhereTheyAreDrawn() {
        let track = HeaderLayout(kind: .track, top: 100, sends: 2, folds: true)
        XCTAssertEqual(track.height, TimelineLayout.trackHeight + 2 * HeaderLayout.sendHeight + HeaderLayout.sendPadding)
        func part(_ rect: CGRect) -> HeaderLayout.Part { track.part(at: CGPoint(x: rect.midX, y: rect.midY)) }
        XCTAssertEqual(part(track.fold), .fold)
        XCTAssertEqual(part(track.name), .name)
        XCTAssertEqual(part(track.mute), .mute)
        XCTAssertEqual(part(track.solo), .solo)
        XCTAssertEqual(part(track.volumeBar), .volume)
        XCTAssertEqual(part(track.volumeText), .volume)
        XCTAssertEqual(part(track.pan), .pan)
        XCTAssertEqual(part(track.sendBar(0)), .send(0))
        XCTAssertEqual(part(track.sendName(1)), .send(1))
        XCTAssertEqual(part(track.auto), .auto)
        XCTAssertEqual(track.part(at: CGPoint(x: 135, y: 110)), .name, "the name reaches to the buttons")
        XCTAssertEqual(track.part(at: CGPoint(x: 207, y: 130)), .body, "beside the pan")
        XCTAssertFalse(track.name.intersects(track.auto))
        XCTAssertFalse(track.auto.intersects(track.mute))
        XCTAssertLessThanOrEqual(track.send(1).maxY, 100 + track.height)
        XCTAssertFalse(track.name.intersects(track.mute))
        XCTAssertFalse(track.volume.intersects(track.pan))
        // Folded, and in a song without returns, there are no sends and no mark.
        let plain = HeaderLayout(kind: .track, top: 100)
        XCTAssertEqual(plain.height, TimelineLayout.trackHeight)
        XCTAssertEqual(plain.part(at: CGPoint(x: plain.fold.midX, y: plain.fold.midY)), .name)
        XCTAssertLessThan(plain.name.minX, track.name.minX)

        let bus = HeaderLayout(kind: .bus, top: 300)
        XCTAssertEqual(bus.height, TimelineLayout.busHeight)
        XCTAssertEqual(bus.part(at: CGPoint(x: bus.mute.midX, y: bus.mute.midY)), .mute)
        XCTAssertEqual(bus.part(at: CGPoint(x: bus.volume.midX, y: bus.volume.midY)), .volume)
        XCTAssertEqual(bus.part(at: CGPoint(x: 30, y: 315)), .name)
        // The master has a volume and nothing to mute.
        let master = HeaderLayout(kind: .master, top: 300)
        XCTAssertEqual(master.part(at: CGPoint(x: master.volume.midX, y: master.volume.midY)), .volume)
        XCTAssertEqual(master.part(at: CGPoint(x: 100, y: 315)), .name)
        XCTAssertGreaterThan(master.volume.maxX, bus.volume.maxX)

        // A group has a track's header, with the mark that folds its tracks
        // away before the mark that shows its sends; a grouped track's marks
        // and name sit in from the edge.
        let group = HeaderLayout(kind: .group, top: 100, sends: 2, folds: true, collapsed: true)
        XCTAssertEqual(group.height, track.height)
        XCTAssertEqual(group.part(at: CGPoint(x: group.members.midX, y: group.members.midY)), .members)
        XCTAssertEqual(group.part(at: CGPoint(x: group.fold.midX, y: group.fold.midY)), .fold)
        XCTAssertEqual(group.part(at: CGPoint(x: group.solo.midX, y: group.solo.midY)), .solo)
        XCTAssertEqual(group.part(at: CGPoint(x: group.sendBar(1).midX, y: group.sendBar(1).midY)), .send(1))
        XCTAssertLessThan(group.members.maxX, group.fold.minX)
        XCTAssertLessThan(group.fold.maxX, group.name.minX)
        XCTAssertEqual(group.part(at: CGPoint(x: group.name.midX, y: group.name.midY)), .name)
        let member = HeaderLayout(kind: .track, top: 100, folds: true, grouped: true)
        XCTAssertEqual(member.fold.minX, track.fold.minX + HeaderLayout.indent)
        XCTAssertEqual(member.name.minX, track.name.minX + HeaderLayout.indent)
        XCTAssertEqual(member.part(at: CGPoint(x: member.fold.midX, y: member.fold.midY)), .fold)
        XCTAssertEqual(member.part(at: CGPoint(x: track.fold.midX, y: track.fold.midY)), .body, "left of the indent")
        XCTAssertEqual(track.part(at: CGPoint(x: track.members.midX, y: track.members.midY)), .fold, "a track has no members mark")
        for row in [bus, master] {
            XCTAssertEqual(row.part(at: CGPoint(x: row.auto.midX, y: row.auto.midY)), .auto)
            XCTAssertFalse(row.auto.intersects(row.volume) || row.auto.intersects(row.name))
        }
    }

    func testLanesFoldOutUnderARowWithTheirMarks() {
        // Two sends, then three lanes and the strip that adds one.
        let track = HeaderLayout(kind: .track, top: 100, sends: 2, folds: true, lanes: 3)
        let base = TimelineLayout.trackHeight + 2 * HeaderLayout.sendHeight + HeaderLayout.sendPadding
        XCTAssertEqual(track.baseHeight, base)
        XCTAssertEqual(track.height, base + 3 * HeaderLayout.laneHeight + HeaderLayout.laneAddHeight)
        XCTAssertEqual(track.lane(0).minY, 100 + base)
        XCTAssertEqual(track.lane(2).maxY, 100 + base + 3 * HeaderLayout.laneHeight)
        func part(_ rect: CGRect) -> HeaderLayout.Part { track.part(at: CGPoint(x: rect.midX, y: rect.midY)) }
        XCTAssertEqual(part(track.laneName(1)), .lane(1))
        XCTAssertEqual(part(track.laneRange(2)), .lane(2))
        XCTAssertEqual(part(track.laneRemove(0)), .laneRemove(0))
        XCTAssertEqual(part(track.laneAdd), .laneAdd)
        XCTAssertEqual(part(track.sendBar(1)), .send(1), "the sends are above the lanes")
        XCTAssertLessThanOrEqual(track.laneAdd.maxY, 100 + track.height)
        // A row with its lanes shown and none yet has only the strip.
        let empty = HeaderLayout(kind: .bus, top: 300, lanes: 0)
        XCTAssertEqual(empty.height, TimelineLayout.busHeight + HeaderLayout.laneAddHeight)
        XCTAssertEqual(empty.part(at: CGPoint(x: empty.laneAdd.midX, y: empty.laneAdd.midY)), .laneAdd)
        XCTAssertEqual(HeaderLayout(kind: .bus, top: 300).height, TimelineLayout.busHeight)
    }

    func testAValueScaleMapsLevelsLinearlyAndFrequenciesInRatios() {
        let level = ValueScale(min: -60, max: 6, log: false)
        XCTAssertEqual(level.fraction(-27), 0.5, accuracy: 1e-9)
        XCTAssertEqual(level.fraction(-96), 0, "values the song allows outside the range are at its edge")
        XCTAssertEqual(level.value(at: 0.5), -27)
        XCTAssertEqual(level.stepped(-5.7499999), -5.7, "tenths of a dB")
        XCTAssertEqual(ValueScale(min: -1, max: 1, log: false).stepped(0.3049), 0.3, "hundredths of pan")
        let cutoff = ValueScale(min: 10, max: 20000, log: true)
        // Each octave is the same distance.
        XCTAssertEqual(cutoff.fraction(400) - cutoff.fraction(200), cutoff.fraction(8000) - cutoff.fraction(4000), accuracy: 1e-9)
        XCTAssertEqual(cutoff.value(at: 1), 20000)
        XCTAssertEqual(cutoff.value(at: 0), 10)
        XCTAssertEqual(cutoff.stepped(1234.56), 1230, "three figures")
        XCTAssertEqual(cutoff.stepped(87.654), 87.7)
        // In a lane, the top of the range is near the top, and a point goes back where it was put.
        let rect = CGRect(x: 0, y: 200, width: 500, height: 45)
        XCTAssertEqual(level.y(6, in: rect), 200 + ValueScale.inset)
        XCTAssertEqual(level.y(-60, in: rect), 245 - ValueScale.inset)
        XCTAssertEqual(level.value(atY: level.y(-12.3, in: rect), in: rect), -12.3)
        XCTAssertEqual(level.value(atY: 0, in: rect), 6)
        XCTAssertEqual(cutoff.value(atY: cutoff.y(1500, in: rect), in: rect), 1500)
        XCTAssertEqual(ValueScale.text(1500, unit: "Hz"), "1.50 kHz")
        XCTAssertEqual(ValueScale.text(87.7, unit: "Hz"), "87.7 Hz")
        XCTAssertEqual(ValueScale.text(-4.5, unit: "dB"), "−4.5 dB")
        XCTAssertEqual(ValueScale.text(35, unit: "%"), "35%")
        XCTAssertEqual(ValueScale.text(0.71, unit: ""), "0.71")
        XCTAssertEqual(ValueScale.text(4, unit: ":1"), "4.0:1")
    }

    func testAnAutomationPointSnapsToTheGridInsideTheSong() {
        var l = layout()
        XCTAssertEqual(l.snapped(9.7, free: false), 8, "a grid of bars")
        XCTAssertEqual(l.snapped(63.9, free: false), 64, "a point may be at the song's end")
        XCTAssertEqual(l.snapped(80, free: false), 64)
        XCTAssertEqual(l.snapped(-3, free: true), 0)
        XCTAssertEqual(l.snapped(9.70049, free: true), 9.7)
        l.pixelsPerBeat = 60
        XCTAssertEqual(l.snapped(9.7, free: false), 9.75)
    }

    func testLevelsFollowADragInSteps() {
        XCTAssertEqual(Fader.fraction(-60, in: Fader.gain), 0)
        XCTAssertEqual(Fader.fraction(6, in: Fader.gain), 1)
        XCTAssertEqual(Fader.fraction(-90, in: Fader.gain), 0, "the song allows less than the bar shows")
        XCTAssertEqual(Fader.fraction(-27, in: Fader.gain), 0.5, accuracy: 1e-9)
        // A quarter of a dB a point; a tenth of that with Shift.
        XCTAssertEqual(Fader.dragged(-6, byX: 10, perPoint: 0.25, fine: false, in: Fader.gain), -3.5)
        XCTAssertEqual(Fader.dragged(-6, byX: 10, perPoint: 0.25, fine: true, in: Fader.gain), -5.75, accuracy: 1e-9)
        XCTAssertEqual(Fader.dragged(-6, byX: 500, perPoint: 0.25, fine: false, in: Fader.gain), 6)
        XCTAssertEqual(Fader.dragged(-6, byX: -500, perPoint: 0.25, fine: false, in: Fader.gain), -60)
        XCTAssertEqual(Fader.stepped(-5.7499999, step: 0.1), -5.7)
        XCTAssertEqual(Fader.stepped(0.30000000000000004, step: 0.01), 0.3)
        XCTAssertEqual(Fader.stepped(-0.004, step: 0.01), 0)
        XCTAssertEqual(Fader.text(db: -4.5), "−4.5 dB")
        XCTAssertEqual(Fader.text(db: 0), "+0.0 dB")
        XCTAssertEqual(Fader.text(pan: 0), "C")
        XCTAssertEqual(Fader.text(pan: -0.25), "L25")
        XCTAssertEqual(Fader.text(pan: 1), "R100")
    }

    @MainActor
    func testUndoIsNamedForTheStepAndWhoMadeIt() {
        let title = SongWindowController.title
        XCTAssertEqual(title("Undo", nil), "Undo")
        XCTAssertEqual(title("Undo", HistoryStep(label: "Move clip beat at 16", origin: .user)), "Undo Move clip beat at 16")
        XCTAssertEqual(title("Undo", HistoryStep(label: "Move clip beat at 16", origin: .agent)), "Undo Agent: Move clip beat at 16")
        XCTAssertEqual(title("Redo", HistoryStep(label: "External edit of song.yaml", origin: .external)),
                       "Redo File Edit: External edit of song.yaml")
        let long = title("Undo", HistoryStep(label: String(repeating: "x", count: 200), origin: .user))
        XCTAssertEqual(long.count, "Undo ".count + 60)
        XCTAssertTrue(long.hasSuffix("…"))
    }

    func testScriptedInputReadsKeysAndClicks() {
        let launch = Launch(arguments: [
            "song.yaml", "--click", "10,20", "--shift-click", "30,40", "--double-click", "5,6",
            "--key", "shift+cmd+z", "--key", "delete", "--key", "left", "--type", "ab", "--wait", "2", "--key", "nonsense",
        ])
        XCTAssertEqual(launch.songs.map(\.lastPathComponent), ["song.yaml"])
        XCTAssertEqual(launch.actions.count, 9)
        XCTAssertEqual(launch.actions[0], .click(CGPoint(x: 10, y: 20)))
        XCTAssertEqual(launch.actions[1], .click(CGPoint(x: 30, y: 40), shift: true))
        XCTAssertEqual(launch.actions[2], .click(CGPoint(x: 5, y: 6), count: 2))
        guard case .key(let redo) = launch.actions[3], case .key(let delete) = launch.actions[4],
              case .key(let left) = launch.actions[5], case .key(let a) = launch.actions[6] else {
            return XCTFail("expected keys")
        }
        XCTAssertEqual(redo.characters, "z")
        XCTAssertEqual(redo.code, 6)
        XCTAssertEqual(redo.modifiers, [.shift, .command])
        XCTAssertEqual(delete.characters, "\u{7f}")
        XCTAssertEqual(delete.code, 51)
        XCTAssertEqual(left.code, 123)
        XCTAssertEqual(a.characters, "a")
        XCTAssertTrue(a.typed && !redo.typed)
        XCTAssertEqual(launch.actions[8], .wait(2))
        XCTAssertNil(launch.measure)
        // A run that times the drawing, and takes a picture after.
        let timed = Launch(arguments: ["song.yaml", "--measure", "/tmp/times.json", "--frames", "90", "--snapshot", "/tmp/window.png", "--size", "1200x700"])
        XCTAssertEqual(timed.measure?.lastPathComponent, "times.json")
        XCTAssertEqual(timed.frames, 90)
        XCTAssertEqual(timed.snapshot?.lastPathComponent, "window.png")
        XCTAssertEqual(timed.size, CGSize(width: 1200, height: 700))
        XCTAssertEqual(Launch(arguments: ["--measure", "/tmp/times.json"]).frames, 240)
        // A file let go at a point; its path may have commas.
        let dropped = Launch(arguments: ["--drop", "/music/One, Two.wav,400,120", "--drop", "nowhere.wav"])
        XCTAssertEqual(dropped.actions, [.drop(URL(fileURLWithPath: "/music/One, Two.wav"), CGPoint(x: 400, y: 120))])
    }

    func testDrawTimesAreSummedUp() {
        var times = DrawTimes()
        XCTAssertEqual(times.report["frames"] as? Int, 0)
        times.draws = [2, 4, 6, 8, 30]
        times.intervals = [16.6, 16.7, 16.8, 33.4]
        let report = times.report
        XCTAssertEqual(report["frames"] as? Int, 5)
        XCTAssertEqual(report["draws_over_16_7_ms"] as? Int, 1)
        XCTAssertEqual(report["draw_ms"] as? [String: Double], ["mean": 10, "median": 6, "p95": 30, "max": 30])
        XCTAssertEqual((report["interval_ms"] as? [String: Double])?["max"], 33.4)
    }

    @MainActor
    func testANumbersBarTakesItsRangeFromAField() {
        let field = FieldView(
            name: "cutoff_hz", label: "Cutoff", kind: .number, value: .number(value: 800), min: 10, max: 20000, unit: "Hz", log: true,
            choices: [], optional: false, initial: .number(value: 1000), live: true, param: "effects.0.cutoff_hz", lane: 12, band: nil
        )
        XCTAssertEqual(BarSpec(field), BarSpec(value: 800, min: 10, max: 20000, log: true, unit: "Hz", initial: 1000, live: true, dimmed: true))
        var off = field
        (off.value, off.initial, off.lane, off.live) = (.absent, .absent, nil, false)
        XCTAssertEqual(BarSpec(off), BarSpec(value: nil, min: 10, max: 20000, log: true, unit: "Hz", initial: nil, live: false, dimmed: false))
    }

    func testATypedValueIsReadInTheControlsUnit() {
        XCTAssertEqual(ValueScale.parse("800", unit: "Hz"), 800)
        XCTAssertEqual(ValueScale.parse(" 800 Hz ", unit: "Hz"), 800)
        XCTAssertEqual(ValueScale.parse("2.5k", unit: "Hz"), 2500)
        XCTAssertEqual(ValueScale.parse("2.50 kHz", unit: "Hz"), 2500, "as the bar shows it")
        XCTAssertEqual(ValueScale.parse("−6.0 dB", unit: "dB"), -6, "the minus a bar draws")
        XCTAssertEqual(ValueScale.parse("-6", unit: "dB"), -6)
        XCTAssertEqual(ValueScale.parse("50%", unit: "%"), 50)
        XCTAssertEqual(ValueScale.parse("4:1", unit: ":1"), 4)
        XCTAssertEqual(ValueScale.parse("0.25", unit: ""), 0.25)
        XCTAssertNil(ValueScale.parse("", unit: "dB"))
        XCTAssertNil(ValueScale.parse("loud", unit: "dB"))
        XCTAssertNil(ValueScale.parse("inf", unit: "dB"))
        XCTAssertNil(ValueScale.parse("12 Hz", unit: "dB"), "another unit")
        XCTAssertNil(ValueScale.parse("2k", unit: "ms"), "thousands are for frequencies")
    }

    func testATypedValueIsHeldToTheBarsRange() {
        let cutoff = BarSpec(value: 800, min: 10, max: 20000, log: true, unit: "Hz", initial: 1000)
        XCTAssertEqual(cutoff.typed("2.5k"), 2500)
        XCTAssertEqual(cutoff.typed("50000"), 20000)
        XCTAssertEqual(cutoff.typed("0"), 10)
        XCTAssertNil(cutoff.typed("x"))
        XCTAssertEqual(cutoff.typing(800), "800")
        XCTAssertEqual(cutoff.typing(180.5), "180.5")
        let velocity = BarSpec(value: 100, min: 1, max: 127, initial: 100, whole: true)
        XCTAssertEqual(velocity.typed("64.6"), 65)
        XCTAssertEqual(velocity.typed("200"), 127)
        XCTAssertEqual(velocity.typing(64), "64")
        let gain = BarSpec(value: -3.25, min: -36, max: 24, unit: "dB", initial: 0)
        XCTAssertEqual(gain.typed("−6 dB"), -6)
        XCTAssertEqual(gain.typing(-3.25), "-3.25")
    }
}
