//! The socket protocol: one JSON request line, one JSON reply line.
//!
//! Request: `{"command": {"op": ...}, "origin": "agent", "expect": SHA|null,
//! "gesture": ID|null}`. Reply: `{"ok": RESULT}` or `{"error": MESSAGE}`.

use crate::command::{Command, Origin};
use crate::registry;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
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

/// Sends a request to the project's running host. `Ok(None)` when no host runs;
/// `Err` carries the host's error.
pub fn send(project: &Path, request: &Request) -> Result<Option<Json>, String> {
    let entry = registry::entry(project)?;
    let Some(mut stream) = registry::connect(&entry) else {
        return Ok(None);
    };
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
        None => Ok(Some(reply.get("ok").cloned().unwrap_or(Json::Null))),
    }
}
