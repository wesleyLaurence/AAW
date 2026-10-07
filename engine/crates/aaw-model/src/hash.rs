//! Project fingerprints: `project_sha256` of the saved form, and the full-dump
//! forms earlier engines wrote, so their render reports still verify.

use crate::pyfmt::json_dumps;
use crate::schema::Project;
use crate::value::{py_eq, Dict, Value};
use sha2::{Digest, Sha256};

fn sha(v: &Value) -> String {
    let digest = Sha256::digest(json_dumps(v).as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of `json.dumps(saved form, sort_keys=True)`.
pub fn project_hash(project: &Project) -> String {
    sha(&project.dump(true))
}

/// A field a schema change added, at its default; dropped from the full
/// dump to give the form an earlier engine wrote. One whose key another
/// model shares names a sibling key, `beside`, and is dropped only from a
/// mapping that holds it.
struct Added {
    key: &'static str,
    default: Value,
    beside: Option<&'static str>,
}

fn added(key: &'static str, default: Value) -> Added {
    Added { key, default, beside: None }
}

/// Newest first: the fields each schema change added, at
/// their defaults. A field added to any model needs a new first entry here.
fn legacy_fields() -> Vec<Vec<Added>> {
    vec![
        // Groups: the list, and a track's group.
        vec![added("groups", Value::List(vec![])), added("group", Value::None)],
        // An equalizer band's slope; the filter has the key too, so only a
        // mapping with a band's shape loses it.
        vec![Added { key: "slope_db_per_octave", default: Value::int(12), beside: Some("shape") }],
        // An equalizer band's shape is a word, so only a point's is dropped.
        vec![added("shape", Value::Float(0.0))],
        vec![added("audio", Value::List(vec![]))],
        vec![added("stretch", Value::str("repitch")), added("stretcher", Value::str("signalsmith"))],
        vec![added("source_sha256", Value::None)],
        vec![added("automation", Value::List(vec![])), added("id", Value::None)],
        vec![added("sends", Value::List(vec![])), added("returns", Value::List(vec![]))],
        vec![added("effects", Value::List(vec![])), added("master", Value::Dict(Dict::new()))],
    ]
}

/// Drops, anywhere in the dump, keys equal to those defaults.
fn without(data: &Value, fields: &[Added]) -> Value {
    match data {
        Value::List(items) => Value::List(items.iter().map(|x| without(x, fields)).collect()),
        Value::Dict(d) => Value::Dict(
            d.iter()
                .map(|(k, v)| (k.clone(), without(v, fields)))
                .filter(|(k, v)| {
                    !fields.iter().any(|f| {
                        k.as_str() == Some(f.key) && py_eq(v, &f.default) && f.beside.is_none_or(|sibling| d.get(&crate::value::Key::str(sibling)).is_some())
                    })
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The fingerprint forms, newest first: the saved form, then the full dump with
/// each schema's added fields removed in turn.
pub fn fingerprints(project: &Project) -> Vec<String> {
    let mut forms = vec![project_hash(project)];
    let full = project.dump(false);
    let mut fields: Vec<Added> = Vec::new();
    forms.push(sha(&without(&full, &fields)));
    for added in legacy_fields() {
        fields.extend(added);
        forms.push(sha(&without(&full, &fields)));
    }
    forms
}

/// Whether `sha` fingerprints the project, also in the forms earlier engines wrote.
pub fn hash_matches(project: &Project, sha: &str) -> bool {
    fingerprints(project).iter().any(|f| f == sha)
}
