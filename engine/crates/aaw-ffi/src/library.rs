//! The sample library, for the app's browser. The index and what is known of
//! each sample are Python's (`daw samples`), which this asks: a search of the
//! index, and the copy of a chosen file into a song's project. The song then
//! takes the copy as an edit like any other (`Edit::SampleAdd`).

use crate::SongError;
use aaw_host::python;
use serde_json::Value as Json;
use std::path::{Path, PathBuf};

/// A sample in the library's index. Category, kind, tempo and key are read
/// from the file's name and are hints; the note is measured from its audio.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SampleInfo {
    pub id: String,
    pub name: String,
    pub pack: String,
    pub path: String,
    pub seconds: f64,
    pub channels: u32,
    pub category: String,
    /// `one-shot`, `loop` or `unknown`.
    pub kind: String,
    pub bpm: Option<u32>,
    pub key: Option<String>,
    /// The measured pitch of a sample that has one, such as `C2`.
    pub note: Option<String>,
    /// The root note to add it to a song with, so that events can play it at
    /// other pitches: its measured pitch, unless its name says it is a drum,
    /// an effect or a loop, which play as they are.
    pub root_note: Option<String>,
}

/// A sample file copied into a song's project, as the song's `samples` list it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Asset {
    /// Relative to the song's folder.
    pub path: String,
    pub sha256: String,
    /// The file it was copied or decoded from.
    pub source: String,
    /// The hash of that file, when the copy was decoded from it.
    pub source_sha256: Option<String>,
    pub root_note: Option<String>,
}

/// The categories a file's name can put it in, as the index names them.
pub const CATEGORIES: [&str; 12] =
    ["kick", "snare", "clap", "hat", "percussion", "808", "bass", "piano", "bell", "brass", "vocal", "fx"];

/// The categories whose samples play as they are, whatever pitch they measure at.
const UNPITCHED: [&str; 6] = ["kick", "snare", "clap", "hat", "percussion", "fx"];

fn text(j: &Json, key: &str) -> String {
    j.get(key).and_then(Json::as_str).unwrap_or_default().to_string()
}

fn optional(j: &Json, key: &str) -> Option<String> {
    j.get(key).and_then(Json::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

fn sample(j: &Json) -> SampleInfo {
    let measured = j.get("measured").filter(|m| m.get("pitched").and_then(Json::as_bool) == Some(true));
    let note = measured.and_then(|m| optional(m, "note"));
    let (category, kind) = (text(j, "category"), text(j, "kind"));
    SampleInfo {
        root_note: note.clone().filter(|_| kind != "loop" && !UNPITCHED.contains(&category.as_str())),
        id: text(j, "id"),
        name: text(j, "name"),
        pack: text(j, "pack"),
        path: text(j, "path"),
        seconds: j.get("duration").and_then(Json::as_f64).unwrap_or(0.0),
        channels: j.get("channels").and_then(Json::as_u64).unwrap_or(0) as u32,
        category,
        kind,
        bpm: j.get("bpm_hint").and_then(Json::as_u64).map(|b| b as u32),
        key: optional(j, "key_hint"),
        note,
    }
}

/// The library index a song's browser reads: AAW_LIBRARY, or
/// `.daw/library.sqlite` in the song's folder or the nearest folder above it
/// that has one, which is where `daw samples scan` writes it in a checkout.
#[uniffi::export]
pub fn library_path(song: String) -> Option<String> {
    if let Some(path) = std::env::var_os("AAW_LIBRARY") {
        return Some(PathBuf::from(path).to_string_lossy().into_owned());
    }
    let mut dir = Path::new(&song).parent();
    while let Some(d) = dir {
        let db = d.join(".daw/library.sqlite");
        if db.is_file() {
            return Some(db.to_string_lossy().into_owned());
        }
        dir = d.parent();
    }
    None
}

/// The categories the browser offers.
#[uniffi::export]
pub fn library_categories() -> Vec<String> {
    CATEGORIES.iter().map(|c| c.to_string()).collect()
}

/// Samples whose path has every word of `query`, by name: what `daw samples
/// search` finds. Blocks while Python runs, so call it off the main thread.
#[uniffi::export]
pub fn library_search(
    db: String,
    query: String,
    category: Option<String>,
    kind: Option<String>,
    limit: u32,
) -> Result<Vec<SampleInfo>, SongError> {
    let limit = limit.clamp(1, 1000).to_string();
    let mut args = vec!["--db", db.as_str(), "search", query.as_str(), "--limit", limit.as_str()];
    if let Some(category) = &category {
        args.extend(["--category", category.as_str()]);
    }
    if let Some(kind) = &kind {
        args.extend(["--type", kind.as_str()]);
    }
    let found = python::run("samples", &args)?;
    Ok(found.as_array().map(|rows| rows.iter().map(sample).collect()).unwrap_or_default())
}

/// Copies a sample file into the project of the song at `song`, leaving the
/// original as it is, and returns the copy as the song would list it. A
/// compressed file is decoded to WAV there. A file already copied or decoded
/// is not done again. Blocks while Python and a decoder run.
#[uniffi::export]
pub fn library_import(song: String, source: String, root_note: Option<String>) -> Result<Asset, SongError> {
    let mut args = vec!["import", source.as_str(), "--project", song.as_str(), "--copy-only"];
    if let Some(note) = &root_note {
        args.extend(["--root-note", note.as_str()]);
    }
    let copied = python::run("samples", &args)?;
    Ok(Asset {
        path: text(&copied, "path"),
        sha256: text(&copied, "sha256"),
        source: text(&copied, "source"),
        source_sha256: optional(&copied, "source_sha256"),
        root_note: optional(&copied, "root_note"),
    })
}
