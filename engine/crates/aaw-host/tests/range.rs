//! Ranges of beats across tracks: copy, insert, delete and clear, and a
//! section duplicated, moved or removed with what is under it.

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

fn file(s: &Session) -> String {
    std::fs::read_to_string(s.path()).unwrap()
}

/// A MIDI track with a note clip of two bars from beat 4, a note across beat 8.
fn with_keys(s: &mut Session) {
    edit(s, json!({"op": "track.add", "id": "keys", "type": "midi"})).unwrap();
    edit(s, json!({"op": "synth.add", "track": "keys"})).unwrap();
    edit(
        s,
        json!({"op": "clip.add", "track": "keys", "id": "phrase", "at": 4, "length_beats": 8, "notes": [
            {"pitch": 60, "at": 0, "duration": 2},
            {"pitch": 64, "at": 3, "duration": 2},
            {"pitch": 67, "at": 6, "duration": 1},
        ]}),
    )
    .unwrap();
}

/// A track of one audio clip: ten seconds of a file at 120 BPM, its second 1
/// playing on beat 4, so beat 8 is second 3.
fn with_audio(s: &mut Session) {
    let song: Vec<[f32; 2]> = (0..480000).map(|i| [((i % 4800) as f32 / 4800.0 - 0.5) * 0.2; 2]).collect();
    let dir = s.path().parent().unwrap().to_path_buf();
    std::fs::write(dir.join("song.wav"), aaw_engine::wav::float_wav_bytes(&song, 48000)).unwrap();
    edit(s, json!({"op": "set", "path": "samples.song", "value": {"path": "song.wav"}})).unwrap();
    edit(s, json!({"op": "track.add", "id": "song"})).unwrap();
    edit(s, json!({"op": "audio.add", "track": "song", "sample": "song", "at": 4, "source_start_seconds": 1, "source_end_seconds": 9})).unwrap();
}

#[test]
fn insert_then_delete_gives_the_song_back() {
    let (_d, mut s) = open();
    with_keys(&mut s);
    with_audio(&mut s);
    edit(&mut s, json!({"op": "lane.set", "owner": "master", "param": "gain_db", "points": [{"at": 0, "value": -6}, {"at": 16, "value": -6, "curve": "hold"}, {"at": 24, "value": 0}]})).unwrap();
    edit(&mut s, json!({"op": "section.add", "id": "verse", "at": 8, "length_beats": 16})).unwrap();
    let before = file(&s);
    // Eight beats opened at beat 8: everything from there moves later, clips
    // across beat 8 are cut there, and the song grows.
    let r = edit(&mut s, json!({"op": "range.insert", "at": 8, "length": 8})).unwrap();
    assert_eq!(r["label"], json!("Insert 8 beats at 8"));
    assert_eq!(r["length_beats"], json!(40));
    assert_eq!(get(&s, "session.length_beats"), json!(40));
    // The drums' first clip of four repeats is two of two, the second moves.
    assert_eq!(
        get(&s, "tracks.drums.clips"),
        json!([{"pattern": "beat", "repeats": 2}, {"pattern": "beat", "at": 16, "repeats": 2}, {"pattern": "beat", "at": 24, "repeats": 4}])
    );
    // The bass clip that ends at 8 is not cut; the rest move.
    assert_eq!(
        get(&s, "tracks.bass.clips"),
        json!([{"pattern": "bass", "repeats": 2}, {"pattern": "bass", "at": 16}, {"pattern": "bass", "at": 24, "repeats": 2}])
    );
    // The note clip from 4 to 12 is cut at 8: the note across it is held to
    // the cut and the rest starts the right clip, which moves to 16.
    assert_eq!(
        get(&s, "tracks.keys.clips"),
        json!([
            {"id": "phrase", "at": 4, "length_beats": 4, "notes": [{"id": "n1", "pitch": 60, "duration": 2}, {"id": "n2", "pitch": 64, "at": 3, "duration": 1}]},
            {"id": "clip1", "at": 16, "length_beats": 4, "notes": [{"id": "n2", "pitch": 64, "duration": 1}, {"id": "n3", "pitch": 67, "at": 2, "duration": 1}]},
        ])
    );
    // The audio clip is split where its audio is, unfaded at the cut, and
    // the right half moves with the cut edges faded, since they now meet
    // other audio.
    assert_eq!(
        get(&s, "tracks.song.audio"),
        json!([
            {"sample": "song", "at": 4, "source_start_seconds": 1.0, "source_end_seconds": 3.0, "fade_out_ms": 12.0},
            {"sample": "song", "at": 16, "source_start_seconds": 3.0, "source_end_seconds": 9.0, "fade_in_ms": 4.0},
        ])
    );
    // The bass lane ramps from -24 at 0 to -6 at 16: it holds -15 across
    // the new beats. The master's lane has nothing to hold at beat 8, so it
    // only moves.
    assert_eq!(
        get(&s, "tracks.bass.automation.0.points"),
        json!([{"at": 0, "value": -24.0}, {"at": 8, "value": -15.0}, {"at": 16, "value": -15.0}, {"at": 24, "value": -6.0}])
    );
    assert_eq!(
        get(&s, "master.automation.0.points"),
        json!([{"at": 0, "value": -6.0}, {"at": 24, "value": -6.0, "curve": "hold"}, {"at": 32, "value": 0.0}])
    );
    // The intro ends at 8 and stays; the verse starts there and moves.
    assert_eq!(get(&s, "sections"), json!([{"id": "intro", "at": 0, "length_beats": 8}, {"id": "verse", "at": 16, "length_beats": 16}]));
    assert_eq!(r["split"], json!(["tracks.drums.clips.0", "tracks.keys.clips.phrase", "tracks.song.audio.0"]));
    assert_eq!(r["moved"].as_array().unwrap().len(), 6);
    assert_eq!(r["removed"], json!([]));
    // The same beats deleted close the gap and give the song back, byte for
    // byte: the cut clips are one again, the divided note whole, the lane as
    // it was.
    let r = edit(&mut s, json!({"op": "range.delete", "start": 8, "length": 8})).unwrap();
    assert_eq!(r["label"], json!("Delete beats 8–16"));
    assert_eq!(file(&s), before);
    // One undo of each.
    s.undo(Origin::User, false).unwrap();
    assert_eq!(get(&s, "session.length_beats"), json!(40));
    s.undo(Origin::User, false).unwrap();
    assert_eq!(file(&s), before);
}

#[test]
fn a_copy_replaces_what_is_at_its_destination_and_owns_its_notes() {
    let (_d, mut s) = open();
    with_keys(&mut s);
    edit(&mut s, json!({"op": "set", "path": "session.length_beats", "value": 48})).unwrap();
    // Beats 4 to 12, the keys' phrase and the drums and bass under it, again
    // at 28, over the drums there.
    let r = edit(&mut s, json!({"op": "range.copy", "start": 4, "length": 8, "to": 28})).unwrap();
    assert_eq!(r["label"], json!("Copy beats 4–12 to 28"));
    // The drums' second clip, 16 to 32, is cut at 28 and the piece there
    // replaced by two bars of the first clip, which is not touched.
    assert_eq!(
        get(&s, "tracks.drums.clips"),
        json!([
            {"pattern": "beat", "repeats": 4},
            {"pattern": "beat", "at": 16, "repeats": 3},
            {"pattern": "beat", "at": 28, "repeats": 2},
        ])
    );
    // The copy of the note clip is a clip of its own with its own notes.
    let keys = get(&s, "tracks.keys.clips");
    assert_eq!(keys[0]["id"], json!("phrase"));
    assert_eq!(keys[1]["id"], json!("clip1"));
    assert_eq!(keys[1]["at"], json!(28));
    assert_eq!(keys[1]["notes"], keys[0]["notes"]);
    assert_ne!(refs(&s, "tracks.keys.clips.clip1.notes"), refs(&s, "tracks.keys.clips.phrase.notes"));
    // The bass plays its second repeat and its clip at 8 in the range.
    assert_eq!(
        get(&s, "tracks.bass.clips"),
        json!([{"pattern": "bass", "repeats": 2}, {"pattern": "bass", "at": 8}, {"pattern": "bass", "at": 16, "repeats": 2}, {"pattern": "bass", "at": 28}, {"pattern": "bass", "at": 32}])
    );
    // The bass lane, -24 at 0 to -6 at 16 then held, plays the copied range's
    // ramp from -19.5 to -10.5 at the destination, and holds -6 around it.
    assert_eq!(
        get(&s, "tracks.bass.automation.0.points"),
        json!([
            {"at": 0, "value": -24.0},
            {"at": 16, "value": -6.0},
            {"at": 28, "value": -6.0},
            {"at": 28, "value": -19.5},
            {"at": 36, "value": -10.5},
            {"at": 36, "value": -6.0},
        ])
    );
    assert_eq!(r["paths"], json!(["tracks.drums.clips.2", "tracks.bass.clips.3", "tracks.bass.clips.4", "tracks.keys.clips.clip1"]));
    assert_eq!(r["split"], json!(["tracks.drums.clips.1"]));
    assert_eq!(r["removed"], json!(["tracks.drums.clips.2"]));
    assert_eq!(r["sections"], json!([]));
    // One undo takes the whole copy back.
    s.undo(Origin::User, false).unwrap();
    assert_eq!(get(&s, "tracks.keys.clips").as_array().unwrap().len(), 1);
    assert_eq!(get(&s, "tracks.drums.clips").as_array().unwrap().len(), 2);
}

#[test]
fn a_copy_can_insert_and_a_copy_past_the_end_grows_the_song() {
    let (_d, mut s) = open();
    // The whole song again after itself, pushing nothing: the song doubles.
    let r = edit(&mut s, json!({"op": "range.copy", "start": 0, "length": 32, "to": 32, "insert": true})).unwrap();
    assert_eq!(r["label"], json!("Insert a copy of beats 0–32 at 32"));
    assert_eq!(get(&s, "session.length_beats"), json!(64));
    assert_eq!(get(&s, "tracks.drums.clips").as_array().unwrap().len(), 4);
    assert_eq!(get(&s, "tracks.drums.clips.3"), json!({"pattern": "beat", "at": 48, "repeats": 4}));
    // The section is copied under a free name.
    assert_eq!(get(&s, "sections"), json!([{"id": "intro", "at": 0, "length_beats": 8}, {"id": "intro-2", "at": 32, "length_beats": 8}]));
    assert_eq!(r["sections"], json!(["intro-2"]));
    // The lane's ramp is repeated, with a jump at 32 from where the first
    // run ended to where the copy starts, so the first run is unchanged.
    assert_eq!(
        get(&s, "tracks.bass.automation.0.points"),
        json!([{"at": 0, "value": -24.0}, {"at": 16, "value": -6.0}, {"at": 32, "value": -6.0}, {"at": 32, "value": -24.0}, {"at": 48, "value": -6.0}])
    );
    // Inserted in the middle, what follows moves later.
    edit(&mut s, json!({"op": "range.copy", "start": 0, "length": 4, "to": 16, "insert": true})).unwrap();
    assert_eq!(get(&s, "session.length_beats"), json!(68));
    assert_eq!(get(&s, "tracks.drums.clips"), json!([
        {"pattern": "beat", "repeats": 4}, {"pattern": "beat", "at": 20, "repeats": 4},
        {"pattern": "beat", "at": 36, "repeats": 4}, {"pattern": "beat", "at": 52, "repeats": 4}, {"pattern": "beat", "at": 16},
    ]));
    assert_eq!(get(&s, "sections.1"), json!({"id": "intro-2", "at": 36, "length_beats": 8}));
    // A copy past the end grows the song to the end of the bar it reaches.
    edit(&mut s, json!({"op": "range.copy", "start": 0, "length": 4, "to": 70})).unwrap();
    assert_eq!(get(&s, "session.length_beats"), json!(76));
}

#[test]
fn clear_removes_the_range_and_moves_nothing_and_tracks_limit_a_range() {
    let (_d, mut s) = open();
    edit(&mut s, json!({"op": "set", "path": "session.length_beats", "value": 48})).unwrap();
    // The drums alone, out for bars 5 and 6: their clip is cut either side,
    // the bass and the lane untouched.
    let r = edit(&mut s, json!({"op": "range.clear", "start": 16, "length": 8, "tracks": ["drums"]})).unwrap();
    assert_eq!(r["label"], json!("Clear beats 16–24 on drums"));
    assert_eq!(get(&s, "tracks.drums.clips"), json!([{"pattern": "beat", "repeats": 4}, {"pattern": "beat", "at": 24, "repeats": 2}]));
    assert_eq!(get(&s, "tracks.bass.clips").as_array().unwrap().len(), 3);
    assert_eq!(get(&s, "tracks.bass.automation.0.points"), json!([{"at": 0, "value": -24.0}, {"at": 16, "value": -6.0}]));
    assert_eq!(r["removed"], json!(["tracks.drums.clips.1"]));
    assert_eq!(get(&s, "session.length_beats"), json!(48));
    // Cleared across the song, the lane loses the point inside the range,
    // keeps its values at the edges and runs straight between them; a
    // range that holds no point leaves a lane as it was; the sections stay.
    edit(&mut s, json!({"op": "range.clear", "start": 12, "length": 8})).unwrap();
    assert_eq!(get(&s, "tracks.bass.automation.0.points"), json!([{"at": 0, "value": -24.0}, {"at": 12, "value": -10.5}, {"at": 20, "value": -6.0}]));
    assert_eq!(get(&s, "tracks.drums.clips"), json!([{"pattern": "beat", "repeats": 3}, {"pattern": "beat", "at": 24, "repeats": 2}]));
    assert_eq!(get(&s, "tracks.bass.clips"), json!([{"pattern": "bass", "repeats": 2}, {"pattern": "bass", "at": 8}, {"pattern": "bass", "at": 20}]));
    assert_eq!(get(&s, "sections"), json!([{"id": "intro", "at": 0, "length_beats": 8}]));
    edit(&mut s, json!({"op": "range.clear", "start": 4, "length": 4})).unwrap();
    assert_eq!(get(&s, "tracks.bass.automation.0.points"), json!([{"at": 0, "value": -24.0}, {"at": 12, "value": -10.5}, {"at": 20, "value": -6.0}]));
    // Deleted on one track, the song keeps its length and the other tracks
    // their places; the lane begins with the value it had after the range.
    let r = edit(&mut s, json!({"op": "range.delete", "start": 0, "length": 16, "tracks": ["bass"]})).unwrap();
    assert_eq!(r["label"], json!("Delete beats 0–16 on bass"));
    assert_eq!(get(&s, "tracks.bass.clips"), json!([{"pattern": "bass", "at": 4}]));
    assert_eq!(get(&s, "tracks.bass.automation.0.points"), json!([{"at": 0, "value": -8.25}, {"at": 4, "value": -6.0}]));
    assert_eq!(get(&s, "session.length_beats"), json!(48));
    assert_eq!(get(&s, "tracks.drums.clips.0"), json!({"pattern": "beat"}));
}

#[test]
fn a_pattern_clip_cut_inside_a_repeat_is_refused_and_ranges_are_checked() {
    let (_d, mut s) = open();
    let e = edit(&mut s, json!({"op": "range.insert", "at": 6, "length": 4})).unwrap_err();
    assert_eq!(
        e,
        "Beat 6 falls inside a repeat of clip tracks.drums.clips.0, whose pattern beat is 4 beats long; a pattern clip is cut only between repeats"
    );
    // Nothing of a refused command lands.
    assert_eq!(get(&s, "session.length_beats"), json!(32));
    assert_eq!(edit(&mut s, json!({"op": "range.delete", "start": 24, "length": 16})).unwrap_err(), "Beats 24–40 reach past the song's end at 32");
    assert_eq!(edit(&mut s, json!({"op": "range.clear", "start": 32, "length": 4})).unwrap_err(), "Beats 32–36 are past the song's end at 32");
    assert_eq!(edit(&mut s, json!({"op": "range.copy", "start": 0, "length": 0, "to": 8})).unwrap_err(), "LENGTH is a number of beats above zero");
    assert_eq!(edit(&mut s, json!({"op": "range.insert", "at": 40, "length": 4})).unwrap_err(), "Beat 40 is past the song's end at 32");
    assert!(edit(&mut s, json!({"op": "range.clear", "start": 0, "length": 4, "tracks": ["nobody"]})).is_err());
}

#[test]
fn sections_duplicate_move_and_go_with_their_content() {
    let (_d, mut s) = open();
    with_keys(&mut s);
    edit(&mut s, json!({"op": "section.add", "id": "verse", "at": 8, "length_beats": 8})).unwrap();
    // The verse again right after itself, pushing the rest later.
    let r = edit(&mut s, json!({"op": "section.duplicate", "section": "verse"})).unwrap();
    assert_eq!(r["label"], json!("Duplicate section verse at 16 as verse-2"));
    assert_eq!(get(&s, "session.length_beats"), json!(40));
    assert_eq!(
        get(&s, "sections"),
        json!([{"id": "intro", "at": 0, "length_beats": 8}, {"id": "verse", "at": 8, "length_beats": 8}, {"id": "verse-2", "at": 16, "length_beats": 8}])
    );
    assert_eq!(
        get(&s, "tracks.bass.clips"),
        json!([{"pattern": "bass", "repeats": 2}, {"pattern": "bass", "at": 8}, {"pattern": "bass", "at": 24, "repeats": 2}, {"pattern": "bass", "at": 16}])
    );
    // What the keys' phrase, 4 to 12, plays from 8 is copied to 16; the
    // phrase itself is not touched.
    assert_eq!(get(&s, "tracks.keys.clips").as_array().unwrap().len(), 2);
    assert_eq!(get(&s, "tracks.keys.clips.0.length_beats"), json!(8));
    assert_eq!(get(&s, "tracks.keys.clips.1"), json!({"id": "clip1", "at": 16, "length_beats": 4, "notes": [{"id": "n2", "pitch": 64, "duration": 1}, {"id": "n3", "pitch": 67, "at": 2, "duration": 1}]}));
    // Named, and somewhere else.
    let r = edit(&mut s, json!({"op": "section.duplicate", "section": "intro", "to": 40, "id": "outro"})).unwrap();
    assert_eq!(r["label"], json!("Duplicate section intro at 40 as outro"));
    assert_eq!(get(&s, "sections.3"), json!({"id": "outro", "at": 40, "length_beats": 8}));
    assert_eq!(get(&s, "session.length_beats"), json!(48));
    assert_eq!(edit(&mut s, json!({"op": "section.duplicate", "section": "intro", "id": "verse"})).unwrap_err(), "Section ID verse is taken");
    // Removed with its content, the song closes up and shrinks, and the
    // drums' clips that meet where the gap closed are one again.
    let r = edit(&mut s, json!({"op": "section.remove", "section": "verse-2", "with_content": true})).unwrap();
    assert_eq!(r["label"], json!("Remove section verse-2 with its content"));
    assert_eq!(get(&s, "session.length_beats"), json!(40));
    assert_eq!(get(&s, "sections"), json!([{"id": "intro", "at": 0, "length_beats": 8}, {"id": "verse", "at": 8, "length_beats": 8}, {"id": "outro", "at": 32, "length_beats": 8}]));
    assert_eq!(get(&s, "tracks.bass.clips"), json!([{"pattern": "bass", "repeats": 2}, {"pattern": "bass", "at": 8}, {"pattern": "bass", "at": 16, "repeats": 2}, {"pattern": "bass", "at": 32, "repeats": 2}]));
    assert_eq!(get(&s, "tracks.drums.clips"), json!([{"pattern": "beat", "repeats": 8}, {"pattern": "beat", "at": 32, "repeats": 2}]));
    // The outro's copy of the phrase's first bar moved earlier with it.
    assert_eq!(
        get(&s, "tracks.keys.clips"),
        json!([
            {"id": "phrase", "at": 4, "length_beats": 8, "notes": [{"id": "n1", "pitch": 60, "duration": 2}, {"id": "n2", "pitch": 64, "at": 3, "duration": 2}, {"id": "n3", "pitch": 67, "at": 6, "duration": 1}]},
            {"id": "clip2", "at": 36, "length_beats": 4, "notes": [{"id": "n1", "pitch": 60, "duration": 2}, {"id": "n2", "pitch": 64, "at": 3, "duration": 1}]},
        ])
    );
    // Moved with its content, the beats go over what is at the destination
    // and leave their place empty; the label goes with them.
    let r = edit(&mut s, json!({"op": "section.move", "section": "intro", "at": 24, "with_content": true})).unwrap();
    assert_eq!(r["label"], json!("Move section intro to 24 with its content"));
    assert_eq!(get(&s, "sections.0"), json!({"id": "intro", "at": 24, "length_beats": 8}));
    assert_eq!(get(&s, "tracks.drums.clips"), json!([{"pattern": "beat", "at": 8, "repeats": 4}, {"pattern": "beat", "at": 32, "repeats": 2}, {"pattern": "beat", "at": 24, "repeats": 2}]));
    assert_eq!(get(&s, "tracks.bass.clips"), json!([{"pattern": "bass", "at": 8}, {"pattern": "bass", "at": 16, "repeats": 2}, {"pattern": "bass", "at": 32, "repeats": 2}, {"pattern": "bass", "at": 24, "repeats": 2}]));
    // The label alone still moves alone, and is removed alone.
    edit(&mut s, json!({"op": "section.move", "section": "intro", "at": 0})).unwrap();
    assert_eq!(get(&s, "sections.0.at"), json!(0));
    assert_eq!(get(&s, "tracks.drums.clips.2.at"), json!(24));
    edit(&mut s, json!({"op": "section.remove", "section": "intro"})).unwrap();
    assert_eq!(get(&s, "sections").as_array().unwrap().len(), 2);
    assert_eq!(get(&s, "session.length_beats"), json!(40));
}

#[test]
fn a_copied_range_renders_as_the_clips_it_stands_for() {
    // A copy of eight beats across the tracks makes what duplicating each
    // clip and editing the lane by hand would, and renders to the same bytes.
    let (_d, mut s) = open();
    with_keys(&mut s);
    edit(&mut s, json!({"op": "set", "path": "session.length_beats", "value": 48})).unwrap();
    let dir = s.path().parent().unwrap().to_path_buf();
    let render = |s: &mut Session, name: &str| -> Vec<u8> {
        s.save().unwrap();
        let out = dir.join(name);
        let options = aaw_engine::offline::RenderOptions { output: Some(out.clone()), ..Default::default() };
        aaw_engine::offline::render(s.path(), &options).unwrap();
        std::fs::read(out.join("mix.wav")).unwrap()
    };
    edit(&mut s, json!({"op": "range.copy", "start": 0, "length": 8, "to": 40})).unwrap();
    let by_range = render(&mut s, "by_range");
    s.undo(Origin::User, false).unwrap();
    edit(
        &mut s,
        json!({"op": "batch", "commands": [
            {"op": "clip.add", "track": "drums", "pattern": "beat", "at": 40, "repeats": 2},
            {"op": "clip.add", "track": "bass", "pattern": "bass", "at": 40, "repeats": 2},
            {"op": "clip.duplicate", "clip": "tracks.keys.clips.phrase", "at": 44},
            {"op": "clip.trim", "clip": "tracks.keys.clips.clip1", "end": 48},
            {"op": "note.set", "note": "tracks.keys.clips.clip1.notes.n2", "duration": 1},
            {"op": "note.remove", "notes": ["tracks.keys.clips.clip1.notes.n3"]},
            {"op": "point.add", "owner": "tracks.bass", "param": "sends.plate.gain_db", "at": 40, "value": -6},
            {"op": "point.add", "owner": "tracks.bass", "param": "sends.plate.gain_db", "at": 40, "value": -24},
            {"op": "point.add", "owner": "tracks.bass", "param": "sends.plate.gain_db", "at": 48, "value": -15},
            {"op": "point.add", "owner": "tracks.bass", "param": "sends.plate.gain_db", "at": 48, "value": -6},
        ]}),
    )
    .unwrap();
    let by_hand = render(&mut s, "by_hand");
    assert!(by_range == by_hand, "the renders differ");
}

#[test]
fn range_commands_round_trip_as_json() {
    let commands = [
        json!({"op": "range.copy", "start": 32, "length": 32, "to": 96}),
        json!({"op": "range.copy", "start": 32, "length": 32, "to": 96, "insert": true, "tracks": ["drums"]}),
        json!({"op": "range.insert", "at": 64, "length": 16}),
        json!({"op": "range.delete", "start": 64, "length": "16/3", "tracks": ["drums", "bass"]}),
        json!({"op": "range.clear", "start": 64, "length": 16}),
        json!({"op": "section.duplicate", "section": "verse", "to": 96, "id": "verse2"}),
        json!({"op": "section.duplicate", "section": "verse"}),
        json!({"op": "section.move", "section": "verse", "at": 96, "with_content": true}),
        json!({"op": "section.move", "section": "verse", "at": 96}),
        json!({"op": "section.remove", "section": "verse", "with_content": true}),
        json!({"op": "section.remove", "section": "verse"}),
    ];
    for c in commands {
        let parsed: aaw_host::command::Command = serde_json::from_value(c.clone()).unwrap();
        assert_eq!(parsed.json(), c);
    }
    let _: Json = json!(null);
}
