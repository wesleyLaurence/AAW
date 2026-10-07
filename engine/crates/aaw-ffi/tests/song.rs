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
            // And saved patches, so the person's library is read and not written.
            std::env::set_var("AAW_WORKSPACE", dir.path().join("workspace"));
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
    assert_eq!((a.revision, a.title.as_str(), a.tempo, a.time_signature.as_str(), a.bar_beats, a.beat_unit, a.length_beats), (0, "Demo", 120.0, "4/4", 4.0, 1.0, 32.0));
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
    assert_eq!(transport(&seen), TransportView { metronome: false, playing: false, cue: 0.0, loop_region: None });
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
    assert_eq!(transport(&seen), TransportView { metronome: false, playing: false, cue: 12.0, loop_region: None });
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
    song.edit(Edit::Solo { row: Row::Track { key: start.tracks[1].key }, on: true }, None).unwrap();
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
    let send = |to: &str, db: f64| Edit::Send { row: Row::Track { key: start.tracks[1].key }, to: to.into(), db };
    song.edit(send("plate", -9.0), None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Set send perc → plate");
    assert_eq!(u.arrangement.tracks[1].sends, [SendView { to: "plate".into(), gain_db: -9.0 }]);
    song.edit(send("plate", -8.5), None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[1].sends[0].gain_db, -8.5);
    song.edit(Edit::SendRemove { row: Row::Track { key: start.tracks[1].key }, to: "plate".into() }, None).unwrap();
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
fn clips_in_a_row_are_joined_into_one_that_plays_the_same() {
    let (_dir, _path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let (first, fill) = (start.tracks[0].clips[0].key, start.tracks[0].clips[1].key);
    // A copy of the first clip right after it, then the two joined: one clip
    // of eight repeats, the first, which the edit returns and which is left
    // selected once the other is gone.
    song.edit(Edit::ClipsMove { clips: vec![fill], by: 8.0, rows: 0 }, None).unwrap();
    update(&seen);
    let copy = song.edit(Edit::ClipsDuplicate { clips: vec![first] }, None).unwrap()[0];
    update(&seen);
    let made = song.edit(Edit::ClipsJoin { clips: vec![copy, first] }, None).unwrap();
    assert_eq!(made, [first]);
    let u = update(&seen);
    assert_eq!(u.change.label, "Join 2 clips into beat at 0");
    let joined: Vec<(u64, f64, u32)> = u.arrangement.tracks[0].clips.iter().map(|c| (c.key, c.at, c.repeats)).collect();
    assert_eq!(joined, [(first, 0.0, 8), (fill, 24.0, 1)]);
    assert!(u.touched.iter().any(|t| t.key == copy && t.delta == Delta::Removed));
    // Clips of different patterns are refused with the reason, and one clip is nothing to do.
    let e = song.edit(Edit::ClipsJoin { clips: vec![first, fill] }, None).unwrap_err().to_string();
    assert_eq!(e, "fill at 24 does not start where beat at 0 ends, at beat 32; clips joined into one meet there");
    assert!(song.edit(Edit::ClipsJoin { clips: vec![first] }, None).unwrap().is_empty());
    song.close();
}

#[test]
fn clips_dragged_with_option_are_copied_as_far_as_a_move_goes() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let (first, fill) = (start.tracks[0].clips[0].key, start.tracks[0].clips[1].key);
    let perc_clip = start.tracks[1].clips[0].key;
    let clips = |a: &aaw_ffi::Arrangement, track: usize| -> Vec<(u64, f64, u32)> {
        a.tracks[track].clips.iter().map(|c| (c.key, c.at, c.repeats)).collect()
    };

    // A copy goes where a move would, and the clip stays.
    let made = song.edit(Edit::ClipsCopy { clips: vec![fill], by: 4.0, rows: 1 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(made.len(), 1);
    assert_eq!(clips(&u.arrangement, 0), [(first, 0.0, 4), (fill, 16.0, 1)]);
    assert_eq!(clips(&u.arrangement, 1), [(perc_clip, 8.0, 2), (made[0], 20.0, 1)]);
    song.undo().unwrap();
    update(&seen);

    // Several keep their places to each other, as one step.
    let made = song.edit(Edit::ClipsCopy { clips: vec![perc_clip, fill], by: 8.0, rows: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.op.as_str(), u.change.label.as_str()), ("batch", "Copy 2 clips"));
    assert_eq!(clips(&u.arrangement, 0), [(first, 0.0, 4), (fill, 16.0, 1), (made[1], 24.0, 1)]);
    assert_eq!(clips(&u.arrangement, 1), [(perc_clip, 8.0, 2), (made[0], 16.0, 2)]);
    assert!(song.edit(Edit::ClipsCopy { clips: vec![perc_clip, fill], by: 0.0, rows: -1 }, None).is_err());
    song.undo().unwrap();
    update(&seen);

    // Exact beats: a copy of a clip on a third stays on thirds.
    let third = agent(&path, json!({"op": "clip.add", "track": "perc", "pattern": "fill", "at": "1/3"}));
    let third: u64 = third["handle"].as_str().unwrap()[1..].parse().unwrap();
    update(&seen);
    let copy = song.edit(Edit::ClipsCopy { clips: vec![third], by: 0.125, rows: 0 }, None).unwrap()[0];
    update(&seen);
    assert_eq!(agent(&path, json!({"op": "get", "path": format!("@{copy}.at")})), json!("11/24"));

    // Nowhere is nothing; before the song, past the tracks or the end is refused.
    assert!(song.edit(Edit::ClipsCopy { clips: vec![fill], by: 0.0, rows: 0 }, None).unwrap().is_empty());
    assert!(song.edit(Edit::ClipsCopy { clips: vec![fill], by: -17.0, rows: 0 }, None).unwrap_err().to_string().contains("before the song"));
    assert!(song.edit(Edit::ClipsCopy { clips: vec![fill], by: 0.0, rows: 2 }, None).is_err());
    assert!(song.edit(Edit::ClipsCopy { clips: vec![fill], by: 15.0, rows: 0 }, None).is_err());
    song.close();
}

#[test]
fn several_automation_points_move_and_go_together() {
    let (_dir, _path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let perc = Row::Track { key: start.tracks[1].key };
    song.edit(Edit::LaneAdd { row: perc.clone(), param: "gain_db".into() }, None).unwrap();
    let lane = update(&seen).arrangement.tracks[1].lanes[0].key;
    let mut keys = Vec::new();
    for (at, value) in [(4.0, -6.0), (8.0, -12.0), (12.0, -9.0)] {
        keys.push(song.edit(Edit::PointAdd { lane, at, value }, None).unwrap()[0]);
        update(&seen);
    }
    let points = |u: &Update| u.arrangement.tracks[1].lanes[0].points.iter().map(|p| (p.at, p.value)).collect::<Vec<_>>();
    let place = |point: u64, at: f64, value: f64| aaw_ffi::PointPlace { point, at, value };

    // A drag of two points is one undo step, however many times it is sent.
    for by in [1.0, 2.0] {
        let moved = vec![place(keys[0], 4.0 + by, -6.0 + by), place(keys[1], 8.0 + by, -12.0 + by)];
        song.edit(Edit::PointsSet { points: moved }, Some("points".into())).unwrap();
    }
    update(&seen);
    let u = update(&seen);
    assert_eq!(points(&u), [(0.0, -3.0), (6.0, -4.0), (10.0, -10.0), (12.0, -9.0)]);
    assert_eq!(u.undo.unwrap().label, "Move 2 automation points");
    song.undo().unwrap();
    assert_eq!(points(&update(&seen)), [(0.0, -3.0), (4.0, -6.0), (8.0, -12.0), (12.0, -9.0)]);
    // One out of range refuses them all.
    assert!(song.edit(Edit::PointsSet { points: vec![place(keys[0], 5.0, -5.0), place(keys[1], 9.0, 99.0)] }, None).is_err());

    // Removed together; the lane goes with its last point.
    song.edit(Edit::PointsRemove { points: vec![keys[0], keys[2]] }, None).unwrap();
    let u = update(&seen);
    assert_eq!((points(&u), u.change.label.as_str()), (vec![(0.0, -3.0), (8.0, -12.0)], "Remove 2 automation points"));
    let rest: Vec<u64> = u.arrangement.tracks[1].lanes[0].points.iter().map(|p| p.key).collect();
    song.edit(Edit::PointsRemove { points: rest }, None).unwrap();
    assert!(update(&seen).arrangement.tracks[1].lanes.is_empty());
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
    let made = song.edit(Edit::TrackAdd { index: 1, midi: false }, None).unwrap();
    let u = update(&seen);
    assert_eq!(ids(&u.arrangement).0, ["drums", "track-1", "perc"]);
    assert_eq!(made, [u.arrangement.tracks[1].key]);
    let track = Row::Track { key: made[0] };
    assert_eq!(song.edit(Edit::TrackAdd { index: 99, midi: false }, None).unwrap().len(), 1);
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
    assert!(song.edit(Edit::TrackAdd { index: 0, midi: false }, None).is_err());
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
fn an_effect_is_duplicated_copied_and_pasted() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let drums = Row::Track { key: start.tracks[0].key };
    let perc = Row::Track { key: start.tracks[1].key };
    let plate = Row::Return { key: start.returns[0].key };
    let kinds = |a: &aaw_ffi::Arrangement, track: usize| a.tracks[track].effects.iter().map(|e| e.kind.clone()).collect::<Vec<_>>();

    // A compressor set up on the drums, keyed from the percussion and named.
    let compressor = song.edit(Edit::EffectAdd { row: drums.clone(), kind: "compressor".into(), index: None }, None).unwrap()[0];
    update(&seen);
    song.edit(Edit::EffectSet { effect: compressor, field: "threshold_db".into(), value: number(-24.0) }, None).unwrap();
    update(&seen);
    song.edit(Edit::EffectSet { effect: compressor, field: "sidechain".into(), value: text("perc") }, None).unwrap();
    update(&seen);
    agent(&path, json!({"op": "set", "path": format!("@{compressor}.id"), "value": "duck"}));
    update(&seen);

    // ⌘D: the copy is right after it, with its settings and no id, and is what the edit made.
    let made = song.edit(Edit::EffectDuplicate { effect: compressor }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.origin, u.change.label.as_str()), (Who::User, "Duplicate compressor on tracks.drums"));
    assert_eq!(kinds(&u.arrangement, 0), ["compressor", "compressor"]);
    let copy = &u.arrangement.tracks[0].effects[1];
    assert_eq!(made, [copy.key]);
    assert_eq!((copy.id.as_deref(), copy.bypass), (None, false));
    assert_eq!(field(copy, "threshold_db").value, number(-24.0));
    assert_eq!(field(copy, "sidechain").value, text("perc"));
    assert_eq!(u.arrangement.tracks[0].effects[0].id.as_deref(), Some("duck"));

    // An Option-drag onto a return's header: the copy keeps its id where it is free.
    let made = song.edit(Edit::EffectCopy { effect: compressor, row: plate.clone(), index: None }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Copy compressor to returns.plate");
    let on_plate = u.arrangement.returns[0].effects.last().unwrap();
    assert_eq!((made.as_slice(), on_plate.id.as_deref()), ([on_plate.key].as_slice(), Some("duck")));
    assert_eq!(field(on_plate, "threshold_db").value, number(-24.0));
    // At a place in the chain; the id is taken there now, so the copy has none.
    song.edit(Edit::EffectCopy { effect: compressor, row: plate.clone(), index: Some(0) }, None).unwrap();
    let u = update(&seen);
    let first = &u.arrangement.returns[0].effects[0];
    assert_eq!((first.kind.as_str(), first.id.as_deref()), ("compressor", None));
    // Onto the track that keys it, the host refuses the copy and says why.
    let before = kinds(&song.arrangement(), 1);
    let e = song.edit(Edit::EffectCopy { effect: compressor, row: perc.clone(), index: None }, None).unwrap_err().to_string();
    assert!(e.contains("sidechain"), "{e}");
    assert_eq!(kinds(&song.arrangement(), 1), before);

    // ⌘C then ⌘V: what Copy took is pasted after the effect has changed and after it is gone.
    let copied = song.copy_effect(compressor).unwrap();
    song.edit(Edit::EffectSet { effect: compressor, field: "threshold_db".into(), value: number(-6.0) }, None).unwrap();
    update(&seen);
    song.edit(Edit::EffectRemove { effect: compressor }, None).unwrap();
    update(&seen);
    let made = song.edit(Edit::EffectPaste { copied: copied.clone(), row: drums.clone(), index: Some(0) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Paste compressor on tracks.drums");
    let pasted = &u.arrangement.tracks[0].effects[0];
    assert_eq!(made, [pasted.key]);
    assert_eq!((pasted.id.as_deref(), field(pasted, "threshold_db").value.clone()), (Some("duck"), number(-24.0)));
    // An index past the chain's end is its end.
    song.edit(Edit::EffectPaste { copied, row: drums.clone(), index: Some(9) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(kinds(&u.arrangement, 0), ["compressor", "compressor", "compressor"]);
    assert_eq!(u.arrangement.tracks[0].effects[2].id, None);
    assert_eq!(song.edit(Edit::EffectPaste { copied: "[]".into(), row: drums.clone(), index: None }, None).unwrap_err().to_string(), "Nothing was copied");

    // A Synth's own effects are not copied this way, and an undo takes a paste back whole.
    let synth = song.edit(Edit::SynthAdd { track: None, patch: None }, None).unwrap()[0];
    update(&seen);
    song.edit(Edit::SynthEffectAdd { track: synth, kind: "chorus".into(), index: None }, None).unwrap();
    let u = update(&seen);
    let chorus = u.arrangement.tracks.iter().find(|t| t.key == synth).unwrap().synth.as_ref().unwrap().effects[0].key;
    let e = song.edit(Edit::EffectDuplicate { effect: chorus }, None).unwrap_err().to_string();
    assert!(e.contains("Synth"), "{e}");
    song.undo().unwrap();
    song.undo().unwrap();
    song.undo().unwrap();
    assert_eq!(kinds(&song.arrangement(), 0), ["compressor", "compressor"]);
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
        song.edit(Edit::PointsRemove { points: vec![point.key] }, None).unwrap();
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
    assert_eq!((e.bands, e.fields.len()), (1, 5));
    assert_eq!(e.fields.iter().map(|f| (f.name.as_str(), f.band)).collect::<Vec<_>>()[..2], [("bands.0.shape", Some(0)), ("bands.0.freq_hz", Some(0))]);
    assert_eq!((field(e, "bands.0.gain_db").value.clone(), field(e, "bands.0.q").value.clone()), (number(0.0), number(0.71)));
    for _ in 0..2 {
        song.edit(Edit::BandAdd { effect: eq, freq_hz: None, gain_db: None }, None).unwrap();
        assert_eq!(update(&seen).change.label, "Add a band to an equalizer");
    }
    // A band added where the curve was double-clicked lands there, as a bell.
    song.edit(Edit::BandAdd { effect: eq, freq_hz: Some(2500.0), gain_db: Some(-4.5) }, None).unwrap();
    let added = &update(&seen).arrangement.tracks[0].effects[0];
    assert_eq!(added.bands, 4);
    assert_eq!((field(added, "bands.3.shape").value.clone(), field(added, "bands.3.freq_hz").value.clone(), field(added, "bands.3.gain_db").value.clone()), (text("bell"), number(2500.0), number(-4.5)));
    song.edit(Edit::BandRemove { effect: eq, band: 3 }, None).unwrap();
    update(&seen);
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
    let added = song.edit(Edit::TrackAdd { index: 2, midi: false }, None).unwrap()[0];
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
    song.edit(Edit::EventsMove { events: vec![second], steps: 2, by: 0.0, semitones: 0 }, None).unwrap();
    update(&seen);
    assert_eq!(get(second)["at"], json!("11/6"));
    song.edit(Edit::EventsMove { events: vec![second], steps: -1, by: 0.125, semitones: 0 }, None).unwrap();
    update(&seen);
    assert_eq!(get(second)["at"], json!("41/24"));
    // In pitch it moves from its note, or from its sample's root when it has none.
    song.edit(Edit::EventsMove { events: vec![first], steps: 0, by: 0.0, semitones: -5 }, None).unwrap();
    update(&seen);
    assert_eq!(get(first)["note"], json!("G2"));
    song.edit(Edit::EventsMove { events: vec![second], steps: 0, by: 0.0, semitones: 3 }, None).unwrap();
    let u = update(&seen);
    assert_eq!((get(second)["note"].clone(), pattern(&u.arrangement, "line").events[1].pitch), (json!("C3"), Some(48)));
    assert!(song.edit(Edit::EventsMove { events: vec![first], steps: 0, by: 0.0, semitones: 0 }, None).unwrap().is_empty());
    assert!(song.edit(Edit::EventsMove { events: vec![first], steps: -1, by: 0.0, semitones: 0 }, None).unwrap_err().to_string().contains("before its pattern"));
    assert!(song.edit(Edit::EventsMove { events: vec![first], steps: 99, by: 0.0, semitones: 0 }, None).unwrap_err().to_string().contains("outside pattern"));
    assert!(song.edit(Edit::EventsMove { events: vec![kick], steps: 0, by: 0.0, semitones: 1 }, None).unwrap_err().to_string().contains("without a root note"));
    assert!(song.edit(Edit::EventsMove { events: vec![first], steps: 0, by: 0.0, semitones: 120 }, None).is_err());

    // Its end goes to a line of the grid; it cannot end before it starts.
    song.edit(Edit::EventEnd { event: first, step: 6 }, None).unwrap();
    update(&seen);
    assert_eq!(get(first)["duration"], json!(1.5));
    song.edit(Edit::EventEnd { event: second, step: 8 }, None).unwrap();
    update(&seen);
    assert_eq!(get(second)["duration"], json!("7/24"));
    assert!(song.edit(Edit::EventEnd { event: second, step: 6 }, None).is_err());

    // Typed values are written as typed; an empty one takes the field away.
    let set = |event: u64, at: Option<&str>, duration: Option<&str>, velocity: Option<u32>, transpose: Option<f64>, note: Option<&str>| Edit::EventsSet {
        events: vec![event],
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

    song.edit(Edit::EventsRemove { events: vec![free] }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Remove event of line: k at 1.2345678");
    assert!(pattern(&u.arrangement, "line").events.iter().all(|e| e.key != free));
    assert!(song.edit(Edit::EventsRemove { events: vec![free] }, None).is_err());
    assert!(song.edit(Edit::EventsMove { events: vec![free], steps: 1, by: 0.0, semitones: 0 }, None).is_err());

    // What the person selects in the editor, the agent can ask about.
    song.select(vec![first]).unwrap();
    assert_eq!(agent(&path, json!({"op": "status"}))["selection"], json!([{"ref": format!("@{first}"), "path": "patterns.line.events.0"}]));
    song.close();
}

#[test]
fn several_events_move_copy_and_paste_together() {
    let (_dir, path, song, seen) = open_with_notes();
    let get = |key: u64| {
        let mut j = agent(&path, json!({"op": "get", "path": format!("@{key}")}));
        j.as_object_mut().unwrap().remove("ref");
        j
    };
    let line = pattern(&song.arrangement(), "line").clone();
    let (first, second, kick) = (line.events[0].key, line.events[1].key, line.events[2].key);

    // Moved together by exact steps, as one step; in pitch, those that have one.
    song.edit(Edit::EventsMove { events: vec![first, second, kick], steps: 1, by: 0.0, semitones: 2 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Move 3 events of line");
    assert_eq!(get(first), json!({"at": 0.25, "pad": "t", "note": "D3", "duration": 1}));
    assert_eq!(get(second), json!({"at": "19/12", "pad": "t", "duration": "2/3", "velocity": 80, "note": "B2"}));
    assert_eq!(get(kick), json!({"at": 2.75, "pad": "k"}));
    song.undo().unwrap();
    update(&seen);
    // None with a pitch is refused, as one is; and one that would leave the pattern refuses them all.
    assert!(song.edit(Edit::EventsMove { events: vec![kick], steps: 0, by: 0.0, semitones: 1 }, None).unwrap_err().to_string().contains("without a root note"));
    assert!(song.edit(Edit::EventsMove { events: vec![first, kick], steps: 6, by: 0.0, semitones: 0 }, None).is_err());
    assert_eq!(get(first)["at"], json!(0));

    // Copied where a move would go, the events staying; the copies are what it made.
    let copies = song.edit(Edit::EventsCopy { events: vec![first, kick], steps: 4, by: 0.0, semitones: -12 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Copy 2 events of line");
    assert_eq!(pattern(&u.arrangement, "line").events.len(), 5);
    assert_eq!(get(copies[0]), json!({"at": 1, "pad": "t", "note": "C2", "duration": 1}));
    assert_eq!(get(copies[1]), json!({"at": 3.5, "pad": "k"}));
    song.undo().unwrap();
    update(&seen);

    // Duplicated right after the span they cover; a hit takes a step.
    let copies = song.edit(Edit::EventsDuplicate { events: vec![second, kick] }, None).unwrap();
    update(&seen);
    // From 4/3 to the kick's step after 2.5, which is 2.75: 17/12 later.
    assert_eq!(get(copies[0])["at"], json!(2.75));
    assert_eq!(get(copies[1])["at"], json!("47/12"));
    song.undo().unwrap();
    update(&seen);

    // Events of two patterns are not moved or copied together.
    agent(&path, json!({"op": "pattern.add", "pattern": "other", "length_beats": 4, "events": [{"at": 0, "pad": "k"}]}));
    agent(&path, json!({"op": "clip.add", "track": "bass", "pattern": "other", "at": 0}));
    update(&seen);
    update(&seen);
    let other = pattern(&song.arrangement(), "other").events[0].key;
    assert!(song.edit(Edit::EventsMove { events: vec![first, other], steps: 1, by: 0.0, semitones: 0 }, None).unwrap_err().to_string().contains("within one pattern"));

    // Pasted into another pattern, from the beat given or right after themselves.
    let copied = |e: &aaw_ffi::view::EventView| aaw_ffi::EventCopy {
        pad: e.pad.clone(),
        at: e.at_text.clone(),
        duration: e.duration_text.clone(),
        velocity: e.velocity,
        note: e.note.clone(),
        transpose: e.transpose,
    };
    let taken: Vec<_> = line.events[..2].iter().map(copied).collect();
    let pasted = song.edit(Edit::EventsPaste { events: taken.clone(), pattern: "other".into(), at: Some(1.0) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Paste 2 events into other");
    assert_eq!(get(pasted[0]), json!({"at": 1, "pad": "t", "note": "C3", "duration": 1}));
    assert_eq!(get(pasted[1]), json!({"at": "7/3", "pad": "t", "duration": "2/3", "velocity": 80}));
    song.undo().unwrap();
    update(&seen);
    let pasted = song.edit(Edit::EventsPaste { events: taken.clone(), pattern: "line".into(), at: None }, None).unwrap();
    update(&seen);
    assert_eq!(get(pasted[0])["at"], json!(2));
    // Past the pattern's end, or on a pad the pattern's tracks lack, is refused.
    assert!(song.edit(Edit::EventsPaste { events: taken.clone(), pattern: "line".into(), at: Some(3.5) }, None).is_err());
    assert!(song.edit(Edit::EventsPaste { events: taken, pattern: "beat".into(), at: Some(0.0) }, None).is_err());

    // Several set and removed at once.
    song.edit(Edit::EventsSet { events: vec![first, kick], at: None, duration: None, velocity: Some(64), transpose: None, note: None }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Change 2 events");
    assert_eq!((get(first)["velocity"].clone(), get(kick)["velocity"].clone()), (json!(64), json!(64)));
    song.edit(Edit::EventsRemove { events: vec![first, kick] }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Remove 2 events");
    assert!(pattern(&u.arrangement, "line").events.iter().all(|e| e.key != first && e.key != kick));
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
fn a_sample_becomes_a_sampler_on_a_midi_track_or_a_new_one() {
    let (dir, path, song, seen) = open();
    transport(&seen);
    let start = song.arrangement();
    let drums = start.tracks[0].key;
    agent(&path, json!({"op": "track.add", "id": "keys", "type": "midi"}));
    let keys = update(&seen).arrangement.tracks[2].key;
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
    let add = |asset: &aaw_ffi::library::Asset, name: &str, track: Option<u64>| Edit::SamplerAdd { asset: asset.clone(), name: name.into(), track, index: 1 };

    // On a MIDI track it is the instrument, in one step with the sample it
    // plays: a hit plays to its end on every note.
    assert!(song.edit(add(&kick, "Kick", Some(keys)), None).unwrap().is_empty());
    let u = update(&seen);
    assert_eq!((u.change.origin, u.change.op.as_str(), u.change.label.as_str()), (Who::User, "batch", "Attach a Sampler of kick to keys"));
    let t = &u.arrangement.tracks[2];
    assert_eq!(t.pads.iter().map(|p| (p.name.as_str(), p.sample.as_str(), p.gate, p.root)).collect::<Vec<_>>(), [("kick", "kick", false, None)]);
    assert_eq!(t.map, [aaw_ffi::view::NoteMapView { low: 0, high: 127, pad: "kick".into(), pitched: true }]);
    assert_eq!(
        agent(&path, json!({"op": "get", "path": "samples.kick"})),
        json!({"path": "samples/0123_kick.wav", "sha256": kick.sha256, "source": "/library/Kicks/808 Kick (Hard).wav"})
    );
    // Another sample takes the place of the Sampler it had.
    let snare = copy("0124_snare.wav");
    song.edit(add(&snare, "Snare", Some(keys)), None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Replace the instrument of keys with a Sampler of snare");
    assert_eq!(u.arrangement.tracks[2].pads.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["snare"]);
    assert_eq!(agent(&path, json!({"op": "inspect"}))["samples"], json!(3));

    // With no track it is a new MIDI track, named after it, which the edit
    // makes. A sample the browser measured a pitch of starts Held, so a note
    // stops at its note-off (D85), and its measured pitch is not taken as
    // the root note (D66). A copy decoded from a compressed file brings that
    // file's hash.
    let mut tone = copy("4567_tone.wav");
    tone.root_note = Some("C2".into());
    tone.source_sha256 = Some("ab".repeat(32));
    let made = song.edit(add(&tone, "808 Sub (C)", None), None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Add track s-808-sub-c with a Sampler of s-808-sub-c");
    assert_eq!(agent(&path, json!({"op": "get", "path": "samples.s-808-sub-c.source_sha256"})), json!("ab".repeat(32)));
    assert_eq!(agent(&path, json!({"op": "get", "path": "samples.s-808-sub-c.root_note"})), Json::Null);
    let track = &u.arrangement.tracks[1];
    assert_eq!((made.as_slice(), track.id.as_str(), track.midi, track.instrument.as_deref()), (&[track.key][..], "s-808-sub-c", true, Some("sampler")));
    assert_eq!(track.pads.iter().map(|p| (p.name.as_str(), p.sample.as_str(), p.gate, p.root)).collect::<Vec<_>>(), [("s-808-sub-c", "s-808-sub-c", true, None)]);
    assert!(track.sampler.is_some(), "one pad on every note is the Sampler device");
    // One undo takes back the track, the Sampler and the sample.
    song.undo().unwrap();
    let u = update(&seen);
    assert_eq!(u.arrangement.tracks.len(), 3);
    assert_eq!(agent(&path, json!({"op": "inspect"}))["samples"], json!(3));
    // A name taken by a track or a return gets a number.
    song.edit(add(&tone, "plate", None), None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[1].id, "plate-2");

    // A track that is not a MIDI track holds no Sampler: a sample lands on
    // its lane as an audio clip (`SampleClip`), never as a pad.
    let e = song.edit(add(&kick, "Kick", Some(drums)), None).unwrap_err().to_string();
    assert_eq!(e, "drums is not a MIDI track: a sample lands on its lane as an audio clip");
    // A file that is not in the project is refused, and nothing changes.
    let missing = aaw_ffi::library::Asset { path: "samples/none.wav".into(), sha256: kick.sha256.clone(), source: String::new(), source_sha256: None, root_note: None };
    assert_eq!(song.edit(add(&missing, "none", Some(keys)), None).unwrap_err().to_string(), "Missing asset none");
    let ident = |name: &str| aaw_host::command::ident(name, "sample");
    assert_eq!(ident("  Hi-Hat #3 (Open)  "), "hi-hat-3-open");
    assert_eq!((ident("808"), ident("!!!"), ident("Ünïcode")), ("s-808".into(), "sample".into(), "n-code".into()));
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
    assert_eq!(aaw_ffi::library::library_path(song_path.clone()), Some(aaw_host::project::data_dir().join("library.sqlite").to_string_lossy().into_owned()));
    assert!(aaw_ffi::library::library_categories().contains(&"kick".to_string()));
    let sources = aaw_ffi::library::library_folders(db.clone(), "list".into(), None).unwrap();
    assert_eq!(sources.len(), 1);
    assert!(sources[0].available);
    let scoped = aaw_ffi::library::library_search_folders(db.clone(), String::new(), None, None, 50, vec![library.join("absent").to_string_lossy().into_owned()]).unwrap();
    assert!(scoped.is_empty());


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

    // As a MIDI track of the song with a Sampler of it, Held since the
    // browser gave it a pitch, which is not taken as its root note.
    song.edit(Edit::SamplerAdd { asset, name: found.category, track: None, index: 0 }, None).unwrap();
    let u = update(&seen);
    let t = &u.arrangement.tracks[0];
    assert_eq!((t.id.as_str(), t.midi, t.pads[0].gate, t.pads[0].root), ("kick", true, true, None));
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
    song.edit(Edit::TrackAdd { index: 0, midi: false }, None).unwrap();
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
    song.edit(Edit::TrackAdd { index: 0, midi: false }, None).unwrap();
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
    // A sampler attached is named, with its pads and which notes play them.
    agent(&path, json!({"op": "instrument.set", "track": "keys", "instrument": {"sampler": {"pads": {"h": {"sample": "hit"}}, "map": [{"notes": 60, "pad": "h"}]}}}));
    let keys = update(&seen).arrangement.tracks[2].clone();
    assert_eq!(keys.instrument.as_deref(), Some("sampler"));
    assert_eq!(keys.pads.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["h"]);
    assert_eq!(keys.map, [aaw_ffi::view::NoteMapView { low: 60, high: 60, pad: "h".into(), pitched: false }]);
    song.close();
}

/// The notes of the first note clip of a track, as (id, pitch, at, duration, velocity).
fn notes_of(track: &aaw_ffi::view::TrackView, clip: usize) -> Vec<(String, i32, String, String, u32)> {
    track.note_clips[clip].notes.iter().map(|n| (n.id.clone(), n.pitch, n.at_text.clone(), n.duration_text.clone(), n.velocity)).collect()
}

fn note(id: &str, pitch: i32, at: &str, duration: &str, velocity: u32) -> (String, i32, String, String, u32) {
    (id.into(), pitch, at.into(), duration.into(), velocity)
}

#[test]
fn the_app_makes_a_midi_track_and_draws_and_edits_its_notes() {
    let (dir, path, song, seen) = open();
    transport(&seen);
    // A MIDI track with no instrument, and a clip of a bar on it.
    let track = song.edit(Edit::TrackAdd { index: 2, midi: true }, None).unwrap()[0];
    let u = update(&seen);
    let keys = &u.arrangement.tracks[2];
    assert!(keys.midi && keys.instrument.is_none() && keys.id == "midi-1");
    let clip = song.edit(Edit::ClipNew { track, at: 8.0 }, None).unwrap()[0];
    let u = update(&seen);
    assert_eq!(u.change.label, "Add note clip to midi-1 at 8");
    let c = &u.arrangement.tracks[2].note_clips[0];
    assert_eq!((c.key, c.at, c.length_beats), (clip, 8.0, 4.0));

    // A chord on the step under the pointer, and a melody note off the grid
    // and on a triplet grid; each is one step of its grid long.
    let add = |at: f64, free: bool, grid: &str, pitch: i32| Edit::NoteAdd { clip, at, free, grid: grid.into(), pitch };
    for pitch in [60, 64, 67] {
        song.edit(add(0.1, false, "1/4", pitch), None).unwrap();
        update(&seen);
    }
    let late = song.edit(add(2.025, true, "1/4", 72), None).unwrap()[0];
    update(&seen);
    song.edit(add(1.4, false, "1/3", 71), None).unwrap();
    let u = update(&seen);
    assert_eq!(notes_of(&u.arrangement.tracks[2], 0), [
        note("n1", 60, "0", "0.25", 100), note("n2", 64, "0", "0.25", 100), note("n3", 67, "0", "0.25", 100),
        note("n4", 72, "2.025", "0.25", 100), note("n5", 71, "4/3", "1/3", 100),
    ]);
    assert!(song.edit(add(4.0, false, "1/4", 60), None).is_err(), "a note is added inside its clip");
    let keys = |t: &aaw_ffi::view::TrackView| t.note_clips[0].notes.iter().map(|n| n.key).collect::<Vec<_>>();
    let chord = keys(&u.arrangement.tracks[2])[..3].to_vec();
    assert_eq!(keys(&u.arrangement.tracks[2])[3], late);

    // The chord moved a step later and up an octave, as one step: by whole
    // steps, so the note off the grid moved by one stays as far off it.
    song.edit(Edit::NotesMove { notes: chord.clone(), steps: 1, grid: "1/4".into(), by: 0.0, semitones: 12 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Move 3 notes");
    song.edit(Edit::NotesMove { notes: vec![late], steps: -1, grid: "1/4".into(), by: -0.05, semitones: 0 }, None).unwrap();
    let u = update(&seen);
    let ns = notes_of(&u.arrangement.tracks[2], 0);
    assert_eq!(ns[0], note("n1", 72, "0.25", "0.25", 100));
    assert_eq!(ns[3], note("n4", 72, "1.725", "0.25", 100));

    // Its end on a line of the grid, the chord's other notes as much longer;
    // off the grid with Option.
    song.edit(Edit::NotesEnd { notes: chord.clone(), grabbed: chord[0], end: 1.1, free: false, grid: "1/4".into() }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Lengthen 3 notes");
    assert!(notes_of(&u.arrangement.tracks[2], 0)[..3].iter().all(|n| n.3 == "0.75"));
    song.edit(Edit::NotesEnd { notes: vec![late], grabbed: late, end: 2.3333, free: true, grid: "1/4".into() }, None).unwrap();
    assert_eq!(notes_of(&update(&seen).arrangement.tracks[2], 0)[3].3, "0.608");
    assert!(song.edit(Edit::NotesEnd { notes: vec![late], grabbed: late, end: 1.5, free: false, grid: "1/4".into() }, None).is_err());

    // Typed fields: a pitch by name is stored as its number, beats as typed.
    song.edit(Edit::NotesSet { notes: vec![late], pitch: Some("D5".into()), at: Some("1.975".into()), duration: Some("1/6".into()), velocity: Some(64) }, None).unwrap();
    assert_eq!(notes_of(&update(&seen).arrangement.tracks[2], 0)[3], note("n4", 74, "1.975", "1/6", 64));
    song.edit(Edit::NotesSet { notes: chord.clone(), pitch: None, at: None, duration: None, velocity: Some(90) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Change 3 notes");
    assert!(notes_of(&u.arrangement.tracks[2], 0)[..3].iter().all(|n| n.4 == 90));
    assert!(song.edit(Edit::NotesSet { notes: vec![late], pitch: Some("200".into()), at: None, duration: None, velocity: None }, None).is_err());

    // The chord copied after itself, in its clip; the copies are made.
    let copies = song.edit(Edit::NotesDuplicate { notes: chord.clone() }, None).unwrap();
    let u = update(&seen);
    assert_eq!(copies.len(), 3);
    let ns = notes_of(&u.arrangement.tracks[2], 0);
    assert_eq!(ns[5..], [note("n6", 72, "1", "0.75", 90), note("n7", 76, "1", "0.75", 90), note("n8", 79, "1", "0.75", 90)]);
    song.edit(Edit::NotesRemove { notes: copies }, None).unwrap();
    assert_eq!(notes_of(&update(&seen).arrangement.tracks[2], 0).len(), 5);

    // Dragged with Option two steps later and down a fifth: copies there,
    // and the chord where it was.
    let copies = song.edit(Edit::NotesCopy { notes: chord.clone(), steps: 2, grid: "1/4".into(), by: 0.0, semitones: -7 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Copy 3 notes");
    assert_eq!(copies.len(), 3);
    let ns = notes_of(&u.arrangement.tracks[2], 0);
    assert_eq!(ns[0], note("n1", 72, "0.25", "0.75", 90));
    assert_eq!(ns[5..], [note("n6", 65, "0.75", "0.75", 90), note("n7", 69, "0.75", "0.75", 90), note("n8", 72, "0.75", "0.75", 90)]);
    assert!(song.edit(Edit::NotesCopy { notes: chord.clone(), steps: 0, grid: "1/4".into(), by: 0.0, semitones: 60 }, None).is_err());
    // The copies' starts dragged to the nearest line, and off the grid:
    // their ends stay where they are.
    song.edit(Edit::NotesStart { notes: copies.clone(), grabbed: copies[0], start: 1.1, free: false, grid: "1/4".into() }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Move the start of 3 notes");
    assert!(notes_of(&u.arrangement.tracks[2], 0)[5..].iter().all(|n| n.2 == "1" && n.3 == "0.5"));
    song.edit(Edit::NotesStart { notes: vec![copies[0]], grabbed: copies[0], start: 0.3333, free: true, grid: "1/4".into() }, None).unwrap();
    assert_eq!(notes_of(&update(&seen).arrangement.tracks[2], 0)[5], note("n6", 65, "0.333", "1.167", 90));
    assert!(song.edit(Edit::NotesStart { notes: vec![copies[1]], grabbed: copies[1], start: 1.5, free: false, grid: "1/4".into() }, None).is_err());
    // Velocities moved together, each held within 1 to 127.
    song.edit(Edit::NotesVelocity { notes: copies.clone(), by: 50 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Change the velocity of 3 notes");
    assert!(notes_of(&u.arrangement.tracks[2], 0)[5..].iter().all(|n| n.4 == 127));
    song.edit(Edit::NotesVelocity { notes: copies.clone(), by: -200 }, None).unwrap();
    assert!(notes_of(&update(&seen).arrangement.tracks[2], 0)[5..].iter().all(|n| n.4 == 1));
    song.edit(Edit::NotesRemove { notes: copies }, None).unwrap();
    assert_eq!(notes_of(&update(&seen).arrangement.tracks[2], 0).len(), 5);
    // Pasted at a beat of the clip, the earliest there and the rest after it.
    let copied: Vec<aaw_ffi::NoteCopy> = u.arrangement.tracks[2].note_clips[0].notes.iter()
        .filter(|n| n.key == late || n.key == chord[0])
        .map(|n| aaw_ffi::NoteCopy { pitch: n.pitch, at: n.at_text.clone(), duration: n.duration_text.clone(), velocity: n.velocity })
        .collect();
    let pasted = song.edit(Edit::NotesPaste { notes: copied.clone(), clip, at: Some(3.0) }, None).unwrap();
    let ns = notes_of(&update(&seen).arrangement.tracks[2], 0);
    assert_eq!(pasted.len(), 2);
    assert_eq!(ns[5..], [note("n6", 72, "3", "0.75", 90), note("n7", 74, "4.725", "1/6", 64)]);
    song.edit(Edit::NotesRemove { notes: pasted }, None).unwrap();
    update(&seen);
    // Without a place, right after what was copied, exactly: the span is 227/120.
    song.edit(Edit::NotesPaste { notes: copied, clip, at: None }, None).unwrap();
    let ns = notes_of(&update(&seen).arrangement.tracks[2], 0);
    assert_eq!((ns[5].2.as_str(), ns[6].2.as_str()), ("257/120", "58/15"));

    // The whole clip saved and read again: the same notes, IDs and places.
    let before = notes_of(&song.arrangement().tracks[2], 0);
    song.close();
    let (tx, rx) = channel();
    let (waves_tx, _waves) = channel();
    let song = Song::open(path.to_string_lossy().into_owned(), Arc::new(Watcher(Mutex::new(tx), Mutex::new(waves_tx)))).unwrap();
    assert_eq!(notes_of(&song.arrangement().tracks[2], 0), before);
    drop(rx);
    song.close();
    drop(dir);
}

#[test]
fn a_note_clip_copied_in_the_app_is_its_own() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    agent(&path, json!({"op": "batch", "label": "A phrase", "commands": [
        {"op": "track.add", "id": "keys", "type": "midi"},
        {"op": "track.add", "id": "bass", "type": "midi"},
        {"op": "clip.add", "track": "keys", "at": 4, "length_beats": 4, "notes": [
            {"pitch": 60, "duration": 1}, {"pitch": 64, "at": 1, "duration": 1}, {"pitch": 67, "at": "1/3", "duration": "1/3"},
        ]},
    ]}));
    let u = update(&seen);
    let original = u.arrangement.tracks[2].note_clips[0].clone();
    let before = notes_of(&u.arrangement.tracks[2], 0);

    // Duplicated right after it: a clip with a new ID, owning copies.
    let copy = song.edit(Edit::ClipsDuplicate { clips: vec![original.key] }, None).unwrap()[0];
    let u = update(&seen);
    let keys = &u.arrangement.tracks[2];
    assert_eq!((keys.note_clips[1].key, keys.note_clips[1].id.as_str(), keys.note_clips[1].at), (copy, "clip2", 8.0));
    let copied: Vec<u64> = keys.note_clips[1].notes.iter().map(|n| n.key).collect();
    song.edit(Edit::NotesMove { notes: copied.clone(), steps: 0, grid: "1/4".into(), by: 0.025, semitones: -12 }, None).unwrap();
    update(&seen);
    song.edit(Edit::NotesSet { notes: copied[..1].to_vec(), pitch: None, at: None, duration: None, velocity: Some(40) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(notes_of(&u.arrangement.tracks[2], 0), before, "the original is as it was");
    assert_eq!(notes_of(&u.arrangement.tracks[2], 1)[0], note("n1", 48, "0.025", "1", 40));
    song.undo().unwrap();
    update(&seen);
    song.undo().unwrap();
    update(&seen);
    song.redo().unwrap();
    let u = update(&seen);
    assert_eq!(notes_of(&u.arrangement.tracks[2], 0), before);
    assert_eq!(notes_of(&u.arrangement.tracks[2], 1)[0].2, "0.025");

    // Notes pasted into another clip, at a place in it.
    let first = &original.notes[0];
    let one = aaw_ffi::NoteCopy { pitch: first.pitch, at: first.at_text.clone(), duration: first.duration_text.clone(), velocity: first.velocity };
    song.edit(Edit::NotesPaste { notes: vec![one], clip: copy, at: Some(2.0) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(notes_of(&u.arrangement.tracks[2], 1).last().unwrap(), &note("n4", 60, "2", "1", 100));
    song.undo().unwrap();
    let u = update(&seen);

    // Pasted at a beat on another MIDI track; a pattern track will not take it.
    let bass = u.arrangement.tracks[3].key;
    let taken = song.copy_clips(vec![original.key]).unwrap();
    let pasted = song.edit(Edit::ClipsPaste { copied: taken.clone(), at: 16.0, track: Some(bass) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Paste a clip");
    let b = &u.arrangement.tracks[3].note_clips[0];
    assert_eq!((b.key, b.at, b.id.as_str()), (pasted[0], 16.0, "clip3"));
    assert_eq!(notes_of(&u.arrangement.tracks[3], 0), before);
    let drums = u.arrangement.tracks[0].key;
    assert!(song.edit(Edit::ClipsPaste { copied: taken.clone(), at: 16.0, track: Some(drums) }, None).is_err());

    // Its length as typed: a triplet's is kept exactly.
    song.edit(Edit::ClipLength { clip: pasted[0], beats: "11/3".into() }, None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[3].note_clips[0].length_beats, 11.0 / 3.0);
    assert!(song.edit(Edit::ClipLength { clip: pasted[0], beats: "0".into() }, None).is_err());

    // Looped every so many beats, as typed, and off again; a pattern clip does not loop.
    song.edit(Edit::ClipLoop { clip: pasted[0], beats: "4/3".into() }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.change.label.as_str(), u.arrangement.tracks[3].note_clips[0].loop_beats), ("Loop clip clip3 at 16 every 4/3 beats", Some(4.0 / 3.0)));
    song.edit(Edit::ClipLoop { clip: pasted[0], beats: "off".into() }, None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[3].note_clips[0].loop_beats, None);
    assert!(song.edit(Edit::ClipLoop { clip: u.arrangement.tracks[0].clips[0].key, beats: "4".into() }, None).is_err());

    // Moved and removed as any clip.
    song.edit(Edit::ClipsMove { clips: vec![pasted[0]], by: -4.0, rows: -1 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.arrangement.tracks[2].note_clips.iter().map(|c| c.at).collect::<Vec<_>>(), [4.0, 8.0, 12.0]);
    song.edit(Edit::ClipsRemove { clips: vec![pasted[0]] }, None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[2].note_clips.len(), 2);

    // What was copied is pasted after the clip it came from is gone, as Cut
    // and Paste do.
    song.edit(Edit::ClipsRemove { clips: vec![original.key] }, None).unwrap();
    update(&seen);
    song.edit(Edit::ClipsPaste { copied: taken, at: 0.0, track: None }, None).unwrap();
    let u = update(&seen);
    let back = u.arrangement.tracks[2].note_clips.iter().position(|c| c.at == 0.0).unwrap();
    assert_eq!(notes_of(&u.arrangement.tracks[2], back), before);
    song.close();
}

#[test]
fn a_note_clip_trimmed_from_its_start_keeps_the_notes_it_passes() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    agent(&path, json!({"op": "batch", "label": "A phrase", "commands": [
        {"op": "track.add", "id": "keys", "type": "midi"},
        {"op": "clip.add", "track": "keys", "at": 4, "length_beats": 4, "notes": [
            {"pitch": 60, "duration": 2}, {"pitch": 64, "at": 1, "duration": 1}, {"pitch": 67, "at": "7/3", "duration": "1/3"},
        ]},
    ]}));
    let clip = update(&seen).arrangement.tracks[2].note_clips[0].key;
    // The start a beat and a half later: the notes stay where they are in the
    // song, the two it passed before the clip, where they do not play.
    song.edit(Edit::ClipTrim { clip, start: Some(5.5), end: None }, None).unwrap();
    let u = update(&seen);
    let c = &u.arrangement.tracks[2].note_clips[0];
    assert_eq!((c.at, c.length_beats), (5.5, 2.5));
    assert_eq!(notes_of(&u.arrangement.tracks[2], 0), [
        note("n1", 60, "-1.5", "2", 100), note("n2", 64, "-0.5", "1", 100), note("n3", 67, "5/6", "1/3", 100),
    ]);
    let listed = agent(&path, json!({"op": "notes", "path": "keys"}));
    let flags: Vec<bool> = listed["clips"][0]["notes"].as_array().unwrap().iter().map(|n| n["outside"] == json!(true)).collect();
    assert_eq!(flags, [true, true, false]);
    // Back again: they are as they were.
    song.edit(Edit::ClipTrim { clip, start: Some(4.0), end: Some(9.0) }, None).unwrap();
    let u = update(&seen);
    assert_eq!((u.arrangement.tracks[2].note_clips[0].at, u.arrangement.tracks[2].note_clips[0].length_beats), (4.0, 5.0));
    assert_eq!(notes_of(&u.arrangement.tracks[2], 0), [
        note("n1", 60, "0", "2", 100), note("n2", 64, "1", "1", 100), note("n3", 67, "7/3", "1/3", 100),
    ]);
    assert!(song.edit(Edit::ClipTrim { clip, start: Some(9.0), end: None }, None).is_err());
    song.close();
}

#[test]
fn a_sample_on_a_midi_track_becomes_its_instrument_and_the_notes_stay() {
    let (dir, path, song, seen) = open();
    transport(&seen);
    agent(&path, json!({"op": "batch", "label": "A phrase", "commands": [
        {"op": "track.add", "id": "keys", "type": "midi"},
        {"op": "clip.add", "track": "keys", "length_beats": 4, "notes": [{"pitch": 60, "duration": 1}, {"pitch": 67, "at": 1, "duration": 1}]},
    ]}));
    let u = update(&seen);
    let track = u.arrangement.tracks[2].key;
    let before = notes_of(&u.arrangement.tracks[2], 0);
    std::fs::create_dir(dir.path().join("samples")).unwrap();
    let tone: Vec<[f32; 2]> = (0..24_000).map(|i| [(i as f32 * 0.0575).sin() * 0.3; 2]).collect();
    for name in ["piano", "organ"] {
        std::fs::write(dir.path().join(format!("samples/{name}.wav")), float_wav_bytes(&tone, 48000)).unwrap();
    }
    let asset = |name: &str, root: Option<&str>| aaw_ffi::library::Asset {
        sha256: aaw_model::digest(&dir.path().join(format!("samples/{name}.wav"))).unwrap(),
        path: format!("samples/{name}.wav"),
        source: format!("/library/{name}.wav"),
        source_sha256: None,
        root_note: root.map(String::from),
    };
    // A sample attached: a sampler that plays it at every note's pitch, as it
    // is at middle C, whatever pitch it was measured at; measured as a pitch,
    // it starts Held.
    song.edit(Edit::SamplerAdd { asset: asset("piano", Some("A3")), name: "Piano".into(), track: Some(track), index: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Attach a Sampler of piano to keys");
    let keys = &u.arrangement.tracks[2];
    assert_eq!((keys.instrument.as_deref(), keys.pads[0].name.as_str(), keys.pads[0].gate), (Some("sampler"), "piano", true));
    assert_eq!(keys.pads[0].root, None);
    assert_eq!(keys.map, [aaw_ffi::view::NoteMapView { low: 0, high: 127, pad: "piano".into(), pitched: true }]);
    assert_eq!(notes_of(keys, 0), before);
    // Another swaps it, and is pitched too; with no measured pitch it plays
    // to its end.
    song.edit(Edit::SamplerAdd { asset: asset("organ", None), name: "Organ".into(), track: Some(track), index: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Replace the instrument of keys with a Sampler of organ");
    let keys = &u.arrangement.tracks[2];
    assert_eq!((keys.pads.len(), keys.pads[0].name.as_str(), keys.pads[0].gate), (1, "organ", false));
    assert!(keys.map[0].pitched);
    assert_eq!(notes_of(keys, 0), before);
    // And taken off: the notes are kept.
    song.edit(Edit::InstrumentRemove { track }, None).unwrap();
    let u = update(&seen);
    assert!(u.arrangement.tracks[2].instrument.is_none() && u.arrangement.tracks[2].pads.is_empty());
    assert_eq!(notes_of(&u.arrangement.tracks[2], 0), before);
    song.undo().unwrap();
    assert_eq!(update(&seen).arrangement.tracks[2].pads[0].name, "organ");
    song.close();
}

#[test]
fn the_sampler_device_is_loaded_shaped_and_given_a_root_note() {
    let (dir, path, song, seen, waves) = open_with_waveforms();
    transport(&seen);
    std::fs::create_dir(dir.path().join("samples")).unwrap();
    let tone: Vec<[f32; 2]> = (0..24_000).map(|i| [(i as f32 * 0.0575).sin() * 0.3; 2]).collect();
    for name in ["piano", "organ"] {
        std::fs::write(dir.path().join(format!("samples/{name}.wav")), float_wav_bytes(&tone, 48000)).unwrap();
    }
    let asset = |name: &str| aaw_ffi::library::Asset {
        sha256: aaw_model::digest(&dir.path().join(format!("samples/{name}.wav"))).unwrap(),
        path: format!("samples/{name}.wav"),
        source: format!("/library/{name}.wav"),
        source_sha256: None,
        root_note: Some("A3".into()),
    };
    // Dragged from the browser, the Sampler is empty.
    let track = song.edit(Edit::InstrumentAdd { track: None }, None).unwrap()[0];
    let u = update(&seen);
    let keys = &u.arrangement.tracks[2];
    let empty = keys.sampler.clone().expect("an empty sampler is the device");
    assert_eq!((empty.pad.clone(), empty.fields.len(), empty.file), (None, 0, 0));
    assert!(song.edit(Edit::SamplerSet { track, field: "attack_ms".into(), value: number(20.0) }, None).is_err());

    // A sample dropped on it plays on every note, as it is at middle C.
    song.edit(Edit::SamplerLoad { track, asset: asset("piano"), name: "Piano".into() }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Load piano into the Sampler on sampler-1");
    let keys = &u.arrangement.tracks[2];
    assert_eq!(keys.map, [aaw_ffi::view::NoteMapView { low: 0, high: 127, pad: "piano".into(), pitched: true }]);
    let s = keys.sampler.clone().unwrap();
    assert_eq!((s.pad.as_deref(), s.sample.as_deref(), s.root, s.root_text.as_str()), (Some("piano"), Some("piano"), None, ""));
    assert_eq!(s.path, "samples/piano.wav");
    assert!(s.file != 0 && u.arrangement.files.iter().any(|f| f.identity == s.file));
    assert_eq!((s.seconds, s.start_seconds, s.end_seconds), (0.5, 0.0, 0.5));
    let names: Vec<&str> = s.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["mode", "gain_db", "pan", "transpose", "start_seconds", "end_seconds", "attack_ms", "release_ms", "reverse"]);
    let field = |s: &aaw_ffi::view::SamplerView, name: &str| s.fields.iter().find(|f| f.name == name).unwrap().clone();
    assert_eq!((field(&s, "mode").value, field(&s, "mode").choices.clone()), (text("one_shot"), vec!["one_shot".to_string(), "gate".into()]));
    assert_eq!((field(&s, "attack_ms").value, field(&s, "attack_ms").live, field(&s, "attack_ms").param.clone()), (number(0.3), false, None));
    assert_eq!((field(&s, "start_seconds").max, field(&s, "end_seconds").max, field(&s, "end_seconds").value), (0.5, 0.5, FieldValue::Absent));
    assert_eq!(field(&s, "end_seconds").initial, number(0.5));
    assert_eq!(field(&s, "reverse").value, FieldValue::Flag { value: false });
    // The file's peaks come for the panel's waveform.
    let (revision, file) = (u.change.revision, s.file);
    loop {
        let w = waves.recv_timeout(Duration::from_secs(10)).expect("waveforms");
        if w.revision < revision {
            continue;
        }
        assert!(w.files.contains(&file));
        if let Some(peaks) = w.file_peaks.iter().find(|p| p.identity == file) {
            assert_eq!((peaks.frames_per_second, peaks.frames), (48000.0, 24000));
            break;
        }
    }

    // Each control is a field of the pad.
    song.edit(Edit::SamplerSet { track, field: "attack_ms".into(), value: number(20.0) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Set the Sampler's attack on sampler-1");
    assert_eq!(field(u.arrangement.tracks[2].sampler.as_ref().unwrap(), "attack_ms").value, number(20.0));
    song.edit(Edit::SamplerSet { track, field: "mode".into(), value: text("gate") }, None).unwrap();
    assert!(update(&seen).arrangement.tracks[2].pads[0].gate);
    song.edit(Edit::SamplerSet { track, field: "end_seconds".into(), value: number(0.25) }, None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[2].sampler.as_ref().unwrap().end_seconds, 0.25);
    assert!(song.edit(Edit::SamplerSet { track, field: "start_seconds".into(), value: number(0.3) }, None).is_err(), "a start past the end");
    song.edit(Edit::SamplerSet { track, field: "end_seconds".into(), value: FieldValue::Absent }, None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[2].sampler.as_ref().unwrap().end_seconds, 0.5);
    assert!(song.edit(Edit::SamplerSet { track, field: "mono".into(), value: FieldValue::Flag { value: true } }, None).is_err());
    assert_eq!(agent(&path, json!({"op": "get", "path": "tracks.sampler-1.instrument.sampler.pads.piano"})), json!({"sample": "piano", "mode": "gate", "attack_ms": 20.0}));

    // The root note is typed as a number or a name, and is the sample's.
    song.edit(Edit::SamplerRoot { track, note: Some("60".into()) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Set the root note of piano to C4");
    let s = u.arrangement.tracks[2].sampler.clone().unwrap();
    assert_eq!((s.root, s.root_text.as_str(), u.arrangement.tracks[2].pads[0].root), (Some(60), "C4", Some(60)));
    song.edit(Edit::SamplerRoot { track, note: Some(" a3 ".into()) }, None).unwrap();
    assert_eq!(update(&seen).arrangement.tracks[2].sampler.as_ref().unwrap().root, Some(57));
    assert!(song.edit(Edit::SamplerRoot { track, note: Some("H9".into()) }, None).is_err());
    song.edit(Edit::SamplerRoot { track, note: None }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Take the root note off piano");
    assert_eq!(u.arrangement.tracks[2].sampler.as_ref().unwrap().root, None);

    // Another sample takes its place and keeps the settings, apart from the
    // start and the end, in one undo step.
    song.edit(Edit::SamplerSet { track, field: "end_seconds".into(), value: number(0.25) }, None).unwrap();
    update(&seen);
    song.edit(Edit::SamplerLoad { track, asset: asset("organ"), name: "Organ".into() }, None).unwrap();
    let u = update(&seen);
    let keys = &u.arrangement.tracks[2];
    let s = keys.sampler.clone().unwrap();
    assert_eq!((keys.pads.len(), s.pad.as_deref(), keys.map[0].pad.as_str()), (1, Some("organ"), "organ"));
    assert_eq!((field(&s, "attack_ms").value, field(&s, "mode").value, s.end_seconds), (number(20.0), text("gate"), 0.5));
    assert_eq!(agent(&path, json!({"op": "get", "path": "samples.organ.root_note"})), Json::Null);
    song.undo().unwrap();
    let u = update(&seen);
    assert_eq!((u.arrangement.tracks[2].sampler.as_ref().unwrap().pad.as_deref(), u.arrangement.tracks[2].sampler.as_ref().unwrap().end_seconds), (Some("piano"), 0.25));

    // A kit of several pads is listed, not drawn as the device.
    agent(&path, json!({"op": "instrument.set", "track": "sampler-1", "instrument": {"sampler": {
        "pads": {"kick": {"sample": "piano"}, "snare": {"sample": "piano"}},
        "map": [{"notes": 36, "pad": "kick"}, {"notes": 38, "pad": "snare"}],
    }}}));
    let u = update(&seen);
    assert_eq!(u.arrangement.tracks[2].sampler, None);
    assert!(song.edit(Edit::SamplerSet { track, field: "attack_ms".into(), value: number(1.0) }, None).is_err());
    song.close();
}

#[test]
fn a_midi_file_dropped_becomes_a_note_clip_and_a_clip_is_exported() {
    let (dir, _path, song, seen) = open();
    transport(&seen);
    let track = song.edit(Edit::TrackAdd { index: 2, midi: true }, None).unwrap()[0];
    update(&seen);
    let clip = song.edit(Edit::ClipNew { track, at: 0.0 }, None).unwrap()[0];
    update(&seen);
    for (at, pitch) in [(0.0, 60), (0.0, 64), (1.0, 67)] {
        song.edit(Edit::NoteAdd { clip, at, free: false, grid: "1/4".into(), pitch }, None).unwrap();
        update(&seen);
    }
    // File › Export MIDI Clip… writes the clip's notes where the panel says.
    let file = dir.path().join("Chords Saved.mid");
    assert_eq!(song.export_midi_clip(clip, file.to_string_lossy().into_owned()).unwrap(), 3);

    // Dropped on the MIDI track's lane, the clip lands there at the drop.
    let drop = |track: Option<u64>, at: f64| Edit::MidiClip { path: file.to_string_lossy().into_owned(), track, index: 3, at };
    let made = song.edit(drop(Some(track), 8.0), None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Import Chords Saved.mid to midi-1");
    let midi = &u.arrangement.tracks[2];
    assert_eq!(made, [midi.note_clips[1].key]);
    assert_eq!((midi.note_clips[1].at, midi.note_clips[1].length_beats), (8.0, 4.0));
    assert_eq!(notes_of(midi, 1), notes_of(midi, 0));

    // Anywhere else, on a new MIDI track named after the file, with no
    // instrument; the song grows to hold it.
    let made = song.edit(drop(None, 30.0), None).unwrap();
    let u = update(&seen);
    let new = &u.arrangement.tracks[3];
    assert_eq!((new.id.as_str(), new.midi, new.instrument.is_none()), ("chords-saved", true, true));
    assert_eq!(made, [new.key, new.note_clips[0].key]);
    assert_eq!(u.arrangement.length_beats, 36.0);
    assert_eq!(notes_of(new, 0), notes_of(&u.arrangement.tracks[2], 0));

    // A file that is not one part is refused, and nothing changes.
    std::fs::write(dir.path().join("hit.mid"), b"not midi").unwrap();
    let refused = Edit::MidiClip { path: dir.path().join("hit.mid").to_string_lossy().into_owned(), track: None, index: 0, at: 0.0 };
    assert_eq!(song.edit(refused, None).unwrap_err().to_string(), "hit.mid: This is not a MIDI file");
    song.close();
}

#[test]
fn tempo_edits_are_saved_validated_and_undoable() {
    let (_dir, path, song, _seen) = open();
    song.edit(Edit::Tempo { bpm: 137.5 }, None).unwrap();
    assert_eq!(song.arrangement().tempo, 137.5);
    assert!(std::fs::read_to_string(&path).unwrap().contains("137.5"));
    for bpm in [19.9, 400.1, f64::NAN, f64::INFINITY] {
        assert!(song.edit(Edit::Tempo { bpm }, None).is_err());
        assert_eq!(song.arrangement().tempo, 137.5);
    }
    song.undo().unwrap();
    assert_eq!(song.arrangement().tempo, 120.0);
    song.redo().unwrap();
    assert_eq!(song.arrangement().tempo, 137.5);
    song.close();
}

#[test]
fn metronome_is_shared_transport_state_without_a_song_edit() {
    let (_dir, path, song, seen) = open();
    assert!(!transport(&seen).metronome);
    let original = std::fs::read(&path).unwrap();
    let revision = song.arrangement().revision;
    song.set_metronome(true).unwrap();
    let state = transport(&seen);
    assert!(state.metronome);
    assert!(!state.playing);
    assert_eq!(agent(&path, json!({"op": "status"}))["metronome"], true);
    song.locate(4.0).unwrap();
    assert!(transport(&seen).metronome);
    agent(&path, json!({"op": "metronome", "enabled": false}));
    assert!(!transport(&seen).metronome);
    assert_eq!(song.arrangement().revision, revision);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    song.close();
}

#[test]
fn browser_instruments_are_atomic_and_effects_insert_at_the_drop() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let drums = song.arrangement().tracks[0].key;
    assert!(
        song.edit(Edit::InstrumentAdd { track: Some(drums) }, None)
            .is_err()
    );
    song.edit(Edit::InstrumentAdd { track: None }, None)
        .unwrap();
    let u = update(&seen);
    let sampler = u.arrangement.tracks.last().unwrap();
    let key = sampler.key;
    assert!(sampler.midi);
    assert_eq!(sampler.id, "sampler-1");
    assert_eq!(
        agent(
            &path,
            json!({"op": "get", "path": "tracks.sampler-1.instrument"})
        ),
        json!({"sampler": {}})
    );
    song.undo().unwrap();
    assert_eq!(update(&seen).arrangement.tracks.len(), 2);
    song.redo().unwrap();
    update(&seen);
    song.edit(Edit::InstrumentRemove { track: key }, None).unwrap();
    update(&seen);
    song.edit(Edit::InstrumentAdd { track: Some(key) }, None).unwrap();
    update(&seen);
    for row in [
        Row::Track { key: drums },
        Row::Return {
            key: song.arrangement().returns[0].key,
        },
        Row::Master,
    ] {
        song.edit(
            Edit::EffectAdd {
                row: row.clone(),
                kind: "delay".into(),
                index: Some(0),
            },
            None,
        )
        .unwrap();
        update(&seen);
        song.edit(
            Edit::EffectAdd {
                row: row.clone(),
                kind: "filter".into(),
                index: Some(0),
            },
            None,
        )
        .unwrap();
        let u = update(&seen);
        let effects = match row {
            Row::Track { .. } => &u.arrangement.tracks[0].effects,
            Row::Group { .. } => &u.arrangement.groups[0].effects,
            Row::Return { .. } => &u.arrangement.returns[0].effects,
            Row::Master => &u.arrangement.master.effects,
        };
        assert_eq!(effects[0].kind, "filter");
        assert_eq!(effects[1].kind, "delay");
        song.undo().unwrap();
        update(&seen);
    }
    song.close();
}

#[test]
fn the_synth_is_drawn_from_its_fields_and_turned_by_its_paths() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    // The agent attaches a synth on a new track; the panel draws every part.
    agent(&path, json!({"op": "synth.add", "track": "lead"}));
    agent(&path, json!({"op": "synth.set", "track": "lead", "values": {"oscillators.sub": {"wave": "sine", "octave": -1}, "lfos.lfo1": {"rate_hz": 3}, "macros.tone": 25}}));
    agent(&path, json!({"op": "synth.mod", "track": "lead", "source": "macros.tone", "target": "filter.cutoff_hz", "amount": 3}));
    let mut u = update(&seen);
    while u.change.op != "synth.mod" {
        u = update(&seen);
    }
    let lead = u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().clone();
    assert_eq!(lead.instrument.as_deref(), Some("synth"));
    assert!(lead.sampler.is_none());
    let s = lead.synth.clone().expect("the synth is the device");
    assert_eq!(
        s.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["voices", "glide_ms", "velocity_percent", "width_percent", "width_mode", "seed"]
    );
    let placement = s.fields.iter().find(|f| f.name == "width_mode").unwrap();
    assert_eq!((placement.kind, placement.value.clone()), (FieldKind::Choice, text("alternate")));
    let width = s.fields.iter().find(|f| f.name == "width_percent").unwrap();
    assert_eq!((width.param.as_deref(), width.live), (Some("instrument.width_percent"), true));
    assert_eq!(aaw_ffi::view::synth_mod_target("width_percent".into()).as_deref(), Some("width_percent"));
    assert_eq!(aaw_ffi::view::synth_mod_target("width_mode".into()), None);
    assert_eq!(s.oscillators.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["a", "sub"]);
    let sub = &s.oscillators[1];
    assert_eq!(sub.fields[0].name, "oscillators.sub.wave");
    assert_eq!(sub.fields[0].value, text("sine"));
    assert_eq!(sub.fields[0].kind, FieldKind::Choice);
    assert!(!sub.fields[0].live, "a wave swaps through a dip");
    let level = sub.fields.iter().find(|f| f.name == "oscillators.sub.level_db").unwrap();
    assert_eq!((level.param.as_deref(), level.live, level.lane), (Some("instrument.oscillators.sub.level_db"), true, None));
    let cutoff = s.filter.iter().find(|f| f.name == "filter.cutoff_hz").unwrap();
    assert_eq!((cutoff.value.clone(), cutoff.log, cutoff.param.as_deref()), (number(20000.0), true, Some("instrument.filter.cutoff_hz")));
    assert_eq!(s.envelopes[0].name, "amp");
    assert_eq!(s.lfos[0].fields.iter().find(|f| f.name == "lfos.lfo1.rate_hz").unwrap().value, number(3.0));
    assert_eq!((s.macros[0].name.as_str(), s.macros[0].label.as_str(), s.macros[0].value.clone()), ("macros.tone", "tone", number(25.0)));
    assert_eq!((s.modulation[0].source.as_str(), s.modulation[0].target.as_str(), s.modulation[0].amount, s.modulation[0].unit.as_str()), ("macros.tone", "filter.cutoff_hz", 3.0, "octaves"));
    assert!(lead.lane_targets.iter().any(|t| t.param == "instrument.macros.tone" && t.label == "Synth macro tone"));

    // The person turns a knob, picks a wave, adds a lane and removes an entry.
    let track = lead.key;
    song.edit(Edit::SynthSet { track, field: "filter.cutoff_hz".into(), value: number(900.0) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Set the Synth's filter cutoff on lead");
    let s = u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().synth.clone().unwrap();
    assert_eq!(s.filter.iter().find(|f| f.name == "filter.cutoff_hz").unwrap().value, number(900.0));
    song.edit(Edit::SynthSet { track, field: "oscillators.a.wave".into(), value: text("square") }, None).unwrap();
    assert_eq!(update(&seen).change.label, "Set the Synth's osc a wave on lead");
    song.edit(Edit::SynthSet { track, field: "oscillators.a.phase".into(), value: number(25.0) }, None).unwrap();
    update(&seen);
    song.edit(Edit::SynthSet { track, field: "oscillators.a.phase".into(), value: FieldValue::Absent }, None).unwrap();
    update(&seen);
    assert_eq!(agent(&path, json!({"op": "get", "path": "tracks.lead.instrument.synth.oscillators.a"})), json!({"wave": "square"}));
    assert!(song.edit(Edit::SynthSet { track, field: "filter.cutoff_hz".into(), value: number(5.0) }, None).is_err());
    assert!(song.edit(Edit::SynthSet { track, field: "filter.nothing".into(), value: number(5.0) }, None).is_err());
    song.edit(Edit::LaneAdd { row: Row::Track { key: track }, param: "instrument.filter.cutoff_hz".into() }, None).unwrap();
    let u = update(&seen);
    let lead = u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().clone();
    assert_eq!((lead.lanes[0].param.as_str(), lead.lanes[0].label.as_str(), lead.lanes[0].unit.as_str(), lead.lanes[0].log), ("instrument.filter.cutoff_hz", "Synth filter cutoff", "Hz", true));
    assert_eq!(lead.lanes[0].points[0].value, 900.0);
    let cutoff = lead.synth.as_ref().unwrap().filter.iter().find(|f| f.name == "filter.cutoff_hz").unwrap();
    assert_eq!(cutoff.lane, Some(lead.lanes[0].key));
    song.edit(Edit::SynthModRemove { track, index: 0 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Remove modulation of filter.cutoff_hz by macros.tone on lead");
    assert!(u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().synth.as_ref().unwrap().modulation.is_empty());
    // A sampler track has no synth to set.
    let bass = u.arrangement.tracks[1].key;
    assert!(song.edit(Edit::SynthSet { track: bass, field: "filter.cutoff_hz".into(), value: number(900.0) }, None).is_err());
}

#[test]
fn the_panel_adds_parts_and_entries_sets_several_fields_saves_a_patch_and_previews_a_note() {
    use aaw_ffi::SynthFieldValue;
    let (_dir, path, song, seen) = open();
    transport(&seen);
    agent(&path, json!({"op": "synth.add", "track": "lead"}));
    let u = update(&seen);
    let lead = u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().clone();
    let track = lead.key;
    // The filter's corner: cutoff and resonance in one step.
    let fields = vec![
        SynthFieldValue { field: "filter.cutoff_hz".into(), value: number(800.0) },
        SynthFieldValue { field: "filter.resonance_percent".into(), value: number(30.0) },
    ];
    song.edit(Edit::SynthSetFields { track, fields }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Set the Synth's filter cutoff and filter resonance on lead");
    assert_eq!(agent(&path, json!({"op": "get", "path": "tracks.lead.instrument.synth.filter"})), json!({"cutoff_hz": 800.0, "resonance_percent": 30.0}));
    // + on each part: the next free name, with a place to start.
    song.edit(Edit::SynthPartAdd { track, part: "oscillators".into() }, None).unwrap();
    assert_eq!(update(&seen).change.label, "Add oscillator b to the Synth on lead");
    song.edit(Edit::SynthPartAdd { track, part: "envelopes".into() }, None).unwrap();
    assert_eq!(update(&seen).change.label, "Add envelope env2 to the Synth on lead");
    song.edit(Edit::SynthPartAdd { track, part: "lfos".into() }, None).unwrap();
    assert_eq!(update(&seen).change.label, "Add LFO lfo1 to the Synth on lead");
    song.edit(Edit::SynthPartAdd { track, part: "macros".into() }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Add macro macro1 to the Synth on lead");
    let s = u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().synth.clone().unwrap();
    assert_eq!(s.oscillators.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    assert_eq!(s.envelopes.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["amp", "env2"]);
    assert_eq!(s.envelopes[1].fields.iter().find(|f| f.name == "envelopes.env2.sustain_percent").unwrap().value, number(0.0));
    assert_eq!(s.lfos[0].name, "lfo1");
    assert_eq!(s.macros[0].name, "macros.macro1");
    assert!(song.edit(Edit::SynthPartAdd { track, part: "strings".into() }, None).is_err());
    // A tab dropped on a control: an entry from the source to the field's target, then its amount set.
    assert_eq!(aaw_ffi::view::synth_mod_target("oscillators.b.semitones".into()).as_deref(), Some("oscillators.b.pitch"));
    assert_eq!(aaw_ffi::view::synth_mod_target("filter.cutoff_hz".into()).as_deref(), Some("filter.cutoff_hz"));
    assert_eq!(aaw_ffi::view::synth_mod_target("filter.mode".into()), None);
    assert_eq!(aaw_ffi::view::synth_mod_target("macros.macro1".into()), None);
    song.edit(Edit::SynthModAdd { track, source: "env2".into(), target: "filter.cutoff_hz".into(), amount: 2.0 }, None).unwrap();
    assert_eq!(update(&seen).change.label, "Modulate filter.cutoff_hz by env2 by 2 on lead");
    song.edit(Edit::SynthModAdd { track, source: "lfo1".into(), target: "oscillators.b.pitch".into(), amount: 0.5 }, None).unwrap();
    let u = update(&seen);
    let s = u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().synth.clone().unwrap();
    assert_eq!(s.modulation[0].field.as_deref(), Some("filter.cutoff_hz"));
    assert_eq!(s.modulation[1].field.as_deref(), Some("oscillators.b.semitones"));
    song.edit(Edit::SynthModSet { track, index: 0, amount: 3.5 }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Set modulation of filter.cutoff_hz by env2 to 3.5 on lead");
    assert_eq!(u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().synth.as_ref().unwrap().modulation[0].amount, 3.5);
    song.edit(Edit::SynthModAdd { track, source: "env2".into(), target: "filter.cutoff_hz".into(), amount: 1.0 }, None).unwrap();
    assert_eq!(update(&seen).change.label, "Set modulation of filter.cutoff_hz by env2 to 1 on lead");
    // × on a part the matrix names is refused; on a free one it goes; amp never does.
    assert!(song.edit(Edit::SynthPartRemove { track, part: "envelopes".into(), name: "env2".into() }, None).is_err());
    assert!(song.edit(Edit::SynthPartRemove { track, part: "envelopes".into(), name: "amp".into() }, None).is_err());
    song.edit(Edit::SynthPartRemove { track, part: "macros".into(), name: "macro1".into() }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Remove macro macro1 from the Synth on lead");
    assert!(u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().synth.as_ref().unwrap().macros.is_empty());
    // Save… writes the patch to the library and names the song's patch; the browser lists it under Mine.
    song.edit(Edit::PatchSave { track, name: "Panel Lead".into(), description: Some("From the panel".into()), tags: vec!["lead".into()], replace: false }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Save patch Panel Lead from lead");
    assert_eq!(u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().synth.as_ref().unwrap().patch.as_deref(), Some("Panel Lead"));
    let mine = aaw_ffi::library::library_patches("panel".into());
    assert_eq!(mine.len(), 1);
    assert!(!mine[0].factory && mine[0].tags == ["lead"] && mine[0].description == "From the panel");
    assert!(song.edit(Edit::PatchSave { track, name: "Panel Lead".into(), description: None, tags: vec![], replace: false }, None).is_err());
    song.edit(Edit::PatchSave { track, name: "Panel Lead".into(), description: None, tags: vec![], replace: true }, None).unwrap();
    // Written over without a description, the file keeps the one it had; the
    // song already named the patch, so nothing in it changed.
    assert_eq!(aaw_ffi::library::library_patches("panel".into())[0].description, "From the panel");
    // A note previewed on a track without an instrument is refused before anything opens.
    let drums = u.arrangement.tracks.iter().find(|t| t.id == "drums").unwrap().key;
    let e = song.preview_note(drums, 60, 100, 1.0).unwrap_err();
    assert!(e.to_string().contains("no instrument"), "{e}");
}

#[test]
fn the_browsers_synth_and_its_patches_are_dropped_on_tracks_and_under_them() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    // The browser lists the factory patches, and a search narrows them by name and tag.
    let all = aaw_ffi::library::library_patches(String::new());
    assert!(all.len() >= 12 && all.iter().all(|p| p.factory), "{all:?}");
    assert_eq!(all[0].name, "Init");
    let pads = aaw_ffi::library::library_patches("pad".into());
    assert!(pads.iter().any(|p| p.name == "Soft Pad") && pads.iter().all(|p| p.name.to_lowercase().contains("pad") || p.tags.iter().any(|t| t.contains("pad"))));
    // Synth dropped under the tracks: a new track with the plain saw.
    song.edit(Edit::SynthAdd { track: None, patch: None }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Add Synth track synth-1");
    let synth = u.arrangement.tracks.iter().find(|t| t.id == "synth-1").unwrap().clone();
    assert_eq!(synth.instrument.as_deref(), Some("synth"));
    assert_eq!(synth.synth.as_ref().unwrap().patch, None);
    // A patch dropped under the tracks: a new track named after it.
    song.edit(Edit::SynthAdd { track: None, patch: Some("Soft Pad".into()) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Add Synth track soft-pad with Soft Pad");
    let pad = u.arrangement.tracks.iter().find(|t| t.id == "soft-pad").unwrap().clone();
    assert_eq!(pad.synth.as_ref().unwrap().patch.as_deref(), Some("Soft Pad"));
    assert_eq!(pad.synth.as_ref().unwrap().oscillators.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["a", "sub"]);
    // A second drop of the same patch takes the next name.
    song.edit(Edit::SynthAdd { track: None, patch: Some("soft-pad".into()) }, None).unwrap();
    assert_eq!(update(&seen).change.label, "Add Synth track soft-pad-2 with Soft Pad");
    // A patch dropped on a track that has a synth loads it; the notes stay.
    agent(&path, json!({"op": "clip.add", "track": "synth-1", "length_beats": 4, "notes": [{"pitch": 60, "duration": 1}]}));
    update(&seen);
    song.edit(Edit::SynthAdd { track: Some(synth.key), patch: Some("Kick".into()) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Load patch Kick into synth-1");
    let loaded = u.arrangement.tracks.iter().find(|t| t.id == "synth-1").unwrap().clone();
    assert_eq!(loaded.synth.as_ref().unwrap().patch.as_deref(), Some("Kick"));
    assert_eq!(loaded.note_clips.len(), 1);
    // Synth dropped on a MIDI track with a sampler takes its place; on a pattern track it is refused.
    song.edit(Edit::InstrumentAdd { track: None }, None).unwrap();
    let u = update(&seen);
    let sampler = u.arrangement.tracks.iter().find(|t| t.instrument.as_deref() == Some("sampler")).unwrap().clone();
    song.edit(Edit::SynthAdd { track: Some(sampler.key), patch: None }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, format!("Attach a Synth to {}", sampler.id));
    let drums = u.arrangement.tracks.iter().find(|t| t.id == "drums").unwrap().key;
    let e = song.edit(Edit::SynthAdd { track: Some(drums), patch: Some("Hat".into()) }, None).unwrap_err();
    assert!(e.to_string().contains("not a MIDI track"), "{e}");
    let e = song.edit(Edit::SynthAdd { track: None, patch: Some("no-such".into()) }, None).unwrap_err();
    assert!(e.to_string().contains("No patch named"), "{e}");
}

#[test]
fn the_patchs_effects_unison_and_tables_are_drawn_and_edited() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    agent(&path, json!({"op": "synth.add", "track": "lead"}));
    agent(&path, json!({"op": "synth.set", "track": "lead", "values": {"oscillators.a.unison": 5, "oscillators.b": {"wave": "wavetable", "table": "vowel"}}}));
    let mut u = update(&seen);
    while u.change.op != "synth.set" {
        u = update(&seen);
    }
    let lead = u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().clone();
    let track = lead.key;
    let s = lead.synth.clone().unwrap();
    // Unison fields are rows of the oscillator, the detune a lane target; the
    // table's choices are the built-in tables and the project's samples, and
    // a wavetable oscillator has a cycle to draw.
    let a = &s.oscillators[0];
    let unison = a.fields.iter().find(|f| f.name == "oscillators.a.unison").unwrap();
    assert_eq!((unison.value.clone(), unison.live, unison.kind), (number(5.0), false, FieldKind::Integer));
    let detune = a.fields.iter().find(|f| f.name == "oscillators.a.unison_detune_cents").unwrap();
    assert_eq!(detune.param.as_deref(), Some("instrument.oscillators.a.unison_detune_cents"));
    assert_eq!(aaw_ffi::view::synth_mod_target("oscillators.a.unison_detune_cents".into()).as_deref(), Some("oscillators.a.unison_detune_cents"));
    let table = a.fields.iter().find(|f| f.name == "oscillators.a.table").unwrap();
    assert_eq!(table.choices, ["organ", "bright", "hollow", "vowel", "fold", "steps", "hit"]);
    assert!(a.cycle.is_empty());
    let b = &s.oscillators[1];
    assert_eq!(b.cycle.len(), 64);
    assert!(b.cycle.iter().any(|x| x.abs() > 0.5) && b.cycle.iter().all(|x| x.abs() <= 1.0 + 1e-9));
    // An effect added to the patch is drawn with its fields and lanes of the track's.
    song.edit(Edit::SynthEffectAdd { track, kind: "chorus".into(), index: None }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Add chorus to the Synth on lead");
    song.edit(Edit::SynthEffectAdd { track, kind: "reverb".into(), index: Some(0) }, None).unwrap();
    let u = update(&seen);
    let lead = u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().clone();
    let s = lead.synth.clone().unwrap();
    assert_eq!(s.effects.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), ["reverb", "chorus"]);
    let mix = s.effects[1].fields.iter().find(|f| f.name == "mix_percent").unwrap();
    assert_eq!((mix.value.clone(), mix.param.as_deref(), mix.lane), (number(50.0), Some("instrument.effects.1.mix_percent"), None));
    assert_eq!(s.effects[0].fields.iter().find(|f| f.name == "mix_percent").unwrap().value, number(25.0));
    assert!(lead.lane_targets.iter().any(|t| t.param == "instrument.effects.1.rate_hz" && t.label == "Synth chorus rate"));
    // Its knob, its lane, its place and its removal go through the effect edits.
    let chorus = s.effects[1].key;
    song.edit(Edit::EffectSet { effect: chorus, field: "depth_ms".into(), value: number(6.0) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().synth.as_ref().unwrap().effects[1].fields.iter().find(|f| f.name == "depth_ms").unwrap().value, number(6.0));
    song.edit(Edit::LaneAdd { row: Row::Track { key: track }, param: "instrument.effects.1.mix_percent".into() }, None).unwrap();
    let u = update(&seen);
    let lead = u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().clone();
    assert_eq!((lead.lanes[0].param.as_str(), lead.lanes[0].label.as_str(), lead.lanes[0].unit.as_str()), ("instrument.effects.1.mix_percent", "Synth chorus mix", "%"));
    assert_eq!(lead.lanes[0].points[0].value, 50.0);
    assert_eq!(lead.synth.as_ref().unwrap().effects[1].fields.iter().find(|f| f.name == "mix_percent").unwrap().lane, Some(lead.lanes[0].key));
    song.edit(Edit::EffectMove { effect: chorus, index: 0 }, None).unwrap();
    let u = update(&seen);
    let lead = u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().clone();
    assert_eq!(lead.synth.as_ref().unwrap().effects[0].kind, "chorus");
    assert_eq!(lead.lanes[0].param, "instrument.effects.0.mix_percent");
    song.edit(Edit::EffectRemove { effect: chorus }, None).unwrap();
    let u = update(&seen);
    let lead = u.arrangement.tracks.iter().find(|t| t.id == "lead").unwrap().clone();
    assert_eq!(lead.synth.as_ref().unwrap().effects.len(), 1);
    assert!(lead.lanes.is_empty(), "the lane went with the effect");
    let left = agent(&path, json!({"op": "get", "path": "tracks.lead.instrument.synth.effects"}));
    assert_eq!((left[0]["type"].clone(), left[0]["mix_percent"].clone(), left.as_array().unwrap().len()), (json!("reverb"), json!(25.0), 1));
}

#[test]
fn the_transport_bars_meter_field_sets_the_time_signature() {
    let (_dir, _path, song, seen) = open();
    transport(&seen);
    assert!(song.edit(Edit::TimeSignature { text: " 6/8 ".into() }, None).unwrap().is_empty());
    let u = update(&seen);
    assert_eq!(u.change.label.as_str(), "Set session.time_signature: \"4/4\" → \"6/8\"");
    let a = song.arrangement();
    assert_eq!((a.time_signature.as_str(), a.bar_beats, a.beat_unit), ("6/8", 3.0, 0.5));
    let refused = song.edit(Edit::TimeSignature { text: "5/3".into() }, None).unwrap_err();
    assert!(refused.to_string().contains("such as 4/4"), "{refused}");
    song.undo().unwrap();
    assert_eq!(song.arrangement().time_signature, "4/4");
}

#[test]
fn an_equalizer_s_spectrum_is_read_for_its_panel() {
    let (_dir, _path, song, _seen) = open();
    let start = song.arrangement();
    assert_eq!(start.sample_rate, 48000);
    let drums = Row::Track { key: start.tracks[0].key };
    let eq = song.edit(Edit::EffectAdd { row: drums, kind: "eq".into(), index: None }, None).unwrap()[0];
    // Nothing has played: every bin is silent, and the frames count is 0.
    let spectrum = song.spectrum(eq).expect("an equalizer on a track has a spectrum");
    assert_eq!((spectrum.sample_rate, spectrum.written, spectrum.levels.len()), (48000, 0, 2049));
    assert!(spectrum.levels.iter().all(|l| *l <= -199.0));
    // Other effects, and anything that is not an effect, have none.
    let bass = &start.tracks[1];
    let filter = bass.effects.iter().find(|e| e.kind == "filter").unwrap();
    assert!(song.spectrum(filter.key).is_none());
    assert!(song.spectrum(bass.key).is_none());
    // A pass band is drawn from the same fields as the others, with its
    // slope, and keeps the gain it had, which the pass ignores.
    song.edit(Edit::EffectSet { effect: eq, field: "bands.0.gain_db".into(), value: number(-3.0) }, None).unwrap();
    song.edit(Edit::EffectSet { effect: eq, field: "bands.0.shape".into(), value: text("highpass") }, None).unwrap();
    song.edit(Edit::EffectSet { effect: eq, field: "bands.0.slope_db_per_octave".into(), value: number(24.0) }, None).unwrap();
    let find = |a: &aaw_ffi::Arrangement| a.tracks[0].effects.iter().find(|e| e.kind == "eq").unwrap().clone();
    let e = find(&song.arrangement());
    assert_eq!(field(&e, "bands.0.shape").value, text("highpass"));
    let slope = field(&e, "bands.0.slope_db_per_octave");
    assert_eq!((slope.value.clone(), slope.choices.clone(), slope.live, slope.param.clone()), (number(24.0), vec!["12".to_string(), "24".into(), "36".into(), "48".into()], false, None));
    assert_eq!(field(&e, "bands.0.gain_db").value, number(-3.0));
    assert!(song.spectrum(eq).is_some(), "the tap follows the edit");
    // A point dragged sets its band's frequency, gain and q as one step.
    song.edit(Edit::BandAdd { effect: eq, freq_hz: None, gain_db: None }, None).unwrap();
    song.edit(Edit::BandSet { effect: eq, band: 1, freq_hz: 6500.0, gain_db: 3.5, q: 0.9 }, None).unwrap();
    let e = find(&song.arrangement());
    assert_eq!(
        (field(&e, "bands.1.freq_hz").value.clone(), field(&e, "bands.1.gain_db").value.clone(), field(&e, "bands.1.q").value.clone()),
        (number(6500.0), number(3.5), number(0.9))
    );
    song.undo().unwrap();
    assert_eq!(field(&find(&song.arrangement()), "bands.1.freq_hz").value, number(1000.0), "one undo step");
}

#[test]
fn groups_are_rows_over_their_tracks() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let ids = |a: &aaw_ffi::Arrangement| -> Vec<String> { a.tracks.iter().map(|t| t.id.clone()).collect() };
    let start = song.arrangement();
    assert!(start.groups.is_empty() && start.tracks.iter().all(|t| t.group.is_none()));
    // ⌘G on the two tracks: a group under a free name, holding them.
    let made = song.edit(Edit::GroupAdd { tracks: vec![start.tracks[1].key, start.tracks[0].key] }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Group perc, drums as group-1");
    let a = &u.arrangement;
    assert_eq!(a.groups.len(), 1);
    let group = &a.groups[0];
    assert_eq!((group.key, group.id.as_str()), (made[0], "group-1"));
    assert_eq!(group.tracks, [a.tracks[0].key, a.tracks[1].key]);
    assert_eq!(ids(a), ["drums", "perc"], "already next to each other, so they stay");
    assert!(a.tracks.iter().all(|t| t.group == Some(group.key)));
    assert!(u.touched.contains(&Touch { part: Part::Group, key: group.key, delta: Delta::Added }));
    // The group's header does what a track's does, and sends.
    let row = Row::Group { key: group.key };
    song.edit(Edit::Gain { row: row.clone(), db: -4.0 }, None).unwrap();
    song.edit(Edit::Solo { row: row.clone(), on: true }, None).unwrap();
    song.edit(Edit::Send { row: row.clone(), to: "plate".into(), db: -9.0 }, None).unwrap();
    update(&seen);
    update(&seen);
    let u = update(&seen);
    assert_eq!(u.change.label, "Set send group-1 → plate");
    let g = &u.arrangement.groups[0];
    assert_eq!((g.gain_db, g.solo), (-4.0, true));
    assert_eq!(g.sends, [SendView { to: "plate".into(), gain_db: -9.0 }]);
    assert!(g.lane_targets.iter().any(|t| t.param == "sends.plate.gain_db"));
    song.edit(Edit::EffectAdd { row: row.clone(), kind: "compressor".into(), index: None }, None).unwrap();
    assert_eq!(update(&seen).arrangement.groups[0].effects[0].kind, "compressor");
    assert!(song.edit(Edit::Solo { row: Row::Return { key: start.returns[0].key }, on: true }, None).is_err());
    // Renamed, its tracks follow; a track taken out goes to the master.
    song.edit(Edit::Rename { row: row.clone(), to: "kit".into() }, None).unwrap();
    assert_eq!(agent(&path, json!({"op": "get", "path": "tracks.drums.group"})), json!("kit"));
    update(&seen);
    song.edit(Edit::TrackGroup { track: start.tracks[1].key, group: None }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, "Take perc out of its group");
    assert_eq!(u.arrangement.tracks[1].group, None);
    assert_eq!(u.arrangement.groups[0].tracks, [start.tracks[0].key]);
    song.edit(Edit::TrackGroup { track: start.tracks[1].key, group: Some(group.key) }, None).unwrap();
    assert_eq!(update(&seen).arrangement.groups[0].tracks.len(), 2);
    // Ungrouped, the tracks stay and the group goes.
    song.edit(Edit::Ungroup { group: group.key }, None).unwrap();
    let u = update(&seen);
    assert!(u.arrangement.groups.is_empty());
    assert_eq!(ids(&u.arrangement), ["drums", "perc"]);
    assert!(u.arrangement.tracks.iter().all(|t| t.group.is_none()));
    assert!(u.touched.contains(&Touch { part: Part::Group, key: group.key, delta: Delta::Removed }));
    song.close();
}

#[test]
fn the_browsers_racks_are_saved_from_a_chain_and_dropped_on_rows() {
    let (_dir, path, song, seen) = open();
    transport(&seen);
    let a = song.arrangement();
    let perc = a.tracks.iter().find(|t| t.id == "perc").unwrap().key;
    let drums = a.tracks.iter().find(|t| t.id == "drums").unwrap().key;
    let plate = a.returns[0].key;
    let name = format!("Panel Rack {}", std::process::id());
    // Save Rack… on perc's chain: a filter and a bypassed delay, listed in the browser.
    song.edit(Edit::RackSave { row: Row::Track { key: perc }, name: name.clone(), description: Some("From the panel".into()), tags: vec!["perc".into()], replace: false }, None).unwrap();
    let mine: Vec<_> = aaw_ffi::library::library_racks("panel".into()).into_iter().filter(|r| r.name == name).collect();
    assert_eq!(mine.len(), 1);
    assert_eq!((mine[0].kinds.clone(), mine[0].description.as_str(), mine[0].tags.clone()), (vec!["filter".to_string(), "delay".to_string()], "From the panel", vec!["perc".to_string()]));
    assert!(aaw_ffi::library::library_racks("delay".into()).iter().any(|r| r.name == name), "searched by kind");
    assert!(aaw_ffi::library::library_racks("reverb".into()).iter().all(|r| r.name != name));
    // Saved again under the name only with replace, which keeps the words; the song did not change.
    assert!(song.edit(Edit::RackSave { row: Row::Track { key: perc }, name: name.clone(), description: None, tags: vec![], replace: false }, None).is_err());
    song.edit(Edit::RackSave { row: Row::Track { key: perc }, name: name.clone(), description: None, tags: vec![], replace: true }, None).unwrap();
    assert_eq!(aaw_ffi::library::library_racks(name.clone()).iter().find(|r| r.name == name).unwrap().description, "From the panel");
    // An empty chain has nothing to save.
    let e = song.edit(Edit::RackSave { row: Row::Track { key: drums }, name: "Nothing".into(), description: None, tags: vec![], replace: false }, None).unwrap_err();
    assert!(e.to_string().contains("no effects to save"), "{e}");
    // Dropped on the drums' header: the effects at the chain's end, one undo step.
    song.edit(Edit::RackAdd { row: Row::Track { key: drums }, rack: name.clone(), index: None }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.change.label, format!("Add rack {name} to tracks.drums"));
    let chain: Vec<_> = u.arrangement.tracks.iter().find(|t| t.id == "drums").unwrap().effects.iter().map(|e| (e.kind.clone(), e.bypass)).collect();
    assert_eq!(chain, [("filter".to_string(), false), ("delay".to_string(), true)]);
    // Dropped on an insertion strip of the return, in front of its reverb; an index past the end lands last.
    song.edit(Edit::RackAdd { row: Row::Return { key: plate }, rack: name.clone(), index: Some(0) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.arrangement.returns[0].effects.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), ["filter", "delay", "reverb"]);
    song.edit(Edit::RackAdd { row: Row::Master, rack: name.clone(), index: Some(99) }, None).unwrap();
    let u = update(&seen);
    assert_eq!(u.arrangement.master.effects.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), ["filter", "delay"]);
    song.undo().unwrap();
    assert!(update(&seen).arrangement.master.effects.is_empty());
    let e = song.edit(Edit::RackAdd { row: Row::Master, rack: "no-such".into(), index: None }, None).unwrap_err();
    assert!(e.to_string().contains("No rack named"), "{e}");
    let _ = agent(&path, json!({"op": "status"}));
}

#[test]
fn an_analyzer_s_measurements_are_read_for_its_window() {
    let (_dir, _path, song, _seen) = open();
    let start = song.arrangement();
    let drums = Row::Track { key: start.tracks[0].key };
    let analyzer = song.edit(Edit::EffectAdd { row: drums, kind: "analyzer".into(), index: None }, None).unwrap()[0];
    let e = song.arrangement().tracks[0].effects.iter().find(|e| e.kind == "analyzer").cloned().unwrap();
    assert!(e.fields.is_empty() && e.bands == 0, "nothing to set: {:?}", e.fields);
    // Nothing has played: silence everywhere, and the frames count is 0.
    let a = song.analysis(analyzer).expect("an analyzer on a track is measured");
    assert_eq!((a.sample_rate, a.written, a.lost, a.spectrum.len(), a.scope.len()), (48000, 0, 0, 2049, 0));
    assert!(a.peak_db == vec![-200.0, -200.0] && a.integrated_lufs.is_none() && a.history.is_empty());
    assert_eq!((a.correlation, a.balance_db), (0.0, 0.0));
    // Other effects, and anything that is not an effect, have none; an
    // equalizer's spectrum is not an analysis and the other way round.
    let bass = &start.tracks[1];
    let filter = bass.effects.iter().find(|e| e.kind == "filter").unwrap();
    assert!(song.analysis(filter.key).is_none() && song.analysis(bass.key).is_none());
    assert!(song.spectrum(analyzer).is_none());
    // The ring follows an edit elsewhere, and a bypass takes it away.
    song.edit(Edit::EffectSet { effect: filter.key, field: "cutoff_hz".into(), value: number(500.0) }, None).unwrap();
    assert!(song.analysis(analyzer).is_some());
    song.edit(Edit::EffectBypass { effect: analyzer, on: true }, None).unwrap();
    assert!(song.analysis(analyzer).is_none(), "bypassed, it measures nothing");
    song.edit(Edit::EffectBypass { effect: analyzer, on: false }, None).unwrap();
    assert!(song.analysis(analyzer).is_some());
    song.reset_analysis(analyzer);
    song.forget_analysis(analyzer);
    assert!(song.analysis(analyzer).is_some(), "watched again from now");
    // A patch's chain does not take one.
    let keys = song.edit(Edit::SynthAdd { track: None, patch: None }, None).unwrap()[0];
    let refused = song.edit(Edit::SynthEffectAdd { track: keys, kind: "analyzer".into(), index: None }, None).unwrap_err();
    assert!(refused.to_string().contains("not offered inside a patch"), "{refused}");
}
