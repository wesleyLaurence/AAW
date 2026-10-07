//! `clip.join`: one clip of clips of one track that plays what they played,
//! as ⌘J makes it. Note clips join across gaps with their loops laid out;
//! pattern clips and audio clips only where they meet and are one music.

mod common;

use aaw_host::command::Origin;
use aaw_host::session::Session;
use common::{edit, get, refs, write_song};
use serde_json::{json, Value as Json};

fn open() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, true).unwrap();
    (dir, s)
}

/// A MIDI track with a phrase of two bars from beat 4 and a second clip a
/// bar later, from beat 14 to 18, whose first note's ID is the first's.
fn with_keys(s: &mut Session) {
    edit(s, json!({"op": "track.add", "id": "keys", "type": "midi"})).unwrap();
    edit(
        s,
        json!({"op": "clip.add", "track": "keys", "id": "a", "at": 4, "length_beats": 8, "notes": [
            {"id": "n1", "pitch": 60, "at": 0, "duration": 2},
            {"id": "n2", "pitch": 64, "at": 3, "duration": 2},
            {"id": "n3", "pitch": 67, "at": 7, "duration": 4},
            {"id": "n4", "pitch": 72, "at": 8, "duration": 1},
            {"id": "n5", "pitch": 48, "at": -1, "duration": 2},
        ]}),
    )
    .unwrap();
    edit(
        s,
        json!({"op": "clip.add", "track": "keys", "id": "b", "at": 14, "length_beats": 4, "notes": [
            {"id": "n1", "pitch": 62, "at": 0, "duration": 1, "velocity": 80},
            {"id": "n9", "pitch": 65, "at": 2, "duration": 1},
        ]}),
    )
    .unwrap();
}

fn notes(s: &Session, clip: &str) -> Vec<Json> {
    get(s, &format!("{clip}.notes")).as_array().unwrap().clone()
}

#[test]
fn note_clips_join_across_a_gap_with_the_notes_that_played() {
    let (_d, mut s) = open();
    with_keys(&mut s);
    let before = refs(&s, "tracks.keys.clips");
    let r = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.keys.clips.b", "tracks.keys.clips.a"]})).unwrap();
    // The first clip in time is kept, with its ID and handle.
    assert_eq!(r["label"], json!("Join 2 clips into a at 4"));
    assert_eq!(r["path"], json!("tracks.keys.clips.a"));
    assert_eq!(r["handle"], json!(before[0]));
    assert_eq!(refs(&s, "tracks.keys.clips"), [before[0].clone()]);
    let clip = get(&s, "tracks.keys.clips.a");
    assert_eq!((&clip["at"], &clip["length_beats"]), (&json!(4), &json!(14)));
    // Every note that played, where it played: the held note cut at its
    // clip's end, the notes before the clip and at its end left out, and the
    // second clip's notes ten beats on, one renamed where its ID was taken.
    assert_eq!(
        notes(&s, "tracks.keys.clips.a"),
        [
            json!({"id": "n1", "pitch": 60, "duration": 2}),
            json!({"id": "n2", "pitch": 64, "at": 3, "duration": 2}),
            json!({"id": "n3", "pitch": 67, "at": 7, "duration": 1}),
            json!({"id": "n10", "pitch": 62, "at": 10, "duration": 1, "velocity": 80}),
            json!({"id": "n9", "pitch": 65, "at": 12, "duration": 1}),
        ]
    );
    // One undo step brings both clips back.
    s.undo(Origin::User, false).unwrap();
    assert_eq!(refs(&s, "tracks.keys.clips"), before);
    assert_eq!(notes(&s, "tracks.keys.clips.a").len(), 5);
}

#[test]
fn a_looped_note_clip_is_laid_out_as_notes_when_joined() {
    let (_d, mut s) = open();
    with_keys(&mut s);
    // The first clip loops its first 3 beats to its end at beat 12: notes at
    // 0, 3, 6, 9 of C4, the E4 at 3 past the loop's end and silent.
    edit(&mut s, json!({"op": "clip.loop", "clip": "tracks.keys.clips.a", "loop_beats": 3})).unwrap();
    edit(&mut s, json!({"op": "clip.resize", "clip": "tracks.keys.clips.a", "length_beats": 10})).unwrap();
    edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.keys.clips.a", "tracks.keys.clips.b"]})).unwrap();
    let clip = get(&s, "tracks.keys.clips.a");
    assert_eq!(clip.get("loop_beats").map_or(true, Json::is_null), true);
    assert_eq!((&clip["at"], &clip["length_beats"]), (&json!(4), &json!(14)));
    let placed: Vec<(Json, Json, Json)> = notes(&s, "tracks.keys.clips.a")
        .iter()
        .map(|n| (n["pitch"].clone(), n.get("at").cloned().unwrap_or(json!(0)), n["duration"].clone()))
        .collect();
    // The last repetition, from beat 9, is cut at the clip's end at 10.
    assert_eq!(
        placed,
        [
            (json!(60), json!(0), json!(2)),
            (json!(60), json!(3), json!(2)),
            (json!(60), json!(6), json!(2)),
            (json!(60), json!(9), json!(1)),
            (json!(62), json!(10), json!(1)),
            (json!(65), json!(12), json!(1)),
        ]
    );
    // Each copy has an ID of its own.
    let ids: Vec<String> = notes(&s, "tracks.keys.clips.a").iter().map(|n| n["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(ids, ["n1", "n10", "n11", "n12", "n13", "n9"]);
}

#[test]
fn pattern_clips_that_meet_become_one_with_the_repeats_summed() {
    let (_d, mut s) = open();
    let before = refs(&s, "tracks.drums.clips");
    let r = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.drums.clips.0", "tracks.drums.clips.1"]})).unwrap();
    assert_eq!(r["label"], json!("Join 2 clips into beat at 0"));
    assert_eq!(get(&s, "tracks.drums.clips"), json!([{"pattern": "beat", "repeats": 8}]));
    assert_eq!(refs(&s, "tracks.drums.clips"), [before[0].clone()]);
    // Clips with a gap between them are refused, and so are different patterns.
    let e = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.bass.clips.1", "tracks.bass.clips.2"]})).unwrap_err();
    assert_eq!(e, "bass at 16 does not start where bass at 8 ends, at beat 12; clips joined into one meet there");
    edit(&mut s, json!({"op": "pattern.duplicate", "pattern": "bass", "to": "bass2"})).unwrap();
    edit(&mut s, json!({"op": "clip.add", "track": "bass", "pattern": "bass2", "at": 12})).unwrap();
    let e = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.bass.clips.1", "tracks.bass.clips.3"]})).unwrap_err();
    assert_eq!(e, "bass at 8 and bass2 at 12 play different patterns, so one clip cannot play what they play");
    // Three in a row, named in any order, as one step.
    edit(&mut s, json!({"op": "clip.remove", "clip": "tracks.bass.clips.3"})).unwrap();
    edit(&mut s, json!({"op": "clip.add", "track": "bass", "pattern": "bass", "at": 12})).unwrap();
    let r = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.bass.clips.2", "tracks.bass.clips.0", "tracks.bass.clips.3", "tracks.bass.clips.1"]})).unwrap();
    assert_eq!(r["label"], json!("Join 4 clips into bass at 0"));
    assert_eq!(get(&s, "tracks.bass.clips"), json!([{"pattern": "bass", "repeats": 6}]));
}

#[test]
fn audio_clips_join_back_into_what_a_split_divided() {
    let (_d, mut s) = open();
    let song: Vec<[f32; 2]> = (0..480000).map(|i| [((i % 4800) as f32 / 4800.0 - 0.5) * 0.2; 2]).collect();
    let dir = s.path().parent().unwrap().to_path_buf();
    std::fs::write(dir.join("song.wav"), aaw_engine::wav::float_wav_bytes(&song, 48000)).unwrap();
    edit(&mut s, json!({"op": "set", "path": "samples.song", "value": {"path": "song.wav"}})).unwrap();
    edit(&mut s, json!({"op": "track.add", "id": "song"})).unwrap();
    edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "song", "at": 4, "source_start_seconds": 1, "gain_db": -3, "fade_out_ms": 20})).unwrap();
    let whole = get(&s, "tracks.song.audio.0");
    let before = refs(&s, "tracks.song.audio");
    edit(&mut s, json!({"op": "audio.split", "clip": "tracks.song.audio.0", "at": 10})).unwrap();
    edit(&mut s, json!({"op": "audio.split", "clip": "tracks.song.audio.0", "at": "13/2"})).unwrap();
    assert_eq!(get(&s, "tracks.song.audio").as_array().unwrap().len(), 3);
    let r = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.song.audio.2", "tracks.song.audio.0", "tracks.song.audio.1"]})).unwrap();
    assert_eq!(r["label"], json!("Join 3 clips into song at 4"));
    assert_eq!(get(&s, "tracks.song.audio.0"), whole);
    assert_eq!(refs(&s, "tracks.song.audio"), before);
    // A join made by a cut, with a crossfade, is not continuous in the file.
    edit(&mut s, json!({"op": "audio.cut", "track": "song", "from": 8, "to": 12})).unwrap();
    let e = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.song.audio.0", "tracks.song.audio.1"]})).unwrap_err();
    assert!(e.starts_with("song at 8 does not go on in the file from where song at 4 leaves"), "{e}");
    // Nor are clips of two files, or one that plays to its file's end.
    edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "tone", "at": 20})).unwrap();
    edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "tone", "at": 20.5})).unwrap();
    let e = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.song.audio.2", "tracks.song.audio.3"]})).unwrap_err();
    assert_eq!(e, "Audio clip tone at 20 plays to its file's end, so nothing goes on from it");
}

#[test]
fn joins_are_of_one_track_and_one_kind() {
    let (_d, mut s) = open();
    let e = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.drums.clips.0", "tracks.bass.clips.0"]})).unwrap_err();
    assert_eq!(e, "The clips are on two tracks; clips joined are on one");
    let e = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.drums.clips.0"]})).unwrap_err();
    assert_eq!(e, "Name two or more clips of one track to join");
    let e = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.drums.clips.0", "tracks.drums.clips.0"]})).unwrap_err();
    assert_eq!(e, "Name two or more clips of one track to join");
    edit(&mut s, json!({"op": "audio.add", "track": "drums", "sample": "tone", "at": 31})).unwrap();
    let e = edit(&mut s, json!({"op": "clip.join", "clips": ["tracks.drums.clips.1", "tracks.drums.audio.0"]})).unwrap_err();
    assert_eq!(e, "Pattern clips and audio clips are not joined into one; join each kind by itself");
}
