//! The app's view of a hosted song: the arrangement it draws, the objects each
//! change touched, and the transport, driven as the app and an agent drive it.
//! No audio device is opened.

use aaw_engine::wav::float_wav_bytes;
use aaw_ffi::view::{Delta, EffectView, FieldKind, FieldValue, FieldView, LaneView, Part, PatternView, SendView, Touch};
use aaw_ffi::{Edit, HistoryStep, Row, Song, SongObserver, TransportView, Update, Waveforms, Who};
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
        // SAFETY: set once, before any test reads them; every test calls this first.
        unsafe {
            std::env::set_var("AAW_HOST_DIR", dir.path());
            // Untitled projects go here, not into the person's own.
            std::env::set_var("AAW_DATA_DIR", dir.path().join("data"));
        }
        dir
    });
}

enum Seen {
    Changed(Update),
    Transport(TransportView),
    Invalid(Option<String>),
    Moved(String),
    Closed,
}

/// Waveforms come from a thread of their own, so they are watched apart.
struct Watcher(Mutex<Sender<Seen>>, Mutex<Sender<Waveforms>>);

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
    fn moved(&self, path: String) {
        self.send(Seen::Moved(path));
    }
    fn warning(&self, _message: String) {}
    fn waveforms(&self, waveforms: Waveforms) {
        let _ = self.1.lock().unwrap().send(waveforms);
    }
    fn closed(&self) {
        self.send(Seen::Closed);
    }
}

fn open() -> (tempfile::TempDir, PathBuf, Arc<Song>, Receiver<Seen>) {
    let (dir, path, song, seen, _) = open_with_waveforms();
    (dir, path, song, seen)
}

fn open_with_waveforms() -> (tempfile::TempDir, PathBuf, Arc<Song>, Receiver<Seen>, Receiver<Waveforms>) {
    registry_dir();
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let (tx, rx) = channel();
    let (waves_tx, waves) = channel();
    let watcher = Watcher(Mutex::new(tx), Mutex::new(waves_tx));
    let song = Song::open(path.to_string_lossy().into_owned(), Arc::new(watcher)).unwrap();
    (dir, path, song, rx, waves)
}

/// A command from another process, as the agent's `daw` sends it.
fn agent(project: &Path, j: Json) -> Json {
    let request = Request::new(serde_json::from_value(j).unwrap(), Origin::Agent);
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
    assert_eq!(drums.sends, [SendView { to: "plate".into(), gain_db: -12.0 }]);
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
    assert_eq!(drums.pads.iter().map(|p| (p.name.as_str(), p.sample.as_str(), p.gate)).collect::<Vec<_>>(), [("h", "hit", false)]);
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

fn step(label: &str, origin: Who) -> Option<HistoryStep> {
    Some(HistoryStep {
        label: label.into(),
        origin,
    })
}

#[test]
fn the_person_edits_the_mixer_in_the_same_history() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let (drums, perc) = (Row::Track { key: start.tracks[0].key }, Row::Track { key: start.tracks[1].key });
    let plate = Row::Return { key: start.returns[0].key };

    assert!(song.edit(Edit::Gain { row: drums.clone(), db: -6.0 }, None).unwrap().is_empty());
    let u = update(&seen);
    assert_eq!((u.change.origin, u.change.label.as_str()), (Who::User, "Set tracks.drums.gain_db: 0.0 → -6"));
    assert_eq!((u.undo, u.redo), (step("Set tracks.drums.gain_db: 0.0 → -6", Who::User), None));
    assert_eq!(u.arrangement.tracks[0].gain_db, -6.0);

    // A drag is one step, named for the whole move.
    for db in [-7.5, -9.0] {
        song.edit(Edit::Gain { row: drums.clone(), db }, Some("drag".into())).unwrap();
        assert_eq!(update(&seen).change.gesture.as_deref(), Some("drag"));
    }
    song.edit(Edit::Pan { row: perc.clone(), pan: -0.25 }, None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[1].pan, -0.25);
    song.undo().unwrap();
    let u = update(&seen);
    assert_eq!(u.change.op, "undo");
    assert_eq!(u.undo, step("Set tracks.drums.gain_db: -6.0 → -9", Who::User));
    assert_eq!(u.redo, step("Set tracks.perc.pan: 0.0 → -0.25", Who::User));
    song.redo().unwrap();
    assert_eq!(update(&seen).redo, None);

    // The agent's edits are steps in the same history, named as the agent's.
    agent(&path, json!({"op": "set", "path": "tracks.perc.gain_db", "value": -1}));
    assert_eq!(update(&seen).undo, step("Set tracks.perc.gain_db: -3.0 → -1", Who::Agent));
    song.undo().unwrap();
    let u = update(&seen);
    assert_eq!((u.change.origin, u.arrangement.tracks[1].gain_db), (Who::User, -3.0));

    song.edit(Edit::Mute { row: drums.clone(), on: true }, None).unwrap();
    song.edit(Edit::Solo { track: start.tracks[1].key, on: true }, None).unwrap();
    update(&seen);
    let a = update(&seen).arrangement;
    assert_eq!((a.tracks[0].mute, a.tracks[1].solo), (true, true));
    song.edit(Edit::Gain { row: plate.clone(), db: -4.0 }, None).unwrap();
    song.edit(Edit::Mute { row: plate.clone(), on: true }, None).unwrap();
    song.edit(Edit::Gain { row: Row::Master, db: -3.0 }, None).unwrap();
    update(&seen);
    update(&seen);
    let a = update(&seen).arrangement;
    assert_eq!((a.returns[0].gain_db, a.returns[0].mute, a.master.gain_db), (-4.0, true, -3.0));
    assert!(song.edit(Edit::Pan { row: Row::Master, pan: 0.5 }, None).is_err());

    // Sends: a level adds the send, and removing it takes it away.
    let send = |to: &str, db: f64| Edit::Send { track: start.tracks[1].key, to: to.into(), db };
    song.edit(send("plate", -9.0), None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Set send perc → plate");
    assert_eq!(u.arrangement.tracks[1].sends, [SendView { to: "plate".into(), gain_db: -9.0 }]);
    song.edit(send("plate", -8.5), None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[1].sends[0].gain_db, -8.5);
    song.edit(Edit::SendRemove { track: start.tracks[1].key, to: "plate".into() }, None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[1].sends, []);
    // A refused edit changes nothing and reports why.
    let e = song.edit(send("nowhere", -9.0), None).unwrap_err().to_string();
    assert!(e.contains("nowhere"), "{e}");
    assert!(song.edit(Edit::Gain { row: drums, db: 99.0 }, None).is_err());
    assert_eq!(agent(&path, json!({"op": "get", "path": "tracks.perc.sends"})), json!([]));
    song.close();
}

#[test]
fn clips_move_resize_and_duplicate_on_exact_beats() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let (first, fill) = (start.tracks[0].clips[0].key, start.tracks[0].clips[1].key);
    let perc_clip = start.tracks[1].clips[0].key;
    let clips = |a: &aaw_ffi::Arrangement, track: usize| -> Vec<(u64, f64, u32)> {
        a.tracks[track].clips.iter().map(|c| (c.key, c.at, c.repeats)).collect()
    };

    // Two clips move together: later, and one track down.
    song.edit(Edit::ClipsMove { clips: vec![fill], by: 4.0, rows: 1 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Move clip fill at 20");
    assert_eq!(clips(&u.arrangement, 1), [(perc_clip, 8.0, 2), (fill, 20.0, 1)]);
    song.edit(Edit::ClipsMove { clips: vec![perc_clip, fill], by: -0.5, rows: -1 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), u.change.label.as_str()), ("batch", "Move 2 clips"));
    assert_eq!(clips(&u.arrangement, 0), [(first, 0.0, 4), (perc_clip, 7.5, 2), (fill, 19.5, 1)]);
    assert_eq!(u.undo.as_ref().map(|s| s.origin), Some(Who::User));
    // Nothing to do is no change; no track above the first is refused.
    assert!(song.edit(Edit::ClipsMove { clips: vec![fill], by: 0.0, rows: 0 }, None).unwrap().is_empty());
    assert!(song.edit(Edit::ClipsMove { clips: vec![fill], by: 0.0, rows: -1 }, None).is_err());
    let e = song.edit(Edit::ClipsMove { clips: vec![fill], by: 12.0, rows: 0 }, None).unwrap_err().to_string();
    assert_eq!(e, "drums: clip exceeds session");

    // A clip on a third of a beat stays on thirds when it moves.
    let third = agent(&path, json!({"op": "clip.add", "track": "perc", "pattern": "fill", "at": "1/3"}));
    let third: u64 = third["handle"].as_str().unwrap()[1..].parse().unwrap();
    update(&seen);
    song.edit(Edit::ClipsMove { clips: vec![third], by: 4.0, rows: 0 }, None).unwrap();
    update(&seen);
    assert_eq!(agent(&path, json!({"op": "get", "path": format!("@{third}.at")})), json!("13/3"));
    song.edit(Edit::ClipsMove { clips: vec![third], by: 0.125, rows: 0 }, None).unwrap();
    update(&seen);
    assert_eq!(agent(&path, json!({"op": "get", "path": format!("@{third}.at")})), json!("107/24"));

    song.edit(Edit::ClipRepeats { clip: fill, repeats: 3 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(clips(&u.arrangement, 0)[2], (fill, 19.5, 3));

    // Copies go right after what was copied: one clip after itself, a group
    // after the group. The edit returns the copies.
    let made = song.edit(Edit::ClipsDuplicate { clips: vec![fill] }, None).unwrap();
    let u = update(&seen);
    assert_eq!(clips(&u.arrangement, 0)[3], (made[0], 25.5, 3));
    song.undo().unwrap();
    update(&seen);
    let made = song.edit(Edit::ClipsDuplicate { clips: vec![perc_clip, first] }, None).unwrap();
    let u = update(&seen);
    // The group spans beats 0 to 16, so each copy is 16 beats after its original.
    let copies: Vec<(u64, f64, u32)> = made.iter().map(|k| *clips(&u.arrangement, 0).iter().find(|c| c.0 == *k).unwrap()).collect();
    assert_eq!(copies, [(made[0], 23.5, 2), (made[1], 16.0, 4)]);
    let mut added: Vec<u64> = u.touched.iter().filter(|t| t.delta == Delta::Added).map(|t| t.key).collect();
    added.sort();
    assert_eq!(added, { let mut m = made.clone(); m.sort(); m });
    assert_eq!(u.undo.unwrap().label, "Duplicate 2 clips");

    song.edit(Edit::ClipsRemove { clips: made }, None).unwrap();
    assert_eq!(clips(&update(&seen).arrangement, 0).len(), 3);
    song.close();
}

#[test]
fn tracks_and_returns_are_added_named_moved_and_removed() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let ids = |a: &aaw_ffi::Arrangement| -> (Vec<String>, Vec<String>) {
        (a.tracks.iter().map(|t| t.id.clone()).collect(), a.returns.iter().map(|r| r.id.clone()).collect())
    };

    // A new track gets a free name, which the person then changes.
    let made = song.edit(Edit::TrackAdd { index: 1 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(ids(&u.arrangement).0, ["drums", "track-1", "perc"]);
    assert_eq!(made, [u.arrangement.tracks[1].key]);
    let track = Row::Track { key: made[0] };
    assert_eq!(song.edit(Edit::TrackAdd { index: 99 }, None).unwrap().len(), 1);
    assert_eq!(ids(&update(&seen).arrangement).0, ["drums", "track-1", "perc", "track-2"]);
    song.edit(Edit::Rename { row: track.clone(), to: "keys".into() }, None).unwrap();
    assert_eq!(update(&seen).change.label, "Rename track track-1 to keys");
    let e = song.edit(Edit::Rename { row: track.clone(), to: "two words".into() }, None).unwrap_err().to_string();
    assert_eq!(e, "tracks.1.id: String should match pattern '^[a-zA-Z][a-zA-Z0-9_-]*$'");
    assert!(song.edit(Edit::Rename { row: track.clone(), to: "drums".into() }, None).is_err());
    song.edit(Edit::Move { row: track.clone(), index: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Move track keys to position 0");
    assert_eq!(ids(&u.arrangement).0, ["keys", "drums", "perc", "track-2"]);
    song.edit(Edit::Remove { row: track }, None).unwrap();
    assert_eq!(ids(&update(&seen).arrangement).0, ["drums", "perc", "track-2"]);

    // Returns likewise; a renamed return keeps its sends.
    let made = song.edit(Edit::ReturnAdd { index: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(ids(&u.arrangement).1, ["return-1", "plate"]);
    let room = Row::Return { key: made[0] };
    song.edit(Edit::Move { row: room.clone(), index: 1 }, None).unwrap();
    assert_eq!(ids(&update(&seen).arrangement).1, ["plate", "return-1"]);
    song.edit(Edit::Rename { row: Row::Return { key: start.returns[0].key }, to: "hall".into() }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.arrangement.tracks[0].sends, [SendView { to: "hall".into(), gain_db: -12.0 }]);
    song.edit(Edit::Remove { row: room }, None).unwrap();
    assert_eq!(ids(&update(&seen).arrangement).1, ["hall"]);
    for edit in [Edit::Rename { row: Row::Master, to: "x".into() }, Edit::Remove { row: Row::Master }, Edit::Move { row: Row::Master, index: 0 }] {
        assert!(song.edit(edit, None).is_err());
    }

    // What the person selects, the agent can ask about.
    let clip = start.tracks[0].clips[1].key;
    song.select(vec![clip, start.tracks[1].key, 9999]).unwrap();
    let selection = agent(&path, json!({"op": "status"}))["selection"].clone();
    assert_eq!(
        selection,
        json!([
            {"ref": format!("@{clip}"), "path": "tracks.drums.clips.1"},
            {"ref": format!("@{}", start.tracks[1].key), "path": "tracks.perc"},
        ])
    );
    song.select(Vec::new()).unwrap();
    assert_eq!(agent(&path, json!({"op": "status"}))["selection"], json!([]));
    song.close();
    assert!(song.edit(Edit::TrackAdd { index: 0 }, None).is_err());
}

#[test]
fn refusals_read_plainly() {
    let one = "1 validation error for Project\n  Value error, bass: unknown pad p [type=value_error]";
    assert_eq!(aaw_ffi::plain(one), "bass: unknown pad p");
    let two = "2 validation errors for Project\nsession.tempo\n  Input should be less than or equal to 400 \
               [type=less_than_equal]\ntracks.0.pan\n  Input should be greater than or equal to -1 [type=greater_than_equal]";
    assert_eq!(
        aaw_ffi::plain(two),
        "session.tempo: Input should be less than or equal to 400; tracks.0.pan: Input should be greater than or equal to -1"
    );
    assert_eq!(aaw_ffi::plain("locate 99 is at or after the session end"), "locate 99 is at or after the session end");
}

fn field<'a>(e: &'a EffectView, name: &str) -> &'a FieldView {
    e.fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("{} has no {name}", e.kind))
}

fn number(value: f64) -> FieldValue {
    FieldValue::Number { value }
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text { value: value.into() }
}

#[test]
fn a_device_panel_is_drawn_from_an_effect_s_fields() {
    let (_dir, _path, song, _seen) = open();
    let a = song.arrangement();
    let filter = &a.tracks[1].effects[0];
    assert_eq!(filter.fields.iter().map(|f| f.label.as_str()).collect::<Vec<_>>(), ["Mode", "Cutoff", "Slope"]);
    let mode = field(filter, "mode");
    assert_eq!((mode.kind, &mode.value, mode.choices.as_slice(), mode.live), (FieldKind::Choice, &text("lowpass"), &["highpass".to_string(), "lowpass".to_string()][..], false));
    let cutoff = field(filter, "cutoff_hz");
    assert_eq!((cutoff.kind, &cutoff.value, cutoff.min, cutoff.max, cutoff.unit.as_str(), cutoff.log, cutoff.live), (FieldKind::Number, &number(800.0), 10.0, 20000.0, "Hz", true, true));
    // Automation can move the cutoff, by the effect's place while it has no ID.
    assert_eq!((cutoff.param.as_deref(), cutoff.lane), (Some("effects.0.cutoff_hz"), None));
    let slope = field(filter, "slope_db_per_octave");
    assert_eq!((slope.kind, &slope.value, slope.choices.len(), slope.param.as_deref()), (FieldKind::Integer, &number(12.0), 4, None));

    let delay = &a.tracks[1].effects[1];
    assert!(delay.bypass);
    let time = field(delay, "time_beats");
    assert_eq!((time.kind, &time.value, time.live), (FieldKind::Beats, &number(0.5), false));
    let cut = field(delay, "highcut_hz");
    assert_eq!((cut.optional, &cut.value, &cut.initial), (true, &FieldValue::Absent, &number(4000.0)));
    assert_eq!(field(delay, "ping_pong").value, FieldValue::Flag { value: false });

    let reverb = &a.returns[0].effects[0];
    assert_eq!(field(reverb, "decay_seconds").value, number(1.5));
    assert!(!field(reverb, "decay_seconds").live && field(reverb, "mix_percent").live);
    // What could have a lane: levels, sends, and each effect's knobs.
    let targets = |t: &[aaw_ffi::view::LaneTarget]| t.iter().map(|t| (t.param.clone(), t.label.clone())).collect::<Vec<_>>();
    assert_eq!(
        targets(&a.tracks[0].lane_targets),
        [("gain_db".into(), "Volume".into()), ("pan".into(), "Pan".into()), ("sends.plate.gain_db".into(), "Send plate".into())]
    );
    assert_eq!(targets(&a.master.lane_targets), [("gain_db".to_string(), "Volume".to_string())]);
    assert!(targets(&a.tracks[1].lane_targets).contains(&("effects.1.feedback_percent".into(), "delay Feedback".into())));
    song.close();
}

#[test]
fn the_person_builds_a_chain_and_turns_its_knobs() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let drums = Row::Track { key: start.tracks[0].key };
    let kinds = |a: &aaw_ffi::Arrangement, track: usize| a.tracks[track].effects.iter().map(|e| e.kind.clone()).collect::<Vec<_>>();

    // An effect is added with its required fields at a place to start.
    let made = song.edit(Edit::EffectAdd { row: drums.clone(), kind: "compressor".into(), index: None }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.origin, u.change.label.as_str()), (Who::User, "Add compressor to tracks.drums"));
    let compressor = &u.arrangement.tracks[0].effects[0];
    assert_eq!(made, [compressor.key]);
    assert_eq!(field(compressor, "threshold_db").value, number(-18.0));
    // Any other track can key it.
    assert_eq!(field(compressor, "sidechain").choices, ["perc"]);
    song.edit(Edit::EffectSet { effect: made[0], field: "sidechain".into(), value: text("perc") }, None).unwrap();
    assert_eq!(field(&update(&seen).arrangement.tracks[0].effects[0], "sidechain").value, text("perc"));

    // A delay or reverb as an insert starts under the dry signal; on a return it is all wet.
    let delay = song.edit(Edit::EffectAdd { row: drums.clone(), kind: "delay".into(), index: Some(0) }, None).unwrap()[0];
    let u = update(&seen);
    assert_eq!(kinds(&u.arrangement, 0), ["delay", "compressor"]);
    assert_eq!(field(&u.arrangement.tracks[0].effects[0], "mix_percent").value, number(25.0));
    assert_eq!(field(&u.arrangement.tracks[0].effects[0], "time_beats").value, text("1/2"));
    let plate = Row::Return { key: start.returns[0].key };
    song.edit(Edit::EffectAdd { row: plate, kind: "delay".into(), index: None }, None).unwrap();
    assert_eq!(field(&update(&seen).arrangement.returns[0].effects[1], "mix_percent").value, number(100.0));
    for kind in ["filter", "eq", "limiter", "reverb"] {
        song.edit(Edit::EffectAdd { row: Row::Master, kind: kind.into(), index: None }, None).unwrap();
        update(&seen);
    }
    assert_eq!(song.arrangement().master.effects.len(), 4);

    // A knob drag is one undo step, named for the whole move.
    let set = |value: f64| Edit::EffectSet { effect: delay, field: "feedback_percent".into(), value: number(value) };
    for value in [40.0, 52.5, 60.0] {
        song.edit(set(value), Some("knob".into())).unwrap();
    }
    update(&seen);
    update(&seen);
    let u = update(&seen);
    assert_eq!(u.change.label, "Set tracks.drums.effects.0.feedback_percent: 35.0 → 60");
    assert_eq!(field(&u.arrangement.tracks[0].effects[0], "feedback_percent").value, number(60.0));
    // A beat is written as a fraction or a number, and an optional field can be left out again.
    song.edit(Edit::EffectSet { effect: delay, field: "time_beats".into(), value: text("0.75") }, None).unwrap();
    update(&seen);
    assert_eq!(agent(&path, json!({"op": "get", "path": format!("@{delay}.time_beats")})), json!(0.75));
    song.edit(Edit::EffectSet { effect: delay, field: "highcut_hz".into(), value: number(3000.0) }, None).unwrap();
    assert_eq!(field(&update(&seen).arrangement.tracks[0].effects[0], "highcut_hz").value, number(3000.0));
    song.edit(Edit::EffectSet { effect: delay, field: "highcut_hz".into(), value: FieldValue::Absent }, None).unwrap();
    assert_eq!(field(&update(&seen).arrangement.tracks[0].effects[0], "highcut_hz").value, FieldValue::Absent);
    let e = song.edit(set(200.0), None).unwrap_err().to_string();
    assert!(e.contains("less than or equal to 95"), "{e}");

    // Bypass, reorder and remove, by the effect's key.
    song.edit(Edit::EffectBypass { effect: delay, on: true }, None).unwrap();
    assert!(update(&seen).arrangement.tracks[0].effects[0].bypass);
    song.edit(Edit::EffectMove { effect: delay, index: 1 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(kinds(&u.arrangement, 0), ["compressor", "delay"]);
    assert_eq!(u.arrangement.tracks[0].effects[1].key, delay);
    song.edit(Edit::EffectRemove { effect: delay }, None).unwrap();
    assert_eq!(kinds(&update(&seen).arrangement, 0), ["compressor"]);
    song.undo().unwrap();
    assert_eq!(kinds(&update(&seen).arrangement, 0), ["compressor", "delay"]);
    song.close();
}

#[test]
fn lanes_and_points_are_edited_by_key() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let perc = Row::Track { key: start.tracks[1].key };
    let lane = |u: &Update, track: usize, index: usize| -> LaneView { u.arrangement.tracks[track].lanes[index].clone() };
    let points = |l: &LaneView| l.points.iter().map(|p| (p.at, p.value, p.hold)).collect::<Vec<_>>();

    // A new lane holds the value the parameter has, which changes nothing.
    song.edit(Edit::LaneAdd { row: perc.clone(), param: "gain_db".into() }, None).unwrap();
    let u = update(&seen);
    let volume = lane(&u, 1, 0);
    assert_eq!((volume.label.as_str(), volume.unit.as_str(), volume.min, volume.max, volume.log), ("Volume", "dB", -60.0, 6.0, false));
    assert_eq!(points(&volume), [(0.0, -3.0, false)]);
    assert!(!u.arrangement.tracks[1].lane_targets.iter().any(|t| t.param == "gain_db"));

    // Points land in time order wherever they are added, on exact beats.
    let late = song.edit(Edit::PointAdd { lane: volume.key, at: 16.0, value: -12.0 }, None).unwrap()[0];
    update(&seen);
    let mid = song.edit(Edit::PointAdd { lane: volume.key, at: 8.25, value: -6.0 }, None).unwrap()[0];
    let u = update(&seen);
    assert_eq!(points(&lane(&u, 1, 0)), [(0.0, -3.0, false), (8.25, -6.0, false), (16.0, -12.0, false)]);
    assert_eq!(lane(&u, 1, 0).points[1].key, mid);
    assert_eq!(agent(&path, json!({"op": "get", "path": format!("@{mid}.at")})), json!(8.25));

    // A dragged point is one undo step; moved past another, it stays in order.
    for (at, value) in [(10.0, -8.0), (20.0, -9.0)] {
        song.edit(Edit::PointSet { point: mid, at: Some(at), value: Some(value), hold: None }, Some("point".into())).unwrap();
    }
    update(&seen);
    let u = update(&seen);
    assert_eq!(points(&lane(&u, 1, 0)), [(0.0, -3.0, false), (16.0, -12.0, false), (20.0, -9.0, false)]);
    assert_eq!(lane(&u, 1, 0).points[2].key, mid);
    // The bend an agent gives a segment comes with its point.
    agent(&path, json!({"op": "set", "path": format!("@{late}.shape"), "value": -0.5}));
    assert_eq!(lane(&update(&seen), 1, 0).points.iter().map(|p| p.shape).collect::<Vec<_>>(), [0.0, -0.5, 0.0]);
    song.undo().unwrap();
    update(&seen);
    song.edit(Edit::PointSet { point: late, at: None, value: None, hold: Some(true) }, None).unwrap();
    assert_eq!(points(&lane(&update(&seen), 1, 0))[1], (16.0, -12.0, true));
    song.undo().unwrap();
    update(&seen);
    song.undo().unwrap();
    assert_eq!(points(&lane(&update(&seen), 1, 0))[1], (8.25, -6.0, false));
    // A point past the session end, or a value out of range, is refused.
    assert!(song.edit(Edit::PointAdd { lane: volume.key, at: 40.0, value: 0.0 }, None).is_err());
    assert!(song.edit(Edit::PointSet { point: mid, at: None, value: Some(99.0), hold: None }, None).is_err());

    // An effect's knob: the field shows its lane, on the knob's own range.
    let filter = start.tracks[1].effects[0].key;
    song.edit(Edit::LaneAdd { row: perc.clone(), param: "effects.0.cutoff_hz".into() }, None).unwrap();
    let u = update(&seen);
    let sweep = lane(&u, 1, 1);
    assert_eq!((sweep.label.as_str(), sweep.unit.as_str(), sweep.min, sweep.max, sweep.log), ("filter Cutoff", "Hz", 10.0, 20000.0, true));
    assert_eq!(points(&sweep), [(0.0, 800.0, false)]);
    let cutoff = field(&u.arrangement.tracks[1].effects[0], "cutoff_hz").clone();
    assert_eq!((u.arrangement.tracks[1].effects[0].key, cutoff.lane), (filter, Some(sweep.key)));

    // A lane goes when it is removed, or with its last point.
    song.edit(Edit::LaneRemove { lane: sweep.key }, None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[1].lanes.len(), 1);
    for point in lane(&u, 1, 0).points {
        song.edit(Edit::PointRemove { point: point.key }, None).unwrap();
        update(&seen);
    }
    assert!(song.arrangement().tracks[1].lanes.is_empty());
    song.close();
}

#[test]
fn equalizer_bands_come_and_go_with_their_lanes() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let drums = Row::Track { key: start.tracks[0].key };
    let eq = song.edit(Edit::EffectAdd { row: drums.clone(), kind: "eq".into(), index: None }, None).unwrap()[0];
    let u = update(&seen);
    let e = &u.arrangement.tracks[0].effects[0];
    assert_eq!((e.bands, e.fields.len()), (1, 4));
    assert_eq!(e.fields.iter().map(|f| (f.name.as_str(), f.band)).collect::<Vec<_>>()[..2], [("bands.0.shape", Some(0)), ("bands.0.freq_hz", Some(0))]);
    assert_eq!((field(e, "bands.0.gain_db").value.clone(), field(e, "bands.0.q").value.clone()), (number(0.0), number(0.71)));
    for _ in 0..2 {
        song.edit(Edit::BandAdd { effect: eq }, None).unwrap();
        assert_eq!(update(&seen).change.label, "Add a band to an equalizer");
    }
    for (band, hz) in [(0, 200.0), (1, 1200.0), (2, 5000.0)] {
        song.edit(Edit::EffectSet { effect: eq, field: format!("bands.{band}.freq_hz"), value: number(hz) }, None).unwrap();
        update(&seen);
    }
    song.edit(Edit::EffectSet { effect: eq, field: "bands.2.shape".into(), value: text("high_shelf") }, None).unwrap();
    update(&seen);
    for param in ["effects.0.bands.1.gain_db", "effects.0.bands.2.gain_db", "effects.0.bands.2.freq_hz"] {
        song.edit(Edit::LaneAdd { row: drums.clone(), param: param.into() }, None).unwrap();
        update(&seen);
    }
    assert_eq!(song.arrangement().tracks[0].lanes[1].label, "eq band 3 Gain");

    // Removing the middle band takes its lane, and the last band's lanes follow it down.
    song.edit(Edit::BandRemove { effect: eq, band: 1 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), u.change.label.as_str()), ("batch", "Remove band 2 of an equalizer"));
    let e = &u.arrangement.tracks[0].effects[0];
    assert_eq!(e.bands, 2);
    assert_eq!((field(e, "bands.1.freq_hz").value.clone(), field(e, "bands.1.shape").value.clone()), (number(5000.0), text("high_shelf")));
    let lanes: Vec<(String, f64)> = u.arrangement.tracks[0].lanes.iter().map(|l| (l.param.clone(), l.points[0].value)).collect();
    assert_eq!(lanes, [("effects.0.bands.1.gain_db".to_string(), 0.0), ("effects.0.bands.1.freq_hz".to_string(), 5000.0)]);
    assert!(field(e, "bands.1.freq_hz").lane.is_some() && field(e, "bands.0.gain_db").lane.is_none());
    // One step, which undo takes back whole.
    song.undo().unwrap();
    let u = update(&seen);
    assert_eq!((u.arrangement.tracks[0].effects[0].bands, u.arrangement.tracks[0].lanes.len()), (3, 3));
    assert_eq!(agent(&path, json!({"op": "get", "path": "tracks.drums.automation.0.param"})), json!("effects.0.bands.1.gain_db"));
    song.close();
}

/// The waveforms of a revision, with every track's peaks arrived: updates
/// until none of the revision's audio is missing from what has been sent.
fn waveforms(waves: &Receiver<Waveforms>, revision: u64, have: &mut std::collections::HashMap<u64, aaw_ffi::waveform::TrackPeaks>) -> Waveforms {
    let mut sent = Vec::new();
    loop {
        let mut w = waves.recv_timeout(Duration::from_secs(10)).expect("waveforms");
        if w.revision < revision {
            continue;
        }
        assert_eq!(w.revision, revision);
        have.retain(|id, _| w.tracks.iter().any(|t| t.identity == *id));
        for p in &w.peaks {
            have.insert(p.identity, p.clone());
        }
        sent.append(&mut w.peaks);
        if w.tracks.iter().all(|t| have.contains_key(&t.identity)) {
            w.peaks = sent;
            return w;
        }
    }
}

#[test]
fn waveforms_follow_the_audio_of_each_track() {
    let (_dir, path, song, seen, waves) = open_with_waveforms();
    transport(&seen);
    let start = song.arrangement();
    let (drums, perc) = (start.tracks[0].key, start.tracks[1].key);
    let mut have = std::collections::HashMap::new();

    // Opening the song gives every track its peaks.
    let first = waveforms(&waves, 0, &mut have);
    assert_eq!(first.tracks.iter().map(|t| t.track).collect::<Vec<_>>(), [drums, perc]);
    assert_eq!(first.peaks.len(), 2);
    let identity = |w: &Waveforms, track: u64| w.tracks.iter().find(|t| t.track == track).unwrap().identity;
    let drum_peaks = have[&identity(&first, drums)].clone();
    // 120 beats a minute at 48 kHz, over 32 beats.
    assert_eq!((drum_peaks.frames_per_beat, drum_peaks.frames), (24000.0, 768000));
    let finest = &drum_peaks.levels[0];
    assert_eq!((finest.frames_per_bucket, finest.data.len()), (64, 2 * 12000));
    assert!(drum_peaks.levels.len() > 1 && drum_peaks.levels.last().unwrap().data.len() <= 2 * 2048);
    // The first hit is at the start, above zero only: half of full scale at
    // a step's velocity. Beats 18 to 32 of the drums have no clip and are silent.
    assert_eq!((finest.data[0] as i8, finest.data[1] as i8), (0, 49));
    let bucket = |beat: f64| 2 * (beat * 24000.0 / 64.0) as usize;
    assert!(finest.data[bucket(18.5)..].iter().all(|x| *x == 0));
    assert!(finest.data[bucket(16.0)..bucket(18.0)].iter().any(|x| *x != 0));

    // A fader changes no audio: the same identities, and nothing to send.
    agent(&path, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -6}));
    let faded = waveforms(&waves, 1, &mut have);
    assert_eq!((faded.tracks.clone(), faded.peaks.len()), (first.tracks.clone(), 0));

    // A moved clip changes its track's audio and no other's.
    let clip = start.tracks[1].clips[0].key;
    song.edit(Edit::ClipsMove { clips: vec![clip], by: 8.0, rows: 0 }, None).unwrap();
    let moved = waveforms(&waves, 2, &mut have);
    assert_eq!(identity(&moved, drums), identity(&first, drums));
    assert_ne!(identity(&moved, perc), identity(&first, perc));
    assert_eq!(moved.peaks.iter().map(|p| p.identity).collect::<Vec<_>>(), [identity(&moved, perc)]);
    let perc_peaks = &moved.peaks[0].levels[0].data;
    assert!(perc_peaks[..bucket(15.9)].iter().all(|x| *x == 0) && perc_peaks[bucket(16.0)..bucket(16.1)].iter().any(|x| *x != 0));

    // Undo brings the old audio back, from what was kept.
    song.undo().unwrap();
    let undone = waveforms(&waves, 3, &mut have);
    assert_eq!(undone.tracks, first.tracks);
    assert_eq!(undone.peaks.len(), 1);
    assert!(undone.peaks[0].levels[0].data[..bucket(8.0)].iter().all(|x| *x == 0));

    // A new track has peaks of its own: silence, until it plays something.
    let added = song.edit(Edit::TrackAdd { index: 2 }, None).unwrap()[0];
    let grown = waveforms(&waves, 4, &mut have);
    assert_eq!(grown.tracks.iter().map(|t| t.track).collect::<Vec<_>>(), [drums, perc, added]);
    assert!(have[&identity(&grown, added)].levels[0].data.iter().all(|x| *x == 0));
    song.close();
}

/// Ten seconds of audio at 48 kHz beside the song, listed as the sample
/// `song`: at 120 BPM a beat of the session is half a second of it.
fn with_song_file(dir: &Path, path: &Path, seen: &Receiver<Seen>) {
    let frames: Vec<[f32; 2]> = (0..480_000).map(|i| [((i % 4800) as f32 / 4800.0 - 0.5) * (i as f32 / 480_000.0); 2]).collect();
    std::fs::write(dir.join("song.wav"), float_wav_bytes(&frames, 48000)).unwrap();
    agent(path, json!({"op": "set", "path": "samples.song", "value": {"path": "song.wav"}}));
    update(seen);
}

fn audio_add(path: &Path, seen: &Receiver<Seen>, fields: Json) -> Update {
    let mut command = json!({"op": "audio.add", "sample": "song"});
    command.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
    agent(path, command);
    update(seen)
}

#[test]
fn audio_clips_are_what_the_app_draws() {
    let (dir, path, song, seen, waves) = open_with_waveforms();
    transport(&seen);
    with_song_file(dir.path(), &path, &seen);
    let drums = song.arrangement().tracks[0].key;
    assert!(song.arrangement().files.is_empty(), "no clip plays a file yet");

    // A clip of seconds 1 to 3 on beat 4: four beats of the session.
    let u = audio_add(&path, &seen, json!({"track": "drums", "at": 4, "source_start_seconds": 1, "source_end_seconds": 3, "gain_db": -3, "fade_out_ms": 20}));
    let clip = u.arrangement.tracks[0].audio[0].clone();
    assert_eq!(u.touched, [touch(Part::Clip, clip.key, Delta::Added)]);
    assert_eq!((clip.reference.as_str(), clip.sample.as_str()), (format!("@{}", clip.key).as_str(), "song"));
    assert_eq!((clip.at, clip.length_beats, clip.seconds_per_beat), (4.0, 4.0, 0.5));
    // Its fade out comes after it leaves: 20 ms more, which is drawn with it.
    assert!((clip.tail_beats - 0.04).abs() < 1e-12);
    assert_eq!((clip.source_start_seconds, clip.source_end_seconds), (1.0, 3.0));
    assert_eq!((clip.gain_db, clip.fade_in_ms, clip.fade_out_ms), (-3.0, 0.3, 20.0));
    assert_eq!((clip.fade_curve.as_str(), clip.source_bpm, clip.stretch.as_str()), ("equal_power", None, "repitch"));
    // The file it plays is listed once, with its length.
    let file = u.arrangement.files[0].clone();
    assert_eq!((u.arrangement.files.len(), file.identity, file.seconds, file.bpm, file.beats.len()), (1, clip.file, 10.0, None, 0));
    assert_ne!(file.identity, 0);
    assert_eq!(u.arrangement.tracks[0].clips.len(), 2, "pattern clips are a list of their own");

    // A clip without an end plays to its file's; one with a tempo of its own
    // counts its file in its own beats.
    let u = audio_add(&path, &seen, json!({"track": "perc", "at": 8}));
    let whole = u.arrangement.tracks[1].audio[0].clone();
    assert_eq!((whole.length_beats, whole.tail_beats, whole.source_end_seconds, whole.file), (20.0, 0.0, 10.0, file.identity));
    assert_eq!(u.arrangement.files.len(), 1);
    agent(&path, json!({"op": "set", "path": format!("@{}.source_bpm", whole.key), "value": 60}));
    let u = update(&seen);
    let slow = &u.arrangement.tracks[1].audio[0];
    assert_eq!((slow.seconds_per_beat, slow.length_beats, slow.source_bpm), (1.0, 10.0, Some(60.0)));
    assert_eq!(u.touched, [touch(Part::Clip, whole.key, Delta::Changed)]);

    // An agent's move to another track keeps the clip's key, and touches the
    // clip and neither track.
    agent(&path, json!({"op": "audio.move", "clip": clip.reference, "track": "perc", "at": 20}));
    let u = update(&seen);
    assert_eq!(u.touched, [touch(Part::Clip, clip.key, Delta::Changed)]);
    assert_eq!(u.arrangement.tracks[1].audio.iter().map(|c| (c.key, c.at)).collect::<Vec<_>>(), [(whole.key, 8.0), (clip.key, 20.0)]);
    assert!(u.arrangement.tracks[0].audio.is_empty());

    // The file's peaks come once, by its identity, at the file's own rate.
    let revision = u.change.revision;
    let mut peaks = Vec::new();
    let w = loop {
        let mut w = waves.recv_timeout(Duration::from_secs(10)).expect("waveforms");
        peaks.append(&mut w.file_peaks);
        if w.revision == revision {
            break w;
        }
    };
    assert_eq!(w.files, [file.identity]);
    assert_eq!(peaks.len(), 1, "sent once, however many revisions play the file");
    assert_eq!((peaks[0].identity, peaks[0].frames_per_second, peaks[0].frames), (file.identity, 48000.0, 480_000));
    assert_eq!((peaks[0].levels[0].frames_per_bucket, peaks[0].levels[0].data.len()), (64, 2 * 7500));
    // The file gets louder: its end peaks higher than its start.
    let finest = &peaks[0].levels[0].data;
    assert!((finest[2 * 7400 + 1] as i8) > (finest[2 * 100 + 1] as i8));
    // Both tracks have pattern clips, so both still have peaks of their own.
    assert_eq!(w.tracks.iter().map(|t| t.track).collect::<Vec<_>>(), [drums, u.arrangement.tracks[1].key]);

    // A track of audio clips alone has none: its clips draw the file.
    agent(&path, json!({"op": "track.add", "id": "edit"}));
    update(&seen);
    let u = audio_add(&path, &seen, json!({"track": "edit", "at": 0, "source_end_seconds": 2}));
    let edit = u.arrangement.tracks[2].key;
    let w = loop {
        let w = waves.recv_timeout(Duration::from_secs(10)).expect("waveforms");
        assert!(w.file_peaks.is_empty());
        if w.revision == u.change.revision {
            break w;
        }
    };
    assert!(!w.tracks.iter().any(|t| t.track == edit));

    // A beat map beside the file marks its beats, from the next change on.
    let map = |sha: &str| {
        json!({
            "sha256": sha, "tempo": {"bpm": 120.0},
            "beats": [
                {"seconds": 0.25, "bar": 0, "beat": 4}, {"seconds": 0.75, "bar": 1, "beat": 1},
                {"seconds": 1.25, "bar": 1, "beat": 2}, {"seconds": 2.75, "bar": 2, "beat": 1},
            ],
        })
        .to_string()
    };
    std::fs::write(dir.path().join("song.beats.json"), map("anything")).unwrap();
    agent(&path, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -1}));
    let u = update(&seen);
    let mapped = &u.arrangement.files[0];
    assert_eq!((mapped.identity, mapped.bpm), (file.identity, Some(120.0)));
    assert_eq!(mapped.beats.iter().map(|b| (b.seconds, b.downbeat)).collect::<Vec<_>>(), [(0.25, false), (0.75, true), (1.25, false), (2.75, true)]);
    // A map of other audio than the song lists says nothing of this file.
    let sha = aaw_model::digest(&dir.path().join("song.wav")).unwrap();
    agent(&path, json!({"op": "set", "path": "samples.song.sha256", "value": sha}));
    let u = update(&seen);
    assert_eq!((u.arrangement.files[0].bpm, u.arrangement.files[0].beats.len()), (None, 0));
    std::fs::write(dir.path().join("song.beats.json"), map(&sha)).unwrap();
    agent(&path, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -2}));
    assert_eq!(update(&seen).arrangement.files[0].beats.len(), 4);

    // A clip whose file is gone still has a place, and no file.
    std::fs::remove_file(dir.path().join("song.wav")).unwrap();
    agent(&path, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -3}));
    let u = update(&seen);
    assert!(u.arrangement.files.is_empty());
    let lost = &u.arrangement.tracks[2].audio[0];
    assert_eq!((lost.file, lost.length_beats), (0, 4.0));
    song.close();
}

#[test]
fn audio_clips_move_trim_fade_and_split_as_they_are_dragged() {
    let (dir, path, song, seen) = open();
    transport(&seen);
    with_song_file(dir.path(), &path, &seen);
    let start = song.arrangement();
    let (drums, perc) = (start.tracks[0].key, start.tracks[1].key);
    let u = audio_add(&path, &seen, json!({"track": "drums", "at": 4, "source_start_seconds": 1, "source_end_seconds": 3}));
    let clip = u.arrangement.tracks[0].audio[0].key;
    let audio = |u: &Update, track: usize| u.arrangement.tracks[track].audio.clone();
    let get = |what: &str| agent(&path, json!({"op": "get", "path": what}));

    // A drag moves it in time and to another track, with its key: one step.
    assert!(song.edit(Edit::ClipsMove { clips: vec![clip], by: 2.5, rows: 1 }, None).unwrap().is_empty());
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), u.change.label.as_str()), ("audio.move", "Move audio clip song to beat 6.5 on perc"));
    assert_eq!(audio(&u, 1).iter().map(|c| (c.key, c.at, c.source_start_seconds)).collect::<Vec<_>>(), [(clip, 6.5, 1.0)]);
    assert!(song.edit(Edit::ClipsMove { clips: vec![clip], by: 0.0, rows: 0 }, None).unwrap().is_empty());
    assert!(song.edit(Edit::ClipsMove { clips: vec![clip], by: 0.0, rows: 1 }, None).is_err(), "no track below");
    // With a pattern clip, each moves as its kind does.
    let fill = start.tracks[0].clips[1].key;
    song.edit(Edit::ClipsMove { clips: vec![fill, clip], by: -2.0, rows: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), u.change.label.as_str()), ("batch", "Move 2 clips"));
    assert_eq!((u.arrangement.tracks[0].clips[1].at, audio(&u, 1)[0].at), (14.0, 4.5));

    // A clip that would end past the song's end lengthens the song to the end
    // of that bar, in the same step.
    song.edit(Edit::ClipsMove { clips: vec![clip], by: 25.0, rows: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), u.change.label.as_str()), ("batch", "Move audio clip song"));
    assert_eq!((audio(&u, 1)[0].at, u.arrangement.length_beats), (29.5, 36.0));
    assert!(u.touched.contains(&touch(Part::Session, 0, Delta::Changed)));
    song.undo().unwrap();
    let u = update(&seen);
    assert_eq!((audio(&u, 1)[0].at, u.arrangement.length_beats), (4.5, 32.0));
    // Ending on the song's end is not past it.
    song.edit(Edit::ClipsMove { clips: vec![clip], by: 23.5, rows: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), audio(&u, 1)[0].at, u.arrangement.length_beats), ("audio.move", 28.0, 32.0));
    song.undo().unwrap();
    update(&seen);

    // An edge moves to a beat and the audio stays where it is: the file's
    // second 1 is on beat 4.5, so beat 3.5 is half a second earlier in it.
    song.edit(Edit::AudioTrim { clip, start: Some(3.5), end: None }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Trim audio clip song at 4.5");
    let c = &audio(&u, 1)[0];
    assert_eq!((c.at, c.source_start_seconds, c.source_end_seconds, c.length_beats), (3.5, 0.5, 3.0, 5.0));
    // No further than the file goes: it starts on beat 2.5 and ends on 22.5.
    song.edit(Edit::AudioTrim { clip, start: Some(0.0), end: Some(40.0) }, None).unwrap();
    let c = audio(&update(&seen), 1)[0].clone();
    assert_eq!((c.at, c.source_start_seconds, c.source_end_seconds, c.length_beats), (2.5, 0.0, 10.0, 20.0));
    assert!(song.edit(Edit::AudioTrim { clip, start: None, end: None }, None).unwrap().is_empty());
    assert!(song.edit(Edit::AudioTrim { clip, start: Some(30.0), end: None }, None).is_err(), "it would end before it starts");
    assert!(song.edit(Edit::AudioTrim { clip: fill, start: Some(3.0), end: None }, None).is_err(), "a pattern clip has no audio to trim");
    // An end dragged past the song's end takes the song with it.
    song.edit(Edit::ClipsMove { clips: vec![clip], by: 13.5, rows: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((audio(&u, 1)[0].at, u.arrangement.length_beats), (16.0, 36.0));
    // An end is where the sound ends: the clip leaves its 8 ms fade out
    // before the beat, so the fade is over on it.
    song.edit(Edit::AudioTrim { clip, start: None, end: Some(24.0) }, None).unwrap();
    let u = update(&seen);
    let c = &audio(&u, 1)[0];
    assert_eq!(u.change.op, "audio.trim");
    assert!((c.length_beats - 7.984).abs() < 1e-9 && (c.tail_beats - 0.016).abs() < 1e-9 && (c.source_end_seconds - 3.992).abs() < 1e-9);
    song.edit(Edit::AudioTrim { clip, start: None, end: Some(36.0) }, None).unwrap();
    let u = update(&seen);
    assert_eq!((audio(&u, 1)[0].length_beats, u.arrangement.length_beats), (20.0, 36.0));
    song.edit(Edit::AudioTrim { clip, start: Some(26.0), end: None }, None).unwrap();
    update(&seen);
    song.edit(Edit::ClipsMove { clips: vec![clip], by: 9.0, rows: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((audio(&u, 1)[0].at, audio(&u, 1)[0].length_beats, u.arrangement.length_beats), (35.0, 10.0, 48.0));

    // Fades, to a tenth of a millisecond; both at once are one step.
    song.edit(Edit::AudioFade { clip, fade_in_ms: Some(250.04), fade_out_ms: None }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.label.as_str(), audio(&u, 1)[0].fade_in_ms, audio(&u, 1)[0].fade_out_ms), ("Set the fade in of audio clip song", 250.0, 8.0));
    song.edit(Edit::AudioFade { clip, fade_in_ms: Some(-4.0), fade_out_ms: Some(1500.0) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Set the fades of audio clip song");
    assert_eq!((audio(&u, 1)[0].fade_in_ms, audio(&u, 1)[0].fade_out_ms), (0.0, 1500.0));

    assert!((0..2).all(|_| song.undo().is_ok() && update(&seen).change.op == "undo"));
    // A fade out is drawn inside the clip: the sound still ends where it did,
    // and the clip leaves the fade's length before that. Here the file goes
    // on past the clip, so the end moves a beat earlier for 500 ms of fade.
    song.edit(Edit::AudioTrim { clip, start: None, end: Some(40.0) }, None).unwrap();
    update(&seen);
    song.edit(Edit::AudioFade { clip, fade_in_ms: None, fade_out_ms: Some(500.0) }, None).unwrap();
    let u = update(&seen);
    let c = &audio(&u, 1)[0];
    assert_eq!(u.change.label, "Set the fade out of audio clip song");
    assert!((c.length_beats - 4.0).abs() < 1e-9 && (c.tail_beats - 1.0).abs() < 1e-9 && c.fade_out_ms == 500.0, "{c:?}");
    // A shorter fade gives the beats back, and one longer than the clip is refused.
    song.edit(Edit::AudioFade { clip, fade_in_ms: None, fade_out_ms: Some(0.0) }, None).unwrap();
    let c = audio(&update(&seen), 1)[0].clone();
    assert!((c.length_beats - 5.0).abs() < 1e-9 && c.tail_beats == 0.0);
    song.edit(Edit::AudioFade { clip, fade_in_ms: None, fade_out_ms: Some(500.0) }, None).unwrap();
    update(&seen);
    let long = song.edit(Edit::AudioFade { clip, fade_in_ms: None, fade_out_ms: Some(4000.0) }, None).unwrap_err();
    assert_eq!(long.to_string(), "A fade out of 4000 ms is longer than audio clip song");
    // An end dragged in until the fade no longer fits shortens the fade to
    // half of what is left: a beat of clip, 250 ms of it fade.
    song.edit(Edit::AudioTrim { clip, start: None, end: Some(36.0) }, None).unwrap();
    let u = update(&seen);
    let c = &audio(&u, 1)[0];
    assert_eq!((u.change.op.as_str(), u.change.label.as_str(), c.fade_out_ms), ("batch", "Trim audio clip song", 250.0));
    assert!((c.length_beats - 0.5).abs() < 1e-9 && (c.tail_beats - 0.5).abs() < 1e-9);
    assert!((0..5).all(|_| song.undo().is_ok() && update(&seen).change.op == "undo"));
    let c = song.arrangement().tracks[1].audio[0].clone();
    assert_eq!((c.at, c.length_beats, c.fade_out_ms), (35.0, 10.0, 8.0));

    // The clip panel's fields.
    let set = |field: &str, value: FieldValue| song.edit(Edit::AudioSet { clip, field: field.into(), value }, None);
    set("gain_db", number(-6.5)).unwrap();
    assert_eq!(audio(&update(&seen), 1)[0].gain_db, -6.5);
    set("fade_curve", text("linear")).unwrap();
    assert_eq!(audio(&update(&seen), 1)[0].fade_curve, "linear");
    set("stretch", text("preserve_pitch")).unwrap();
    assert_eq!(audio(&update(&seen), 1)[0].stretch, "preserve_pitch");
    // At half the song's tempo of its own, its five seconds are five beats.
    set("source_bpm", number(60.0)).unwrap();
    let u = update(&seen);
    assert_eq!((audio(&u, 1)[0].source_bpm, audio(&u, 1)[0].length_beats, u.arrangement.length_beats), (Some(60.0), 5.0, 48.0));
    // At four times it they are forty, and the song grows to hold them.
    set("source_bpm", number(240.0)).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Set the tempo of audio clip song");
    assert_eq!((audio(&u, 1)[0].length_beats, u.arrangement.length_beats), (20.0, 56.0));
    set("source_bpm", FieldValue::Absent).unwrap();
    let u = update(&seen);
    assert_eq!((audio(&u, 1)[0].source_bpm, audio(&u, 1)[0].length_beats), (None, 10.0));
    assert_eq!(get(&format!("@{clip}")).get("source_bpm"), None);
    assert!(set("gain_db", number(99.0)).is_err());
    assert_eq!(set("sample", text("hit")).unwrap_err().to_string(), "An audio clip's sample is not set here");
    assert!(set("fade_out_ms", number(4.0)).is_err(), "fades are set where the clip's end is kept");

    // A split makes two that play as the one did; the later half is new.
    let made = song.edit(Edit::AudioSplit { clips: vec![clip, fill], at: 39.0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.op, "audio.split");
    let halves = audio(&u, 1);
    assert_eq!(halves.iter().map(|c| (c.key, c.at, c.length_beats)).collect::<Vec<_>>(), [(clip, 35.0, 4.0), (made[0], 39.0, 6.0)]);
    assert_eq!((halves[0].source_end_seconds, halves[1].source_start_seconds), (halves[1].source_start_seconds, 7.0));
    let outside = song.edit(Edit::AudioSplit { clips: vec![clip, made[0]], at: 39.0 }, None).unwrap_err();
    assert_eq!(outside.to_string(), "No audio clip to split plays at the start position");

    // A copy goes right after what was copied, and can lengthen the song;
    // removing a clip says what it was.
    let copies = song.edit(Edit::ClipsDuplicate { clips: vec![clip, made[0]] }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.label.as_str(), u.arrangement.length_beats), ("Duplicate 2 clips", 56.0));
    let all = audio(&u, 1);
    assert_eq!(all.iter().map(|c| (c.key, c.at, c.length_beats)).collect::<Vec<_>>()[2..], [(copies[0], 45.0, 4.0), (copies[1], 49.0, 6.0)]);
    assert_eq!((all[3].gain_db, all[3].fade_curve.as_str(), all[3].stretch.as_str()), (-6.5, "linear", "preserve_pitch"));
    song.edit(Edit::ClipsDuplicate { clips: vec![copies[1]] }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.label.as_str(), u.arrangement.length_beats), ("Duplicate audio clip song", 64.0));
    song.edit(Edit::ClipsRemove { clips: vec![copies[1]] }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.label.as_str(), u.touched.as_slice()), ("Remove audio clip song", &[touch(Part::Clip, copies[1], Delta::Removed)][..]));
    song.edit(Edit::ClipsRemove { clips: vec![clip, fill] }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.label.as_str(), audio(&u, 1).len(), u.arrangement.tracks[0].clips.len()), ("Remove 2 clips", 3, 1));

    // A clip's length comes from seconds of its file, and a copy still lands
    // on a beat a person would write: this one sounds for 7.984 beats and
    // 0.016 of fade, and its copy starts on beat 8.
    let u = audio_add(&path, &seen, json!({"track": "drums", "at": 0, "source_end_seconds": 3.992}));
    let short = u.arrangement.tracks[0].audio[0].key;
    let copy = song.edit(Edit::ClipsDuplicate { clips: vec![short] }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), u.change.label.as_str()), ("batch", "Duplicate audio clip song"));
    assert_eq!(get(&format!("@{}.at", copy[0])), json!(8));
    song.edit(Edit::ClipsMove { clips: vec![copy[0]], by: 4.0, rows: 0 }, None).unwrap();
    assert_eq!(update(&seen).change.label, "Move audio clip song to beat 12 on drums");
    let _ = (drums, perc);
    song.close();
}

#[test]
fn a_dropped_file_becomes_an_audio_clip_and_the_song_grows_to_hold_it() {
    let (dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let drums = start.tracks[0].key;
    // A file as the library's import leaves it: copied into the project. Ten
    // seconds, which is twenty beats at the song's 120 BPM.
    std::fs::create_dir(dir.path().join("samples")).unwrap();
    let frames: Vec<[f32; 2]> = (0..441_000).map(|i| [((i % 441) as f32 / 441.0 - 0.5) * 0.4; 2]).collect();
    std::fs::write(dir.path().join("samples/ab12_song.wav"), float_wav_bytes(&frames, 44100)).unwrap();
    let asset = aaw_ffi::library::Asset {
        sha256: aaw_model::digest(&dir.path().join("samples/ab12_song.wav")).unwrap(),
        path: "samples/ab12_song.wav".into(),
        source: "/music/My Song.wav".into(),
        source_sha256: None,
        root_note: None,
    };
    let drop = |track: Option<u64>, at: f64| Edit::SampleClip { asset: asset.clone(), name: "My Song".into(), track, index: 2, at };

    // On a track's lane: a clip of the whole file at that beat, in one step
    // with the sample it plays.
    let made = song.edit(drop(Some(drums), 8.0), None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.origin, u.change.op.as_str(), u.change.label.as_str()), (Who::User, "batch", "Add audio clip of my-song to drums"));
    let clip = u.arrangement.tracks[0].audio[0].clone();
    assert_eq!(made, [clip.key]);
    assert_eq!((clip.sample.as_str(), clip.at, clip.length_beats, clip.source_start_seconds, clip.source_end_seconds), ("my-song", 8.0, 20.0, 0.0, 10.0));
    assert_eq!(u.arrangement.length_beats, 32.0, "it fits");
    assert_eq!(u.arrangement.tracks[0].pads.len(), 1, "no pad is made");
    assert_eq!(
        agent(&path, json!({"op": "get", "path": "samples.my-song"})),
        json!({"path": "samples/ab12_song.wav", "sha256": asset.sha256, "source": "/music/My Song.wav"})
    );

    // Past where it fits, the song grows to the end of the bar it ends in:
    // from beat 14.5 its twenty beats end at 34.5. The file is the same sample.
    song.edit(drop(Some(drums), 14.5), None).unwrap();
    let u = update(&seen);
    assert_eq!((u.arrangement.length_beats, u.arrangement.tracks[0].audio[1].at), (36.0, 14.5));
    assert_eq!(agent(&path, json!({"op": "inspect"}))["samples"], json!(2));
    song.undo().unwrap();
    let u = update(&seen);
    assert_eq!((u.arrangement.length_beats, u.arrangement.tracks[0].audio.len()), (32.0, 1));

    // Where there is no track: a new one named after the file, with the clip,
    // even at a beat past the song's end.
    let made = song.edit(drop(None, 40.0), None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Add track my-song with audio clip my-song");
    let track = &u.arrangement.tracks[2];
    assert_eq!((track.id.as_str(), track.pads.len(), track.clips.len()), ("my-song", 0, 0));
    assert_eq!(made, [track.key, track.audio[0].key]);
    assert_eq!((track.audio[0].at, u.arrangement.length_beats), (40.0, 60.0));
    // One undo takes back the track, the clip and the length.
    song.undo().unwrap();
    let u = update(&seen);
    assert_eq!((u.arrangement.tracks.len(), u.arrangement.length_beats), (2, 32.0));
    song.close();
}

fn pattern<'a>(a: &'a aaw_ffi::Arrangement, name: &str) -> &'a PatternView {
    a.patterns.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no pattern {name}"))
}

/// The song with a pad that is held, a pad with a pitch, and a pattern that
/// plays them with events.
fn open_with_notes() -> (tempfile::TempDir, PathBuf, Arc<Song>, Receiver<Seen>) {
    let (dir, path, song, seen) = open();
    transport(&seen);
    let tone: Vec<[f32; 2]> = (0..24000).map(|i| [((i as f64 * 0.03).sin() * 0.3) as f32; 2]).collect();
    std::fs::write(dir.path().join("tone.wav"), float_wav_bytes(&tone, 48000)).unwrap();
    agent(
        &path,
        json!({"op": "batch", "commands": [
            {"op": "set", "path": "samples.tone", "value": {"path": "tone.wav", "root_note": "A2"}},
            {"op": "track.add", "id": "bass"},
            {"op": "pad.add", "track": "bass", "pad": "t", "sample": "tone", "mode": "gate"},
            {"op": "pad.add", "track": "bass", "pad": "k", "sample": "hit"},
            {"op": "pattern.add", "pattern": "line", "length_beats": 4, "events": [
                {"at": 0, "pad": "t", "note": "C3", "duration": 1},
                {"at": "4/3", "pad": "t", "duration": "2/3", "velocity": 80},
                {"at": 2.5, "pad": "k"},
            ]},
            {"op": "clip.add", "track": "bass", "pattern": "line", "at": 8, "repeats": 2},
        ]}),
    );
    update(&seen);
    (dir, path, song, seen)
}

#[test]
fn a_pattern_is_what_the_editor_draws() {
    let (_dir, _path, song, _seen) = open_with_notes();
    let a = song.arrangement();
    assert_eq!(a.patterns.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["beat", "fill", "line"]);
    let beat = pattern(&a, "beat");
    assert_eq!((beat.length_beats, beat.grid, beat.grid_text.as_str(), beat.swing, beat.steps, beat.clips), (4.0, 0.25, "1/4", 0.5, Some(16), 2));
    assert_eq!(beat.length_text, "4");
    assert_eq!(beat.rows.len(), 1);
    assert_eq!((beat.rows[0].pad.as_str(), &beat.rows[0].cells[..4]), ("h", &[10u8, 0, 10, 0][..]));
    assert!(beat.events.is_empty());

    let line = pattern(&a, "line");
    assert_eq!((line.rows.len(), line.clips), (0, 1));
    let events: Vec<_> = line.events.iter().map(|e| (e.at, e.pad.as_str(), e.velocity, e.note.as_deref(), e.pitch, e.duration)).collect();
    assert_eq!(
        events,
        [
            (0.0, "t", 100, Some("C3"), Some(48), Some(1.0)),
            (4.0 / 3.0, "t", 80, None, None, Some(2.0 / 3.0)),
            (2.5, "k", 100, None, None, None),
        ]
    );
    assert!(line.events.iter().all(|e| e.key != 0));
    let written: Vec<_> = line.events.iter().map(|e| (e.at_text.as_str(), e.duration_text.as_deref())).collect();
    assert_eq!(written, [("0", Some("1")), ("4/3", Some("2/3")), ("2.5", None)]);
    // A pad says whether it is held and what pitch its sample has.
    let pads: Vec<_> = a.tracks[2].pads.iter().map(|p| (p.name.as_str(), p.gate, p.root)).collect();
    assert_eq!(pads, [("t", true, Some(45)), ("k", false, None)]);
    assert_eq!((aaw_ffi::note_name(45), aaw_ffi::note_name(61), aaw_ffi::note_name(200)), ("A2".into(), "C#4".into(), String::new()));
    song.close();
}

#[test]
fn steps_are_set_on_a_pattern_s_grid() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let steps = |pad: &str, steps: Vec<u32>, level: u32| Edit::Steps { pattern: "beat".into(), pad: pad.into(), steps, level };
    let row = |pad: &str| agent(&path, json!({"op": "get", "path": format!("patterns.beat.steps.{pad}")}));

    // Steps go on and off where they are, and a row keeps how it was written.
    agent(&path, json!({"op": "pattern.steps", "pattern": "beat", "pad": "h", "row": "x.x. x.x. | x.x. x.x."}));
    update(&seen);
    song.edit(steps("h", vec![1, 2], 10), None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.origin, u.change.label.as_str()), (Who::User, "Set beat steps for h"));
    assert_eq!(row("h"), json!("xxx. x.x. | x.x. x.x."));
    song.edit(steps("h", vec![0, 8], 0), None).unwrap();
    song.edit(steps("h", vec![15], 3), None).unwrap();
    update(&seen);
    let u = update(&seen);
    assert_eq!(row("h"), json!(".xx. x.x. | ..x. x.x3"));
    assert_eq!(pattern(&u.arrangement, "beat").rows[0].cells[12..], [10, 0, 10, 3]);
    // Both clips of the pattern play the new steps.
    assert_eq!(u.touched.iter().filter(|t| t.part == Part::Clip && t.delta == Delta::Changed).count(), 2);

    // A row left without steps goes; clearing nothing is no change.
    song.edit(steps("h", (0..16).collect(), 0), None).unwrap();
    assert!(pattern(&update(&seen).arrangement, "beat").rows.is_empty());
    assert!(song.edit(steps("h", vec![3], 0), None).unwrap().is_empty());
    // A pad's first step writes its row, a beat to a group.
    song.edit(steps("h", vec![0, 6], 10), None).unwrap();
    update(&seen);
    assert_eq!(row("h"), json!("x... ..x. .... ...."));
    // No such step, level or pad.
    assert!(song.edit(steps("h", vec![16], 10), None).unwrap_err().to_string().contains("has 16 steps"));
    assert!(song.edit(steps("h", vec![0], 11), None).is_err());
    assert_eq!(song.edit(steps("nope", vec![0], 10), None).unwrap_err().to_string(), "drums: unknown pad nope");
    song.close();
}

#[test]
fn events_are_added_moved_and_shaped() {
    let (_dir, path, song, seen) = open_with_notes();
    let get = |key: u64| agent(&path, json!({"op": "get", "path": format!("@{key}")}));
    let fields = |key: u64| {
        let mut j = get(key);
        j.as_object_mut().unwrap().remove("ref");
        j
    };
    let line = pattern(&song.arrangement(), "line").clone();
    let (first, second, kick) = (line.events[0].key, line.events[1].key, line.events[2].key);

    // On the grid a new event is on an exact step, held for as many steps as asked; off it, where it was put.
    let add = |pad: &str, at: f64, free: bool, pitch: Option<i32>, steps: u32| Edit::EventAdd { pattern: "line".into(), pad: pad.into(), at, free, pitch, steps };
    let made = song.edit(add("t", 3.26, false, Some(52), 2), None).unwrap()[0];
    let u = update(&seen);
    assert_eq!((u.change.origin, u.change.label.as_str()), (Who::User, "Add event to line"));
    assert_eq!(fields(made), json!({"at": 3.25, "pad": "t", "note": "E3", "duration": 0.5}));
    assert_eq!(pattern(&u.arrangement, "line").events[3].pitch, Some(52));
    let free = song.edit(add("k", 1.2345678, true, None, 0), None).unwrap()[0];
    update(&seen);
    assert_eq!(fields(free), json!({"at": 1.2345678, "pad": "k"}));
    // The last step is the latest an event can start.
    let late = song.edit(add("k", 4.0, false, None, 0), None).unwrap()[0];
    update(&seen);
    assert_eq!(fields(late)["at"], json!(3.75));
    // A held pad needs a length, and a pitch needs a root note.
    assert_eq!(song.edit(add("t", 0.0, false, None, 0), None).unwrap_err().to_string(), "bass.t: gated events need positive duration");
    assert_eq!(song.edit(add("k", 0.0, false, Some(60), 0), None).unwrap_err().to_string(), "hit needs root_note for pitched events");

    // A move is by exact steps: an event on a third stays on thirds.
    song.edit(Edit::EventMove { event: second, steps: 2, by: 0.0, semitones: 0 }, None).unwrap();
    update(&seen);
    assert_eq!(get(second)["at"], json!("11/6"));
    song.edit(Edit::EventMove { event: second, steps: -1, by: 0.125, semitones: 0 }, None).unwrap();
    update(&seen);
    assert_eq!(get(second)["at"], json!("41/24"));
    // In pitch it moves from its note, or from its sample's root when it has none.
    song.edit(Edit::EventMove { event: first, steps: 0, by: 0.0, semitones: -5 }, None).unwrap();
    update(&seen);
    assert_eq!(get(first)["note"], json!("G2"));
    song.edit(Edit::EventMove { event: second, steps: 0, by: 0.0, semitones: 3 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((get(second)["note"].clone(), pattern(&u.arrangement, "line").events[1].pitch), (json!("C3"), Some(48)));
    assert!(song.edit(Edit::EventMove { event: first, steps: 0, by: 0.0, semitones: 0 }, None).unwrap().is_empty());
    assert!(song.edit(Edit::EventMove { event: first, steps: -1, by: 0.0, semitones: 0 }, None).unwrap_err().to_string().contains("before its pattern"));
    assert!(song.edit(Edit::EventMove { event: first, steps: 99, by: 0.0, semitones: 0 }, None).unwrap_err().to_string().contains("outside pattern"));
    assert!(song.edit(Edit::EventMove { event: kick, steps: 0, by: 0.0, semitones: 1 }, None).unwrap_err().to_string().contains("without a root note"));
    assert!(song.edit(Edit::EventMove { event: first, steps: 0, by: 0.0, semitones: 120 }, None).is_err());

    // Its end goes to a line of the grid; it cannot end before it starts.
    song.edit(Edit::EventEnd { event: first, step: 6 }, None).unwrap();
    update(&seen);
    assert_eq!(get(first)["duration"], json!(1.5));
    song.edit(Edit::EventEnd { event: second, step: 8 }, None).unwrap();
    update(&seen);
    assert_eq!(get(second)["duration"], json!("7/24"));
    assert!(song.edit(Edit::EventEnd { event: second, step: 6 }, None).is_err());

    // Typed values are written as typed; an empty one takes the field away.
    let set = |event: u64, at: Option<&str>, duration: Option<&str>, velocity: Option<u32>, transpose: Option<f64>, note: Option<&str>| Edit::EventSet {
        event,
        at: at.map(str::to_string),
        duration: duration.map(str::to_string),
        velocity,
        transpose,
        note: note.map(str::to_string),
    };
    song.edit(set(kick, Some("7/3"), Some(" 0.5 "), Some(64), Some(-12.0), None), None).unwrap();
    update(&seen);
    assert_eq!(fields(kick), json!({"at": "7/3", "pad": "k", "velocity": 64, "duration": 0.5, "transpose": -12.0}));
    song.edit(set(kick, Some("2"), Some(""), None, Some(0.0), None), None).unwrap();
    update(&seen);
    assert_eq!(fields(kick), json!({"at": 2, "pad": "k", "velocity": 64}));
    song.edit(set(first, None, None, None, None, Some("")), None).unwrap();
    update(&seen);
    assert_eq!(get(first).get("note"), None);
    assert!(song.edit(set(first, None, None, Some(200), None, None), None).is_err());
    assert!(song.edit(set(first, None, None, None, None, Some("H2")), None).is_err());

    // A velocity drag is one undo step.
    for velocity in [70, 90, 110] {
        song.edit(set(first, None, None, Some(velocity), None, None), Some("velocity".into())).unwrap();
    }
    update(&seen);
    update(&seen);
    assert_eq!(update(&seen).undo.unwrap().label, "Change event of line: t at 0");
    song.undo().unwrap();
    assert_eq!(pattern(&update(&seen).arrangement, "line").events[0].velocity, 100);

    song.edit(Edit::EventRemove { event: free }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Remove event of line: k at 1.2345678");
    assert!(pattern(&u.arrangement, "line").events.iter().all(|e| e.key != free));
    assert!(song.edit(Edit::EventRemove { event: free }, None).is_err());
    assert!(song.edit(Edit::EventMove { event: free, steps: 1, by: 0.0, semitones: 0 }, None).is_err());

    // What the person selects in the editor, the agent can ask about.
    song.select(vec![first]).unwrap();
    assert_eq!(agent(&path, json!({"op": "status"}))["selection"], json!([{"ref": format!("@{first}"), "path": "patterns.line.events.0"}]));
    song.close();
}

#[test]
fn a_pattern_s_length_grid_and_swing_change_with_its_steps() {
    let (_dir, path, song, seen) = open_with_notes();
    let steps = |name: &str| agent(&path, json!({"op": "get", "path": format!("patterns.{name}.steps")}));
    let length = |pattern: &str, beats: &str| Edit::PatternLength { pattern: pattern.into(), beats: beats.into() };
    let grid = |pattern: &str, grid: &str| Edit::PatternGrid { pattern: pattern.into(), grid: grid.into() };

    song.edit(Edit::PatternSwing { pattern: "fill".into(), swing: 0.62 }, None).unwrap();
    assert_eq!(pattern(&update(&seen).arrangement, "fill").swing, 0.62);
    assert!(song.edit(Edit::PatternSwing { pattern: "fill".into(), swing: 0.9 }, None).is_err());
    assert!(song.edit(Edit::PatternSwing { pattern: "nope".into(), swing: 0.6 }, None).is_err());

    // A shorter pattern keeps the steps and events that still fit, as one step.
    song.edit(length("fill", "1"), None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), u.change.label.as_str()), ("batch", "Set the length of pattern fill to 1 beats"));
    assert_eq!((steps("fill"), pattern(&u.arrangement, "fill").length_beats), (json!({"h": "xxxx"}), 1.0));
    song.edit(length("fill", "3/2"), None).unwrap();
    update(&seen);
    assert_eq!(steps("fill"), json!({"h": "xxxx .."}));
    song.edit(length("line", "2"), None).unwrap();
    let u = update(&seen);
    assert_eq!(pattern(&u.arrangement, "line").events.iter().map(|e| e.at).collect::<Vec<_>>(), [0.0, 4.0 / 3.0]);
    song.undo().unwrap();
    assert_eq!(pattern(&update(&seen).arrangement, "line").events.len(), 3);
    // A length that is not a whole number of steps cannot hold step rows, and
    // clips must still fit the song.
    assert!(song.edit(length("fill", "1.1"), None).unwrap_err().to_string().contains("whole number of steps"));
    assert_eq!(song.edit(length("beat", "16"), None).unwrap_err().to_string(), "drums: clip exceeds session");
    assert!(song.edit(length("fill", "3/2"), None).unwrap().is_empty());

    // A finer grid spreads the steps out; a coarser one takes them if they fall on it.
    song.edit(length("fill", "1"), None).unwrap();
    update(&seen);
    song.edit(grid("fill", "1/8"), None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Set the grid of pattern fill to 1/8 beats");
    assert_eq!((steps("fill"), pattern(&u.arrangement, "fill").steps), (json!({"h": "x.x.x.x."}), Some(8)));
    song.edit(grid("fill", "1/4"), None).unwrap();
    update(&seen);
    assert_eq!(steps("fill"), json!({"h": "xxxx"}));
    let e = song.edit(grid("fill", "1/2"), None).unwrap_err().to_string();
    assert_eq!(e, "Step 2 of h does not fall on a grid of 1/2 beats");
    // Thirds: the steps on whole beats carry over.
    song.edit(Edit::Steps { pattern: "fill".into(), pad: "h".into(), steps: vec![1, 2, 3], level: 0 }, None).unwrap();
    update(&seen);
    song.edit(grid("fill", "1/3"), None).unwrap();
    assert_eq!((update(&seen).arrangement.patterns[1].grid_text.as_str(), steps("fill")), ("1/3", json!({"h": "x.."})));
    assert!(song.edit(grid("fill", "0"), None).is_err());
    song.close();
}

#[test]
fn a_clip_gets_a_new_pattern_or_a_copy_of_its_own() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let (drums, perc) = (start.tracks[0].key, start.tracks[1].key);

    // A new clip plays a new pattern one bar long, named for its track.
    let made = song.edit(Edit::ClipNew { track: perc, at: 24.0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), u.change.label.as_str()), ("batch", "Add clip perc-1 to perc"));
    let clip = u.arrangement.tracks[1].clips.last().unwrap();
    assert_eq!((made.as_slice(), clip.pattern.as_str(), clip.at, clip.pattern_beats), (&[clip.key][..], "perc-1", 24.0, 4.0));
    assert_eq!(pattern(&u.arrangement, "perc-1").steps, Some(16));
    // Near the end it is as long as what is left of the song.
    song.edit(Edit::ClipNew { track: perc, at: 30.0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(pattern(&u.arrangement, "perc-2").length_beats, 2.0);
    assert!(song.edit(Edit::ClipNew { track: perc, at: 32.0 }, None).is_err());
    assert!(song.edit(Edit::ClipNew { track: 9999, at: 0.0 }, None).is_err());
    song.undo().unwrap();
    let u = update(&seen);
    assert!(u.arrangement.patterns.iter().all(|p| p.name != "perc-2"));

    // A clip's own pattern: a copy, which the other clips do not play.
    let second = start.tracks[0].clips[0].key;
    song.edit(Edit::ClipOwnPattern { clip: second }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Give clip beat at 0 its own pattern beat-2");
    assert_eq!(u.arrangement.tracks[0].clips[0].pattern, "beat-2");
    assert_eq!((pattern(&u.arrangement, "beat").clips, pattern(&u.arrangement, "beat-2").clips), (1, 1));
    assert_eq!(pattern(&u.arrangement, "beat-2").rows, pattern(&u.arrangement, "beat").rows);
    song.edit(Edit::ClipOwnPattern { clip: second }, None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[0].clips[0].pattern, "beat-3");
    assert_eq!(agent(&path, json!({"op": "get", "path": format!("@{drums}.clips.0.pattern")})), json!("beat-3"));
    song.close();
}

#[test]
fn a_sample_becomes_a_pad_or_a_track() {
    let (dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let drums = start.tracks[0].key;
    // A file as the library's import leaves it: copied into the project.
    std::fs::create_dir(dir.path().join("samples")).unwrap();
    let copy = |name: &str| {
        let to = format!("samples/{name}");
        std::fs::copy(dir.path().join("hit.wav"), dir.path().join(&to)).unwrap();
        aaw_ffi::library::Asset {
            sha256: aaw_model::digest(&dir.path().join(&to)).unwrap(),
            path: to,
            source: "/library/Kicks/808 Kick (Hard).wav".into(),
            source_sha256: None,
            root_note: None,
        }
    };
    let kick = copy("0123_kick.wav");
    let add = |asset: &aaw_ffi::library::Asset, name: &str, track: Option<u64>| Edit::SampleAdd { asset: asset.clone(), name: name.into(), track, index: 1 };

    // On a track it is a new pad, in one step with the sample it plays.
    assert!(song.edit(add(&kick, "Kick", Some(drums)), None).unwrap().is_empty());
    let u = update(&seen);
    assert_eq!((u.change.origin, u.change.op.as_str(), u.change.label.as_str()), (Who::User, "batch", "Add pad kick to drums"));
    assert_eq!(u.arrangement.tracks[0].pads.iter().map(|p| (p.name.as_str(), p.sample.as_str())).collect::<Vec<_>>(), [("h", "hit"), ("kick", "kick")]);
    assert_eq!(
        agent(&path, json!({"op": "get", "path": "samples.kick"})),
        json!({"path": "samples/0123_kick.wav", "sha256": kick.sha256, "source": "/library/Kicks/808 Kick (Hard).wav"})
    );
    // The same file again is the same sample, under another pad.
    song.edit(add(&kick, "kick", Some(drums)), None).unwrap();
    let u = update(&seen);
    assert_eq!(u.arrangement.tracks[0].pads.iter().map(|p| (p.name.as_str(), p.sample.as_str())).collect::<Vec<_>>()[2], ("kick-2", "kick"));
    assert_eq!(agent(&path, json!({"op": "inspect"}))["samples"], json!(2));

    // With no track it is a new track, named after it, which the edit makes.
    // A copy decoded from a compressed file brings that file's hash.
    let mut tone = copy("4567_tone.wav");
    tone.root_note = Some("C2".into());
    tone.source_sha256 = Some("ab".repeat(32));
    let made = song.edit(add(&tone, "808 Sub (C)", None), None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Add track s-808-sub-c with pad s-808-sub-c");
    assert_eq!(agent(&path, json!({"op": "get", "path": "samples.s-808-sub-c.source_sha256"})), json!("ab".repeat(32)));
    let track = &u.arrangement.tracks[1];
    assert_eq!((made.as_slice(), track.id.as_str()), (&[track.key][..], "s-808-sub-c"));
    assert_eq!(track.pads.iter().map(|p| (p.name.as_str(), p.sample.as_str(), p.root)).collect::<Vec<_>>(), [("s-808-sub-c", "s-808-sub-c", Some(36))]);
    // One undo takes back the track, the pad and the sample.
    song.undo().unwrap();
    let u = update(&seen);
    assert_eq!(u.arrangement.tracks.len(), 2);
    assert_eq!(agent(&path, json!({"op": "inspect"}))["samples"], json!(2));
    // A name taken by a track or a return gets a number.
    song.edit(add(&tone, "plate", None), None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[1].id, "plate-2");

    // A file that is not in the project is refused, and nothing changes.
    let missing = aaw_ffi::library::Asset { path: "samples/none.wav".into(), sha256: kick.sha256.clone(), source: String::new(), source_sha256: None, root_note: None };
    assert_eq!(song.edit(add(&missing, "none", Some(drums)), None).unwrap_err().to_string(), "Missing asset none");
    assert_eq!(aaw_ffi::edits::ident("  Hi-Hat #3 (Open)  "), "hi-hat-3-open");
    assert_eq!((aaw_ffi::edits::ident("808"), aaw_ffi::edits::ident("!!!"), aaw_ffi::edits::ident("Ünïcode")), ("s-808".into(), "sample".into(), "n-code".into()));
    song.close();
}

#[test]
fn the_library_is_searched_and_a_sample_copied_in() {
    // The index and the copy are Python's; without it there is nothing to ask.
    if aaw_host::python::interpreter().is_err() {
        eprintln!("skipped: no Python with agent_daw; run `uv sync`");
        return;
    }
    let (dir, path, song, seen) = open();
    transport(&seen);
    let library = dir.path().join("library");
    for folder in ["Pack/Drum_Hits/Kicks", "Pack/Loops"] {
        std::fs::create_dir_all(library.join(folder)).unwrap();
    }
    for name in ["Pack/Drum_Hits/Kicks/deep_kick_01.wav", "Pack/Drum_Hits/Kicks/soft_kick_02.wav", "Pack/Loops/drum_loop_120_Am.wav"] {
        std::fs::copy(dir.path().join("hit.wav"), library.join(name)).unwrap();
    }
    // Tones, which measure as a pitch: one named as a kick, one as nothing in particular.
    let tone: Vec<[f32; 2]> = (0..48000).map(|i| [((i as f64 * 2.0 * std::f64::consts::PI * 110.0 / 48000.0).sin() * 0.4) as f32; 2]).collect();
    for name in ["Pack/Drum_Hits/Kicks/tuned_kick_03.wav", "Pack/Drum_Hits/sub_tone.wav"] {
        std::fs::write(library.join(name), float_wav_bytes(&tone, 48000)).unwrap();
    }
    let db = dir.path().join(".daw/library.sqlite").to_string_lossy().into_owned();
    aaw_host::python::run("samples", &["--db", &db, "scan", &library.to_string_lossy()]).unwrap();
    aaw_host::python::run("samples", &["--db", &db, "analyze", "--all"]).unwrap();
    let song_path = path.to_string_lossy().into_owned();
    assert_eq!(aaw_ffi::library::library_path(song_path.clone()), Some(db.clone()));
    assert!(aaw_ffi::library::library_categories().contains(&"kick".to_string()));

    let search = |query: &str, category: Option<&str>, kind: Option<&str>| {
        let found = aaw_ffi::library::library_search(db.clone(), query.into(), category.map(str::to_string), kind.map(str::to_string), 50).unwrap();
        found.into_iter().map(|s| (s.name, s.category, s.kind, s.bpm, s.key)).collect::<Vec<_>>()
    };
    let hit = |name: &str| (name.to_string(), "kick".to_string(), "one-shot".to_string(), None, None);
    assert_eq!(search("", Some("kick"), None), [hit("deep_kick_01.wav"), hit("soft_kick_02.wav"), hit("tuned_kick_03.wav")]);
    assert_eq!(search("deep kick", None, None), [hit("deep_kick_01.wav")]);
    assert_eq!(search("", None, Some("loop")), [("drum_loop_120_Am.wav".to_string(), "other".to_string(), "loop".to_string(), Some(120), Some("Am".to_string()))]);
    assert_eq!(search("", None, None).len(), 5);
    // A measured pitch is the root note to add a sample with, unless its name says it is a drum.
    let notes = |query: &str| {
        let found = aaw_ffi::library::library_search(db.clone(), query.into(), None, None, 1).unwrap().remove(0);
        (found.note, found.root_note)
    };
    assert_eq!(notes("sub_tone"), (Some("A2".to_string()), Some("A2".to_string())));
    assert_eq!(notes("tuned_kick"), (Some("A2".to_string()), None));
    assert_eq!(notes("deep_kick"), (None, None));
    let e = aaw_ffi::library::library_search(db.clone(), String::new(), None, Some("nope".into()), 50).unwrap_err().to_string();
    assert!(e.contains("invalid choice"), "{e}");

    // The chosen file is copied into the project, and the original left alone.
    let found = aaw_ffi::library::library_search(db, "deep".into(), None, None, 1).unwrap().remove(0);
    assert_eq!((found.pack.as_str(), found.channels, found.seconds, found.note), ("Pack", 2, 0.1, None));
    let asset = aaw_ffi::library::library_import(song_path.clone(), found.path.clone(), Some("C2".into())).unwrap();
    assert!(asset.path.starts_with("samples/") && asset.path.ends_with("_deep_kick_01.wav"));
    assert_eq!((asset.source.as_str(), asset.root_note.as_deref()), (found.path.as_str(), Some("C2")));
    assert_eq!(std::fs::read(dir.path().join(&asset.path)).unwrap(), std::fs::read(&found.path).unwrap());
    assert_eq!(aaw_ffi::library::library_import(song_path.clone(), found.path.clone(), None).unwrap().path, asset.path);
    assert!(aaw_ffi::library::library_import(song_path, "/nowhere.wav".into(), None).unwrap_err().to_string().contains("nowhere.wav"));

    // As a track of the song.
    song.edit(Edit::SampleAdd { asset, name: found.category, track: None, index: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.arrangement.tracks[0].id.as_str(), u.arrangement.tracks[0].pads[0].root), ("kick", Some(36)));
    song.close();
}

/// Opens the song at a path, with what its observer is told.
fn open_at(path: &str) -> (Arc<Song>, Receiver<Seen>) {
    let (tx, rx) = channel();
    let (waves_tx, _) = channel();
    let song = Song::open(path.into(), Arc::new(Watcher(Mutex::new(tx), Mutex::new(waves_tx)))).unwrap();
    (song, rx)
}

#[test]
fn an_untitled_project_is_named_by_a_move_and_a_named_one_by_a_copy() {
    registry_dir();
    use aaw_ffi::projects::*;
    let first = project_new_untitled().unwrap();
    assert!(first.ends_with("/Untitled/Untitled/song.yaml") && project_is_untitled(first.clone()));
    assert!(first.starts_with(&project_data_dir()));
    // A folder opens as its song file does.
    let folder = Path::new(&first).parent().unwrap().to_path_buf();
    assert_eq!(project_song_file(folder.to_string_lossy().into_owned()), first);
    let (song, seen) = open_at(&folder.to_string_lossy());
    transport(&seen);
    let a = song.arrangement();
    assert_eq!((a.title.as_str(), a.tempo, a.length_beats, a.tracks.len()), ("Untitled", 120.0, 128.0, 0));
    assert_eq!((song.path(), song.is_untitled(), song.untouched()), (first.clone(), true, true));
    // While it is open it is not swept away, and a second takes the next name.
    assert_eq!(project_sweep_untitled(), Vec::<String>::new());
    assert!(Path::new(&first).exists());
    assert!(project_new_untitled().unwrap().ends_with("/Untitled/Untitled 2/song.yaml"));
    assert_eq!(project_sweep_untitled(), Vec::<String>::new());
    assert!(!folder.with_file_name("Untitled 2").exists());

    // The window in front is reported to whoever asks.
    assert_eq!(agent(Path::new(&first), json!({"op": "status"}))["front"], json!(false));
    song.set_front(true).unwrap();
    let status = agent(Path::new(&first), json!({"op": "status"}));
    assert_eq!((status["front"].clone(), status["untitled"].clone(), status["title"].clone()), (json!(true), json!(true), json!("Untitled")));

    // A track by hand and one from a terminal are in one history, and make
    // the project one that holds something.
    song.edit(Edit::TrackAdd { index: 0 }, None).unwrap();
    update(&seen);
    agent(Path::new(&first), json!({"op": "track.add", "id": "bass"}));
    assert_eq!(update(&seen).change.origin, Who::Agent);
    assert!(!song.untouched());

    // Save As… on an Untitled project moves it and names the song.
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let named = root.join("My Beat");
    let at = song.save_as(named.to_string_lossy().into_owned()).unwrap();
    assert_eq!(at, named.join("song.yaml").to_string_lossy());
    assert!(matches!(next(&seen), Seen::Moved(path) if path == at));
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), u.change.label.as_str(), u.change.origin), ("project.move", "Saved as My Beat", Who::User));
    assert_eq!(u.arrangement.title, "My Beat");
    assert!(u.touched.contains(&touch(Part::Session, 0, Delta::Changed)));
    assert_eq!((song.path(), song.is_untitled(), folder.exists()), (at.clone(), false, false));
    // Undo is still the agent's track, and a command for the old path lands.
    assert_eq!(u.undo, step("Add track bass", Who::Agent));
    let reply = client::ask(Path::new(&first), &Request::new(serde_json::from_value(json!({"op": "undo"})).unwrap(), Origin::Agent)).unwrap().unwrap();
    assert_eq!(reply.moved(), Some(Path::new(&at)));
    assert_eq!(update(&seen).arrangement.tracks.len(), 1);

    // Save As… on a project that has a name copies it; the song carries on
    // in the copy, and the original is as it was.
    let before = std::fs::read_to_string(&at).unwrap();
    let copy = root.join("My Beat 2");
    let second = song.save_as(copy.to_string_lossy().into_owned()).unwrap();
    assert!(matches!(next(&seen), Seen::Moved(path) if path == second));
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), u.arrangement.title.as_str()), ("project.copy", "My Beat 2"));
    assert_eq!(std::fs::read_to_string(&at).unwrap(), before);
    song.undo().unwrap();
    assert_eq!(update(&seen).arrangement.tracks.len(), 0);
    assert_eq!(std::fs::read_to_string(&at).unwrap(), before);
    // A name that is taken is refused, and the song stays where it is.
    let e = song.save_as(named.to_string_lossy().into_owned()).unwrap_err().to_string();
    assert!(e.contains("already there"), "{e}");
    assert_eq!(song.path(), second);

    // The original opens once its path is released, beside the copy.
    assert!(Song::open(at.clone(), Arc::new(Watcher(Mutex::new(channel().0), Mutex::new(channel().0)))).is_err());
    song.release(at.clone()).unwrap();
    let (original, _seen) = open_at(&at);
    assert_eq!((original.arrangement().title.as_str(), original.arrangement().tracks.len()), ("My Beat", 1));
    original.close();
    song.close();

    // An Untitled project that holds something is left for the person; one
    // that holds nothing is swept. Only an Untitled project is deleted.
    let kept = project_new_untitled().unwrap();
    let (song, seen) = open_at(&kept);
    transport(&seen);
    song.edit(Edit::TrackAdd { index: 0 }, None).unwrap();
    update(&seen);
    song.close();
    let blank = project_new_untitled().unwrap();
    assert_eq!(project_sweep_untitled(), [kept.clone()]);
    assert!(!Path::new(&blank).exists());
    assert!(project_delete_untitled(second).unwrap_err().to_string().contains("not an Untitled project"));
    project_delete_untitled(kept.clone()).unwrap();
    assert!(!Path::new(&kept).parent().unwrap().exists());
}

#[test]
fn a_midi_track_shows_its_note_clips_as_the_agent_makes_them() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    agent(&path, json!({"op": "batch", "label": "A phrase", "commands": [
        {"op": "track.add", "id": "keys", "type": "midi"},
        {"op": "clip.add", "track": "keys", "at": 4, "length_beats": 4, "notes": [
            {"pitch": "C4", "duration": 1, "velocity": 96}, {"pitch": 64, "at": 1.975, "duration": "1/3"},
        ]},
    ]}));
    let u = update(&seen);
    let keys = &u.arrangement.tracks[2];
    assert!(keys.midi && keys.instrument.is_none() && keys.clips.is_empty() && keys.pads.is_empty());
    let clip = &keys.note_clips[0];
    assert_eq!((clip.id.as_str(), clip.at, clip.length_beats), ("clip1", 4.0, 4.0));
    assert_eq!(clip.reference, format!("@{}", clip.key));
    let notes: Vec<_> = clip.notes.iter().map(|n| (n.id.as_str(), n.pitch, n.at, n.velocity)).collect();
    assert_eq!(notes, [("n1", 60, 0.0, 96), ("n2", 64, 1.975, 100)]);
    assert!(!u.arrangement.tracks[0].midi);
    // A change to its notes touches the clip, which keeps its key.
    agent(&path, json!({"op": "note.transpose", "notes": [clip.reference.clone()], "by": 12}));
    let u = update(&seen);
    assert_eq!(u.touched, [touch(Part::Clip, clip.key, Delta::Changed)]);
    assert_eq!(u.arrangement.tracks[2].note_clips[0].notes[0].pitch, 72);
    // A sampler attached is named; an app edit of a note clip is refused, not a crash.
    agent(&path, json!({"op": "instrument.set", "track": "keys", "instrument": {"sampler": {"pads": {"h": {"sample": "hit"}}, "map": [{"notes": 60, "pad": "h"}]}}}));
    assert_eq!(update(&seen).arrangement.tracks[2].instrument.as_deref(), Some("sampler"));
    assert!(song.edit(Edit::ClipsMove { clips: vec![clip.key], by: 1.0, rows: 0 }, None).is_err());
    song.close();
}
