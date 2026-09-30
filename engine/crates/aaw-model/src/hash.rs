//! Project fingerprints: `project_sha256` of the saved form, and the full-dump
//! forms earlier engines wrote, so their render reports still verify.

use crate::pyfmt::json_dumps;
use crate::schema::Project;
use crate::value::{dict, py_eq, Dict, Value};
use sha2::{Digest, Sha256};

fn sha(v: &Value) -> String {
    let digest = Sha256::digest(json_dumps(v).as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of `json.dumps(saved form, sort_keys=True)`.
pub fn project_hash(project: &Project) -> String {
    sha(&project.dump(true))
}

/// `model.LEGACY_FIELDS`, newest first: the fields each schema change added, at
/// their defaults. A field added to any model needs a new first entry here.
fn legacy_fields() -> Vec<Value> {
    vec![
        dict(vec![("automation", Value::List(vec![])), ("id", Value::None)]),
        dict(vec![("sends", Value::List(vec![])), ("returns", Value::List(vec![]))]),
        dict(vec![("effects", Value::List(vec![])), ("master", Value::Dict(Dict::new()))]),
    ]
}

/// `model._without`: drops, anywhere in the dump, keys equal to those defaults.
fn without(data: &Value, fields: &Dict) -> Value {
    match data {
        Value::List(items) => Value::List(items.iter().map(|x| without(x, fields)).collect()),
        Value::Dict(d) => Value::Dict(
            d.iter()
                .map(|(k, v)| (k.clone(), without(v, fields)))
                .filter(|(k, v)| !fields.get(k).is_some_and(|default| py_eq(v, default)))
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
    let mut fields = Dict::new();
    forms.push(sha(&without(&full, &fields)));
    for added in legacy_fields() {
        if let Value::Dict(d) = added {
            fields.extend(d);
        }
        forms.push(sha(&without(&full, &fields)));
    }
    forms
}

/// Whether `sha` fingerprints the project, also in the forms earlier engines wrote.
pub fn hash_matches(project: &Project, sha: &str) -> bool {
    fingerprints(project).iter().any(|f| f == sha)
}
