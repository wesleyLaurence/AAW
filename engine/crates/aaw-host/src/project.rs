//! A project as a folder: the song file in it, a blank project, moving and
//! copying one, and the Untitled projects the app keeps in its data folder.
//!
//! The data folder is `~/Library/Application Support/AAW`, or `AAW_DATA_DIR`.
//! It holds `Untitled/`, a folder for each project that has no name yet, and
//! `projects.json`, the index of the projects the app knows, which the app
//! writes and `daw projects --all` reads.

use aaw_model::value::{dict, Value};
use aaw_model::Project;
use serde_json::Value as Json;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

/// The song's file in a project's folder.
pub const SONG_FILE: &str = "song.yaml";
/// What a project made by the app starts with.
pub const UNTITLED_TEMPO: f64 = 120.0;
pub const UNTITLED_BARS: i64 = 32;

/// Files a host or the Finder leaves in a folder, which are not the project's.
const IGNORED: [&str; 2] = [".daw.lock", ".DS_Store"];

pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("AAW_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join("Library/Application Support/AAW")
}

/// The song file a command's PROJECT names: the file itself, or `song.yaml`
/// in a folder. A path that is gone, as a project's is after it moved, is a
/// folder unless it is written as a YAML file.
pub fn song_file(project: &Path) -> PathBuf {
    let yaml = project
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"));
    if project.is_dir() || (!yaml && !project.exists()) {
        project.join(SONG_FILE)
    } else {
        project.to_path_buf()
    }
}

/// How a project is named to a person or an agent: its folder, or its file
/// when that is not `song.yaml`.
pub fn shown(song: &Path) -> PathBuf {
    match (song.file_name(), song.parent()) {
        (Some(name), Some(folder)) if name == SONG_FILE && !folder.as_os_str().is_empty() => folder.to_path_buf(),
        _ => song.to_path_buf(),
    }
}

/// A path with its folders resolved, for a file that may not exist: the
/// canonical path of the nearest folder that does, and the rest as written.
pub fn resolved(path: &Path) -> Result<PathBuf> {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return Ok(canonical);
    }
    let absolute = std::path::absolute(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut rest: Vec<Component> = Vec::new();
    let mut base = absolute.as_path();
    loop {
        if let Ok(canonical) = std::fs::canonicalize(base) {
            let mut out = canonical;
            for part in rest.iter().rev() {
                match part {
                    Component::ParentDir => {
                        out.pop();
                    }
                    Component::CurDir => {}
                    other => out.push(other),
                }
            }
            return Ok(out);
        }
        match (base.components().next_back(), base.parent()) {
            (Some(last), Some(parent)) => {
                rest.push(last);
                base = parent;
            }
            _ => return Ok(absolute),
        }
    }
}

/// A song of so many 4/4 bars with everything else at its default.
pub fn blank(tempo: f64, bars: i64, title: Option<&str>) -> Result<Project> {
    if bars <= 0 {
        return Err("bars must be positive".into());
    }
    let length = bars.checked_mul(4).ok_or("bars is too large")?;
    let mut session = vec![("tempo", Value::Float(tempo)), ("length_beats", Value::int(length))];
    if let Some(title) = title {
        session.insert(0, ("title", Value::Str(title.to_string())));
    }
    Project::validate(&dict(vec![("session", dict(session))])).map_err(|e| e.to_string())
}

/// Writes a blank project into a folder, which is made if it is not there,
/// and returns its song file.
pub fn create(directory: &Path, tempo: f64, bars: i64, title: Option<&str>) -> Result<PathBuf> {
    let path = directory.join(SONG_FILE);
    if path.exists() {
        return Err(format!("Project already exists: {}", path.display()));
    }
    let project = blank(tempo, bars, title)?;
    aaw_model::save(&project, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

fn name_of(folder: &Path) -> String {
    folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

// MARK: Untitled projects

pub fn untitled_root() -> PathBuf {
    data_dir().join("Untitled")
}

/// Makes a blank project named Untitled, or Untitled 2 and so on with the
/// lowest number not in use, in the data folder, and returns its song file.
pub fn create_untitled() -> Result<PathBuf> {
    let root = untitled_root();
    std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    for n in 1.. {
        let name = if n == 1 { "Untitled".to_string() } else { format!("Untitled {n}") };
        let folder = root.join(&name);
        // Making the folder is the claim on the name.
        match std::fs::create_dir(&folder) {
            Ok(()) => return create(&folder, UNTITLED_TEMPO, UNTITLED_BARS, Some(&name)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{}: {e}", folder.display())),
        }
    }
    unreachable!("a number is free")
}

/// Whether a song is one of the data folder's Untitled projects.
pub fn is_untitled(song: &Path) -> bool {
    let (Ok(song), Ok(root)) = (resolved(song), resolved(&untitled_root())) else {
        return false;
    };
    song.parent().and_then(Path::parent) == Some(root.as_path())
}

/// Whether a folder has no file but the song.
fn only_song(folder: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    entries.flatten().all(|entry| {
        let name = entry.file_name();
        name == SONG_FILE || IGNORED.iter().any(|i| name == *i)
    })
}

/// Whether a project holds nothing: its folder has only the song, and the
/// song, given as its canonical YAML, is the blank one the folder began with.
pub fn untouched(folder: &Path, yaml: &str) -> bool {
    only_song(folder)
        && blank(UNTITLED_TEMPO, UNTITLED_BARS, Some(&name_of(folder))).is_ok_and(|b| aaw_model::to_yaml(&b) == yaml)
}

/// Deletes an Untitled project's folder. Refuses any other folder.
pub fn delete_untitled(song: &Path) -> Result<()> {
    if !is_untitled(song) {
        return Err(format!("{} is not an Untitled project", song.display()));
    }
    let folder = song.parent().expect("a song in the Untitled folder has a folder");
    std::fs::remove_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))
}

/// The Untitled projects left in the data folder, as their song files, after
/// deleting those that hold nothing. `open` says whether a host has a song
/// open, and those are left alone and left out.
pub fn sweep_untitled(open: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(untitled_root()) else {
        return Vec::new();
    };
    let mut left = Vec::new();
    for entry in entries.flatten() {
        let folder = entry.path();
        let song = folder.join(SONG_FILE);
        if !folder.is_dir() || open(&song) {
            continue;
        }
        let holds_nothing = if song.exists() {
            aaw_model::load(&song, false).is_ok_and(|p| untouched(&folder, &aaw_model::to_yaml(&p)))
        } else {
            // A folder without a song is a project that was never made.
            only_song(&folder)
        };
        if holds_nothing {
            let _ = std::fs::remove_dir_all(&folder);
        } else {
            left.push(song);
        }
    }
    left.sort();
    left
}

// MARK: Moving and copying

/// The folder a project is to be saved as: an absolute path where nothing is
/// yet, outside the project itself.
pub fn target(to: &Path, from: &Path) -> Result<PathBuf> {
    if !to.is_absolute() {
        return Err(format!("{} is not an absolute path", to.display()));
    }
    let to = resolved(to)?;
    if name_of(&to).is_empty() {
        return Err(format!("{} has no name", to.display()));
    }
    if to.symlink_metadata().is_ok() {
        return Err(format!("{} is already there; choose another name", to.display()));
    }
    if to.starts_with(resolved(from)?) {
        return Err(format!("{} is inside the project", to.display()));
    }
    Ok(to)
}

fn copy_into(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (source, name) = (entry.path(), entry.file_name());
        if name == ".daw.lock" {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_into(&source, &to.join(&name))?;
        } else if kind.is_symlink() {
            std::os::unix::fs::symlink(std::fs::read_link(&source)?, to.join(&name))?;
        } else {
            std::fs::copy(&source, to.join(&name))?;
        }
    }
    Ok(())
}

/// Copies a project's folder to `to`, which must not exist. A copy that
/// fails leaves nothing behind.
pub fn copy_folder(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    copy_into(from, to).map_err(|e| {
        let _ = std::fs::remove_dir_all(to);
        format!("Copying to {}: {e}", to.display())
    })
}

/// Moves a project's folder to `to`, which must not exist: a rename, or
/// across volumes a copy and then the removal of the original.
pub fn move_folder(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            copy_folder(from, to)?;
            // The project is at its new place; what is left of the old one is litter.
            let _ = std::fs::remove_dir_all(from);
            Ok(())
        }
        Err(e) => Err(format!("Moving to {}: {e}", to.display())),
    }
}

// MARK: The index

/// A project in the app's index.
#[derive(Clone, Debug, PartialEq)]
pub struct Known {
    /// The song file, where the app last saw it.
    pub song: PathBuf,
    pub title: String,
    pub untitled: bool,
    /// Unix time in seconds.
    pub opened: f64,
}

pub fn index_file() -> PathBuf {
    data_dir().join("projects.json")
}

/// The projects in the app's index, most recently opened first. Empty when
/// the app has not written one.
pub fn known() -> Vec<Known> {
    let Some(index) = std::fs::read_to_string(index_file()).ok().and_then(|t| serde_json::from_str::<Json>(&t).ok()) else {
        return Vec::new();
    };
    let mut out: Vec<Known> = index["projects"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    Some(Known {
                        song: PathBuf::from(row["path"].as_str()?),
                        title: row["title"].as_str().unwrap_or_default().to_string(),
                        untitled: row["untitled"].as_bool().unwrap_or(false),
                        opened: row["opened"].as_f64().unwrap_or(0.0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.opened.total_cmp(&a.opened));
    out
}
