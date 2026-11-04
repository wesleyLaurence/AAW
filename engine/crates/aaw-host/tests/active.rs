//! Clips deactivated, as 0 does in the app: a pattern clip, a note clip or
//! an audio clip that stays where it is and plays nothing, through the
//! session's command, the file, the schedule, the map, the checks and join.

mod common;

use aaw_host::session::Session;
use common::{edit, get, write_song};
use serde_json::json;
use std::collections::HashMap;

fn open() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, true).unwrap();
    (dir, s)
}

/// A MIDI track with a note clip of a bar at beat 0: notes at 0 and 2.
fn with_keys(s: &mut Session) {
    edit(s, json!({"op": "track.add", "id": "keys", "type": "midi"})).unwrap();
    edit(s, json!({"op": "synth.add", "track": "keys"})).unwrap();
    edit(
        s,
        json!({"op": "clip.add", "track": "keys", "id": "riff", "at": 0, "length_beats": 4, "notes": [
            {"pitch": 60, "at": 0, "duration": 1},
            {"pitch": 64, "at": 2, "duration": 1},
        ]}),
    )
    .unwrap();
}

fn hits(s: &Session, track: usize) -> usize {
    aaw_model::schedule::track_triggers(s.project(), track).len()
}

fn notes(s: &Session) -> usize {
    let p = s.project();
    let keys = p.tracks.iter().find(|t| t.id == "keys").unwrap();
    aaw_model::schedule::track_notes(p, keys.midi.as_ref().unwrap()).len()
}

fn map_text(s: &Session) -> String {
    let m = s.map(None, None, None, &[], false).unwrap();
    m["map"].as_array().unwrap().iter().map(|l| format!("{}\n", l.as_str().unwrap())).collect()
}

fn row(s: &Session, name: &str) -> String {
    let text = map_text(s);
    let line = text.lines().map(str::trim_start).find(|l| l.starts_with(&format!("{name} "))).unwrap_or_else(|| panic!("{text}"));
    line[name.len()..].trim_start().to_string()
}

#[test]
fn a_deactivated_clip_stays_and_plays_nothing_until_activated() {
    let (dir, mut s) = open();
    with_keys(&mut s);
    let drums = hits(&s, 0);
    assert_eq!(notes(&s), 2);

    // A pattern clip: its hits are gone, the clip and its place are kept.
    let r = edit(&mut s, json!({"op": "clip.active", "clip": "tracks.drums.clips.0", "active": false})).unwrap();
    assert_eq!(r["label"], json!("Deactivate clip beat at 0"));
    assert_eq!(get(&s, "tracks.drums.clips.0")["active"], json!(false));
    assert_eq!(hits(&s, 0), drums / 2);
    assert_eq!(s.project().tracks[0].clips.len(), 2);

    // A note clip: none of its notes play.
    let r = edit(&mut s, json!({"op": "clip.active", "clip": "tracks.keys.clips.riff", "active": false})).unwrap();
    assert_eq!(r["label"], json!("Deactivate clip riff at 0"));
    assert_eq!(notes(&s), 0);

    // The file says so; the song's length still holds the clips.
    s.save().unwrap();
    let saved = std::fs::read_to_string(dir.path().join("song.yaml")).unwrap();
    assert_eq!(saved.matches("active: false").count(), 2, "{saved}");
    assert_eq!(s.project().session.length_exact(), num_rational::BigRational::from_integer(32.into()));

    // Activated again, the field is gone, as it was before.
    let r = edit(&mut s, json!({"op": "clip.active", "clip": "tracks.drums.clips.0", "active": true})).unwrap();
    assert_eq!(r["label"], json!("Activate clip beat at 0"));
    assert_eq!(hits(&s, 0), drums);
    assert!(get(&s, "tracks.drums.clips.0").get("active").is_none());
    edit(&mut s, json!({"op": "clip.active", "clip": "tracks.keys.clips.riff", "active": true})).unwrap();
    assert_eq!(notes(&s), 2);
    s.save().unwrap();
    assert!(!std::fs::read_to_string(dir.path().join("song.yaml")).unwrap().contains("active"));

    // Undo takes a deactivation back.
    edit(&mut s, json!({"op": "clip.active", "clip": "tracks.drums.clips.1", "active": false})).unwrap();
    s.undo(aaw_host::command::Origin::User, false).unwrap();
    assert_eq!(hits(&s, 0), drums);

    // What is not a clip is refused.
    assert!(edit(&mut s, json!({"op": "clip.active", "clip": "tracks.drums", "active": false})).is_err());
}

#[test]
fn an_audio_clip_is_deactivated_by_its_path() {
    let (dir, mut s) = open();
    let song: Vec<[f32; 2]> = vec![[0.1; 2]; 48000];
    std::fs::write(dir.path().join("song.wav"), aaw_engine::wav::float_wav_bytes(&song, 48000)).unwrap();
    edit(&mut s, json!({"op": "set", "path": "samples.song", "value": {"path": "song.wav"}})).unwrap();
    edit(&mut s, json!({"op": "track.add", "id": "song"})).unwrap();
    edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "song", "at": 4})).unwrap();
    let r = edit(&mut s, json!({"op": "clip.active", "clip": "tracks.song.audio.0", "active": false})).unwrap();
    assert!(r["label"].as_str().unwrap().starts_with("Deactivate clip "), "{r}");
    assert!(!s.project().tracks[2].audio[0].active);
    assert_eq!(row(&s, "song"), ".C......");
    assert!(map_text(&s).contains("C  deactivated: audio song 0–1 s, 2 beats; song bar 2"), "{}", map_text(&s));
}

#[test]
fn the_map_shows_a_deactivated_clip_as_its_own_music_under_one_that_plays() {
    let (_d, mut s) = open();
    // The drums' two clips are the same music, one letter.
    assert_eq!(row(&s, "drums"), "AAAAAAAA");
    edit(&mut s, json!({"op": "clip.active", "clip": "tracks.drums.clips.1", "active": false})).unwrap();
    let text = map_text(&s);
    let drums = row(&s, "drums");
    let off = drums.chars().nth(4).unwrap();
    assert_eq!(&drums[..4], "AAAA");
    assert!(drums[4..].chars().all(|c| c == off) && off != 'A', "{text}");
    assert!(text.contains(&format!("{off}  deactivated: pattern beat, 4 beats, 8 hits; drums bar 5 ×4")), "{text}");
    // A clip that plays is drawn over a deactivated one under it, and the
    // two sound at once nowhere.
    edit(&mut s, json!({"op": "clip.duplicate", "clip": "tracks.drums.clips.0", "at": 16})).unwrap();
    assert_eq!(row(&s, "drums"), "AAAAAAAA");
    let warnings: Vec<String> = aaw_model::check::check(s.project(), &HashMap::new())
        .into_iter()
        .filter(|w| w.code == "clips-overlap" || w.code == "clips-stacked")
        .map(|w| w.message)
        .collect();
    assert!(warnings.is_empty(), "{warnings:?}");
    // Nothing deactivated sounds at a marker.
    let sounding = aaw_host::map::sounding(s.project(), s.path().parent().unwrap(), 18.0, &|_| String::new());
    assert!(sounding.iter().all(|c| c["track"] != json!("drums") || !c["what"].as_str().unwrap().starts_with("deactivated")));
}

#[test]
fn a_deactivated_clip_and_one_that_plays_are_not_joined() {
    let (_d, mut s) = open();
    edit(&mut s, json!({"op": "clip.active", "clip": "tracks.bass.clips.1", "active": false})).unwrap();
    let e = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.bass.clips.0", "tracks.bass.clips.1"]})).unwrap_err();
    assert_eq!(e, "bass at 8 is deactivated and bass at 0 is not; activate or deactivate both to join them");
    // Both deactivated, they join, and the clip they make is deactivated.
    edit(&mut s, json!({"op": "clip.active", "clip": "tracks.bass.clips.0", "active": false})).unwrap();
    edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.bass.clips.0", "tracks.bass.clips.1"]})).unwrap();
    assert_eq!(get(&s, "tracks.bass.clips.0")["active"], json!(false));
}
