//! Favorites: what the person keeps at hand, as one YAML file in the
//! workspace's library, `~/Music/AAW/library/favorites.yaml` (or under
//! `AAW_WORKSPACE`), beside the patches, racks and instruments. A favorite
//! is a sample by its file, a patch, a rack or an instrument rack by its
//! name, or a device kind: an instrument (`sampler`, `synth`), an audio
//! effect kind or a MIDI effect kind. The file lists them in the order they
//! were added, which is the order the browser's Favorites shows within each
//! of its groups. The file holds what was named; a favorite whose patch,
//! rack, file or kind is gone is listed as missing until it is removed.

use crate::patches::{slug, workspace};
use aaw_model::value::{Dict, Key, Value};
use aaw_model::{yaml_emit, yaml_load, EFFECT_TYPES, MIDI_EFFECT_TYPES};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

/// The instruments the browser offers by kind.
pub const INSTRUMENT_KINDS: [&str; 2] = ["sampler", "synth"];

/// What a favorite is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A sample, by its file's path.
    Sample,
    /// A Synth patch, by name.
    Patch,
    /// An effect rack, by name.
    Rack,
    /// An instrument rack, by name.
    InstrumentRack,
    /// A device kind: `sampler`, `synth`, an audio effect kind or a MIDI
    /// effect kind.
    Device,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Sample, Kind::Patch, Kind::Rack, Kind::InstrumentRack, Kind::Device];

    /// The word the file and `daw favorite list` use.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Sample => "sample",
            Kind::Patch => "patch",
            Kind::Rack => "rack",
            Kind::InstrumentRack => "instrument_rack",
            Kind::Device => "device",
        }
    }

    /// The kind a word names, `instrument-rack` and `instrument_rack` alike.
    pub fn parse(word: &str) -> Result<Kind> {
        match word.trim().to_lowercase().replace('-', "_").as_str() {
            "sample" => Ok(Kind::Sample),
            "patch" => Ok(Kind::Patch),
            "rack" => Ok(Kind::Rack),
            "instrument_rack" | "instrument" => Ok(Kind::InstrumentRack),
            "device" | "effect" => Ok(Kind::Device),
            other => Err(format!("{other:?} is not a kind of favorite; one of sample, patch, rack, instrument-rack or device")),
        }
    }

    /// The field that names a favorite of this kind in the file.
    fn field(self) -> &'static str {
        match self {
            Kind::Sample => "path",
            Kind::Device => "type",
            _ => "name",
        }
    }
}

/// One favorite as the file holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Favorite {
    pub kind: Kind,
    /// A sample's path, a patch's, a rack's or an instrument rack's name,
    /// or a device's kind.
    pub what: String,
}

impl Favorite {
    /// What two favorites are the same by: a path as it is, a name by its
    /// slug, a kind as it is.
    fn key(&self) -> (Kind, String) {
        let k = match self.kind {
            Kind::Sample | Kind::Device => self.what.clone(),
            _ => slug(&self.what),
        };
        (self.kind, k)
    }

    fn is(&self, kind: Kind, what: &str) -> bool {
        self.key() == Favorite { kind, what: what.to_string() }.key()
    }
}

/// A favorite with what the library has of it now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub favorite: Favorite,
    /// The name as the library has it now, or as the file has it when the
    /// thing is missing; a sample's file name.
    pub name: String,
    /// A patch's, a rack's or an instrument rack's slug; a device's kind.
    pub slug: String,
    /// The patch, rack, file or kind is no longer there.
    pub missing: bool,
}

/// The file: `library/favorites.yaml` in the workspace.
pub fn file() -> PathBuf {
    workspace().join("library").join("favorites.yaml")
}

fn key(name: &str) -> Key {
    Key::str(name)
}

/// Reads the file's text. An empty text has no favorites.
pub fn parse(text: &str) -> Result<Vec<Favorite>> {
    let value = yaml_load::load(text)?;
    let d = match &value {
        Value::None => return Ok(Vec::new()),
        Value::Dict(d) => d,
        _ => return Err("the file is a mapping with favorites in it".into()),
    };
    for k in d.keys() {
        match k.as_str() {
            Some("favorites") => {}
            Some(other) => return Err(format!("{other} is not a field of the favorites file")),
            None => return Err("the file's fields are named".into()),
        }
    }
    let items = match d.get(&key("favorites")) {
        None | Some(Value::None) => return Ok(Vec::new()),
        Some(Value::List(items)) => items,
        Some(_) => return Err("favorites is a list".into()),
    };
    let mut out: Vec<Favorite> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let Value::Dict(m) = item else {
            return Err(format!("favorites[{i}] is a mapping with a kind"));
        };
        let kind = match m.get(&key("kind")) {
            Some(Value::Str(s)) => Kind::parse(s).map_err(|e| format!("favorites[{i}]: {e}"))?,
            _ => return Err(format!("favorites[{i}] has a kind")),
        };
        let field = kind.field();
        let what = match m.get(&key(field)) {
            Some(Value::Str(s)) if !s.trim().is_empty() => s.trim().to_string(),
            _ => return Err(format!("favorites[{i}] is a {} with a {field}", kind.as_str())),
        };
        for k in m.keys() {
            match k.as_str() {
                Some("kind") => {}
                Some(f) if f == field => {}
                Some(other) => return Err(format!("favorites[{i}]: {other} is not a field of a {}", kind.as_str())),
                None => return Err(format!("favorites[{i}]: the fields are named")),
            }
        }
        let f = Favorite { kind, what };
        if !out.iter().any(|have| have.key() == f.key()) {
            out.push(f);
        }
    }
    Ok(out)
}

/// The favorites, in the order they were added. No file is no favorites.
pub fn read() -> Result<Vec<Favorite>> {
    let file = file();
    match std::fs::read_to_string(&file) {
        Ok(text) => parse(&text).map_err(|e| format!("{}: {e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// The file's text for the favorites.
pub fn text_of(favorites: &[Favorite]) -> String {
    let items = favorites
        .iter()
        .map(|f| {
            let mut m = Dict::new();
            m.insert(key("kind"), Value::str(f.kind.as_str()));
            m.insert(key(f.kind.field()), Value::str(&f.what));
            Value::Dict(m)
        })
        .collect();
    let mut d = Dict::new();
    d.insert(key("favorites"), Value::List(items));
    yaml_emit::dump(&Value::Dict(d))
}

fn write(favorites: &[Favorite]) -> Result<()> {
    let file = file();
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    aaw_model::atomic_write(&file, &text_of(favorites)).map_err(|e| format!("{}: {e}", file.display()))
}

/// Whether `kind` is a device the browser offers: an instrument, an audio
/// effect kind or a MIDI effect kind.
pub fn is_device_kind(kind: &str) -> bool {
    INSTRUMENT_KINDS.contains(&kind) || EFFECT_TYPES.contains(&kind) || MIDI_EFFECT_TYPES.contains(&kind)
}

/// What `what` names as a favorite of `kind`, checked against the library:
/// a sample's file as its full path, a patch's, a rack's or an instrument
/// rack's name as the library has it, a device kind the browser offers.
pub fn resolve(kind: Kind, what: &str) -> Result<Favorite> {
    let what = what.trim();
    let what = match kind {
        Kind::Sample => {
            let path = Path::new(what);
            if !path.is_file() {
                return Err(format!("No file at {what}; a favorite sample is a file's path, as daw samples search prints it"));
            }
            std::fs::canonicalize(path).map_err(|e| format!("{what}: {e}"))?.to_string_lossy().into_owned()
        }
        Kind::Patch => crate::patches::find(what)?.name,
        Kind::Rack => crate::racks::find(what)?.name,
        Kind::InstrumentRack => crate::instruments::find(what)?.name,
        Kind::Device => {
            let k = what.to_lowercase();
            if !is_device_kind(&k) {
                return Err(format!(
                    "{what} is not a device kind; one of {}, an effect kind ({}) or a MIDI effect kind ({})",
                    INSTRUMENT_KINDS.join(", "),
                    EFFECT_TYPES.join(", "),
                    MIDI_EFFECT_TYPES.join(", ")
                ));
            }
            k
        }
    };
    Ok(Favorite { kind, what })
}

/// Adds a favorite at the end of the file, making the file when it is the
/// first. One already there is left where it is, and `false` says so.
pub fn add(kind: Kind, what: &str) -> Result<(Vec<Favorite>, Favorite, bool)> {
    let favorite = resolve(kind, what)?;
    let mut all = read()?;
    if let Some(have) = all.iter().find(|f| f.key() == favorite.key()) {
        return Ok((all.clone(), have.clone(), false));
    }
    all.push(favorite.clone());
    write(&all)?;
    Ok((all, favorite, true))
}

/// Takes a favorite out of the file; one not there is refused. `what` is
/// taken as the file has it, so a sample whose file is gone can still be
/// removed; a path that is a file is matched by its full path too.
pub fn remove(kind: Kind, what: &str) -> Result<(Vec<Favorite>, Favorite)> {
    let mut all = read()?;
    let what = what.trim();
    let mut keys = vec![what.to_string()];
    if kind == Kind::Sample {
        if let Ok(full) = std::fs::canonicalize(what) {
            keys.push(full.to_string_lossy().into_owned());
        }
    }
    if kind == Kind::Device {
        keys.push(what.to_lowercase());
    }
    let Some(at) = all.iter().position(|f| keys.iter().any(|k| f.is(kind, k))) else {
        return Err(format!("{what} is not a favorite {}; daw favorite list shows them", kind.as_str().replace('_', " ")));
    };
    let removed = all.remove(at);
    write(&all)?;
    Ok((all, removed))
}

/// Whether `what` is a favorite of `kind` now.
pub fn has(favorites: &[Favorite], kind: Kind, what: &str) -> bool {
    favorites.iter().any(|f| f.is(kind, what))
}

/// Each favorite with what the library has of it now.
pub fn listed(favorites: &[Favorite]) -> Vec<Listed> {
    let patches = crate::patches::list().patches;
    let racks = crate::racks::list().racks;
    let instruments = crate::instruments::list().racks;
    favorites
        .iter()
        .map(|f| {
            let (name, slug_of, missing) = match f.kind {
                Kind::Sample => {
                    let path = Path::new(&f.what);
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| f.what.clone());
                    (name, String::new(), !path.is_file())
                }
                Kind::Patch => {
                    let s = slug(&f.what);
                    match patches.iter().find(|p| p.slug == s) {
                        Some(p) => (p.name.clone(), s, false),
                        None => (f.what.clone(), s, true),
                    }
                }
                Kind::Rack => {
                    let s = slug(&f.what);
                    match racks.iter().find(|r| r.slug == s) {
                        Some(r) => (r.name.clone(), s, false),
                        None => (f.what.clone(), s, true),
                    }
                }
                Kind::InstrumentRack => {
                    let s = slug(&f.what);
                    match instruments.iter().find(|r| r.slug == s) {
                        Some(r) => (r.name.clone(), s, false),
                        None => (f.what.clone(), s, true),
                    }
                }
                Kind::Device => (f.what.clone(), f.what.clone(), !is_device_kind(&f.what)),
            };
            Listed { favorite: f.clone(), name, slug: slug_of, missing }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_is_read_and_written_in_order_without_doubles() {
        let text = "favorites:\n- {kind: patch, name: Soft Pad}\n- {kind: sample, path: /tmp/kick.wav}\n- {kind: device, type: compressor}\n- {kind: patch, name: soft-pad}\n- {kind: instrument-rack, name: Warm Bass}\n";
        let favs = parse(text).unwrap();
        assert_eq!(favs.len(), 4);
        assert_eq!(favs[0], Favorite { kind: Kind::Patch, what: "Soft Pad".into() });
        assert_eq!(favs[3].kind, Kind::InstrumentRack);
        let again = parse(&text_of(&favs)).unwrap();
        assert_eq!(again, favs);
        assert!(text_of(&favs).contains("kind: instrument_rack"));
        assert!(parse("").unwrap().is_empty());
        assert!(parse("favorites: []\n").unwrap().is_empty());
        assert!(has(&favs, Kind::Patch, "SOFT PAD") && !has(&favs, Kind::Rack, "Soft Pad"));
    }

    #[test]
    fn a_bad_file_says_what_is_wrong() {
        assert!(parse("favorites:\n- {kind: colour, name: x}\n").unwrap_err().contains("not a kind"));
        assert!(parse("favorites:\n- {kind: patch}\n").unwrap_err().contains("with a name"));
        assert!(parse("favorites:\n- {kind: sample, path: /a, name: b}\n").unwrap_err().contains("not a field"));
        assert!(parse("stars: []\n").unwrap_err().contains("not a field"));
    }

    #[test]
    fn kinds_have_words() {
        for k in Kind::ALL {
            assert_eq!(Kind::parse(k.as_str()).unwrap(), k);
        }
        assert_eq!(Kind::parse("instrument-rack").unwrap(), Kind::InstrumentRack);
        assert!(Kind::parse("song").is_err());
        assert!(is_device_kind("synth") && is_device_kind("arpeggiator") && is_device_kind("rack") && !is_device_kind("piano"));
    }
}
