//! Automation envelopes evaluated at timeline frames, as `automation.Envelope`,
//! and parameters that follow them.
//!
//! A lane holds its first value before its first point and its last value after
//! its last point. A point's curve shapes the segment after it: linear moves in
//! the parameter's domain (log for frequencies and q), hold keeps the value until
//! the next point. Two points at one position jump there. Nothing is smoothed.

use aaw_model::rules::Domain;
use aaw_model::{frame, Curve, Lane};
use std::sync::Arc;

#[derive(Debug, PartialEq)]
pub struct Envelope {
    frames: Vec<i64>,
    /// The points' values, or their logarithms in the log domain.
    values: Vec<f64>,
    hold: Vec<bool>,
    log: bool,
    constant: Option<f64>,
}

impl Envelope {
    pub fn new(lane: &Lane, domain: Domain, tempo: f64, rate: i64) -> Envelope {
        let log = domain == Domain::Log;
        let first = lane.points[0].value;
        Envelope {
            frames: lane.points.iter().map(|p| frame(&p.at_exact(), tempo, rate)).collect(),
            values: lane.points.iter().map(|p| if log { p.value.ln() } else { p.value }).collect(),
            hold: lane.points.iter().map(|p| p.curve == Curve::Hold).collect(),
            log,
            constant: lane.points.iter().all(|p| p.value == first).then_some(first),
        }
    }

    /// The value of a lane whose points all share one, which renders exactly
    /// as that static value.
    pub fn constant(&self) -> Option<f64> {
        self.constant
    }

    /// The segment a frame is in: the last point at or before it, or None
    /// before the first point.
    fn segment(&self, f: i64) -> Option<usize> {
        self.frames.partition_point(|x| *x <= f).checked_sub(1)
    }

    /// The value in the envelope's own domain for a frame in segment `k`.
    #[inline]
    fn raw(&self, k: Option<usize>, f: i64) -> f64 {
        let last = self.frames.len() - 1;
        match k {
            None => self.values[0],
            Some(k) if k == last || self.hold[k] => self.values[k],
            Some(k) => {
                let span = (self.frames[k + 1] - self.frames[k]).max(1);
                (f - self.frames[k]) as f64 / span as f64 * (self.values[k + 1] - self.values[k]) + self.values[k]
            }
        }
    }

    #[inline]
    fn finish(&self, v: f64) -> f64 {
        if self.log { v.exp() } else { v }
    }

    /// The value at a timeline frame, which may precede zero.
    pub fn at(&self, f: i64) -> f64 {
        self.finish(self.raw(self.segment(f), f))
    }

    /// The values at `out.len()` frames from `start`, `step` frames apart
    /// (1, or 0 for a stopped timeline). Does not allocate.
    pub fn fill(&self, start: i64, step: i64, out: &mut [f64]) {
        if step == 0 {
            out.fill(self.at(start));
            return;
        }
        let mut k = self.segment(start);
        let mut next = k.map_or(0, |k| k + 1);
        for (i, o) in out.iter_mut().enumerate() {
            let f = start + i as i64;
            while next < self.frames.len() && self.frames[next] <= f {
                k = Some(next);
                next += 1;
            }
            *o = self.finish(self.raw(k, f));
        }
    }
}

/// A parameter: a static value, or a lane that moves it. A constant lane is its
/// static value. Two are equal when they have the same value at every frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    value: f64,
    envelope: Option<Arc<Envelope>>,
}

impl Param {
    pub fn fixed(value: f64) -> Param {
        Param { value, envelope: None }
    }

    /// `value` unless a lane overrides it.
    pub fn new(value: f64, lane: Option<&Arc<Envelope>>) -> Param {
        match lane {
            None => Param::fixed(value),
            Some(e) => match e.constant() {
                Some(c) => Param::fixed(c),
                None => Param {
                    value,
                    envelope: Some(e.clone()),
                },
            },
        }
    }

    /// Whether a lane moves the parameter.
    pub fn moves(&self) -> bool {
        self.envelope.is_some()
    }

    /// The static value; meaningful when no lane moves the parameter.
    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn at(&self, f: i64) -> f64 {
        match &self.envelope {
            Some(e) => e.at(f),
            None => self.value,
        }
    }

    pub fn fill(&self, start: i64, step: i64, out: &mut [f64]) {
        match &self.envelope {
            Some(e) => e.fill(start, step, out),
            None => out.fill(self.value),
        }
    }
}

/// A value that glides from where another left off. When a program replaces
/// the playing one, each level and knob starts from the value last in effect
/// and reaches its own over a few milliseconds, so a live edit does not click.
/// Until it takes over from another, it adds nothing: an offline render reads
/// exact values.
#[derive(Clone, Debug)]
pub struct Glide {
    /// The value to start from, set when taking over.
    carry: Option<f64>,
    offset: f64,
    left: usize,
    length: usize,
    last: f64,
}

impl Glide {
    /// `length` is the glide's duration in frames.
    pub fn new(length: usize) -> Glide {
        Glide {
            carry: None,
            offset: 0.0,
            left: 0,
            length: length.max(1),
            last: f64::NAN,
        }
    }

    /// Continues from `old`. When the value it glides to is `changed`, the
    /// next block starts from the value `old` last had and glides from there;
    /// otherwise it goes on exactly as `old` would have.
    pub fn take_over(&mut self, old: &Glide, changed: bool) {
        if changed {
            self.carry = old.carry.or((!old.last.is_nan()).then_some(old.last));
        } else {
            *self = Glide {
                length: self.length,
                ..old.clone()
            };
        }
    }

    /// The value last in effect; NaN before any block.
    pub fn last(&self) -> f64 {
        self.last
    }

    /// Whether the next block's values are exact.
    pub fn resting(&self) -> bool {
        self.carry.is_none() && self.left == 0
    }

    /// Adds what is left of the glide to a block of exact values.
    pub fn apply(&mut self, out: &mut [f64]) {
        let Some(first) = out.first().copied() else { return };
        if let Some(from) = self.carry.take() {
            self.offset = from - first;
            self.left = if self.offset == 0.0 { 0 } else { self.length };
        }
        if self.left > 0 {
            let n = self.left.min(out.len());
            for (i, o) in out[..n].iter_mut().enumerate() {
                *o += self.offset * ((self.left - i) as f64 / self.length as f64);
            }
            self.left -= n;
        }
        self.last = out[out.len() - 1];
    }

    /// What is left of the glide for a block of exact values that starts at
    /// `first` and ends at `last`: the amount to add to each frame, in `out`.
    /// False while resting, when there is nothing to add and `out` is left
    /// as it is.
    pub fn offsets(&mut self, first: f64, last: f64, out: &mut [f64]) -> bool {
        if let Some(from) = self.carry.take() {
            self.offset = from - first;
            self.left = if self.offset == 0.0 { 0 } else { self.length };
        }
        if self.left == 0 || out.is_empty() {
            self.last = last;
            return false;
        }
        let n = self.left.min(out.len());
        for (i, o) in out[..n].iter_mut().enumerate() {
            *o = self.offset * ((self.left - i) as f64 / self.length as f64);
        }
        out[n..].fill(0.0);
        self.left -= n;
        self.last = last + out[out.len() - 1];
        true
    }
}

/// A parameter with its glide: what a device reads each block.
#[derive(Clone, Debug)]
pub struct Knob {
    pub param: Param,
    pub glide: Glide,
}

impl Knob {
    pub fn new(param: Param, glide: usize) -> Knob {
        Knob {
            param,
            glide: Glide::new(glide),
        }
    }

    pub fn fill(&mut self, start: i64, step: i64, out: &mut [f64]) {
        self.param.fill(start, step, out);
        self.glide.apply(out);
    }

    /// Continues from `old`, gliding when the parameter is another.
    pub fn take_over(&mut self, old: &Knob) {
        self.glide.take_over(&old.glide, self.param != old.param);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaw_model::{Beat, Point};

    fn lane(points: &[(f64, f64, Curve)]) -> Lane {
        Lane {
            param: "gain_db".into(),
            points: points
                .iter()
                .map(|(at, value, curve)| Point {
                    at: Beat::Float(*at),
                    value: *value,
                    curve: *curve,
                })
                .collect(),
        }
    }

    // 120 BPM at 48 kHz: a beat is 24000 frames.
    fn envelope(points: &[(f64, f64, Curve)], domain: Domain) -> Envelope {
        Envelope::new(&lane(points), domain, 120.0, 48000)
    }

    #[test]
    fn holds_the_ends_and_interpolates_between() {
        let e = envelope(&[(1.0, -6.0, Curve::Linear), (2.0, 0.0, Curve::Linear)], Domain::Linear);
        assert_eq!(e.at(-500), -6.0);
        assert_eq!(e.at(0), -6.0);
        assert_eq!(e.at(24000), -6.0);
        assert_eq!(e.at(36000), -3.0);
        assert_eq!(e.at(48000), 0.0);
        assert_eq!(e.at(1 << 40), 0.0);
        assert_eq!(e.constant(), None);
    }

    #[test]
    fn hold_keeps_the_value_and_coincident_points_jump() {
        let e = envelope(
            &[(0.0, 1.0, Curve::Hold), (1.0, 2.0, Curve::Linear), (1.0, 5.0, Curve::Linear), (2.0, 7.0, Curve::Linear)],
            Domain::Linear,
        );
        assert_eq!(e.at(23999), 1.0);
        assert_eq!(e.at(24000), 5.0);
        assert_eq!(e.at(36000), 6.0);
    }

    #[test]
    fn log_lanes_move_in_equal_ratios() {
        let e = envelope(&[(0.0, 200.0, Curve::Linear), (6.0, 12800.0, Curve::Linear)], Domain::Log);
        for beat in 0..6 {
            let v = e.at(beat * 24000);
            assert!((v / (200.0 * 2f64.powi(beat as i32)) - 1.0).abs() < 1e-12, "{v}");
        }
    }

    #[test]
    fn a_lane_of_one_value_is_constant() {
        let e = envelope(&[(0.0, 300.0, Curve::Linear), (4.0, 300.0, Curve::Hold)], Domain::Log);
        assert_eq!(e.constant(), Some(300.0));
        let p = Param::new(1000.0, Some(&Arc::new(e)));
        assert!(!p.moves());
        assert_eq!(p.at(12345), 300.0);
    }

    #[test]
    fn filling_a_run_equals_each_frame() {
        let e = envelope(
            &[(0.5, 0.0, Curve::Linear), (1.0, 1.0, Curve::Hold), (1.5, 0.25, Curve::Linear), (1.5, 0.5, Curve::Linear), (3.0, -1.0, Curve::Linear)],
            Domain::Linear,
        );
        for (start, n) in [(-100, 300), (11990, 30), (23000, 14000), (35990, 40000), (80000, 50)] {
            let mut out = vec![0.0; n];
            e.fill(start, 1, &mut out);
            for (i, v) in out.iter().enumerate() {
                assert_eq!(*v, e.at(start + i as i64), "frame {}", start + i as i64);
            }
        }
        let mut frozen = vec![0.0; 8];
        e.fill(30000, 0, &mut frozen);
        assert!(frozen.iter().all(|v| *v == e.at(30000)));
    }

    #[test]
    fn a_glide_adds_nothing_until_it_takes_over() {
        let mut g = Glide::new(4);
        let mut block = [1.0, 2.0, 3.0];
        g.apply(&mut block);
        assert_eq!(block, [1.0, 2.0, 3.0]);
        assert!(g.resting());
        let mut next = Glide::new(4);
        next.take_over(&g, true);
        let mut block = [7.0; 3];
        next.apply(&mut block);
        // From 3 to 7 in four frames, then exactly 7.
        assert_eq!(block, [3.0, 4.0, 5.0]);
        // Taken over part-way with the value unchanged, it goes on as it was.
        let mut same = Glide::new(4);
        same.take_over(&next, false);
        let mut block = [7.0; 3];
        same.apply(&mut block);
        assert_eq!(block, [6.0, 7.0, 7.0]);
        assert!(same.resting());
    }
}
