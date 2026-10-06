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
patterns:
  beat: {length_beats: 4, events: [{at: 0, pad: kick}, {at: 2, pad: kick}]}
  fill: {length_beats: 4, events: [{at: 3, pad: kick}]}
tracks:
  - id: drums
    pads: {kick: {sample: kick}}
    clips: [{pattern: beat, at: 0, repeats: 8}]
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

fn found(text: &str, code: &str) -> Vec<Warning> {
    check(&song(text), &HashMap::new()).into_iter().filter(|w| w.code == code).collect()
}

#[test]
fn the_song_raises_nothing() {
    assert_eq!(check(&song(SONG), &HashMap::new()), []);
}

#[test]
fn clips_stacked_by_duplicating_onto_one_beat() {
    let stacked = edit("clips: [{pattern: beat, at: 0, repeats: 8}]", "clips: [{pattern: beat, at: 0, repeats: 4}, {pattern: beat, at: 16, repeats: 4}, {pattern: beat, at: 16}, {pattern: beat, at: 16}]");
    let w = found(&stacked, "clips-stacked");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].paths, ["tracks.drums.clips.1", "tracks.drums.clips.2", "tracks.drums.clips.3"]);
    assert_eq!(w[0].to_json()["at"], json!(16));
    assert_eq!(w[0].message, "drums: tracks.drums.clips.1, tracks.drums.clips.2 and tracks.drums.clips.3 start on beat 16 with the same music, so it plays 3 times at once");
    // Stacked clips are not also reported as overlapping.
    assert_eq!(found(&stacked, "clips-overlap"), []);
    // Two note clips with the same notes on one beat, and their notes are not
    // reported again as struck twice.
    let notes = edit("      - id: line\n", "      - {id: copy, at: 0, length_beats: 16, notes: [{pitch: C2, duration: 3.5}, {pitch: A#1, at: 4, duration: 3.5}, {pitch: C2, at: 8, duration: 3.5}, {pitch: A#1, at: 12, duration: 3.5}]}\n      - id: line\n");
    assert_eq!(found(&notes, "clips-stacked")[0].paths, ["tracks.bass.clips.copy", "tracks.bass.clips.line"]);
    assert_eq!(found(&notes, "note-retriggered"), []);
    // The same beat with other music is an overlap, not a stack.
    let other = edit("clips: [{pattern: beat, at: 0, repeats: 8}]", "clips: [{pattern: beat, at: 0, repeats: 8}, {pattern: fill, at: 0}]");
    assert_eq!(found(&other, "clips-stacked"), []);
}

#[test]
fn clips_overlap_is_information() {
    let fill = edit("clips: [{pattern: beat, at: 0, repeats: 8}]", "clips: [{pattern: beat, at: 0, repeats: 8}, {pattern: fill, at: 28}]");
    let w = found(&fill, "clips-overlap");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!((w[0].level, w[0].to_json()["level"].clone()), (Level::Info, json!("info")));
    assert_eq!(w[0].paths, ["tracks.drums.clips.0", "tracks.drums.clips.1"]);
    assert_eq!(w[0].message, "drums: tracks.drums.clips.0 and tracks.drums.clips.1 sound at once from beat 28 to 32");
    // One after the other, they only meet.
    let after = edit("clips: [{pattern: beat, at: 0, repeats: 8}]", "clips: [{pattern: beat, at: 0, repeats: 7}, {pattern: fill, at: 28}]");
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
fn an_audio_clip_the_song_ends_inside() {
    let audio = |clip: &str| with_track(&format!("  - id: edit\n    pads: {{}}\n    audio: [{clip}]\n"));
    let long = audio("{sample: song, at: 24, source_end_seconds: 6}");
    let w = found(&long, "song-ends-inside");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].message, "tracks.edit.audio.0 runs to beat 36 and the song ends at 32, so its last 4 beats are cut off");
    assert_eq!(w[0].to_json()["at"], json!(32));
    assert_eq!(found(&audio("{sample: song, at: 24, source_end_seconds: 4}"), "song-ends-inside"), []);
    // A clip to the end of its file needs the file's length.
    let open = song(&audio("{sample: song, at: 24}"));
    let seconds = HashMap::from([("song".to_string(), 6.0)]);
    assert_eq!(check(&open, &seconds).iter().filter(|w| w.code == "song-ends-inside").count(), 1);
    assert!(check(&open, &HashMap::new()).iter().all(|w| w.code != "song-ends-inside"));
    // A clip from its file's second 2 at 60 BPM is six beats of eight seconds.
    let slow = song(&audio("{sample: song, at: 24, source_start_seconds: 2, source_bpm: 60}"));
    assert!(check(&slow, &HashMap::from([("song".to_string(), 8.0)])).iter().all(|w| w.code != "song-ends-inside"));
}

#[test]
fn tracks_with_nothing_to_play() {
    let cases = [
        ("  - {id: pad, type: midi, instrument: {synth: {oscillators: {a: {}}}}}\n", "pad: a synth and no notes for it to play"),
        ("  - {id: pad, type: midi}\n", "pad: no instrument and no notes"),
        ("  - {id: pad, type: midi, clips: [{length_beats: 4, notes: [{pitch: 60, duration: 1}]}]}\n", "pad: 1 note and no instrument, so nothing is heard"),
        ("  - {id: pad, pads: {kick: {sample: kick}}}\n", "pad: pads and no clips to play them"),
        ("  - {id: pad, pads: {}}\n", "pad: no pads, clips or audio"),
    ];
    for (track, message) in cases {
        let w = found(&with_track(track), "track-empty");
        assert_eq!(w.iter().map(|w| w.message.as_str()).collect::<Vec<_>>(), [message]);
        assert_eq!(w[0].paths, ["tracks.pad"]);
    }
    assert_eq!(found(&with_track("  - {id: pad, pads: {}, audio: [{sample: song}]}\n"), "track-empty"), []);
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
    // A pattern's pitched event, and the pad's own transpose.
    let pattern = with_track("  - {id: hits, pads: {p: {sample: piano, transpose: 12}}, clips: [{pattern: high, at: 8}]}\n")
        .replace("  fill:", "  high: {length_beats: 4, events: [{at: 2, pad: p, note: C6}]}\n  fill:");
    let w = found(&pattern, "pad-far-from-root");
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!((w[0].paths.clone(), w[0].to_json()["at"].clone()), (vec!["tracks.hits.pads.p".to_string()], json!(10)));
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
