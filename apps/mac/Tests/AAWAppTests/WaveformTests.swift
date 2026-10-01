import AAWCore
import XCTest
@testable import AAWApp

final class WaveformTests: XCTestCase {
    /// A level whose buckets hold these least and greatest samples.
    private func level(_ framesPerBucket: Double, _ buckets: [(Int8, Int8)]) -> Waveform.Level {
        Waveform.Level(framesPerBucket: framesPerBucket,
                       data: Data(buckets.flatMap { [UInt8(bitPattern: $0.0), UInt8(bitPattern: $0.1)] }))
    }

    /// 100 frames to a beat: buckets of 10 frames, and of 40.
    private func waveform() -> Waveform {
        var fine = [(Int8, Int8)](repeating: (0, 0), count: 16)
        for i in 0..<8 { fine[i] = (Int8(-8 * i), Int8(4 * i)) }
        let coarse: [(Int8, Int8)] = [(-24, 12), (-56, 28), (0, 0), (0, 0)]
        return Waveform(identity: 7, framesPerBeat: 100, levels: [level(10, fine), level(40, coarse)])
    }

    func testTheLevelDrawnHasABucketForEachColumn() {
        let w = waveform()
        XCTAssertEqual(w.level(forColumn: 100)?.framesPerBucket, 40)
        XCTAssertEqual(w.level(forColumn: 40)?.framesPerBucket, 40)
        XCTAssertEqual(w.level(forColumn: 39)?.framesPerBucket, 10)
        XCTAssertEqual(w.level(forColumn: 2)?.framesPerBucket, 10, "zoomed in past the finest, buckets repeat")
        XCTAssertNil(Waveform(identity: 0, framesPerBeat: 100, levels: []).level(forColumn: 10))
    }

    func testARangeIsTheLeastAndGreatestOfItsBuckets() {
        let fine = waveform().levels[0]
        var r = Waveform.range(fine, from: 20, to: 50)
        XCTAssertEqual([r.lo, r.hi], [-32.0 / 127, 16.0 / 127])
        r = Waveform.range(fine, from: 25, to: 26)
        XCTAssertEqual([r.lo, r.hi], [-16.0 / 127, 8.0 / 127], "inside one bucket")
        r = Waveform.range(fine, from: 100, to: 150)
        XCTAssertEqual([r.lo, r.hi], [0, 0])
        // Before the song and past what the peaks cover there is silence.
        r = Waveform.range(fine, from: -50, to: -10)
        XCTAssertEqual([r.lo, r.hi], [0, 0])
        r = Waveform.range(fine, from: 500, to: 600)
        XCTAssertEqual([r.lo, r.hi], [0, 0])
    }

    func testColumnsFillAClipFromItsStartAndOnlyWhereItShows() {
        let w = waveform()
        // A clip of one beat from beat 0.2, 50 points wide and 40 high: 2 frames a point.
        let rect = CGRect(x: 100, y: 10, width: 50, height: 40)
        let all = w.columns(in: rect, clippedTo: rect, fromBeat: 0.2, pixelsPerBeat: 50, step: 5)
        XCTAssertEqual(all.count, 10)
        XCTAssertEqual(all.map(\.minX), stride(from: 100.0, to: 150, by: 5).map { CGFloat($0) })
        // A column is ten frames. The third is frames 40 to 50: bucket 4, from -32 to 16 of 127.
        XCTAssertEqual(all[2].minY, 30 - 16.0 / 127 * 20, accuracy: 1e-9)
        XCTAssertEqual(all[2].height, 48.0 / 127 * 20, accuracy: 1e-9)
        // The first, from -16 to 8, is as tall as a column is wide at the least.
        XCTAssertEqual(all[0].height, 5)
        // Silence is a line one column's step tall, on the middle.
        XCTAssertEqual(all[9], CGRect(x: 145, y: 30, width: 5, height: 5))
        // Cut by the view, a clip draws the columns that show, where they were.
        let cut = w.columns(in: rect, clippedTo: CGRect(x: 118, y: 0, width: 14, height: 100), fromBeat: 0.2, pixelsPerBeat: 50, step: 5)
        XCTAssertEqual(cut, Array(all[3...6]))
        XCTAssertTrue(w.columns(in: rect, clippedTo: CGRect(x: 500, y: 0, width: 10, height: 100), fromBeat: 0, pixelsPerBeat: 50, step: 5).isEmpty)
        // Zoomed out, a column takes the coarser level's buckets.
        let far = w.columns(in: CGRect(x: 0, y: 0, width: 4, height: 254), clippedTo: .infinite, fromBeat: 0, pixelsPerBeat: 2.5, step: 1)
        XCTAssertEqual(far.count, 4)
        for (column, height) in zip(far, [36.0, 84, 1, 1]) { XCTAssertEqual(column.height, height, accuracy: 1e-9) }
    }

    func testTheStoreKeepsThePeaksOfTheLatestRevision() {
        var store = WaveformStore()
        let peaks = { (id: UInt64) in TrackPeaks(identity: id, framesPerBeat: 100, frames: 160, levels: []) }
        XCTAssertNil(store.waveform(of: 1, at: 0))
        store.apply(Waveforms(revision: 0, tracks: [TrackWave(track: 1, identity: 10), TrackWave(track: 2, identity: 20)], peaks: [peaks(10)]))
        XCTAssertEqual(store.waveform(of: 1, at: 0)?.identity, 10)
        XCTAssertNil(store.waveform(of: 2, at: 0), "still being worked out")
        XCTAssertFalse(store.complete)
        store.apply(Waveforms(revision: 0, tracks: [TrackWave(track: 1, identity: 10), TrackWave(track: 2, identity: 20)], peaks: [peaks(20)]))
        XCTAssertTrue(store.complete)
        XCTAssertEqual(store.waveform(of: 2, at: 0)?.identity, 20)
        // The song has moved on: what is here is of an older revision.
        XCTAssertNil(store.waveform(of: 1, at: 1))
        // A revision that changes one track's audio keeps the other's peaks and drops the old ones.
        store.apply(Waveforms(revision: 1, tracks: [TrackWave(track: 1, identity: 10), TrackWave(track: 2, identity: 21)], peaks: []))
        XCTAssertEqual(store.waveform(of: 1, at: 1)?.identity, 10)
        XCTAssertNil(store.waveform(of: 2, at: 1))
        XCTAssertEqual(store.peaks.keys.sorted(), [10])
        XCTAssertNil(store.waveform(of: 3, at: 1), "no such track")
    }

    func testTheBrowserNamesAPadAfterItsSample() {
        let sample = { (name: String, category: String) in
            SampleInfo(id: "1", name: name, pack: "Pack", path: "/library/\(name)", seconds: 0.5, channels: 2, category: category,
                       kind: "one-shot", bpm: nil, key: nil, note: nil, rootNote: nil)
        }
        XCTAssertEqual(Browser.padName(of: sample("BBL_kick_dana.wav", "kick")), "kick")
        XCTAssertEqual(Browser.padName(of: sample("Rhodes Chord 03.wav", "other")), "Rhodes Chord 03")
        var loop = sample("top_loop_120_Am.wav", "other")
        (loop.seconds, loop.bpm, loop.key, loop.note) = (16, 120, "Am", "A2")
        XCTAssertEqual(Browser.detail(of: loop), "16 s · 120 BPM · Am · ♪ A2")
        XCTAssertEqual(Browser.detail(of: sample("x.wav", "kick")), "0.50 s")
    }
}
