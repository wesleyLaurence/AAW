import AAWCore
import XCTest
@testable import AAWApp

final class AudioClipLayoutTests: XCTestCase {
    /// A clip of seconds 4 to 20 of a 40-second file on beat 8 of a song at
    /// 120 BPM, where a beat is half a second: 32 beats, with fades of one
    /// beat and of two, the second after the clip leaves.
    private func clip() -> AudioClipView {
        AudioClipView(
            key: 9, reference: "@9", sample: "song", at: 8, lengthBeats: 32, tailBeats: 2, sourceStartSeconds: 4,
            sourceEndSeconds: 20, secondsPerBeat: 0.5, gainDb: 0, fadeInMs: 500, fadeOutMs: 1000, fadeCurve: "equal_power",
            sourceBpm: nil, stretch: "repitch", file: 3
        )
    }

    func testAClipIsDrawnToWhereItsSoundEndsWithItsFadesInside() {
        let shape = AudioClipLayout(clip(), tempo: 120, seconds: 40)
        // It leaves at beat 40 and fades for two more.
        XCTAssertEqual([shape.start, shape.end, shape.length], [8, 42, 34])
        XCTAssertEqual([shape.fadeIn, shape.fadeOut], [1, 2])
        // Its file starts eight beats before it and is eighty beats long.
        XCTAssertEqual(shape.fileStart, 0)
        XCTAssertEqual(shape.fileEnd, 80)
        // A file that cannot be read has no end to stop at.
        XCTAssertNil(AudioClipLayout(clip(), tempo: 120, seconds: nil).fileEnd)
        // A fade longer than the clip is drawn as long as the clip.
        var short = clip()
        (short.lengthBeats, short.tailBeats, short.fadeInMs) = (1, 0, 5000)
        XCTAssertEqual(AudioClipLayout(short, tempo: 120, seconds: 40).fadeIn, 1)
        XCTAssertEqual(AudioClipLayout.beats(ms: 250, tempo: 120), 0.5)
        XCTAssertEqual(AudioClipLayout.ms(beats: 0.5, tempo: 120), 250)
        XCTAssertEqual(AudioClipLayout.ms(beats: 1, tempo: 0), 0)
    }

    func testAPressTakesAnEdgeAHandleOrTheClip() {
        let shape = AudioClipLayout(clip(), tempo: 120, seconds: 40)
        // 10 points a beat: the handles are a beat in and two beats from the end.
        let body = CGRect(x: 100, y: 50, width: 340, height: 27)
        let handles = shape.handles(in: body)
        XCTAssertEqual([handles.fadeIn, handles.fadeOut], [CGPoint(x: 110, y: 50), CGPoint(x: 420, y: 50)])
        XCTAssertEqual(shape.part(at: CGPoint(x: 111, y: 54), in: body), .fadeIn)
        XCTAssertEqual(shape.part(at: CGPoint(x: 416, y: 52), in: body), .fadeOut)
        XCTAssertEqual(shape.part(at: CGPoint(x: 111, y: 70), in: body), .body, "under the handles the clip moves")
        XCTAssertEqual(shape.part(at: CGPoint(x: 130, y: 54), in: body), .body, "and beside them")
        XCTAssertEqual(shape.part(at: CGPoint(x: 102, y: 70), in: body), .start)
        XCTAssertEqual(shape.part(at: CGPoint(x: 438, y: 70), in: body), .end)
        XCTAssertEqual(shape.part(at: CGPoint(x: 102, y: 40), in: body), .start, "the title strip's ends trim too")
        XCTAssertEqual(shape.part(at: CGPoint(x: 270, y: 60), in: body), .body)
        // With no fades the handles are in the corners, over the top of each edge.
        var plain = shape
        (plain.fadeIn, plain.fadeOut) = (0, 0)
        XCTAssertEqual(plain.part(at: CGPoint(x: 102, y: 54), in: body), .fadeIn)
        XCTAssertEqual(plain.part(at: CGPoint(x: 438, y: 54), in: body), .fadeOut)
        XCTAssertEqual(plain.part(at: CGPoint(x: 102, y: 70), in: body), .start)
        // A clip too narrow for its parts only moves.
        XCTAssertEqual(plain.part(at: CGPoint(x: 101, y: 54), in: CGRect(x: 100, y: 50, width: 12, height: 27)), .body)
    }

    func testAnEdgeGoesNoFurtherThanTheFileAndAFadeNoFurtherThanTheOther() {
        let shape = AudioClipLayout(clip(), tempo: 120, seconds: 40)
        XCTAssertEqual(shape.start(draggedTo: 4), 4)
        XCTAssertEqual(shape.start(draggedTo: -3), 0, "the file starts on beat 0, which is the song's start too")
        XCTAssertEqual(shape.start(draggedTo: 60), 40 - AudioClipLayout.least, "short of where its fade out begins")
        XCTAssertEqual(shape.end(draggedTo: 60), 60)
        XCTAssertEqual(shape.end(draggedTo: 300), 80, "where the file ends")
        XCTAssertEqual(shape.end(draggedTo: 2), 8 + AudioClipLayout.least)
        // A file that starts before the song: the clip can start no earlier than the song.
        var early = shape
        early.fileStart = -6
        XCTAssertEqual(early.start(draggedTo: -3), 0)
        // An unread file leaves the end where it is at the most.
        var unread = shape
        unread.fileEnd = nil
        XCTAssertEqual(unread.end(draggedTo: 60), 42)
        // Fades: from the edge to the handle, and never past the other fade.
        XCTAssertEqual(shape.fadeIn(draggedTo: 12), 4)
        XCTAssertEqual(shape.fadeIn(draggedTo: 2), 0)
        XCTAssertEqual(shape.fadeIn(draggedTo: 100), 32, "up to where the fade out begins")
        XCTAssertEqual(shape.fadeOut(draggedTo: 30), 12)
        XCTAssertEqual(shape.fadeOut(draggedTo: 50), 0)
        XCTAssertEqual(shape.fadeOut(draggedTo: -5), 33)
        // With no fade in, a fade out is still short of the whole clip.
        var plain = shape
        plain.fadeIn = 0
        XCTAssertEqual(plain.fadeOut(draggedTo: -5), 34 - AudioClipLayout.least)
        XCTAssertEqual(plain.fadeOut(draggedTo: 90), 0)
    }

    func testAFadesCurveIsTheOneTheEnginePlays() {
        XCTAssertEqual(AudioClipLayout.level(0.5, linear: true), 0.5)
        XCTAssertEqual(AudioClipLayout.level(0.5, linear: false), sin(Double.pi / 4), accuracy: 1e-12)
        XCTAssertEqual([AudioClipLayout.level(-1, linear: false), AudioClipLayout.level(2, linear: false)], [0, 1])
    }

    func testAShapedSegmentBendsAsTheEngineBendsIt() {
        // The engine's own figures: halfway, a shape of 0.5 is the square and
        // 1 the fourth power; below zero it is the mirror.
        XCTAssertEqual(shapedProgress(0.5, shape: 0), 0.5)
        XCTAssertEqual(shapedProgress(0.5, shape: 0.5), 0.25, accuracy: 1e-12)
        XCTAssertEqual(shapedProgress(0.5, shape: 1), 0.0625, accuracy: 1e-12)
        XCTAssertEqual(shapedProgress(0.5, shape: -0.5), 0.75, accuracy: 1e-12)
        XCTAssertEqual(shapedProgress(0.5, shape: -1), 0.9375, accuracy: 1e-12)
        for shape in [-1, -0.3, 0.3, 1] {
            XCTAssertEqual([shapedProgress(0, shape: shape), shapedProgress(1, shape: shape)], [0, 1], "it starts and ends on its points")
            XCTAssertLessThan(shapedProgress(0.3, shape: shape), shapedProgress(0.6, shape: shape))
        }
        XCTAssertEqual(shapedProgress(7, shape: 0.5), 1)
    }

    func testNumbersAreWrittenPlainly() {
        XCTAssertEqual(ValueScale.plain(44), "44")
        XCTAssertEqual(ValueScale.plain(6.5), "6.5")
        XCTAssertEqual(ValueScale.plain(25.0 / 3), "8.333")
        XCTAssertEqual(ValueScale.plain(120.25), "120.25")
    }
}
