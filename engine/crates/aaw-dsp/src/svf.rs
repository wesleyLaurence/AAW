//! A filter or equalizer whose coefficients follow automation, as
//! `effects.AutomatedSos`.
//!
//! Each second-order section is a trapezoidal state-variable filter (Simper's
//! "Linear Trap SVF"). Its two integrator states stay near signal level, so they
//! carry across a coefficient jump without the burst a direct-form state
//! produces. Coefficients are updated every `CONTROL` timeline frames, counted
//! from the start of the song, from the value at the first frame of each period.
//! Between updates a section runs as its equivalent direct form II biquad; on an
//! update its history is converted so the state-variable state is unchanged.

use crate::envelope::Param;
use crate::{Clock, Frame};
use aaw_model::BandShape;
use std::f64::consts::PI;

/// Frames per coefficient update.
pub const CONTROL: i64 = 64;

#[derive(Clone, Debug)]
pub enum Shape {
    /// A Butterworth filter of `order`, whose cutoff is the one parameter.
    Filter { highpass: bool, order: usize },
    /// Bands with three parameters each: frequency, gain and q.
    Eq { bands: Vec<BandShape> },
}

/// State-variable parameters (g, k, m0, m1, m2) of one section.
type Tuning = [f64; 5];

#[derive(Clone, Debug)]
struct Section {
    tuning: Tuning,
    /// Direct form II coefficients b0, b1, b2, a1, a2.
    coefficients: [f64; 5],
    /// The matrix from the direct form's history to the state-variable state.
    to_state: [f64; 4],
    /// The all-pole part's filter state, per channel.
    zi: [[f64; 2]; 2],
    /// w[n-1] and w[n-2] of the direct form, per channel.
    history: [[f64; 2]; 2],
}

#[derive(Clone, Debug)]
pub struct Svf {
    shape: Shape,
    rate: f64,
    params: Vec<Param>,
    sections: Vec<Section>,
    pending: Vec<Tuning>,
    /// The period the sections are tuned for.
    tick: Option<i64>,
    tuned: bool,
}

/// Direct form coefficients of a tuning, the matrix T with state-variable state
/// s = T (w[n-1], w[n-2]), and its inverse.
fn biquad(t: &Tuning) -> ([f64; 5], [f64; 4], [f64; 4]) {
    let [g, k, m0, m1, m2] = *t;
    let c1 = 1.0 / (1.0 + g * (g + k));
    let (c2, c3) = (g * c1, g * g * c1);
    // One step in state-space form: s' = A s + B x, y = C s + D x.
    let (a00, a01, a10, a11) = (2.0 * c1 - 1.0, -2.0 * c2, 2.0 * c2, 1.0 - 2.0 * c3);
    let (b_0, b_1) = (2.0 * c2, 2.0 * c3);
    let (c_0, c_1) = (m1 * c1 + m2 * c2, m2 * (1.0 - c3) - m1 * c2);
    let d = m0 + m1 * c2 + m2 * c3;
    let (a1, a2) = (-(a00 + a11), a00 * a11 - a01 * a10);
    let b1 = d * a1 + c_0 * b_0 + c_1 * b_1;
    let b2 = d * a2 + c_0 * (a01 * b_1 - a11 * b_0) + c_1 * (a10 * b_0 - a00 * b_1);
    // T maps the controllable canonical basis to the state-variable basis:
    // T = [B, A B + a1 B]. It is invertible whenever g > 0.
    let (t00, t01) = (b_0, a00 * b_0 + a01 * b_1 + a1 * b_0);
    let (t10, t11) = (b_1, a10 * b_0 + a11 * b_1 + a1 * b_1);
    let det = t00 * t11 - t01 * t10;
    ([d, b1, b2, a1, a2], [t00, t01, t10, t11], [t11 / det, -t01 / det, -t10 / det, t00 / det])
}

/// A 2×2 matrix times the two history rows.
fn apply(m: &[f64; 4], h: &[[f64; 2]; 2]) -> [[f64; 2]; 2] {
    [
        [m[0] * h[0][0] + m[1] * h[1][0], m[0] * h[0][1] + m[1] * h[1][1]],
        [m[2] * h[0][0] + m[3] * h[1][0], m[2] * h[0][1] + m[3] * h[1][1]],
    ]
}

impl Svf {
    /// `params` holds the cutoff of a filter, or each band's frequency, gain
    /// and q in turn.
    pub fn new(shape: Shape, params: Vec<Param>, rate: f64) -> Svf {
        let count = match &shape {
            Shape::Filter { order, .. } => order / 2,
            Shape::Eq { bands } => bands.len(),
        };
        let section = Section {
            tuning: [0.0; 5],
            coefficients: [0.0; 5],
            to_state: [0.0; 4],
            zi: [[0.0; 2]; 2],
            history: [[0.0; 2]; 2],
        };
        Svf {
            shape,
            rate,
            params,
            sections: vec![section; count],
            pending: vec![[0.0; 5]; count],
            tick: None,
            tuned: false,
        }
    }

    /// Each section's tuning for the period starting at `tick`.
    fn tune(&mut self, tick: i64) {
        match &self.shape {
            Shape::Filter { highpass, order } => {
                let g = (PI * self.params[0].at(tick) / self.rate).tan();
                for (i, t) in self.pending.iter_mut().enumerate() {
                    let k = 2.0 * (PI * (2 * i + 1) as f64 / (2 * order) as f64).sin();
                    *t = if *highpass { [g, k, 1.0, -k, -1.0] } else { [g, k, 0.0, 0.0, 1.0] };
                }
            }
            Shape::Eq { bands } => {
                for (j, (shape, t)) in bands.iter().zip(self.pending.iter_mut()).enumerate() {
                    let f = self.params[3 * j].at(tick);
                    let gain = self.params[3 * j + 1].at(tick);
                    let q = self.params[3 * j + 2].at(tick);
                    let a = 10f64.powf(gain / 40.0);
                    let tan = (PI * f / self.rate).tan();
                    *t = match shape {
                        BandShape::Bell => {
                            let k = 1.0 / (q * a);
                            [tan, k, 1.0, k * (a * a - 1.0), 0.0]
                        }
                        BandShape::LowShelf => {
                            let k = 1.0 / q;
                            [tan / a.sqrt(), k, 1.0, k * (a - 1.0), a * a - 1.0]
                        }
                        BandShape::HighShelf => {
                            let k = 1.0 / q;
                            [tan * a.sqrt(), k, a * a, k * (1.0 - a) * a, 1.0 - a * a]
                        }
                    };
                }
            }
        }
    }

    /// Switches to the pending tunings, keeping each section's state-variable
    /// state.
    fn retune(&mut self) {
        for (s, tuning) in self.sections.iter_mut().zip(&self.pending) {
            let (coefficients, to_state, from_state) = biquad(tuning);
            if self.tuned {
                s.history = apply(&from_state, &apply(&s.to_state, &s.history));
            }
            let (a1, a2) = (coefficients[3], coefficients[4]);
            let [w1, w2] = s.history;
            s.zi = [[-a1 * w1[0] - a2 * w2[0], -a1 * w1[1] - a2 * w2[1]], [-a2 * w1[0], -a2 * w1[1]]];
            s.tuning = *tuning;
            s.coefficients = coefficients;
            s.to_state = to_state;
        }
        self.tuned = true;
    }

    pub fn process(&mut self, x: &mut [Frame], clock: Clock) {
        for (i, frame) in x.iter_mut().enumerate() {
            let f = clock.frame + i as i64 * clock.step;
            if i == 0 || f.rem_euclid(CONTROL) == 0 {
                let tick = f.div_euclid(CONTROL) * CONTROL;
                if self.tick != Some(tick) {
                    self.tick = Some(tick);
                    self.tune(tick);
                    if !self.tuned || self.sections.iter().zip(&self.pending).any(|(s, t)| s.tuning != *t) {
                        self.retune();
                    }
                }
            }
            for (c, sample) in frame.iter_mut().enumerate() {
                let mut y = *sample;
                for s in &mut self.sections {
                    let [b0, b1, b2, a1, a2] = s.coefficients;
                    let w = s.zi[0][c] + y;
                    s.zi[0][c] = s.zi[1][c] - w * a1;
                    s.zi[1][c] = -(w * a2);
                    y = b0 * w + b1 * s.history[0][c] + b2 * s.history[1][c];
                    s.history[1][c] = s.history[0][c];
                    s.history[0][c] = w;
                }
                *sample = y;
            }
        }
    }

    /// Continues from `old`'s state, when it has the same sections.
    pub fn take_over(&mut self, old: &Svf) {
        if old.sections.len() == self.sections.len() {
            self.sections.clone_from_slice(&old.sections);
            self.tuned = old.tuned;
            self.tick = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biquad::{band, butter, Cascade};
    use crate::envelope::Envelope;
    use aaw_model::rules::Domain;
    use aaw_model::{Beat, Curve, EqBand, Lane, Point};
    use std::sync::Arc;

    fn noise(n: usize) -> Vec<Frame> {
        let mut s = 9u64;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let v = (s >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
                [v, v * 0.3]
            })
            .collect()
    }

    fn clock(frame: i64) -> Clock {
        Clock {
            frame,
            step: 1,
            total: 1 << 40,
        }
    }

    fn lane(points: &[(f64, f64)], domain: Domain) -> Param {
        let lane = Lane {
            param: "x".into(),
            points: points
                .iter()
                .map(|(at, value)| Point {
                    at: Beat::Float(*at),
                    value: *value,
                    curve: Curve::Linear,
                })
                .collect(),
        };
        Param::new(0.0, Some(&Arc::new(Envelope::new(&lane, domain, 120.0, 48000))))
    }

    fn worst(a: &[Frame], b: &[Frame]) -> f64 {
        a.iter().zip(b).fold(0.0, |m, (x, y)| m.max((x[0] - y[0]).abs()).max((x[1] - y[1]).abs()))
    }

    #[test]
    fn a_still_filter_matches_the_static_sections() {
        for (highpass, order, cutoff) in [(false, 2, 900.0), (true, 4, 200.0), (false, 8, 3000.0)] {
            let x = noise(6000);
            let mut expected = x.clone();
            Cascade::new(butter(order, cutoff, highpass, 48000.0)).process(&mut expected);
            let mut got = x.clone();
            Svf::new(Shape::Filter { highpass, order }, vec![Param::fixed(cutoff)], 48000.0).process(&mut got, clock(0));
            assert!(worst(&expected, &got) < 1e-9, "order {order}: {}", worst(&expected, &got));
        }
    }

    #[test]
    fn still_bands_match_the_cookbook() {
        let bands = [
            (BandShape::Bell, 400.0, -6.0, 1.2),
            (BandShape::LowShelf, 150.0, 5.0, 0.71),
            (BandShape::HighShelf, 6000.0, -4.0, 0.9),
        ];
        let x = noise(6000);
        let mut expected = x.clone();
        let sections = bands
            .iter()
            .map(|(shape, freq_hz, gain_db, q)| {
                let b = EqBand {
                    shape: *shape,
                    freq_hz: *freq_hz,
                    gain_db: *gain_db,
                    q: *q,
                };
                band(&b, 48000.0)
            })
            .collect();
        Cascade::new(sections).process(&mut expected);
        let params = bands.iter().flat_map(|b| [Param::fixed(b.1), Param::fixed(b.2), Param::fixed(b.3)]).collect();
        let mut got = x.clone();
        Svf::new(
            Shape::Eq {
                bands: bands.iter().map(|b| b.0).collect(),
            },
            params,
            48000.0,
        )
        .process(&mut got, clock(0));
        assert!(worst(&expected, &got) < 1e-9, "{}", worst(&expected, &got));
    }

    #[test]
    fn a_sweep_does_not_depend_on_blocks_and_a_jump_stays_bounded() {
        let x = noise(48000);
        let sweep = || Svf::new(Shape::Filter { highpass: false, order: 8 }, vec![lane(&[(0.0, 20000.0), (1.0, 10.0), (1.0, 20000.0)], Domain::Log)], 48000.0);
        let mut whole = x.clone();
        sweep().process(&mut whole, clock(-37));
        let mut pieces = x.clone();
        let mut f = sweep();
        let (mut at, mut size) = (0, 1);
        while at < pieces.len() {
            let n = size.min(pieces.len() - at);
            f.process(&mut pieces[at..at + n], clock(at as i64 - 37));
            at += n;
            size = size * 7 % 613 + 1;
        }
        assert_eq!(whole, pieces);
        let peak = whole.iter().fold(0.0f64, |m, f| m.max(f[0].abs()));
        assert!(peak < 1.0, "burst of {peak}");
        assert!(whole[40000..].iter().any(|f| f[0].abs() > 0.1), "open again after the jump");
    }
}
