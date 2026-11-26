//! The song's tempo changes: `tempo.set`, `tempo.remove` and `tempo.clear`
//! edit the master's tempo lane through the host, `session.tempo` stays the
//! lane's opening tempo, and a song back at one tempo saves as it did.

mod common;

use aaw_host::session::Session;
use common::{edit, write_audio_song, write_song};
use serde_json::json;

fn steps(s: &Session) -> Vec<(f64, f64)> {
    s.tempo_list()["steps"].as_array().unwrap().iter().map(|x| (x["at"].as_f64().unwrap(), x["bpm"].as_f64().unwrap())).collect()
}

#[test]
fn a_step_is_a_lane_on_the_master_and_the_song_is_version_3() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let mut s = Session::open(&path, false).unwrap();
    edit(&mut s, json!({"op": "tempo.set", "at": 16, "bpm": 150})).unwrap();
    assert_eq!(steps(&s), vec![(0.0, 120.0), (16.0, 150.0)]);
    let lane = s.project().tempo_lane().expect("a tempo lane");
    assert!(lane.points.iter().all(|p| p.curve == aaw_model::Curve::Hold));
    assert_eq!(s.project().schema_version(), 3);
    assert_eq!(s.project().session.tempo, 120.0);
    let list = s.tempo_list();
    assert_eq!(list["steps"][1]["bar"], 5);
    assert_eq!(list["steps"][1]["seconds"], 8.0);
    // The opening tempo is the lane's first step, and the session's with it.
    edit(&mut s, json!({"op": "tempo.set", "at": 0, "bpm": 100})).unwrap();
    assert_eq!(steps(&s), vec![(0.0, 100.0), (16.0, 150.0)]);
    assert_eq!(s.project().session.tempo, 100.0);
    s.write().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("schema_version: 3\n"), "{text}");
}

#[test]
fn the_session_tempo_alone_is_refused_while_the_lane_holds_the_tempo() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::open(&write_song(dir.path()), false).unwrap();
    edit(&mut s, json!({"op": "tempo.set", "at": 16, "bpm": 150})).unwrap();
    let e = edit(&mut s, json!({"op": "set", "path": "session.tempo", "value": 130})).unwrap_err();
    assert!(e.contains("daw tempo set"), "{e}");
    // Setting it to what it is changes nothing and is not refused.
    edit(&mut s, json!({"op": "set", "path": "session.tempo", "value": 120})).unwrap();
}

#[test]
fn a_song_back_at_one_tempo_has_no_lane_and_saves_as_before() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let mut s = Session::open(&path, false).unwrap();
    let before = s.project().dump(true);
    edit(&mut s, json!({"op": "tempo.set", "at": 16, "bpm": 150})).unwrap();
    edit(&mut s, json!({"op": "tempo.set", "at": 24, "bpm": 90})).unwrap();
    edit(&mut s, json!({"op": "tempo.remove", "at": 16})).unwrap();
    assert_eq!(steps(&s), vec![(0.0, 120.0), (24.0, 90.0)]);
    let e = edit(&mut s, json!({"op": "tempo.remove", "at": 0})).unwrap_err();
    assert!(e.contains("opening tempo"), "{e}");
    assert!(edit(&mut s, json!({"op": "tempo.remove", "at": 3})).unwrap_err().contains("no step at beat 3"));
    // A step to the tempo already playing is no step: the lane goes.
    edit(&mut s, json!({"op": "tempo.set", "at": 24, "bpm": 120})).unwrap();
    assert!(s.project().tempo_lane().is_none());
    assert!(aaw_model::value::py_eq(&s.project().dump(true), &before));
    edit(&mut s, json!({"op": "tempo.set", "at": 8, "bpm": 140})).unwrap();
    edit(&mut s, json!({"op": "tempo.clear"})).unwrap();
    assert!(s.project().tempo_lane().is_none());
    assert_eq!(s.project().schema_version(), 2);
    // On a song without steps, beat 0 is its one tempo.
    edit(&mut s, json!({"op": "tempo.set", "at": 0, "bpm": 96})).unwrap();
    assert!(s.project().tempo_lane().is_none());
    assert_eq!(s.project().session.tempo, 96.0);
}

#[test]
fn a_tempo_change_is_refused_while_a_track_is_frozen() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::open(&write_song(dir.path()), false).unwrap();
    edit(&mut s, json!({"op": "track.freeze", "track": "bass"})).unwrap();
    let e = edit(&mut s, json!({"op": "tempo.set", "at": 16, "bpm": 150})).unwrap_err();
    assert!(e.contains("frozen"), "{e}");
}

#[test]
fn an_audio_clip_s_length_is_read_through_the_steps() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::open(&write_audio_song(dir.path()), false).unwrap();
    // The tone plays half a second from beat 16: one beat at 120, and at 60
    // half a beat.
    edit(&mut s, json!({"op": "tempo.set", "at": 16, "bpm": 60})).unwrap();
    let m = s.project().tempo_map();
    let clip = &s.project().tracks[1].audio[1];
    let end = clip.sound_end(&m, Some(0.5)).unwrap();
    assert!((end - 16.5).abs() < 1e-9, "{end}");
    // A split a quarter second in is half a quarter of a beat... at 60, a
    // quarter beat: the right half starts a quarter second into the file.
    edit(&mut s, json!({"op": "clip.split", "clip": "tracks.bass.audio.1", "at": "65/4"})).unwrap();
    let right = &s.project().tracks[1].audio[2];
    assert!((right.source_start_seconds - 0.25).abs() < 1e-9, "{}", right.source_start_seconds);
}

#[test]
fn a_midi_file_written_from_a_clip_carries_the_steps_where_it_plays() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::open(&write_song(dir.path()), false).unwrap();
    edit(&mut s, json!({"op": "tempo.set", "at": 20, "bpm": 150})).unwrap();
    let file = dir.path().join("beat.mid");
    s.export_midi("tracks.drums.clips.1", file.to_str().unwrap()).unwrap();
    let bytes = std::fs::read(&file).unwrap();
    // Two tempo events: 120 at the clip's start and 150 four beats in.
    let tempos: Vec<u32> = bytes
        .windows(6)
        .filter(|w| w[0] == 0xff && w[1] == 0x51 && w[2] == 3)
        .map(|w| u32::from_be_bytes([0, w[3], w[4], w[5]]))
        .collect();
    assert_eq!(tempos, vec![500_000, 400_000]);
}
