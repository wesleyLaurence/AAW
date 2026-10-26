//! A project as a folder: the song file in it, the `.aaw` file beside it
//! that names the project from outside, a blank project and the notes
//! written beside it, moving and copying one, and the Untitled projects the
//! app keeps in its data folder.
//!
//! The data folder is `~/Library/Application Support/AAW`, or `AAW_DATA_DIR`.
//! It holds `Untitled/`, a folder for each project that has no name yet, and
//! `projects.json`, the index of the projects the app knows, which the app
//! writes and `daw projects --all` reads.

use aaw_model::value::{dict, Key, Value};
use aaw_model::Project;
use serde_json::Value as Json;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

/// The song's file in a project's folder.
pub const SONG_FILE: &str = "song.yaml";
/// The extension of the project file beside the song, `My Beat/My Beat.aaw`:
/// the way into the project from outside the app, which the Finder opens
/// with it and every command takes as PROJECT. It names the format, the
/// project's ID and the song's file; the song stays in `song.yaml`.
pub const PROJECT_EXT: &str = "aaw";
/// The format a `.aaw` written now says. One that says a later number was
/// written by a newer AAW and is refused rather than guessed at.
pub const PROJECT_FORMAT: u64 = 1;
const PROJECT_COMMENT: &str = "# An AAW project. Open it with AAW; the song is in song.yaml beside it.";
/// The song's notes, which the person and the agent keep: what the song is
/// and is for, the person's instructions for it, where it is heading and what
/// was decided. A project starts with the four headings and nothing under them.
pub const NOTES_FILE: &str = "SONG.md";
pub const NOTES: &str = "## Vision\n\n## Instructions\n\n## Direction\n\n## Decisions\n";
/// What puts the notes in an agent's session: an agent that expands imports
/// has their text, and one that does not reads the first line.
pub const AGENT_FILE: &str = "AGENTS.md";
pub const AGENT_TEXT: &str = "Read SONG.md before working on this project, and keep it current.\n\n@SONG.md\n";
/// The files a project is made with beside its song, and what each says then.
const BESIDE: [(&str, &str); 2] = [(NOTES_FILE, NOTES), (AGENT_FILE, AGENT_TEXT)];
/// What a project made by the app starts with.
pub const UNTITLED_TEMPO: f64 = 120.0;

/// Files a host or the Finder leaves in a folder, which are not the project's:
/// the host's lock, the Finder's, and the folder's custom icon, which the app
/// sets so the Finder draws the project's folder with the app's icon.
const IGNORED: [&str; 3] = [".daw.lock", ".DS_Store", "Icon\r"];

pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("AAW_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join("Library/Application Support/AAW")
}

/// The song file a command's PROJECT names: the file itself, `song.yaml` in
/// a folder, or the song a `.aaw` names beside itself. A path that is gone,
/// as a project's is after it moved, is a folder unless it is written as a
/// YAML file, and a `.aaw` that is gone names its folder's `song.yaml`, which
/// a host may still answer for. A `.aaw` that does not read is refused with
/// what is wrong and the song file to use instead.
pub fn song_file(project: &Path) -> Result<PathBuf> {
    if is_project_file(project) {
        let folder = project.parent().unwrap_or(Path::new(""));
        if project.is_file() {
            return Ok(folder.join(read_project_file(project)?.song));
        }
        return Ok(folder.join(SONG_FILE));
    }
    let yaml = project
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"));
    Ok(if project.is_dir() || (!yaml && !project.exists()) {
        project.join(SONG_FILE)
    } else {
        project.to_path_buf()
    })
}

// MARK: The project file

/// What a project's `.aaw` says.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectFile {
    pub path: PathBuf,
    /// A random UUID written once, when the project was made; a copy gets
    /// its own. It is about the folder, which it tells apart after a move.
    pub id: String,
    /// The song's file name in the same folder: a bare name, never a path.
    pub song: String,
}

fn is_project_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(PROJECT_EXT))
}

/// A new project ID: a random UUID (version 4), in lower case.
pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the system gives random bytes");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let hex = hex.concat();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// The `.aaw` files at the top of a folder, by name.
pub fn project_files_in(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| is_project_file(p) && p.is_file()).collect();
    files.sort();
    files
}

/// A folder's `.aaw`: the one named for the folder, else the first by name.
pub fn project_file_in(folder: &Path) -> Option<PathBuf> {
    let files = project_files_in(folder);
    let named = named_project_file(folder);
    files.iter().find(|f| f.file_name() == named.file_name()).or(files.first()).cloned()
}

/// Where a folder's `.aaw` named for it is: `My Beat/My Beat.aaw`.
fn named_project_file(folder: &Path) -> PathBuf {
    let name = resolved(folder).ok().map(|f| name_of(&f)).filter(|n| !n.is_empty()).unwrap_or_else(|| name_of(folder));
    folder.join(format!("{name}.{PROJECT_EXT}"))
}

fn write_project_file(path: &Path, id: &str) -> Result<()> {
    let text = format!("{PROJECT_COMMENT}\nformat: {PROJECT_FORMAT}\nid: {id}\nsong: {SONG_FILE}\n");
    aaw_model::atomic_write(path, &text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads a `.aaw`. One that does not read says what is wrong and names the
/// song file to open instead.
pub fn read_project_file(path: &Path) -> Result<ProjectFile> {
    let instead = path.with_file_name(SONG_FILE);
    let refuse = |why: String| format!("{} {why}; open {} instead", path.display(), instead.display());
    let text = std::fs::read_to_string(path).map_err(|e| refuse(format!("does not read: {e}")))?;
    let value = aaw_model::yaml_load::load(&text).map_err(|e| refuse(format!("is not an AAW project file: {e}")))?;
    let Value::Dict(fields) = value else {
        return Err(refuse("is not an AAW project file: not a mapping".into()));
    };
    let field = |name: &str| fields.get(&Key::str(name));
    let string = |v: &Value| match v {
        Value::Str(s) => Some(s.clone()),
        _ => None,
    };
    let format = match field("format") {
        Some(Value::Int(n)) => u64::try_from(n).map_err(|_| refuse("has a format that is not a whole number".into()))?,
        Some(_) => return Err(refuse("has a format that is not a whole number".into())),
        None => return Err(refuse("is not an AAW project file: no format".into())),
    };
    if format > PROJECT_FORMAT {
        return Err(refuse(format!("is format {format}, written by a newer AAW than this one, which reads {PROJECT_FORMAT}")));
    }
    let id = match field("id").and_then(string) {
        Some(id) if !id.trim().is_empty() => id.trim().to_string(),
        _ => return Err(refuse("has no id".into())),
    };
    let song = match field("song").and_then(string) {
        Some(song) if !song.is_empty() && song != "." && song != ".." && !song.contains(['/', '\\']) => song,
        Some(_) => return Err(refuse("names a song that is not a file in its folder".into())),
        None => return Err(refuse("names no song".into())),
    };
    Ok(ProjectFile {
        path: path.to_path_buf(),
        id,
        song,
    })
}

/// A folder's `.aaw`, written named for the folder with a new ID when the
/// folder has none: a project made before the file existed gets one the
/// first time a host opens it.
pub fn ensure_project_file(folder: &Path) -> Result<ProjectFile> {
    if let Some(file) = project_file_in(folder) {
        return read_project_file(&file);
    }
    let path = named_project_file(folder);
    let id = new_id();
    write_project_file(&path, &id)?;
    Ok(ProjectFile {
        path,
        id,
        song: SONG_FILE.into(),
    })
}

/// A project's ID, as its folder's `.aaw` says; None without one that reads.
pub fn id_of(song: &Path) -> Option<String> {
    let folder = song.parent().filter(|f| !f.as_os_str().is_empty()).unwrap_or(Path::new("."));
    project_file_in(folder).and_then(|f| read_project_file(&f).ok()).map(|f| f.id)
}

/// Gives a project a new ID, in its `.aaw`, which is written when it has
/// none: for a folder duplicated in the Finder, so that the two are told
/// apart. Returns the ID.
pub fn renew_id(song: &Path) -> Result<String> {
    let folder = song.parent().filter(|f| !f.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let path = project_file_in(folder).unwrap_or_else(|| named_project_file(folder));
    let id = new_id();
    write_project_file(&path, &id)?;
    Ok(id)
}

/// Names a folder's `.aaw` for the folder, as after a move, keeping its ID
/// or with `renew` giving it a new one; a folder with none gets one. The
/// file keeps saying `song.yaml`, which is where the song is.
fn name_project_file(folder: &Path, renew: bool) -> Result<()> {
    let named = named_project_file(folder);
    let id = match project_file_in(folder) {
        Some(file) if file.file_name() == named.file_name() && !renew => return Ok(()),
        Some(file) => {
            let id = read_project_file(&file).map(|f| f.id).ok().filter(|_| !renew).unwrap_or_else(new_id);
            std::fs::remove_file(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            id
        }
        None => new_id(),
    };
    write_project_file(&named, &id)
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

/// An empty song in `meter` (4/4 when None) with everything else at its
/// default: eight bars long until something is put in it, since the length
/// is the song's content (D130, D163).
pub fn blank(tempo: f64, title: Option<&str>, meter: Option<&str>) -> Result<Project> {
    let meter = meter.map(aaw_model::Meter::parse).transpose()?.unwrap_or_default();
    let length = aaw_model::Beat::from_exact(&aaw_model::empty_length(&meter.bar()));
    let mut session = vec![("tempo", Value::Float(tempo)), ("length_beats", length.to_value())];
    if meter != aaw_model::Meter::COMMON {
        session.push(("time_signature", Value::Str(meter.text())));
    }
    if let Some(title) = title {
        session.insert(0, ("title", Value::Str(title.to_string())));
    }
    Project::validate(&dict(vec![("session", dict(session))])).map_err(|e| e.to_string())
}

/// Writes a blank project into a folder, which is made if it is not there,
/// and returns its song file. The notes and the file that imports them are
/// written beside the song, and the `.aaw` named for the folder with a new
/// ID; one that is already in the folder is left as it is.
pub fn create(directory: &Path, tempo: f64, title: Option<&str>, meter: Option<&str>) -> Result<PathBuf> {
    let path = directory.join(SONG_FILE);
    if path.exists() {
        return Err(format!("Project already exists: {}", path.display()));
    }
    let project = blank(tempo, title, meter)?;
    aaw_model::save(&project, &path).map_err(|e| e.to_string())?;
    for (name, text) in BESIDE {
        let file = directory.join(name);
        if file.symlink_metadata().is_err() {
            std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
        }
    }
    ensure_project_file(directory)?;
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
            Ok(()) => return create(&folder, UNTITLED_TEMPO, Some(&name), None),
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

/// Whether a folder has no file but the song, its `.aaw` and the files
/// written beside it, each as it was written.
fn only_song(folder: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    entries.flatten().all(|entry| {
        let name = entry.file_name();
        name == SONG_FILE
            || is_project_file(Path::new(&name))
            || IGNORED.iter().any(|i| name == *i)
            || BESIDE.iter().any(|(file, text)| name == *file && std::fs::read_to_string(entry.path()).is_ok_and(|t| t == *text))
    })
}

/// Whether a project holds nothing: its folder has only the song and its
/// notes with nothing written in them, and the song, given as its canonical
/// YAML, is the blank one the folder began with.
pub fn untouched(folder: &Path, yaml: &str) -> bool {
    only_song(folder)
        && blank(UNTITLED_TEMPO, Some(&name_of(folder)), None).is_ok_and(|b| aaw_model::to_yaml(&b) == yaml)
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

/// Copies a project's folder to `to`, which must not exist, and names the
/// copy's `.aaw` for it with a new ID. A copy that fails leaves nothing
/// behind.
pub fn copy_folder(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    copy_into(from, to)
        .map_err(|e| format!("Copying to {}: {e}", to.display()))
        .and_then(|()| name_project_file(to, true))
        .map_err(|e| {
            let _ = std::fs::remove_dir_all(to);
            e
        })
}

/// Moves a project's folder to `to`, which must not exist: a rename, or
/// across volumes a copy and then the removal of the original. The `.aaw`
/// is named for the new folder and keeps its ID.
pub fn move_folder(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    match std::fs::rename(from, to) {
        Ok(()) => name_project_file(to, false),
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            copy_into(from, to).map_err(|e| {
                let _ = std::fs::remove_dir_all(to);
                format!("Copying to {}: {e}", to.display())
            })?;
            // The project is at its new place; what is left of the old one is litter.
            let _ = std::fs::remove_dir_all(from);
            name_project_file(to, false)
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
    /// The project's ID as the app last read it from its `.aaw`.
    pub id: Option<String>,
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
                        id: row["id"].as_str().map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.opened.total_cmp(&a.opened));
    out
}
