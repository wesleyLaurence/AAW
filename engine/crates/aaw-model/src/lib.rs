//! Schema v1 song model: types, validation, exact beats, canonical YAML and fingerprints.
//!
//! This began as a port of the first engine's Python model (pydantic and PyYAML)
//! and keeps its behavior exactly: the same documents are accepted, `save`
//! writes the same bytes and `project_sha256` is the same, so existing projects,
//! render reports and `--expect` SHAs carry over. tests/test_model.py pins that
//! behavior on a generated corpus.

pub mod beat;
pub mod contract;
pub mod describe;
pub mod hash;
pub mod pyfmt;
pub mod rules;
pub mod schedule;
pub mod schema;
pub mod validate;
pub mod value;
pub mod yaml_emit;
pub mod yaml_load;

pub use beat::{beat, frame, Beat};
pub use hash::{fingerprints, hash_matches, project_hash};
pub use schema::*;
pub use validate::ValidationError;

use crate::value::{Dict, Key, Value};
use num_bigint::BigInt;
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug)]
pub enum ModelError {
    /// The text is not YAML, or not the single document `safe_load` accepts.
    Yaml(String),
    /// A patch is not JSON.
    Json(String),
    Validation(ValidationError),
    /// A referenced sample is missing or changed.
    Asset(String),
    Io(std::io::Error),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::Yaml(e) | ModelError::Json(e) | ModelError::Asset(e) => f.write_str(e),
            ModelError::Validation(e) => e.fmt(f),
            ModelError::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ModelError {}

impl From<ValidationError> for ModelError {
    fn from(e: ValidationError) -> Self {
        ModelError::Validation(e)
    }
}

impl From<std::io::Error> for ModelError {
    fn from(e: std::io::Error) -> Self {
        ModelError::Io(e)
    }
}

/// Validates YAML text as `Project.model_validate(yaml.safe_load(text))`.
pub fn parse(text: &str) -> Result<Project, ModelError> {
    let value = yaml_load::load(text).map_err(ModelError::Yaml)?;
    Ok(Project::validate(&value)?)
}

/// The canonical document `save` writes.
pub fn to_yaml(project: &Project) -> String {
    let Value::Dict(saved) = project.dump(true) else {
        unreachable!("a project dumps to a mapping")
    };
    let mut doc = Dict::new();
    doc.insert(Key::str("schema_version"), Value::int(project.schema_version()));
    doc.extend(saved);
    yaml_emit::dump(&Value::Dict(doc))
}

/// SHA-256 of a file's bytes, as hex.
pub fn digest(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Checks each sample exists and, when it has a hash, is unchanged.
pub fn verify_assets(project: &Project, root: &Path) -> Result<(), ModelError> {
    for (name, asset) in &project.samples {
        let path = root.join(&asset.path);
        let resolved = std::fs::canonicalize(&path).unwrap_or(path);
        if !resolved.is_file() {
            return Err(ModelError::Asset(format!(
                "Missing sample {name}: {}",
                resolved.display()
            )));
        }
        if let Some(expected) = &asset.sha256 {
            if &digest(&resolved)? != expected {
                return Err(ModelError::Asset(format!("Sample content changed: {name}")));
            }
        }
    }
    Ok(())
}

/// Reads and validates a song, and optionally verifies its samples.
pub fn load(path: &Path, check_assets: bool) -> Result<Project, ModelError> {
    let text = std::fs::read_to_string(path)?;
    let project = parse(&text)?;
    if check_assets {
        verify_assets(&project, path.parent().unwrap_or(Path::new(".")))?;
    }
    Ok(project)
}

/// Writes through a temporary file in the same directory, then renames it.
pub fn atomic_write(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut temp = tempfile::Builder::new()
        .prefix(&format!(".{name}"))
        .tempfile_in(dir)?;
    temp.write_all(text.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Writes a song's canonical YAML.
pub fn save(project: &Project, path: &Path) -> std::io::Result<()> {
    atomic_write(path, &to_yaml(project))
}

/// Python `json.loads` into the value tree: integers stay exact, other numbers
/// become floats, and duplicate keys keep the last value.
pub fn json_value(text: &str) -> Result<Value, ModelError> {
    let parsed: serde_json::Value =
        serde_json::from_str(text).map_err(|e| ModelError::Json(e.to_string()))?;
    fn convert(v: serde_json::Value) -> Value {
        match v {
            serde_json::Value::Null => Value::None,
            serde_json::Value::Bool(b) => Value::Bool(b),
            serde_json::Value::Number(n) => {
                let s = n.to_string();
                if s.contains(['.', 'e', 'E']) {
                    Value::Float(s.parse().unwrap_or(f64::NAN))
                } else {
                    Value::Int(s.parse::<BigInt>().expect("integer"))
                }
            }
            serde_json::Value::String(s) => Value::Str(s),
            serde_json::Value::Array(items) => Value::List(items.into_iter().map(convert).collect()),
            serde_json::Value::Object(map) => Value::Dict(
                map.into_iter()
                    .map(|(k, v)| (Key::str(&k), convert(v)))
                    .collect(),
            ),
        }
    }
    Ok(convert(parsed))
}

/// RFC 7386 JSON merge patch: objects merge, null deletes, and
/// anything else replaces.
pub fn merge(target: &Value, patch: &Value) -> Value {
    let Value::Dict(changes) = patch else {
        return patch.clone();
    };
    let mut result = match target {
        Value::Dict(d) => d.clone(),
        _ => Dict::new(),
    };
    for (k, v) in changes {
        if v.is_none() {
            result.shift_remove(k);
        } else {
            let merged = merge(result.get(k).unwrap_or(&Value::None), v);
            result.insert(k.clone(), merged);
        }
    }
    Value::Dict(result)
}

/// `daw apply`: the patch merged into the full dump, then validated.
pub fn apply_patch(project: &Project, patch: &Value) -> Result<Project, ModelError> {
    Ok(Project::validate(&merge(&project.dump(false), patch))?)
}
