//! Patterns converted (D190) after patterns were removed (D191): a song of
//! pads and pattern clips read from its text is converted to Drum Racks and
//! note clips exactly as the conversion did while the engine still played
//! patterns, when every case below was rendered both ways and found to sound
//! the same, sample for sample. `converted/NAME.before.yaml` is each song as
//! written and `NAME.after.yaml` what that conversion wrote.

use aaw_model::{convert, parse, to_yaml, Project};
use std::path::Path;

const CASES: [&str; 4] = ["voices", "mix", "cases", "pitched"];

fn fixture(name: &str, part: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/converted").join(format!("{name}.{part}.yaml"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn converted(name: &str) -> Project {
    let (p, was) = convert::read(&fixture(name, "before")).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert!(was, "{name} holds patterns");
    p
}

#[test]
fn each_song_converts_as_it_did_when_patterns_still_played() {
    for name in CASES {
        // The conversion then wrote an audio track's `pads: {}`, which the
        // song no longer writes; read, the two are the same song.
        let then = parse(&fixture(name, "after")).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(to_yaml(&converted(name)), to_yaml(&then), "{name}");
    }
}

#[test]
fn a_converted_song_keeps_its_tracks_and_plays_its_pads_from_drum_racks() {
    let new = converted("cases");
    // The drums keep their name, and the audio clip has a track of its own
    // right after them with the same chain.
    let ids: Vec<&str> = new.tracks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["drums", "drums-audio", "bass"]);
    assert_eq!(new.tracks[1].audio.len(), 1);
    assert_eq!(new.tracks[1].effects.len(), 1);
    let rack = new.tracks[0].midi.as_ref().unwrap().sampler().unwrap();
    assert!(rack.drum_rack);
    let key = |pad: &str| rack.map.iter().find(|m| m.pad == pad).map(|m| m.low);
    // The General MIDI keys for the names it knows; the rest from C2 up.
    assert_eq!((key("kick"), key("snare"), key("hat"), key("ohat")), (Some(36), Some(38), Some(42), Some(46)));
    assert_eq!(key("spare"), Some(37));
    // A transposed hit plays a copy of its pad, moved as far.
    assert_eq!(rack.pads["kick-2"].transpose, -2.0);
    assert_eq!(rack.pads["kick+0.5"].transpose, 0.5);
    // The bass's notes are a pitched range; its hit without a note a key.
    let bass = new.tracks[2].midi.as_ref().unwrap();
    assert!(bass.sampler().unwrap().map.iter().any(|m| m.pitched && m.pad == "bass"));
    // The line's last note is gated past the pattern's end, so its clip is
    // written out rather than looped.
    assert!(bass.clips[0].loop_beats.is_none());
    assert_eq!(new.tracks[0].midi.as_ref().unwrap().clips[0].loop_beats.as_ref().map(|b| b.text()), Some("4".into()));
    assert!(!new.tracks[0].midi.as_ref().unwrap().clips[1].active);
    // Pads far apart share the keys by the notes they play.
    let pitched = converted("pitched");
    let rack = pitched.tracks[0].midi.as_ref().unwrap().sampler().unwrap();
    assert!(rack.map.iter().all(|m| m.high - m.low < 10), "{:?}", rack.map);
}

#[test]
fn reading_a_song_converts_it_once_and_keeps_it_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("song.yaml");
    let before = fixture("voices", "before");
    std::fs::write(&path, &before).unwrap();
    let p = aaw_model::load(&path, false).unwrap();
    assert!(p.tracks.iter().all(|t| t.midi.is_some()));
    let kept = dir.path().join("song.before-drum-rack.yaml");
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), before);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), to_yaml(&p));
    // Read again, nothing changes and nothing more is kept.
    let after = std::fs::read_to_string(&path).unwrap();
    aaw_model::load(&path, false).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), after);
    assert!(!dir.path().join("song.before-drum-rack-2.yaml").exists());
}

#[test]
fn a_song_of_patterns_given_as_text_is_refused_with_why() {
    let error = parse(&fixture("voices", "before")).unwrap_err().to_string();
    assert!(error.contains("converted to a Drum Rack and note clips when it is read from its file"), "{error}");
    // A song made outside the model that puts them back is refused too.
    let p = converted("voices");
    let mut doc = p.dump(false);
    if let aaw_model::value::Value::Dict(d) = &mut doc {
        d.insert(aaw_model::value::Key::str("patterns"), aaw_model::json_value(r#"{"p": {"length_beats": 4}}"#).unwrap());
    }
    let error = Project::validate(&doc).unwrap_err().to_string();
    assert!(error.contains("Patterns are how songs before note clips played drums"), "{error}");
}

#[test]
fn an_audio_track_written_with_empty_pads_reads_and_is_written_without_them() {
    let p = parse("session: {}\nsamples: {s: {path: s.wav}}\ntracks:\n- id: a\n  pads: {}\n  clips: []\n  audio: [{sample: s}]\n").unwrap();
    let yaml = to_yaml(&p);
    assert!(!yaml.contains("pads") && !yaml.contains("clips"), "{yaml}");
    let error = parse("session: {}\nsamples: {s: {path: s.wav}}\ntracks:\n- id: a\n  pads: {k: {sample: s}}\n").unwrap_err().to_string();
    assert!(error.contains("converted"), "{error}");
}
