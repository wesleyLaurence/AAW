//! Effect racks: a chain of effects as a YAML file outside any song, so that
//! a chain built on one track is added to a track in any song. A rack is the
//! `effects` list a track, a group, a return, the master or a Synth patch
//! holds, with a name, a description, tags and who saved it and when around
//! it. Saved racks are files in the workspace's library,
//! `~/Music/AAW/library/racks/` (or under `AAW_WORKSPACE`), one a rack,
//! named by the slug of the rack's name: `drum-glue.yaml` for Drum Glue.
//! There are no factory racks.

use crate::patches::{slug, timestamp, workspace};
use aaw_model::value::{Dict, Key, Value};
use aaw_model::{yaml_emit, yaml_load, Effect};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

/// A rack as its file holds it.
#[derive(Clone, Debug)]
pub struct Rack {
    pub name: String,
    /// The file's stem: the slug of the name.
    pub slug: String,
    pub description: String,
    pub tags: Vec<String>,
    /// `user` or `agent`.
    pub saved_by: Option<String>,
    /// When it was saved, as `2026-10-07T21:04:00Z`.
    pub saved_at: Option<String>,
    /// The file.
    pub file: Option<PathBuf>,
    /// The `effects` list as the file holds it, fields at their defaults
    /// left out.
    pub effects: Value,
}

impl Rack {
    /// The kinds of the effects, in order: `compressor`, `eq`, `limiter`.
    pub fn kinds(&self) -> Vec<String> {
        match &self.effects {
            Value::List(items) => items
                .iter()
                .filter_map(|e| match e.get("type") {
                    Some(Value::Str(s)) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// The racks there are, and the files in the library that are not racks.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub racks: Vec<Rack>,
    /// A file that could not be read as a rack, and why.
    pub problems: Vec<String>,
}

/// Where saved racks are: `library/racks` in the workspace.
pub fn dir() -> PathBuf {
    workspace().join("library").join("racks")
}

fn key(name: &str) -> Key {
    Key::str(name)
}

fn text(d: &Dict, name: &str) -> Result<Option<String>> {
    match d.get(&key(name)) {
        None | Some(Value::None) => Ok(None),
        Some(Value::Str(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("{name} must be text")),
    }
}

/// Validates an `effects` list on its own, as the song validates a chain,
/// and gives it back as the song would save it: defaults left out.
pub fn validate(effects: &Value) -> Result<Vec<Effect>> {
    Effect::parse_chain(effects).map_err(|e| {
        let lines: Vec<String> = e
            .errors
            .iter()
            .map(|item| {
                let loc = item.loc_text();
                if loc.is_empty() || loc == "effects" {
                    item.msg.clone()
                } else {
                    format!("{loc}: {}", item.msg)
                }
            })
            .collect();
        lines.join("; ")
    })
}

/// The list as the song would save it.
fn saved_list(chain: &[Effect]) -> Value {
    Value::List(chain.iter().map(|e| e.dump(true)).collect())
}

/// Reads a rack from its file's text. `slug_of_file` is the file's stem,
/// which names a rack whose text gives no name.
pub fn parse(text_of_file: &str, slug_of_file: &str, file: Option<PathBuf>) -> Result<Rack> {
    let value = yaml_load::load(text_of_file)?;
    let Value::Dict(d) = &value else {
        return Err("a rack is a mapping with an effects list in it".into());
    };
    for k in d.keys() {
        match k.as_str() {
            Some("name" | "description" | "tags" | "saved_by" | "saved_at" | "effects") => {}
            Some(other) => return Err(format!("{other} is not a field of a rack")),
            None => return Err("a rack's fields are named".into()),
        }
    }
    let effects = match d.get(&key("effects")) {
        Some(e @ Value::List(_)) => e.clone(),
        _ => return Err("a rack has an effects list".into()),
    };
    let chain = validate(&effects)?;
    if chain.is_empty() {
        return Err("a rack has at least one effect".into());
    }
    let name = text(d, "name")?.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| slug_of_file.to_string());
    let tags = match d.get(&key("tags")) {
        None | Some(Value::None) => Vec::new(),
        Some(Value::List(items)) => items
            .iter()
            .map(|t| match t {
                Value::Str(s) => Ok(s.trim().to_string()),
                _ => Err("tags are words".to_string()),
            })
            .collect::<Result<Vec<_>>>()?,
        Some(_) => return Err("tags is a list of words".into()),
    };
    Ok(Rack {
        name,
        slug: slug_of_file.to_string(),
        description: text(d, "description")?.unwrap_or_default(),
        tags: tags.into_iter().filter(|t| !t.is_empty()).collect(),
        saved_by: text(d, "saved_by")?,
        saved_at: text(d, "saved_at")?,
        file,
        effects: saved_list(&chain),
    })
}

/// The saved racks, by name, and the files that are not racks.
pub fn list() -> Listing {
    let mut out = Listing::default();
    let dir = dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yaml" || x == "yml") && p.is_file())
        .collect();
    files.sort();
    for file in files {
        let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        match std::fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|t| parse(&t, &stem, Some(file.clone()))) {
            Ok(r) => out.racks.push(r),
            Err(e) => out.problems.push(format!("{}: {e}", file.display())),
        }
    }
    out.racks.sort_by_key(|r| r.name.to_lowercase());
    out
}

/// Whether every word of `query` is in the rack's name, one of its tags or
/// the kind of one of its effects.
pub fn matches(rack: &Rack, query: &str) -> bool {
    let name = rack.name.to_lowercase();
    let kinds = rack.kinds();
    query.split_whitespace().all(|word| {
        let word = word.to_lowercase();
        name.contains(&word) || rack.tags.iter().any(|t| t.to_lowercase().contains(&word)) || kinds.iter().any(|k| k.contains(&word))
    })
}

/// The rack a name means: a saved rack of that name or slug, else a `.yaml`
/// file the name is the path of.
pub fn find(name: &str) -> Result<Rack> {
    let want = slug(name);
    if !want.is_empty() {
        let file = dir().join(format!("{want}.yaml"));
        if file.is_file() {
            let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            return parse(&text, &want, Some(file.clone())).map_err(|e| format!("{}: {e}", file.display()));
        }
    }
    let path = Path::new(name);
    if path.extension().is_some_and(|x| x == "yaml" || x == "yml") && path.is_file() {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let text = std::fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))?;
        return parse(&text, &slug(&stem), Some(path.to_path_buf())).map_err(|e| format!("{name}: {e}"));
    }
    Err(format!("No rack named {name}; daw rack list shows them"))
}

/// The file's text for a rack.
pub fn text_of(rack: &Rack) -> String {
    let mut d = Dict::new();
    d.insert(key("name"), Value::str(&rack.name));
    if !rack.description.is_empty() {
        d.insert(key("description"), Value::str(&rack.description));
    }
    if !rack.tags.is_empty() {
        d.insert(key("tags"), Value::List(rack.tags.iter().map(|t| Value::str(t)).collect()));
    }
    if let Some(by) = &rack.saved_by {
        d.insert(key("saved_by"), Value::str(by));
    }
    if let Some(at) = &rack.saved_at {
        d.insert(key("saved_at"), Value::str(at));
    }
    d.insert(key("effects"), rack.effects.clone());
    yaml_emit::dump(&Value::Dict(d))
}

/// Writes a chain to the library as a rack named `name`, making the
/// library's folder when it is the first. A rack already saved under that
/// name is written over only with `replace`.
pub fn save(name: &str, description: &str, tags: &[String], chain: &[Effect], by: &str, replace: bool) -> Result<Rack> {
    let name = name.trim();
    let slug_of = slug(name);
    if slug_of.is_empty() {
        return Err(format!("A rack's name has a letter or a digit in it; {name:?} has none"));
    }
    if chain.is_empty() {
        return Err("The chain has no effects to save".into());
    }
    let dir = dir();
    let file = dir.join(format!("{slug_of}.yaml"));
    if file.exists() && !replace {
        return Err(format!("A rack named {name} is already saved at {}; --replace writes over it", file.display()));
    }
    let rack = Rack {
        name: name.to_string(),
        slug: slug_of,
        description: description.trim().to_string(),
        tags: tags.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
        saved_by: Some(by.to_string()),
        saved_at: Some(timestamp()),
        file: Some(file.clone()),
        effects: saved_list(chain),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    aaw_model::atomic_write(&file, &text_of(&rack)).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(rack)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rack_is_validated_as_the_song_validates_a_chain() {
        let e = parse("name: X\neffects: [{type: compresor}]\n", "x", None).unwrap_err();
        assert!(e.contains("compresor"), "{e}");
        let e = parse("name: X\neffects: [{type: limiter, ceiling_db: 3}]\n", "x", None).unwrap_err();
        assert!(e.contains("0.limiter.ceiling_db"), "{e}");
        let e = parse("name: X\neffects: [{type: limiter, id: a}, {type: filter, id: a, mode: lowpass, cutoff_hz: 400}]\n", "x", None).unwrap_err();
        assert!(e.contains("unique"), "{e}");
        let e = parse("name: X\neffects: []\n", "x", None).unwrap_err();
        assert!(e.contains("at least one"), "{e}");
        let e = parse("name: X\n", "x", None).unwrap_err();
        assert!(e.contains("effects list"), "{e}");
        let e = parse("name: X\neffects: [{type: limiter}]\ncolour: red\n", "x", None).unwrap_err();
        assert!(e.contains("colour"), "{e}");
        // A file without a name is named by its file, and defaults are left out.
        let r = parse("effects: [{type: limiter, ceiling_db: -1, release_ms: 100}, {type: utility, gain_db: -3}]\n", "plain", None).unwrap();
        assert_eq!(r.name, "plain");
        assert_eq!(r.kinds(), ["limiter", "utility"]);
        let text = text_of(&r);
        assert!(text.starts_with("name: plain\neffects:\n") && !text.contains("ceiling_db") && text.contains("gain_db: -3"), "{text}");
        let again = parse(&text, "plain", None).unwrap();
        assert!(aaw_model::value::py_eq(&again.effects, &r.effects));
        assert!(matches(&r, "") && matches(&r, "LIMIT plain") && !matches(&r, "reverb"));
        assert!(find("no-such-rack").unwrap_err().contains("No rack named"));
    }
}
