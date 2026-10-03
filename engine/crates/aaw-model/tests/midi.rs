//! MIDI tracks and note clips in the model: what is written, the IDs given,
//! the version a song is saved as, what is refused, and what the schedule
//! plays of the notes.

use aaw_model::schedule::{schedule, track_notes};
use aaw_model::{parse, to_yaml, Project};

const KEYS: &str = r#"
session: {tempo: 120, length_beats: 32, sample_rate: 48000}
tracks:
  - id: keys
    type: midi
    clips:
      - at: 16
        length_beats: 4
        notes:
          - {pitch: 60, duration: 1, velocity: 96}
          - {pitch: E4, duration: 1, velocity: 80}
          - {id: n7, pitch: 67, duration: 1, velocity: 88}
          - {pitch: 62, at: 1.975, duration: 0.5}
          - {pitch: 64, at: 2.025, duration: 1/3}
          - {pitch: 65, at: 7/3, duration: 1/3}
"#;

fn song(text: &str) -> Project {
    parse(text).unwrap_or_else(|e| panic!("{e}"))
}

fn refused(text: &str) -> String {
    match parse(text) {
        Ok(_) => panic!("accepted:\n{text}"),
        Err(e) => e.to_string(),
    }
}

/// KEYS with `with` in place of `from`.
fn keys(from: &str, with: &str) -> String {
    assert!(KEYS.contains(from), "{from}");
    KEYS.replace(from, with)
}

#[test]
fn notes_are_kept_as_written_with_ids_given_where_left_out() {
    let p = song(KEYS);
    let clip = &p.tracks[0].midi.as_ref().unwrap().clips[0];
    assert_eq!(clip.id, "clip1");
    let ids: Vec<&str> = clip.notes.iter().map(|n| n.id.as_str()).collect();
    // A note written with an ID keeps it, and the others follow the highest.
    assert_eq!(ids, ["n8", "n9", "n7", "n10", "n11", "n12"]);
    // A name is stored as its number.
    assert_eq!(clip.notes[1].pitch, 64);
    let yaml = to_yaml(&p);
    assert!(yaml.starts_with("schema_version: 2\n"), "{yaml}");
    for written in ["pitch: 64", "at: 1.975", "at: 2.025", "duration: 1/3", "at: 7/3", "type: midi"] {
        assert!(yaml.contains(written), "{written} in\n{yaml}");
    }
    assert!(!yaml.contains("E4") && !yaml.contains("instrument"), "{yaml}");
    // Saved and read again, the song is the same, IDs and all.
    let again = song(&yaml);
    assert_eq!(to_yaml(&again), yaml);
    assert_eq!(aaw_model::project_hash(&again), aaw_model::project_hash(&p));
}

#[test]
fn a_song_without_a_midi_track_stays_version_1() {
    let p = song("session: {}\nschema_version: 2\n");
    assert!(to_yaml(&p).starts_with("schema_version: 1\n"));
    assert_eq!(p.schema_version(), 1);
    // A song with one written as version 1 is read, and saved as 2.
    let midi = song(&format!("schema_version: 1\n{KEYS}"));
    assert_eq!(midi.schema_version(), 2);
    assert!(refused("schema_version: 3\nsession: {}\n").contains("Input should be 1 or 2"));
}

#[test]
fn clip_ids_are_unique_in_the_song_and_note_ids_in_their_clip() {
    let text = r#"
session: {length_beats: 32}
tracks:
  - {id: a, type: midi, clips: [{id: clip2, length_beats: 4, notes: [{id: n1, pitch: 60, duration: 1}]}, {length_beats: 4}]}
  - {id: b, type: midi, clips: [{length_beats: 4, notes: [{id: n1, pitch: 60, duration: 1}]}]}
"#;
    let p = song(text);
    let ids: Vec<&str> = p.tracks.iter().flat_map(|t| &t.midi.as_ref().unwrap().clips).map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["clip2", "clip3", "clip4"]);
    assert!(refused(&text.replace("{length_beats: 4}]", "{id: clip2, length_beats: 4}]")).contains("Two note clips have the ID clip2"));
    let twice = text.replace("[{id: n1, pitch: 60, duration: 1}]}, {", "[{id: n1, pitch: 60, duration: 1}, {id: n1, pitch: 62, duration: 1}]}, {");
    assert!(refused(&twice).contains("Two notes of a clip have the ID n1"));
}

#[test]
fn what_a_note_or_a_midi_track_cannot_be() {
    let cases = [
        (keys("{pitch: 60, duration: 1, velocity: 96}", "{pitch: 128, duration: 1}"), "less than or equal to 127"),
        (keys("{pitch: 60, duration: 1, velocity: 96}", "{pitch: H2, duration: 1}"), "Invalid note 'H2'"),
        (keys("{pitch: 60, duration: 1, velocity: 96}", "{pitch: 60, duration: 0}"), "duration must be positive"),
        (keys("{pitch: 60, duration: 1, velocity: 96}", "{pitch: 60, at: -1/4, duration: 1}"), "Beat values must be nonnegative"),
        (keys("{pitch: 60, duration: 1, velocity: 96}", "{pitch: 60, duration: 1, velocity: 0}"), "greater than or equal to 1"),
        (keys("length_beats: 4", "length_beats: 0"), "length must be positive"),
        (keys("at: 16", "at: 30"), "keys: clip clip1 exceeds session"),
        (keys("type: midi", "type: midi\n    pads: {}"), "Extra inputs are not permitted"),
        (keys("type: midi", "type: audio"), "Input should be 'midi'"),
        (keys("- at: 16", "- pattern: p\n        at: 16"), "Extra inputs are not permitted"),
    ];
    for (text, message) in cases {
        let error = refused(&text);
        assert!(error.contains(message), "{message}: {error}");
    }
}

const SAMPLED: &str = r#"
session: {tempo: 120, length_beats: 32, sample_rate: 48000}
samples:
  piano: {path: piano.wav, root_note: C4}
  kick: {path: kick.wav}
tracks:
  - id: keys
    type: midi
    instrument:
      sampler:
        pads:
          piano: {sample: piano, mode: gate, release_ms: 300}
          kick: {sample: kick}
        map:
          - {notes: [48, 72], pad: piano, pitched: true}
          - {notes: C2, pad: kick}
    clips:
      - id: phrase
        at: 4
        length_beats: 4
        notes:
          - {pitch: 60, duration: 1, velocity: 127}
          - {pitch: 64, duration: 1}
          - {pitch: 36, at: 1, duration: 1/2}
          - {pitch: 90, at: 2, duration: 1}
          - {pitch: 67, at: 3, duration: 4}
          - {pitch: 69, at: 4, duration: 1}
"#;

#[test]
fn the_sampler_plays_mapped_notes_and_releases_gated_pads_at_the_note_off() {
    let p = song(SAMPLED);
    let rows: Vec<(i64, String, Option<String>, Option<i64>, i64)> = schedule(&p)
        .into_iter()
        .map(|t| (t.start, t.pad, t.event.note, t.cutoff, t.event.velocity))
        .collect();
    // A beat is 24000 frames at 120 BPM and 48 kHz; the clip starts on beat 4.
    let beat = 24000;
    assert_eq!(
        rows,
        [
            (4 * beat, "piano".into(), Some("C4".into()), Some(5 * beat), 127),
            (4 * beat, "piano".into(), Some("E4".into()), Some(5 * beat), 100),
            // An unpitched pad plays as it is, and a one-shot is not released.
            (5 * beat, "kick".into(), None, None, 100),
            // Note 90 is mapped to nothing; the note past the clip's end is cut
            // at it, and the one that starts at the end does not play.
            (7 * beat, "piano".into(), Some("G4".into()), Some(8 * beat), 100),
        ]
    );
    let midi = p.tracks[0].midi.as_ref().unwrap();
    let notes = track_notes(&p, midi);
    assert_eq!(notes.len(), 5, "the note at the clip's end is left out");
    assert_eq!(aaw_model::rules::note_warnings(&p), [
        "keys.phrase: notes n6 start at or after the clip's end and do not play",
        "keys.phrase: the sampler maps no pad to notes 90 (F#6), which are silent",
    ]);
}

#[test]
fn notes_play_at_exact_positions_off_the_grid() {
    let p = song(KEYS);
    let midi = p.tracks[0].midi.as_ref().unwrap();
    let starts: Vec<i64> = track_notes(&p, midi).iter().map(|n| n.start).collect();
    // 12.5 ms either side of beat 2 of a clip on beat 16, at 120 BPM: 600 frames.
    let beat2 = 18 * 24000;
    assert!(starts.contains(&(beat2 - 600)) && starts.contains(&(beat2 + 600)), "{starts:?}");
    // A triplet's start is its exact fraction, rounded once.
    assert!(starts.contains(&(16 * 24000 + 56000)), "{starts:?}");
    // Without an instrument nothing plays, and the notes are all there.
    assert!(schedule(&p).is_empty());
    assert_eq!(track_notes(&p, midi).len(), 6);
}

#[test]
fn what_a_sampler_cannot_map() {
    let cases = [
        (SAMPLED.replace("{notes: C2, pad: kick}", "{notes: 60, pad: kick}"), "note 60 (C4) is mapped to both piano and kick"),
        (SAMPLED.replace("{notes: C2, pad: kick}", "{notes: C2, pad: snare}"), "the map names unknown pad snare"),
        (SAMPLED.replace("{notes: C2, pad: kick}", "{notes: C2, pad: kick, pitched: true}"), "kick needs root_note for a pitched map entry"),
        (SAMPLED.replace("[48, 72]", "[72, 48]"), "from the lower to the higher"),
        (SAMPLED.replace("{sample: kick}", "{sample: snare}"), "keys: unknown sample snare"),
    ];
    for (text, message) in cases {
        let error = refused(&text);
        assert!(error.contains(message), "{message}: {error}");
    }
}

#[test]
fn removing_the_instrument_or_swapping_it_leaves_the_notes() {
    let p = song(SAMPLED);
    let notes = |p: &Project| aaw_model::value::py_eq(&p.tracks[0].dump(true).get("clips").unwrap().clone(), &song(SAMPLED).tracks[0].dump(true).get("clips").unwrap().clone());
    let bare = song(&SAMPLED.replace("    instrument:\n", "    instrument: null\n    old:\n").replace("    old:\n      sampler:\n        pads:\n          piano: {sample: piano, mode: gate, release_ms: 300}\n          kick: {sample: kick}\n        map:\n          - {notes: [48, 72], pad: piano, pitched: true}\n          - {notes: C2, pad: kick}\n", ""));
    assert!(bare.tracks[0].midi.as_ref().unwrap().instrument.is_none());
    assert!(notes(&p) && notes(&bare));
    assert!(schedule(&bare).is_empty());
}
