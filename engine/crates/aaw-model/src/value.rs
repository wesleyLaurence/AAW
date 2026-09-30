//! A tree of the Python objects that `yaml.safe_load` and `json.loads` produce.
//!
//! Validation reads this tree the way pydantic reads Python objects, and the dumps
//! build it the way `model_dump(mode="json")` does, so equality, hashing and string
//! forms follow Python's rules rather than Rust's.

use crate::pyfmt::{float_repr, str_repr};
use indexmap::IndexMap;
use num_bigint::BigInt;
use num_traits::{FromPrimitive, Zero};
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
pub enum Value {
    None,
    Bool(bool),
    Int(BigInt),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    Dict(Dict),
    /// `!!binary` data, which string fields decode as UTF-8.
    Bytes(Vec<u8>),
    /// A value no model field accepts, named by its Python type: `date`,
    /// `datetime`, `set` or `tuple`.
    Other(&'static str),
}

pub type Dict = IndexMap<Key, Value>;

/// A dict key compared and hashed as Python does, so `1`, `1.0` and `True` collide.
#[derive(Clone, Debug)]
pub struct Key(pub Value);

impl Key {
    pub fn str(s: &str) -> Key {
        Key(Value::Str(s.to_string()))
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.0 {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        py_eq(&self.0, &other.0)
    }
}

impl Eq for Key {}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.0 {
            Value::None => 0u8.hash(state),
            Value::Str(s) => {
                1u8.hash(state);
                s.hash(state);
            }
            Value::Other(name) => {
                2u8.hash(state);
                name.hash(state);
            }
            v => match integral(v) {
                Some(n) => {
                    3u8.hash(state);
                    n.hash(state);
                }
                None => {
                    4u8.hash(state);
                    if let Value::Float(f) = v {
                        f.to_bits().hash(state);
                    }
                }
            },
        }
    }
}

/// The exact integer a number equals, if it is integral.
fn integral(v: &Value) -> Option<BigInt> {
    match v {
        Value::Bool(b) => Some(BigInt::from(*b as u8)),
        Value::Int(n) => Some(n.clone()),
        Value::Float(f) if f.is_finite() && f.fract() == 0.0 => BigInt::from_f64(*f),
        _ => None,
    }
}

fn is_number(v: &Value) -> bool {
    matches!(v, Value::Bool(_) | Value::Int(_) | Value::Float(_))
}

/// Python `==`: numbers compare by exact value across bool, int and float.
pub fn py_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::None, Value::None) => true,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (x, y) if is_number(x) && is_number(y) => match (integral(x), integral(y)) {
            (Some(m), Some(n)) => m == n,
            _ => false,
        },
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| py_eq(p, q))
        }
        (Value::Dict(x), Value::Dict(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| py_eq(v, w)))
        }
        _ => false,
    }
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(s.to_string())
    }

    pub fn int(n: i64) -> Value {
        Value::Int(BigInt::from(n))
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }

    /// Python truthiness.
    pub fn truthy(&self) -> bool {
        match self {
            Value::None => false,
            Value::Bool(b) => *b,
            Value::Int(n) => !n.is_zero(),
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Dict(d) => !d.is_empty(),
            Value::Bytes(b) => !b.is_empty(),
            Value::Other(_) => true,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(d) => d.get(&Key::str(key)),
            _ => None,
        }
    }
}

/// Python `str(value)`.
pub fn py_str(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        other => py_repr(other),
    }
}

/// Python `repr(value)`.
pub fn py_repr(v: &Value) -> String {
    match v {
        Value::None => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => float_repr(*f),
        Value::Str(s) => str_repr(s),
        Value::List(items) => {
            let inner: Vec<String> = items.iter().map(py_repr).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Dict(d) => {
            let inner: Vec<String> = d
                .iter()
                .map(|(k, v)| format!("{}: {}", py_repr(&k.0), py_repr(v)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
        Value::Bytes(b) => format!("b{}", crate::pyfmt::str_repr(&String::from_utf8_lossy(b))),
        Value::Other(name) => format!("<{name}>"),
    }
}

/// A dict with string keys, built in insertion order.
pub fn dict(items: Vec<(&str, Value)>) -> Value {
    Value::Dict(items.into_iter().map(|(k, v)| (Key::str(k), v)).collect())
}
