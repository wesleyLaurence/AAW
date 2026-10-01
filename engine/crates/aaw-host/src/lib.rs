//! The session host: the in-memory authority for a song. It applies edit
//! commands one at a time, keeps undo history and a change log, saves
//! `song.yaml` after each change, reloads external edits, and plays the song
//! with edits heard as they land. The `daw` CLI reaches a running host through
//! a per-user socket, or runs a session headless when no host owns the project.

pub mod client;
pub mod command;
pub mod host;
pub mod python;
pub mod registry;
pub mod session;
pub mod tree;
