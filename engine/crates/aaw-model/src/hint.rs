//! What a refused edit says to do next. A key the song has no field for is
//! answered with the field nearest to it, from the schema `daw describe`
//! prints: "`bpm` is not a field; did you mean `session.tempo`?". The
//! validation message itself is pydantic's and stays as it is.

use crate::contract::schema;
use crate::validate::{Loc, ValidationError};
use crate::value::{Key, Value};
use serde_json::Value as Json;

/// Words for a field that other tools use, and the field the song has.
const SYNONYMS: &[(&str, &str)] = &[
    ("bpm", "tempo"),
    ("volume", "gain_db"),
    ("vol", "gain_db"),
    ("level", "gain_db"),
    ("start", "at"),
    ("position", "at"),
    ("offset", "at"),
    ("length", "length_beats"),
    ("note", "pitch"),
    ("key", "root_note"),
    ("root", "root_note"),
    ("file", "path"),
    ("kind", "type"),
    ("effect", "type"),
    ("name", "id"),
    ("bypassed", "bypass"),
    ("muted", "mute"),
    ("loop", "repeats"),
];

/// The hints for a refused song, one line for each key it has no field
/// for, or None when nothing was such a key. `doc` is the song as it was
/// given, whose track IDs name the places in the hints.
pub fn hint(doc: &Value, error: &ValidationError) -> Option<String> {
    let lines: Vec<String> = error
        .errors
        .iter()
        .filter(|e| e.kind == "extra_forbidden")
        .filter_map(|e| unknown_field(doc, &e.loc))
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The error of a refused song as `daw` shows it: the hints, then the
/// validation message.
pub fn explain(doc: &Value, error: &ValidationError) -> String {
    match hint(doc, error) {
        Some(h) => format!("{h}\n{error}"),
        None => error.to_string(),
    }
}

fn unknown_field(doc: &Value, loc: &[Loc]) -> Option<String> {
    let (Loc::Key(name), parent) = loc.split_last()? else {
        return None;
    };
    let (models, parent) = models_at(doc, parent);
    let parent = parent.as_slice();
    let mut fields: Vec<&str> = Vec::new();
    for field in models.iter().filter_map(|m| m.get("properties").and_then(Json::as_object)).flat_map(|p| p.keys()) {
        if !fields.contains(&field.as_str()) {
            fields.push(field);
        }
    }
    if fields.is_empty() {
        return None;
    }
    let place = shown(doc, parent);
    let full = |field: &str| if place.is_empty() { field.to_string() } else { format!("{place}.{field}") };
    let given = full(name);
    let mut meant = nearest(name, &fields).map(|f| full(&f));
    // At the top of the song, a session field is the likelier meaning.
    if meant.is_none() && parent.is_empty() {
        let session = resolve(&schema()["properties"]["session"]);
        let session: Vec<&str> = session
            .first()
            .and_then(|m| m.get("properties"))
            .and_then(Json::as_object)
            .map(|p| p.keys().map(String::as_str).collect())
            .unwrap_or_default();
        meant = nearest(name, &session).map(|f| format!("session.{f}"));
    }
    Some(match meant {
        Some(field) => format!("`{given}` is not a field; did you mean `{field}`?"),
        None => format!(
            "`{given}` is not a field; {} has {}",
            if place.is_empty() { "the song" } else { place.as_str() },
            fields.join(", ")
        ),
    })
}

/// The field a mistaken name most likely meant: a synonym, the name with its
/// unit left off (`cutoff` for `cutoff_hz`), or a spelling a letter or two away.
fn nearest(name: &str, fields: &[&str]) -> Option<String> {
    let name = name.to_ascii_lowercase().replace('-', "_");
    if let Some(field) = fields.iter().find(|f| **f == name) {
        return Some(field.to_string());
    }
    if let Some((_, field)) = SYNONYMS.iter().find(|(word, field)| *word == name && fields.contains(field)) {
        return Some(field.to_string());
    }
    let prefixed = fields
        .iter()
        .filter(|f| name.len() >= 3 && f.starts_with(name.as_str()))
        .min_by_key(|f| f.len());
    if let Some(field) = prefixed {
        return Some(field.to_string());
    }
    let within = (name.len() / 3).clamp(1, 3);
    fields
        .iter()
        .map(|f| (distance(&name, f), *f))
        .filter(|(d, _)| *d <= within)
        .min_by_key(|(d, _)| *d)
        .map(|(_, f)| f.to_string())
}

/// Levenshtein distance.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut last = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = (row[j + 1] + 1).min(row[j] + 1).min(last + usize::from(ca != *cb));
            last = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

/// A schema node with its references followed and its alternatives listed,
/// leaving out null.
fn resolve(node: &Json) -> Vec<&Json> {
    if let Some(name) = node.get("$ref").and_then(Json::as_str).and_then(|r| r.strip_prefix("#/$defs/")) {
        return resolve(&schema()["$defs"][name]);
    }
    if let Some(alternatives) = node.get("anyOf").or_else(|| node.get("oneOf")).and_then(Json::as_array) {
        return alternatives.iter().flat_map(resolve).collect();
    }
    if node.get("type").and_then(Json::as_str) == Some("null") {
        return Vec::new();
    }
    vec![node]
}

/// The models a place in the song can be, those whose `type` agrees with
/// the document's where the alternatives are told apart by it, and the
/// place without the tags of tagged unions, which the validation error's
/// location has (`effects.1.limiter.ceiling`) and the song does not.
fn models_at<'a>(doc: &Value, loc: &[Loc]) -> (Vec<&'a Json>, Vec<Loc>) {
    let mut nodes: Vec<&Json> = vec![schema()];
    let mut here = Some(doc);
    let mut place = Vec::new();
    for step in loc {
        let tagged = match step {
            Loc::Key(k) => nodes.iter().find_map(|n| {
                let model = n["discriminator"]["mapping"].get(k)?.as_str()?.strip_prefix("#/$defs/")?;
                Some(&schema()["$defs"][model])
            }),
            Loc::Index(_) => None,
        };
        if let Some(tagged) = tagged {
            nodes = vec![tagged];
            continue;
        }
        place.push(step.clone());
        let mut next = Vec::new();
        for node in nodes.iter().flat_map(|n| resolve(n)) {
            match step {
                Loc::Key(k) => {
                    if let Some(child) = node.get("properties").and_then(|p| p.get(k)) {
                        next.push(child);
                    } else if let Some(child) = node.get("additionalProperties").filter(|a| a.is_object()) {
                        next.push(child);
                    }
                }
                Loc::Index(_) => {
                    // A tagged union is resolved by the tag that follows.
                    match node.get("items") {
                        Some(items) if items.get("discriminator").is_some() => next.push(items),
                        Some(items) => next.extend(resolve(items)),
                        None => {}
                    }
                }
            }
        }
        here = here.and_then(|v| child(v, step));
        nodes = next;
    }
    let models: Vec<&Json> = nodes.into_iter().flat_map(resolve).collect();
    let kind = here.and_then(|v| child(v, &Loc::Key("type".into()))).and_then(|t| match t {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    });
    let tagged = |m: &&Json| m["properties"]["type"].get("const").and_then(Json::as_str).map(str::to_string);
    let matching: Vec<&Json> = models.iter().copied().filter(|m| tagged(m).as_deref() == kind).collect();
    (if matching.is_empty() { models } else { matching }, place)
}

fn child<'v>(v: &'v Value, step: &Loc) -> Option<&'v Value> {
    match (v, step) {
        (Value::Dict(d), Loc::Key(k)) => d.get(&Key::str(k)),
        (Value::List(items), Loc::Index(i)) => items.get(*i),
        _ => None,
    }
}

/// A place in the song as commands address it: a list item by its `id`
/// where it has one, as `tracks.drums`, else by its index.
fn shown(doc: &Value, loc: &[Loc]) -> String {
    let mut here = Some(doc);
    let mut parts = Vec::new();
    for step in loc {
        here = here.and_then(|v| child(v, step));
        parts.push(match step {
            Loc::Key(k) => k.clone(),
            Loc::Index(i) => match here.and_then(|v| child(v, &Loc::Key("id".into()))) {
                Some(Value::Str(id)) => id.clone(),
                _ => i.to_string(),
            },
        });
    }
    parts.join(".")
}
