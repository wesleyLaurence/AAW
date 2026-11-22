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

    func testTheLevelDrawnHasFourBucketsForEachColumn() {
        let w = waveform()
        XCTAssertEqual(w.level(forColumn: 200)?.framesPerBucket, 40)
        XCTAssertEqual(w.level(forColumn: 160)?.framesPerBucket, 40)
        XCTAssertEqual(w.level(forColumn: 159)?.framesPerBucket, 10)
        XCTAssertEqual(w.level(forColumn: 40)?.framesPerBucket, 10)
        XCTAssertEqual(w.level(forColumn: 39)?.framesPerBucket, 10, "fewer than four of the finest")
        XCTAssertEqual(w.level(forColumn: 2)?.framesPerBucket, 10, "zoomed in past the finest, buckets repeat")
        XCTAssertNil(Waveform(identity: 0, framesPerBeat: 100, levels: []).level(forColumn: 10))
    }

    func testARangeIsTheLeastAndGreatestOfItsBuckets() {
        let fine = waveform().levels[0]
        var r = Waveform.range(fine, from: 20, to: 50)
        XCTAssertEqual([r.lo, r.hi], [-32.0 / 127, 16.0 / 127])
        r = Waveform.range(fine, from: 25, to: 26)
        XCTAssertEqual([r.lo, r.hi], [-16.0 / 127, 8.0 / 127], "inside one bucket")
        // A bucket counts where its middle falls: frames 24 to 46 hold the
        // middles of buckets 2, 3 and 4, and to 44 those of 2 and 3.
        r = Waveform.range(fine, from: 24, to: 46)
        XCTAssertEqual([r.lo, r.hi], [-32.0 / 127, 16.0 / 127])
        r = Waveform.range(fine, from: 24, to: 44)
        XCTAssertEqual([r.lo, r.hi], [-24.0 / 127, 12.0 / 127])
        r = Waveform.range(fine, from: 31, to: 34)
        XCTAssertEqual([r.lo, r.hi], [-24.0 / 127, 12.0 / 127], "no middle inside: the bucket of its own middle")
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

    func testColumnsStandOnTheViewsPixelsWhereverTheClipStarts() {
        let w = waveform()
        // A clip from a fractional point, drawn in device pixels of half a point.
        let rect = CGRect(x: 100.3, y: 10, width: 2, height: 40)
        let columns = w.columns(in: rect, clippedTo: rect, fromFrame: 0, framesPerPoint: 40, step: 0.5)
        XCTAssertEqual(columns.count, 5)
        // Cut at the clip's start and end, and on the pixels between.
        for (column, edges) in zip(columns, [(100.3, 100.5), (100.5, 101), (101, 101.5), (101.5, 102), (102, 102.3)]) {
            XCTAssertEqual(column.minX, edges.0, accuracy: 1e-9)
            XCTAssertEqual(column.maxX, edges.1, accuracy: 1e-9)
        }
        // Each column is of the frames under it: the second, from 100.5, is
        // frames 8 to 28, the middles of buckets 1 and 2, from -16 to 8.
        XCTAssertEqual(columns[1].height, 24.0 / 127 * 20, accuracy: 1e-9)
        // Cut by the view, the columns that show are the same ones.
        let cut = w.columns(in: rect, clippedTo: CGRect(x: 100.7, y: 0, width: 1, height: 100), fromFrame: 0, framesPerPoint: 40, step: 0.5)
        XCTAssertEqual(cut, Array(columns[1...3]))
    }

    func testAClickIsOneColumnAtEveryZoom() {
        // One loud bucket of ten frames among silence, at both levels.
        var fine = [(Int8, Int8)](repeating: (0, 0), count: 64)
        fine[20] = (-100, 100)
        var coarse = [(Int8, Int8)](repeating: (0, 0), count: 16)
        coarse[5] = (-100, 100)
        let w = Waveform(identity: 1, framesPerSecond: 100, levels: [level(10, fine), level(40, coarse)])
        for framesPerColumn in [10.0, 13, 20, 39, 40, 41, 80, 120, 159, 160, 161, 240, 300] {
            for origin in [0.0, 0.25, 0.3, 0.5, 0.71] {
                let rect = CGRect(x: origin, y: 0, width: 640 / framesPerColumn * 0.5 + 1, height: 100)
                let columns = w.columns(in: rect, clippedTo: rect, fromFrame: 0, framesPerPoint: framesPerColumn / 0.5, step: 0.5)
                let loud = columns.filter { $0.height > 1 }
                XCTAssertEqual(loud.count, 1, "\(framesPerColumn) frames a column from \(origin)")
            }
        }
    }

    func testTheStoreKeepsThePeaksOfTheLatestRevision() {
        var store = WaveformStore()
        let peaks = { (id: UInt64) in TrackPeaks(identity: id, framesPerBeat: 100, frames: 160, levels: []) }
        XCTAssertNil(store.waveform(of: 1, at: 0))
        store.apply(Waveforms(revision: 0, tracks: [TrackWave(track: 1, identity: 10), TrackWave(track: 2, identity: 20)], peaks: [peaks(10)], files: [], filePeaks: []))
        XCTAssertEqual(store.waveform(of: 1, at: 0)?.identity, 10)
        XCTAssertNil(store.waveform(of: 2, at: 0), "still being worked out")
        XCTAssertFalse(store.complete)
        store.apply(Waveforms(revision: 0, tracks: [TrackWave(track: 1, identity: 10), TrackWave(track: 2, identity: 20)], peaks: [peaks(20)], files: [], filePeaks: []))
        XCTAssertTrue(store.complete)
        XCTAssertEqual(store.waveform(of: 2, at: 0)?.identity, 20)
        // The song has moved on: what is here is of an older revision.
        XCTAssertNil(store.waveform(of: 1, at: 1))
        // A revision that changes one track's audio keeps the other's peaks and drops the old ones.
        store.apply(Waveforms(revision: 1, tracks: [TrackWave(track: 1, identity: 10), TrackWave(track: 2, identity: 21)], peaks: [],
                              files: [], filePeaks: []))
        XCTAssertEqual(store.waveform(of: 1, at: 1)?.identity, 10)
        XCTAssertNil(store.waveform(of: 2, at: 1))
        XCTAssertEqual(store.peaks.keys.sorted(), [10])
        XCTAssertNil(store.waveform(of: 3, at: 1), "no such track")
    }

    func testTheStoreKeepsAFilesPeaksWhileAClipPlaysIt() {
        var store = WaveformStore()
        let file = FilePeaks(identity: 5, framesPerSecond: 48000, frames: 96000, levels: [])
        store.apply(Waveforms(revision: 3, tracks: [], peaks: [], files: [5, 6], filePeaks: [file]))
        XCTAssertEqual(store.file(5)?.framesPerSecond, 48000)
        XCTAssertNil(store.file(6), "still being read")
        XCTAssertTrue(store.complete, "tracks are what a revision waits for")
        // A file's peaks hold for any revision: they are of the file, not of the song.
        store.apply(Waveforms(revision: 4, tracks: [], peaks: [], files: [5], filePeaks: []))
        XCTAssertEqual(store.file(5)?.identity, 5)
        // When no clip plays it any more they go; the host sends them again.
        store.apply(Waveforms(revision: 5, tracks: [], peaks: [], files: [], filePeaks: []))
        XCTAssertNil(store.file(5))
    }

    func testAnAudioClipDrawsItsFileFromWhereItStartsAtItsLevel() {
        // A file of 100 frames a second, drawn at 2 frames a point from its frame 20.
        let w = Waveform(identity: 7, framesPerSecond: 100, levels: waveform().levels)
        let rect = CGRect(x: 100, y: 10, width: 50, height: 40)
        let plain = w.columns(in: rect, clippedTo: rect, fromFrame: 20, framesPerPoint: 2, step: 5)
        XCTAssertEqual(plain, waveform().columns(in: rect, clippedTo: rect, fromBeat: 0.2, pixelsPerBeat: 50, step: 5),
                       "as a track's from that beat")
        // Twice the level is twice the height, about the middle.
        let loud = w.columns(in: rect, clippedTo: rect, fromFrame: 20, framesPerPoint: 2, step: 5, gain: 2)
        XCTAssertEqual(loud[2].height, 2 * plain[2].height, accuracy: 1e-9)
        XCTAssertEqual(loud[2].minY, 30 - 32.0 / 127 * 20, accuracy: 1e-9)
        // Louder than full scale is drawn as full scale, inside the clip.
        let clipped = w.columns(in: rect, clippedTo: rect, fromFrame: 20, framesPerPoint: 2, step: 5, gain: 100)
        XCTAssertEqual(clipped[2], CGRect(x: 110, y: 10, width: 5, height: 40))
        XCTAssertTrue(w.columns(in: rect, clippedTo: rect, fromFrame: 0, framesPerPoint: 0, step: 5).isEmpty)
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

    func testTheArrowKeysWalkTheBrowsersSamples() {
        let results = ["a", "b", "c"].map {
            SampleInfo(id: $0, name: "\($0).wav", pack: "Pack", path: "/library/\($0).wav", seconds: 0.5, channels: 2,
                       category: "kick", kind: "one-shot", bpm: nil, key: nil, note: nil, rootNote: nil)
        }
        XCTAssertEqual(Browser.neighbor(of: "a", in: results, by: 1)?.id, "b")
        XCTAssertEqual(Browser.neighbor(of: "b", in: results, by: -1)?.id, "a")
        XCTAssertNil(Browser.neighbor(of: "c", in: results, by: 1), "the last stays")
        XCTAssertNil(Browser.neighbor(of: "a", in: results, by: -1), "the first stays")
        XCTAssertEqual(Browser.neighbor(of: nil, in: results, by: 1)?.id, "a", "from nothing, the first")
        XCTAssertEqual(Browser.neighbor(of: nil, in: results, by: -1)?.id, "a")
        XCTAssertEqual(Browser.neighbor(of: "gone", in: results, by: 1)?.id, "a", "a selection the search dropped")
        XCTAssertNil(Browser.neighbor(of: nil, in: [], by: 1))
    }
}
