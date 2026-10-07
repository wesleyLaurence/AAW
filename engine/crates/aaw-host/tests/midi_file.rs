//! MIDI files through the session's commands: a note clip exported and
//! imported again, a file's notes made into a clip on a new or a given MIDI
//! track as one undo step, and what the song cannot hold counted (D65).

mod common;

use aaw_host::command::Origin;
use aaw_host::midi_file::{self, FileNote};
use aaw_host::session::Session;
use common::{edit, get, write_song};
use serde_json::{json, Value as Json};
use std::path::Path;

fn open() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, true).unwrap();
    (dir, s)
}

/// A MIDI track with a chord, notes off the beat and on triplets, a note
/// that lasts past the clip's end and one after it.
fn phrase(s: &mut Session) {
    edit(s, json!({"op": "track.add", "id": "keys", "type": "midi"})).unwrap();
    edit(
        s,
        json!({"op": "clip.add", "track": "keys", "at": 4, "length_beats": 4, "notes": [
            {"pitch": 60, "duration": 1, "velocity": 96},
            {"pitch": 64, "duration": 1, "velocity": 80},
            {"pitch": 67, "duration": 1, "velocity": 88},
            {"pitch": 62, "at": 1.975, "duration": 0.5, "velocity": 72},
            {"pitch": 62, "at": 2.025, "duration": 0.5, "velocity": 1},
            {"pitch": 65, "at": "7/3", "duration": "1/3", "velocity": 127},
            {"pitch": 48, "at": 3.5, "duration": 2},
            {"pitch": 36, "at": 5, "duration": 1},
        ]}),
    )
    .unwrap();
}

/// A clip's notes as (pitch, at, duration, velocity), without their IDs.
fn notes(s: &Session, clip: &str) -> Vec<Json> {
    let clip = get(s, clip);
    let mut out: Vec<Json> = clip["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| json!([n["pitch"], n.get("at").cloned().unwrap_or(json!(0)), n["duration"], n["velocity"]]))
        .collect();
    out.sort_by_key(|n| n.to_string());
    out
}

/// An exact beat written as text: `10`, `1/2`.
fn b(text: &str) -> num_rational::BigRational {
    aaw_model::signed_beat(&aaw_model::Beat::Str(text.into())).unwrap()
}

fn path(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

#[test]
fn a_clip_exported_and_imported_again_has_the_same_notes() {
    let (dir, mut s) = open();
    phrase(&mut s);
    let file = path(dir.path(), "My Chords.mid");
    let r = s.export_midi("tracks.keys.clips.clip1", &file).unwrap();
    assert_eq!(r["notes"], json!(7));
    assert_eq!(r["left_out"], json!({"notes outside the clip": 1}));
    assert_eq!(r["adjusted"], json!({"notes shortened to the clip's end": 1}));
    assert_eq!(r["rounded"], json!([]));
    // A note inside a longer one of its pitch is told of.
    edit(&mut s, json!({"op": "note.add", "clip": "tracks.keys.clips.clip1", "pitch": 60, "at": 0.5, "duration": 0.25})).unwrap();
    let inside = path(dir.path(), "inside.mid");
    let r = s.export_midi("tracks.keys.clips.clip1", &inside).unwrap();
    assert_eq!(r["adjusted"]["notes inside a longer one of their pitch, read back with its length"], json!(1));
    edit(&mut s, json!({"op": "note.remove", "notes": ["tracks.keys.clips.clip1.notes.n9"]})).unwrap();

    let r = edit(&mut s, json!({"op": "midi.import", "file": file, "at": 8})).unwrap();
    assert_eq!(r["label"], json!("Import My Chords.mid to new track my-chords"));
    assert_eq!(r["paths"], json!(["tracks.my-chords", "tracks.my-chords.clips.clip2"]));
    assert_eq!((&r["notes"], &r["file_tempo"], &r["left_out"]), (&json!(7), &json!(120.0), &json!({})));
    let clip = get(&s, "tracks.my-chords.clips.clip2");
    assert_eq!((&clip["at"], &clip["length_beats"]), (&json!(8), &json!(4)));
    assert!(s.project().tracks[3].midi.as_ref().unwrap().instrument.is_none());
    // What played comes back: the note after the clip is gone, and the one
    // that lasted past its end ends there.
    let mut expected = notes(&s, "tracks.keys.clips.clip1");
    expected.retain(|n| n[0] != json!(36));
    for n in expected.iter_mut().filter(|n| n[0] == json!(48)) {
        n[2] = json!("1/2");
    }
    let mut got = notes(&s, "tracks.my-chords.clips.clip2");
    for n in got.iter_mut().filter(|n| n[0] == json!(48)) {
        n[2] = json!("1/2");
    }
    assert_eq!(got, expected);
    assert_eq!(get(&s, "tracks.my-chords.clips.clip2.notes.n7.duration"), json!(0.5));

    // A second import of the same file is named apart, and goes after the first.
    let r = edit(&mut s, json!({"op": "midi.import", "file": file})).unwrap();
    assert_eq!(r["paths"][0], json!("tracks.my-chords-2"));
}

#[test]
fn an_import_is_one_undo_step_and_the_song_grows_to_hold_it() {
    let (dir, mut s) = open();
    let file = path(dir.path(), "long.mid");
    // Ten beats of one note, past a 32-beat song from beat 28.
    let notes = [FileNote { pitch: 50, at: b("0"), duration: b("10"), velocity: 90 }];
    std::fs::write(&file, midi_file::write(&notes, "x", 90.0, aaw_model::Meter::COMMON).bytes).unwrap();
    let before = get(&s, "");
    edit(&mut s, json!({"op": "midi.import", "file": file, "at": 28, "index": 0})).unwrap();
    assert_eq!(get(&s, "session.length_beats"), json!(40));
    assert_eq!(get(&s, "tracks.long.clips.clip1.length_beats"), json!(12));
    assert_eq!(s.project().tracks[0].id, "long");
    s.undo(Origin::User, false).unwrap();
    assert_eq!(get(&s, ""), before);
}

#[test]
fn an_import_goes_on_a_midi_track_it_is_given_and_on_no_other() {
    let (dir, mut s) = open();
    phrase(&mut s);
    let file = path(dir.path(), "part.mid");
    let notes = [FileNote { pitch: 38, at: b("1/2"), duration: b("1"), velocity: 100 }];
    std::fs::write(&file, midi_file::write(&notes, "x", 120.0, aaw_model::Meter::COMMON).bytes).unwrap();
    let r = edit(&mut s, json!({"op": "midi.import", "file": file, "track": "keys", "at": "16/3"})).unwrap();
    assert_eq!(r["label"], json!("Import part.mid to keys"));
    assert_eq!(r["path"], json!("tracks.keys.clips.clip2"));
    assert_eq!(get(&s, "tracks.keys.clips.clip2.at"), json!("16/3"));
    assert_eq!(get(&s, "tracks.keys.clips.clip2.notes"), json!([{"id": "n1", "pitch": 38, "at": 0.5, "duration": 1}]));
    let err = edit(&mut s, json!({"op": "midi.import", "file": file, "track": "bass"})).unwrap_err();
    assert_eq!(err, "A MIDI file's notes go on a MIDI track, and bass is not one");
}

#[test]
fn a_file_that_cannot_be_read_changes_nothing() {
    let (dir, mut s) = open();
    let before = get(&s, "");
    let wav = path(dir.path(), "tone.wav");
    let err = edit(&mut s, json!({"op": "midi.import", "file": wav})).unwrap_err();
    assert_eq!(err, "tone.wav: This is not a MIDI file");
    let missing = path(dir.path(), "gone.mid");
    assert!(edit(&mut s, json!({"op": "midi.import", "file": missing})).unwrap_err().starts_with("gone.mid: "));
    assert_eq!(get(&s, ""), before);
}

#[test]
fn only_a_note_clip_with_notes_that_play_is_exported() {
    let (dir, mut s) = open();
    phrase(&mut s);
    let file = path(dir.path(), "x.mid");
    assert_eq!(s.export_midi("tracks.drums.clips.0", &file).unwrap_err(), "tracks.drums.clips.0 is not a note clip");
    assert_eq!(s.export_midi("tracks.keys", &file).unwrap_err(), "tracks.keys is not a note clip");
    edit(&mut s, json!({"op": "clip.add", "track": "keys", "at": 12, "length_beats": 4})).unwrap();
    let err = s.export_midi("tracks.keys.clips.clip2", &file).unwrap_err();
    assert_eq!(err, "Clip clip2 has no notes that play, so there is nothing to write");
    assert!(!Path::new(&file).exists());
}

#[test]
fn an_import_fills_bars_of_the_songs_meter_and_an_export_writes_it() {
    let (dir, mut s) = open();
    edit(&mut s, json!({"op": "set", "path": "session.time_signature", "value": "6/8"})).unwrap();
    let file = path(dir.path(), "waltz.mid");
    // Four beats of notes: two bars of 6/8, three beats each, not one of 4/4.
    let notes = [FileNote { pitch: 60, at: b("0"), duration: b("4"), velocity: 90 }];
    std::fs::write(&file, midi_file::write(&notes, "x", 120.0, aaw_model::Meter::COMMON).bytes).unwrap();
    edit(&mut s, json!({"op": "midi.import", "file": file, "at": 30, "index": 0})).unwrap();
    assert_eq!(get(&s, "tracks.waltz.clips.clip1.length_beats"), json!(6));
    assert_eq!(get(&s, "session.length_beats"), json!(36), "grown to the end of a 6/8 bar");
    // The file written carries the song's time signature: 6 over 2^3.
    let out = dir.path().join("out.mid");
    let r = s.export_midi("tracks.waltz.clips.clip1", &out.to_string_lossy()).unwrap();
    assert_eq!(r["notes"], json!(1));
    let bytes = std::fs::read(&out).unwrap();
    let at = bytes.windows(3).position(|w| w == [0xff, 0x58, 0x04]).expect("a time signature");
    assert_eq!(&bytes[at + 3..at + 7], &[6, 3, 24, 8]);
}
