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
        XCTAssertEqual(track.part(at: CGPoint(x: 150, y: 110)), .name, "the name reaches to the buttons")
        XCTAssertEqual(track.part(at: CGPoint(x: 207, y: 130)), .body, "beside the pan")
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
    }
}
