//! Exact musical time. A beat is written as an integer, a decimal or a fraction
//! string and converted as Python's `Fraction(str(value))` does, so `0.1` is
//! exactly 1/10 and `"1/3"` exactly a third.

use crate::pyfmt::{float_repr, is_py_space, str_repr};
use crate::value::Value;
use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};

/// A beat value as written: its Python type decides how it is saved and hashed.
#[derive(Clone, Debug, PartialEq)]
pub enum Beat {
    Int(BigInt),
    Float(f64),
    Str(String),
}

impl Beat {
    pub fn int(n: i64) -> Beat {
        Beat::Int(BigInt::from(n))
    }

    /// An exact value as it is written: a whole number, or else a fraction
    /// such as `1/3`.
    pub fn from_exact(x: &BigRational) -> Beat {
        if x.is_integer() {
            Beat::Int(x.to_integer())
        } else {
            Beat::Str(crate::fraction_str(x))
        }
    }

    /// Python `str(value)`.
    pub fn text(&self) -> String {
        match self {
            Beat::Int(n) => n.to_string(),
            Beat::Float(f) => float_repr(*f),
            Beat::Str(s) => s.clone(),
        }
    }

    /// Python `repr(value)`, used in error messages.
    pub fn repr(&self) -> String {
        match self {
            Beat::Str(s) => str_repr(s),
            other => other.text(),
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            Beat::Int(n) => Value::Int(n.clone()),
            Beat::Float(f) => Value::Float(*f),
            Beat::Str(s) => Value::Str(s.clone()),
        }
    }

    /// Python `==` against another written beat, as `exclude_defaults` compares.
    pub fn py_eq(&self, other: &Beat) -> bool {
        crate::value::py_eq(&self.to_value(), &other.to_value())
    }

    /// The exact value, or why it is not a beat.
    pub fn exact(&self) -> Result<BigRational, String> {
        beat(self)
    }
}

pub enum FractionError {
    Invalid,
    ZeroDenominator,
}

/// Consumes `\d+(_\d+)*` from `i`, or nothing. Returns the end and the digits.
fn digits(chars: &[char], mut i: usize) -> (usize, String) {
    let mut out = String::new();
    while i < chars.len() && chars[i].is_ascii_digit() {
        out.push(chars[i]);
        i += 1;
        if i + 1 < chars.len()
            && chars[i] == '_'
            && chars[i + 1].is_ascii_digit()
            && !out.is_empty()
        {
            i += 1;
        }
    }
    (i, out)
}

fn int_of(digits: &str) -> BigInt {
    if digits.is_empty() {
        BigInt::zero()
    } else {
        digits.parse().expect("ascii digits")
    }
}

/// Python's `Fraction(text)` parser (`fractions._RATIONAL_FORMAT`), with ASCII
/// digits: sign, then `n`, `n/d`, or a decimal with an optional exponent.
pub fn parse_fraction(text: &str) -> Result<BigRational, FractionError> {
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    let mut end = chars.len();
    while start < end && is_py_space(chars[start]) {
        start += 1;
    }
    while end > start && is_py_space(chars[end - 1]) {
        end -= 1;
    }
    let chars = &chars[start..end];
    let mut i = 0;
    let negative = match chars.first() {
        Some('-') => {
            i = 1;
            true
        }
        Some('+') => {
            i = 1;
            false
        }
        _ => false,
    };
    let lookahead = chars.get(i).is_some_and(|c| c.is_ascii_digit())
        || (chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()));
    if !lookahead {
        return Err(FractionError::Invalid);
    }
    let (after_num, num) = digits(chars, i);
    let mut numerator = int_of(&num);
    let mut denominator = BigInt::from(1);
    // n/d, with optional whitespace around the slash.
    let mut j = after_num;
    while j < chars.len() && is_py_space(chars[j]) {
        j += 1;
    }
    if chars.get(j) == Some(&'/') {
        j += 1;
        while j < chars.len() && is_py_space(chars[j]) {
            j += 1;
        }
        let (after_denom, denom) = digits(chars, j);
        if denom.is_empty() || after_denom != chars.len() {
            return Err(FractionError::Invalid);
        }
        denominator = int_of(&denom);
        if denominator.is_zero() {
            return Err(FractionError::ZeroDenominator);
        }
    } else {
        let mut k = after_num;
        if chars.get(k) == Some(&'.') {
            let (after_decimal, decimal) = digits(chars, k + 1);
            k = after_decimal;
            let scale = BigInt::from(10).pow(decimal.len() as u32);
            numerator = numerator * &scale + int_of(&decimal);
            denominator = scale;
        }
        if matches!(chars.get(k), Some('e' | 'E')) {
            let mut m = k + 1;
            let exp_negative = match chars.get(m) {
                Some('-') => {
                    m += 1;
                    true
                }
                Some('+') => {
                    m += 1;
                    false
                }
                _ => false,
            };
            let (after_exp, exp) = digits(chars, m);
            if exp.is_empty() {
                return Err(FractionError::Invalid);
            }
            let power = BigInt::from(10).pow(exp.parse::<u32>().map_err(|_| FractionError::Invalid)?);
            if exp_negative {
                denominator *= power;
            } else {
                numerator *= power;
            }
            k = after_exp;
        }
        if k != chars.len() {
            return Err(FractionError::Invalid);
        }
    }
    if negative {
        numerator = -numerator;
    }
    Ok(BigRational::new(numerator, denominator))
}

/// The exact nonnegative value of a written beat.
pub fn beat(value: &Beat) -> Result<BigRational, String> {
    match parse_fraction(&value.text()) {
        Ok(x) if x.is_negative() => Err("Beat values must be nonnegative".into()),
        Ok(x) => Ok(x),
        Err(_) => Err(format!(
            "Invalid beat value {}; use a number or fraction like '1/3'",
            value.repr()
        )),
    }
}

/// The exact value of a written beat that may be negative, as a note's place
/// in its clip is.
pub fn signed_beat(value: &Beat) -> Result<BigRational, String> {
    parse_fraction(&value.text())
        .map_err(|_| format!("Invalid beat value {}; use a number or fraction like '1/3'", value.repr()))
}

/// `Fraction(str(x))` for a float: exact in its shortest decimal form.
pub fn float_fraction(x: f64) -> BigRational {
    match parse_fraction(&float_repr(x)) {
        Ok(f) => f,
        Err(_) => panic!("float_fraction of a non-finite value"),
    }
}

/// The frame a beat falls on, rounding half up from absolute time.
pub fn frame(position: &BigRational, tempo: f64, rate: i64) -> i64 {
    let x = position * BigRational::from_integer(BigInt::from(60 * rate)) / float_fraction(tempo);
    let (n, d) = (x.numer(), x.denom());
    let two = BigInt::from(2);
    (&two * n + d)
        .div_floor(&(&two * d))
        .to_i64()
        .expect("frame fits in i64")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frac(s: &str) -> Option<(String, String)> {
        parse_fraction(s)
            .ok()
            .map(|f| (f.numer().to_string(), f.denom().to_string()))
    }

    fn pair(n: &str, d: &str) -> Option<(String, String)> {
        Some((n.to_string(), d.to_string()))
    }

    #[test]
    fn parses_like_python_fraction() {
        assert_eq!(frac("1/3"), pair("1", "3"));
        assert_eq!(frac(" 1 / 3 "), pair("1", "3"));
        assert_eq!(frac("0.1"), pair("1", "10"));
        assert_eq!(frac("1."), pair("1", "1"));
        assert_eq!(frac(".5"), pair("1", "2"));
        assert_eq!(frac("1e-05"), pair("1", "100000"));
        assert_eq!(frac("1.5E+2"), pair("150", "1"));
        assert_eq!(frac("-2/4"), pair("-1", "2"));
        assert_eq!(frac("1_000"), pair("1000", "1"));
        assert_eq!(frac("0.30000000000000004"), pair("7500000000000001", "25000000000000000"));
        for bad in ["", ".", "1/", "/2", "1.5/2", "1__0", "_1", "1_", "abc", "1e", "1/-2", "--1", "1 2"] {
            assert_eq!(frac(bad), None, "{bad:?}");
        }
        assert!(matches!(parse_fraction("1/0"), Err(FractionError::ZeroDenominator)));
    }

    #[test]
    fn beat_messages_match_python() {
        assert_eq!(
            beat(&Beat::Str("x".into())).unwrap_err(),
            "Invalid beat value 'x'; use a number or fraction like '1/3'"
        );
        assert_eq!(
            beat(&Beat::Str("-1".into())).unwrap_err(),
            "Beat values must be nonnegative"
        );
        assert_eq!(beat(&Beat::Float(-0.0)).unwrap(), BigRational::zero());
    }

    #[test]
    fn frames_round_half_up_from_absolute_time() {
        // 1/3 beat at 120 BPM and 48 kHz is exactly 8000 frames.
        assert_eq!(frame(&parse_fraction("1/3").ok().unwrap(), 120.0, 48000), 8000);
        // Exactly half a frame rounds up; 0.375 of a frame rounds down.
        let half = parse_fraction("1/48000").ok().unwrap();
        assert_eq!(frame(&half, 120.0, 48000), 1);
        let x = parse_fraction("1/64000").ok().unwrap();
        assert_eq!(frame(&x, 120.0, 48000), 0);
    }
}
