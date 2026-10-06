//! Clips that loop: a note clip or an audio clip whose first beats play again
//! and again until its end, through the session's commands, the range verbs,
//! the map, the checks, `note list` and the file.

mod common;

use aaw_host::session::Session;
use common::{edit, get, write_song};
use serde_json::{json, Value as Json};
use std::collections::HashMap;

fn open() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, true).unwrap();
    (dir, s)
}

const PHRASE: &str = "tracks.keys.clips.phrase";
const AUDIO: &str = "tracks.song.audio.0";

/// A MIDI track with a note clip of two bars from beat 4: notes at 0, 3 and 6.
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

/// A track of one audio clip: seconds 1 to 9 of a ten-second file at 120 BPM
/// from beat 4, so it ends on beat 20.
fn with_audio(s: &mut Session) {
    let song: Vec<[f32; 2]> = (0..480000).map(|i| [((i % 4800) as f32 / 4800.0 - 0.5) * 0.2; 2]).collect();
    let dir = s.path().parent().unwrap().to_path_buf();
    std::fs::write(dir.join("song.wav"), aaw_engine::wav::float_wav_bytes(&song, 48000)).unwrap();
    edit(s, json!({"op": "set", "path": "samples.song", "value": {"path": "song.wav"}})).unwrap();
    edit(s, json!({"op": "track.add", "id": "song"})).unwrap();
    edit(s, json!({"op": "audio.add", "track": "song", "sample": "song", "at": 4, "source_start_seconds": 1, "source_end_seconds": 9})).unwrap();
}

fn warnings(s: &Session, code: &str) -> Vec<String> {
    aaw_model::check::check(s.project(), &HashMap::new()).into_iter().filter(|w| w.code == code).map(|w| w.message).collect()
}

fn row(s: &Session, name: &str) -> String {
    let m = s.map(None, None, None, &[], false).unwrap();
    let text: String = m["map"].as_array().unwrap().iter().map(|l| format!("{}\n", l.as_str().unwrap())).collect();
    let line = text.lines().map(str::trim_start).find(|l| l.starts_with(&format!("{name} "))).unwrap_or_else(|| panic!("{text}"));
    line[name.len()..].trim_start().to_string()
}

fn legend(s: &Session) -> String {
    let m = s.map(None, None, None, &[], false).unwrap();
    m["map"].as_array().unwrap().iter().map(|l| format!("{}\n", l.as_str().unwrap())).collect()
}

#[test]
fn a_note_clip_loops_its_first_beats_until_its_end() {
    let (_d, mut s) = open();
    with_keys(&mut s);
    let r = edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 4})).unwrap();
    assert_eq!(r["label"], json!("Loop clip phrase at 4 every 4 beats"));
    edit(&mut s, json!({"op": "clip.resize", "clip": PHRASE, "length_beats": 20})).unwrap();
    let clip = get(&s, PHRASE);
    assert_eq!((&clip["loop_beats"], &clip["length_beats"]), (&json!(4), &json!(20)));
    // The map shows one clip of five bars, looping; the note past the loop's
    // end is kept, and note list and check say it does not play.
    assert_eq!(row(&s, "keys"), ".CCCCC..");
    assert!(legend(&s).contains("C  notes phrase, 20 beats, loops every 4 beats, 2 notes C4–E4; keys bar 2 ×5"), "{}", legend(&s));
    let listed = s.notes(PHRASE, None, None).unwrap();
    let outside: Vec<&str> = listed["clips"][0]["notes"].as_array().unwrap().iter().filter(|n| n["outside"] == json!(true)).map(|n| n["id"].as_str().unwrap()).collect();
    assert_eq!(outside, ["n3"]);
    assert_eq!(warnings(&s, "notes-outside-clip"), ["keys.phrase: notes n3 start at or after the loop's end and do not play"]);
    // The loop and the clip are played as the schedule unrolls them: a note
    // a repetition, and the held note cut at each wrap.
    let notes = aaw_model::schedule::track_notes(s.project(), s.project().tracks[2].midi.as_ref().unwrap());
    let starts: Vec<String> = notes.iter().map(|n| format!("{}:{}", n.at, n.beats)).collect();
    assert_eq!(starts, ["4:2", "7:1", "8:2", "11:1", "12:2", "15:1", "16:2", "19:1", "20:2", "23:1"]);
    // It is saved and read back, and turned off.
    s.save().unwrap();
    let text = std::fs::read_to_string(s.path()).unwrap();
    assert!(text.contains("loop_beats: 4"), "{text}");
    let again = Session::open(s.path(), false).unwrap();
    assert_eq!(get(&again, PHRASE), clip);
    let r = edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": "off"})).unwrap();
    assert_eq!(r["label"], json!("Loop clip phrase at 4 off"));
    let clip = get(&s, PHRASE);
    assert!(clip.get("loop_beats").is_none() && clip["length_beats"] == json!(20));
    assert_eq!(row(&s, "keys"), ".CC:::..");
    // Refusals.
    let err = edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 0})).unwrap_err();
    assert_eq!(err, "A loop is longer than 0 beats, or off");
    let err = edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 4, "length": 16})).unwrap_err();
    assert_eq!(err, "Clip phrase at 4 is a note clip, whose length clip resize sets");
    let err = edit(&mut s, json!({"op": "clip.loop", "clip": "tracks.drums.clips.0", "loop_beats": 4})).unwrap_err();
    assert_eq!(err, "tracks.drums.clips.0 is a pattern clip, not a note clip");
}

#[test]
fn a_looped_note_clip_is_cut_only_at_a_wrap_and_rejoined() {
    let (_d, mut s) = open();
    with_keys(&mut s);
    edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 4})).unwrap();
    edit(&mut s, json!({"op": "clip.resize", "clip": PHRASE, "length_beats": 16})).unwrap();
    let before = get(&s, "tracks.keys.clips");
    // Beat 12 is a wrap: each half keeps the loop and every note.
    edit(&mut s, json!({"op": "range.insert", "at": 12, "length": 4})).unwrap();
    let clips = get(&s, "tracks.keys.clips");
    let notes = &before[0]["notes"];
    assert_eq!(clips.as_array().unwrap().len(), 2);
    assert_eq!((&clips[0]["length_beats"], &clips[0]["loop_beats"], &clips[0]["notes"]), (&json!(8), &json!(4), notes));
    assert_eq!((&clips[1]["at"], &clips[1]["length_beats"], &clips[1]["loop_beats"], &clips[1]["notes"]), (&json!(16), &json!(8), &json!(4), notes));
    // Deleting the beats again makes the halves one loop again.
    edit(&mut s, json!({"op": "range.delete", "start": 12, "length": 4})).unwrap();
    assert_eq!(get(&s, "tracks.keys.clips"), before);
    // Inside a repetition the command is refused, naming the clip.
    let err = edit(&mut s, json!({"op": "range.insert", "at": 10, "length": 4, "tracks": ["keys"]})).unwrap_err();
    assert_eq!(
        err,
        "Beat 10 falls inside a repetition of clip tracks.keys.clips.phrase, which loops every 4 beats; a looped clip is cut only at a wrap, or turn its loop off first"
    );
    assert_eq!(get(&s, "tracks.keys.clips"), before);
    // A copy of two loops is a looped clip of eight beats.
    edit(&mut s, json!({"op": "range.copy", "start": 8, "length": 8, "to": 24, "tracks": ["keys"]})).unwrap();
    let copy = &get(&s, "tracks.keys.clips")[1];
    assert_eq!((&copy["at"], &copy["length_beats"], &copy["loop_beats"], &copy["notes"]), (&json!(24), &json!(8), &json!(4), notes));
}

#[test]
fn an_audio_clip_loops_as_copies_of_its_first_beats() {
    let (_d, mut s) = open();
    with_audio(&mut s);
    // The clip keeps its length, sixteen beats, and loops its first eight.
    let r = edit(&mut s, json!({"op": "clip.loop", "clip": AUDIO, "loop_beats": 8})).unwrap();
    assert_eq!(r["label"], json!("Loop clip song at 4 every 8 beats"));
    let clip = get(&s, AUDIO);
    assert_eq!((&clip["loop_beats"], &clip["length_beats"], &clip["source_end_seconds"]), (&json!(8), &json!(16), &json!(9.0)));
    assert_eq!(row(&s, "song"), ".CCCC...");
    assert!(legend(&s).contains("C  audio song 1–5 s, 16 beats, loops every 8 beats; song bar 2 ×2"), "{}", legend(&s));
    // Resize and trim --end say how long it plays; its start cannot be trimmed.
    edit(&mut s, json!({"op": "clip.resize", "clip": AUDIO, "length_beats": 40})).unwrap();
    assert_eq!(get(&s, &format!("{AUDIO}.length_beats")), json!(40));
    edit(&mut s, json!({"op": "audio.trim", "clip": AUDIO, "end": 36})).unwrap();
    assert_eq!(get(&s, &format!("{AUDIO}.length_beats")), json!(32));
    let err = edit(&mut s, json!({"op": "audio.trim", "clip": AUDIO, "start": 8})).unwrap_err();
    assert!(err.starts_with("Audio clip song at 4 loops from its start, which cannot be trimmed"), "{err}");
    // The engine plays it as four copies that meet.
    let voices = aaw_engine::program::compile(s.project(), s.path().parent().unwrap()).unwrap().tracks[2].voices.to_vec();
    assert_eq!(voices.iter().map(|v| v.start).collect::<Vec<_>>(), [96000, 288000, 480000, 672000]);
    // Split at a wrap, and only there; a cut across it is refused.
    let err = edit(&mut s, json!({"op": "audio.split", "clip": AUDIO, "at": 22})).unwrap_err();
    assert!(err.starts_with("Beat 22 falls inside a repetition of audio clip song at 4, which loops every 8 beats"), "{err}");
    let err = edit(&mut s, json!({"op": "audio.cut", "track": "song", "from": 6, "to": 10})).unwrap_err();
    assert!(err.starts_with("Audio clip song at 4 loops across the cut"), "{err}");
    edit(&mut s, json!({"op": "audio.split", "clip": AUDIO, "at": 20})).unwrap();
    let halves = get(&s, "tracks.song.audio");
    assert_eq!(halves[0]["length_beats"], json!(16));
    assert_eq!((&halves[1]["at"], &halves[1]["length_beats"], &halves[1]["loop_beats"], &halves[1]["source_start_seconds"]), (&json!(20), &json!(16), &json!(8), &json!(1.0)));
    assert_eq!(halves[0]["fade_out_ms"], halves[1]["fade_out_ms"]);
    // A delete at a wrap rejoins the halves into one loop.
    edit(&mut s, json!({"op": "range.delete", "start": 20, "length": 8})).unwrap();
    let joined = get(&s, "tracks.song.audio");
    assert_eq!(joined.as_array().unwrap().len(), 1);
    assert_eq!((&joined[0]["length_beats"], &joined[0]["loop_beats"]), (&json!(24), &json!(8)));
    // Off: the clip plays its audio once again, as before.
    edit(&mut s, json!({"op": "clip.loop", "clip": AUDIO, "loop_beats": null})).unwrap();
    let clip = get(&s, AUDIO);
    assert!(clip.get("loop_beats").is_none() && clip.get("length_beats").is_none());
    let err = edit(&mut s, json!({"op": "clip.resize", "clip": AUDIO, "length_beats": 8})).unwrap_err();
    assert!(err.starts_with("Audio clip song at 4 does not loop"), "{err}");
    // A clip that plays to its file's end needs to be told how long to play.
    edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "song", "at": 8, "source_start_seconds": 1})).unwrap();
    let err = edit(&mut s, json!({"op": "clip.loop", "clip": "tracks.song.audio.1", "loop_beats": 4})).unwrap_err();
    assert_eq!(err, "Audio clip song at 8 plays to its file's end; give length, how many beats it should play");
    edit(&mut s, json!({"op": "clip.loop", "clip": "tracks.song.audio.1", "loop_beats": 4, "length": 12})).unwrap();
    assert_eq!(get(&s, "tracks.song.audio.1.length_beats"), json!(12));
    // A looped clip is written as given and validated: a loop without a length is refused.
    let err = edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "song", "at": 0, "loop_beats": 4})).unwrap_err();
    assert!(err.contains("A looped audio clip needs length_beats"), "{err}");
}

#[test]
fn a_looped_clip_against_a_copy_is_the_same_music_only_with_the_same_loop() {
    let (_d, mut s) = open();
    with_keys(&mut s);
    edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 4})).unwrap();
    edit(&mut s, json!({"op": "clip.duplicate", "clip": PHRASE, "at": 16})).unwrap();
    assert_eq!(row(&s, "keys"), ".CC.CC..");
    edit(&mut s, json!({"op": "clip.loop", "clip": "tracks.keys.clips.clip1", "loop_beats": 2})).unwrap();
    assert_eq!(row(&s, "keys"), ".CC.DD..");
    assert_eq!(warnings(&s, "clips-stacked"), Vec::<String>::new());
    let _: Json = get(&s, "tracks.keys.clips");
}
