//! The time signature: how many beats a bar has and which note is one. The
//! song's positions stay quarter-note beats whatever the meter, so a bar of
//! 3/4 is three beats long, a bar of 6/8 three and a bar of 7/8 three and a
//! half. One meter holds for the whole song.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::ToPrimitive;

/// The most beats a bar may have.
pub const MOST_BEATS: u32 = 32;
/// The notes a meter may count: a whole note to a sixteenth.
pub const UNITS: [u32; 5] = [1, 2, 4, 8, 16];
/// The pattern `session.time_signature` is held to in the schema.
pub const PATTERN: &str = "^([1-9]|[12][0-9]|3[0-2])/(1|2|4|8|16)$";
/// What a refused time signature is told.
pub const EXPECTED: &str = "a time signature such as 4/4, 3/4 or 6/8: 1 to 32 beats over 1, 2, 4, 8 or 16";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Meter {
    /// Beats to a bar: the number above the line.
    pub beats: u32,
    /// The note that is one beat, as the number below the line: 4 is a
    /// quarter note, 8 an eighth.
    pub unit: u32,
}

impl Meter {
    pub const COMMON: Meter = Meter { beats: 4, unit: 4 };

    /// `N/D` as written, with spaces around the numbers allowed.
    pub fn parse(text: &str) -> Result<Meter, String> {
        let refused = || format!("Input should be {EXPECTED}");
        let (above, below) = text.split_once('/').ok_or_else(refused)?;
        let beats: u32 = above.trim().parse().map_err(|_| refused())?;
        let unit: u32 = below.trim().parse().map_err(|_| refused())?;
        if !(1..=MOST_BEATS).contains(&beats) || !UNITS.contains(&unit) {
            return Err(refused());
        }
        Ok(Meter { beats, unit })
    }

    pub fn text(&self) -> String {
        format!("{}/{}", self.beats, self.unit)
    }

    /// The note the meter counts, in quarter-note beats: 1 in 3/4, 1/2 in 6/8.
    pub fn beat(&self) -> BigRational {
        BigRational::new(BigInt::from(4), BigInt::from(self.unit))
    }

    /// A bar in quarter-note beats: 3 in 3/4 and in 6/8, 7/2 in 7/8.
    pub fn bar(&self) -> BigRational {
        self.beat() * BigRational::from_integer(BigInt::from(self.beats))
    }

    pub fn beat_f64(&self) -> f64 {
        4.0 / self.unit as f64
    }

    pub fn bar_f64(&self) -> f64 {
        self.bar().to_f64().unwrap_or(f64::NAN)
    }

    /// The bar a song beat is in and the beat of that bar, each from 1, the
    /// beat as the time signature counts it and to a hundredth: where a
    /// marker is, wherever in the bar it fell.
    pub fn bar_beat(&self, beat: f64) -> (i64, f64) {
        const EPS: f64 = 1e-6;
        let bar = (beat / self.bar_f64() + EPS).floor();
        let rest = (beat - bar * self.bar_f64()).max(0.0);
        (bar as i64 + 1, (rest / self.beat_f64() * 100.0).round() / 100.0 + 1.0)
    }

    /// A song beat as a DAW counts it: the bar, then the beat of the bar and
    /// the sixteenth of the beat when it is not on the bar, each from 1; a
    /// beat off the sixteenths is written as the beat commands take it.
    pub fn place(&self, beat: f64) -> String {
        const EPS: f64 = 1e-6;
        let bar_beats = self.bar_f64();
        let unit = self.beat_f64();
        let bar = (beat / bar_beats + EPS).floor();
        let rest = beat - bar * bar_beats;
        let units = rest / unit;
        let sixteenths = rest * 4.0;
        if rest.abs() < EPS {
            format!("{}", bar as i64 + 1)
        } else if (units - units.round()).abs() < EPS {
            format!("{}.{}", bar as i64 + 1, units.round() as i64 + 1)
        } else if (sixteenths - sixteenths.round()).abs() < EPS {
            let s = sixteenths.round() as i64;
            let per_unit = (unit * 4.0).round() as i64;
            format!("{}.{}.{}", bar as i64 + 1, s / per_unit + 1, s % per_unit + 1)
        } else {
            format!("beat {}", number(beat))
        }
    }
}

/// A beat to three places, without trailing zeros.
fn number(x: f64) -> String {
    let s = format!("{x:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

impl Default for Meter {
    fn default() -> Self {
        Meter::COMMON
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meters_are_read_and_measured() {
        assert_eq!(Meter::parse("4/4"), Ok(Meter::COMMON));
        assert_eq!(Meter::parse(" 7 / 8 ").map(|m| m.text()), Ok("7/8".into()));
        for bad in ["4", "0/4", "33/4", "4/3", "4/32", "a/4", "3/4/4", ""] {
            assert!(Meter::parse(bad).unwrap_err().contains("such as 4/4"), "{bad}");
        }
        let seven_eight = Meter { beats: 7, unit: 8 };
        assert_eq!(seven_eight.bar_f64(), 3.5);
        assert_eq!(seven_eight.beat_f64(), 0.5);
        assert_eq!(Meter { beats: 6, unit: 8 }.bar(), BigRational::from_integer(3.into()));
        assert_eq!(Meter { beats: 3, unit: 4 }.bar_f64(), 3.0);
    }

    #[test]
    fn places_count_the_meters_beats() {
        assert_eq!(Meter::COMMON.place(0.0), "1");
        assert_eq!(Meter::COMMON.place(5.0), "2.2");
        assert_eq!(Meter::COMMON.place(5.75), "2.2.4");
        assert_eq!(Meter::COMMON.place(1.0 / 3.0), "beat 0.333");
        let waltz = Meter { beats: 3, unit: 4 };
        assert_eq!(waltz.place(3.0), "2");
        assert_eq!(waltz.place(5.0), "2.3");
        let six_eight = Meter { beats: 6, unit: 8 };
        assert_eq!(six_eight.place(0.5), "1.2");
        assert_eq!(six_eight.place(3.0), "2");
        assert_eq!(six_eight.place(3.75), "2.2.2");
        let seven_eight = Meter { beats: 7, unit: 8 };
        assert_eq!(seven_eight.place(3.5), "2");
        assert_eq!(seven_eight.place(6.5), "2.7");
        assert_eq!(Meter::COMMON.bar_beat(0.0), (1, 1.0));
        assert_eq!(Meter::COMMON.bar_beat(34.417), (9, 3.42));
        assert_eq!(six_eight.bar_beat(3.75), (2, 2.5));
        assert_eq!(seven_eight.bar_beat(3.5), (2, 1.0));
    }
}
