@testable import AAWApp
import XCTest

final class SynthLayoutTests: XCTestCase {
    let box = CGRect(x: 0, y: 0, width: 176, height: 56)

    func testTheFilterResponseIsTheSynthsAndTheCornerIsAtTheCutoff() {
        // A lowpass with no resonance is -6 dB at its cutoff (k = 2), flat
        // well below it, and 24 dB an octave falls twice as fast as 12.
        let at = { (hz: Double, sections: Int) in SynthLayout.responseDb(mode: "lowpass", sections: sections, cutoffHz: 1000, resonancePercent: 0, hz: hz) }
        XCTAssertEqual(at(1000, 1), -6.02, accuracy: 0.05)
        XCTAssertEqual(at(10, 1), 0, accuracy: 0.01)
        XCTAssertEqual(at(8000, 1), -36.1, accuracy: 0.3)
        XCTAssertEqual(at(8000, 2), 2 * at(8000, 1), accuracy: 1e-9)
        // Resonance lifts the corner: 100 is k = 0.02, 34 dB.
        XCTAssertEqual(SynthLayout.cornerDb(mode: "lowpass", sections: 1, resonancePercent: 100), 33.98, accuracy: 0.05)
        XCTAssertEqual(SynthLayout.resonance(cornerDb: SynthLayout.cornerDb(mode: "lowpass", sections: 2, resonancePercent: 37), sections: 2), 37, accuracy: 1e-6)
        // The highpass mirrors the lowpass about the cutoff; the notch is nothing at it.
        XCTAssertEqual(SynthLayout.responseDb(mode: "highpass", sections: 1, cutoffHz: 1000, resonancePercent: 0, hz: 125), at(8000, 1), accuracy: 1e-6)
        XCTAssertLessThan(SynthLayout.responseDb(mode: "notch", sections: 1, cutoffHz: 1000, resonancePercent: 0, hz: 1000), -100)
        XCTAssertEqual(SynthLayout.responseDb(mode: "bandpass", sections: 1, cutoffHz: 1000, resonancePercent: 50, hz: 1000), 0, accuracy: 1e-6)
        // The corner sits at the cutoff's place across the box, in equal ratios.
        let corner = SynthLayout.corner(mode: "lowpass", sections: 1, cutoffHz: 1000, resonancePercent: 0, in: box)
        XCTAssertEqual(corner.x, box.width * (log(100.0) / log(2000.0)), accuracy: 1e-6)
        XCTAssertEqual(corner.y, SynthLayout.y(db: -6.02, in: box), accuracy: 0.05)
        // Dragged, it reads back the cutoff and the resonance it was dragged to.
        let moved = SynthLayout.filter(at: CGPoint(x: SynthLayout.x(hz: 440, in: box), y: SynthLayout.y(db: 6, in: box)), mode: "lowpass", sections: 1, in: box)
        XCTAssertEqual(moved.cutoffHz, 440, accuracy: 0.5)
        XCTAssertEqual(moved.resonancePercent, SynthLayout.resonance(cornerDb: 6, sections: 1), accuracy: 0.06)
        XCTAssertEqual(SynthLayout.filter(at: CGPoint(x: -50, y: 1000), mode: "lowpass", sections: 1, in: box).cutoffHz, 10)
        XCTAssertEqual(SynthLayout.filter(at: CGPoint(x: 1000, y: -50), mode: "lowpass", sections: 1, in: box).resonancePercent, 100)
    }

    func testTheEnvelopeHasFourCornersAndThreeHandlesThatReadBackTheTimes() {
        let corners = SynthLayout.envelope(attackMs: 0, decayMs: 100, sustainPercent: 50, releaseMs: 20000, in: box)
        XCTAssertEqual(corners.count, 5)
        XCTAssertEqual(corners[0], CGPoint(x: 0, y: 56))
        XCTAssertEqual(corners[1], CGPoint(x: 0, y: 0), "no attack: straight up")
        XCTAssertEqual(corners[2].y, 28, accuracy: 1e-9, "the sustain is half way up")
        XCTAssertEqual(corners[3].x - corners[2].x, 44, accuracy: 1e-9, "the hold is a quarter")
        XCTAssertEqual(corners[4].x - corners[3].x, 44, accuracy: 1e-9, "the longest release fills its quarter")
        XCTAssertEqual(corners[4].y, 56)
        // Each handle is found near its corner, and the decay's first where they meet.
        XCTAssertEqual(SynthLayout.handle(at: CGPoint(x: 2, y: 3), corners: corners), .attack)
        XCTAssertEqual(SynthLayout.handle(at: corners[4], corners: corners), .release)
        let met = SynthLayout.envelope(attackMs: 0, decayMs: 0, sustainPercent: 100, releaseMs: 100, in: box)
        XCTAssertEqual(met[1], met[2])
        XCTAssertEqual(SynthLayout.handle(at: CGPoint(x: 2, y: 3), corners: met), .decay)
        XCTAssertNil(SynthLayout.handle(at: CGPoint(x: 100, y: 50), corners: corners))
        let apart = SynthLayout.envelope(attackMs: 500, decayMs: 100, sustainPercent: 50, releaseMs: 300, in: box)
        XCTAssertEqual(SynthLayout.handle(at: apart[1], corners: apart), .attack)
        // A handle dragged reads back its time, and the decay's the sustain too.
        let attack = SynthLayout.envelope(.attack, at: CGPoint(x: apart[1].x, y: 0), corners: apart, in: box)
        XCTAssertEqual(attack.count, 1)
        XCTAssertEqual(attack[0].field, "attack_ms")
        XCTAssertEqual(attack[0].value, 500, accuracy: 1)
        let decay = SynthLayout.envelope(.decay, at: CGPoint(x: apart[2].x, y: 14), corners: apart, in: box)
        XCTAssertEqual(decay.map(\.field), ["decay_ms", "sustain_percent"])
        XCTAssertEqual(decay[0].value, 100, accuracy: 1)
        XCTAssertEqual(decay[1].value, 75)
        let release = SynthLayout.envelope(.release, at: CGPoint(x: apart[3].x, y: 56), corners: apart, in: box)
        XCTAssertEqual(release[0].value, 0)
        XCTAssertEqual(SynthLayout.envelope(.release, at: CGPoint(x: 1000, y: 56), corners: apart, in: box)[0].value, 20000)
        // Short times are kept to a tenth of a millisecond.
        XCTAssertEqual(SynthLayout.ms(stretch: SynthLayout.stretch(ms: 2.5)), 2.5, accuracy: 1e-9)
    }

    func testWavesAndLfosAreTheSynths() {
        XCTAssertEqual(SynthLayout.wave("sine", at: 0.25), 1, accuracy: 1e-9)
        XCTAssertEqual(SynthLayout.wave("saw", at: 0), -1)
        XCTAssertEqual(SynthLayout.wave("saw", at: 0.5), 0)
        XCTAssertEqual(SynthLayout.wave("square", at: 0.25), 1)
        XCTAssertEqual(SynthLayout.wave("square", at: 0.75), -1)
        XCTAssertEqual(SynthLayout.wave("triangle", at: 0.25), 1)
        XCTAssertEqual(SynthLayout.wave("triangle", at: 0.75), -1)
        // A pulse high a quarter of the time is kept centered: its mean is zero.
        let mean = (0..<1000).map { SynthLayout.wave("pulse", at: Double($0) / 1000, pulseWidth: 25) }.reduce(0, +) / 1000
        XCTAssertEqual(mean, 0, accuracy: 0.01)
        XCTAssertEqual(SynthLayout.wave("noise", at: 0.3), SynthLayout.wave("noise", at: 0.3), "the drawing holds still")
        XCTAssertTrue((0..<50).contains { abs(SynthLayout.wave("noise", at: Double($0) / 50)) > 0.3 })
        XCTAssertEqual(SynthLayout.lfo("saw", at: 0), 1, "an LFO's saw falls")
        XCTAssertEqual(SynthLayout.lfo("saw", at: 0.5), 0)
        XCTAssertEqual(SynthLayout.lfo("square", at: 0.9), -1)
        XCTAssertEqual(SynthLayout.lfo("sine", at: 1.25), 1, accuracy: 1e-9, "the phase wraps")
    }

    func testTheKeysAreAnOctaveWithTheBlackOnesOverTheWhite() {
        let keys = SynthLayout.Keys(box: CGRect(x: 0, y: 0, width: 160, height: 40), low: 48)
        XCTAssertEqual(keys.whiteWidth, 20)
        XCTAssertEqual(keys.frame(semitone: 0), CGRect(x: 0, y: 0, width: 20, height: 40))
        XCTAssertEqual(keys.frame(semitone: 12), CGRect(x: 140, y: 0, width: 20, height: 40))
        let cSharp = keys.frame(semitone: 1)
        XCTAssertEqual(cSharp.midX, 20, accuracy: 1e-9, "astride the line between C and D")
        XCTAssertEqual(cSharp.height, 24, accuracy: 1e-9)
        // Under the black key is the black key; below it, the white one.
        XCTAssertEqual(keys.note(at: CGPoint(x: 20, y: 10))?.pitch, 49)
        XCTAssertEqual(keys.note(at: CGPoint(x: 21, y: 35))?.pitch, 50)
        XCTAssertEqual(keys.note(at: CGPoint(x: 5, y: 35))?.pitch, 48)
        XCTAssertEqual(keys.note(at: CGPoint(x: 155, y: 35))?.pitch, 60)
        XCTAssertNil(keys.note(at: CGPoint(x: 170, y: 10)))
        // Softly at the top, hard at the bottom.
        XCTAssertEqual(keys.note(at: CGPoint(x: 5, y: 0))?.velocity, 40)
        XCTAssertEqual(keys.note(at: CGPoint(x: 5, y: 40))?.velocity, 127)
        XCTAssertEqual(SynthLayout.octaveName(low: 48), "C3")
    }

    func testAnEntrysReachOnAControlFollowsItsUnit() {
        // Two octaves up from 500 Hz on a log control is 2000 Hz.
        let cutoff = SynthLayout.reach(value: 500, min: 10, max: 20000, log: true, amount: 2, unit: "octaves")
        XCTAssertEqual(cutoff.from, log(50.0) / log(2000.0), accuracy: 1e-9)
        XCTAssertEqual(cutoff.to, log(200.0) / log(2000.0), accuracy: 1e-9)
        // Points add; the reach stops at the control's end.
        let resonance = SynthLayout.reach(value: 90, min: 0, max: 100, log: false, amount: 25, unit: "points")
        XCTAssertEqual(resonance.from, 0.9, accuracy: 1e-9)
        XCTAssertEqual(resonance.to, 1, accuracy: 1e-9)
        let down = SynthLayout.reach(value: 0, min: -36, max: 36, log: false, amount: -12, unit: "semitones")
        XCTAssertEqual(down.to, 1.0 / 3, accuracy: 1e-9)
        XCTAssertEqual(SynthLayout.startingAmount(unit: "octaves"), 1)
        XCTAssertEqual(SynthLayout.startingAmount(unit: "points"), 25)
        XCTAssertEqual(SynthLayout.unit(ofTarget: "oscillators.a.pitch"), "semitones")
        XCTAssertEqual(SynthLayout.unit(ofTarget: "filter.cutoff_hz"), "octaves")
        XCTAssertEqual(SynthLayout.unit(ofTarget: "envelopes.amp.sustain_percent"), "points")
        XCTAssertEqual(SynthLayout.unit(ofTarget: "width_percent"), "points", "the patch's width, dropped on from a source")
    }
}
