//! Patches: a Synth's sound as a YAML file outside any song, so that a sound
//! designed in one project is loaded into another. A patch is the `synth`
//! mapping a song holds, with a name, a description, tags and who saved it
//! and when around it. Saved patches are files in the workspace's library,
//! `~/Music/AAW/library/patches/` (or under `AAW_WORKSPACE`), one a patch,
//! named by the slug of the patch's name: `soft-pad.yaml` for Soft Pad.
//! Factory patches are built into the binary from `engine/patches/`, and a
//! saved patch of the same slug shadows the factory one.

use aaw_model::value::{Dict, Key, Value};
use aaw_model::{yaml_emit, yaml_load, Instrument, Synth};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, String>;

/// The factory patches, by slug, in the order the browser lists them.
pub const FACTORY: &[(&str, &str)] = &[
    ("init", include_str!("../../../patches/init.yaml")),
    ("sub-bass", include_str!("../../../patches/sub-bass.yaml")),
    ("reese", include_str!("../../../patches/reese.yaml")),
    ("supersaw", include_str!("../../../patches/supersaw.yaml")),
    ("pluck", include_str!("../../../patches/pluck.yaml")),
    ("soft-pad", include_str!("../../../patches/soft-pad.yaml")),
    ("bright-lead", include_str!("../../../patches/bright-lead.yaml")),
    ("organ", include_str!("../../../patches/organ.yaml")),
    ("bell", include_str!("../../../patches/bell.yaml")),
    ("kick", include_str!("../../../patches/kick.yaml")),
    ("hat", include_str!("../../../patches/hat.yaml")),
    ("riser", include_str!("../../../patches/riser.yaml")),
];

/// A patch as its file holds it.
#[derive(Clone, Debug)]
pub struct Patch {
    pub name: String,
    /// The file's stem: the slug of the name.
    pub slug: String,
    pub description: String,
    pub tags: Vec<String>,
    /// `user` or `agent`, for a saved patch.
    pub saved_by: Option<String>,
    /// When it was saved, as `2026-10-03T21:04:00Z`.
    pub saved_at: Option<String>,
    /// Built into `daw`, rather than a file of the person's.
    pub factory: bool,
    /// The file, for a saved patch.
    pub file: Option<PathBuf>,
    /// The `synth` mapping as the file holds it, fields at their defaults
    /// left out and without `patch`, which is the file's name.
    pub synth: Value,
}

/// The patches there are, and the files in the library that are not patches.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub patches: Vec<Patch>,
    /// A file that could not be read as a patch, and why.
    pub problems: Vec<String>,
}

/// The workspace: `AAW_WORKSPACE`, or `~/Music/AAW`.
pub fn workspace() -> PathBuf {
    if let Some(dir) = std::env::var_os("AAW_WORKSPACE") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join("Music/AAW")
}

/// Where saved patches are: `library/patches` in the workspace.
pub fn dir() -> PathBuf {
    workspace().join("library").join("patches")
}

/// A patch's file name from its name: lower-case letters, digits and `-`,
/// so Soft Pad is `soft-pad`. Empty for a name with nothing of that.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
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

/// Validates a `synth` mapping on its own, as the song validates it, and
/// gives it back as the song would save it: defaults left out.
pub fn validate(synth: &Value) -> Result<Synth> {
    let mut wrapped = Dict::new();
    wrapped.insert(key("synth"), synth.clone());
    match Instrument::parse(&Value::Dict(wrapped)) {
        Ok(Instrument::Synth(s)) => Ok(s),
        Ok(_) => Err("synth must be a mapping".into()),
        Err(e) => {
            let lines: Vec<String> = e
                .errors
                .iter()
                .map(|item| {
                    // The location inside the patch, without the wrapping.
                    let loc = item.loc_text();
                    let loc = loc.strip_prefix("Synth.").or_else(|| loc.strip_prefix("synth.")).unwrap_or(&loc);
                    if loc.is_empty() || loc == "Synth" || loc == "synth" {
                        item.msg.clone()
                    } else {
                        format!("{loc}: {}", item.msg)
                    }
                })
                .collect();
            Err(lines.join("; "))
        }
    }
}

/// Reads a patch from its file's text. `slug` is the file's stem, which
/// names a patch whose text gives no name.
pub fn parse(text_of_file: &str, slug_of_file: &str, factory: bool, file: Option<PathBuf>) -> Result<Patch> {
    let value = yaml_load::load(text_of_file)?;
    let Value::Dict(d) = &value else {
        return Err("a patch is a mapping with a synth in it".into());
    };
    for k in d.keys() {
        match k.as_str() {
            Some("name" | "description" | "tags" | "saved_by" | "saved_at" | "synth") => {}
            Some(other) => return Err(format!("{other} is not a field of a patch")),
            None => return Err("a patch's fields are named".into()),
        }
    }
    let mut synth = match d.get(&key("synth")) {
        Some(Value::Dict(s)) => s.clone(),
        _ => return Err("a patch has a synth mapping".into()),
    };
    // The name is the file's; a name written inside the mapping is dropped.
    synth.shift_remove(&key("patch"));
    let synth = Value::Dict(synth);
    let saved = validate(&synth)?.dump(true);
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
    Ok(Patch {
        name,
        slug: slug_of_file.to_string(),
        description: text(d, "description")?.unwrap_or_default(),
        tags: tags.into_iter().filter(|t| !t.is_empty()).collect(),
        saved_by: text(d, "saved_by")?,
        saved_at: text(d, "saved_at")?,
        factory,
        file,
        synth: saved,
    })
}

/// The factory patches, in their order.
pub fn factory() -> Vec<Patch> {
    FACTORY
        .iter()
        .map(|(slug, text)| parse(text, slug, true, None).unwrap_or_else(|e| panic!("factory patch {slug}: {e}")))
        .collect()
}

/// The saved patches, by name, and the files that are not patches.
pub fn saved() -> Listing {
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
        match std::fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|t| parse(&t, &stem, false, Some(file.clone()))) {
            Ok(p) => out.patches.push(p),
            Err(e) => out.problems.push(format!("{}: {e}", file.display())),
        }
    }
    out.patches.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// Every patch: the factory patches first, in their order, then the saved
/// ones by name. A saved patch shadows a factory patch of the same slug.
pub fn list() -> Listing {
    let mut mine = saved();
    let shadowed: Vec<String> = mine.patches.iter().map(|p| p.slug.clone()).collect();
    let mut patches: Vec<Patch> = factory().into_iter().filter(|p| !shadowed.contains(&p.slug)).collect();
    patches.append(&mut mine.patches);
    Listing { patches, problems: mine.problems }
}

/// Whether every word of `query` is in the patch's name or one of its tags.
pub fn matches(patch: &Patch, query: &str) -> bool {
    let name = patch.name.to_lowercase();
    query.split_whitespace().all(|word| {
        let word = word.to_lowercase();
        name.contains(&word) || patch.tags.iter().any(|t| t.to_lowercase().contains(&word))
    })
}

/// The patch a name means: a saved patch of that name or slug, else a
/// factory patch of it, else a `.yaml` file the name is the path of.
pub fn find(name: &str) -> Result<Patch> {
    let want = slug(name);
    if !want.is_empty() {
        let file = dir().join(format!("{want}.yaml"));
        if file.is_file() {
            let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            return parse(&text, &want, false, Some(file.clone())).map_err(|e| format!("{}: {e}", file.display()));
        }
        if let Some((slug, text)) = FACTORY.iter().find(|(s, _)| *s == want) {
            return parse(text, slug, true, None);
        }
    }
    let path = Path::new(name);
    if path.extension().is_some_and(|x| x == "yaml" || x == "yml") && path.is_file() {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let text = std::fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))?;
        return parse(&text, &slug(&stem), false, Some(path.to_path_buf())).map_err(|e| format!("{name}: {e}"));
    }
    Err(format!("No patch named {name}; daw patch list shows them"))
}

/// The `synth` mapping to put in a song: the patch's, led by its name.
pub fn loaded(patch: &Patch) -> Value {
    let mut d = Dict::new();
    d.insert(key("patch"), Value::str(&patch.name));
    if let Value::Dict(s) = &patch.synth {
        for (k, v) in s {
            d.insert(k.clone(), v.clone());
        }
    }
    Value::Dict(d)
}

/// The file's text for a patch.
pub fn text_of(patch: &Patch) -> String {
    let mut d = Dict::new();
    d.insert(key("name"), Value::str(&patch.name));
    if !patch.description.is_empty() {
        d.insert(key("description"), Value::str(&patch.description));
    }
    if !patch.tags.is_empty() {
        d.insert(key("tags"), Value::List(patch.tags.iter().map(|t| Value::str(t)).collect()));
    }
    if let Some(by) = &patch.saved_by {
        d.insert(key("saved_by"), Value::str(by));
    }
    if let Some(at) = &patch.saved_at {
        d.insert(key("saved_at"), Value::str(at));
    }
    d.insert(key("synth"), patch.synth.clone());
    yaml_emit::dump(&Value::Dict(d))
}

/// Writes a track's synth to the library as a patch named `name`, making
/// the library's folder when it is the first. A patch already saved under
/// that name is written over only with `replace`; a factory patch of the
/// name is shadowed without asking.
pub fn save(name: &str, description: &str, tags: &[String], synth: &Synth, by: &str, replace: bool) -> Result<Patch> {
    let name = name.trim();
    let slug_of = slug(name);
    if slug_of.is_empty() {
        return Err(format!("A patch's name has a letter or a digit in it; {name:?} has none"));
    }
    let dir = dir();
    let file = dir.join(format!("{slug_of}.yaml"));
    if file.exists() && !replace {
        return Err(format!("A patch named {name} is already saved at {}; --replace writes over it", file.display()));
    }
    let mut mapping = match synth.dump(true) {
        Value::Dict(d) => d,
        _ => unreachable!("a synth dumps as a mapping"),
    };
    mapping.shift_remove(&key("patch"));
    let patch = Patch {
        name: name.to_string(),
        slug: slug_of,
        description: description.trim().to_string(),
        tags: tags.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
        saved_by: Some(by.to_string()),
        saved_at: Some(timestamp()),
        factory: false,
        file: Some(file.clone()),
        synth: Value::Dict(mapping),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    aaw_model::atomic_write(&file, &text_of(&patch)).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(patch)
}

/// Now, as `2026-10-03T21:04:00Z`.
pub fn timestamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_factory_patch_reads_and_has_a_name_and_tags() {
        let all = factory();
        assert_eq!(all.len(), FACTORY.len());
        for p in &all {
            assert!(p.factory && p.file.is_none(), "{}", p.slug);
            assert_eq!(slug(&p.name), p.slug, "{} is not the slug of {}", p.slug, p.name);
            assert!(!p.description.is_empty() && !p.tags.is_empty(), "{}", p.slug);
            assert!(p.synth.get("patch").is_none());
            // Reading the file's text back gives the same patch.
            let again = parse(&text_of(p), &p.slug, true, None).unwrap();
            assert!(aaw_model::value::py_eq(&again.synth, &p.synth), "{}", p.slug);
        }
        let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names[..3], ["Init", "Sub Bass", "Reese"]);
    }

    #[test]
    fn a_slug_is_the_file_name_of_a_patch() {
        assert_eq!(slug("Soft Pad"), "soft-pad");
        assert_eq!(slug("  Round Sub (v2)! "), "round-sub-v2");
        assert_eq!(slug("Ünïcode Bell"), "n-code-bell");
        assert_eq!(slug("---"), "");
    }

    #[test]
    fn a_patch_is_validated_as_the_song_validates_a_synth() {
        let e = parse("name: X\nsynth: {oscillators: {a: {wave: sawtooth}}}\n", "x", false, None).unwrap_err();
        assert!(e.contains("oscillators.a.wave"), "{e}");
        let e = parse("name: X\nsynth: {oscillators: {a: {}}, modulation: [{source: env9, target: pitch, amount: 1}]}\n", "x", false, None).unwrap_err();
        assert!(e.contains("env9"), "{e}");
        let e = parse("name: X\nsynth: {oscillators: {a: {}}}\ncolour: red\n", "x", false, None).unwrap_err();
        assert!(e.contains("colour"), "{e}");
        let e = parse("name: X\n", "x", false, None).unwrap_err();
        assert!(e.contains("synth mapping"), "{e}");
        // A name inside the mapping is the file's, not the patch's.
        let p = parse("synth: {patch: Old, oscillators: {a: {}}}\n", "plain", false, None).unwrap();
        assert_eq!(p.name, "plain");
        assert!(p.synth.get("patch").is_none());
        let loaded = loaded(&p);
        assert!(matches!(loaded.get("patch"), Some(Value::Str(s)) if s == "plain"));
        assert!(matches!(&loaded, Value::Dict(d) if d.keys().next().and_then(Key::as_str) == Some("patch")));
    }

    #[test]
    fn a_query_matches_names_and_tags() {
        let p = find("Soft Pad").unwrap();
        assert!(matches(&p, "") && matches(&p, "soft") && matches(&p, "PAD chords") && !matches(&p, "soft bass"));
        assert!(find("no-such-patch").unwrap_err().contains("No patch named"));
    }

    #[test]
    fn the_timestamp_is_a_date_and_time() {
        let t = timestamp();
        assert_eq!(t.len(), 20, "{t}");
        assert!(t.starts_with("20") && t.ends_with('Z') && &t[10..11] == "T", "{t}");
    }
}
