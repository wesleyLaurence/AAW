//! Python's text forms for numbers and strings, and `json.dumps(sort_keys=True)`.
//!
//! Fingerprints hash the JSON Python writes, so these must match it byte for byte.

use crate::value::{Key, Value};

/// Digits and decimal exponent of the shortest representation that round-trips:
/// `x == 0.DIGITS * 10**decpt`. Rust and Python both choose the shortest digit
/// string, and the closest one when several are equally short.
fn shortest(x: f64) -> (String, i32) {
    let s = format!("{:e}", x.abs());
    let (mantissa, exp) = s.split_once('e').expect("exponent form");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    (digits, exp.parse::<i32>().expect("exponent") + 1)
}

fn exponent(e: i32) -> String {
    format!("e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
}

/// Python `repr(float)`.
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.into();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x == 0.0 {
        return format!("{sign}0.0");
    }
    let (digits, decpt) = shortest(x);
    let n = digits.len() as i32;
    let body = if decpt <= -4 || decpt > 16 {
        let mut m = digits[..1].to_string();
        if n > 1 {
            m.push('.');
            m.push_str(&digits[1..]);
        }
        m + &exponent(decpt - 1)
    } else if decpt <= 0 {
        format!("0.{}{}", "0".repeat((-decpt) as usize), digits)
    } else if decpt < n {
        format!("{}.{}", &digits[..decpt as usize], &digits[decpt as usize..])
    } else {
        format!("{}{}.0", digits, "0".repeat((decpt - n) as usize))
    };
    format!("{sign}{body}")
}

fn strip_fraction_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

/// Python `format(x, f".{precision}g")`; `precision` 6 is `f"{x:g}"`.
pub fn format_g(x: f64, precision: usize) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.into();
    }
    let p = precision.max(1);
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let s = format!("{:.*e}", p - 1, x);
    let (mantissa, exp) = s.split_once('e').expect("exponent form");
    let exp: i32 = exp.parse().expect("exponent");
    if exp < -4 || exp >= p as i32 {
        strip_fraction_zeros(mantissa) + &exponent(exp)
    } else {
        strip_fraction_zeros(&format!("{:.*}", (p as i32 - 1 - exp) as usize, x))
    }
}

/// `str.isspace()` and the `\s` of Python's `re` module for text patterns.
pub fn is_py_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0b}'
            | '\u{0c}'
            | '\r'
            | '\u{1c}'..='\u{1f}'
            | ' '
            | '\u{85}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
    )
}

/// Whether `repr` prints a character as is. Python consults the Unicode category
/// tables; this covers the control, format and separator characters that occur
/// in practice, which only affects the text of error messages.
fn printable(c: char) -> bool {
    !(c < ' '
        || ('\u{7f}'..='\u{a0}').contains(&c)
        || c == '\u{ad}'
        || c == '\u{1680}'
        || ('\u{2000}'..='\u{200f}').contains(&c)
        || ('\u{2028}'..='\u{202f}').contains(&c)
        || ('\u{205f}'..='\u{2064}').contains(&c)
        || c == '\u{3000}'
        || ('\u{e000}'..='\u{f8ff}').contains(&c)
        || c == '\u{feff}'
        || ('\u{fff9}'..='\u{fffb}').contains(&c))
}

/// Python `repr(str)`.
pub fn str_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if printable(c) => out.push(c),
            c if (c as u32) < 0x100 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
    out.push(quote);
    out
}

fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{:04x}", unit));
                }
            }
        }
    }
    out.push('"');
}

fn json_into(v: &Value, out: &mut String) {
    match v {
        Value::None => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Float(f) if f.is_nan() => out.push_str("NaN"),
        Value::Float(f) if f.is_infinite() => {
            out.push_str(if *f > 0.0 { "Infinity" } else { "-Infinity" })
        }
        Value::Float(f) => out.push_str(&float_repr(*f)),
        Value::Str(s) => json_str(s, out),
        Value::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                json_into(item, out);
            }
            out.push(']');
        }
        Value::Dict(d) => {
            let mut entries: Vec<(&Key, &Value)> = d.iter().collect();
            entries.sort_by(|a, b| a.0.as_str().cmp(&b.0.as_str()));
            out.push('{');
            for (i, (k, v)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                json_str(k.as_str().expect("string keys"), out);
                out.push_str(": ");
                json_into(v, out);
            }
            out.push('}');
        }
        Value::Bytes(_) => unreachable!("dumps hold no bytes"),
        Value::Other(name) => json_str(name, out),
    }
}

/// `json.dumps(value, sort_keys=True)` for a dump with string keys.
pub fn json_dumps(v: &Value) -> String {
    let mut out = String::new();
    json_into(v, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_repr_matches_python() {
        for (x, s) in [
            (0.1, "0.1"),
            (100.0, "100.0"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (1e-5, "1e-05"),
            (0.0001, "0.0001"),
            (1.5e-7, "1.5e-07"),
            (-0.0, "-0.0"),
            (0.30000000000000004, "0.30000000000000004"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
            (5e-324, "5e-324"),
            (123456789.125, "123456789.125"),
        ] {
            assert_eq!(float_repr(x), s);
        }
    }

    #[test]
    fn format_g_matches_python() {
        for (x, s) in [
            (-96.0, "-96"),
            (0.1, "0.1"),
            (20000.0, "20000"),
            (4294967295.0, "4.29497e+09"),
            (1e-5, "1e-05"),
            (0.0001, "0.0001"),
            (123456.0, "123456"),
            (1234567.0, "1.23457e+06"),
            (-0.1, "-0.1"),
        ] {
            assert_eq!(format_g(x, 6), s);
        }
        assert_eq!(format_g(12.34567, 4), "12.35");
        assert_eq!(format_g(0.000123456, 4), "0.0001235");
        assert_eq!(format_g(10.001, 4), "10");
    }

    #[test]
    fn str_repr_quotes_like_python() {
        assert_eq!(str_repr("abc"), "'abc'");
        assert_eq!(str_repr("it's"), "\"it's\"");
        assert_eq!(str_repr("a'b\"c"), "'a\\'b\"c'");
        assert_eq!(str_repr("tab\there\n"), "'tab\\there\\n'");
        assert_eq!(str_repr("\u{1}é"), "'\\x01é'");
    }

    #[test]
    fn json_escapes_like_python() {
        let v = Value::str("é\u{1f600}\u{7f}\"");
        assert_eq!(json_dumps(&v), "\"\\u00e9\\ud83d\\ude00\\u007f\\\"\"");
    }
}
