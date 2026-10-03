//! MIDI tracks through the session's commands: note clips made, edited and
//! copied, instruments attached and swapped without touching the notes, the
//! notes read back, and all of it saved, reopened, undone and redone.

mod common;

use aaw_host::command::Origin;
use aaw_host::session::Session;
use common::{cmd, edit, get, write_song};
use serde_json::{json, Value as Json};

fn open(hosted: bool) -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, hosted).unwrap();
    (dir, s)
}

/// An instrumentless MIDI track with a chord and a melody in one clip.
fn phrase(s: &mut Session) {
    edit(s, json!({"op": "track.add", "id": "keys", "type": "midi"})).unwrap();
    let r = edit(s, json!({"op": "clip.add", "track": "keys", "at": 4, "length_beats": 4})).unwrap();
    assert_eq!(r["path"], json!("tracks.keys.clips.clip1"));
    let r = edit(
        s,
        json!({"op": "note.add", "clip": "tracks.keys.clips.clip1", "notes": [
            {"pitch": "C4", "duration": 1, "velocity": 96},
            {"pitch": 64, "duration": 1, "velocity": 80},
            {"pitch": 67, "duration": 1, "velocity": 88},
            {"pitch": 62, "at": 1.975, "duration": 0.5},
            {"pitch": 64, "at": 2.025, "duration": "1/3"},
            {"pitch": 65, "at": "7/3", "duration": "1/3"},
        ]}),
    )
    .unwrap();
    let paths: Vec<&str> = r["paths"].as_array().unwrap().iter().map(|p| p.as_str().unwrap()).collect();
    assert_eq!(paths, (1..=6).map(|i| format!("tracks.keys.clips.clip1.notes.n{i}")).collect::<Vec<_>>());
}

fn clips(s: &Session) -> Json {
    get(s, "tracks.keys.clips")
}

#[test]
fn notes_made_by_commands_are_saved_and_read_back_with_their_ids() {
    let (dir, mut s) = open(true);
    phrase(&mut s);
    let made = clips(&s);
    let notes = &made[0]["notes"];
    // A name is stored as its number; off-grid and triplet places are kept.
    assert_eq!(notes[0], json!({"id": "n1", "pitch": 60, "duration": 1, "velocity": 96}));
    assert_eq!((&notes[3]["at"], &notes[4]["at"], &notes[4]["duration"], &notes[5]["at"]), (&json!(1.975), &json!(2.025), &json!("1/3"), &json!("7/3")));
    s.save().unwrap();
    let text = std::fs::read_to_string(s.path()).unwrap();
    assert!(text.starts_with("schema_version: 2\n"), "{text}");
    let again = Session::open(&dir.path().join("song.yaml"), true).unwrap();
    assert_eq!(clips(&again), made);
    assert!(again.project().tracks[2].midi.as_ref().unwrap().instrument.is_none());
}

#[test]
fn each_field_of_a_note_is_edited_on_its_own_and_nothing_snaps() {
    let (_d, mut s) = open(true);
    phrase(&mut s);
    let n = "tracks.keys.clips.clip1.notes.n2";
    let r = edit(&mut s, json!({"op": "note.set", "note": n, "pitch": "G#4"})).unwrap();
    assert_eq!(r["label"], json!("Change note n2 of clip clip1 at 4"));
    edit(&mut s, json!({"op": "note.set", "note": n, "duration": "2/3"})).unwrap();
    edit(&mut s, json!({"op": "note.set", "note": n, "velocity": 30})).unwrap();
    assert_eq!(get(&s, n), json!({"id": "n2", "pitch": 68, "duration": "2/3", "velocity": 30}));
    // A move by a hair is kept exactly, and moving back restores the place.
    edit(&mut s, json!({"op": "note.move", "notes": ["tracks.keys.clips.clip1.notes.n4"], "by": "-1/48"})).unwrap();
    assert_eq!(get(&s, "tracks.keys.clips.clip1.notes.n4.at"), json!("469/240"));
    edit(&mut s, json!({"op": "note.move", "notes": ["tracks.keys.clips.clip1.notes.n4"], "by": "1/48"})).unwrap();
    assert_eq!(get(&s, "tracks.keys.clips.clip1.notes.n4.at"), json!(1.975));
    // A note moved before its clip is kept there and does not play; one
    // that would leave the pitch range is refused, not clamped.
    edit(&mut s, json!({"op": "note.move", "notes": [n], "by": "-1/480"})).unwrap();
    assert_eq!(get(&s, &format!("{n}.at")), json!("-1/480"));
    edit(&mut s, json!({"op": "note.move", "notes": [n], "by": "1/480"})).unwrap();
    assert_eq!(get(&s, &format!("{n}.at")), json!(0));
    let e = edit(&mut s, json!({"op": "note.transpose", "notes": ["tracks.keys.clips.clip1"], "by": 60})).unwrap_err();
    assert!(e.contains("would be pitch 128, outside 0 to 127"), "{e}");
    let e = edit(&mut s, json!({"op": "note.set", "note": n, "id": "n9"})).unwrap_err();
    assert_eq!(e, "A note's ID cannot be changed");
}

#[test]
fn a_copy_owns_its_notes() {
    let (dir, mut s) = open(true);
    phrase(&mut s);
    let original = get(&s, "tracks.keys.clips.clip1");
    let r = edit(&mut s, json!({"op": "clip.duplicate", "clip": "tracks.keys.clips.clip1"})).unwrap();
    assert_eq!(r["path"], json!("tracks.keys.clips.clip2"));
    let copy = "tracks.keys.clips.clip2";
    assert_eq!(get(&s, &format!("{copy}.at")), json!(8), "right after the original");
    edit(&mut s, json!({"op": "note.transpose", "notes": [copy], "by": 12})).unwrap();
    edit(&mut s, json!({"op": "note.move", "notes": [format!("{copy}.notes.n1")], "by": "1/3"})).unwrap();
    edit(&mut s, json!({"op": "note.set", "note": format!("{copy}.notes.n3"), "velocity": 20})).unwrap();
    edit(&mut s, json!({"op": "note.remove", "notes": [format!("{copy}.notes.n6")]})).unwrap();
    assert_eq!(get(&s, "tracks.keys.clips.clip1"), original);
    assert_eq!(get(&s, &format!("{copy}.notes.n1")), json!({"id": "n1", "pitch": 72, "at": "1/3", "duration": 1, "velocity": 96}));
    // Saved and reopened, and through undo and redo, the original stays.
    s.save().unwrap();
    let mut again = Session::open(&dir.path().join("song.yaml"), true).unwrap();
    assert_eq!(get(&again, "tracks.keys.clips.clip1"), original);
    let edited = clips(&s);
    for _ in 0..5 {
        s.undo(Origin::User, false).unwrap();
        assert_eq!(get(&s, "tracks.keys.clips.clip1"), original);
    }
    assert_eq!(clips(&s).as_array().unwrap().len(), 1);
    for _ in 0..5 {
        s.undo(Origin::User, true).unwrap();
    }
    assert_eq!(clips(&s), edited);
    // A note added to the copy takes the next ID of its clip.
    let r = edit(&mut again, json!({"op": "note.add", "clip": copy, "pitch": 48, "duration": 4})).unwrap();
    assert_eq!(r["path"], json!("tracks.keys.clips.clip2.notes.n6"));
}

#[test]
fn instruments_are_attached_replaced_and_removed_and_the_notes_stay() {
    let (_d, mut s) = open(true);
    phrase(&mut s);
    let notes = clips(&s);
    let piano = json!({"sampler": {"pads": {"p": {"sample": "tone", "mode": "gate", "release_ms": 30}}, "map": [{"notes": [0, 127], "pad": "p", "pitched": true}]}});
    let r = edit(&mut s, json!({"op": "instrument.set", "track": "keys", "instrument": piano})).unwrap();
    assert_eq!(r["label"], json!("Attach a sampler to keys"));
    let pitched: Vec<Option<String>> = aaw_model::schedule::schedule(s.project())
        .into_iter()
        .filter(|t| t.track_id == "keys")
        .map(|t| t.event.note)
        .collect();
    assert_eq!(pitched.len(), 6);
    assert_eq!(pitched[0].as_deref(), Some("C4"));
    assert_eq!(clips(&s), notes);
    let drums = json!({"sampler": {"pads": {"k": {"sample": "hit"}}, "map": [{"notes": 60, "pad": "k"}]}});
    let r = edit(&mut s, json!({"op": "instrument.set", "track": "keys", "instrument": drums})).unwrap();
    assert_eq!(r["label"], json!("Replace the instrument of keys with a sampler"));
    assert_eq!(clips(&s), notes);
    let hits = aaw_model::schedule::schedule(s.project()).into_iter().filter(|t| t.track_id == "keys").count();
    assert_eq!(hits, 1, "only note 60 is mapped");
    assert_eq!(aaw_model::rules::note_warnings(s.project()), ["keys.clip1: the sampler maps no pad to notes 62 (D4), 64 (E4), 65 (F4), 67 (G4), which are silent"]);
    let r = edit(&mut s, json!({"op": "instrument.set", "track": "keys", "instrument": null})).unwrap();
    assert_eq!(r["label"], json!("Remove the instrument of keys"));
    assert_eq!(clips(&s), notes);
    assert_eq!(get(&s, "tracks.keys").get("instrument"), None);
}

#[test]
fn pads_and_the_map_are_edited_on_a_midi_tracks_sampler() {
    let (_d, mut s) = open(false);
    phrase(&mut s);
    // A pad added to a track with no instrument makes a sampler for it.
    edit(&mut s, json!({"op": "pad.add", "track": "keys", "pad": "k", "sample": "hit"})).unwrap();
    edit(&mut s, json!({"op": "pad.add", "track": "keys", "pad": "t", "sample": "tone", "mode": "gate"})).unwrap();
    let r = edit(&mut s, json!({"op": "instrument.map", "track": "keys", "notes": "C2", "pad": "k"})).unwrap();
    assert_eq!(r["label"], json!("Map notes \"C2\" to k on keys"));
    edit(&mut s, json!({"op": "instrument.map", "track": "keys", "notes": [48, 72], "pad": "t", "pitched": true})).unwrap();
    edit(&mut s, json!({"op": "pad.set", "track": "keys", "pad": "t", "release_ms": 40})).unwrap();
    assert_eq!(
        get(&s, "tracks.keys.instrument"),
        json!({"sampler": {
            "pads": {"k": {"sample": "hit"}, "t": {"sample": "tone", "mode": "gate", "release_ms": 40.0}},
            "map": [{"notes": 36, "pad": "k"}, {"notes": [48, 72], "pad": "t", "pitched": true}],
        }})
    );
    let e = edit(&mut s, json!({"op": "instrument.map", "track": "keys", "notes": 60, "pad": "k"})).unwrap_err();
    assert!(e.contains("note 60 (C4) is mapped to both t and k"), "{e}");
    let e = edit(&mut s, json!({"op": "pad.remove", "track": "keys", "pad": "k"})).unwrap_err();
    assert!(e.contains("the map names unknown pad k"), "{e}");
    let e = edit(&mut s, json!({"op": "instrument.map", "track": "drums", "notes": 60, "pad": "h"})).unwrap_err();
    assert_eq!(e, "drums is not a MIDI track");
}

#[test]
fn clips_move_resize_and_keep_to_their_kind_of_track() {
    let (_d, mut s) = open(true);
    phrase(&mut s);
    let clip = "tracks.keys.clips.clip1";
    edit(&mut s, json!({"op": "clip.move", "clip": clip, "at": "33/4"})).unwrap();
    assert_eq!(get(&s, &format!("{clip}.at")), json!("33/4"));
    let e = edit(&mut s, json!({"op": "clip.move", "clip": clip, "track": "drums"})).unwrap_err();
    assert_eq!(e, "A note clip moves only to a MIDI track, and drums is not one");
    let e = edit(&mut s, json!({"op": "clip.move", "clip": "tracks.drums.clips.0", "track": "keys"})).unwrap_err();
    assert_eq!(e, "A pattern clip cannot move to MIDI track keys");
    let e = edit(&mut s, json!({"op": "clip.resize", "clip": "tracks.drums.clips.0", "length_beats": 2})).unwrap_err();
    assert_eq!(e, "tracks.drums.clips.0 is a pattern clip, not a note clip");
    // Made shorter, the clip keeps the notes past its end, which do not play.
    edit(&mut s, json!({"op": "clip.resize", "clip": clip, "length_beats": 2})).unwrap();
    let read = s.notes(clip, None, None).unwrap();
    let outside: Vec<&str> = read["clips"][0]["notes"].as_array().unwrap().iter().filter(|n| n["outside"] == json!(true)).map(|n| n["id"].as_str().unwrap()).collect();
    assert_eq!(outside, ["n5", "n6"]);
    assert_eq!(get(&s, clip)["notes"].as_array().unwrap().len(), 6);
    let e = edit(&mut s, json!({"op": "clip.resize", "clip": clip, "length_beats": 24})).unwrap_err();
    assert!(e.contains("keys: clip clip1 exceeds session"), "{e}");
    edit(&mut s, json!({"op": "clip.remove", "clip": clip})).unwrap();
    assert_eq!(clips(&s), json!([]));
}

#[test]
fn notes_are_read_by_clip_or_track_and_by_song_beats() {
    let (_d, mut s) = open(true);
    phrase(&mut s);
    edit(&mut s, json!({"op": "clip.duplicate", "clip": "tracks.keys.clips.clip1", "at": 16})).unwrap();
    let all = s.notes("keys", None, None).unwrap();
    assert_eq!(all["clips"].as_array().unwrap().len(), 2);
    let first = &all["clips"][0]["notes"][3];
    assert_eq!((&first["name"], &first["at"], &first["song_at"]), (&json!("D4"), &json!(1.975), &json!(5.975)));
    assert!(first["ref"].as_str().unwrap().starts_with('@'));
    // Notes starting from beat 6 until beat 17: the end of the first clip and
    // the chord of the second.
    let some = s.notes("tracks.keys", Some(&json!(6)), Some(&json!(17))).unwrap();
    let found: Vec<(String, String)> = some["clips"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["notes"].as_array().unwrap().iter().map(move |n| (c["id"].as_str().unwrap().to_string(), n["id"].as_str().unwrap().to_string())))
        .collect();
    let want = [("clip1", "n5"), ("clip1", "n6"), ("clip2", "n1"), ("clip2", "n2"), ("clip2", "n3")];
    assert_eq!(found, want.map(|(c, n)| (c.to_string(), n.to_string())));
    let e = s.notes("tracks.drums", None, None).unwrap_err();
    assert_eq!(e, "drums is not a MIDI track");
}

#[test]
fn a_labeled_batch_makes_and_varies_a_phrase_as_one_step() {
    let (_d, mut s) = open(true);
    phrase(&mut s);
    let before = clips(&s);
    let sha = s.doc().sha.clone();
    let batch = json!({"op": "batch", "label": "Answer the phrase an octave up", "commands": [
        {"op": "clip.duplicate", "clip": "tracks.keys.clips.clip1", "at": 12},
        {"op": "clip.add", "track": "keys", "at": 20, "length_beats": 4},
        {"op": "note.add", "clip": "tracks.keys.clips.clip2", "pitch": 72, "duration": 4},
    ]});
    let (reply, _) = s.edit(&cmd(batch.clone()), Origin::Agent, Some(&sha), None).unwrap();
    assert_eq!(reply["label"], json!("Answer the phrase an octave up"));
    assert_eq!(reply["paths"], json!(["tracks.keys.clips.clip2", "tracks.keys.clips.clip3", "tracks.keys.clips.clip2.notes.n7"]));
    // The same expectation again is stale.
    let stale = s.edit(&cmd(batch), Origin::Agent, Some(&sha), None).unwrap_err();
    assert_eq!(stale, "Stale project revision; inspect and retry");
    s.undo(Origin::User, false).unwrap();
    assert_eq!(clips(&s), before);
}

#[test]
fn a_note_keeps_its_handle_as_others_are_added_and_removed() {
    let (_d, mut s) = open(true);
    phrase(&mut s);
    let refs = |s: &Session| -> Vec<String> {
        s.get("tracks.keys.clips.clip1.notes").unwrap().as_array().unwrap().iter().map(|n| n["ref"].as_str().unwrap().to_string()).collect()
    };
    let before = refs(&s);
    edit(&mut s, json!({"op": "note.remove", "notes": [before[0].clone()]})).unwrap();
    edit(&mut s, json!({"op": "note.add", "clip": "tracks.keys.clips.clip1", "pitch": 50, "duration": 1})).unwrap();
    let after = refs(&s);
    assert_eq!(&after[..5], &before[1..]);
    edit(&mut s, json!({"op": "note.set", "note": before[3].clone(), "velocity": 1})).unwrap();
    assert_eq!(get(&s, "tracks.keys.clips.clip1.notes.n4.velocity"), json!(1));
}
