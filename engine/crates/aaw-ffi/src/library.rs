//! The sample library, for the app's browser. The index and what is known of
//! each sample are Python's (`daw samples`), which this asks: a search of the
//! index, and the copy of a chosen file into a song's project. The song then
//! takes the copy as an edit like any other (`Edit::SamplerAdd`,
//! `Edit::SampleClip`).

use crate::SongError;
use aaw_host::python;
use serde_json::Value as Json;
use std::path::PathBuf;

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

/// The workspace library shared by every project and the agent.
#[uniffi::export]
pub fn library_path(_song: String) -> Option<String> {
    Some(std::env::var_os("AAW_LIBRARY").map(PathBuf::from)
        .unwrap_or_else(|| aaw_host::project::data_dir().join("library.sqlite"))
        .to_string_lossy().into_owned())
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LibraryFolder {
    pub path: String,
    pub available: bool,
}

/// Folder operations share the CLI's storage and never change the source audio.
#[uniffi::export]
pub fn library_folders(db: String, operation: String, path: Option<String>) -> Result<Vec<LibraryFolder>, SongError> {
    let mut args = vec!["--db", db.as_str(), "folders", operation.as_str()];
    if let Some(path) = &path { args.push(path); }
    let result = python::run("samples", &args)?;
    if matches!(operation.as_str(), "add" | "refresh") {
        // Surface unreadable files and unavailable folders instead of reporting success.
        let reports = if let Some(rows) = result.as_array() { rows.clone() } else { vec![result.clone()] };
        for report in reports {
            if let Some(error) = report.get("error").and_then(Json::as_str) {
                return Err(SongError::from(format!("{}: {error}", text(&report, "root"))));
            }
            if let Some(errors) = report.get("errors").and_then(Json::as_array).filter(|e| !e.is_empty()) {
                return Err(SongError::from(format!("Could not index {} audio files. {}: {}", errors.len(), text(&errors[0], "path"), text(&errors[0], "error"))));
            }
        }
    }
    let result = if matches!(operation.as_str(), "list" | "remove") { result } else { python::run("samples", &["--db", &db, "folders", "list"])? };
    Ok(result.as_array().into_iter().flatten().map(|row| LibraryFolder {
        path: text(row, "path"), available: row["available"].as_bool().unwrap_or(false)
    }).collect())
}

/// Device kinds supplied by the engine, used by the browser and Add Effect.
#[uniffi::export]
pub fn browser_effects() -> Vec<String> {
    aaw_model::EFFECT_TYPES.iter().map(|kind| kind.to_string()).collect()
}

/// A Synth patch as the browser lists it: a factory patch built into the
/// engine, or one saved in the workspace library.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PatchInfo {
    pub name: String,
    /// The file's stem, `soft-pad` for Soft Pad.
    pub slug: String,
    pub description: String,
    pub tags: Vec<String>,
    pub factory: bool,
    /// The file, for a saved patch.
    pub file: Option<String>,
}

/// The patches, factory first and then the person's by name, that have
/// every word of `query` in their name or a tag; all of them for an empty
/// query. What `daw patch list` lists. Reads the library's folder.
#[uniffi::export]
pub fn library_patches(query: String) -> Vec<PatchInfo> {
    aaw_host::patches::list()
        .patches
        .iter()
        .filter(|p| aaw_host::patches::matches(p, &query))
        .map(|p| PatchInfo {
            name: p.name.clone(),
            slug: p.slug.clone(),
            description: p.description.clone(),
            tags: p.tags.clone(),
            factory: p.factory,
            file: p.file.as_ref().map(|f| f.to_string_lossy().into_owned()),
        })
        .collect()
}

/// An effect rack as the browser lists it: a chain saved in the workspace
/// library.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RackInfo {
    pub name: String,
    /// The file's stem, `drum-glue` for Drum Glue.
    pub slug: String,
    pub description: String,
    pub tags: Vec<String>,
    /// The kinds of its effects, in order.
    pub kinds: Vec<String>,
    pub file: Option<String>,
}

/// The racks, by name, that have every word of `query` in their name, a tag
/// or an effect's kind; all of them for an empty query. What `daw rack list`
/// lists. Reads the library's folder.
#[uniffi::export]
pub fn library_racks(query: String) -> Vec<RackInfo> {
    aaw_host::racks::list()
        .racks
        .iter()
        .filter(|r| aaw_host::racks::matches(r, &query))
        .map(|r| RackInfo {
            name: r.name.clone(),
            slug: r.slug.clone(),
            description: r.description.clone(),
            tags: r.tags.clone(),
            kinds: r.kinds(),
            file: r.file.as_ref().map(|f| f.to_string_lossy().into_owned()),
        })
        .collect()
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
    library_search_folders(db, query, category, kind, limit, vec![])
}

#[uniffi::export]
pub fn library_search_folders(
    db: String, query: String, category: Option<String>, kind: Option<String>,
    limit: u32, folders: Vec<String>,
) -> Result<Vec<SampleInfo>, SongError> {
    let limit = limit.clamp(1, 1000).to_string();
    let mut args = vec!["--db", db.as_str(), "search", query.as_str(), "--limit", limit.as_str()];
    if let Some(category) = &category {
        args.extend(["--category", category.as_str()]);
    }
    if let Some(kind) = &kind {
        args.extend(["--type", kind.as_str()]);
    }
    for folder in &folders { args.extend(["--folder", folder.as_str()]); }
    let found = python::run("samples", &args)?;
    Ok(found.as_array().map(|rows| rows.iter().map(sample).collect()).unwrap_or_default())
}

/// The pitch `daw samples analyze` measures from the file at `path`, as a
/// note with its octave, such as `A3`; None when the file has no one pitch
/// it is sure of, as a drum or a chord has not. Blocks while Python runs,
/// so call it off the main thread.
#[uniffi::export]
pub fn library_pitch(db: String, path: String) -> Result<Option<String>, SongError> {
    let report = python::run("samples", &["--db", &db, "analyze", &path])?;
    let pitch = &report["pitch"];
    Ok(pitch["pitched"].as_bool().unwrap_or(false).then(|| optional(pitch, "note")).flatten())
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
