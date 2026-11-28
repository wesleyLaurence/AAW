import AAWCore
import XCTest
@testable import AAWApp

/// The song's tempo edited in the app: steps on the master's tempo lane,
/// and an audio clip drawn through them.
final class TempoEditingTests: XCTestCase {
    /// A song at 120 that steps to 150 at bar 17.
    private let stepped = TempoMap([.init(beat: 0, seconds: 0, bpm: 120), .init(beat: 64, seconds: 32, bpm: 150)])

    private func lane(_ points: [(UInt64, Double, Double)]) -> LaneView {
        LaneView(key: 1, param: "tempo", label: "Tempo", unit: "BPM", min: 100, max: 170, log: false,
                 points: points.map { PointView(key: $0.0, at: $0.1, value: $0.2, hold: true, shape: 0) })
    }

    func testAStepLandsOnTheNearestBarLineOrFreeToAThousandth() {
        var l = TimelineLayout()
        l.lengthBeats = 64
        XCTAssertEqual(l.snapped(17.9, toBars: 4, free: false), 16)
        XCTAssertEqual(l.snapped(18.1, toBars: 4, free: false), 20)
        XCTAssertEqual(l.snapped(18.1234, toBars: 4, free: true), 18.123)
        XCTAssertEqual(l.snapped(-3, toBars: 4, free: false), 0)
        XCTAssertEqual(l.snapped(10.4, toBars: 3, free: false), 9, "bars of three beats in 3/4")
    }

    func testTheOpeningStepKeepsItsPlaceAndNoStepGoesOntoBeatZero() {
        // The opening one among them: only up and down.
        XCTAssertEqual(TimelineLayout.tempoRange(-16...32, steps: [0, 64], opening: true, least: 4), 0...0)
        // A step at bar 5 goes back as far as bar 2, a bar after the opening one.
        XCTAssertEqual(TimelineLayout.tempoRange(-16...32, steps: [16], opening: false, least: 4), -12...32)
        // Free, as near beat 0 as a thousandth.
        XCTAssertEqual(TimelineLayout.tempoRange(-16...32, steps: [16], opening: false, least: 0.001).lowerBound, -15.999, accuracy: 1e-9)
        // A point that stays nearer limits it as before.
        XCTAssertEqual(TimelineLayout.tempoRange(-8...32, steps: [16], opening: false, least: 4), -8...32)
        // Off a bar line before bar 2: it stays where it is in time rather than go back.
        XCTAssertEqual(TimelineLayout.tempoRange(-2...32, steps: [2], opening: false, least: 4), 0...32)
    }

    func testTheOpeningStepGoesOnlyWithEveryOtherAndLast() {
        let tempo = lane([(10, 0, 120), (11, 64, 150), (12, 96, 140)])
        XCTAssertEqual(SongModel.removal(of: [10, 11], tempo: tempo), [11], "the opening step stays while another does")
        XCTAssertEqual(SongModel.removal(of: [10], tempo: tempo), [])
        XCTAssertEqual(SongModel.removal(of: [12, 10, 11], tempo: tempo), [11, 12, 10], "last, taking the lane")
        XCTAssertEqual(SongModel.removal(of: [11, 40], tempo: tempo), [11, 40], "other lanes' points as they are")
        XCTAssertEqual(SongModel.removal(of: [10], tempo: lane([(10, 0, 120)])), [10], "a lane of one tempo goes")
        XCTAssertEqual(SongModel.removal(of: [7, 3], tempo: nil), [3, 7])
    }

    func testTheTypedTempoSetsTheStepThePlayheadIsIn() {
        let tempo = lane([(10, 0, 120), (11, 64, 150)])
        XCTAssertEqual(SongModel.step(of: tempo, at: 0)?.key, 10)
        XCTAssertEqual(SongModel.step(of: tempo, at: 63.99)?.key, 10)
        XCTAssertEqual(SongModel.step(of: tempo, at: 64)?.key, 11)
        XCTAssertEqual(SongModel.step(of: tempo, at: 200)?.key, 11)
    }

    func testTheTempoIsCutIntoAPieceAStep() {
        let pieces = stepped.pieces(from: 60, to: 70)
        XCTAssertEqual(pieces.map(\.from), [60, 64])
        XCTAssertEqual(pieces.map(\.to), [64, 70])
        XCTAssertEqual(pieces.map(\.bpm), [120, 150])
        XCTAssertEqual(stepped.pieces(from: 70, to: 80).map(\.bpm), [150])
        XCTAssertEqual(stepped.pieces(from: 0, to: 8).map(\.to), [8])
        XCTAssertTrue(stepped.pieces(from: 8, to: 8).isEmpty)
    }

    /// A clip at bar 1 that plays its file at its own speed: after the step
    /// a beat of the song is 0.4 s of the file, not 0.5 s.
    func testAClipAtItsOwnSpeedPlaysItsFileThroughTheSteps() {
        let time = ClipTime(map: stepped, beat: 0, seconds: 0, secondsPerBeat: 0.5, clipBeat: 0, follows: false)
        XCTAssertTrue(time.stepped)
        XCTAssertEqual(time.seconds(atBeat: 64), 32, accuracy: 1e-9)
        XCTAssertEqual(time.seconds(atBeat: 74), 36, accuracy: 1e-9)
        XCTAssertEqual(time.beat(atSeconds: 36), 74, accuracy: 1e-9)
        let pieces = time.pieces(from: 60, to: 70)
        XCTAssertEqual(pieces.map(\.seconds), [30, 32])
        XCTAssertEqual(pieces.map(\.secondsPerBeat), [0.5, 0.4])
        // A clip repitched up an octave plays twice as much of its file.
        let up = ClipTime(map: stepped, beat: 0, seconds: 0, secondsPerBeat: 1, clipBeat: 0, follows: false)
        XCTAssertEqual(up.seconds(atBeat: 74), 72, accuracy: 1e-9)
        // A clip that starts after the step, 3 s into its file.
        let late = ClipTime(map: stepped, beat: 80, seconds: 3, secondsPerBeat: 0.4, clipBeat: 80, follows: false)
        XCTAssertEqual(late.seconds(atBeat: 90), 7, accuracy: 1e-9)
    }

    func testAClipThatFollowsTheSongsTempoOrASongOfOneTempoIsOnePiece() {
        let follows = ClipTime(map: stepped, beat: 0, seconds: 0, secondsPerBeat: 0.5, clipBeat: 0, follows: true)
        XCTAssertFalse(follows.stepped)
        XCTAssertEqual(follows.seconds(atBeat: 74), 37, accuracy: 1e-9)
        XCTAssertEqual(follows.pieces(from: 60, to: 70).count, 1)
        let one = ClipTime(map: .constant(120), beat: 4, seconds: 1, secondsPerBeat: 0.5, clipBeat: 4, follows: false)
        XCTAssertFalse(one.stepped)
        XCTAssertEqual(one.seconds(atBeat: 8), 3, accuracy: 1e-9)
        XCTAssertEqual(one.beat(atSeconds: 3), 8, accuracy: 1e-9)
    }

    func testOneAudioClipsMenuHasTempoFollowThisClip() {
        let items = ContextMenu.clip(editor: "Audio Clip", canSplit: false, canJoin: false, follow: true)
        let tempo = items.first { $0.title == "Tempo" }
        XCTAssertEqual(tempo?.children?.map(\.title), ["Follow This Clip"])
        XCTAssertEqual(tempo?.children?.first?.action, .followTempo)
        XCTAssertEqual(items.last?.title, "Delete")
        let busy = ContextMenu.clip(editor: "Audio Clip", canSplit: false, canJoin: false, follow: false)
        XCTAssertEqual(busy.first { $0.title == "Tempo" }?.children?.first?.enabled, false, "while one is measured")
        XCTAssertNil(ContextMenu.clip(editor: "Notes", canSplit: false, canJoin: false).first { $0.title == "Tempo" })
    }
}
