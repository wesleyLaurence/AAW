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
    edit(s, json!({"op": "track.add", "type": "audio", "id": "song"})).unwrap();
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
    assert_eq!(row(&s, "keys"), ".DDDDD..");
    assert!(legend(&s).contains("D  notes phrase, loops every 4 beats, 2 notes C4–E4; keys bar 2 ×5"), "{}", legend(&s));
    let listed = s.notes(PHRASE, None, None, false, None).unwrap();
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
    assert_eq!(row(&s, "keys"), ".DD:::..");
    // Refusals.
    let err = edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 0})).unwrap_err();
    assert_eq!(err, "A loop is longer than 0 beats, or off");
    let err = edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 4, "length": 16})).unwrap_err();
    assert_eq!(err, "Clip phrase at 4 is a note clip, whose length clip resize sets");
}

#[test]
fn a_looped_note_clip_is_cut_at_any_beat_and_rejoined() {
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
    // Inside a repetition the right half starts as far into the loop as
    // the cut is, and a delete of the beats makes them one loop again.
    edit(&mut s, json!({"op": "range.insert", "at": 10, "length": 4, "tracks": ["keys"]})).unwrap();
    let clips = get(&s, "tracks.keys.clips");
    assert_eq!((&clips[0]["length_beats"], &clips[0].get("loop_offset_beats")), (&json!(6), &None));
    assert_eq!((&clips[1]["at"], &clips[1]["length_beats"], &clips[1]["loop_offset_beats"], &clips[1]["notes"]), (&json!(14), &json!(10), &json!(2), notes));
    edit(&mut s, json!({"op": "range.delete", "start": 10, "length": 4, "tracks": ["keys"]})).unwrap();
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
    assert_eq!(row(&s, "song"), ".DDDD...");
    assert!(legend(&s).contains("D  audio song 1–5 s, 16 beats, loops every 8 beats; song bar 2 ×2"), "{}", legend(&s));
    // Resize and trim --end say how long it plays.
    edit(&mut s, json!({"op": "clip.resize", "clip": AUDIO, "length_beats": 40})).unwrap();
    assert_eq!(get(&s, &format!("{AUDIO}.length_beats")), json!(40));
    edit(&mut s, json!({"op": "audio.trim", "clip": AUDIO, "end": 36})).unwrap();
    assert_eq!(get(&s, &format!("{AUDIO}.length_beats")), json!(32));
    // The engine plays it as four copies that meet.
    let voices = aaw_engine::program::compile(s.project(), s.path().parent().unwrap()).unwrap().tracks[2].voices.to_vec();
    assert_eq!(voices.iter().map(|v| v.start).collect::<Vec<_>>(), [96000, 288000, 480000, 672000]);
    // Split at a wrap; a cut across it is refused.
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
    assert_eq!(row(&s, "keys"), ".DD.DD..");
    edit(&mut s, json!({"op": "clip.loop", "clip": "tracks.keys.clips.clip1", "loop_beats": 2})).unwrap();
    assert_eq!(row(&s, "keys"), ".DD.EE..");
    assert_eq!(warnings(&s, "clips-stacked"), Vec::<String>::new());
    let _: Json = get(&s, "tracks.keys.clips");
}

#[test]
fn clip_split_cuts_any_clip_at_a_beat_inside_it() {
    let (_d, mut s) = open();
    with_keys(&mut s);
    with_audio(&mut s);
    edit(&mut s, json!({"op": "clip.active", "clip": PHRASE, "active": false})).unwrap();
    // A note clip at beat 8: the note across it held to it on the left, the
    // rest from the right clip's start, which gets the next free ID. Both
    // halves stay deactivated.
    let made = edit(&mut s, json!({"op": "clip.split", "clip": PHRASE, "at": 8})).unwrap();
    assert_eq!(made["label"], json!("Split clip phrase at 4 at beat 8"));
    let clips = get(&s, "tracks.keys.clips");
    assert_eq!((&clips[0]["at"], &clips[0]["length_beats"], &clips[0]["active"]), (&json!(4), &json!(4), &json!(false)));
    assert_eq!((&clips[1]["id"], &clips[1]["at"], &clips[1]["length_beats"], &clips[1]["active"]), (&json!("clip1"), &json!(8), &json!(4), &json!(false)));
    let notes = |i: usize| clips[i]["notes"].as_array().unwrap().iter().map(|n| (n["pitch"].clone(), n.get("at").cloned().unwrap_or(json!(0)), n["duration"].clone())).collect::<Vec<_>>();
    assert_eq!(notes(0), [(json!(60), json!(0), json!(2)), (json!(64), json!(3), json!(1))]);
    assert_eq!(notes(1), [(json!(64), json!(0), json!(1)), (json!(67), json!(2), json!(1))]);
    // At a clip's edge or outside it there is nothing to split.
    let err = edit(&mut s, json!({"op": "clip.split", "clip": PHRASE, "at": 8})).unwrap_err();
    assert_eq!(err, "Beat 8 is not inside clip phrase at 4");
    // A looped clip is cut inside a repetition, the right half starting
    // into the loop, or at a wrap, starting at its start.
    edit(&mut s, json!({"op": "clip.loop", "clip": "tracks.keys.clips.clip1", "loop_beats": 2})).unwrap();
    edit(&mut s, json!({"op": "clip.split", "clip": "tracks.keys.clips.clip1", "at": 9})).unwrap();
    let clips = get(&s, "tracks.keys.clips");
    assert_eq!((&clips[2]["at"], &clips[2]["length_beats"], &clips[2]["loop_offset_beats"]), (&json!(9), &json!(3), &json!(1)));
    edit(&mut s, json!({"op": "clip.split", "clip": "tracks.keys.clips.clip2", "at": 10})).unwrap();
    let clips = get(&s, "tracks.keys.clips");
    assert_eq!(clips.as_array().unwrap().len(), 4);
    assert_eq!((&clips[3]["at"], &clips[3].get("loop_offset_beats")), (&json!(10), &None));
    // An audio clip is split as audio.split splits it.
    edit(&mut s, json!({"op": "clip.split", "clip": AUDIO, "at": 12})).unwrap();
    let halves = get(&s, "tracks.song.audio");
    assert_eq!((&halves[0]["source_end_seconds"], &halves[1]["at"], &halves[1]["source_start_seconds"]), (&json!(5.0), &json!(12), &json!(5.0)));
}

/// The notes of the keys track as they play: start and length in beats.
fn played(s: &Session) -> Vec<String> {
    let midi = s.project().tracks.iter().find(|t| t.id == "keys").unwrap().midi.as_ref().unwrap();
    aaw_model::schedule::track_notes(s.project(), midi).iter().map(|n| format!("{}:{}", n.at, n.beats)).collect()
}

#[test]
fn a_looped_note_clip_split_inside_a_repetition_plays_as_it_did() {
    let (_d, mut s) = open();
    with_keys(&mut s);
    edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 4})).unwrap();
    edit(&mut s, json!({"op": "clip.resize", "clip": PHRASE, "length_beats": 16})).unwrap();
    let before = played(&s);
    assert_eq!(before, ["4:2", "7:1", "8:2", "11:1", "12:2", "15:1", "16:2", "19:1"]);
    // Between notes: the halves play what the clip played.
    edit(&mut s, json!({"op": "clip.split", "clip": PHRASE, "at": 14})).unwrap();
    assert_eq!(get(&s, "tracks.keys.clips.clip1.loop_offset_beats"), json!(2));
    assert_eq!(played(&s), before);
    assert!(legend(&s).contains("loops every 4 beats from 2 beats in"), "{}", legend(&s));
    // Inside a held note: it is held to the cut on the left and its rest
    // plays from the right half's start, as a split of any clip leaves it.
    edit(&mut s, json!({"op": "clip.split", "clip": PHRASE, "at": 9})).unwrap();
    assert_eq!(get(&s, "tracks.keys.clips.clip2.loop_offset_beats"), json!(1));
    assert_eq!(played(&s), ["4:2", "7:1", "8:1", "9:1", "11:1", "12:2", "15:1", "16:2", "19:1"]);
    // Joined, the notes are written out as they played.
    edit(&mut s, json!({"op": "clip.join", "clips": [PHRASE, "tracks.keys.clips.clip2", "tracks.keys.clips.clip1"]})).unwrap();
    let clip = get(&s, PHRASE);
    assert!(clip.get("loop_beats").is_none() && clip.get("loop_offset_beats").is_none(), "{clip}");
    assert_eq!(played(&s), ["4:2", "7:1", "8:1", "9:1", "11:1", "12:2", "15:1", "16:2", "19:1"]);
}

#[test]
fn a_looped_clip_trimmed_at_its_start_keeps_its_loop_where_it_is() {
    let (_d, mut s) = open();
    with_keys(&mut s);
    edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 4})).unwrap();
    edit(&mut s, json!({"op": "clip.resize", "clip": PHRASE, "length_beats": 16})).unwrap();
    let notes = get(&s, &format!("{PHRASE}.notes"));
    edit(&mut s, json!({"op": "clip.trim", "clip": PHRASE, "start": 10})).unwrap();
    let clip = get(&s, PHRASE);
    assert_eq!((&clip["at"], &clip["length_beats"], &clip["loop_offset_beats"], &clip["notes"]), (&json!(10), &json!(10), &json!(2), &notes));
    assert_eq!(played(&s), ["11:1", "12:2", "15:1", "16:2", "19:1"]);
    // Back to before its loop's start: the offset is where it is in the loop.
    edit(&mut s, json!({"op": "clip.trim", "clip": PHRASE, "start": 3})).unwrap();
    assert_eq!(get(&s, &format!("{PHRASE}.loop_offset_beats")), json!(3));
    assert_eq!(played(&s), ["3:1", "4:2", "7:1", "8:2", "11:1", "12:2", "15:1", "16:2", "19:1"]);
    // A shorter loop keeps the offset inside it, and drops one past it.
    edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 2})).unwrap();
    assert!(get(&s, PHRASE).get("loop_offset_beats").is_none());
    edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 4})).unwrap();
    edit(&mut s, json!({"op": "clip.trim", "clip": PHRASE, "start": 5})).unwrap();
    assert_eq!(get(&s, &format!("{PHRASE}.loop_offset_beats")), json!(2));
    // Off: it plays its notes once from where it started in its loop, the
    // notes moved to it, no longer cut at the loop's end.
    assert_eq!(played(&s)[..2], ["6:1", "7:2"]);
    edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": "off"})).unwrap();
    let clip = get(&s, PHRASE);
    assert!(clip.get("loop_offset_beats").is_none(), "{clip}");
    assert_eq!(played(&s), ["6:2", "9:1"]);
    // An offset needs a loop, and is inside it.
    let err = edit(&mut s, json!({"op": "set", "path": format!("{PHRASE}.loop_offset_beats"), "value": 1})).unwrap_err();
    assert!(err.contains("loop_offset_beats is how far into its loop a clip starts; it needs loop_beats"), "{err}");
    edit(&mut s, json!({"op": "clip.loop", "clip": PHRASE, "loop_beats": 4})).unwrap();
    let err = edit(&mut s, json!({"op": "set", "path": format!("{PHRASE}.loop_offset_beats"), "value": 4})).unwrap_err();
    assert!(err.contains("loop_offset_beats must be at least 0 and less than loop_beats"), "{err}");
}

#[test]
fn a_looped_audio_clip_split_inside_a_repetition_plays_as_it_did() {
    let (_d, mut s) = open();
    with_audio(&mut s);
    edit(&mut s, json!({"op": "clip.loop", "clip": AUDIO, "loop_beats": 8, "length": 32})).unwrap();
    edit(&mut s, json!({"op": "set", "path": format!("{AUDIO}.lead_ms"), "value": 5})).unwrap();
    let voices = |s: &Session| {
        let program = aaw_engine::program::compile(s.project(), s.path().parent().unwrap()).unwrap();
        let track = program.tracks.iter().find(|t| t.id == "song").unwrap();
        track.voices.iter().map(|v| v.start).collect::<Vec<_>>()
    };
    let before = voices(&s);
    edit(&mut s, json!({"op": "audio.split", "clip": AUDIO, "at": 22})).unwrap();
    let halves = get(&s, "tracks.song.audio");
    assert_eq!((&halves[0]["length_beats"], &halves[1]["at"], &halves[1]["length_beats"], &halves[1]["loop_offset_beats"]), (&json!(18), &json!(22), &json!(14), &json!(2)));
    // The right half's first repetition starts inside the audio, without a
    // lead: a voice at beat 22, the rest of the loop, and a loop from 28.
    let after = voices(&s);
    let at = |beat: i64| beat * 24000;
    assert!(after.contains(&at(22)) && after.contains(&(at(28) - 240)), "{after:?}");
    assert_eq!(after.len(), before.len() + 1);
    let repetitions = s.project().tracks.iter().find(|t| t.id == "song").unwrap().audio[1].repetitions(&aaw_model::TempoMap::constant(120.0));
    assert_eq!((repetitions[0].source_start_seconds, repetitions[0].lead_ms, repetitions[1].source_start_seconds), (2.0, 0.0, 1.0));
    // Moved apart and back, the halves are one loop again.
    edit(&mut s, json!({"op": "range.insert", "at": 22, "length": 4})).unwrap();
    edit(&mut s, json!({"op": "range.delete", "start": 22, "length": 4})).unwrap();
    let joined = get(&s, "tracks.song.audio");
    assert_eq!(joined.as_array().unwrap().len(), 1, "{joined}");
    assert_eq!((&joined[0]["length_beats"], joined[0].get("loop_offset_beats")), (&json!(32), None));
    assert_eq!(voices(&s), before);
    // Trimmed at its start, a looped audio clip starts into its loop.
    edit(&mut s, json!({"op": "audio.trim", "clip": AUDIO, "start": 25})).unwrap();
    let clip = get(&s, AUDIO);
    assert_eq!((&clip["at"], &clip["length_beats"], &clip["loop_offset_beats"]), (&json!(25), &json!(11), &json!(5)));
}
