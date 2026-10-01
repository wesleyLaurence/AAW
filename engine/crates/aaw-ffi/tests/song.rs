//! The app's view of a hosted song: the arrangement it draws, the objects each
//! change touched, and the transport, driven as the app and an agent drive it.
//! No audio device is opened.

use aaw_engine::wav::float_wav_bytes;
use aaw_ffi::view::{Delta, EffectView, FieldKind, FieldValue, FieldView, LaneView, Part, SendView, Touch};
use aaw_ffi::{Edit, HistoryStep, Row, Song, SongObserver, TransportView, Update, Who};
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
