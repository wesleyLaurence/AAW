//! The socket protocol: one JSON request line, one JSON reply line.
//!
//! Request: `{"command": {"op": ...}, "origin": "agent", "expect": SHA|null,
//! "gesture": ID|null}`. Reply: `{"ok": RESULT, "project": SONG}` or
//! `{"error": MESSAGE}`.

use crate::command::{Command, Origin};
use crate::registry;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub command: Command,
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<String>,
    /// Names the drag an edit belongs to. Edits of one gesture that land one
    /// after another are one undo step and one entry in the change log, and
    /// the host saves when they pause rather than after each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gesture: Option<String>,
}

impl Request {
    pub fn new(command: Command, origin: Origin) -> Request {
        Request {
            command,
            origin,
            expect: None,
            gesture: None,
        }
    }
}

/// A host's answer, and the project it answered for.
#[derive(Clone, Debug)]
pub struct Reply {
    pub result: Json,
    /// The song the host has open: not the one asked for when the project
    /// was saved under another name since.
    pub project: Option<PathBuf>,
    /// The path asked for, as the registry has it.
    pub asked: PathBuf,
}

impl Reply {
    /// Where the project is now, when that is not the path asked for.
    pub fn moved(&self) -> Option<&Path> {
        self.project.as_deref().filter(|p| *p != self.asked)
    }
}

fn exchange(mut stream: UnixStream, request: &Request, asked: PathBuf) -> Result<Reply, String> {
    // A first compile of a large song can take a while.
    stream
        .set_read_timeout(Some(Duration::from_secs(600)))
        .map_err(|e| e.to_string())?;
    let line = serde_json::to_string(request).map_err(|e| e.to_string())?;
    writeln!(stream, "{line}").map_err(|e| format!("Sending to the host: {e}"))?;
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|e| format!("Waiting for the host: {e}"))?;
    if reply.trim().is_empty() {
        return Err("The host closed the connection without replying".into());
    }
    let reply: Json = serde_json::from_str(&reply).map_err(|e| format!("Bad reply from the host: {e}"))?;
    match reply.get("error") {
        Some(e) => Err(e.as_str().unwrap_or_default().to_string()),
        None => Ok(Reply {
            result: reply.get("ok").cloned().unwrap_or(Json::Null),
            project: reply.get("project").and_then(Json::as_str).map(PathBuf::from),
            asked,
        }),
    }
}

/// Sends a request to the project's running host. `Ok(None)` when no host runs;
/// `Err` carries the host's error.
pub fn ask(project: &Path, request: &Request) -> Result<Option<Reply>, String> {
    let entry = registry::entry(project)?;
    match registry::connect(&entry) {
        Some(stream) => exchange(stream, request, entry.project).map(Some),
        // As a missing file was reported before a moved project could be found.
        None => match std::fs::metadata(project) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!("{}: {e}", project.display())),
            _ => Ok(None),
        },
    }
}

/// The result of `ask`, without where the project is.
pub fn send(project: &Path, request: &Request) -> Result<Option<Json>, String> {
    Ok(ask(project, request)?.map(|reply| reply.result))
}

/// Sends a request to the host listening on a socket, as the registry lists
/// it. `Ok(None)` when nothing answers there.
pub fn ask_socket(listed: &registry::Listed, request: &Request) -> Result<Option<Reply>, String> {
    match UnixStream::connect(&listed.socket) {
        Ok(stream) => exchange(stream, request, listed.project.clone()).map(Some),
        Err(_) => Ok(None),
    }
}
