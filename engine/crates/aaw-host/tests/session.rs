//! Session commands, handles, undo and redo, batches, external edits and saves.

mod common;

use aaw_host::command::{Command, Origin};
use aaw_host::session::Session;
use common::{cmd, edit, get, refs, write_song};
use serde_json::{json, Value as Json};

fn open(hosted: bool) -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, hosted).unwrap();
    (dir, s)
}

fn file(s: &Session) -> String {
    std::fs::read_to_string(s.path()).unwrap()
}

#[test]
fn every_command_round_trips_as_json() {
    let commands = [
        json!({"op": "set", "path": "tracks.drums.gain_db", "value": -3}),
        json!({"op": "track.add", "id": "keys", "index": 1, "gain_db": -6}),
        json!({"op": "clip.add", "track": "drums", "pattern": "beat", "at": "1/3"}),
        json!({"op": "clip.move", "clip": "@4", "at": 8}),
        json!({"op": "audio.add", "track": "drums", "sample": "tone", "at": 4, "source_start_seconds": 0.1}),
        json!({"op": "audio.split", "clip": "@9", "at": "13/3"}),
        json!({"op": "audio.trim", "clip": "tracks.drums.audio.0", "end": 6}),
        json!({"op": "audio.crossfade", "clip": "@9", "ms": 20.0, "lead_ms": 4.0}),
        json!({"op": "audio.cut", "track": "drums", "from": 4, "to": 8, "in_ms": 3.0}),
        json!({"op": "effect.add", "owner": "master", "type": "limiter", "ceiling_db": -1}),
        json!({"op": "return.remove", "return": "plate"}),
        json!({"op": "loop", "start": 0, "length": 8}),
        json!({"op": "batch", "commands": [{"op": "toggle", "path": "tracks.drums.mute"}]}),
        json!({"op": "undo"}),
        json!({"op": "changes", "since": 3}),
    ];
    for c in commands {
        let parsed: Command = serde_json::from_value(c.clone()).unwrap();
        assert_eq!(parsed.json(), c);
    }
}

#[test]
fn set_toggle_and_remove() {
    let (_d, mut s) = open(true);
    let r = edit(&mut s, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -4.5})).unwrap();
    assert_eq!(r["before"], json!(0.0));
    assert_eq!(r["revision"], json!(1));
    assert_eq!(get(&s, "tracks.drums.gain_db"), json!(-4.5));
    edit(&mut s, json!({"op": "toggle", "path": "tracks.bass.solo"})).unwrap();
    assert_eq!(get(&s, "tracks.bass.solo"), json!(true));
    // A new map key: a pad.
    edit(&mut s, json!({"op": "set", "path": "tracks.drums.pads.k", "value": {"sample": "hit", "gain_db": -2}})).unwrap();
    assert_eq!(get(&s, "tracks.drums.pads.k.gain_db"), json!(-2.0));
    // Removing a field resets it to its default.
    edit(&mut s, json!({"op": "remove", "path": "tracks.drums.gain_db"})).unwrap();
    assert_eq!(get(&s, "tracks.drums.gain_db"), json!(0.0));
    let e = edit(&mut s, json!({"op": "toggle", "path": "tracks.drums.gain_db"})).unwrap_err();
    assert_eq!(e, "tracks.drums.gain_db is not a boolean");
    let e = edit(&mut s, json!({"op": "set", "path": "tracks.nope.gain_db", "value": 1})).unwrap_err();
    assert_eq!(e, "tracks has no nope");
}

#[test]
fn invalid_edits_are_refused_with_the_models_message_and_change_nothing() {
    let (_d, mut s) = open(true);
    let before = s.doc().sha.clone();
    let e = edit(&mut s, json!({"op": "set", "path": "session.tempo", "value": 9999})).unwrap_err();
    assert_eq!(
        e,
        "1 validation error for Project\nsession.tempo\n  Input should be less than or equal to 400 [type=less_than_equal]"
    );
    let e = edit(&mut s, json!({"op": "clip.move", "clip": "tracks.bass.clips.2", "at": 30})).unwrap_err();
    assert!(e.contains("bass: clip exceeds session"), "{e}");
    let e = edit(&mut s, json!({"op": "set", "path": "tracks.drums.volume", "value": 1})).unwrap_err();
    assert!(e.contains("Extra inputs are not permitted"), "{e}");
    assert_eq!(s.doc().sha, before);
    assert_eq!(s.revision(), 0);
}

#[test]
fn tracks_add_move_rename_and_remove() {
    let (_d, mut s) = open(true);
    edit(&mut s, json!({"op": "track.add", "id": "keys", "index": 0, "gain_db": -6})).unwrap();
    assert_eq!(get(&s, "tracks.0.id"), json!("keys"));
    edit(&mut s, json!({"op": "track.move", "track": "keys", "index": 2})).unwrap();
    assert_eq!(get(&s, "tracks.2.id"), json!("keys"));
    // Renaming a track renames the sidechains that name it.
    let r = edit(&mut s, json!({"op": "track.rename", "track": "drums", "to": "kit"})).unwrap();
    assert_eq!(r["label"], json!("Rename track drums to kit"));
    assert_eq!(get(&s, "tracks.bass.effects.0.sidechain"), json!("kit"));
    // Removing a track that a sidechain names is refused.
    let e = edit(&mut s, json!({"op": "track.remove", "track": "kit"})).unwrap_err();
    assert!(e.contains("bass: unknown sidechain track kit"), "{e}");
    edit(&mut s, json!({"op": "track.remove", "track": "keys"})).unwrap();
    assert_eq!(get(&s, "tracks").as_array().unwrap().len(), 2);
}

#[test]
fn returns_carry_their_sends_and_lanes() {
    let (_d, mut s) = open(true);
    edit(&mut s, json!({"op": "return.rename", "return": "plate", "to": "hall"})).unwrap();
    assert_eq!(get(&s, "tracks.bass.sends.0.to"), json!("hall"));
    assert_eq!(get(&s, "tracks.bass.automation.0.param"), json!("sends.hall.gain_db"));
    let r = edit(&mut s, json!({"op": "return.remove", "return": "hall"})).unwrap();
    assert_eq!(
        r["also"],
        json!(["tracks.bass: removed lane sends.hall.gain_db", "removed tracks.bass.sends.hall"])
    );
    assert_eq!(get(&s, "tracks.bass.sends"), json!([]));
    assert_eq!(get(&s, "tracks.bass.automation"), json!([]));
    edit(&mut s, json!({"op": "return.add", "id": "room", "gain_db": -3})).unwrap();
    edit(&mut s, json!({"op": "send.set", "track": "drums", "to": "room", "gain_db": -10})).unwrap();
    edit(&mut s, json!({"op": "send.set", "track": "drums", "to": "room", "pre_fader": true})).unwrap();
    assert_eq!(get(&s, "tracks.drums.sends.room"), json!({"to": "room", "gain_db": -10.0, "pre_fader": true}));
    edit(&mut s, json!({"op": "send.remove", "track": "drums", "to": "room"})).unwrap();
    assert_eq!(get(&s, "tracks.drums.sends"), json!([]));
}

#[test]
fn clips_by_handle_survive_other_edits() {
    let (_d, mut s) = open(true);
    let clips = refs(&s, "tracks.bass.clips");
    let last = clips[2].clone();
    edit(&mut s, json!({"op": "clip.remove", "clip": clips[0]})).unwrap();
    // The handle still names the same clip, now at index 1.
    edit(&mut s, json!({"op": "clip.move", "clip": last, "at": 20})).unwrap();
    assert_eq!(get(&s, "tracks.bass.clips.1.at"), json!(20));
    // Moving to another track keeps the handle.
    edit(&mut s, json!({"op": "pad.add", "track": "drums", "pad": "t", "sample": "tone", "mode": "gate"})).unwrap();
    edit(&mut s, json!({"op": "clip.move", "clip": last, "track": "drums"})).unwrap();
    assert_eq!(refs(&s, "tracks.drums.clips").last().unwrap(), &last);
    // Undo brings the removed clip back with its handle.
    s.undo(Origin::User, false).unwrap();
    s.undo(Origin::User, false).unwrap();
    s.undo(Origin::User, false).unwrap();
    s.undo(Origin::User, false).unwrap();
    assert_eq!(refs(&s, "tracks.bass.clips"), clips);
}

#[test]
fn clips_add_repeat_and_duplicate() {
    let (_d, mut s) = open(true);
    let r = edit(&mut s, json!({"op": "clip.add", "track": "drums", "pattern": "beat", "at": 8, "repeats": 2})).unwrap();
    assert_eq!(r["path"], json!("tracks.drums.clips.2"));
    let h = r["handle"].as_str().unwrap().to_string();
    edit(&mut s, json!({"op": "clip.repeats", "clip": h, "repeats": 1})).unwrap();
    // A copy goes right after the original: beat 8 + 4 beats.
    let r = edit(&mut s, json!({"op": "clip.duplicate", "clip": h})).unwrap();
    assert_eq!(r["path"], json!("tracks.drums.clips.3"));
    assert_eq!(get(&s, "tracks.drums.clips.3"), json!({"pattern": "beat", "at": 12}));
    // Thirds stay exact.
    edit(&mut s, json!({"op": "clip.add", "track": "bass", "pattern": "bass", "at": "1/3"})).unwrap();
    let r = edit(&mut s, json!({"op": "clip.duplicate", "clip": "tracks.bass.clips.3"})).unwrap();
    assert_eq!(get(&s, r["path"].as_str().unwrap()), json!({"pattern": "bass", "at": "13/3"}));
    let r = edit(&mut s, json!({"op": "clip.duplicate", "clip": "tracks.bass.clips.1", "at": 4.5})).unwrap();
    assert_eq!(get(&s, &format!("{}.at", r["path"].as_str().unwrap())), json!(4.5));
    edit(&mut s, json!({"op": "clip.remove", "clip": h})).unwrap();
    assert_eq!(get(&s, "tracks.drums.clips").as_array().unwrap().len(), 3);
}

/// A song of ten seconds at 120 BPM on a track of its own, as one audio clip
/// whose second 1 plays on beat 4.
fn with_song(s: &mut Session) -> String {
    let song: Vec<[f32; 2]> = (0..480000).map(|i| [((i % 4800) as f32 / 4800.0 - 0.5) * 0.2; 2]).collect();
    let dir = s.path().parent().unwrap().to_path_buf();
    std::fs::write(dir.join("song.wav"), aaw_engine::wav::float_wav_bytes(&song, 48000)).unwrap();
    edit(s, json!({"op": "set", "path": "samples.song", "value": {"path": "song.wav"}})).unwrap();
    edit(s, json!({"op": "track.add", "id": "song"})).unwrap();
    let made = edit(s, json!({"op": "audio.add", "track": "song", "sample": "song", "at": 4, "source_start_seconds": 1})).unwrap();
    assert_eq!(made["path"], json!("tracks.song.audio.0"));
    assert_eq!(made["label"], json!("Add audio clip of song to song"));
    made["handle"].as_str().unwrap().to_string()
}

#[test]
fn audio_clips_split_and_trim_where_their_audio_is() {
    let (_d, mut s) = open(true);
    let clip = with_song(&mut s);
    // Two beats on is a second on at 120 BPM: the halves meet there, unfaded.
    let r = edit(&mut s, json!({"op": "audio.split", "clip": clip, "at": 6})).unwrap();
    assert_eq!((r["path"].clone(), r["label"].clone()), (json!("tracks.song.audio.1"), json!("Split audio clip song at 4 at beat 6")));
    assert_eq!(
        get(&s, "tracks.song.audio"),
        json!([
            {"sample": "song", "at": 4, "source_start_seconds": 1.0, "source_end_seconds": 2.0, "fade_out_ms": 0.0},
            {"sample": "song", "at": 6, "source_start_seconds": 2.0, "fade_in_ms": 0.0},
        ])
    );
    // The first keeps its handle; a beat outside the clip is refused.
    assert_eq!(refs(&s, "tracks.song.audio")[0], clip);
    let outside = edit(&mut s, json!({"op": "audio.split", "clip": clip, "at": 6})).unwrap_err();
    assert_eq!(outside, "Beat 6 is not inside audio clip song at 4");
    assert!(edit(&mut s, json!({"op": "audio.split", "clip": clip, "at": 4})).is_err());
    // An edge moves to a beat and the audio stays where it was: beat 3 plays
    // half a second earlier in the file, and beat 5.5 ends three quarters in.
    edit(&mut s, json!({"op": "audio.trim", "clip": clip, "start": 3, "end": 5.5})).unwrap();
    assert_eq!(
        get(&s, "tracks.song.audio.0"),
        json!({"sample": "song", "at": 3, "source_start_seconds": 0.5, "source_end_seconds": 1.75, "fade_out_ms": 0.0})
    );
    let early = edit(&mut s, json!({"op": "audio.trim", "clip": clip, "start": 0})).unwrap_err();
    assert_eq!(early, "Audio clip song at 3 has no audio at beat 0");
    assert!(edit(&mut s, json!({"op": "audio.trim", "clip": clip})).is_err());
    assert!(edit(&mut s, json!({"op": "audio.trim", "clip": clip, "end": 2})).is_err());
    // A clip with a tempo of its own follows the session's, so its beats are
    // its file's: at 100 BPM a beat is 0.6 s of the file whatever the tempo.
    edit(&mut s, json!({"op": "set", "path": "tracks.song.audio.1.source_bpm", "value": 100})).unwrap();
    edit(&mut s, json!({"op": "audio.trim", "clip": "tracks.song.audio.1", "end": 11})).unwrap();
    assert_eq!(get(&s, "tracks.song.audio.1.source_end_seconds"), json!(5.0));
    // Moving and removing are the commands every list has.
    edit(&mut s, json!({"op": "set", "path": "tracks.song.audio.1.at", "value": "25/3"})).unwrap();
    edit(&mut s, json!({"op": "remove", "path": clip})).unwrap();
    assert_eq!(get(&s, "tracks.song.audio").as_array().unwrap().len(), 1);
    assert_eq!(get(&s, "tracks.song.audio.0.at"), json!("25/3"));
}

#[test]
fn a_cut_removes_beats_closes_the_gap_and_crossfades_the_join() {
    let (_d, mut s) = open(true);
    let clip = with_song(&mut s);
    edit(&mut s, json!({"op": "audio.trim", "clip": clip, "end": 20})).unwrap();
    edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "song", "at": 24, "source_end_seconds": 2})).unwrap();
    // Beats 8 to 12 go: the clip across them becomes two that meet at beat 8,
    // and the clip after them moves four beats earlier.
    let r = edit(&mut s, json!({"op": "audio.cut", "track": "song", "from": 8, "to": 12})).unwrap();
    assert_eq!(r["label"], json!("Cut beats 8 to 12 from song and close the gap"));
    assert_eq!(
        get(&s, "tracks.song.audio"),
        json!([
            {"sample": "song", "at": 4, "source_start_seconds": 1.0, "source_end_seconds": 3.0, "fade_out_ms": 12.0},
            {"sample": "song", "at": 8, "source_start_seconds": 5.0, "source_end_seconds": 9.0, "lead_ms": 5.0, "fade_in_ms": 4.0},
            {"sample": "song", "at": 20, "source_end_seconds": 2.0},
        ])
    );
    assert_eq!(refs(&s, "tracks.song.audio")[0], clip);
    // One undo takes the cut back.
    s.undo(Origin::User, false).unwrap();
    assert_eq!(get(&s, "tracks.song.audio.0.source_end_seconds"), json!(9.0));
    assert_eq!(get(&s, "tracks.song.audio").as_array().unwrap().len(), 2);
    // A cut from a clip's start leaves only what follows it, and nothing to crossfade.
    edit(&mut s, json!({"op": "audio.cut", "track": "song", "from": 0, "to": 6, "ms": 30.0})).unwrap();
    assert_eq!(
        get(&s, "tracks.song.audio"),
        json!([
            {"sample": "song", "source_start_seconds": 2.0, "source_end_seconds": 9.0},
            {"sample": "song", "at": 18, "source_end_seconds": 2.0},
        ])
    );
    // A cut that holds a whole clip removes it; the crossfade can be set.
    edit(&mut s, json!({"op": "audio.cut", "track": "song", "from": 10, "to": 30, "ms": 30.0, "in_ms": 2.0, "lead_ms": 8.0})).unwrap();
    assert_eq!(
        get(&s, "tracks.song.audio"),
        json!([{"sample": "song", "source_start_seconds": 2.0, "source_end_seconds": 7.0}])
    );
    assert_eq!(edit(&mut s, json!({"op": "audio.cut", "track": "song", "from": 8, "to": 8})).unwrap_err(), "A cut runs from a beat to a later one");
    assert!(edit(&mut s, json!({"op": "audio.cut", "track": "nobody", "from": 0, "to": 4})).is_err());
}

#[test]
fn a_crossfade_sets_the_fades_of_a_join() {
    let (_d, mut s) = open(true);
    let clip = with_song(&mut s);
    edit(&mut s, json!({"op": "audio.trim", "clip": clip, "end": 8})).unwrap();
    let r = edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "song", "at": 8, "source_start_seconds": 6})).unwrap();
    let next = r["handle"].as_str().unwrap().to_string();
    // The entering clip starts ahead of its beat and fades in within that; the
    // one before it fades out from there.
    let r = edit(&mut s, json!({"op": "audio.crossfade", "clip": next})).unwrap();
    assert_eq!(r["label"], json!("Crossfade audio clips at beat 8"));
    assert_eq!(get(&s, "tracks.song.audio.0.fade_out_ms"), json!(12.0));
    assert_eq!(get(&s, "tracks.song.audio.1"), json!({"sample": "song", "at": 8, "source_start_seconds": 6.0, "lead_ms": 5.0, "fade_in_ms": 4.0}));
    edit(&mut s, json!({"op": "audio.crossfade", "clip": next, "ms": 40.0, "in_ms": 10.0, "lead_ms": 12.0})).unwrap();
    assert_eq!(get(&s, "tracks.song.audio.0.fade_out_ms"), json!(40.0));
    assert_eq!(get(&s, "tracks.song.audio.1.lead_ms"), json!(12.0));
    assert_eq!(get(&s, "tracks.song.audio.1.fade_in_ms"), json!(10.0));
    let first = edit(&mut s, json!({"op": "audio.crossfade", "clip": clip})).unwrap_err();
    assert_eq!(first, "No audio clip comes before song at 4 on its track");
    // What the model refuses, a command is refused for: a sample the song lacks.
    let unknown = edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "nothing"})).unwrap_err();
    assert!(unknown.contains("unknown sample nothing"), "{unknown}");
    let late = edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "song", "at": 32})).unwrap_err();
    assert!(late.contains("audio clip starts past the session"), "{late}");
}

#[test]
fn patterns_events_and_pads() {
    let (_d, mut s) = open(true);
    edit(&mut s, json!({"op": "pattern.add", "pattern": "fill", "length_beats": 2, "grid": "1/8"})).unwrap();
    edit(&mut s, json!({"op": "pattern.steps", "pattern": "fill", "pad": "h", "row": "x.x.xxxx x.x.xxxx"})).unwrap();
    assert!(edit(&mut s, json!({"op": "pattern.add", "pattern": "fill"})).unwrap_err().contains("already exists"));
    edit(&mut s, json!({"op": "pattern.steps", "pattern": "fill", "pad": "h"})).unwrap();
    assert_eq!(get(&s, "patterns.fill.steps"), json!({}));
    let r = edit(&mut s, json!({"op": "event.add", "pattern": "bass", "at": 3, "pad": "t", "note": "E3", "duration": "1/2"})).unwrap();
    let e = r["handle"].as_str().unwrap().to_string();
    edit(&mut s, json!({"op": "event.set", "event": e, "velocity": 90, "note": null})).unwrap();
    assert_eq!(get(&s, "patterns.bass.events.2"), json!({"at": 3, "pad": "t", "velocity": 90, "duration": "1/2"}));
    edit(&mut s, json!({"op": "pattern.duplicate", "pattern": "bass", "to": "bass2"})).unwrap();
    assert_eq!(get(&s, "patterns.bass2"), get(&s, "patterns.bass"));
    // The copy's events are new objects.
    assert_ne!(refs(&s, "patterns.bass2.events"), refs(&s, "patterns.bass.events"));
    edit(&mut s, json!({"op": "event.remove", "event": e})).unwrap();
    assert_eq!(get(&s, "patterns.bass.events").as_array().unwrap().len(), 2);
    edit(&mut s, json!({"op": "pad.set", "track": "bass", "pad": "t", "release_ms": 50, "transpose": 12})).unwrap();
    assert_eq!(get(&s, "tracks.bass.pads.t.transpose"), json!(12.0));
    let e = edit(&mut s, json!({"op": "pad.remove", "track": "bass", "pad": "t"})).unwrap_err();
    assert!(e.contains("bass: unknown pad t"), "{e}");
}

#[test]
fn effects_keep_their_lanes_pointed_at_them() {
    let (_d, mut s) = open(true);
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.drums", "type": "filter", "mode": "lowpass", "cutoff_hz": 800})).unwrap();
    edit(&mut s, json!({"op": "point.add", "owner": "tracks.drums", "param": "effects.0.cutoff_hz", "at": 0, "value": 400})).unwrap();
    // Inserting before the filter shifts the lane's index.
    let r = edit(&mut s, json!({"op": "effect.add", "owner": "tracks.drums", "type": "limiter", "index": 0})).unwrap();
    assert_eq!(r["also"], json!(["tracks.drums: lane effects.0.cutoff_hz is now effects.1.cutoff_hz"]));
    // Moving the filter back to the front follows it.
    edit(&mut s, json!({"op": "effect.move", "effect": "tracks.drums.effects.1", "index": 0})).unwrap();
    assert_eq!(get(&s, "tracks.drums.automation.0.param"), json!("effects.0.cutoff_hz"));
    edit(&mut s, json!({"op": "effect.bypass", "effect": "tracks.drums.effects.0", "bypass": true})).unwrap();
    assert_eq!(get(&s, "tracks.drums.effects.0.bypass"), json!(true));
    // Removing the filter removes its lane.
    let r = edit(&mut s, json!({"op": "effect.remove", "effect": "tracks.drums.effects.0"})).unwrap();
    assert_eq!(r["also"], json!(["tracks.drums: removed lane effects.0.cutoff_hz"]));
    assert_eq!(get(&s, "tracks.drums.effects.0.type"), json!("limiter"));
    // By id.
    edit(&mut s, json!({"op": "effect.add", "owner": "returns.plate", "type": "filter", "mode": "highpass", "id": "cut", "cutoff_hz": 200})).unwrap();
    edit(&mut s, json!({"op": "set", "path": "returns.plate.effects.cut.cutoff_hz", "value": 300})).unwrap();
    edit(&mut s, json!({"op": "point.add", "owner": "returns.plate", "param": "effects.cut.cutoff_hz", "at": 4, "value": 500})).unwrap();
    let r = edit(&mut s, json!({"op": "effect.remove", "effect": "returns.plate.effects.cut"})).unwrap();
    assert_eq!(r["also"], json!(["returns.plate: removed lane effects.cut.cutoff_hz"]));
}

#[test]
fn automation_points_stay_in_time_order() {
    let (_d, mut s) = open(true);
    let lane = "tracks.bass.automation.0.points";
    let r = edit(&mut s, json!({"op": "point.add", "owner": "tracks.bass", "param": "sends.plate.gain_db", "at": 8, "value": -12})).unwrap();
    assert_eq!(r["path"], json!(format!("{lane}.1")));
    let p = r["handle"].as_str().unwrap().to_string();
    edit(&mut s, json!({"op": "point.set", "point": p, "at": 20})).unwrap();
    assert_eq!(get(&s, lane), json!([{"at": 0, "value": -24.0}, {"at": 16, "value": -6.0}, {"at": 20, "value": -12.0}]));
    edit(&mut s, json!({"op": "lane.set", "owner": "master", "param": "gain_db", "points": [{"at": 0, "value": 0}]})).unwrap();
    // A lane's last point takes the lane with it.
    let r = edit(&mut s, json!({"op": "point.remove", "point": "master.automation.0.points.0"})).unwrap();
    assert_eq!(r["also"], json!(["master: removed lane gain_db, which had no points left"]));
    edit(&mut s, json!({"op": "lane.remove", "owner": "tracks.bass", "param": "sends.plate.gain_db"})).unwrap();
    assert_eq!(get(&s, "tracks.bass.automation"), json!([]));
}

#[test]
fn sections() {
    let (_d, mut s) = open(true);
    edit(&mut s, json!({"op": "section.add", "id": "drop", "at": 16, "length_beats": 8})).unwrap();
    edit(&mut s, json!({"op": "section.move", "section": "drop", "at": 24})).unwrap();
    assert!(edit(&mut s, json!({"op": "section.move", "section": "drop", "at": 28})).unwrap_err().contains("Section drop exceeds session"));
    edit(&mut s, json!({"op": "section.remove", "section": "intro"})).unwrap();
    assert_eq!(get(&s, "sections"), json!([{"id": "drop", "at": 24, "length_beats": 8}]));
}

#[test]
fn a_batch_is_one_step_and_all_or_nothing() {
    let (_d, mut s) = open(true);
    let before = s.doc().sha.clone();
    let bad = json!({"op": "batch", "commands": [
        {"op": "set", "path": "tracks.drums.gain_db", "value": -3},
        {"op": "clip.move", "clip": "tracks.drums.clips.0", "at": 31},
    ]});
    assert!(edit(&mut s, bad).is_err());
    assert_eq!(s.doc().sha, before);
    // Intermediate states may be invalid; the result is validated.
    let good = json!({"op": "batch", "commands": [
        {"op": "pad.remove", "track": "drums", "pad": "h"},
        {"op": "pad.add", "track": "drums", "pad": "h", "sample": "tone"},
        {"op": "set", "path": "tracks.drums.gain_db", "value": -3},
    ]});
    edit(&mut s, good).unwrap();
    assert_eq!(s.revision(), 1);
    s.undo(Origin::Agent, false).unwrap();
    assert_eq!(s.doc().sha, before);
    let nested = json!({"op": "batch", "commands": [{"op": "undo"}]});
    assert!(edit(&mut s, nested).unwrap_err().contains("undo cannot be batched"));
    // What a batch makes, its later commands can add to.
    let built = json!({"op": "batch", "commands": [
        {"op": "track.add", "id": "lead"},
        {"op": "pad.add", "track": "lead", "pad": "t", "sample": "tone"},
        {"op": "effect.add", "owner": "tracks.lead", "type": "limiter"},
        {"op": "pattern.add", "pattern": "riff", "length_beats": 2},
        {"op": "pattern.steps", "pattern": "riff", "pad": "t", "row": "x...x..."},
        {"op": "event.add", "pattern": "riff", "at": 1.5, "pad": "t"},
        {"op": "clip.add", "track": "lead", "pattern": "riff", "at": 4},
        {"op": "return.add", "id": "room"},
        {"op": "effect.add", "owner": "returns.room", "type": "reverb"},
        {"op": "send.set", "track": "lead", "to": "room", "gain_db": -6},
    ]});
    edit(&mut s, built).unwrap();
    assert_eq!(get(&s, "tracks.lead.clips"), json!([{"pattern": "riff", "at": 4}]));
    assert_eq!(get(&s, "patterns.riff"), json!({"length_beats": 2, "steps": {"t": "x...x..."}, "events": [{"at": 1.5, "pad": "t"}]}));
    assert_eq!(get(&s, "returns.room.effects.0.type"), json!("reverb"));
}

#[test]
fn undo_and_redo_are_logged_with_their_origin() {
    let (_d, mut s) = open(true);
    let original = s.doc().sha.clone();
    edit(&mut s, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -6})).unwrap();
    let edited = s.doc().sha.clone();
    s.undo(Origin::User, false).unwrap();
    assert_eq!(s.doc().sha, original);
    s.undo(Origin::User, true).unwrap();
    assert_eq!(s.doc().sha, edited);
    s.undo(Origin::Agent, false).unwrap();
    // A new edit clears redo.
    edit(&mut s, json!({"op": "toggle", "path": "tracks.drums.mute"})).unwrap();
    assert_eq!(s.undo(Origin::Agent, true).unwrap_err(), "Nothing to redo");
    let log = s.changes(0);
    let rows: Vec<(u64, &str, &str)> = log["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["revision"].as_u64().unwrap(), c["origin"].as_str().unwrap(), c["op"].as_str().unwrap()))
        .collect();
    assert_eq!(
        rows,
        [(1, "agent", "set"), (2, "user", "undo"), (3, "user", "redo"), (4, "agent", "undo"), (5, "agent", "toggle")]
    );
    assert_eq!(s.changes(3)["changes"].as_array().unwrap().len(), 2);
    // A no-op edit makes no revision.
    let r = edit(&mut s, json!({"op": "set", "path": "tracks.drums.mute", "value": true})).unwrap();
    assert_eq!(r["changed"], json!(false));
    assert_eq!(s.revision(), 5);
}

#[test]
fn saves_are_canonical_and_headless_matches_hosted() {
    let edits = [
        json!({"op": "set", "path": "tracks.drums.gain_db", "value": -4.5}),
        json!({"op": "clip.add", "track": "bass", "pattern": "bass", "at": "20/3"}),
        json!({"op": "effect.add", "owner": "master", "type": "limiter", "ceiling_db": -1}),
        json!({"op": "section.add", "id": "b", "at": 8, "length_beats": 8}),
    ];
    let mut files = Vec::new();
    for hosted in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let path = write_song(dir.path());
        for e in &edits {
            // Headless: a fresh session per command, as the CLI runs it.
            let mut s = Session::open(&path, hosted).unwrap();
            edit(&mut s, e.clone()).unwrap();
            s.save().unwrap();
            assert_eq!(file(&s), aaw_model::to_yaml(s.project()));
        }
        files.push(std::fs::read_to_string(&path).unwrap());
    }
    assert_eq!(files[0], files[1]);
}

#[test]
fn handles_need_a_host() {
    let (_d, mut s) = open(false);
    let e = edit(&mut s, json!({"op": "clip.remove", "clip": "@3"})).unwrap_err();
    assert!(e.starts_with("Handles such as @12 exist only while a host runs"), "{e}");
    assert_eq!(refs(&s, "tracks.drums.clips"), ["tracks.drums.clips.0", "tracks.drums.clips.1"]);
}

#[test]
fn stale_expectations_are_refused() {
    let (_d, mut s) = open(true);
    let sha = s.doc().sha.clone();
    let c = cmd(json!({"op": "set", "path": "tracks.drums.pan", "value": 0.5}));
    assert_eq!(s.edit(&c, Origin::Agent, Some("0".repeat(64).as_str()), None).unwrap_err(), "Stale project revision; inspect and retry");
    s.edit(&c, Origin::Agent, Some(&sha), None).unwrap();
}

#[test]
fn external_edits_load_as_undoable_steps() {
    let (_d, mut s) = open(true);
    edit(&mut s, json!({"op": "set", "path": "tracks.drums.gain_db", "value": -2})).unwrap();
    s.save().unwrap();
    let handles = refs(&s, "tracks.bass.clips");
    assert!(s.sync().is_none(), "the host's own write is not external");
    // A raw edit: the tempo changes and a clip is appended.
    let text = file(&s).replace("tempo: 120.0", "tempo: 100").replace(
        "- {pattern: bass, at: 16, repeats: 2}",
        "- {pattern: bass, at: 16, repeats: 2}\n  - {pattern: bass, at: 28}",
    );
    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(s.path(), &text).unwrap();
    let c = s.sync().expect("external change");
    assert_eq!((c.origin, c.op.as_str()), (Origin::External, "reload"));
    assert_eq!(get(&s, "session.tempo"), json!(100.0));
    // Existing clips keep their handles; the file is left as written.
    assert_eq!(&refs(&s, "tracks.bass.clips")[..3], &handles[..]);
    assert_eq!(file(&s), text);
    // An invalid edit pauses edits until fixed.
    std::fs::write(s.path(), text.replace("tempo: 100", "tempo: 9999")).unwrap();
    assert!(s.sync().is_none());
    assert!(s.invalid().unwrap().contains("session.tempo"));
    assert!(edit(&mut s, json!({"op": "toggle", "path": "tracks.drums.mute"})).unwrap_err().contains("edits wait until it is fixed"));
    std::fs::write(s.path(), &text).unwrap();
    assert!(s.sync().is_none(), "back to the loaded version");
    assert!(s.invalid().is_none());
    // Undo restores the host's version and writes it.
    s.undo(Origin::Agent, false).unwrap();
    s.save().unwrap();
    assert_eq!(get(&s, "session.tempo"), json!(120.0));
    assert!(file(&s).contains("tempo: 120.0"));
}

#[test]
fn apply_merges_into_the_song_and_keeps_handles() {
    let (_d, mut s) = open(true);
    let clips = refs(&s, "tracks.drums.clips");
    let patch = json!({"session": {"tempo": 90}, "tracks": [
        {"id": "drums", "pads": {"h": {"sample": "hit"}}, "clips": [{"pattern": "beat", "repeats": 4}, {"pattern": "beat", "at": 16, "repeats": 4}], "gain_db": -1},
        {"id": "bass", "pads": {"t": {"sample": "tone", "mode": "gate", "release_ms": 20}}, "clips": [{"pattern": "bass"}]},
    ]});
    let r = s.edit(&Command::Apply { patch, label: None }, Origin::Agent, None, None).unwrap().0;
    assert_eq!(r["label"], json!("Apply a merge patch"));
    assert_eq!(refs(&s, "tracks.drums.clips"), clips);
    assert_eq!(get(&s, "session.tempo"), json!(90.0));
    let _: Json = get(&s, "tracks");
    // A patch may say what it does, as a batch may.
    let named = json!({"op": "apply", "patch": {"session": {"tempo": 91}}, "label": "Slow down"});
    assert_eq!(edit(&mut s, named).unwrap()["label"], json!("Slow down"));
    assert_eq!(s.history().undo.unwrap().0, "Slow down");
}

#[test]
fn reordering_keys_is_a_change_although_the_sha_is_the_same() {
    let (_d, mut s) = open(true);
    edit(&mut s, json!({"op": "pad.add", "track": "drums", "pad": "k", "sample": "hit"})).unwrap();
    let sha = s.doc().sha.clone();
    let patch = json!({"tracks": [
        {"id": "drums", "pads": {"k": {"sample": "hit"}, "h": {"sample": "hit"}}, "clips": [{"pattern": "beat", "repeats": 4}, {"pattern": "beat", "at": 16, "repeats": 4}]},
        get(&s, "tracks.bass"),
    ]});
    let r = s.edit(&Command::Apply { patch, label: None }, Origin::Agent, None, None).unwrap().0;
    assert_eq!(r["changed"], json!(true));
    assert_eq!(s.doc().sha, sha);
    s.save().unwrap();
    let saved = aaw_model::load(s.path(), false).unwrap();
    assert_eq!(saved.tracks[0].pads.keys().collect::<Vec<_>>(), ["k", "h"]);
}

/// An edit that is part of a drag, as the app sends it.
fn drag(s: &mut Session, gesture: &str, j: Json) -> Json {
    s.edit(&cmd(j), Origin::User, None, Some(gesture)).unwrap().0
}

fn gain(db: f64) -> Json {
    json!({"op": "set", "path": "tracks.drums.gain_db", "value": db})
}

#[test]
fn a_gesture_is_one_undo_step_and_one_log_entry() {
    let (_d, mut s) = open(true);
    let start = s.doc().sha.clone();
    for db in [-1.0, -2.5, -3.0] {
        drag(&mut s, "g1", gain(db));
    }
    // Three revisions, one step named for the whole move, one log entry.
    assert_eq!(s.revision(), 3);
    let label = "Set tracks.drums.gain_db: 0.0 → -3.0";
    assert_eq!(s.history().undo, Some((label.to_string(), Origin::User)));
    let log = s.changes(0);
    let entries = log["changes"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!((&entries[0]["revision"], &entries[0]["label"], &entries[0]["gesture"]), (&json!(3), &json!(label), &json!("g1")));
    assert_eq!(entries[0]["command"]["value"], json!(-3.0));
    // An agent that had seen the drag begin is told where it ended.
    assert_eq!(s.changes(1)["changes"].as_array().unwrap().len(), 1);
    s.undo(Origin::User, false).unwrap();
    assert_eq!(s.doc().sha, start);
    s.undo(Origin::User, true).unwrap();
    assert_eq!(get(&s, "tracks.drums.gain_db"), json!(-3.0));

    // Another gesture is another step, as is the same one after someone
    // else's edit, or made by someone else.
    drag(&mut s, "g2", gain(-4.0));
    edit(&mut s, json!({"op": "set", "path": "tracks.bass.pan", "value": 0.2})).unwrap();
    drag(&mut s, "g2", gain(-5.0));
    s.edit(&cmd(gain(-6.0)), Origin::Agent, None, Some("g2")).unwrap();
    let labels: Vec<String> = (0..4).map(|_| s.undo(Origin::User, false).unwrap().0["label"].as_str().unwrap().to_string()).collect();
    assert_eq!(
        labels,
        [
            "Undo Set tracks.drums.gain_db: -5.0 → -6.0 (agent)",
            "Undo Set tracks.drums.gain_db: -4.0 → -5.0 (user)",
            "Undo Set tracks.bass.pan: 0.0 → 0.2 (agent)",
            "Undo Set tracks.drums.gain_db: -3.0 → -4.0 (user)",
        ]
    );

    // A drag that ends where it began leaves nothing to undo.
    let top = s.history().undo;
    drag(&mut s, "g3", gain(-9.0));
    drag(&mut s, "g3", gain(-3.0));
    assert_eq!(s.history().undo, top);
    // Commands of other kinds merge too, under the latest label.
    drag(&mut s, "g4", json!({"op": "send.set", "track": "drums", "to": "plate", "gain_db": -20}));
    drag(&mut s, "g4", json!({"op": "send.set", "track": "drums", "to": "plate", "gain_db": -10}));
    assert_eq!(s.history().undo, Some(("Set send drums → plate".to_string(), Origin::User)));
    s.undo(Origin::User, false).unwrap();
    assert_eq!(get(&s, "tracks.drums.sends"), json!([]));
}

#[test]
fn references_work_wherever_names_do() {
    let (_d, mut s) = open(true);
    let tracks = refs(&s, "tracks");
    let plate = refs(&s, "returns")[0].clone();
    // A return renamed by handle takes its sends and their lanes with it.
    let r = edit(&mut s, json!({"op": "return.rename", "return": plate, "to": "hall"})).unwrap();
    assert_eq!(r["label"], json!("Rename return plate to hall"));
    assert_eq!(get(&s, "tracks.bass.sends.0.to"), json!("hall"));
    assert_eq!(get(&s, "tracks.bass.automation.0.param"), json!("sends.hall.gain_db"));
    // Labels name objects, however the command referred to them.
    let r = edit(&mut s, json!({"op": "send.set", "track": tracks[0], "to": "hall", "gain_db": -9})).unwrap();
    assert_eq!(r["label"], json!("Set send drums → hall"));
    let r = edit(&mut s, json!({"op": "track.move", "track": tracks[1], "index": 0})).unwrap();
    assert_eq!(r["label"], json!("Move track bass to position 0"));
    assert_eq!(refs(&s, "tracks"), [tracks[1].clone(), tracks[0].clone()]);
    let r = edit(&mut s, json!({"op": "set", "path": format!("tracks.{}.gain_db", tracks[0]), "value": -2})).unwrap();
    assert_eq!(r["label"], json!("Set tracks.drums.gain_db: 0.0 → -2"));

    // Returns move as tracks do.
    edit(&mut s, json!({"op": "return.add", "id": "room"})).unwrap();
    let r = edit(&mut s, json!({"op": "return.move", "return": "room", "index": 0})).unwrap();
    assert_eq!(r["label"], json!("Move return room to position 0"));
    assert_eq!(get(&s, "returns.0.id"), json!("room"));
    assert_eq!(refs(&s, "returns")[1], plate);
    let e = edit(&mut s, json!({"op": "return.move", "return": "room", "index": 2})).unwrap_err();
    assert_eq!(e, "returns index 2 is past the end (2 items)");
}

#[test]
fn a_batch_reports_what_it_made() {
    let (_d, mut s) = open(true);
    let clips = refs(&s, "tracks.drums.clips");
    let r = edit(
        &mut s,
        json!({"op": "batch", "commands": [
            {"op": "clip.repeats", "clip": clips[1], "repeats": 2},
            {"op": "clip.duplicate", "clip": clips[1]},
            {"op": "clip.duplicate", "clip": clips[0], "at": 8},
        ]}),
    )
    .unwrap();
    assert_eq!(r["paths"], json!(["tracks.drums.clips.3", "tracks.drums.clips.1"]));
    let made: Vec<&str> = r["handles"].as_array().unwrap().iter().map(|h| h.as_str().unwrap()).collect();
    let now = refs(&s, "tracks.drums.clips");
    assert_eq!(made, [now[3].as_str(), now[1].as_str()]);
    assert_eq!(get(&s, "tracks.drums.clips.3.at"), json!(24));
    assert_eq!(r["label"].as_str().unwrap().matches("Duplicate clip beat at").count(), 2);
    // A batch can say what it does as a whole.
    let named = json!({"op": "batch", "label": "Thin the drums", "commands": [
        {"op": "clip.remove", "clip": now[3]},
        {"op": "set", "path": "tracks.drums.gain_db", "value": -3},
    ]});
    assert_eq!(cmd(named.clone()).json(), named);
    assert_eq!(edit(&mut s, named).unwrap()["label"], json!("Thin the drums"));
    assert_eq!(s.history().undo, Some(("Thin the drums".to_string(), Origin::Agent)));
}
