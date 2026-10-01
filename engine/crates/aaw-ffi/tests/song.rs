//! The app's view of a hosted song: the arrangement it draws, the objects each
//! change touched, and the transport, driven as the app and an agent drive it.
//! No audio device is opened.

use aaw_engine::wav::float_wav_bytes;
use aaw_ffi::view::{Delta, Part, Touch};
use aaw_ffi::{Song, SongObserver, TransportView, Update, Who};
use aaw_host::client::{self, Request};
use aaw_host::command::Origin;
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const SONG: &str = r#"
session: {title: Demo, tempo: 120, length_beats: 32}
samples:
  hit: {path: hit.wav}
patterns:
  beat:
    length_beats: 4
    steps: {h: x.x.x.x.x.x.x.x.}
  fill:
    length_beats: 2
    steps: {h: xxxxxxxx}
tracks:
- id: drums
  pads: {h: {sample: hit}}
  clips: [{pattern: beat, repeats: 4}, {pattern: fill, at: 16}]
  sends: [{to: plate, gain_db: -12}]
- id: perc
  gain_db: -3
  pads: {h: {sample: hit}}
  clips: [{pattern: beat, at: 8, repeats: 2}]
  effects: [{type: filter, mode: lowpass, cutoff_hz: 800}, {type: delay, time_beats: 0.5, bypass: true}]
returns:
- id: plate
  effects: [{type: reverb, decay_seconds: 1.5}]
sections: [{id: intro, at: 0, length_beats: 8}, {id: drop, at: 8, length_beats: 24}]
"#;

fn write_song(dir: &Path) -> PathBuf {
    let hit: Vec<[f32; 2]> = (0..4800).map(|i| [(0.5 * (-(i as f64) / 400.0).exp()) as f32; 2]).collect();
    std::fs::write(dir.join("hit.wav"), float_wav_bytes(&hit, 48000)).unwrap();
    let path = dir.join("song.yaml");
    std::fs::write(&path, SONG).unwrap();
    path
}

/// Socket paths are short, so the registry goes in a short directory.
fn registry_dir() {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::Builder::new().prefix("aaw").tempdir_in("/tmp").unwrap();
        // SAFETY: set once, before any test reads it; every test calls this first.
        unsafe { std::env::set_var("AAW_HOST_DIR", dir.path()) };
        dir
    });
}

enum Seen {
    Changed(Update),
    Transport(TransportView),
    Invalid(Option<String>),
    Closed,
}

struct Watcher(Mutex<Sender<Seen>>);

impl Watcher {
    fn send(&self, seen: Seen) {
        let _ = self.0.lock().unwrap().send(seen);
    }
}

impl SongObserver for Watcher {
    fn changed(&self, update: Update) {
        self.send(Seen::Changed(update));
    }
    fn transport(&self, transport: TransportView) {
        self.send(Seen::Transport(transport));
    }
    fn invalid(&self, error: Option<String>) {
        self.send(Seen::Invalid(error));
    }
    fn warning(&self, _message: String) {}
    fn closed(&self) {
        self.send(Seen::Closed);
    }
}

fn open() -> (tempfile::TempDir, PathBuf, Arc<Song>, Receiver<Seen>) {
    registry_dir();
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let (tx, rx) = channel();
    let song = Song::open(path.to_string_lossy().into_owned(), Arc::new(Watcher(Mutex::new(tx)))).unwrap();
    (dir, path, song, rx)
}

/// A command from another process, as the agent's `daw` sends it.
fn agent(project: &Path, j: Json) -> Json {
    let request = Request {
        command: serde_json::from_value(j).unwrap(),
        origin: Origin::Agent,
        expect: None,
    };
    client::send(project, &request).unwrap().expect("the song is hosted")
}

fn next(seen: &Receiver<Seen>) -> Seen {
    seen.recv_timeout(Duration::from_secs(10)).expect("the observer is told")
}

fn update(seen: &Receiver<Seen>) -> Update {
    match next(seen) {
        Seen::Changed(u) => u,
        _ => panic!("expected a change"),
    }
}

fn transport(seen: &Receiver<Seen>) -> TransportView {
    match next(seen) {
        Seen::Transport(t) => t,
        _ => panic!("expected the transport"),
    }
}

fn touch(part: Part, key: u64, delta: Delta) -> Touch {
    Touch { part, key, delta }
}

#[test]
fn the_arrangement_is_what_the_app_draws() {
    let (_dir, _path, song, _seen) = open();
    let a = song.arrangement();
    assert_eq!((a.revision, a.title.as_str(), a.tempo, a.beats_per_bar, a.length_beats), (0, "Demo", 120.0, 4, 32.0));
    assert_eq!(a.tracks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["drums", "perc"]);
    let drums = &a.tracks[0];
    assert_eq!(drums.sends, ["plate"]);
    let clips: Vec<_> = drums.clips.iter().map(|c| (c.pattern.as_str(), c.at, c.pattern_beats, c.repeats)).collect();
    assert_eq!(clips, [("beat", 0.0, 4.0, 4), ("fill", 16.0, 2.0, 1)]);
    let perc = &a.tracks[1];
    assert_eq!(perc.gain_db, -3.0);
    let chain: Vec<_> = perc.effects.iter().map(|e| (e.kind.as_str(), e.bypass)).collect();
    assert_eq!(chain, [("filter", false), ("delay", true)]);
    assert_eq!(a.returns[0].id, "plate");
    assert_eq!(a.returns[0].effects[0].kind, "reverb");
    assert_eq!(a.master.gain_db, -6.0);
    let sections: Vec<_> = a.sections.iter().map(|s| (s.id.as_str(), s.at, s.length_beats)).collect();
    assert_eq!(sections, [("intro", 0.0, 8.0), ("drop", 8.0, 24.0)]);
    // Every object has its own key, and a clip's reference is its handle.
    let mut keys: Vec<u64> = a.tracks.iter().flat_map(|t| std::iter::once(t.key).chain(t.clips.iter().map(|c| c.key))).collect();
    keys.extend(a.returns.iter().map(|r| r.key));
    keys.extend(a.sections.iter().map(|s| s.key));
    assert!(keys.iter().all(|k| *k != 0));
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 8);
    assert_eq!(drums.clips[0].reference, format!("@{}", drums.clips[0].key));
    song.close();
}

#[test]
fn each_change_names_what_it_touched() {
    let (_dir, path, song, seen) = open();
    assert_eq!(transport(&seen), TransportView { playing: false, cue: 0.0, loop_region: None });
    let start = song.arrangement();
    let (drums, perc) = (start.tracks[0].key, start.tracks[1].key);
    let (first, fill) = (start.tracks[0].clips[0].key, start.tracks[0].clips[1].key);
    let perc_clip = start.tracks[1].clips[0].key;

    // A fader: the track, and none of its clips.
    agent(&path, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -6}));
    let u = update(&seen);
    assert_eq!((u.change.revision, u.change.origin, u.change.op.as_str()), (1, Who::Agent, "set"));
    assert_eq!(u.change.label, "Set tracks.drums.gain_db: 0.0 → -6");
    assert_eq!(u.touched, [touch(Part::Track, drums, Delta::Changed)]);
    assert_eq!(u.arrangement.tracks[0].gain_db, -6.0);
    assert_eq!(song.arrangement(), u.arrangement);

    // Moving a clip to another track keeps its key.
    agent(&path, json!({"op": "clip.move", "clip": format!("@{fill}"), "track": "perc", "at": 20}));
    let u = update(&seen);
    assert_eq!(u.touched, [touch(Part::Clip, fill, Delta::Changed)]);
    let moved = &u.arrangement.tracks[1].clips[1];
    assert_eq!((moved.key, moved.at), (fill, 20.0));

    // A new clip, then its removal.
    let made = agent(&path, json!({"op": "clip.add", "track": "drums", "pattern": "fill", "at": 24}));
    let u = update(&seen);
    let added = u.arrangement.tracks[0].clips[1].key;
    assert_eq!(made["handle"], json!(format!("@{added}")));
    assert_eq!(u.touched, [touch(Part::Clip, added, Delta::Added)]);
    agent(&path, json!({"op": "clip.remove", "clip": format!("@{added}")}));
    assert_eq!(update(&seen).touched, [touch(Part::Clip, added, Delta::Removed)]);

    // A pattern edit touches the clips that play it.
    agent(&path, json!({"op": "pattern.steps", "pattern": "beat", "pad": "h", "row": "x...x...x...x..."}));
    let mut clips = update(&seen).touched;
    clips.sort_by_key(|t| t.key);
    assert_eq!(clips, [touch(Part::Clip, first, Delta::Changed), touch(Part::Clip, perc_clip, Delta::Changed)]);

    // Reordering changes the tracks, not their clips; a rename keeps the key.
    agent(&path, json!({"op": "track.move", "track": "perc", "index": 0}));
    let mut tracks = update(&seen).touched;
    tracks.sort_by_key(|t| t.key);
    assert_eq!(tracks, [touch(Part::Track, drums, Delta::Changed), touch(Part::Track, perc, Delta::Changed)]);
    agent(&path, json!({"op": "track.rename", "track": "perc", "to": "shaker"}));
    let u = update(&seen);
    assert_eq!(u.touched, [touch(Part::Track, perc, Delta::Changed)]);
    assert_eq!((u.arrangement.tracks[0].key, u.arrangement.tracks[0].id.as_str()), (perc, "shaker"));

    // Session, master, returns and sections.
    agent(&path, json!({"op": "set", "path": "session.tempo", "value": 140}));
    assert_eq!(update(&seen).touched, [touch(Part::Session, 0, Delta::Changed)]);
    agent(&path, json!({"op": "set", "path": "session.master_gain_db", "value": -3}));
    assert_eq!(update(&seen).touched, [touch(Part::Master, 0, Delta::Changed)]);
    agent(&path, json!({"op": "set", "path": "returns.plate.gain_db", "value": -2}));
    assert_eq!(update(&seen).touched, [touch(Part::Return, start.returns[0].key, Delta::Changed)]);
    agent(&path, json!({"op": "section.add", "id": "outro", "at": 28, "length_beats": 4}));
    let u = update(&seen);
    assert_eq!(u.touched, [touch(Part::Section, u.arrangement.sections[2].key, Delta::Added)]);

    // Undo is a change like any other, with what it put back.
    agent(&path, json!({"op": "undo"}));
    let u = update(&seen);
    assert_eq!(u.change.op, "undo");
    assert_eq!(u.touched.len(), 1);
    assert_eq!((u.touched[0].part, u.touched[0].delta), (Part::Section, Delta::Removed));

    // An edit of the file is an external change.
    let text = std::fs::read_to_string(&path).unwrap().replace("tempo: 140.0", "tempo: 90.0");
    std::fs::write(&path, text).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.origin, u.arrangement.tempo), (Who::External, 90.0));
    assert_eq!(u.touched, [touch(Part::Session, 0, Delta::Changed)]);
    // A file that does not load is reported, and again when it is fixed.
    let good = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, "session: {tempo: 0}\n").unwrap();
    assert!(matches!(next(&seen), Seen::Invalid(Some(e)) if e.contains("tempo")));
    std::fs::write(&path, good).unwrap();
    assert!(matches!(next(&seen), Seen::Invalid(None)));
    song.close();
    assert!(matches!(next(&seen), Seen::Closed));
}

#[test]
fn the_app_and_the_agent_share_the_transport() {
    let (_dir, path, song, seen) = open();
    assert_eq!(transport(&seen).cue, 0.0);
    song.locate(12.0).unwrap();
    assert_eq!(transport(&seen), TransportView { playing: false, cue: 12.0, loop_region: None });
    song.set_loop(8.0, 8.0).unwrap();
    assert_eq!(transport(&seen).loop_region.map(|l| (l.start, l.length)), Some((8.0, 8.0)));
    // The agent's transport commands show in the app.
    agent(&path, json!({"op": "locate", "at": 4}));
    assert_eq!(transport(&seen).cue, 4.0);
    assert_eq!(agent(&path, json!({"op": "status"}))["loop"], json!({"start": 8, "length": 8}));
    song.clear_loop().unwrap();
    assert_eq!(transport(&seen).loop_region, None);
    // Errors come back as errors, and nothing changed.
    assert!(song.locate(99.0).unwrap_err().to_string().contains("session end"));
    assert!(song.set_loop(30.0, 8.0).is_err());
    song.stop().unwrap();
    // Nothing has played, so there is no playhead.
    assert_eq!(song.playhead(), None);

    // `daw close` from outside closes the song under the app.
    agent(&path, json!({"op": "close"}));
    assert!(matches!(next(&seen), Seen::Closed));
    assert!(song.locate(0.0).is_err());
    song.close();
}
