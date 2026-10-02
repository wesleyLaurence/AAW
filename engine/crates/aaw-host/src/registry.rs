//! Which host owns which project. A running host binds a Unix socket named after
//! the project's canonical path, with a small JSON file beside it, in a per-user
//! directory: `~/Library/Application Support/AAW/hosts`, or `AAW_HOST_DIR`.
//! Sockets stay out of project directories because projects may be synced.
//!
//! Binding the socket is the claim: a second host for the same project finds it
//! in use and a live host answering. A socket nobody answers is stale and is
//! removed.
//!
//! A host whose project was saved under another name keeps the registration
//! of the path it had, so a command sent to that path still reaches it. Such
//! a registration says in its JSON file where the project is `now`.

use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::ErrorKind;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

pub fn root() -> PathBuf {
    if let Some(dir) = std::env::var_os("AAW_HOST_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join("Library/Application Support/AAW/hosts")
}

/// A project's registration paths.
#[derive(Clone, Debug)]
pub struct Entry {
    pub project: PathBuf,
    pub socket: PathBuf,
    pub info: PathBuf,
}

/// The registration of a project by its path. A path that is gone, as a
/// project's is after it moved, is taken as it is written, so that the host
/// that had it there can still be found.
pub fn entry(project: &Path) -> Result<Entry> {
    let canonical = crate::project::resolved(project)?;
    let digest = Sha256::digest(canonical.as_os_str().as_encoded_bytes());
    let key: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    let dir = root();
    Ok(Entry {
        project: canonical,
        socket: dir.join(format!("{key}.sock")),
        info: dir.join(format!("{key}.json")),
    })
}

fn remove(e: &Entry) {
    let _ = std::fs::remove_file(&e.socket);
    let _ = std::fs::remove_file(&e.info);
}

/// A connection to the live host of a project, if there is one. A socket that
/// refuses connections twice, 50 ms apart, belongs to a host that exited without
/// unregistering and is removed. (A host binds before it listens, so a single
/// refusal may be a host starting.)
pub fn connect(e: &Entry) -> Option<UnixStream> {
    for attempt in 0..2 {
        match UnixStream::connect(&e.socket) {
            Ok(s) => return Some(s),
            Err(err) if err.kind() == ErrorKind::ConnectionRefused => {
                if attempt == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                } else {
                    remove(e);
                }
            }
            Err(_) => return None,
        }
    }
    None
}

/// This process's claim on a project. Dropping it unregisters.
pub struct Claim {
    entry: Entry,
}

impl Claim {
    pub fn entry(&self) -> &Entry {
        &self.entry
    }

    /// Says where the project is now, after it was saved under another name.
    pub fn moved(&self, now: &Path) -> Result<()> {
        write_info(&self.entry, Some(now))
    }
}

fn write_info(e: &Entry, now: Option<&Path>) -> Result<()> {
    let mut info = json!({
        "project": e.project,
        "socket": e.socket,
        "pid": std::process::id(),
    });
    if let Some(now) = now {
        info["now"] = json!(now);
    }
    std::fs::write(&e.info, format!("{info}\n")).map_err(|err| err.to_string())
}

impl Drop for Claim {
    fn drop(&mut self) {
        remove(&self.entry);
    }
}

fn info(path: &Path) -> Option<serde_json::Value> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// The host that owns a project, from its registration.
pub fn owner(e: &Entry) -> String {
    info(&e.info)
        .and_then(|v| v["pid"].as_u64())
        .map_or_else(|| "another host".to_string(), |pid| format!("the host with pid {pid}"))
}

/// Why a project that has a live host cannot be claimed.
fn taken(e: &Entry) -> String {
    match info(&e.info).and_then(|v| v["now"].as_str().map(str::to_string)) {
        Some(now) => format!(
            "{} was saved as {now}, which is open in {} and answers for it; close that project first",
            e.project.display(),
            owner(e)
        ),
        None => format!("{} is already open in {}", e.project.display(), owner(e)),
    }
}

/// Where a project is now, when it was saved under another name since and
/// the host that has it there still answers at this path.
pub fn moved_to(project: &Path) -> Option<PathBuf> {
    let e = entry(project).ok()?;
    let now = info(&e.info)?["now"].as_str().map(PathBuf::from)?;
    connect(&e).map(|_| now)
}

/// A registration as its JSON file has it.
#[derive(Clone, Debug)]
pub struct Listed {
    /// The path the registration is for.
    pub project: PathBuf,
    pub socket: PathBuf,
    pub pid: Option<u64>,
    /// Where the project is, when it was saved under another name since.
    pub now: Option<PathBuf>,
}

/// Every registration, whether or not its host still answers.
pub fn list() -> Vec<Listed> {
    let Ok(entries) = std::fs::read_dir(root()) else {
        return Vec::new();
    };
    let mut out: Vec<Listed> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let v = info(&e.path())?;
            Some(Listed {
                project: PathBuf::from(v["project"].as_str()?),
                socket: PathBuf::from(v["socket"].as_str()?),
                pid: v["pid"].as_u64(),
                now: v["now"].as_str().map(PathBuf::from),
            })
        })
        .collect();
    out.sort_by(|a, b| a.project.cmp(&b.project));
    out
}

/// Unix socket paths are limited to 104 bytes on macOS.
const SOCKET_PATH_LIMIT: usize = 103;

/// Registers this process as the project's host and returns its listener.
pub fn claim(e: &Entry) -> Result<(UnixListener, Claim)> {
    let dir = root();
    if e.socket.as_os_str().len() > SOCKET_PATH_LIMIT {
        return Err(format!(
            "{}: the socket path is too long; set AAW_HOST_DIR to a shorter directory",
            e.socket.display()
        ));
    }
    std::fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let listener = match UnixListener::bind(&e.socket) {
        Ok(l) => l,
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            if connect(e).is_some() {
                return Err(taken(e));
            }
            UnixListener::bind(&e.socket).map_err(|err| format!("{}: {err}", e.socket.display()))?
        }
        Err(err) => return Err(format!("{}: {err}", e.socket.display())),
    };
    write_info(e, None)?;
    Ok((listener, Claim { entry: e.clone() }))
}
