//! A host over its socket: commands from several clients at once, one owner per
//! project, stale registrations and closing. No audio device is opened.

mod common;

use aaw_host::client::{self, Request};
use aaw_host::command::{Command, Origin};
use aaw_host::host::{self, Event, Options, TransportState};
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
    client::send(project, &Request::new(cmd(j), origin))
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
                    prepare: false,
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
fn a_note_preview_is_refused_before_the_output_opens_and_is_no_edit() {
    let h = Running::start();
    h.send(json!({"op": "synth.add", "track": "lead"})).unwrap();
    // The note is checked, and the track, before any audio device is touched.
    let e = h.send(json!({"op": "note.preview", "track": "bass", "pitch": "C3"})).unwrap_err();
    assert!(e.contains("has no synth"), "{e}");
    let e = h.send(json!({"op": "note.preview", "track": "nobody", "pitch": 60})).unwrap_err();
    assert!(e.contains("Unknown track"), "{e}");
    let e = h.send(json!({"op": "note.preview", "track": "lead", "pitch": 200})).unwrap_err();
    assert!(e.contains("outside 0 to 127"), "{e}");
    let e = h.send(json!({"op": "note.preview", "track": "lead", "pitch": "H9"})).unwrap_err();
    assert!(!e.is_empty());
    let e = h.send(json!({"op": "note.preview", "track": "lead", "pitch": 60, "velocity": 0})).unwrap_err();
    assert!(e.contains("velocity"), "{e}");
    let e = h.send(json!({"op": "note.preview", "track": "lead", "pitch": 60, "length_beats": 0})).unwrap_err();
    assert!(e.contains("length_beats"), "{e}");
    // Nothing of it reached the song or the history.
    let status = h.send(json!({"op": "status"})).unwrap();
    assert_eq!(status["revision"], json!(1));
    assert_eq!(status["undo"]["label"], json!("Add Synth track lead"));
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
            prepare: false,
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

/// What a host reports beside the events a test waits for: its warnings, and
/// each revision it compiled with the program's tracks.
#[derive(Default)]
struct Aside {
    warnings: Vec<String>,
    compiled: Vec<(u64, Vec<String>)>,
}

/// The next event that is not a warning or a compiled program, which are
/// collected.
fn next(events: &std::sync::mpsc::Receiver<Event>, aside: &mut Aside) -> Event {
    loop {
        match events.recv_timeout(Duration::from_secs(10)).expect("an event") {
            Event::Warning(w) => aside.warnings.push(w),
            Event::Compiled { revision, program } => {
                aside.compiled.push((revision, program.tracks.iter().map(|t| t.id.clone()).collect()));
            }
            other => return other,
        }
    }
}

#[test]
fn an_embedded_host_reports_what_changes() {
    registry_dir();
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let options = || Options {
        buffer: 128,
        play: None,
        exit_on_stop: false,
        seconds: None,
        feed: false,
        prepare: true,
    };
    let (tx, events) = std::sync::mpsc::channel();
    let running = host::spawn(&path, options(), move |e| {
        let _ = tx.send(e);
    })
    .unwrap();
    let mut warnings = Aside::default();
    let Event::Opened(doc) = next(&events, &mut warnings) else {
        panic!("the first event is the opened song")
    };
    assert_eq!(doc.project.tracks.len(), 2);
    let stopped = |cue: f64, region| {
        Event::Transport(TransportState {
            metronome: false,
            playing: false,
            cue,
            region,
        })
    };
    let transport = |e: Event| match e {
        Event::Transport(t) => t,
        _ => panic!("expected a transport event"),
    };
    assert_eq!(transport(next(&events, &mut warnings)), transport(stopped(0.0, None)));

    // An edit from another process arrives with its origin and the new song.
    request(&path, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -6}), Origin::Agent).unwrap();
    let Event::Changed { change, doc, history } = next(&events, &mut warnings) else {
        panic!("expected a change")
    };
    assert_eq!((change.revision, change.origin), (1, Origin::Agent));
    assert_eq!(change.label, "Set tracks.drums.gain_db: 0.0 → -6");
    assert_eq!(doc.project.tracks[0].gain_db, -6.0);
    assert_eq!((history.undo, history.redo), (Some((change.label.clone(), Origin::Agent)), None));
    // `prepare` compiled the song, effects and all, for the first play, and
    // each revision's program is reported once it is compiled.
    assert!(warnings.warnings.is_empty(), "{:?}", warnings.warnings);
    let status = request(&path, json!({"op": "status"}), Origin::Agent).unwrap().unwrap();
    assert_eq!(status["latency_frames"], json!(0));
    assert_eq!(status["playback_error"], Json::Null);

    // Requests made in-process are answered as over the socket.
    let ask = |j: Json| running.request(Request::new(cmd(j), Origin::User));
    assert_eq!(ask(json!({"op": "locate", "at": 8})).unwrap()["cue"], json!(8));
    assert_eq!(transport(next(&events, &mut warnings)), transport(stopped(8.0, None)));
    ask(json!({"op": "loop", "start": 8, "length": 4})).unwrap();
    assert_eq!(transport(next(&events, &mut warnings)), transport(stopped(8.0, Some((8.0, 4.0)))));
    assert!(ask(json!({"op": "locate", "at": 99})).unwrap_err().contains("session end"));
    // No output has been opened, so there is no playhead yet.
    assert!(running.clock().now().is_none());

    // The project has one host, and a song that does not load is refused.
    let e = host::spawn(&path, options(), |_| {}).err().unwrap();
    assert!(e.contains("is already open in the host with pid"), "{e}");
    assert!(host::spawn(&dir.path().join("missing.yaml"), options(), |_| {}).is_err());

    let summary = running.close().unwrap();
    assert_eq!(summary["revision"], json!(1));
    assert!(matches!(next(&events, &mut warnings), Event::Closed));
    let tracks = vec!["drums".to_string(), "bass".to_string()];
    assert_eq!(warnings.compiled, [(0, tracks.clone()), (1, tracks)]);
    assert!(request(&path, json!({"op": "status"}), Origin::Agent).unwrap().is_none());
}

#[test]
fn a_drag_is_saved_when_it_pauses() {
    let h = Running::start();
    let saved = || std::fs::read_to_string(&h.path).unwrap();
    let original = saved();
    let drag = |db: f64| {
        let request = Request {
            gesture: Some("drag-1".into()),
            ..Request::new(cmd(json!({"op": "set", "path": "tracks.drums.gain_db", "value": db})), Origin::User)
        };
        client::send(&h.path, &request).unwrap().unwrap()
    };
    for db in [-1.0, -2.0, -3.0] {
        assert_eq!(drag(db)["changed"], json!(true));
    }
    // The host answers from memory at once; the file follows the pause.
    let status = h.send(json!({"op": "status"})).unwrap();
    assert_eq!((&status["revision"], &status["saved_revision"]), (&json!(3), &json!(0)));
    assert_eq!(status["undo"]["label"], json!("Set tracks.drums.gain_db: 0.0 → -3.0"));
    assert_eq!(saved(), original);
    let started = Instant::now();
    while !saved().contains("gain_db: -3.0") {
        assert!(started.elapsed() < Duration::from_secs(5), "the drag was not saved");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(h.send(json!({"op": "status"})).unwrap()["saved_revision"], json!(3));
    assert_eq!(h.send(json!({"op": "changes"})).unwrap()["changes"].as_array().unwrap().len(), 1);

    // Any other edit saves at once, with the drag before it.
    drag(-4.0);
    h.send(json!({"op": "toggle", "path": "tracks.bass.mute"})).unwrap();
    assert!(saved().contains("gain_db: -4.0") && saved().contains("mute: true"));
    // So does closing.
    drag(-5.0);
    h.send(json!({"op": "close"})).unwrap();
    while !saved().contains("gain_db: -5.0") {
        assert!(started.elapsed() < Duration::from_secs(10), "closing did not save the drag");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn status_lists_what_the_person_selected() {
    let h = Running::start();
    assert_eq!(h.send(json!({"op": "status"})).unwrap()["selection"], json!([]));
    let inspect = h.send(json!({"op": "inspect"})).unwrap();
    let clip = inspect["tracks"][1]["clips"][2]["ref"].as_str().unwrap().to_string();
    let r = h.send(json!({"op": "select", "items": [clip, "tracks.drums", clip]})).unwrap();
    let selection = r["selection"].as_array().unwrap();
    assert_eq!(selection.len(), 2);
    assert_eq!(selection[0], json!({"ref": clip, "path": "tracks.bass.clips.2"}));
    assert_eq!(selection[1]["path"], json!("tracks.drums"));
    // The selection follows its objects through edits, and forgets removed ones.
    h.send(json!({"op": "clip.remove", "clip": "tracks.bass.clips.0"})).unwrap();
    h.send(json!({"op": "track.rename", "track": "drums", "to": "kit"})).unwrap();
    let status = h.send(json!({"op": "status"})).unwrap();
    assert_eq!(status["selection"], json!([{"ref": clip, "path": "tracks.bass.clips.1"}, {"ref": selection[1]["ref"], "path": "tracks.kit"}]));
    h.send(json!({"op": "clip.remove", "clip": clip})).unwrap();
    assert_eq!(h.send(json!({"op": "status"})).unwrap()["selection"].as_array().unwrap().len(), 1);
    // Only objects can be selected, and selecting is not a change.
    let e = h.send(json!({"op": "select", "items": ["tracks.kit.gain_db"]})).unwrap_err();
    assert!(e.contains("is not a track, a clip or another object with a handle"), "{e}");
    assert!(h.send(json!({"op": "select", "items": ["@9999"]})).is_err());
    assert_eq!(h.send(json!({"op": "select"})).unwrap()["selection"], json!([]));
    assert_eq!(h.send(json!({"op": "changes"})).unwrap()["changes"].as_array().unwrap().len(), 3);
}
