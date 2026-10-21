//! Instrument racks: a MIDI track's instrument with its chain of effects as
//! a YAML file outside any song, so that a bass with its compressor and
//! saturation, or a key with its chorus and reverb, is loaded onto a track
//! in any song as one thing. An instrument rack holds the `instrument` a
//! MIDI track holds, a Synth or a Sampler, the files a Sampler's pads play
//! by where they are, and an `effects` list, with a name, a description,
//! tags and who saved it and when around them. Saved instrument racks are
//! files in the workspace's library, `~/Music/AAW/library/instruments/`
//! (or under `AAW_WORKSPACE`), one a rack, named by the slug of its name.
//! Loaded, the instrument goes in place of the track's and the effects
//! become one rack device first in the track's chain.

use crate::patches::{slug, timestamp, workspace};
use aaw_model::value::{Dict, Key, Value};
use aaw_model::{yaml_emit, yaml_load, Effect, Instrument};
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

/// An instrument rack as its file holds it.
#[derive(Clone, Debug)]
pub struct InstrumentRack {
    pub name: String,
    /// The file's stem: the slug of the name.
    pub slug: String,
    pub description: String,
    pub tags: Vec<String>,
    /// `user` or `agent`.
    pub saved_by: Option<String>,
    /// When it was saved, as `2026-10-21T21:04:00Z`.
    pub saved_at: Option<String>,
    /// The file.
    pub file: Option<PathBuf>,
    /// The `instrument` mapping as the file holds it, `{synth: ...}` or
    /// `{sampler: ...}`, fields at their defaults left out.
    pub instrument: Value,
    /// The files a sampler's pads play, by the sample's id in the
    /// instrument: each `{file, sha256, source, root_note}`, `file` being
    /// where the file was when the rack was saved.
    pub samples: Dict,
    /// The `effects` list as the file holds it: a chain with no rack in it.
    pub effects: Value,
}

impl InstrumentRack {
    /// `synth` or `sampler`.
    pub fn kind(&self) -> &'static str {
        match self.instrument.get("synth") {
            Some(v) if !v.is_none() => "synth",
            _ => "sampler",
        }
    }

    /// The kinds of the effects, in order.
    pub fn effect_kinds(&self) -> Vec<String> {
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

/// The instrument racks there are, and the files in the library that are
/// not instrument racks.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub racks: Vec<InstrumentRack>,
    /// A file that could not be read as an instrument rack, and why.
    pub problems: Vec<String>,
}

/// Where saved instrument racks are: `library/instruments` in the workspace.
pub fn dir() -> PathBuf {
    workspace().join("library").join("instruments")
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

/// Validates an `instrument` mapping on its own, as the song validates it.
pub fn validate(instrument: &Value) -> Result<Instrument> {
    Instrument::parse(instrument).map_err(|e| {
        let lines: Vec<String> = e.errors.iter().map(|item| {
            let loc = item.loc_text();
            if loc.is_empty() { item.msg.clone() } else { format!("{loc}: {}", item.msg) }
        }).collect();
        format!("instrument: {}", lines.join("; "))
    })
}

/// The ids of the samples a sampler's pads play.
fn pad_samples(instrument: &Instrument) -> Vec<String> {
    match instrument {
        Instrument::Sampler(s) => s.pads.values().map(|p| p.sample.clone()).collect(),
        Instrument::Synth(_) => Vec::new(),
    }
}

/// Reads an instrument rack from its file's text. `slug_of_file` is the
/// file's stem, which names a rack whose text gives no name.
pub fn parse(text_of_file: &str, slug_of_file: &str, file: Option<PathBuf>) -> Result<InstrumentRack> {
    let value = yaml_load::load(text_of_file)?;
    let Value::Dict(d) = &value else {
        return Err("an instrument rack is a mapping with an instrument in it".into());
    };
    for k in d.keys() {
        match k.as_str() {
            Some("name" | "description" | "tags" | "saved_by" | "saved_at" | "instrument" | "samples" | "effects") => {}
            Some(other) => return Err(format!("{other} is not a field of an instrument rack")),
            None => return Err("an instrument rack's fields are named".into()),
        }
    }
    let instrument = match d.get(&key("instrument")) {
        Some(i @ Value::Dict(_)) => i.clone(),
        _ => return Err("an instrument rack has an instrument".into()),
    };
    let parsed = validate(&instrument)?;
    let effects = match d.get(&key("effects")) {
        None | Some(Value::None) => Value::List(Vec::new()),
        Some(e @ Value::List(_)) => e.clone(),
        Some(_) => return Err("effects is a list".into()),
    };
    let chain = crate::racks::validate(&effects)?;
    if chain.iter().any(|e| matches!(e, Effect::Rack(_))) {
        return Err("an instrument rack's effects hold no rack: they are loaded as one".into());
    }
    let samples = match d.get(&key("samples")) {
        None | Some(Value::None) => Dict::new(),
        Some(Value::Dict(s)) => s.clone(),
        Some(_) => return Err("samples is a mapping of the files the pads play".into()),
    };
    for id in pad_samples(&parsed) {
        let Some(Value::Dict(entry)) = samples.get(&key(&id)) else {
            return Err(format!("the pads play sample {id}, which samples does not name"));
        };
        if text(entry, "file")?.is_none() {
            return Err(format!("samples.{id} names no file"));
        }
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
    Ok(InstrumentRack {
        name,
        slug: slug_of_file.to_string(),
        description: text(d, "description")?.unwrap_or_default(),
        tags: tags.into_iter().filter(|t| !t.is_empty()).collect(),
        saved_by: text(d, "saved_by")?,
        saved_at: text(d, "saved_at")?,
        file,
        instrument: parsed.dump(true),
        samples,
        effects: Value::List(chain.iter().map(|e| e.dump(true)).collect()),
    })
}

/// The saved instrument racks, by name, and the files that are not racks.
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
        match std::fs::read_to_string(&file) {
            Ok(text) => match parse(&text, &stem, Some(file.clone())) {
                Ok(r) => out.racks.push(r),
                Err(e) => out.problems.push(format!("{}: {e}", file.display())),
            },
            Err(e) => out.problems.push(format!("{}: {e}", file.display())),
        }
    }
    out.racks.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Whether every word of `query` is in the rack's name, a tag, its
/// instrument's kind or an effect's kind, ignoring case.
pub fn matches(rack: &InstrumentRack, query: &str) -> bool {
    let kinds = rack.effect_kinds();
    query.split_whitespace().all(|word| {
        let w = word.to_lowercase();
        rack.name.to_lowercase().contains(&w)
            || rack.tags.iter().any(|t| t.to_lowercase().contains(&w))
            || rack.kind().contains(&w)
            || kinds.iter().any(|k| k.contains(&w))
    })
}

/// An instrument rack by its name or its slug, or a `.yaml` file by its path.
pub fn find(name: &str) -> Result<InstrumentRack> {
    let wanted = slug(name);
    for r in list().racks {
        if r.name == name || r.slug == wanted {
            return Ok(r);
        }
    }
    let path = Path::new(name);
    if path.extension().is_some_and(|x| x == "yaml" || x == "yml") && path.is_file() {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let text = std::fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))?;
        return parse(&text, &slug(&stem), Some(path.to_path_buf())).map_err(|e| format!("{name}: {e}"));
    }
    Err(format!("No instrument rack named {name}; daw instrument rack list shows them"))
}

/// The file's text for an instrument rack.
pub fn text_of(rack: &InstrumentRack) -> String {
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
    d.insert(key("instrument"), rack.instrument.clone());
    if !rack.samples.is_empty() {
        d.insert(key("samples"), Value::Dict(rack.samples.clone()));
    }
    if let Value::List(items) = &rack.effects {
        if !items.is_empty() {
            d.insert(key("effects"), rack.effects.clone());
        }
    }
    yaml_emit::dump(&Value::Dict(d))
}

/// Writes a track's instrument and chain to the library as an instrument
/// rack named `name`, making the library's folder when it is the first.
/// `samples` are the song's, for the files a sampler's pads play, under
/// the project's folder `dir`; a rack in the chain is saved as its
/// effects. One already saved under that name is written over only with
/// `replace`.
pub fn save(
    name: &str,
    description: &str,
    tags: &[String],
    instrument: &Value,
    effects: &Value,
    samples: &Value,
    dir: &Path,
    by: &str,
    replace: bool,
) -> Result<InstrumentRack> {
    let name = name.trim();
    let slug_of = slug(name);
    if slug_of.is_empty() {
        return Err(format!("An instrument rack's name has a letter or a digit in it; {name:?} has none"));
    }
    let parsed = validate(instrument)?;
    let chain = Effect::laid_out(&crate::racks::validate(effects)?);
    let mut files = Dict::new();
    for id in pad_samples(&parsed) {
        let Some(Value::Dict(entry)) = samples.get(&id) else {
            return Err(format!("the pads play sample {id}, which the song does not have"));
        };
        let path = text(entry, "path")?.ok_or_else(|| format!("samples.{id} has no path"))?;
        let file = dir.join(&path);
        if !file.is_file() {
            return Err(format!("samples.{id}: {} is not there", file.display()));
        }
        let mut spec = Dict::new();
        spec.insert(key("file"), Value::str(&file.to_string_lossy()));
        for field in ["sha256", "source", "root_note"] {
            if let Some(v) = text(entry, field)? {
                spec.insert(key(field), Value::str(&v));
            }
        }
        files.insert(key(&id), Value::Dict(spec));
    }
    let library = dir_of();
    let file = library.join(format!("{slug_of}.yaml"));
    if file.exists() && !replace {
        return Err(format!("An instrument rack named {name} is already saved at {}; --replace writes over it", file.display()));
    }
    let rack = InstrumentRack {
        name: name.to_string(),
        slug: slug_of,
        description: description.trim().to_string(),
        tags: tags.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
        saved_by: Some(by.to_string()),
        saved_at: Some(timestamp()),
        file: Some(file.clone()),
        instrument: parsed.dump(true),
        samples: files,
        effects: Value::List(chain.iter().map(|e| e.dump(true)).collect()),
    };
    std::fs::create_dir_all(&library).map_err(|e| format!("{}: {e}", library.display()))?;
    aaw_model::atomic_write(&file, &text_of(&rack)).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(rack)
}

fn dir_of() -> PathBuf {
    dir()
}

/// A sample the song has, for `bring_samples` to match a rack's against.
#[derive(Clone, Debug)]
pub struct Have {
    pub id: String,
    pub sha256: Option<String>,
    pub path: String,
}

/// A sample of a rack as it is in the song after a load: what the rack
/// called it, what the song calls it, and the song's `samples` entry to
/// set for a file brought in, or None when the song had the file.
#[derive(Clone, Debug)]
pub struct Brought {
    pub was: String,
    pub id: String,
    pub entry: Option<Json>,
    pub from: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Brings the files a rack's pads play into the project at `dir`: a file
/// the song has already, by its hash, is used as it is; another is copied
/// under `samples/` and named for the rack's sample, numbered when the
/// song has the name.
pub fn bring_samples(rack: &InstrumentRack, dir: &Path, have: &[Have]) -> Result<Vec<Brought>> {
    let mut taken: Vec<String> = have.iter().map(|h| h.id.clone()).collect();
    let mut out = Vec::new();
    for (k, spec) in &rack.samples {
        let was = k.as_str().unwrap_or_default().to_string();
        let Value::Dict(spec) = spec else { continue };
        let file = PathBuf::from(text(spec, "file")?.unwrap_or_default());
        let bytes = std::fs::read(&file).map_err(|e| format!("{}: the file {} plays is not there: {e}", rack.name, was))?;
        let sha256 = hex(&Sha256::digest(&bytes));
        // The song has the file already: by the hash it recorded, or by
        // the file's own where it recorded none.
        let same = have.iter().find(|h| match &h.sha256 {
            Some(recorded) => *recorded == sha256,
            None => std::fs::read(dir.join(&h.path)).is_ok_and(|b| hex(&Sha256::digest(&b)) == sha256),
        });
        if let Some(h) = same {
            out.push(Brought { was, id: h.id.clone(), entry: None, from: h.path.clone() });
            continue;
        }
        let stem = crate::command::ident(&was, "sample");
        let ext = file.extension().map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|| "wav".into());
        let id = crate::command::unique(&stem, |n| taken.iter().any(|t| t == n) || dir.join("samples").join(format!("{n}.{ext}")).exists());
        taken.push(id.clone());
        let relative = PathBuf::from("samples").join(format!("{id}.{ext}"));
        let to = dir.join(&relative);
        std::fs::create_dir_all(dir.join("samples")).map_err(|e| format!("{}: {e}", dir.join("samples").display()))?;
        std::fs::write(&to, &bytes).map_err(|e| format!("{}: {e}", to.display()))?;
        let mut entry = json!({"path": relative.to_string_lossy(), "sha256": sha256});
        let source = text(spec, "source")?.unwrap_or_else(|| file.to_string_lossy().into_owned());
        entry["source"] = json!(source);
        if let Some(root) = text(spec, "root_note")? {
            entry["root_note"] = json!(root);
        }
        out.push(Brought { was, id, entry: Some(entry), from: file.to_string_lossy().into_owned() });
    }
    Ok(out)
}

/// The rack's instrument as it goes into the song: each pad's sample named
/// as the song names it after `bring_samples`.
pub fn instrument_for(rack: &InstrumentRack, brought: &[Brought]) -> Json {
    let mut instrument = crate::session::value_json(&rack.instrument);
    if let Some(pads) = instrument.get_mut("sampler").and_then(|s| s.get_mut("pads")).and_then(Json::as_object_mut) {
        for pad in pads.values_mut() {
            if let Some(sample) = pad.get("sample").and_then(Json::as_str).map(str::to_string) {
                if let Some(b) = brought.iter().find(|b| b.was == sample) {
                    pad["sample"] = json!(b.id);
                }
            }
        }
    }
    instrument
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instrument_rack_is_validated_as_the_song_validates_an_instrument() {
        let text = "name: Warm Bass\ninstrument: {synth: {oscillators: {a: {wave: saw}}}}\neffects: [{type: saturation, drive_db: 6}]\n";
        let r = parse(text, "warm-bass", None).unwrap();
        assert_eq!((r.kind(), r.effect_kinds()), ("synth", vec!["saturation".to_string()]));
        assert!(matches(&r, "warm synth saturation") && !matches(&r, "sampler"));
        let e = parse("name: X\ninstrument: {synth: {oscillators: {a: {wave: sawtooth}}}}\n", "x", None).unwrap_err();
        assert!(e.contains("wave"), "{e}");
        let e = parse("name: X\ninstrument: {sampler: {pads: {k: {sample: kick}}}}\n", "x", None).unwrap_err();
        assert!(e.contains("samples does not name"), "{e}");
        let e = parse("name: X\ninstrument: {synth: {oscillators: {a: {}}}}\neffects: [{type: rack}]\n", "x", None).unwrap_err();
        assert!(e.contains("hold no rack"), "{e}");
    }
}
