//! The sample files audio clips play: each file's length, the identity of its
//! peaks and the beat map kept beside it, read once and kept while the file
//! and the map stay as they are.

use aaw_engine::sndfile;
use aaw_model::Sample;
use serde_json::Value as Json;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// A beat of a file's beat map.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct BeatMark {
    /// Where it is in the file.
    pub seconds: f64,
    /// Whether it starts a bar.
    pub downbeat: bool,
}

/// A file some audio clip plays.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FileView {
    /// What its clips and its peaks name it by: equal while the file is the same.
    pub identity: u64,
    pub seconds: f64,
    /// The tempo of its beat map, when `daw samples beats` has left one beside it.
    pub bpm: Option<f64>,
    /// The beats of that map, in order.
    pub beats: Vec<BeatMark>,
}

/// A file's size and when it was written: what says it is still the same file.
type Stamp = (u64, Option<SystemTime>);

struct Known {
    stamp: Stamp,
    /// The stamp of the beat map beside it, or None when there is none.
    map: Option<Stamp>,
    /// The hash the song lists for the sample, which a map must be of.
    sha256: Option<String>,
    view: FileView,
}

/// What is known of each file, by its path.
#[derive(Default)]
pub struct Files {
    known: Mutex<HashMap<PathBuf, Known>>,
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

/// The beat map `daw samples beats` keeps beside a file: `NAME.beats.json`.
fn sidecar(path: &Path) -> PathBuf {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{stem}.beats.json"))
}

/// The tempo and beats of a map, when it is of the file the song lists.
fn beat_map(path: &Path, sha256: Option<&str>) -> (Option<f64>, Vec<BeatMark>) {
    let none = (None, Vec::new());
    let Ok(text) = std::fs::read_to_string(path) else { return none };
    let Ok(map) = serde_json::from_str::<Json>(&text) else { return none };
    // A map of other audio says nothing of this file.
    if let (Some(listed), Some(measured)) = (sha256, map.get("sha256").and_then(Json::as_str)) {
        if listed != measured {
            return none;
        }
    }
    let beats = map.get("beats").and_then(Json::as_array).map(|beats| {
        beats
            .iter()
            .filter_map(|b| {
                Some(BeatMark {
                    seconds: b.get("seconds")?.as_f64()?,
                    downbeat: b.get("beat").and_then(Json::as_i64) == Some(1),
                })
            })
            .collect()
    });
    let bpm = map.get("tempo").and_then(|t| t.get("bpm")).and_then(Json::as_f64);
    (bpm, beats.unwrap_or_default())
}

impl Files {
    /// The file a sample of the song in `directory` names, or None when it
    /// cannot be read.
    pub fn of(&self, directory: &Path, sample: &Sample) -> Option<FileView> {
        let path = directory.join(&sample.path);
        let now = stamp(&path)?;
        let beside = sidecar(&path);
        let map = stamp(&beside);
        let mut known = self.known.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(k) = known.get(&path) {
            if k.stamp == now && k.map == map && k.sha256 == sample.sha256 {
                return Some(k.view.clone());
            }
        }
        let info = sndfile::info(&path).ok()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (&path, &now).hash(&mut hasher);
        let (bpm, beats) = match map {
            Some(_) => beat_map(&beside, sample.sha256.as_deref()),
            None => (None, Vec::new()),
        };
        let view = FileView {
            // Zero is no file.
            identity: hasher.finish().max(1),
            seconds: info.seconds(),
            bpm,
            beats,
        };
        known.insert(
            path,
            Known {
                stamp: now,
                map,
                sha256: sample.sha256.clone(),
                view: view.clone(),
            },
        );
        Some(view)
    }
}
