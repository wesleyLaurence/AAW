//! A host over its socket: commands from several clients at once, one owner per
//! project, stale registrations and closing. No audio device is opened.

mod common;

use aaw_host::client::{self, Request};
use aaw_host::command::{Command, Origin};
use aaw_host::host::{self, Options};
use aaw_host::registry;
use common::{cmd, write_song};
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Socket paths are short, so the registry goes in a short directory.
fn registry_dir() -> &'static Path {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::Builder::new().prefix("aaw").tempdir_in("/tmp").unwrap();
        // SAFETY: set once, before any test reads it; every test calls this first.
        unsafe { std::env::set_var("AAW_HOST_DIR", dir.path()) };
        dir
    })
    .path()
}

fn request(project: &Path, j: Json, origin: Origin) -> Result<Option<Json>, String> {
    client::send(
        project,
        &Request {
            command: cmd(j),
            origin,
            expect: None,
        },
    )
}

struct Running {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<Json, String>>>,
    _dir: tempfile::TempDir,
}

impl Running {
    fn start() -> Running {
        registry_dir();
        let dir = tempfile::tempdir().unwrap();
        let path = write_song(dir.path());
        let stop = Arc::new(AtomicBool::new(false));
        let (p, s) = (path.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            host::run(
                &p,
                Options {
                    buffer: 128,
                    play: None,
                    exit_on_stop: false,
                    seconds: None,
                    feed: false,
                },
                s,
            )
        });
        let started = Instant::now();
        while request(&path, json!({"op": "status"}), Origin::Agent).unwrap().is_none() {
            if thread.is_finished() {
                panic!("the host stopped: {:?}", thread.join().unwrap());
            }
            assert!(started.elapsed() < Duration::from_secs(10), "the host did not start");
            std::thread::sleep(Duration::from_millis(5));
        }
        Running {
            path,
            stop,
            thread: Some(thread),
            _dir: dir,
        }
    }

    fn send(&self, j: Json) -> Result<Json, String> {
        request(&self.path, j, Origin::Agent).map(|r| r.expect("host running"))
    }

    fn join(&mut self) -> Json {
        self.thread.take().unwrap().join().unwrap().unwrap()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[test]
fn the_host_applies_saves_undoes_and_logs() {
    let mut h = Running::start();
    let r = h.send(json!({"op": "set", "path": "tracks.drums.gain_db", "value": -6})).unwrap();
    assert_eq!(r["revision"], json!(1));
    // Saved before the reply.
    let text = std::fs::read_to_string(&h.path).unwrap();
    assert!(text.contains("gain_db: -6.0"), "{text}");
    let status = h.send(json!({"op": "status"})).unwrap();
    assert_eq!(status["undo"], json!({"label": "Set tracks.drums.gain_db: 0.0 → -6", "origin": "agent"}));
    assert_eq!(status["playing"], json!(false));
    request(&h.path, json!({"op": "undo"}), Origin::User).unwrap();
    assert!(!std::fs::read_to_string(&h.path).unwrap().contains("gain_db: -6.0"));
    let log = h.send(json!({"op": "changes", "since": 0})).unwrap();
    let rows: Vec<(String, String)> = log["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["op"].as_str().unwrap().into(), c["origin"].as_str().unwrap().into()))
        .collect();
    assert_eq!(rows, [("set".into(), "agent".into()), ("undo".into(), "user".into())]);
    // Handles appear in inspect and address clips.
    let inspect = h.send(json!({"op": "inspect"})).unwrap();
    let clip = inspect["tracks"][1]["clips"][2]["ref"].as_str().unwrap().to_string();
    assert!(clip.starts_with('@'));
    h.send(json!({"op": "clip.move", "clip": clip, "at": 24})).unwrap();
    assert_eq!(inspect["host"]["revision"], json!(2));
    // Errors come back as errors.
    let e = h.send(json!({"op": "set", "path": "session.tempo", "value": 0})).unwrap_err();
    assert!(e.contains("session.tempo"), "{e}");
    let closed = h.send(json!({"op": "close"})).unwrap();
    assert_eq!(closed, json!({"closed": true, "revision": 3}));
    let summary = h.join();
    assert_eq!(summary["revision"], json!(3));
    // Unregistered: commands now run headless.
    assert!(request(&h.path, json!({"op": "status"}), Origin::Agent).unwrap().is_none());
}

#[test]
fn concurrent_clients_all_land_in_order() {
    let h = Running::start();
    let path = Arc::new(h.path.clone());
    let threads: Vec<_> = (0..8)
        .map(|t| {
            let path = path.clone();
            std::thread::spawn(move || {
                let origin = if t % 2 == 0 { Origin::Agent } else { Origin::User };
                for i in 0..10 {
                    let (track, field) = if t < 4 { ("drums", "pan") } else { ("bass", "gain_db") };
                    let value = -((t * 10 + i + 1) as f64) / 100.0;
                    request(&path, json!({"op": "set", "path": format!("tracks.{track}.{field}"), "value": value}), origin)
                        .unwrap()
                        .unwrap();
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let log = h.send(json!({"op": "changes"})).unwrap();
    let revisions: Vec<u64> = log["changes"].as_array().unwrap().iter().map(|c| c["revision"].as_u64().unwrap()).collect();
    assert_eq!(revisions, (1..=80).collect::<Vec<_>>());
    // The file holds the last revision.
    let sha = h.send(json!({"op": "status"})).unwrap()["project_sha256"].clone();
    let saved = aaw_model::load(&h.path, false).unwrap();
    assert_eq!(json!(aaw_model::project_hash(&saved)), sha);
}

#[test]
fn one_host_per_project_and_stale_sockets_are_cleared() {
    let h = Running::start();
    let e = host::run(
        &h.path,
        Options {
            buffer: 128,
            play: None,
            exit_on_stop: false,
            seconds: None,
            feed: false,
        },
        Arc::new(AtomicBool::new(true)),
    )
    .unwrap_err();
    assert!(e.contains("is already open in the host with pid"), "{e}");
    drop(h);

    // A socket left by a host that died is removed on the next look.
    registry_dir();
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let entry = registry::entry(&path).unwrap();
    std::fs::create_dir_all(registry::root()).unwrap();
    drop(std::os::unix::net::UnixListener::bind(&entry.socket).unwrap());
    assert!(entry.socket.exists());
    assert!(request(&path, json!({"op": "status"}), Origin::Agent).unwrap().is_none());
    assert!(!entry.socket.exists());
}

#[test]
fn transport_settings_without_a_device() {
    let h = Running::start();
    let s = h.send(json!({"op": "locate", "at": "10/3"})).unwrap();
    assert_eq!(s["cue"], json!("10/3"));
    assert_eq!(s["position"]["frame"], json!(80000));
    let s = h.send(json!({"op": "loop", "start": 8, "length": 8})).unwrap();
    assert_eq!(s["loop"], json!({"start": 8, "length": 8}));
    assert!(h.send(json!({"op": "loop", "start": 30, "length": 8})).unwrap_err().contains("after the session end"));
    assert_eq!(h.send(json!({"op": "loop"})).unwrap()["loop"], Json::Null);
    assert!(h.send(json!({"op": "locate", "at": 40})).unwrap_err().contains("at or after the session end"));
    let _: Command = cmd(json!({"op": "stop"}));
    assert_eq!(h.send(json!({"op": "stop"})).unwrap()["playing"], json!(false));
}
