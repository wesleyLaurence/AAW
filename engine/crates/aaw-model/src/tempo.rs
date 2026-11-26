//! The song's tempo through time: `session.tempo`, or a `tempo` lane on the
//! master whose points are steps. Everything placed in beats stays on its
//! beat and a tempo change moves only the length of a beat, so the map turns
//! beats into seconds and frames and back for every reader: the schedule,
//! the engine's voices and envelopes, the host's clock, the app and the
//! Python tools.
//!
//! A map of one tempo gives the frames `beat::frame` gives, by the same
//! exact arithmetic, and the seconds `beats * 60 / tempo` gives, so a song
//! without a lane plays and saves as it did. With steps the map is a sum of
//! exact segments, each from its step's beat at its own tempo, and stays exact.

use crate::beat::float_fraction;
use crate::schema::{Lane, Project};
use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};

/// The lane on the master that holds the song's tempo.
pub const TEMPO_PARAM: &str = "tempo";

/// One step of the map: from `beat` on, `bpm` quarter notes a minute, the
/// step starting `seconds` into the song.
#[derive(Clone, Debug, PartialEq)]
pub struct TempoStep {
    pub beat: BigRational,
    pub bpm: f64,
    pub seconds: BigRational,
    beat_f: f64,
    seconds_f: f64,
}

impl TempoStep {
    pub fn beat_f64(&self) -> f64 {
        self.beat_f
    }

    pub fn seconds_f64(&self) -> f64 {
        self.seconds_f
    }
}

/// The song's tempo map: its steps in order, the first at beat 0.
#[derive(Clone, Debug, PartialEq)]
pub struct TempoMap {
    steps: Vec<TempoStep>,
}

impl TempoMap {
    /// One tempo for the whole song.
    pub fn constant(bpm: f64) -> TempoMap {
        TempoMap {
            steps: vec![TempoStep {
                beat: BigRational::zero(),
                bpm,
                seconds: BigRational::zero(),
                beat_f: 0.0,
                seconds_f: 0.0,
            }],
        }
    }

    /// The map of a tempo lane, or of `opening` alone without one. Before
    /// its first point the first value holds, as on any lane; of two points
    /// at one beat the second holds from there; a step to the tempo already
    /// playing is no step.
    pub fn new(opening: f64, lane: Option<&Lane>) -> TempoMap {
        match lane {
            Some(lane) => TempoMap::from_points(opening, lane.points.iter().map(|p| (p.at_exact(), p.value)).collect()),
            None => TempoMap::constant(opening),
        }
    }

    /// The map of a lane's points as (beat, bpm), in order, or of
    /// `opening` alone without any.
    pub fn from_points(opening: f64, points: Vec<(BigRational, f64)>) -> TempoMap {
        let Some(first) = points.first() else {
            return TempoMap::constant(opening);
        };
        let mut steps: Vec<(BigRational, f64)> = vec![(BigRational::zero(), first.1)];
        for (at, value) in points {
            let last = steps.last_mut().expect("a step");
            if at <= last.0 {
                last.1 = value;
            } else if value != last.1 {
                steps.push((at, value));
            }
        }
        steps.dedup_by(|b, a| a.1 == b.1);
        TempoMap::from_steps(steps)
    }

    /// The map of steps given as (beat, bpm), the first at beat 0.
    pub fn from_steps(given: Vec<(BigRational, f64)>) -> TempoMap {
        let sixty = BigRational::from_integer(BigInt::from(60));
        let mut steps: Vec<TempoStep> = Vec::with_capacity(given.len());
        for (beat, bpm) in given {
            let seconds = match steps.last() {
                Some(prev) => &prev.seconds + (&beat - &prev.beat) * &sixty / float_fraction(prev.bpm),
                None => BigRational::zero(),
            };
            steps.push(TempoStep {
                beat_f: beat.to_f64().unwrap_or(0.0),
                seconds_f: seconds.to_f64().unwrap_or(0.0),
                beat,
                bpm,
                seconds,
            });
        }
        if steps.is_empty() {
            return TempoMap::constant(120.0);
        }
        TempoMap { steps }
    }

    /// The song's map: its master's tempo lane, or its one tempo.
    pub fn of(p: &Project) -> TempoMap {
        TempoMap::new(p.session.tempo, p.tempo_lane())
    }

    pub fn steps(&self) -> &[TempoStep] {
        &self.steps
    }

    /// The tempo of a song that keeps one.
    pub fn constant_tempo(&self) -> Option<f64> {
        (self.steps.len() == 1).then(|| self.steps[0].bpm)
    }

    /// The tempo at beat 0.
    pub fn opening(&self) -> f64 {
        self.steps[0].bpm
    }

    /// The slowest and the fastest tempo of the song.
    pub fn bounds(&self) -> (f64, f64) {
        let tempos = self.steps.iter().map(|s| s.bpm);
        (tempos.clone().fold(f64::INFINITY, f64::min), tempos.fold(f64::NEG_INFINITY, f64::max))
    }

    fn at_beat(&self, beat: f64) -> &TempoStep {
        let i = self.steps.partition_point(|s| s.beat_f <= beat);
        &self.steps[i.saturating_sub(1)]
    }

    fn at_exact(&self, beat: &BigRational) -> &TempoStep {
        let i = self.steps.partition_point(|s| &s.beat <= beat);
        &self.steps[i.saturating_sub(1)]
    }

    fn at_seconds(&self, seconds: f64) -> &TempoStep {
        let i = self.steps.partition_point(|s| s.seconds_f <= seconds);
        &self.steps[i.saturating_sub(1)]
    }

    /// The index of the step a beat is in.
    pub fn index_at(&self, beat: f64) -> usize {
        self.steps.partition_point(|s| s.beat_f <= beat).saturating_sub(1)
    }

    /// The tempo playing at a beat.
    pub fn tempo_at(&self, beat: f64) -> f64 {
        self.at_beat(beat).bpm
    }

    pub fn tempo_at_exact(&self, beat: &BigRational) -> f64 {
        self.at_exact(beat).bpm
    }

    /// The tempo playing at a second of the song.
    pub fn tempo_at_seconds(&self, seconds: f64) -> f64 {
        self.at_seconds(seconds).bpm
    }

    /// The second a beat falls on, exactly.
    pub fn seconds_exact(&self, beat: &BigRational) -> BigRational {
        let s = self.at_exact(beat);
        &s.seconds + (beat - &s.beat) * BigRational::from_integer(BigInt::from(60)) / float_fraction(s.bpm)
    }

    /// The frame a beat falls on, rounding half up from absolute time: with
    /// one tempo, `beat::frame`'s exactly.
    pub fn frame(&self, beat: &BigRational, rate: i64) -> i64 {
        let s = self.at_exact(beat);
        let x = if s.beat.is_zero() {
            // The first step: the arithmetic of `beat::frame`, unchanged.
            beat * BigRational::from_integer(BigInt::from(60 * rate)) / float_fraction(s.bpm)
        } else {
            self.seconds_exact(beat) * BigRational::from_integer(BigInt::from(rate))
        };
        let (n, d) = (x.numer(), x.denom());
        let two = BigInt::from(2);
        (&two * n + d).div_floor(&(&two * d)).to_i64().expect("frame fits in i64")
    }

    /// The second a beat falls on: `beat * 60 / tempo` with one tempo.
    pub fn seconds(&self, beat: f64) -> f64 {
        let s = self.at_beat(beat);
        s.seconds_f + (beat - s.beat_f) * 60.0 / s.bpm
    }

    /// The beat a second of the song falls on: `seconds * tempo / 60` with one tempo.
    pub fn beat(&self, seconds: f64) -> f64 {
        let s = self.at_seconds(seconds);
        s.beat_f + (seconds - s.seconds_f) * s.bpm / 60.0
    }

    /// The beat a frame falls on: `frame * tempo / 60 / rate` with one tempo.
    pub fn beat_of_frame(&self, frame: f64, rate: f64) -> f64 {
        match self.constant_tempo() {
            Some(t) => frame * t / 60.0 / rate,
            None => self.beat(frame / rate),
        }
    }

    /// Seconds from one beat to another.
    pub fn seconds_between(&self, from: f64, to: f64) -> f64 {
        match self.constant_tempo() {
            Some(t) => (to - from) * 60.0 / t,
            None => self.seconds(to) - self.seconds(from),
        }
    }

    /// The beat `seconds` after `from`: `from + seconds * tempo / 60` with one tempo.
    pub fn beat_after(&self, from: f64, seconds: f64) -> f64 {
        match self.constant_tempo() {
            Some(t) => from + seconds * t / 60.0,
            None => self.beat(self.seconds(from) + seconds),
        }
    }

    /// The steps strictly between two beats, where a stretch of the song
    /// from one to the other changes tempo.
    pub fn changes_between(&self, from: f64, to: f64) -> impl Iterator<Item = &TempoStep> {
        self.steps.iter().skip(1).filter(move |s| s.beat_f > from && s.beat_f < to)
    }

    /// Text that is equal for equal maps, for the keys of caches.
    pub fn key(&self) -> String {
        self.steps.iter().map(|s| format!("{}@{}", s.bpm.to_bits(), crate::fraction_str(&s.beat))).collect::<Vec<_>>().join(",")
    }
}

impl Project {
    /// The master's tempo lane, when the song changes tempo.
    pub fn tempo_lane(&self) -> Option<&Lane> {
        self.master.automation.iter().find(|l| l.param == TEMPO_PARAM)
    }

    /// The song's tempo map.
    pub fn tempo_map(&self) -> TempoMap {
        TempoMap::of(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beat::{frame, parse_fraction};

    fn q(s: &str) -> BigRational {
        parse_fraction(s).ok().unwrap()
    }

    fn stepped() -> TempoMap {
        TempoMap::from_steps(vec![(q("0"), 120.0), (q("64"), 150.0)])
    }

    #[test]
    fn one_tempo_is_the_frames_and_seconds_of_today() {
        let map = TempoMap::constant(143.987);
        for b in ["0", "1/3", "17", "1023/7", "100000.125"] {
            assert_eq!(map.frame(&q(b), 48000), frame(&q(b), 143.987, 48000), "{b}");
            let f = q(b).to_f64().unwrap();
            assert_eq!(map.seconds(f).to_bits(), (f * 60.0 / 143.987).to_bits());
            assert_eq!(map.beat(f).to_bits(), (f * 143.987 / 60.0).to_bits());
            assert_eq!(map.beat_of_frame(f, 48000.0).to_bits(), (f * 143.987 / 60.0 / 48000.0).to_bits());
        }
    }

    #[test]
    fn a_step_starts_its_tempo_on_its_beat() {
        let map = stepped();
        // 64 beats at 120 are 32 s; from there a beat is 0.4 s.
        assert_eq!(map.frame(&q("64"), 48000), 32 * 48000);
        assert_eq!(map.frame(&q("65"), 48000), 32 * 48000 + 19200);
        assert_eq!(map.frame(&q("63"), 48000), frame(&q("63"), 120.0, 48000));
        assert_eq!(map.seconds(66.0), 32.8);
        assert!((map.beat(32.8) - 66.0).abs() < 1e-12);
        assert_eq!(map.tempo_at(63.99), 120.0);
        assert_eq!(map.tempo_at(64.0), 150.0);
        assert_eq!(map.tempo_at_seconds(31.9), 120.0);
        assert_eq!(map.seconds_exact(&q("1/3")), q("1/6"));
        assert_eq!(map.changes_between(0.0, 64.0).count(), 0);
        assert_eq!(map.changes_between(0.0, 64.5).count(), 1);
    }

    #[test]
    fn a_frame_of_a_step_rounds_once_from_absolute_time() {
        // At 7 BPM-odd tempos the segments do not end on frames; each frame
        // is rounded from the exact second, not from the step's frame.
        let map = TempoMap::from_steps(vec![(q("0"), 133.0), (q("3"), 97.0)]);
        let exact = q("3") * q("60") / float_fraction(133.0) + q("1/3") * q("60") / float_fraction(97.0);
        let x = exact * q("44100");
        let want = (x.clone() + q("1/2")).floor().to_integer().to_i64().unwrap();
        assert_eq!(map.frame(&q("10/3"), 44100), want);
    }

    #[test]
    fn a_lane_holds_its_first_value_and_merges_repeats() {
        let points = |list: &[(&str, f64)]| Lane {
            param: "tempo".into(),
            points: list
                .iter()
                .map(|(at, v)| crate::schema::Point {
                    at: crate::Beat::Str(at.to_string()),
                    value: *v,
                    curve: crate::schema::Curve::Hold,
                    shape: 0.0,
                })
                .collect(),
        };
        let map = TempoMap::new(90.0, Some(&points(&[("8", 140.0), ("16", 140.0), ("32", 170.0), ("32", 160.0)])));
        let steps: Vec<(f64, f64)> = map.steps().iter().map(|s| (s.beat_f64(), s.bpm)).collect();
        assert_eq!(steps, vec![(0.0, 140.0), (32.0, 160.0)]);
        assert_eq!(TempoMap::new(90.0, None).constant_tempo(), Some(90.0));
        assert_eq!(TempoMap::new(90.0, Some(&points(&[("0", 100.0)]))).constant_tempo(), Some(100.0));
    }
}
