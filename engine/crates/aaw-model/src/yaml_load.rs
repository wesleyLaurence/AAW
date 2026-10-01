//! `yaml.safe_load`: libyaml events composed and constructed with PyYAML's YAML 1.1
//! resolver and safe constructors, so `010` is 8, `1e5` is a string, `yes` is true
//! and merge keys apply, as songs have always been read.

use crate::value::{Dict, Key, Value};
use libyaml_safer::{EventData, Mark, Parser};
use num_bigint::BigInt;
use num_traits::{Num, Zero};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

pub const NULL: &str = "tag:yaml.org,2002:null";
pub const BOOL: &str = "tag:yaml.org,2002:bool";
pub const INT: &str = "tag:yaml.org,2002:int";
pub const FLOAT: &str = "tag:yaml.org,2002:float";
pub const STR: &str = "tag:yaml.org,2002:str";
pub const SEQ: &str = "tag:yaml.org,2002:seq";
pub const MAP: &str = "tag:yaml.org,2002:map";
const MERGE: &str = "tag:yaml.org,2002:merge";
const VALUE: &str = "tag:yaml.org,2002:value";
const TIMESTAMP: &str = "tag:yaml.org,2002:timestamp";

// PyYAML's implicit resolvers. Python's `$` also matches before a final newline.
static RESOLVERS: LazyLock<Vec<(&'static str, Regex, &'static str)>> = LazyLock::new(|| {
    let r = |p: &str| Regex::new(&format!(r"\A(?:{p})\n?\z")).expect("resolver pattern");
    vec![
        (
            BOOL,
            r("yes|Yes|YES|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF"),
            "yYnNtTfFoO",
        ),
        (
            FLOAT,
            r(r"[-+]?(?:[0-9][0-9_]*)\.[0-9_]*(?:[eE][-+][0-9]+)?|\.[0-9][0-9_]*(?:[eE][-+][0-9]+)?|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN)"),
            "-+0123456789.",
        ),
        (
            INT,
            r(r"[-+]?0b[0-1_]+|[-+]?0[0-7_]+|[-+]?(?:0|[1-9][0-9_]*)|[-+]?0x[0-9a-fA-F_]+|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+"),
            "-+0123456789",
        ),
        (MERGE, r("<<"), "<"),
        (NULL, r("~|null|Null|NULL|"), "~nN"),
        (
            TIMESTAMP,
            r(r"[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]|[0-9][0-9][0-9][0-9]-[0-9][0-9]?-[0-9][0-9]?(?:[Tt]|[ \t]+)[0-9][0-9]?:[0-9][0-9]:[0-9][0-9](?:\.[0-9]*)?(?:[ \t]*(?:Z|[-+][0-9][0-9]?(?::[0-9][0-9])?))?"),
            "0123456789",
        ),
        (VALUE, r("="), "="),
    ]
});

static DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\A[0-9]{4}-[0-9]{2}-[0-9]{2}\n?\z").expect("date pattern"));

/// The tag PyYAML gives a scalar, trying implicit resolvers only for plain scalars.
pub fn resolve_scalar(value: &str, plain: bool) -> &'static str {
    if plain {
        if value.is_empty() {
            return NULL;
        }
        let first = value.chars().next().expect("nonempty");
        for (tag, pattern, firsts) in RESOLVERS.iter() {
            if firsts.contains(first) && pattern.is_match(value) {
                return tag;
            }
        }
    }
    STR
}

enum Kind {
    Scalar(String),
    Seq(Vec<usize>),
    Map(Vec<(usize, usize)>),
}

struct Node {
    tag: String,
    kind: Kind,
    mark: Mark,
}

fn at(mark: Mark) -> String {
    format!("line {}, column {}", mark.line + 1, mark.column + 1)
}

struct Composer<'a> {
    parser: Parser<&'a mut &'a [u8]>,
    nodes: Vec<Node>,
    anchors: HashMap<String, usize>,
}

impl Composer<'_> {
    fn next(&mut self) -> Result<libyaml_safer::Event, String> {
        self.parser.parse().map_err(|e| e.to_string())
    }

    fn anchor(&mut self, anchor: Option<String>, id: usize, mark: Mark) -> Result<(), String> {
        if let Some(name) = anchor {
            if let Some(first) = self.anchors.get(&name) {
                return Err(format!(
                    "found duplicate anchor {name:?}; first occurrence at {}, second occurrence at {}",
                    at(self.nodes[*first].mark),
                    at(mark)
                ));
            }
            self.anchors.insert(name, id);
        }
        Ok(())
    }

    fn push(&mut self, tag: String, kind: Kind, mark: Mark) -> usize {
        self.nodes.push(Node { tag, kind, mark });
        self.nodes.len() - 1
    }

    /// Composes the node that starts with `event`; None for a collection end.
    fn compose(&mut self, event: libyaml_safer::Event) -> Result<Option<usize>, String> {
        let mark = event.start_mark;
        match event.data {
            EventData::Alias { anchor } => match self.anchors.get(&anchor) {
                Some(id) => Ok(Some(*id)),
                None => Err(format!("found undefined alias {anchor:?} at {}", at(mark))),
            },
            EventData::Scalar {
                anchor,
                tag,
                value,
                plain_implicit,
                ..
            } => {
                let tag = match tag {
                    Some(t) if t != "!" => t,
                    _ => resolve_scalar(&value, plain_implicit).to_string(),
                };
                let id = self.push(tag, Kind::Scalar(value), mark);
                self.anchor(anchor, id, mark)?;
                Ok(Some(id))
            }
            EventData::SequenceStart { anchor, tag, .. } => {
                let tag = tag.filter(|t| t != "!").unwrap_or_else(|| SEQ.to_string());
                let id = self.push(tag, Kind::Seq(Vec::new()), mark);
                self.anchor(anchor, id, mark)?;
                let mut items = Vec::new();
                loop {
                    let e = self.next()?;
                    match self.compose(e)? {
                        Some(item) => items.push(item),
                        None => break,
                    }
                }
                self.nodes[id].kind = Kind::Seq(items);
                Ok(Some(id))
            }
            EventData::MappingStart { anchor, tag, .. } => {
                let tag = tag.filter(|t| t != "!").unwrap_or_else(|| MAP.to_string());
                let id = self.push(tag, Kind::Map(Vec::new()), mark);
                self.anchor(anchor, id, mark)?;
                let mut pairs = Vec::new();
                loop {
                    let e = self.next()?;
                    let Some(key) = self.compose(e)? else { break };
                    let e = self.next()?;
                    let value = self.compose(e)?.ok_or("mapping ended after a key")?;
                    pairs.push((key, value));
                }
                self.nodes[id].kind = Kind::Map(pairs);
                Ok(Some(id))
            }
            EventData::SequenceEnd | EventData::MappingEnd => Ok(None),
            _ => Err(format!("unexpected YAML event at {}", at(mark))),
        }
    }
}

struct Constructor {
    nodes: Vec<Node>,
    active: Vec<bool>,
}

fn scalar_error(expected: &str, node: &Node) -> String {
    let found = match node.kind {
        Kind::Scalar(_) => "scalar",
        Kind::Seq(_) => "sequence",
        Kind::Map(_) => "mapping",
    };
    format!("expected a {expected} node, but found {found} at {}", at(node.mark))
}

/// Python `int(text, base)` for text already stripped of underscores and sign.
fn int_base(text: &str, base: u32) -> Option<BigInt> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    BigInt::from_str_radix(t, base).ok()
}

/// Python `float(text)` for the strings PyYAML hands it.
fn py_float(text: &str) -> Option<f64> {
    let t = text.trim_matches(crate::pyfmt::is_py_space);
    if t.is_empty() || t.contains('_') {
        return None;
    }
    t.parse::<f64>().ok()
}

impl Constructor {
    fn scalar_text(&self, id: usize) -> Result<String, String> {
        let node = &self.nodes[id];
        match &node.kind {
            Kind::Scalar(s) => Ok(s.clone()),
            Kind::Map(pairs) => {
                for (k, v) in pairs {
                    if self.nodes[*k].tag == VALUE {
                        return self.scalar_text(*v);
                    }
                }
                Err(scalar_error("scalar", node))
            }
            Kind::Seq(_) => Err(scalar_error("scalar", node)),
        }
    }

    /// PyYAML's `flatten_mapping`: merge keys first, then the mapping's own keys.
    fn flatten(&mut self, id: usize) -> Result<(), String> {
        let Kind::Map(pairs) = &self.nodes[id].kind else {
            return Ok(());
        };
        let mut pairs = pairs.clone();
        let mut merge = Vec::new();
        let mut index = 0;
        while index < pairs.len() {
            let (k, v) = pairs[index];
            if self.nodes[k].tag == MERGE {
                pairs.remove(index);
                match &self.nodes[v].kind {
                    Kind::Map(_) => {
                        self.flatten(v)?;
                        if let Kind::Map(sub) = &self.nodes[v].kind {
                            merge.extend(sub.clone());
                        }
                    }
                    Kind::Seq(items) => {
                        let items = items.clone();
                        let mut submerge = Vec::new();
                        for sub in items {
                            if !matches!(self.nodes[sub].kind, Kind::Map(_)) {
                                return Err(format!(
                                    "while constructing a mapping: expected a mapping for merging at {}",
                                    at(self.nodes[sub].mark)
                                ));
                            }
                            self.flatten(sub)?;
                            if let Kind::Map(p) = &self.nodes[sub].kind {
                                submerge.push(p.clone());
                            }
                        }
                        submerge.reverse();
                        for p in submerge {
                            merge.extend(p);
                        }
                    }
                    Kind::Scalar(_) => {
                        return Err(format!(
                            "while constructing a mapping: expected a mapping or list of mappings for merging at {}",
                            at(self.nodes[v].mark)
                        ));
                    }
                }
            } else {
                if self.nodes[k].tag == VALUE {
                    self.nodes[k].tag = STR.to_string();
                }
                index += 1;
            }
        }
        if !merge.is_empty() {
            merge.extend(pairs);
            pairs = merge;
        }
        self.nodes[id].kind = Kind::Map(pairs);
        Ok(())
    }

    fn construct(&mut self, id: usize) -> Result<Value, String> {
        if self.active[id] {
            return Err(format!(
                "recursive YAML structure at {}",
                at(self.nodes[id].mark)
            ));
        }
        self.active[id] = true;
        let result = self.construct_inner(id);
        self.active[id] = false;
        result
    }

    fn construct_inner(&mut self, id: usize) -> Result<Value, String> {
        let tag = self.nodes[id].tag.clone();
        let mark = self.nodes[id].mark;
        match tag.as_str() {
            MAP | "tag:yaml.org,2002:set" => {
                if !matches!(self.nodes[id].kind, Kind::Map(_)) {
                    return Err(scalar_error("mapping", &self.nodes[id]));
                }
                self.flatten(id)?;
                let Kind::Map(pairs) = &self.nodes[id].kind else {
                    unreachable!()
                };
                let pairs = pairs.clone();
                let mut dict = Dict::new();
                for (k, v) in pairs {
                    let key = self.construct(k)?;
                    if matches!(key, Value::List(_) | Value::Dict(_) | Value::Other("set")) {
                        return Err(format!(
                            "while constructing a mapping: found unhashable key at {}",
                            at(self.nodes[k].mark)
                        ));
                    }
                    let value = self.construct(v)?;
                    dict.insert(Key(key), value);
                }
                if tag == MAP {
                    Ok(Value::Dict(dict))
                } else {
                    Ok(Value::Other("set"))
                }
            }
            SEQ | "tag:yaml.org,2002:omap" | "tag:yaml.org,2002:pairs" => {
                let Kind::Seq(items) = &self.nodes[id].kind else {
                    return Err(scalar_error("sequence", &self.nodes[id]));
                };
                let items = items.clone();
                if tag != SEQ {
                    return Ok(Value::List(items.iter().map(|_| Value::Other("tuple")).collect()));
                }
                let mut list = Vec::with_capacity(items.len());
                for item in items {
                    list.push(self.construct(item)?);
                }
                Ok(Value::List(list))
            }
            STR => Ok(Value::Str(self.scalar_text(id)?)),
            NULL => {
                self.scalar_text(id)?;
                Ok(Value::None)
            }
            BOOL => {
                let text = self.scalar_text(id)?;
                match text.to_lowercase().as_str() {
                    "yes" | "true" | "on" => Ok(Value::Bool(true)),
                    "no" | "false" | "off" => Ok(Value::Bool(false)),
                    _ => Err(format!("invalid boolean {text:?} at {}", at(mark))),
                }
            }
            INT => {
                let text = self.scalar_text(id)?.replace('_', "");
                construct_int(&text).ok_or_else(|| format!("invalid int {text:?} at {}", at(mark)))
            }
            FLOAT => {
                let text = self.scalar_text(id)?.replace('_', "").to_lowercase();
                construct_float(&text)
                    .map(Value::Float)
                    .ok_or_else(|| format!("invalid float {text:?} at {}", at(mark)))
            }
            "tag:yaml.org,2002:binary" => {
                let text = self.scalar_text(id)?;
                decode_base64(&text)
                    .map(Value::Bytes)
                    .ok_or_else(|| format!("failed to decode base64 data at {}", at(mark)))
            }
            TIMESTAMP => {
                let text = self.scalar_text(id)?;
                Ok(Value::Other(if DATE.is_match(&text) { "date" } else { "datetime" }))
            }
            other => Err(format!(
                "could not determine a constructor for the tag {other:?} at {}",
                at(mark)
            )),
        }
    }
}

/// Python's `base64.decodebytes`: characters outside the alphabet are skipped and
/// data ends at padding.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let sextet = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut bits: Vec<u8> = Vec::new();
    for c in text.bytes() {
        if c == b'=' {
            break;
        }
        if let Some(s) = sextet(c) {
            bits.push(s);
        }
    }
    if bits.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bits.len() * 3 / 4);
    for chunk in bits.chunks(4) {
        let n = chunk.iter().fold(0u32, |acc, s| (acc << 6) | *s as u32) << (6 * (4 - chunk.len()));
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..chunk.len() - 1]);
    }
    Some(out)
}

/// PyYAML's `construct_yaml_int`, after underscores are removed.
fn construct_int(text: &str) -> Option<Value> {
    let first = text.chars().next()?;
    let negative = first == '-';
    let body = if first == '-' || first == '+' {
        &text[1..]
    } else {
        text
    };
    let magnitude = if body == "0" {
        BigInt::zero()
    } else if let Some(bin) = body.strip_prefix("0b") {
        int_base(bin, 2)?
    } else if let Some(hex) = body.strip_prefix("0x") {
        int_base(hex, 16)?
    } else if body.starts_with('0') {
        int_base(body, 8)?
    } else if body.contains(':') {
        let mut total = BigInt::zero();
        for part in body.split(':') {
            total = total * 60 + int_base(part, 10)?;
        }
        total
    } else {
        int_base(body, 10)?
    };
    Some(Value::Int(if negative { -magnitude } else { magnitude }))
}

/// PyYAML's `construct_yaml_float`, after underscores are removed and lowercased.
fn construct_float(text: &str) -> Option<f64> {
    let first = text.chars().next()?;
    let sign = if first == '-' { -1.0 } else { 1.0 };
    let body = if first == '-' || first == '+' {
        &text[1..]
    } else {
        text
    };
    if body == ".inf" {
        Some(sign * f64::INFINITY)
    } else if body == ".nan" {
        Some(f64::NAN)
    } else if body.contains(':') {
        let mut total = 0.0;
        let mut base = 1.0;
        for part in body.split(':').rev() {
            total += py_float(part)? * base;
            base *= 60.0;
        }
        Some(sign * total)
    } else {
        Some(sign * py_float(body)?)
    }
}

/// Parses one YAML document as `yaml.safe_load` does. An empty stream is None.
pub fn load(text: &str) -> Result<Value, String> {
    let mut bytes = text.as_bytes();
    let input: &mut &[u8] = &mut bytes;
    let mut parser = Parser::new();
    parser.set_input_string(input);
    let mut composer = Composer {
        parser,
        nodes: Vec::new(),
        anchors: HashMap::new(),
    };
    let mut root = None;
    loop {
        let event = composer.next()?;
        match event.data {
            EventData::StreamStart { .. } | EventData::DocumentEnd { .. } => {}
            EventData::StreamEnd => break,
            EventData::DocumentStart { .. } => {
                if root.is_some() {
                    return Err(format!(
                        "expected a single document in the stream, but found another document at {}",
                        at(event.start_mark)
                    ));
                }
                composer.anchors.clear();
                let first = composer.next()?;
                root = composer.compose(first)?;
            }
            _ => return Err(format!("unexpected YAML event at {}", at(event.start_mark))),
        }
    }
    let Some(root) = root else {
        return Ok(Value::None);
    };
    let count = composer.nodes.len();
    let mut constructor = Constructor {
        nodes: composer.nodes,
        active: vec![false; count],
    };
    constructor.construct(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::py_repr;

    fn yaml(text: &str) -> String {
        match load(text) {
            Ok(v) => py_repr(&v),
            Err(e) => format!("ERR {e}"),
        }
    }

    #[test]
    fn resolves_scalars_like_pyyaml() {
        assert_eq!(
            yaml("[1, 010, 0x1f, 0b11, 1_000, '1', 1.5, 1e5, 1.0e+5, .5, -.inf, 1:30, yes, Off, ~, null, '', x, 1/3, 2001-12-14]"),
            "[1, 8, 31, 3, 1000, '1', 1.5, '1e5', 100000.0, 0.5, -inf, 90, True, False, None, None, '', 'x', '1/3', <date>]"
        );
    }

    #[test]
    fn merge_keys_and_anchors() {
        assert_eq!(
            yaml("base: &b {a: 1, b: 2}\nx:\n  <<: *b\n  b: 3\n"),
            "{'base': {'a': 1, 'b': 2}, 'x': {'a': 1, 'b': 3}}"
        );
        assert_eq!(yaml("{1: a, 1.0: b, true: c}"), "{1: 'c'}");
        assert_eq!(yaml(""), "None");
        assert!(yaml("a: 1\n---\nb: 2\n").starts_with("ERR expected a single document"));
        assert!(yaml("? [1]\n: 2\n").starts_with("ERR while constructing a mapping: found unhashable key"));
        assert!(yaml("a: !!python/object x\n").starts_with("ERR could not determine a constructor"));
        assert!(yaml("a: &x 1\nb: &x 2\n").starts_with("ERR found duplicate anchor"));
    }
}
