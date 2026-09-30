//! Field validation with pydantic's coercions and error types.
//!
//! Every error pydantic would report is collected with its location, type and
//! message, so the Rust model rejects the same documents with equivalent errors.

use crate::beat::Beat;
use crate::pyfmt::str_repr;
use crate::value::{py_str, Dict, Key, Value};
use indexmap::IndexMap;
use num_bigint::BigInt;
use num_traits::{FromPrimitive, ToPrimitive, Zero};
use regex::Regex;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Loc {
    Key(String),
    Index(usize),
}

impl fmt::Display for Loc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Loc::Key(k) => f.write_str(k),
            Loc::Index(i) => write!(f, "{i}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ErrorItem {
    pub loc: Vec<Loc>,
    pub kind: &'static str,
    pub msg: String,
}

impl ErrorItem {
    /// The location as pydantic prints it, e.g. `tracks.0.pads.kick`.
    pub fn loc_text(&self) -> String {
        self.loc
            .iter()
            .map(Loc::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// A rejected document, printed in pydantic's layout without the input echo.
#[derive(Clone, Debug)]
pub struct ValidationError {
    pub errors: Vec<ErrorItem>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.errors.len();
        write!(
            f,
            "{n} validation error{} for Project",
            if n == 1 { "" } else { "s" }
        )?;
        for e in &self.errors {
            if !e.loc.is_empty() {
                write!(f, "\n{}", e.loc_text())?;
            }
            write!(f, "\n  {} [type={}]", e.msg, e.kind)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

#[derive(Default)]
pub struct Ctx {
    loc: Vec<Loc>,
    pub errors: Vec<ErrorItem>,
}

impl Ctx {
    pub fn error(&mut self, kind: &'static str, msg: impl Into<String>) {
        self.errors.push(ErrorItem {
            loc: self.loc.clone(),
            kind,
            msg: msg.into(),
        });
    }

    /// A `ValueError` raised by a validator.
    pub fn value_error(&mut self, msg: impl fmt::Display) {
        self.error("value_error", format!("Value error, {msg}"));
    }

    pub fn at<T>(&mut self, loc: Loc, f: impl FnOnce(&mut Ctx) -> T) -> T {
        self.loc.push(loc);
        let result = f(self);
        self.loc.pop();
        result
    }

    pub fn key<T>(&mut self, key: &str, f: impl FnOnce(&mut Ctx) -> T) -> T {
        self.at(Loc::Key(key.to_string()), f)
    }

    pub fn count(&self) -> usize {
        self.errors.len()
    }
}

/// A numeric limit as declared, kept with its source text for messages.
#[derive(Clone, Copy)]
pub struct Limit {
    pub value: f64,
    pub text: &'static str,
}

#[derive(Clone, Copy, Default)]
pub struct Bounds {
    pub ge: Option<Limit>,
    pub gt: Option<Limit>,
    pub le: Option<Limit>,
    pub lt: Option<Limit>,
}

fn limit(text: &'static str) -> Option<Limit> {
    Some(Limit {
        value: text.parse().expect("numeric limit"),
        text,
    })
}

impl Bounds {
    pub const NONE: Bounds = Bounds {
        ge: None,
        gt: None,
        le: None,
        lt: None,
    };

    pub fn ge_le(low: &'static str, high: &'static str) -> Bounds {
        Bounds {
            ge: limit(low),
            le: limit(high),
            ..Bounds::NONE
        }
    }

    pub fn ge(low: &'static str) -> Bounds {
        Bounds {
            ge: limit(low),
            ..Bounds::NONE
        }
    }

    pub fn gt(low: &'static str) -> Bounds {
        Bounds {
            gt: limit(low),
            ..Bounds::NONE
        }
    }

    pub fn gt_le(low: &'static str, high: &'static str) -> Bounds {
        Bounds {
            gt: limit(low),
            le: limit(high),
            ..Bounds::NONE
        }
    }

    /// Whether `value` passes, recording pydantic's error if not.
    fn check(&self, ctx: &mut Ctx, value: f64) -> bool {
        if let Some(l) = self.le {
            if !(value <= l.value) {
                ctx.error(
                    "less_than_equal",
                    format!("Input should be less than or equal to {}", l.text),
                );
                return false;
            }
        }
        if let Some(l) = self.lt {
            if !(value < l.value) {
                ctx.error("less_than", format!("Input should be less than {}", l.text));
                return false;
            }
        }
        if let Some(l) = self.ge {
            if !(value >= l.value) {
                ctx.error(
                    "greater_than_equal",
                    format!("Input should be greater than or equal to {}", l.text),
                );
                return false;
            }
        }
        if let Some(l) = self.gt {
            if !(value > l.value) {
                ctx.error("greater_than", format!("Input should be greater than {}", l.text));
                return false;
            }
        }
        true
    }
}

/// pydantic's number text: trimmed, with underscores allowed only between digits.
fn number_text(s: &str) -> Option<String> {
    let t: Vec<char> = s.trim().chars().collect();
    let mut out = String::with_capacity(t.len());
    for (i, &c) in t.iter().enumerate() {
        if c == '_' {
            let ok = i > 0
                && i + 1 < t.len()
                && t[i - 1].is_ascii_digit()
                && t[i + 1].is_ascii_digit();
            if !ok {
                return None;
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

fn parse_float_text(s: &str) -> Option<f64> {
    let t = number_text(s)?;
    if t.is_empty() || t.starts_with("0x") || t.starts_with("0X") {
        return None;
    }
    t.parse::<f64>().ok()
}

fn parse_int_text(s: &str) -> Option<BigInt> {
    let t = number_text(s)?;
    // A decimal point followed only by zeros is accepted, as in "100.0".
    let int_part = match t.split_once('.') {
        Some((i, f)) if f.chars().all(|c| c == '0') => i,
        Some(_) => return None,
        None => t.as_str(),
    };
    let digits = int_part.strip_prefix(['+', '-']).unwrap_or(int_part);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    int_part.parse().ok()
}

/// A `float` field with `allow_inf_nan=False`.
pub fn float(ctx: &mut Ctx, v: &Value, bounds: Bounds) -> Option<f64> {
    let x = match v {
        Value::Float(f) => *f,
        Value::Bool(b) => *b as u8 as f64,
        Value::Int(n) => match n.to_f64() {
            Some(f) if f.is_finite() => f,
            _ => {
                ctx.error("float_type", "Input should be a valid number");
                return None;
            }
        },
        Value::Str(s) => match parse_float_text(s) {
            Some(f) => f,
            None => {
                ctx.error(
                    "float_parsing",
                    "Input should be a valid number, unable to parse string as a number",
                );
                return None;
            }
        },
        _ => {
            ctx.error("float_type", "Input should be a valid number");
            return None;
        }
    };
    if !x.is_finite() {
        ctx.error("finite_number", "Input should be a finite number");
        return None;
    }
    bounds.check(ctx, x).then_some(x)
}

/// An `int` field, bounded to fit i64.
pub fn int(ctx: &mut Ctx, v: &Value, bounds: Bounds) -> Option<i64> {
    let n = match v {
        Value::Int(n) => n.clone(),
        Value::Bool(b) => BigInt::from(*b as u8),
        Value::Float(f) if !f.is_finite() => {
            ctx.error("finite_number", "Input should be a finite number");
            return None;
        }
        Value::Float(f) if f.fract() != 0.0 => {
            ctx.error(
                "int_from_float",
                "Input should be a valid integer, got a number with a fractional part",
            );
            return None;
        }
        Value::Float(f) => BigInt::from_f64(*f).expect("finite integral float"),
        Value::Str(s) => match parse_int_text(s) {
            Some(n) => n,
            None => {
                ctx.error(
                    "int_parsing",
                    "Input should be a valid integer, unable to parse string as an integer",
                );
                return None;
            }
        },
        _ => {
            ctx.error("int_type", "Input should be a valid integer");
            return None;
        }
    };
    // Every int field's bounds fit in i64, so larger inputs fail them.
    let x = match n.to_i64() {
        Some(x) => x as f64,
        None => {
            if n > BigInt::zero() {
                f64::INFINITY
            } else {
                f64::NEG_INFINITY
            }
        }
    };
    if !bounds.check(ctx, x) {
        return None;
    }
    n.to_i64()
}

pub fn boolean(ctx: &mut Ctx, v: &Value) -> Option<bool> {
    let parsing = "Input should be a valid boolean, unable to interpret input";
    // Numbers are read as a 64-bit integer first; others are the wrong type.
    let whole = match v {
        Value::Int(n) => n.to_i64(),
        Value::Float(f) if f.fract() == 0.0 && *f >= -(2f64.powi(63)) && *f < 2f64.powi(63) => {
            Some(*f as i64)
        }
        _ => None,
    };
    match v {
        Value::Bool(b) => Some(*b),
        Value::Int(_) | Value::Float(_) if whole.is_some() => match whole {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => {
                ctx.error("bool_parsing", parsing);
                None
            }
        },
        Value::Str(s) => match s.to_lowercase().as_str() {
            "0" | "off" | "f" | "false" | "n" | "no" => Some(false),
            "1" | "on" | "t" | "true" | "y" | "yes" => Some(true),
            _ => {
                ctx.error("bool_parsing", parsing);
                None
            }
        },
        _ => {
            ctx.error("bool_type", "Input should be a valid boolean");
            None
        }
    }
}

pub fn string(ctx: &mut Ctx, v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::Bytes(b) => match String::from_utf8(b.clone()) {
            Ok(s) => Some(s),
            Err(_) => {
                ctx.error(
                    "string_unicode",
                    "Input should be a valid string, unable to parse raw data as a unicode string",
                );
                None
            }
        },
        _ => {
            ctx.error("string_type", "Input should be a valid string");
            None
        }
    }
}

pub fn pattern(ctx: &mut Ctx, v: &Value, re: &Regex, source: &str) -> Option<String> {
    let s = string(ctx, v)?;
    if re.is_match(&s) {
        Some(s)
    } else {
        ctx.error(
            "string_pattern_mismatch",
            format!("String should match pattern '{source}'"),
        );
        None
    }
}

fn expected(options: &[String]) -> String {
    match options {
        [one] => one.clone(),
        [init @ .., last] => format!("{} or {last}", init.join(", ")),
        [] => String::new(),
    }
}

pub fn literal_str(ctx: &mut Ctx, v: &Value, options: &[&'static str]) -> Option<&'static str> {
    if let Value::Str(s) = v {
        if let Some(o) = options.iter().find(|o| **o == s) {
            return Some(o);
        }
    }
    let reprs: Vec<String> = options.iter().map(|o| str_repr(o)).collect();
    ctx.error(
        "literal_error",
        format!("Input should be {}", expected(&reprs)),
    );
    None
}

pub fn literal_int(ctx: &mut Ctx, v: &Value, options: &[i64]) -> Option<i64> {
    let n = match v {
        Value::Int(n) => n.to_i64(),
        Value::Bool(b) => Some(*b as i64),
        Value::Float(f) if f.is_finite() && f.fract() == 0.0 => BigInt::from_f64(*f).and_then(|n| n.to_i64()),
        _ => None,
    };
    if let Some(n) = n.filter(|n| options.contains(n)) {
        return Some(n);
    }
    let reprs: Vec<String> = options.iter().map(|o| o.to_string()).collect();
    ctx.error(
        "literal_error",
        format!("Input should be {}", expected(&reprs)),
    );
    None
}

/// `Beat = int | float | str` in pydantic's smart union mode.
pub fn beat(ctx: &mut Ctx, v: &Value) -> Option<Beat> {
    match v {
        Value::Int(n) => Some(Beat::Int(n.clone())),
        Value::Bool(b) => Some(Beat::int(*b as i64)),
        Value::Float(f) if f.is_finite() => Some(Beat::Float(*f)),
        Value::Str(s) => Some(Beat::Str(s.clone())),
        Value::Float(_) => {
            ctx.key("int", |c| c.error("finite_number", "Input should be a finite number"));
            ctx.key("float", |c| c.error("finite_number", "Input should be a finite number"));
            ctx.key("str", |c| c.error("string_type", "Input should be a valid string"));
            None
        }
        _ => {
            ctx.key("int", |c| c.error("int_type", "Input should be a valid integer"));
            ctx.key("float", |c| c.error("float_type", "Input should be a valid number"));
            ctx.key("str", |c| c.error("string_type", "Input should be a valid string"));
            None
        }
    }
}

/// `T | None`: None passes through.
pub fn optional<T>(
    ctx: &mut Ctx,
    v: &Value,
    f: impl FnOnce(&mut Ctx, &Value) -> Option<T>,
) -> Option<Option<T>> {
    match v {
        Value::None => Some(None),
        other => f(ctx, other).map(Some),
    }
}

pub fn list<T>(
    ctx: &mut Ctx,
    v: &Value,
    min: usize,
    max: Option<usize>,
    mut item: impl FnMut(&mut Ctx, &Value) -> Option<T>,
) -> Option<Vec<T>> {
    let Value::List(items) = v else {
        ctx.error("list_type", "Input should be a valid list");
        return None;
    };
    if let Some(max) = max {
        if items.len() > max {
            ctx.error(
                "too_long",
                format!(
                    "List should have at most {max} item{} after validation, not {}",
                    if max == 1 { "" } else { "s" },
                    items.len()
                ),
            );
            return None;
        }
    }
    let before = ctx.count();
    let mut out = Vec::with_capacity(items.len());
    for (i, x) in items.iter().enumerate() {
        if let Some(t) = ctx.at(Loc::Index(i), |c| item(c, x)) {
            out.push(t);
        }
    }
    if ctx.count() > before {
        return None;
    }
    if out.len() < min {
        ctx.error(
            "too_short",
            format!(
                "List should have at least {min} item{} after validation, not {}",
                if min == 1 { "" } else { "s" },
                out.len()
            ),
        );
        return None;
    }
    Some(out)
}

/// `dict[str, T]`.
pub fn str_dict<T>(
    ctx: &mut Ctx,
    v: &Value,
    mut item: impl FnMut(&mut Ctx, &Value) -> Option<T>,
) -> Option<IndexMap<String, T>> {
    let Value::Dict(d) = v else {
        ctx.error("dict_type", "Input should be a valid dictionary");
        return None;
    };
    let before = ctx.count();
    let mut out = IndexMap::with_capacity(d.len());
    for (k, x) in d {
        match k.as_str() {
            Some(key) => {
                if let Some(t) = ctx.key(key, |c| item(c, x)) {
                    out.insert(key.to_string(), t);
                }
            }
            // pydantic reports the key and still validates its value.
            None => ctx.at(Loc::Key(py_str(&k.0)), |c| {
                c.key("[key]", |c| c.error("string_type", "Input should be a valid string"));
                item(c, x);
            }),
        }
    }
    (ctx.count() == before).then_some(out)
}

/// The input of one model: its fields by name, checked for missing and extra keys.
pub struct Fields<'a> {
    dict: &'a Dict,
    known: &'static [&'static str],
}

impl<'a> Fields<'a> {
    /// Starts a model, or records pydantic's type error for a non-mapping input.
    pub fn of(ctx: &mut Ctx, v: &'a Value, model: &str, known: &'static [&'static str]) -> Option<Self> {
        match v {
            Value::Dict(dict) => Some(Fields { dict, known }),
            _ => {
                ctx.error(
                    "model_type",
                    format!("Input should be a valid dictionary or instance of {model}"),
                );
                None
            }
        }
    }

    /// A tagged union member, whose non-mapping inputs were already rejected.
    pub fn member(v: &'a Value, known: &'static [&'static str]) -> Self {
        let Value::Dict(dict) = v else {
            unreachable!("union members are mappings")
        };
        Fields { dict, known }
    }

    pub fn raw(&self, name: &str) -> Option<&'a Value> {
        debug_assert!(self.known.contains(&name));
        self.dict.get(&Key::str(name))
    }

    /// A required field.
    pub fn req<T>(
        &self,
        ctx: &mut Ctx,
        name: &str,
        f: impl FnOnce(&mut Ctx, &Value) -> Option<T>,
    ) -> Option<T> {
        ctx.key(name, |c| match self.raw(name) {
            Some(v) => f(c, v),
            None => {
                c.error("missing", "Field required");
                None
            }
        })
    }

    /// A field with a default.
    pub fn opt<T>(
        &self,
        ctx: &mut Ctx,
        name: &str,
        default: T,
        f: impl FnOnce(&mut Ctx, &Value) -> Option<T>,
    ) -> Option<T> {
        match self.raw(name) {
            Some(v) => ctx.key(name, |c| f(c, v)),
            None => Some(default),
        }
    }

    /// Records a key the model does not declare, as `extra="forbid"` does.
    pub fn finish(&self, ctx: &mut Ctx) {
        for k in self.dict.keys() {
            match k.as_str() {
                Some(name) if self.known.contains(&name) => {}
                Some(name) => ctx.key(name, |c| {
                    c.error("extra_forbidden", "Extra inputs are not permitted")
                }),
                None => ctx.at(Loc::Key(py_str(&k.0)), |c| {
                    c.error("invalid_key", "Keys should be strings")
                }),
            }
        }
    }
}
