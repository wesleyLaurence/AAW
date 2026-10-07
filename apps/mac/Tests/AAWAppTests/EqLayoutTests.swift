@testable import AAWApp
import AAWCore
import XCTest

final class EqLayoutTests: XCTestCase {
    let box = CGRect(x: 0, y: 0, width: 300, height: 120)
    let rate = 48000.0

    private func band(_ shape: String, _ hz: Double, gain: Double = 0, q: Double = 0.71, slope: Int = 12) -> EqLayout.Band {
        EqLayout.Band(shape: shape, freqHz: hz, gainDb: gain, q: q, slope: slope)
    }

    func testEachShapeRespondsAsTheEngineDoes() {
        // A bell is its gain at its frequency and nothing far from it.
        let bell = band("bell", 1000, gain: 6, q: 1)
        XCTAssertEqual(EqLayout.responseDb(bell, hz: 1000, rate: rate), 6, accuracy: 0.01)
        XCTAssertEqual(EqLayout.responseDb(bell, hz: 20, rate: rate), 0, accuracy: 0.05)
        // Shelves hold their gain on their side and nothing on the other.
        XCTAssertEqual(EqLayout.responseDb(band("low_shelf", 200, gain: -9), hz: 30, rate: rate), -9, accuracy: 0.2)
        XCTAssertEqual(EqLayout.responseDb(band("low_shelf", 200, gain: -9), hz: 5000, rate: rate), 0, accuracy: 0.1)
        XCTAssertEqual(EqLayout.responseDb(band("high_shelf", 5000, gain: 4), hz: 15000, rate: rate), 4, accuracy: 0.3)
        // A flat highpass is −3 dB at its corner and falls at its slope, two
        // octaves down about twice the slope; a pass ignores its gain.
        for slope in [12, 24, 36, 48] {
            let flat = band("highpass", 400, gain: 12, q: 1 / 2.squareRoot(), slope: slope)
            XCTAssertEqual(EqLayout.responseDb(flat, hz: 400, rate: rate), -3.01, accuracy: 0.02, "slope \(slope)")
            XCTAssertEqual(EqLayout.responseDb(flat, hz: 100, rate: rate), -2 * Double(slope), accuracy: 0.3 + Double(slope) / 48, "slope \(slope)")
            XCTAssertEqual(EqLayout.responseDb(flat, hz: 8000, rate: rate), 0, accuracy: 0.01, "slope \(slope)")
            XCTAssertEqual(EqLayout.cornerDb(flat), -3.01, accuracy: 0.02)
        }
        XCTAssertEqual(EqLayout.responseDb(band("lowpass", 1000, slope: 24), hz: 4000, rate: rate), -48, accuracy: 1)
        // Resonance lifts the corner to the product of the sections' Qs: q itself at 12 dB.
        XCTAssertEqual(EqLayout.cornerDb(band("highpass", 300, q: 2)), 6.02, accuracy: 0.01)
        XCTAssertEqual(EqLayout.responseDb(band("highpass", 300, q: 2), hz: 300, rate: rate), 6.02, accuracy: 0.02)
        XCTAssertEqual(EqLayout.passQ(cornerDb: EqLayout.cornerDb(band("lowpass", 300, q: 1.3, slope: 48)), slope: 48), 1.3, accuracy: 1e-9)
        // The curve is the bands together.
        let bands = [bell, band("highpass", 400, slope: 24)]
        XCTAssertEqual(EqLayout.curveDb(bands, hz: 1000, rate: rate), EqLayout.responseDb(bell, hz: 1000, rate: rate) + EqLayout.responseDb(bands[1], hz: 1000, rate: rate), accuracy: 1e-9)
    }

    /// A band's field as the host would describe it, with only what the layout reads.
    private func field(_ band: Int, _ name: String, _ value: FieldValue) -> FieldView {
        FieldView(name: "bands.\(band).\(name)", label: name, kind: .number, value: value, min: 0, max: 0, unit: "", log: false,
                  choices: [], optional: false, initial: value, live: true, param: nil, lane: nil, band: UInt32(band))
    }

    func testBandsAreReadFromTheFieldsInOrder() {
        var fields: [FieldView] = []
        fields.append(field(0, "shape", .text(value: "high_shelf")))
        fields.append(field(0, "freq_hz", .number(value: 5000)))
        fields.append(field(0, "gain_db", .number(value: 2)))
        fields.append(field(0, "q", .number(value: 0.9)))
        fields.append(field(0, "slope_db_per_octave", .number(value: 12)))
        fields.append(field(1, "shape", .text(value: "lowpass")))
        fields.append(field(1, "freq_hz", .number(value: 12000)))
        fields.append(field(1, "slope_db_per_octave", .number(value: 48)))
        let bands = EqLayout.Band.bands(of: fields)
        XCTAssertEqual(bands, [band("high_shelf", 5000, gain: 2, q: 0.9), band("lowpass", 12000, q: 0.71, slope: 48)])
        XCTAssertEqual(bands.map { $0.sections }, [1, 4])
    }

    func testPointsSitAtTheGainOrTheCornerAndReadBackADrag() {
        let bell = band("bell", 1000, gain: 6)
        let p = EqLayout.point(bell, in: box)
        XCTAssertEqual(p.x, box.width * log(1000 / 20) / log(20000 / 20), accuracy: 1e-6)
        XCTAssertEqual(p.y, EqLayout.y(db: 6, in: box), accuracy: 1e-9)
        let pass = band("highpass", 100, q: 2, slope: 24)
        XCTAssertEqual(EqLayout.point(pass, in: box).y, EqLayout.y(db: EqLayout.cornerDb(pass), in: box), accuracy: 1e-9)
        // The nearest point within reach is grabbed; nothing further off.
        let bands = [bell, pass]
        XCTAssertEqual(EqLayout.grab(at: CGPoint(x: p.x + 3, y: p.y - 3), bands: bands, in: box), 0)
        XCTAssertNil(EqLayout.grab(at: CGPoint(x: p.x + 20, y: p.y), bands: bands, in: box))
        // A bell dragged reads back the frequency under it and the gain at
        // its height; a pass reads the resonance whose corner is there.
        let moved = EqLayout.dragged(bell, to: CGPoint(x: EqLayout.x(hz: 440, in: box), y: EqLayout.y(db: -3, in: box)), in: box)
        XCTAssertEqual(moved.freqHz, 440, accuracy: 0.5)
        XCTAssertEqual(moved.gainDb, -3, accuracy: 0.05)
        XCTAssertEqual(moved.q, bell.q)
        let lifted = EqLayout.dragged(pass, to: CGPoint(x: EqLayout.x(hz: 100, in: box), y: EqLayout.y(db: 12, in: box)), in: box)
        XCTAssertEqual(lifted.gainDb, 0)
        XCTAssertEqual(EqLayout.cornerDb(lifted), 12, accuracy: 0.1)
        XCTAssertEqual(EqLayout.dragged(pass, to: CGPoint(x: 0, y: -50), in: box).q, EqLayout.highQ)
        XCTAssertEqual(EqLayout.dragged(bell, to: CGPoint(x: 1000, y: 1000), in: box).gainDb, -24)
        XCTAssertEqual(EqLayout.dragged(bell, to: CGPoint(x: 1000, y: 1000), in: box).freqHz, 20000)
        // Option up narrows: q doubles each 40 points; held to its range.
        XCTAssertEqual(EqLayout.widened(q: 1, by: -40), 2, accuracy: 1e-9)
        XCTAssertEqual(EqLayout.widened(q: 1, by: 40), 0.5, accuracy: 1e-9)
        XCTAssertEqual(EqLayout.widened(q: 1, by: -1000), EqLayout.highQ)
    }

    func testTheSpectrumFillsEachColumnFromItsBins() {
        // 2049 bins at 48 kHz: a tone at bin 100 (1171.9 Hz) at −6 dB, the rest silent.
        var levels = [Float](repeating: -200, count: 2049)
        levels[100] = -6
        let columns = EqLayout.spectrumColumns(levels: levels, sampleRate: rate, in: box)
        XCTAssertEqual(columns.count, 300)
        let at = Int(EqLayout.x(hz: 100 * rate / 4096, in: box))
        XCTAssertEqual(columns[at], box.maxY - box.height * CGFloat(84.0 / 90), accuracy: 1e-6)
        XCTAssertEqual(columns[0], box.maxY, "silence lies on the floor")
        XCTAssertEqual(columns[299], box.maxY)
        // Low down the bins are further apart than the columns, so a column
        // between two bins reads between them.
        levels[2] = -20
        levels[3] = -40
        let low = EqLayout.spectrumColumns(levels: levels, sampleRate: rate, in: box)
        let between = Int(EqLayout.x(hz: 2.5 * rate / 4096, in: box))
        XCTAssertEqual(low[between], box.maxY - box.height * CGFloat(60.0 / 90), accuracy: box.height * 0.08)
        XCTAssertTrue(EqLayout.spectrumColumns(levels: [], sampleRate: rate, in: box).isEmpty)
    }
}
