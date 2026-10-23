//! Devices anyone builds, to a spec: an effect or an instrument written as
//! a folder, with a manifest that names it and declares its fields and the
//! code that makes its sound, under the project's `devices/` or the
//! workspace's (`~/Music/AAW/devices/`, or under `AAW_WORKSPACE`), named by
//! its slug.
//!
//! An offline device's `device.py` defines `process(audio, sample_rate,
//! fields)` over numpy, which the host runs in the checkout's Python on the
//! device's input as the song makes it, whenever what feeds the device
//! changes, and keeps the result under the project's `.daw/devices/` by an
//! identity of everything it was made from. The engine plays the cache in
//! the device's place, in playback and in a render alike; until it is
//! there, or when the run fails, the device passes its input through.
//!
//! A real-time device's `device.wasm` is a WebAssembly module meeting the
//! device ABI (`aaw_engine::wasm`), built from its source, `device.rs`, by
//! `daw plugin build`. The host makes it ready and the engine runs it on
//! the audio thread, an instance to each place the device is in a chain.
//!
//! An instrument (`kind: instrument`) is a MIDI track's instrument, given
//! the track's notes rather than audio: offline, `device.py` defines
//! `play(notes, frames, sample_rate, fields)`, run over the whole song and
//! cached as an effect's output is; in real time, the module also exports
//! `aaw_note`, and plays previewed notes too.

use crate::patches::{slug, workspace};
use crate::python;
use aaw_dsp::device::Made;
use aaw_engine::custom::{self, Caches};
use aaw_engine::wasm;
use aaw_model::value::{Dict, Key, Value};
use aaw_model::{yaml_load, Effect, FieldValue, Project};
use serde_json::{json, Map, Value as Json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

type Result<T> = std::result::Result<T, String>;

/// The manifest format this reads.
pub const FORMAT: i64 = 1;
/// A field's types, as the manifest names them.
pub const FIELD_TYPES: [&str; 8] = ["db", "hz", "percent", "ms", "beats", "number", "switch", "choice"];
/// The block size the device's input is rendered in.
const BLOCK: usize = 4096;

/// What a field holds and how it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Db,
    Hz,
    Percent,
    Ms,
    Beats,
    Number,
    Switch,
    Choice,
}

impl FieldKind {
    fn parse(s: &str) -> Option<FieldKind> {
        Some(match s {
            "db" => FieldKind::Db,
            "hz" => FieldKind::Hz,
            "percent" => FieldKind::Percent,
            "ms" => FieldKind::Ms,
            "beats" => FieldKind::Beats,
            "number" => FieldKind::Number,
            "switch" => FieldKind::Switch,
            "choice" => FieldKind::Choice,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            FieldKind::Db => "db",
            FieldKind::Hz => "hz",
            FieldKind::Percent => "percent",
            FieldKind::Ms => "ms",
            FieldKind::Beats => "beats",
            FieldKind::Number => "number",
            FieldKind::Switch => "switch",
            FieldKind::Choice => "choice",
        }
    }

    /// Whether the field is a number with a range.
    pub fn numeric(self) -> bool {
        !matches!(self, FieldKind::Switch | FieldKind::Choice)
    }

    fn unit(self) -> &'static str {
        match self {
            FieldKind::Db => "dB",
            FieldKind::Hz => "Hz",
            FieldKind::Percent => "%",
            FieldKind::Ms => "ms",
            FieldKind::Beats => "beats",
            _ => "",
        }
    }
}

/// A field as the manifest declares it.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldSpec {
    pub name: String,
    pub label: String,
    pub kind: FieldKind,
    pub min: f64,
    pub max: f64,
    pub default: FieldValue,
    /// Whether the value is best moved in equal ratios, as a frequency is.
    pub log: bool,
    /// Whether a lane may move it.
    pub lane: bool,
    pub choices: Vec<String>,
    pub unit: String,
}

impl FieldSpec {
    /// Refuses a value the field does not take, in words naming the field.
    pub fn accept(&self, value: &FieldValue) -> Result<()> {
        match (self.kind, value) {
            (FieldKind::Switch, FieldValue::Flag(_)) => Ok(()),
            (FieldKind::Switch, _) => Err(format!("{} takes true or false", self.name)),
            (FieldKind::Choice, FieldValue::Text(s)) if self.choices.iter().any(|c| c == s) => Ok(()),
            (FieldKind::Choice, _) => Err(format!("{} is one of {}", self.name, self.choices.join(", "))),
            (_, FieldValue::Number(x)) if *x >= self.min && *x <= self.max => Ok(()),
            (_, FieldValue::Number(x)) => Err(format!("{} is {} to {}; {x} is outside", self.name, num(self.min), num(self.max))),
            (_, _) => Err(format!("{} takes a number, {} to {}", self.name, num(self.min), num(self.max))),
        }
    }

    /// The default as JSON, for a song.
    pub fn default_json(&self) -> Json {
        match &self.default {
            FieldValue::Number(x) => json!(x),
            FieldValue::Flag(b) => json!(b),
            FieldValue::Text(s) => json!(s),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn num(x: f64) -> String {
    if x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// A device's manifest, `device.yaml`.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub format: i64,
    /// `effect` or `instrument`; MIDI effects are a later stage.
    pub kind: String,
    /// The title, such as `Tilt EQ`.
    pub name: String,
    /// The folder's name, which the song uses.
    pub slug: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub license: String,
    /// `offline` or `realtime`.
    pub run: String,
    /// The code, relative to the folder: `device.py` offline, `device.wasm`
    /// in real time.
    pub entry: String,
    /// What a real-time device's entry is built from, `device.rs`, when the
    /// folder holds its source.
    pub source: Option<String>,
    pub fields: Vec<FieldSpec>,
}

impl Manifest {
    pub fn field(&self, name: &str) -> Option<&FieldSpec> {
        self.fields.iter().find(|f| f.name == name)
    }

    fn field_names(&self) -> String {
        if self.fields.is_empty() {
            "none".to_string()
        } else {
            self.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ")
        }
    }

    /// The manifest as JSON, for listings and `describe`.
    pub fn json(&self) -> Json {
        json!({
            "format": self.format,
            "kind": self.kind,
            "name": self.name,
            "slug": self.slug,
            "description": self.description,
            "version": self.version,
            "author": self.author,
            "license": self.license,
            "run": self.run,
            "entry": self.entry,
            "source": self.source,
            "fields": self.fields.iter().map(|f| {
                let mut m = Map::new();
                m.insert("name".into(), json!(f.name));
                m.insert("label".into(), json!(f.label));
                m.insert("type".into(), json!(f.kind.name()));
                if f.kind.numeric() {
                    m.insert("min".into(), json!(f.min));
                    m.insert("max".into(), json!(f.max));
                    m.insert("scale".into(), json!(if f.log { "log" } else { "linear" }));
                    m.insert("lane".into(), json!(f.lane));
                }
                if f.kind == FieldKind::Choice {
                    m.insert("choices".into(), json!(f.choices));
                }
                m.insert("default".into(), f.default_json());
                if !f.unit.is_empty() {
                    m.insert("unit".into(), json!(f.unit));
                }
                Json::Object(m)
            }).collect::<Vec<_>>(),
        })
    }
}

/// Where a device was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Where {
    Project,
    Workspace,
}

/// A device as installed: its manifest, its folder and a hash of its code.
#[derive(Clone, Debug)]
pub struct Installed {
    pub manifest: Manifest,
    pub dir: PathBuf,
    pub from: Where,
    /// SHA-256 over the folder's files, so a change of code is a change of
    /// what every cache made with it was made from.
    pub code: String,
}

impl Installed {
    pub fn json(&self) -> Json {
        let mut m = self.manifest.json();
        m["folder"] = json!(self.dir);
        m["from"] = json!(match self.from {
            Where::Project => "project",
            Where::Workspace => "workspace",
        });
        m["code"] = json!(self.code);
        m
    }
}

/// Where the workspace's devices are: `devices` in the workspace.
pub fn dir() -> PathBuf {
    workspace().join("devices")
}

/// Where a project's own devices are.
pub fn project_dir(project: &Path) -> PathBuf {
    project.join("devices")
}

fn key(name: &str) -> Key {
    Key::str(name)
}

fn text(d: &Dict, name: &str) -> Result<Option<String>> {
    match d.get(&key(name)) {
        None | Some(Value::None) => Ok(None),
        Some(Value::Str(s)) => Ok(Some(s.clone())),
        Some(Value::Int(n)) => Ok(Some(n.to_string())),
        Some(Value::Float(x)) => Ok(Some(num(*x))),
        Some(_) => Err(format!("{name} must be text")),
    }
}

fn number(d: &Dict, name: &str) -> Result<Option<f64>> {
    match d.get(&key(name)) {
        None | Some(Value::None) => Ok(None),
        Some(Value::Int(n)) => Ok(Some(n.to_string().parse().map_err(|_| format!("{name} must be a number"))?)),
        Some(Value::Float(x)) => Ok(Some(*x)),
        Some(_) => Err(format!("{name} must be a number")),
    }
}

fn flag(d: &Dict, name: &str) -> Result<Option<bool>> {
    match d.get(&key(name)) {
        None | Some(Value::None) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(format!("{name} must be true or false")),
    }
}

fn field_spec(name: &str, spec: &Value) -> Result<FieldSpec> {
    let Value::Dict(d) = spec else {
        return Err(format!("field {name}: a field is a mapping with a type"));
    };
    for k in d.keys() {
        match k.as_str() {
            Some("type" | "min" | "max" | "default" | "scale" | "lane" | "choices" | "label" | "unit") => {}
            Some(other) => return Err(format!("field {name}: {other} is not part of a field; the parts are type, min, max, default, scale, lane, choices, label and unit")),
            None => return Err(format!("field {name}: parts are named")),
        }
    }
    let kind_name = text(d, "type")?.ok_or_else(|| format!("field {name}: type is required, one of {}", FIELD_TYPES.join(", ")))?;
    let kind = FieldKind::parse(&kind_name).ok_or_else(|| format!("field {name}: type {kind_name} is not one of {}", FIELD_TYPES.join(", ")))?;
    let label = text(d, "label")?.unwrap_or_else(|| readable(name));
    let unit = text(d, "unit")?.unwrap_or_else(|| kind.unit().to_string());
    let choices = match d.get(&key("choices")) {
        None | Some(Value::None) => Vec::new(),
        Some(Value::List(items)) => items
            .iter()
            .map(|c| match c {
                Value::Str(s) => Ok(s.clone()),
                Value::Int(n) => Ok(n.to_string()),
                _ => Err(format!("field {name}: choices are words")),
            })
            .collect::<Result<Vec<_>>>()?,
        Some(_) => return Err(format!("field {name}: choices is a list of words")),
    };
    let scale = text(d, "scale")?;
    let log = match scale.as_deref() {
        None => kind == FieldKind::Hz,
        Some("log") => true,
        Some("linear") => false,
        Some(other) => return Err(format!("field {name}: scale {other} is linear or log")),
    };
    let default = d.get(&key("default")).cloned().unwrap_or(Value::None);
    let (min, max, default, lane) = match kind {
        FieldKind::Switch => {
            let default = match default {
                Value::Bool(b) => b,
                Value::None => return Err(format!("field {name}: a switch has a default, true or false")),
                _ => return Err(format!("field {name}: a switch's default is true or false")),
            };
            (0.0, 1.0, FieldValue::Flag(default), false)
        }
        FieldKind::Choice => {
            if choices.len() < 2 {
                return Err(format!("field {name}: a choice has at least two choices"));
            }
            let default = match default {
                Value::Str(s) if choices.contains(&s) => s,
                Value::None => return Err(format!("field {name}: a choice has a default among its choices")),
                _ => return Err(format!("field {name}: the default is one of {}", choices.join(", "))),
            };
            (0.0, (choices.len() - 1) as f64, FieldValue::Text(default), false)
        }
        numeric => {
            let min = number(d, "min")?.ok_or_else(|| format!("field {name}: min is required for a {}", numeric.name()))?;
            let max = number(d, "max")?.ok_or_else(|| format!("field {name}: max is required for a {}", numeric.name()))?;
            if !(min.is_finite() && max.is_finite() && min < max) {
                return Err(format!("field {name}: min is below max"));
            }
            if log && min <= 0.0 {
                return Err(format!("field {name}: a log scale needs a min above 0"));
            }
            let default = match default {
                Value::Int(n) => n.to_string().parse::<f64>().map_err(|_| format!("field {name}: default must be a number"))?,
                Value::Float(x) => x,
                Value::None => return Err(format!("field {name}: default is required, {} to {}", num(min), num(max))),
                _ => return Err(format!("field {name}: default must be a number")),
            };
            if !(default >= min && default <= max) {
                return Err(format!("field {name}: default {} is outside {} to {}", num(default), num(min), num(max)));
            }
            (min, max, FieldValue::Number(default), flag(d, "lane")?.unwrap_or(true))
        }
    };
    Ok(FieldSpec {
        name: name.to_string(),
        label,
        kind,
        min,
        max,
        default,
        log,
        lane,
        choices,
        unit,
    })
}

/// A field's name as a label: `gain_db` reads Gain db.
fn readable(name: &str) -> String {
    let mut s: String = name.replace('_', " ");
    if let Some(first) = s.get(..1) {
        let upper = first.to_uppercase();
        s.replace_range(..1, &upper);
    }
    s
}

/// Reads a manifest from its text. `folder` is the folder's name, the slug
/// the song uses.
pub fn parse_manifest(text_of_file: &str, folder: &str) -> Result<Manifest> {
    let value = yaml_load::load(text_of_file)?;
    let Value::Dict(d) = &value else {
        return Err("a manifest is a mapping".into());
    };
    for k in d.keys() {
        match k.as_str() {
            Some("format" | "kind" | "name" | "description" | "version" | "author" | "license" | "run" | "entry" | "source" | "fields") => {}
            Some(other) => return Err(format!("{other} is not a field of a manifest")),
            None => return Err("a manifest's fields are named".into()),
        }
    }
    let format = match number(d, "format")? {
        Some(f) if f == FORMAT as f64 => FORMAT,
        Some(f) => return Err(format!("format {} is not one this reads; it reads format {FORMAT}", num(f))),
        None => return Err(format!("format: {FORMAT} is required")),
    };
    if !aaw_model::valid_device_slug(folder) {
        return Err(format!("the folder {folder} is not a slug: lowercase letters, digits and dashes"));
    }
    let kind = text(d, "kind")?.unwrap_or_else(|| "effect".into());
    match kind.as_str() {
        "effect" | "instrument" => {}
        "midi" => return Err("kind midi is a later stage: effects and instruments are built".into()),
        other => return Err(format!("kind {other} is not one of effect, instrument, midi")),
    }
    let run = text(d, "run")?.unwrap_or_else(|| "offline".into());
    let (default_entry, extension) = match run.as_str() {
        "offline" => ("device.py", ".py"),
        "realtime" => ("device.wasm", ".wasm"),
        other => return Err(format!("run {other} is offline or realtime")),
    };
    let name = text(d, "name")?.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).ok_or("name is required")?;
    let in_folder = |file: &str| !file.contains('/') && !file.contains("..") && !file.starts_with('.');
    let entry = text(d, "entry")?.unwrap_or_else(|| default_entry.into());
    if !in_folder(&entry) || !entry.ends_with(extension) {
        return Err(format!("entry {entry} is a {extension} file in the device's folder, for run {run}"));
    }
    let source = text(d, "source")?;
    match &source {
        Some(_) if run == "offline" => return Err("source is for a real-time device, whose entry is built from it; an offline device's entry is its source".into()),
        Some(file) if !in_folder(file) || !file.ends_with(".rs") => {
            return Err(format!("source {file} is a .rs file in the device's folder, which daw plugin build compiles"));
        }
        _ => {}
    }
    let mut fields = Vec::new();
    match d.get(&key("fields")) {
        None | Some(Value::None) => {}
        Some(Value::Dict(specs)) => {
            for (k, spec) in specs {
                let Some(fname) = k.as_str() else { return Err("fields are named".into()) };
                if !aaw_model::valid_device_field(fname) {
                    return Err(format!("field {fname}: a field's name is lowercase letters, digits and underscores, starting with a letter"));
                }
                fields.push(field_spec(fname, spec)?);
            }
        }
        Some(_) => return Err("fields is a mapping of names to fields".into()),
    }
    if fields.len() > 64 {
        return Err("a device has at most 64 fields".into());
    }
    Ok(Manifest {
        format,
        kind,
        name,
        slug: folder.to_string(),
        description: text(d, "description")?.unwrap_or_default(),
        version: text(d, "version")?.unwrap_or_else(|| "0".into()),
        author: text(d, "author")?.unwrap_or_default(),
        license: text(d, "license")?.unwrap_or_default(),
        run,
        entry,
        source,
        fields,
    })
}

/// SHA-256 over a device folder's files, by name and bytes, leaving out
/// Python's caches and hidden files.
fn code_hash(dir: &Path) -> Result<String> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(d) = pending.pop() {
        let entries = std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "__pycache__" {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut h = Sha256::new();
    for f in files {
        let rel = f.strip_prefix(dir).unwrap_or(&f).to_string_lossy().into_owned();
        h.update(rel.as_bytes());
        h.update([0]);
        let bytes = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(&bytes);
    }
    Ok(hex(&h.finalize()))
}

/// Reads the device in `folder`.
pub fn read(folder: &Path, from: Where) -> Result<Installed> {
    let file = folder.join("device.yaml");
    let name = folder.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let text_of_file = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let manifest = parse_manifest(&text_of_file, &name).map_err(|e| format!("{}: {e}", file.display()))?;
    // A real-time device whose source is there and not yet built is read,
    // so that it can be built; it plays once it is.
    let entry = folder.join(&manifest.entry);
    let built_later = manifest.source.as_ref().is_some_and(|s| folder.join(s).is_file());
    if !entry.is_file() && !built_later {
        return Err(format!("{}: the manifest's entry {} is not there", file.display(), manifest.entry));
    }
    Ok(Installed {
        manifest,
        dir: folder.to_path_buf(),
        from,
        code: code_hash(folder)?,
    })
}

/// The device a slug names: the project's own first, then the workspace's.
pub fn find(project: Option<&Path>, slug_of: &str) -> Result<Installed> {
    let want = slug(slug_of);
    if want.is_empty() {
        return Err(format!("{slug_of:?} names no device"));
    }
    let mut places = Vec::new();
    if let Some(p) = project {
        places.push((project_dir(p).join(&want), Where::Project));
    }
    places.push((dir().join(&want), Where::Workspace));
    for (folder, from) in &places {
        if folder.join("device.yaml").is_file() {
            return read(folder, *from);
        }
    }
    Err(format!(
        "No device {want}: none under {}; daw plugin list shows what is installed and daw plugin new makes one",
        places.iter().map(|(f, _)| f.parent().unwrap_or(f).display().to_string()).collect::<Vec<_>>().join(" or ")
    ))
}

/// The devices there are, and the folders that are not devices.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub devices: Vec<Installed>,
    pub problems: Vec<String>,
}

/// The devices installed: the project's own and the workspace's, a
/// project's shadowing the workspace's of the same slug, by name.
pub fn list(project: Option<&Path>) -> Listing {
    let mut out = Listing::default();
    let mut places = Vec::new();
    if let Some(p) = project {
        places.push((project_dir(p), Where::Project));
    }
    places.push((dir(), Where::Workspace));
    for (root, from) in places {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        let mut folders: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        folders.sort();
        for folder in folders {
            let name = folder.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if name.starts_with('.') || !folder.join("device.yaml").is_file() {
                continue;
            }
            if out.devices.iter().any(|d| d.manifest.slug == name) {
                continue;
            }
            match read(&folder, from) {
                Ok(d) => out.devices.push(d),
                Err(e) => out.problems.push(e),
            }
        }
    }
    out.devices.sort_by_key(|d| d.manifest.name.to_lowercase());
    out
}

/// Whether every word of `query` is in the device's name, slug, description
/// or a field's name.
pub fn matches(d: &Installed, query: &str) -> bool {
    let m = &d.manifest;
    query.split_whitespace().all(|word| {
        let word = word.to_lowercase();
        m.name.to_lowercase().contains(&word) || m.slug.contains(&word) || m.description.to_lowercase().contains(&word) || m.fields.iter().any(|f| f.name.contains(&word))
    })
}

/// The manifest `daw plugin new` writes, with NAME filled in.
pub const MANIFEST_TEMPLATE: &str = r#"# A device of your own. The manifest is the contract: what the song, the
# app's panel, daw set, lanes and the browser know of the device comes from
# here. daw describe effects, under device, says how it is used.
format: 1
kind: effect
name: NAME
description: "A gain, a tone and a warmth: the scaffold to edit."
version: "0.1"
author: ""
license: MIT
# offline: device.py runs in Python on the device's input over the whole
# song, when what feeds it changes; the result is cached and played. The
# other is realtime, on the audio thread: daw plugin new --realtime.
run: offline
entry: device.py
fields:
  # type: db, hz, percent, ms, beats or number take min, max and default;
  # scale is linear or log (hz is log unless said); lane says whether a lane
  # may move the field (yes unless said). switch has a default, true or
  # false; choice has choices and a default among them. label and unit are
  # optional.
  gain_db: {type: db, min: -24, max: 24, default: 0, label: Gain}
  tone_hz: {type: hz, min: 200, max: 20000, default: 20000, lane: false, label: Tone}
  warmth_percent: {type: percent, min: 0, max: 100, default: 0, label: Warmth}
  mode: {type: choice, choices: [stereo, mono], default: stereo}
  invert: {type: switch, default: false}
"#;

/// The code `daw plugin new` writes.
pub const CODE_TEMPLATE: &str = r#""""NAME: the scaffold of a device, to edit.

process(audio, sample_rate, fields) is the whole contract of an offline
device. audio is a float64 array of shape (frames, channels), the device's
input as the song makes it, over the whole song; sample_rate is the song's.
fields maps each name the manifest declares to its value: a float for a
number, True or False for a switch, a string for a choice, or, where a lane
moves the field, a float64 array of shape (frames, 1) with one value a
frame, which broadcasts against audio. Return a float array of shape
(frames, channels), (frames, 1) or (frames,) for the output; it may be
longer than the input, for a tail. Only numpy and scipy are promised.
"""

import numpy as np
from scipy.signal import butter, lfilter


def process(audio, sample_rate, fields):
    gain = 10 ** (np.asarray(fields["gain_db"], dtype=np.float64) / 20)
    out = audio * gain
    tone = float(fields["tone_hz"])
    if tone < sample_rate / 2 * 0.99:
        b, a = butter(1, tone / (sample_rate / 2))
        out = lfilter(b, a, out, axis=0)
    warmth = float(fields["warmth_percent"]) / 100
    if warmth > 0:
        drive = 1 + 3 * warmth
        out = (1 - warmth) * out + warmth * np.tanh(out * drive) / np.tanh(drive)
    if fields["mode"] == "mono" and out.shape[1] > 1:
        out = np.repeat(out.mean(axis=1, keepdims=True), out.shape[1], axis=1)
    if fields["invert"]:
        out = -out
    return out
"#;

/// The manifest `daw plugin new --realtime` writes, with NAME filled in.
pub const REALTIME_MANIFEST_TEMPLATE: &str = r#"# A device of your own. The manifest is the contract: what the song, the
# app's panel, daw set, lanes and the browser know of the device comes from
# here. daw describe effects, under device, says how it is used.
format: 1
kind: effect
name: NAME
description: "A gain, a tone and a warmth: the scaffold to edit."
version: "0.1"
author: ""
license: MIT
# realtime: device.wasm runs on the audio thread, built from device.rs by
# daw plugin build. The fields reach the code in this order.
run: realtime
entry: device.wasm
source: device.rs
fields:
  # type: db, hz, percent, ms, beats or number take min, max and default;
  # scale is linear or log (hz is log unless said); lane says whether a lane
  # may move the field (yes unless said). switch has a default, true or
  # false; choice has choices and a default among them. label and unit are
  # optional.
  gain_db: {type: db, min: -24, max: 24, default: 0, label: Gain}
  tone_hz: {type: hz, min: 200, max: 20000, default: 20000, label: Tone}
  warmth_percent: {type: percent, min: 0, max: 100, default: 0, label: Warmth}
  mode: {type: choice, choices: [stereo, mono], default: stereo}
  invert: {type: switch, default: false}
"#;

/// The source `daw plugin new --realtime` writes: Rust, built to the
/// device ABI.
pub const RUST_TEMPLATE: &str = include_str!("../templates/device.rs");

/// The manifest `daw plugin new --instrument` writes, with NAME and RUN
/// filled in.
pub const INSTRUMENT_MANIFEST_TEMPLATE: &str = r#"# An instrument of your own. The manifest is the contract: what the song,
# the app's panel, daw set, lanes and the browser know of the instrument
# comes from here. daw describe midi, under device, says how it is used.
format: 1
kind: instrument
name: NAME
description: "A wave, an envelope and a tone, a voice a note: the scaffold to edit."
version: "0.1"
author: ""
license: MIT
RUN
fields:
  # type: db, hz, percent, ms, beats or number take min, max and default;
  # scale is linear or log (hz is log unless said); lane says whether a lane
  # may move the field (yes unless said). switch has a default, true or
  # false; choice has choices and a default among them. label and unit are
  # optional.
  gain_db: {type: db, min: -36, max: 12, default: -12, label: Gain}
  wave: {type: choice, choices: [sine, saw, square], default: saw}
  attack_ms: {type: ms, min: 0, max: 2000, default: 5, lane: false, label: Attack}
  release_ms: {type: ms, min: 1, max: 5000, default: 250, lane: false, label: Release}
  tone_hz: {type: hz, min: 200, max: 20000, default: 6000, label: Tone}
"#;

/// How an offline instrument's manifest says where its code is.
const INSTRUMENT_OFFLINE_RUN: &str = "# offline: device.py runs in Python on the track's notes over the whole\n# song, when they or the fields change; the result is cached and played.\n# The other is realtime, on the audio thread, which also plays previews:\n# daw plugin new --instrument --realtime.\nrun: offline\nentry: device.py";

/// How a real-time instrument's manifest says where its code is.
const INSTRUMENT_REALTIME_RUN: &str = "# realtime: device.wasm runs on the audio thread, built from device.rs by\n# daw plugin build. The fields reach the code in this order.\nrun: realtime\nentry: device.wasm\nsource: device.rs";

/// The code `daw plugin new --instrument` writes.
pub const INSTRUMENT_CODE_TEMPLATE: &str = r#""""NAME: the scaffold of an instrument, to edit.

play(notes, frames, sample_rate, fields) is the whole contract of an
offline instrument. notes holds the track's notes as they play, after its
MIDI effects, a row a note in the order they start: notes["start"] and
notes["end"] are frames of the song, notes["pitch"] the MIDI note and
notes["velocity"] 1 to 127; a row unpacks as start, end, pitch, velocity.
frames is the song's length in frames and sample_rate its rate. fields maps
each name the manifest declares to its value: a float for a number, True or
False for a switch, a string for a choice, or, where a lane moves the field,
a float64 array of shape (frames, 1) with one value a frame. Return a float
array of shape (frames, channels), (frames, 1) or (frames,): the
instrument's sound over the whole song, frame N at the song's frame N.
Only numpy and scipy are promised.
"""

import numpy as np
from scipy.signal import lfilter


def play(notes, frames, sample_rate, fields):
    out = np.zeros(frames)
    attack = max(1, int(float(fields["attack_ms"]) / 1000 * sample_rate))
    release = max(1, int(float(fields["release_ms"]) / 1000 * sample_rate))
    for start, end, pitch, velocity in notes:
        held = max(1, end - start)
        n = min(held + release, frames - start)
        if n <= 0:
            continue
        t = np.arange(n)
        hz = 440 * 2 ** ((pitch - 69) / 12)
        phase = (hz * t / sample_rate) % 1
        if fields["wave"] == "sine":
            wave = np.sin(2 * np.pi * phase)
        elif fields["wave"] == "square":
            wave = np.where(phase < 0.5, 1.0, -1.0)
        else:
            wave = 2 * phase - 1
        # Up over the attack while held, then down over the release.
        level = np.minimum(1, (t + 1) / attack)
        level = np.where(t < held, level, level[min(held, n) - 1] * np.maximum(0, 1 - (t - held) / release))
        out[start:start + n] += wave * level * velocity / 127
    # The tone: a one-pole lowpass, its cutoff from the first frame's value.
    tone = float(np.asarray(fields["tone_hz"]).flat[0])
    pole = np.exp(-2 * np.pi * min(tone, 0.49 * sample_rate) / sample_rate)
    out = lfilter([1 - pole], [1, -pole], out)
    gain = 10 ** (np.asarray(fields["gain_db"], dtype=np.float64) / 20)
    return out.reshape(-1, 1) * gain
"#;

/// The source `daw plugin new --instrument --realtime` writes: Rust, built
/// to the device ABI with `aaw_note`.
pub const RUST_INSTRUMENT_TEMPLATE: &str = include_str!("../templates/instrument.rs");

/// Makes a device folder for `name` under `root`, the project's or the
/// workspace's `devices/`, from the templates: a working effect, or with
/// `instrument` an instrument, to edit, offline in Python or, with
/// `realtime`, in Rust for the audio thread, which is left to `build`.
/// Refuses a folder that is there.
pub fn new(name: &str, root: &Path, realtime: bool, instrument: bool) -> Result<Installed> {
    let name = name.trim();
    let slug_of = slug(name);
    if slug_of.is_empty() {
        return Err(format!("A device's name has a letter or a digit in it; {name:?} has none"));
    }
    let folder = root.join(&slug_of);
    if folder.exists() {
        return Err(format!("{} is already there; choose another name or edit it", folder.display()));
    }
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let (manifest, file, code) = match (instrument, realtime) {
        (false, true) => (REALTIME_MANIFEST_TEMPLATE.to_string(), "device.rs", RUST_TEMPLATE),
        (false, false) => (MANIFEST_TEMPLATE.to_string(), "device.py", CODE_TEMPLATE),
        (true, true) => (INSTRUMENT_MANIFEST_TEMPLATE.replace("RUN", INSTRUMENT_REALTIME_RUN), "device.rs", RUST_INSTRUMENT_TEMPLATE),
        (true, false) => (INSTRUMENT_MANIFEST_TEMPLATE.replace("RUN", INSTRUMENT_OFFLINE_RUN), "device.py", INSTRUMENT_CODE_TEMPLATE),
    };
    let manifest = manifest.replace("NAME", name);
    let code = code.replacen("NAME", name, 1).replace("SLUG", &slug_of);
    aaw_model::atomic_write(&folder.join("device.yaml"), &manifest).map_err(|e| e.to_string())?;
    aaw_model::atomic_write(&folder.join(file), &code).map_err(|e| e.to_string())?;
    read(&folder, if root == dir() { Where::Workspace } else { Where::Project })
}

/// The Python that runs devices, as a command on the runner module.
fn runner() -> Result<Command> {
    let mut python = Command::new(python::interpreter()?);
    python.args(["-m", "agent_daw.device_runner"]);
    Ok(python)
}

fn run_python(mut command: Command, what: &str) -> Result<Json> {
    let out = command.output().map_err(|e| format!("Could not start Python for {what}: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        let message = stderr
            .lines()
            .rev()
            .find_map(|l| serde_json::from_str::<Json>(l.trim()).ok())
            .and_then(|j| j.get("error").and_then(Json::as_str).map(str::to_string));
        return Err(message.unwrap_or_else(|| match stderr.trim() {
            "" => format!("{what} failed with status {}", out.status),
            text => text.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n"),
        }));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("{what} printed no JSON: {e}"))
}

/// The compiler a real-time device's source is built with: `AAW_RUSTC`,
/// else rustup's under `~/.cargo/bin`, else `rustc` on the path.
fn rustc() -> PathBuf {
    if let Some(r) = std::env::var_os("AAW_RUSTC") {
        return PathBuf::from(r);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let rustup = Path::new(&home).join(".cargo/bin/rustc");
        if rustup.is_file() {
            return rustup;
        }
    }
    PathBuf::from("rustc")
}

/// Builds a real-time device: compiles its source to its entry with rustc
/// for WebAssembly, then checks it. The reply is the check's, with the
/// module's file, its size and the seconds the build took.
pub fn build(device: &Installed) -> Result<Json> {
    let m = &device.manifest;
    if m.run != "realtime" {
        return Err(format!("{} runs offline: its {} is run as it is, with nothing to build", m.slug, m.entry));
    }
    let source = m.source.as_ref().ok_or_else(|| format!("{} names no source to build; its device.yaml says source: device.rs", m.slug))?;
    let src = device.dir.join(source);
    if !src.is_file() {
        return Err(format!("{}: the source {source} is not there", device.dir.display()));
    }
    let entry = device.dir.join(&m.entry);
    let partial = device.dir.join(format!(".{}.building", m.entry));
    let started = std::time::Instant::now();
    let compiler = rustc();
    let out = Command::new(&compiler)
        .args(["--edition", "2024", "--crate-type", "cdylib", "--target", "wasm32-unknown-unknown"])
        .args(["-C", "opt-level=3", "-C", "panic=abort", "-C", "strip=symbols"])
        .arg("-o")
        .arg(&partial)
        .arg(&src)
        .current_dir(&device.dir)
        .output()
        .map_err(|e| format!("Could not start {}: {e}; Rust from rustup.rs builds a real-time device", compiler.display()))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&partial);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let mut message = stderr.trim().to_string();
        if stderr.contains("wasm32-unknown-unknown") && (stderr.contains("can't find crate") || stderr.contains("target may not be installed")) {
            message.push_str("\nThe WebAssembly target is not installed: rustup target add wasm32-unknown-unknown installs it");
        }
        return Err(format!("{source} does not build:\n{message}"));
    }
    std::fs::rename(&partial, &entry).map_err(|e| format!("{}: {e}", entry.display()))?;
    let built = read(&device.dir, device.from)?;
    let mut reply = check(&built)?;
    reply["built"] = json!(entry);
    reply["bytes"] = json!(std::fs::metadata(&entry).map(|m| m.len()).unwrap_or(0));
    reply["build_seconds"] = json!((started.elapsed().as_secs_f64() * 1000.0).round() / 1000.0);
    Ok(reply)
}

impl Installed {
    pub fn realtime(&self) -> bool {
        self.manifest.run == "realtime"
    }

    /// Whether it is an instrument, which a MIDI track's notes play.
    pub fn instrument(&self) -> bool {
        self.manifest.kind == "instrument"
    }

    /// The fields as a real-time device's code is given them.
    pub fn wasm_fields(&self) -> Vec<wasm::Field> {
        self.manifest
            .fields
            .iter()
            .map(|f| wasm::Field {
                name: f.name.clone(),
                default: f.default.clone(),
                min: f.min,
                max: f.max,
                log: f.log,
                choices: f.choices.clone(),
            })
            .collect()
    }

    /// Whether the source was changed after the module was built from it.
    pub fn stale(&self) -> bool {
        let modified = |f: &str| std::fs::metadata(self.dir.join(f)).and_then(|m| m.modified()).ok();
        match (self.manifest.source.as_deref().and_then(modified), modified(&self.manifest.entry)) {
            (Some(source), Some(entry)) => source > entry,
            _ => false,
        }
    }

    /// A real-time device's code made ready for a song at `rate` and
    /// `tempo`: its module compiled, its exports checked, and run once.
    pub fn code(&self, rate: f64, tempo: f64) -> Result<Arc<wasm::Code>> {
        let m = &self.manifest;
        let file = self.dir.join(&m.entry);
        if !file.is_file() {
            return Err(match &m.source {
                Some(source) => format!("{} is not built yet: daw plugin build {} compiles {source}", m.entry, m.slug),
                None => format!("{} is not there", m.entry),
            });
        }
        let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        wasm::Code::new(&bytes, self.wasm_fields(), rate, tempo, self.instrument()).map(Arc::new).map_err(|e| format!("{}: {e}", m.entry))
    }
}

/// The notes an instrument's check plays over a second at 48 kHz, as
/// (start, end, key, velocity): a chord held half a second, then a note
/// above it.
const CHECK_NOTES: [(usize, usize, u8, u8); 4] = [(0, 24000, 60, 100), (0, 24000, 64, 90), (0, 24000, 67, 80), (28800, 43200, 72, 110)];

/// Runs a real-time device for a second of noise at 48 kHz, or an
/// instrument for a second of `CHECK_NOTES`, with every field at its
/// default: whether its module meets the ABI and runs, and what share of
/// the time it took.
fn check_realtime(device: &Installed) -> Result<Json> {
    use aaw_dsp::instrument::NoteEvent;
    const RATE: usize = 48000;
    const BLOCK: usize = 512;
    let code = device.code(RATE as f64, 120.0)?;
    let defaults = device.manifest.fields.iter().map(|f| (f.name.clone(), f.default.clone())).collect();
    let mut running = code.instance_at(&defaults);
    // A fixed noise, the same every check; an instrument's input is silence.
    let mut seed: u32 = 1;
    let mut noise = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (f64::from(seed >> 8) / f64::from(1u32 << 24) - 0.5) * 0.5
    };
    let instrument = device.instrument();
    let mut audio: Vec<aaw_dsp::Frame> = (0..RATE).map(|_| if instrument { [0.0, 0.0] } else { [noise(), noise()] }).collect();
    let mut notes = Vec::with_capacity(16);
    let started = std::time::Instant::now();
    for (k, block) in audio.chunks_mut(BLOCK).enumerate() {
        let at = k * BLOCK;
        let clock = aaw_dsp::Clock {
            frame: at as i64,
            step: 1,
            total: RATE as i64,
        };
        if instrument {
            notes.clear();
            for &(start, end, key, velocity) in &CHECK_NOTES {
                for (frame, v) in [(start, velocity), (end, 0)] {
                    if (at..at + block.len()).contains(&frame) {
                        notes.push(NoteEvent { at: frame - at, key, velocity: v });
                    }
                }
            }
            notes.sort_by_key(|e| (e.at, e.velocity != 0));
            running.play(block, clock, &notes);
        } else {
            running.process(block, clock);
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    if let Some(e) = code.failure() {
        return Err(format!("{}: {e}", device.manifest.entry));
    }
    let peak = audio.iter().flat_map(|f| f.iter()).fold(0.0f64, |m, x| m.max(x.abs()));
    let mut reply = json!({
        "ok": true,
        "run": "realtime",
        "frames_in": RATE,
        "frames_out": RATE,
        "channels": 2,
        "peak": peak,
        "latency": code.latency(),
        // The share of real time one instance takes on this Mac.
        "load_percent": (seconds * 100.0 * 1000.0).round() / 1000.0,
    });
    if device.stale() {
        reply["stale"] = json!(format!("{} was changed after {} was built: daw plugin build {} builds it again", device.manifest.source.as_deref().unwrap_or_default(), device.manifest.entry, device.manifest.slug));
    }
    Ok(reply)
}

/// Runs the device's code once on a short noise, with every field at its
/// default: whether it loads, has `process` and returns audio of the right
/// shape. The reply says the frames in and out and the channels; a
/// real-time device's also its latency and its load.
pub fn check(device: &Installed) -> Result<Json> {
    if device.realtime() {
        let mut reply = check_realtime(device)?;
        reply["device"] = json!(device.manifest.slug);
        reply["folder"] = json!(device.dir);
        return Ok(reply);
    }
    // An instrument plays a second of the check's notes; an effect half a
    // second of noise.
    let (frames, notes) = match device.instrument() {
        true => (48000, CHECK_NOTES.iter().map(|&(s, e, k, v)| json!([s, e, k, v])).collect::<Vec<_>>()),
        false => (24000, Vec::new()),
    };
    let params = json!({
        "sample_rate": 48000,
        "tempo": 120.0,
        "frames": frames,
        "fields": device.manifest.fields.iter().map(|f| (f.name.clone(), f.default_json())).collect::<Map<String, Json>>(),
        "lanes": {},
        "notes": notes,
    });
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let params_file = work.path().join("params.json");
    std::fs::write(&params_file, params.to_string()).map_err(|e| e.to_string())?;
    let mut command = runner()?;
    command.args(["--check", "--device"]).arg(&device.dir).arg("--params").arg(&params_file);
    let mut reply = run_python(command, &format!("the check of {}", device.manifest.slug))?;
    reply["device"] = json!(device.manifest.slug);
    reply["folder"] = json!(device.dir);
    Ok(reply)
}

// ---------------------------------------------------------------------------
// The devices of a song

/// One device in a song's chains, or a track's instrument.
#[derive(Clone, Debug)]
pub struct Instance {
    /// The chain's owner as the host addresses it: `tracks.T`.
    pub owner: String,
    /// Its place in the chain, or `custom::INSTRUMENT` for the track's
    /// instrument.
    pub index: usize,
    /// The effect as commands name it, `tracks.T.effects.REF`, or the
    /// instrument, `tracks.T.instrument`.
    pub reference: String,
    pub slug: String,
    pub version: Option<String>,
    /// The device as installed, or why it is not.
    pub installed: std::result::Result<Installed, String>,
    /// Whether the engine runs it: not bypassed, and not on a frozen track.
    pub heard: bool,
    /// What an offline device's cache is made from, when it is installed
    /// and heard; a real-time device has none.
    pub identity: Option<String>,
}

impl Instance {
    pub fn key(&self) -> (String, usize) {
        (self.owner.clone(), self.index)
    }

    /// Whether it is a track's instrument rather than an effect.
    pub fn is_instrument(&self) -> bool {
        self.index == custom::INSTRUMENT
    }

    /// The cache's file, under the project's `.daw/devices/`.
    pub fn cache_file(&self, project: &Path) -> Option<PathBuf> {
        self.identity.as_ref().map(|id| cache_dir(project).join(format!("{id}.wav")))
    }
}

/// Where a project's caches are.
pub fn cache_dir(project: &Path) -> PathBuf {
    project.join(".daw").join("devices")
}

fn effect_reference(e: &Effect, index: usize) -> String {
    aaw_model::rules::effect_ref(e, index)
}

/// The devices of a song, in the order their inputs can be made: tracks,
/// each with its instrument first, then groups each after the groups in it,
/// returns, then the master, and within a chain in order, so that a device
/// fed by another comes after it.
pub fn instances(p: &Project, project: &Path) -> Vec<Instance> {
    let mut owners: Vec<(String, &[Effect], bool, Option<&aaw_model::Device>)> = Vec::new();
    for t in &p.tracks {
        owners.push((format!("tracks.{}", t.id), &t.effects, t.frozen.is_none(), t.midi.as_ref().and_then(|m| m.device())));
    }
    for g in p.group_order() {
        owners.push((format!("groups.{}", g.id), &g.effects, true, None));
    }
    for r in &p.returns {
        owners.push((format!("returns.{}", r.id), &r.effects, true, None));
    }
    owners.push(("master".to_string(), &p.master.effects, true, None));
    let mut found: HashMap<String, std::result::Result<Installed, String>> = HashMap::new();
    let mut out = Vec::new();
    for (owner, effects, live, instrument) in owners {
        if let Some(d) = instrument {
            let installed = found.entry(d.name.clone()).or_insert_with(|| find(Some(project), &d.name)).clone();
            out.push(Instance {
                owner: owner.clone(),
                index: custom::INSTRUMENT,
                reference: format!("{owner}.instrument"),
                slug: d.name.clone(),
                version: d.version.clone(),
                installed,
                heard: live,
                identity: None,
            });
        }
        for (index, e) in effects.iter().enumerate() {
            let Effect::Device(d) = e else { continue };
            let installed = found.entry(d.name.clone()).or_insert_with(|| find(Some(project), &d.name)).clone();
            out.push(Instance {
                owner: owner.clone(),
                index,
                reference: format!("{owner}.effects.{}", effect_reference(e, index)),
                slug: d.name.clone(),
                version: d.version.clone(),
                installed,
                heard: live && !d.bypass,
                identity: None,
            });
        }
    }
    // Every device's code is part of every identity: a device fed by
    // another whose code changed is made again.
    let codes: Vec<String> = {
        let mut c: Vec<String> = out.iter().filter_map(|i| i.installed.as_ref().ok().map(|d| format!("{}={}", d.manifest.slug, d.code))).collect();
        c.sort();
        c.dedup();
        c
    };
    for inst in &mut out {
        let Ok(device) = &inst.installed else { continue };
        if !inst.heard || device.realtime() {
            continue;
        }
        if inst.is_instrument() {
            inst.identity = instrument_identity(p, &inst.owner, device);
            continue;
        }
        let Ok(fed) = custom::cut(p, &inst.owner, inst.index) else { continue };
        let mut h = Sha256::new();
        h.update(aaw_model::to_yaml(&fed).as_bytes());
        h.update([0]);
        // The device's own entry, with its fields, and the lanes on it.
        let (effects, lanes) = owner_parts(p, &inst.owner).expect("the owner");
        h.update(aaw_model::to_yaml(&own_project(p, &effects[inst.index], lanes, inst.index, &inst.owner)).as_bytes());
        h.update([0]);
        h.update(device.code.as_bytes());
        h.update([0]);
        h.update(codes.join(",").as_bytes());
        h.update([0]);
        h.update(p.session.sample_rate.to_string().as_bytes());
        inst.identity = Some(hex(&h.finalize()));
    }
    out
}

/// What an offline instrument's cache is made from: the track's notes as
/// they play, the instrument's entry and the lanes on its fields, its code,
/// the song's length, tempo and rate.
fn instrument_identity(p: &Project, owner: &str, device: &Installed) -> Option<String> {
    let t = p.track(owner.strip_prefix("tracks.")?)?;
    let midi = t.midi.as_ref()?;
    let d = midi.device()?;
    let mut h = Sha256::new();
    for n in aaw_model::schedule::track_notes(p, midi) {
        h.update(format!("{},{},{},{};", n.start, n.end, n.pitch, n.velocity).as_bytes());
    }
    h.update([0]);
    h.update(aaw_model::yaml_emit::dump(&aaw_model::Instrument::Device(d.clone()).dump(false)).as_bytes());
    for lane in instrument_lanes(t) {
        h.update(aaw_model::yaml_emit::dump(&lane.dump(false)).as_bytes());
    }
    h.update([0]);
    h.update(device.code.as_bytes());
    h.update([0]);
    h.update(format!("{}|{}|{}", total_frames(p), p.session.tempo, p.session.sample_rate).as_bytes());
    Some(hex(&h.finalize()))
}

/// The lanes on a track's instrument of the person's own, on
/// `instrument.fields.F`.
fn instrument_lanes(t: &aaw_model::Track) -> Vec<&aaw_model::Lane> {
    t.automation
        .iter()
        .filter(|lane| matches!(aaw_model::rules::target(aaw_model::rules::Owner::Track(t), &lane.param), Ok(x) if x.kind == aaw_model::rules::TargetKind::Instrument))
        .collect()
}

/// The song's length in frames, as the engine plays it.
fn total_frames(p: &Project) -> usize {
    aaw_model::frame(&p.session.length_exact(), p.session.tempo, p.session.sample_rate).max(0) as usize
}

fn owner_parts<'a>(p: &'a Project, owner: &str) -> Option<(&'a [Effect], &'a [aaw_model::Lane])> {
    if owner == "master" {
        return Some((&p.master.effects, &p.master.automation));
    }
    let (list, name) = owner.split_once('.')?;
    match list {
        "tracks" => p.track(name).map(|t| (&t.effects[..], &t.automation[..])),
        "groups" => p.group(name).map(|g| (&g.effects[..], &g.automation[..])),
        "returns" => p.returns.iter().find(|r| r.id == name).map(|r| (&r.effects[..], &r.automation[..])),
        _ => None,
    }
}

/// A small document of the device's own entry and the lanes on it, as text
/// for its identity: what of the song, beyond what feeds it, it is made of.
fn own_project(p: &Project, e: &Effect, lanes: &[aaw_model::Lane], index: usize, owner: &str) -> Project {
    let mut own = Project::validate(&aaw_model::value::dict(vec![("session", aaw_model::value::dict(vec![]))])).expect("a blank song");
    own.session.tempo = p.session.tempo;
    own.session.sample_rate = p.session.sample_rate;
    own.session.length_beats = p.session.length_beats.clone();
    own.master.effects = vec![e.with_bypass(false)];
    let who = owner_enum(p, owner);
    for lane in lanes {
        let Some(who) = who else { break };
        if let Ok(t) = aaw_model::rules::target(who, &lane.param) {
            if let aaw_model::rules::TargetKind::Effect { index: i, .. } = t.kind {
                if i == index {
                    let mut l = lane.clone();
                    l.param = format!("effects.0.{}", t.field);
                    own.master.automation.push(l);
                }
            }
        }
    }
    own
}

fn owner_enum<'a>(p: &'a Project, owner: &str) -> Option<aaw_model::rules::Owner<'a>> {
    use aaw_model::rules::Owner;
    if owner == "master" {
        return Some(Owner::Master(&p.master));
    }
    let (list, name) = owner.split_once('.')?;
    match list {
        "tracks" => p.track(name).map(Owner::Track),
        "groups" => p.group(name).map(Owner::Group),
        "returns" => p.returns.iter().find(|r| r.id == name).map(Owner::Return),
        _ => None,
    }
}

/// Refuses a device whose fields or the lanes on it are not what its
/// manifest allows: a field the device has not, a value outside its range
/// or choices, a lane on a field that takes none; in words naming the
/// field and what it takes. `e` is the device at `index` of `owner`'s chain
/// and `lanes` the owner's lanes.
fn check_device(p: &Project, owner: &str, index: usize, e: &Effect, lanes: &[aaw_model::Lane], device: &Installed) -> Result<()> {
    let Effect::Device(d) = e else { return Ok(()) };
    let reference = format!("{owner}.effects.{}", effect_reference(e, index));
    let m = &device.manifest;
    if device.instrument() {
        return Err(format!("{reference}: {} is an instrument, which a MIDI track's notes play: daw instrument set SONG TRACK {}", d.name, d.name));
    }
    for (name, value) in &d.fields {
        let Some(spec) = m.field(name) else {
            return Err(format!("{reference}: {} has no field {name}; its fields are {}", d.name, m.field_names()));
        };
        spec.accept(value).map_err(|e| format!("{reference}.fields.{e}"))?;
    }
    let Some(who) = owner_enum(p, owner) else { return Ok(()) };
    for lane in lanes {
        let Ok(t) = aaw_model::rules::target(who, &lane.param) else { continue };
        let aaw_model::rules::TargetKind::Effect { index: i, .. } = t.kind else { continue };
        if i != index {
            continue;
        }
        let field = t.field.strip_prefix("fields.").unwrap_or(&t.field);
        match m.field(field) {
            None => return Err(format!("{owner}.automation: {}: {} has no field {field}; its fields are {}", lane.param, d.name, m.field_names())),
            Some(spec) if !spec.lane => return Err(format!("{owner}.automation: {}: {} does not take a lane on {field}", lane.param, d.name)),
            Some(_) => {}
        }
    }
    Ok(())
}

/// Refuses a track's instrument of the person's own whose fields or lanes
/// are not what its manifest allows, or whose manifest is an effect's.
fn check_instrument(t: &aaw_model::Track, d: &aaw_model::Device, device: &Installed) -> Result<()> {
    let reference = format!("tracks.{}.instrument.device", t.id);
    let m = &device.manifest;
    if !device.instrument() {
        return Err(format!("{reference}: {} is an effect, which goes in a chain: daw effect add SONG tracks.{} --type {}", d.name, t.id, d.name));
    }
    for (name, value) in &d.fields {
        let Some(spec) = m.field(name) else {
            return Err(format!("{reference}: {} has no field {name}; its fields are {}", d.name, m.field_names()));
        };
        spec.accept(value).map_err(|e| format!("{reference}.fields.{e}"))?;
    }
    for lane in instrument_lanes(t) {
        let field = lane.param.strip_prefix("instrument.fields.").unwrap_or(&lane.param);
        match m.field(field) {
            None => return Err(format!("tracks.{}.automation: {}: {} has no field {field}; its fields are {}", t.id, lane.param, d.name, m.field_names())),
            Some(spec) if !spec.lane => return Err(format!("tracks.{}.automation: {}: {} does not take a lane on {field}", t.id, lane.param, d.name)),
            Some(_) => {}
        }
    }
    Ok(())
}

/// Refuses a song whose devices' fields or lanes are not what their
/// manifests allow, an instrument in a chain or an effect as a track's
/// instrument. A device that is not installed cannot be checked and
/// passes.
pub fn check_song(p: &Project, project: &Path) -> Result<()> {
    let mut found: HashMap<String, Option<Installed>> = HashMap::new();
    for t in &p.tracks {
        let Some(d) = t.midi.as_ref().and_then(|m| m.device()) else { continue };
        let installed = found.entry(d.name.clone()).or_insert_with(|| find(Some(project), &d.name).ok());
        if let Some(device) = installed {
            check_instrument(t, d, device)?;
        }
    }
    let mut owners: Vec<(String, &[Effect], &[aaw_model::Lane])> = Vec::new();
    for t in &p.tracks {
        owners.push((format!("tracks.{}", t.id), &t.effects, &t.automation));
    }
    for g in &p.groups {
        owners.push((format!("groups.{}", g.id), &g.effects, &g.automation));
    }
    for r in &p.returns {
        owners.push((format!("returns.{}", r.id), &r.effects, &r.automation));
    }
    owners.push(("master".into(), &p.master.effects, &p.master.automation));
    for (owner, effects, lanes) in owners {
        for (index, e) in effects.iter().enumerate() {
            let Effect::Device(d) = e else { continue };
            let installed = found.entry(d.name.clone()).or_insert_with(|| find(Some(project), &d.name).ok());
            let Some(device) = installed else { continue };
            check_device(p, &owner, index, e, lanes, device)?;
        }
    }
    Ok(())
}

/// The effect a `daw effect add --type SLUG` makes: `{type: device, name,
/// version, fields}` with the manifest's defaults under any fields given,
/// `--gain-db -3` as `gain_db`. `given` is the command's fields, `type`
/// among them.
pub fn effect_node(project: Option<&Path>, slug_of: &str, given: &Map<String, Json>) -> Result<Map<String, Json>> {
    let device = find(project, slug_of)?;
    let m = &device.manifest;
    if device.instrument() {
        return Err(format!("{} is an instrument, which a MIDI track's notes play: daw instrument set SONG TRACK {}", m.slug, m.slug));
    }
    let mut out = Map::new();
    out.insert("type".into(), json!("device"));
    if let Some(id) = given.get("id") {
        out.insert("id".into(), id.clone());
    }
    out.insert("name".into(), json!(m.slug));
    out.insert("version".into(), json!(m.version));
    let mut fields: Map<String, Json> = m.fields.iter().map(|f| (f.name.clone(), f.default_json())).collect();
    for (k, v) in given {
        match k.as_str() {
            "type" | "id" | "bypass" | "name" | "version" | "index" | "owner" => {}
            "fields" => {
                if let Json::Object(f) = v {
                    for (name, value) in f {
                        if m.field(name).is_none() {
                            return Err(format!("{} has no field {name}; its fields are {}", m.slug, m.field_names()));
                        }
                        fields.insert(name.clone(), value.clone());
                    }
                }
            }
            name => {
                if m.field(name).is_none() {
                    return Err(format!("{} has no field {name}; its fields are {}", m.slug, m.field_names()));
                }
                fields.insert(name.to_string(), v.clone());
            }
        }
    }
    out.insert("fields".into(), Json::Object(fields));
    if let Some(b) = given.get("bypass") {
        out.insert("bypass".into(), b.clone());
    }
    Ok(out)
}

/// The instrument `daw instrument set SONG TRACK SLUG` puts on a track:
/// `{device: {name, version, fields}}` with the manifest's defaults.
pub fn instrument_node(project: Option<&Path>, slug_of: &str) -> Result<Json> {
    let device = find(project, slug_of)?;
    let m = &device.manifest;
    if !device.instrument() {
        return Err(format!("{} is an effect, which goes in a chain: daw effect add SONG OWNER --type {}", m.slug, m.slug));
    }
    let fields: Map<String, Json> = m.fields.iter().map(|f| (f.name.clone(), f.default_json())).collect();
    Ok(json!({"device": {"name": m.slug, "version": m.version, "fields": fields}}))
}

/// Reads a cache file: stereo float frames.
pub fn load_cache(file: &Path) -> Result<Arc<Vec<[f32; 2]>>> {
    let audio = aaw_engine::sndfile::read(file)?;
    Ok(Arc::new(to_stereo(&audio)))
}

fn to_stereo(audio: &aaw_engine::sndfile::Audio) -> Vec<[f32; 2]> {
    let c = audio.channels.max(1);
    audio
        .data
        .chunks(c)
        .map(|f| if c == 1 { [f[0] as f32; 2] } else { [f[0] as f32, f[1] as f32] })
        .collect()
}

/// Makes a device's cache: renders what feeds it, with the caches of the
/// devices before it, evaluates the lanes on its fields a frame at a time,
/// runs its code on them and writes the result under `.daw/devices/`,
/// returning the audio. `p` is the song as it stands.
pub fn compute(p: &Project, project: &Path, inst: &Instance, caches: &Caches) -> Result<Arc<Vec<[f32; 2]>>> {
    let device = inst.installed.clone()?;
    let identity = inst.identity.clone().ok_or("the device is not heard")?;
    let file = cache_dir(project).join(format!("{identity}.wav"));
    if file.is_file() {
        return load_cache(&file);
    }
    if inst.is_instrument() {
        return compute_instrument(p, project, inst, &device, &identity);
    }
    // A song written by hand, or rendered without a host, is held to the
    // manifest here, as every edit through a host is.
    let (effects, lanes) = owner_parts(p, &inst.owner).ok_or("the owner")?;
    check_device(p, &inst.owner, inst.index, &effects[inst.index], lanes, &device)?;
    let rate = p.session.sample_rate;
    let input = custom::input(p, project, &inst.owner, inst.index, BLOCK, caches)?;
    let frames = input.len();
    let work = cache_dir(project).join(format!("work-{}-{}", std::process::id(), &identity[..12]));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let result = (|| -> Result<Arc<Vec<[f32; 2]>>> {
        let input_file = work.join("input.wav");
        aaw_engine::wav::write_atomic(&input_file, &aaw_engine::wav::float_wav_bytes(&input, rate as u32))?;
        // The fields: the song's values over the defaults, and a file of
        // one value a frame for each field a lane moves.
        let Effect::Device(d) = &effects[inst.index] else { return Err("not a device".into()) };
        let m = &device.manifest;
        let mut fields: Map<String, Json> = m.fields.iter().map(|f| (f.name.clone(), f.default_json())).collect();
        for (name, value) in &d.fields {
            fields.insert(name.clone(), match value {
                FieldValue::Number(x) => json!(x),
                FieldValue::Flag(b) => json!(b),
                FieldValue::Text(s) => json!(s),
            });
        }
        let mut lane_files = Map::new();
        if let Some(who) = owner_enum(p, &inst.owner) {
            let mut values = vec![0.0f64; frames];
            for lane in lanes {
                let Ok(t) = aaw_model::rules::target(who, &lane.param) else { continue };
                let aaw_model::rules::TargetKind::Effect { index: i, .. } = t.kind else { continue };
                if i != inst.index {
                    continue;
                }
                let name = t.field.strip_prefix("fields.").unwrap_or(&t.field).to_string();
                let Some(spec) = m.field(&name).filter(|s| s.lane && s.kind.numeric()) else { continue };
                let domain = if spec.log { aaw_model::rules::Domain::Log } else { aaw_model::rules::Domain::Linear };
                let env = aaw_dsp::envelope::Envelope::new(lane, domain, p.session.tempo, rate);
                env.fill(0, 1, &mut values);
                let bytes: Vec<u8> = values.iter().flat_map(|v| (v.clamp(spec.min, spec.max) as f32).to_le_bytes()).collect();
                let lane_file = work.join(format!("{name}.f32"));
                std::fs::write(&lane_file, bytes).map_err(|e| format!("{}: {e}", lane_file.display()))?;
                lane_files.insert(name, json!(lane_file));
            }
        }
        let params = json!({
            "sample_rate": rate,
            "tempo": p.session.tempo,
            "frames": frames,
            "fields": fields,
            "lanes": lane_files,
        });
        let params_file = work.join("params.json");
        std::fs::write(&params_file, params.to_string()).map_err(|e| e.to_string())?;
        let output_file = work.join("output.wav");
        let mut command = runner()?;
        command
            .arg("--device")
            .arg(&device.dir)
            .arg("--input")
            .arg(&input_file)
            .arg("--output")
            .arg(&output_file)
            .arg("--params")
            .arg(&params_file);
        run_python(command, &format!("{} on {}", m.slug, inst.reference))?;
        let audio = aaw_engine::sndfile::read(&output_file)?;
        if audio.rate as i64 != rate {
            return Err(format!("{}: the device wrote audio at {} Hz; the song is at {rate}", m.slug, audio.rate));
        }
        if audio.channels == 0 || audio.channels > 2 {
            return Err(format!("{}: the device wrote {} channels; one or two are played", m.slug, audio.channels));
        }
        let frames_out = to_stereo(&audio);
        if frames_out.is_empty() {
            return Err(format!("{}: the device wrote no audio", m.slug));
        }
        if !frames_out.iter().all(|f| f[0].is_finite() && f[1].is_finite()) {
            return Err(format!("{}: the device wrote samples that are not numbers", m.slug));
        }
        std::fs::create_dir_all(cache_dir(project)).map_err(|e| e.to_string())?;
        aaw_engine::wav::write_atomic(&file, &aaw_engine::wav::float_wav_bytes(&frames_out, rate as u32))?;
        Ok(Arc::new(frames_out))
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Writes the lanes on a device's fields as raw files of one float a frame
/// under `work`, by field: `lanes` are the lanes by the field each moves.
fn lane_files(p: &Project, m: &Manifest, lanes: &[(String, &aaw_model::Lane)], frames: usize, work: &Path) -> Result<Map<String, Json>> {
    let mut out = Map::new();
    let mut values = vec![0.0f64; frames];
    for (name, lane) in lanes {
        let Some(spec) = m.field(name).filter(|s| s.lane && s.kind.numeric()) else { continue };
        let domain = if spec.log { aaw_model::rules::Domain::Log } else { aaw_model::rules::Domain::Linear };
        let env = aaw_dsp::envelope::Envelope::new(lane, domain, p.session.tempo, p.session.sample_rate);
        env.fill(0, 1, &mut values);
        let bytes: Vec<u8> = values.iter().flat_map(|v| (v.clamp(spec.min, spec.max) as f32).to_le_bytes()).collect();
        let lane_file = work.join(format!("{name}.f32"));
        std::fs::write(&lane_file, bytes).map_err(|e| format!("{}: {e}", lane_file.display()))?;
        out.insert(name.clone(), json!(lane_file));
    }
    Ok(out)
}

/// The fields as the code is given them: the song's values over the
/// manifest's defaults.
fn field_values(m: &Manifest, d: &aaw_model::Device) -> Map<String, Json> {
    let mut fields: Map<String, Json> = m.fields.iter().map(|f| (f.name.clone(), f.default_json())).collect();
    for (name, value) in &d.fields {
        fields.insert(name.clone(), match value {
            FieldValue::Number(x) => json!(x),
            FieldValue::Flag(b) => json!(b),
            FieldValue::Text(s) => json!(s),
        });
    }
    fields
}

/// Runs the device's code with `params` and reads what it wrote as the
/// song's frames, checked; `output` is where it writes.
fn run_and_read(command: Command, what: &str, slug_of: &str, output: &Path, rate: i64) -> Result<Vec<[f32; 2]>> {
    run_python(command, what)?;
    let audio = aaw_engine::sndfile::read(output)?;
    if audio.rate as i64 != rate {
        return Err(format!("{slug_of}: the device wrote audio at {} Hz; the song is at {rate}", audio.rate));
    }
    if audio.channels == 0 || audio.channels > 2 {
        return Err(format!("{slug_of}: the device wrote {} channels; one or two are played", audio.channels));
    }
    let frames_out = to_stereo(&audio);
    if frames_out.is_empty() {
        return Err(format!("{slug_of}: the device wrote no audio"));
    }
    if !frames_out.iter().all(|f| f[0].is_finite() && f[1].is_finite()) {
        return Err(format!("{slug_of}: the device wrote samples that are not numbers"));
    }
    Ok(frames_out)
}

/// Makes an offline instrument's cache: hands its code the track's notes as
/// they play, in frames, with its fields and the lanes on them, and keeps
/// what it plays under `.daw/devices/`.
fn compute_instrument(p: &Project, project: &Path, inst: &Instance, device: &Installed, identity: &str) -> Result<Arc<Vec<[f32; 2]>>> {
    let file = cache_dir(project).join(format!("{identity}.wav"));
    let t = p.track(inst.owner.strip_prefix("tracks.").unwrap_or_default()).ok_or("the track")?;
    let midi = t.midi.as_ref().ok_or("the track's notes")?;
    let d = midi.device().ok_or("the track's instrument")?;
    check_instrument(t, d, device)?;
    let rate = p.session.sample_rate;
    let frames = total_frames(p);
    if frames == 0 {
        return Err("The song has no frames".into());
    }
    let m = &device.manifest;
    let work = cache_dir(project).join(format!("work-{}-{}", std::process::id(), &identity[..12]));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let result = (|| -> Result<Arc<Vec<[f32; 2]>>> {
        let lanes: Vec<(String, &aaw_model::Lane)> = instrument_lanes(t)
            .into_iter()
            .map(|l| (l.param.strip_prefix("instrument.fields.").unwrap_or(&l.param).to_string(), l))
            .collect();
        let notes: Vec<Json> = aaw_model::schedule::track_notes(p, midi).iter().map(|n| json!([n.start, n.end, n.pitch, n.velocity])).collect();
        let params = json!({
            "sample_rate": rate,
            "tempo": p.session.tempo,
            "frames": frames,
            "fields": field_values(m, d),
            "lanes": lane_files(p, m, &lanes, frames, &work)?,
            "notes": notes,
        });
        let params_file = work.join("params.json");
        std::fs::write(&params_file, params.to_string()).map_err(|e| e.to_string())?;
        let output_file = work.join("output.wav");
        let mut command = runner()?;
        command.arg("--device").arg(&device.dir).arg("--output").arg(&output_file).arg("--params").arg(&params_file);
        let frames_out = run_and_read(command, &format!("{} on {}", m.slug, inst.reference), &m.slug, &output_file, rate)?;
        std::fs::create_dir_all(cache_dir(project)).map_err(|e| e.to_string())?;
        aaw_engine::wav::write_atomic(&file, &aaw_engine::wav::float_wav_bytes(&frames_out, rate as u32))?;
        Ok(Arc::new(frames_out))
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Removes cache files that no device of the song is made from.
pub fn sweep(project: &Path, instances: &[Instance]) {
    let dir = cache_dir(project);
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    let keep: Vec<String> = instances.iter().filter_map(|i| i.identity.as_ref().map(|id| format!("{id}.wav"))).collect();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".wav") && !keep.contains(&name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// What a device in a song is doing.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// Its cache is made and plays.
    Ready,
    /// Its code is running.
    Computing,
    /// Its turn has not come.
    Waiting,
    /// Its code failed, with the reason; the input passes through.
    Failed(String),
    /// It is not installed, with the reason; the input passes through.
    Missing(String),
    /// Bypassed, or on a frozen track: nothing to make.
    Off,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub reference: String,
    pub device: String,
    pub state: State,
}

impl Status {
    pub fn json(&self) -> Json {
        let (state, reason) = match &self.state {
            State::Ready => ("ready", None),
            State::Computing => ("computing", None),
            State::Waiting => ("waiting", None),
            State::Failed(e) => ("failed", Some(e.clone())),
            State::Missing(e) => ("missing", Some(e.clone())),
            State::Off => ("off", None),
        };
        let mut m = Map::new();
        m.insert("effect".into(), json!(self.reference));
        m.insert("device".into(), json!(self.device));
        m.insert("state".into(), json!(state));
        if let Some(r) = reason {
            m.insert("error".into(), json!(r));
        }
        Json::Object(m)
    }
}

/// The caches of a song's devices, made now where they are not there yet.
#[derive(Debug)]
pub struct Settled {
    pub caches: Caches,
    pub status: Vec<Status>,
}

/// Makes every device's cache in order, each with the caches of those
/// before it, and returns them with each device's state. A device that
/// fails stops nothing else; `strict` makes its failure the error.
pub fn settle(p: &Project, project: &Path, strict: bool) -> Result<Settled> {
    let mut caches = Caches::new();
    let mut status = Vec::new();
    for inst in instances(p, project) {
        let state = match (&inst.installed, inst.heard) {
            (Err(e), _) => State::Missing(e.clone()),
            (Ok(_), false) => State::Off,
            (Ok(device), true) if device.realtime() => match device.code(p.session.sample_rate as f64, p.session.tempo) {
                Ok(code) => {
                    caches.insert(inst.key(), Made::Code(code));
                    State::Ready
                }
                Err(e) => {
                    if strict {
                        return Err(format!("{}: {e}", inst.reference));
                    }
                    State::Failed(e)
                }
            },
            (Ok(_), true) => match compute(p, project, &inst, &caches) {
                Ok(audio) => {
                    caches.insert(inst.key(), Made::Audio(audio));
                    State::Ready
                }
                Err(e) => {
                    if strict {
                        return Err(format!("{}: {e}", inst.reference));
                    }
                    State::Failed(e)
                }
            },
        };
        status.push(Status {
            reference: inst.reference.clone(),
            device: inst.slug.clone(),
            state,
        });
    }
    Ok(Settled { caches, status })
}

/// The state of a song's devices without a host: an offline device ready
/// where a cache file is there and waiting otherwise, a real-time device
/// ready where its code runs.
pub fn status_headless(p: &Project, project: &Path) -> Vec<Status> {
    instances(p, project)
        .iter()
        .map(|inst| Status {
            reference: inst.reference.clone(),
            device: inst.slug.clone(),
            state: match (&inst.installed, inst.heard) {
                (Err(e), _) => State::Missing(e.clone()),
                (Ok(_), false) => State::Off,
                (Ok(device), true) if device.realtime() => match device.code(p.session.sample_rate as f64, p.session.tempo) {
                    Ok(_) => State::Ready,
                    Err(e) => State::Failed(e),
                },
                (Ok(_), true) => match inst.cache_file(project) {
                    Some(f) if f.is_file() => State::Ready,
                    _ => State::Waiting,
                },
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_is_read_and_refused_in_words() {
        let m = parse_manifest(&MANIFEST_TEMPLATE.replace("NAME", "Scaffold"), "scaffold").unwrap();
        assert_eq!(m.name, "Scaffold");
        assert_eq!(m.slug, "scaffold");
        assert_eq!(m.fields.len(), 5);
        let gain = m.field("gain_db").unwrap();
        assert_eq!((gain.kind, gain.min, gain.max, gain.lane, gain.log, gain.unit.as_str(), gain.label.as_str()), (FieldKind::Db, -24.0, 24.0, true, false, "dB", "Gain"));
        let tone = m.field("tone_hz").unwrap();
        assert!(tone.log && !tone.lane);
        let mode = m.field("mode").unwrap();
        assert_eq!((mode.kind, mode.choices.clone(), mode.lane), (FieldKind::Choice, vec!["stereo".to_string(), "mono".to_string()], false));
        assert_eq!(m.field("invert").unwrap().default, FieldValue::Flag(false));
        assert!(gain.accept(&FieldValue::Number(3.0)).is_ok());
        assert!(gain.accept(&FieldValue::Number(30.0)).unwrap_err().contains("-24 to 24"));
        assert!(mode.accept(&FieldValue::Text("wide".into())).unwrap_err().contains("stereo, mono"));
        assert!(m.field("invert").unwrap().accept(&FieldValue::Number(1.0)).unwrap_err().contains("true or false"));

        let refused = |text: &str| parse_manifest(text, "x").unwrap_err();
        assert!(refused("name: X\n").contains("format"));
        assert!(refused("format: 2\nname: X\n").contains("format 2"));
        assert!(refused("format: 1\n").contains("name is required"));
        assert!(refused("format: 1\nname: X\nkind: midi\n").contains("later stage"));
        assert_eq!(parse_manifest("format: 1\nname: X\nkind: instrument\n", "x").unwrap().kind, "instrument");
        assert!(refused("format: 1\nname: X\nrun: live\n").contains("offline or realtime"));
        assert!(refused("format: 1\nname: X\nrun: realtime\nentry: device.py\n").contains(".wasm file"));
        assert!(refused("format: 1\nname: X\nsource: device.rs\n").contains("real-time device"));
        assert!(refused("format: 1\nname: X\nrun: realtime\nsource: ../x.rs\n").contains(".rs file in the device's folder"));
        let rt = parse_manifest("format: 1\nname: X\nrun: realtime\nsource: device.rs\n", "x").unwrap();
        assert_eq!((rt.entry.as_str(), rt.source.as_deref()), ("device.wasm", Some("device.rs")));
        assert!(refused("format: 1\nname: X\ncolour: red\n").contains("colour"));
        assert!(refused("format: 1\nname: X\nfields: {Gain: {type: db}}\n").contains("field Gain"));
        assert!(refused("format: 1\nname: X\nfields: {gain: {type: loud}}\n").contains("type loud"));
        assert!(refused("format: 1\nname: X\nfields: {gain: {type: db, min: 0, max: 1}}\n").contains("default is required"));
        assert!(refused("format: 1\nname: X\nfields: {gain: {type: db, min: 1, max: 0, default: 0}}\n").contains("min is below max"));
        assert!(refused("format: 1\nname: X\nfields: {f: {type: hz, min: 0, max: 100, default: 10}}\n").contains("above 0"));
        assert!(refused("format: 1\nname: X\nfields: {m: {type: choice, choices: [a], default: a}}\n").contains("at least two"));
        assert!(refused("format: 1\nname: X\nfields: {m: {type: choice, choices: [a, b], default: c}}\n").contains("one of a, b"));
        assert!(refused("format: 1\nname: X\nfields: {s: {type: switch}}\n").contains("switch has a default"));
        assert!(parse_manifest("format: 1\nname: X\n", "Bad Slug").unwrap_err().contains("not a slug"));
        // A number alone, with its own unit, and a version as a number.
        let m = parse_manifest("format: 1\nname: X\nversion: 1.5\nfields: {ratio: {type: number, min: 1, max: 20, default: 4, unit: ':1', scale: log}}\n", "x").unwrap();
        assert_eq!(m.version, "1.5");
        let r = m.field("ratio").unwrap();
        assert!(r.log && r.lane && r.unit == ":1" && r.label == "Ratio");
    }

    #[test]
    fn a_device_is_found_in_the_project_before_the_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("ws");
        let proj = tmp.path().join("proj");
        let made = new("Tilt EQ", &ws.join("devices"), false, false).unwrap();
        assert_eq!(made.manifest.slug, "tilt-eq");
        assert!(made.dir.join("device.py").is_file());
        assert!(new("Tilt EQ", &ws.join("devices"), false, false).unwrap_err().contains("already there"));
        let own = new("Tilt EQ", &proj.join("devices"), false, false).unwrap();
        assert_eq!(own.from, Where::Project);
        assert_eq!(own.code, made.code, "the same files hash the same");
        std::fs::write(own.dir.join("device.py"), "def process(a, r, f):\n    return a\n").unwrap();
        let again = read(&own.dir, Where::Project).unwrap();
        assert_ne!(again.code, made.code, "a change of code is a change of hash");
        // Listing under a workspace: the project's shadows the workspace's.
        let listing = {
            let devices = |project: Option<&Path>| {
                let mut out = Listing::default();
                for (root, from) in [(project.map(project_dir), Where::Project), (Some(ws.join("devices")), Where::Workspace)] {
                    let Some(root) = root else { continue };
                    for folder in std::fs::read_dir(&root).into_iter().flatten().flatten().map(|e| e.path()) {
                        let name = folder.file_name().unwrap().to_string_lossy().into_owned();
                        if out.devices.iter().any(|d| d.manifest.slug == name) {
                            continue;
                        }
                        out.devices.push(read(&folder, from).unwrap());
                    }
                }
                out
            };
            devices(Some(&proj))
        };
        assert_eq!(listing.devices.len(), 1);
        assert_eq!(listing.devices[0].from, Where::Project);
        assert!(matches(&listing.devices[0], "tilt gain") && !matches(&listing.devices[0], "reverb"));
        // An effect node from the manifest, with a field given.
        let mut given = Map::new();
        given.insert("type".into(), json!("tilt-eq"));
        given.insert("gain_db".into(), json!(-3));
        let node = effect_node(Some(&proj), "tilt-eq", &given).unwrap();
        assert_eq!(node["type"], "device");
        assert_eq!(node["name"], "tilt-eq");
        assert_eq!(node["version"], "0.1");
        assert_eq!(node["fields"]["gain_db"], json!(-3));
        assert_eq!(node["fields"]["mode"], "stereo");
        given.insert("colour".into(), json!(1));
        assert!(effect_node(Some(&proj), "tilt-eq", &given).unwrap_err().contains("no field colour"));
        assert!(find(Some(&proj), "nothing").unwrap_err().contains("No device nothing"));
    }
}

// ---------------------------------------------------------------------------
// A host's work on a song's devices

/// What a host's `Work::update` or `poll` changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changed {
    /// The caches the next compile plays changed: the host compiles again.
    pub caches: bool,
    /// What the host says of its devices changed.
    pub status: bool,
}

struct Job {
    identity: String,
    rx: std::sync::mpsc::Receiver<Result<Arc<Vec<[f32; 2]>>>>,
}

/// A host's work on a song's devices of the person's own: the caches it has
/// loaded for the engine, one device's code running at a time off the
/// host's thread, in the order their inputs can be made, and what it says
/// of each device.
#[derive(Default)]
pub struct Work {
    pub plan: Vec<Instance>,
    /// The caches the engine plays, by owner and index.
    pub caches: Caches,
    /// The identity each loaded cache was made from.
    loaded: HashMap<(String, usize), String>,
    /// Why a cache could not be made, by identity, until something changes.
    failed: HashMap<String, String>,
    /// The real-time devices' code as loaded, by owner and index.
    codes: HashMap<(String, usize), Arc<wasm::Code>>,
    job: Option<Job>,
    pub status: Vec<Status>,
    /// The device folders the song uses, each with its files' latest
    /// change, and when they were last looked at.
    folders: Vec<(PathBuf, Option<std::time::SystemTime>)>,
    looked: Option<std::time::Instant>,
}

/// The latest change of a folder's files, as `code_hash` reads them.
fn folder_stamp(dir: &Path) -> Option<std::time::SystemTime> {
    let mut latest = std::fs::metadata(dir).and_then(|m| m.modified()).ok();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(d) = pending.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') || name == "__pycache__" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(t) = entry.metadata().and_then(|m| m.modified()) {
                latest = latest.max(Some(t));
            }
        }
    }
    latest
}

impl Work {
    /// Reads the song's devices anew: loads the caches that are there,
    /// drops those of devices that changed or went, and starts the next
    /// device whose cache is wanting when none is running.
    pub fn update(&mut self, p: &Arc<Project>, project: &Path) -> Changed {
        self.plan = instances(p, project);
        let mut dirs: Vec<PathBuf> = self.plan.iter().filter_map(|i| i.installed.as_ref().ok().map(|d| d.dir.clone())).collect();
        dirs.sort();
        dirs.dedup();
        self.folders = dirs.into_iter().map(|d| (d.clone(), folder_stamp(&d))).collect();
        self.looked = Some(std::time::Instant::now());
        self.refresh(p, project)
    }

    /// Whether a device folder the song uses changed since the song's
    /// devices were last read, as a build or an edit of the code changes
    /// it; looked at once a second at most.
    pub fn folders_changed(&mut self) -> bool {
        if self.folders.is_empty() || self.looked.is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(1)) {
            return false;
        }
        self.looked = Some(std::time::Instant::now());
        self.folders.iter().any(|(dir, stamp)| folder_stamp(dir) != *stamp)
    }

    /// Whether a real-time device failed while playing and the host has
    /// not said so yet.
    pub fn failing(&self) -> bool {
        self.plan.iter().zip(&self.status).any(|(inst, status)| {
            !matches!(status.state, State::Failed(_)) && self.codes.get(&inst.key()).is_some_and(|c| c.failure().is_some())
        })
    }

    /// Takes in a finished run, if one finished, and a real-time device's
    /// failure.
    pub fn poll(&mut self, p: &Arc<Project>, project: &Path) -> Changed {
        let Some(job) = &self.job else {
            return if self.failing() { self.refresh(p, project) } else { Changed::default() };
        };
        let result = match job.rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return Changed::default(),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("the device's run ended without a result".into()),
        };
        let identity = job.identity.clone();
        self.job = None;
        if let Err(e) = result {
            self.failed.insert(identity, e);
        }
        // A result lands by its file, which `refresh` loads where a device
        // is still made from it; a run of what the song no longer holds is
        // left on disk until the sweep.
        self.plan = instances(p, project);
        let changed = self.refresh(p, project);
        sweep(project, &self.plan);
        changed
    }

    /// Whether a device's code is running.
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    fn refresh(&mut self, p: &Arc<Project>, project: &Path) -> Changed {
        let before: Vec<((String, usize), String)> = self.loaded.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut status = Vec::new();
        let mut keep: Vec<(String, usize)> = Vec::new();
        let mut next: Option<Instance> = None;
        for inst in &self.plan {
            let key = inst.key();
            let state = match (&inst.installed, inst.heard, &inst.identity) {
                (Err(e), _, _) => State::Missing(e.clone()),
                // A real-time device's code is loaded here, a few
                // milliseconds' work, and made again when its folder or
                // the rate or the tempo change.
                (Ok(device), true, _) if device.realtime() => {
                    let want = format!("live:{}:{}:{}", device.code, p.session.sample_rate, p.session.tempo);
                    let loaded = self.loaded.get(&key) == Some(&want) && self.codes.contains_key(&key);
                    if !loaded && !self.failed.contains_key(&want) {
                        match device.code(p.session.sample_rate as f64, p.session.tempo) {
                            Ok(code) => {
                                self.caches.insert(key.clone(), Made::Code(code.clone()));
                                self.codes.insert(key.clone(), code);
                                self.loaded.insert(key.clone(), want.clone());
                            }
                            Err(e) => {
                                self.failed.insert(want.clone(), e);
                            }
                        }
                    }
                    match (self.failed.get(&want), self.codes.get(&key)) {
                        (Some(e), _) => State::Failed(e.clone()),
                        (None, Some(code)) => {
                            keep.push(key.clone());
                            code.failure().map_or(State::Ready, State::Failed)
                        }
                        (None, None) => State::Failed("the code was not loaded".into()),
                    }
                }
                (Ok(_), false, _) | (Ok(_), true, None) => State::Off,
                (Ok(_), true, Some(identity)) => {
                    if self.loaded.get(&key) == Some(identity) && self.caches.contains_key(&key) {
                        keep.push(key.clone());
                        State::Ready
                    } else if let Some(e) = self.failed.get(identity) {
                        State::Failed(e.clone())
                    } else if self.job.as_ref().is_some_and(|j| &j.identity == identity) {
                        State::Computing
                    } else {
                        let file = cache_dir(project).join(format!("{identity}.wav"));
                        match file.is_file().then(|| load_cache(&file)) {
                            Some(Ok(audio)) => {
                                self.caches.insert(key.clone(), Made::Audio(audio));
                                self.loaded.insert(key.clone(), identity.clone());
                                keep.push(key.clone());
                                State::Ready
                            }
                            Some(Err(e)) => {
                                self.failed.insert(identity.clone(), format!("the cache could not be read: {e}"));
                                State::Failed(e)
                            }
                            None => {
                                if next.is_none() && self.job.is_none() {
                                    next = Some(inst.clone());
                                }
                                State::Waiting
                            }
                        }
                    }
                }
            };
            status.push(Status {
                reference: inst.reference.clone(),
                device: inst.slug.clone(),
                state,
            });
        }
        self.caches.retain(|k, _| keep.contains(k));
        self.loaded.retain(|k, _| keep.contains(k));
        self.codes.retain(|k, _| keep.contains(k));
        if let Some(inst) = next {
            let identity = inst.identity.clone().expect("a waiting device has an identity");
            let (tx, rx) = std::sync::mpsc::channel();
            let (p, project, caches) = (p.clone(), project.to_path_buf(), self.caches.clone());
            std::thread::Builder::new()
                .name(format!("device {}", inst.slug))
                .spawn(move || {
                    let _ = tx.send(compute(&p, &project, &inst, &caches));
                })
                .expect("a thread for the device");
            self.job = Some(Job { identity, rx });
            if let Some(s) = status.iter_mut().find(|s| s.state == State::Waiting) {
                s.state = State::Computing;
            }
        }
        let mut after: Vec<((String, usize), String)> = self.loaded.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut before = before;
        before.sort();
        after.sort();
        let changed = Changed {
            caches: before != after,
            status: self.status != status,
        };
        self.status = status;
        changed
    }

    /// The devices' states, for `daw status`.
    pub fn status_json(&self) -> Json {
        Json::Array(self.status.iter().map(Status::json).collect())
    }
}
