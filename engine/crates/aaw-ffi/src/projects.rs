//! Projects, for the app: the song file a path names, a new Untitled project,
//! and the ones a crash left in the data folder. The index of the projects
//! the app knows is the app's own, kept in the same folder.

use crate::SongError;
use aaw_host::{project, registry};
use std::path::Path;

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The song file a path names: the file itself, or `song.yaml` in a folder.
#[uniffi::export]
pub fn project_song_file(path: String) -> String {
    text(&project::song_file(Path::new(&path)))
}

/// The app's data folder, which holds the Untitled projects and the index:
/// `~/Library/Application Support/AAW`, or what `AAW_DATA_DIR` names.
#[uniffi::export]
pub fn project_data_dir() -> String {
    text(&project::data_dir())
}

/// Makes a blank project in the data folder, named Untitled or Untitled N
/// with the lowest number not in use, and returns its song file.
#[uniffi::export]
pub fn project_new_untitled() -> Result<String, SongError> {
    Ok(text(&project::create_untitled()?))
}

/// Whether a song is one of the data folder's Untitled projects.
#[uniffi::export]
pub fn project_is_untitled(path: String) -> bool {
    project::is_untitled(Path::new(&path))
}

/// Deletes an Untitled project's folder, once its song is closed. Refuses a
/// project that is anywhere else.
#[uniffi::export]
pub fn project_delete_untitled(path: String) -> Result<(), SongError> {
    Ok(project::delete_untitled(Path::new(&path))?)
}

/// The Untitled projects left in the data folder that hold something, as
/// their song files, after deleting the ones that hold nothing. Projects a
/// host has open, such as another copy of the app, are left alone.
#[uniffi::export]
pub fn project_sweep_untitled() -> Vec<String> {
    let open = |song: &Path| registry::entry(song).is_ok_and(|entry| registry::connect(&entry).is_some());
    project::sweep_untitled(open).iter().map(|song| text(song)).collect()
}
