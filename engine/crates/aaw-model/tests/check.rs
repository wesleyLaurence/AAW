//! `daw check`'s musical checks: for each code, a generated song that raises
//! it and the same song without the mistake, which does not.

use aaw_model::check::{check, Level, Warning};
use aaw_model::{parse, Project};
use serde_json::json;
use std::collections::HashMap;

/// Drums, a bass line and chords that raise nothing.
const SONG: &str = r#"
session: {tempo: 120, length_beats: 32, sample_rate: 48000}
samples:
  kick: {path: kick.wav}
  piano: {path: piano.wav, root_note: C4}
  song: {path: song.wav}
tracks:
  - id: drums
    type: midi
    instrument: {sampler: {drum_rack: true, pads: {kick: {sample: kick}}, map: [{notes: 36, pad: kick}]}}
    clips: [{id: beat, length_beats: 32, loop_beats: 4, notes: [{pitch: 36, duration: 1}, {pitch: 36, at: 2, duration: 1}]}]
  - id: bass
    type: midi
    instrument: {synth: {oscillators: {a: {}}}}
    clips:
      - id: line
        length_beats: 16
        notes: [{pitch: C2, duration: 3.5}, {pitch: A#1, at: 4, duration: 3.5}, {pitch: C2, at: 8, duration: 3.5}, {pitch: A#1, at: 12, duration: 3.5}]
  - id: keys
    type: midi
    instrument: {synth: {oscillators: {a: {}}}}
    clips:
      - id: chords
        length_beats: 16
        notes: [{pitch: C4, duration: 8}, {pitch: E4, duration: 8}, {pitch: G4, duration: 8}]
"#;

fn song(text: &str) -> Project {
    parse(text).unwrap_or_else(|e| panic!("{e}\n{text}"))
}

/// SONG with `from` replaced by `with`.
fn edit(from: &str, with: &str) -> String {
    assert!(SONG.contains(from), "{from}");
    SONG.replace(from, with)
}

/// SONG with a track added at the end.
fn with_track(track: &str) -> String {
    format!("{SONG}{track}")
}

/// The drums' clip as SONG writes it.
const BEAT: &str = "clips: [{id: beat, length_beats: 32, loop_beats: 4, notes: [{pitch: 36, duration: 1}, {pitch: 36, at: 2, duration: 1}]}]";

/// A clip of the drums' beat looped a bar, or of a fill on beat 3.
fn beat(id: &str, at: u32, beats: u32) -> String {
    format!("{{id: {id}, at: {at}, length_beats: {beats}, loop_beats: 4, notes: [{{pitch: 36, duration: 1}}, {{pitch: 36, at: 2, duration: 1}}]}}")
}

fn fill(id: &str, at: u32) -> String {
    format!("{{id: {id}, at: {at}, length_beats: 4, notes: [{{pitch: 36, at: 3, duration: 1}}]}}")
}

fn found(text: &str, code: &str) -> Vec<Warning> {
    check(&song(text), &HashMap::new()).into_iter().filter(|w| w.code == code).collect()
}

#[test]
fn the_song_raises_nothing() {
    assert_eq!(check(&song(SONG), &HashMap::new()), []);
}

#[test]
fn clips_stacked_by_duplicating_onto_one_beat() {
    let stacked = edit(BEAT, &format!("clips: [{}, {}, {}, {}]", beat("a", 0, 16), beat("b", 16, 16), beat("c", 16, 16), beat("d", 16, 4)));
    let w = found(&stacked, "clips-stacked");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].paths, ["tracks.drums.clips.b", "tracks.drums.clips.c", "tracks.drums.clips.d"]);
    assert_eq!(w[0].to_json()["at"], json!(16));
    assert_eq!(w[0].message, "drums: tracks.drums.clips.b, tracks.drums.clips.c and tracks.drums.clips.d start on beat 16 with the same music, so it plays 3 times at once");
    // Stacked clips are not also reported as overlapping.
    assert_eq!(found(&stacked, "clips-overlap"), []);
    // Two note clips with the same notes on one beat, and their notes are not
    // reported again as struck twice.
    let notes = edit("      - id: line\n", "      - {id: copy, at: 0, length_beats: 16, notes: [{pitch: C2, duration: 3.5}, {pitch: A#1, at: 4, duration: 3.5}, {pitch: C2, at: 8, duration: 3.5}, {pitch: A#1, at: 12, duration: 3.5}]}\n      - id: line\n");
    assert_eq!(found(&notes, "clips-stacked")[0].paths, ["tracks.bass.clips.copy", "tracks.bass.clips.line"]);
    assert_eq!(found(&notes, "note-retriggered"), []);
    // The same beat with other music is an overlap, not a stack.
    let other = edit(BEAT, &format!("clips: [{}, {}]", beat("a", 0, 32), fill("f", 0)));
    assert_eq!(found(&other, "clips-stacked"), []);
}

#[test]
fn clips_overlap_is_information() {
    let both = edit(BEAT, &format!("clips: [{}, {}]", beat("a", 0, 32), fill("f", 28)));
    let w = found(&both, "clips-overlap");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!((w[0].level, w[0].to_json()["level"].clone()), (Level::Info, json!("info")));
    assert_eq!(w[0].paths, ["tracks.drums.clips.a", "tracks.drums.clips.f"]);
    assert_eq!(w[0].message, "drums: tracks.drums.clips.a and tracks.drums.clips.f sound at once from beat 28 to 32");
    // One after the other, they only meet.
    let after = edit(BEAT, &format!("clips: [{}, {}]", beat("a", 0, 28), fill("f", 28)));
    assert_eq!(found(&after, "clips-overlap"), []);
}

#[test]
fn a_note_struck_again_while_it_sounds() {
    let held = edit("{pitch: C2, duration: 3.5}, {pitch: A#1, at: 4", "{pitch: C2, duration: 6}, {pitch: C2, at: 5, duration: 1/2}, {pitch: A#1, at: 4");
    let w = found(&held, "note-retriggered");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].message, "bass.line: 1 note starts while a note of the same pitch is still sounding, the first C2 at beat 5");
    assert_eq!((w[0].paths.clone(), w[0].to_json()["at"].clone()), (vec!["tracks.bass.clips.line".to_string()], json!(5)));
    // A note of another pitch, or the same pitch after the first ends, is not.
    let fine = edit("{pitch: C2, duration: 3.5}, {pitch: A#1, at: 4", "{pitch: C2, duration: 3.5}, {pitch: D2, at: 1, duration: 1}, {pitch: C2, at: 3.5, duration: 1/2}, {pitch: A#1, at: 4");
    assert_eq!(found(&fine, "note-retriggered"), []);
}

#[test]
fn nothing_is_said_of_an_audio_clip_past_the_song_s_length() {
    // The length is the song's content, which a host settles after every
    // edit (D130); a file saved longer or shorter than its content is not
    // warned about, and no code is about the end.
    let audio = |clip: &str| with_track(&format!("  - id: edit\n    audio: [{clip}]\n"));
    let long = song(&audio("{sample: song, at: 24, source_end_seconds: 6}"));
    let seconds = HashMap::from([("song".to_string(), 6.0)]);
    assert!(check(&long, &seconds).iter().all(|w| !w.code.contains("ends") && !w.message.contains("cut off")), "{:?}", check(&long, &seconds));
    assert!(aaw_model::check::CODES.iter().all(|(code, _)| *code != "song-ends-inside"));
}

#[test]
fn tracks_with_nothing_to_play() {
    let cases = [
        ("  - {id: pad, type: midi, instrument: {synth: {oscillators: {a: {}}}}}\n", "pad: a synth and no notes for it to play"),
        ("  - {id: pad, type: midi}\n", "pad: no instrument and no notes"),
        ("  - {id: pad, type: midi, clips: [{length_beats: 4, notes: [{pitch: 60, duration: 1}]}]}\n", "pad: 1 note and no instrument, so nothing is heard"),
        ("  - {id: pad, type: midi, instrument: {sampler: {pads: {kick: {sample: kick}}, map: [{notes: 36, pad: kick}]}}}\n", "pad: a sampler and no notes for it to play"),
        ("  - {id: pad}\n", "pad: no audio clips"),
    ];
    for (track, message) in cases {
        let w = found(&with_track(track), "track-empty");
        assert_eq!(w.iter().map(|w| w.message.as_str()).collect::<Vec<_>>(), [message]);
        assert_eq!(w[0].paths, ["tracks.pad"]);
    }
    assert_eq!(found(&with_track("  - {id: pad, audio: [{sample: song}]}\n"), "track-empty"), []);
}

#[test]
fn two_low_parts_in_one_octave() {
    let low = edit("notes: [{pitch: C4, duration: 8}", "notes: [{pitch: C2, duration: 12}, {pitch: C4, duration: 8}");
    let w = found(&low, "register-crowded");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].message, "bass and keys both play notes from C2 to B2 in bars 1, 3; two low parts in one octave muddy the mix");
    assert_eq!((w[0].paths.clone(), w[0].to_json()["at"].clone()), (vec!["tracks.bass".to_string(), "tracks.keys".to_string()], json!(0)));
    // A muted track, or a drum map's kick on C2, has no register.
    assert_eq!(found(&low.replace("  - id: keys\n", "  - id: keys\n    mute: true\n"), "register-crowded"), []);
    let kick = with_track("  - {id: kit, type: midi, instrument: {sampler: {pads: {kick: {sample: kick}}, map: [{notes: 36, pad: kick}]}}, clips: [{length_beats: 16, notes: [{pitch: 36, duration: 1}, {pitch: 36, at: 4, duration: 1}]}]}\n");
    assert_eq!(found(&kick, "register-crowded"), []);
    // Above C3 they may share an octave.
    assert_eq!(found(SONG, "register-crowded"), []);
}

#[test]
fn a_sample_far_from_its_root() {
    let sampler = |note: &str| with_track(&format!("  - {{id: piano, type: midi, instrument: {{sampler: {{pads: {{piano: {{sample: piano}}}}, map: [{{notes: [0, 127], pad: piano, pitched: true}}]}}}}, clips: [{{length_beats: 4, notes: [{{pitch: {note}, at: 1, duration: 1}}]}}]}}\n"));
    let w = found(&sampler("C1"), "pad-far-from-root");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].message, "piano.piano: 1 hit plays the sample as far as 36 semitones below its root, the first at beat 1; past two octaves a repitched sample sounds artificial");
    assert_eq!(w[0].paths, ["tracks.piano.instrument.sampler.pads.piano"]);
    assert_eq!(found(&sampler("C2"), "pad-far-from-root"), [], "two octaves is not past them");
    // A pitched note, and the pad's own transpose.
    let high = with_track("  - {id: hits, type: midi, instrument: {sampler: {pads: {p: {sample: piano, transpose: 12}}, map: [{notes: [72, 96], pad: p, pitched: true}]}}, clips: [{id: high, at: 8, length_beats: 4, notes: [{pitch: C6, at: 2, duration: 1}]}]}\n");
    let w = found(&high, "pad-far-from-root");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!((w[0].paths.clone(), w[0].to_json()["at"].clone()), (vec!["tracks.hits.instrument.sampler.pads.p".to_string()], json!(10)));
    assert!(w[0].message.contains("36 semitones above"), "{}", w[0].message);
}

#[test]
fn a_synth_note_below_hearing() {
    let low = edit("{pitch: C2, duration: 3.5}, {pitch: A#1", "{pitch: C0, duration: 3.5}, {pitch: A#1");
    let w = found(&low, "note-below-hearing");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].message, "bass: 1 note has a fundamental under 20 Hz, the lowest 16.35 Hz, the first at beat 0; it is felt rather than heard");
    // A sub oscillator an octave down takes C1 under 20 Hz too; noise has no pitch.
    let sub = edit("{pitch: C2, duration: 3.5}, {pitch: A#1", "{pitch: C1, duration: 3.5}, {pitch: A#1");
    assert_eq!(found(&sub, "note-below-hearing"), []);
    let sub = sub.replacen("oscillators: {a: {}}", "oscillators: {a: {}, sub: {wave: sine, octave: -1}, n: {wave: noise, octave: -4}}", 1);
    assert_eq!(found(&sub, "note-below-hearing").len(), 1);
}

#[test]
fn describe_check_says_what_each_code_means() {
    let described = aaw_model::contract::describe("check").unwrap();
    let codes = described["codes"].as_object().unwrap();
    assert_eq!(codes.keys().map(String::as_str).collect::<Vec<_>>(), aaw_model::check::CODES.iter().map(|(c, _)| *c).collect::<Vec<_>>());
    assert!(described["semantics"]["warnings"].as_str().unwrap().contains("paths"));
}
