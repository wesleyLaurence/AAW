//! The Synth through the session's commands: attached to a track or a new
//! one, its fields set by their paths as one step, its matrix edited, lanes
//! on its fields, what is refused, and all of it saved, undone and redone.

mod common;

use aaw_host::command::Origin;
use aaw_host::session::Session;
use common::{edit, get, write_song};
use serde_json::{json, Value as Json};

fn open() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, true).unwrap();
    (dir, s)
}

fn synth(s: &Session) -> Json {
    get(s, "tracks.lead.instrument.synth")
}

#[test]
fn a_synth_is_added_to_a_new_track_or_an_existing_midi_track_and_saved() {
    let (dir, mut s) = open();
    let r = edit(&mut s, json!({"op": "synth.add", "track": "lead"})).unwrap();
    assert_eq!(r["label"], json!("Add Synth track lead"));
    assert_eq!(synth(&s), json!({"oscillators": {"a": {}}}));
    // The full form has every default, as the panel and `get` of a field read it.
    assert_eq!(get(&s, "tracks.lead.instrument.synth.filter.cutoff_hz"), json!(20000.0));
    assert_eq!(get(&s, "tracks.lead.instrument.synth.envelopes.amp.release_ms"), json!(50.0));
    assert_eq!(get(&s, "tracks.lead.instrument.synth.voices"), json!(8));
    // On a MIDI track that has an instrument, the synth takes its place; the notes stay.
    edit(&mut s, json!({"op": "track.add", "id": "keys", "type": "midi"})).unwrap();
    edit(&mut s, json!({"op": "clip.add", "track": "keys", "length_beats": 4, "notes": [{"pitch": 60, "duration": 1}]})).unwrap();
    edit(&mut s, json!({"op": "instrument.set", "track": "keys", "instrument": {"sampler": {"pads": {}, "map": []}}})).unwrap();
    let r = edit(&mut s, json!({"op": "synth.add", "track": "keys"})).unwrap();
    assert_eq!(r["label"], json!("Attach a Synth to keys"));
    assert_eq!(get(&s, "tracks.keys.instrument.synth.oscillators"), json!({"a": {}}));
    assert_eq!(get(&s, "tracks.keys.instrument.sampler"), Json::Null);
    assert_eq!(get(&s, "tracks.keys.clips.clip1.notes.n1.pitch"), json!(60));
    // Not on a track of patterns.
    let e = edit(&mut s, json!({"op": "synth.add", "track": "bass"})).unwrap_err();
    assert!(e.contains("not a MIDI track"), "{e}");
    s.save().unwrap();
    let text = std::fs::read_to_string(s.path()).unwrap();
    assert!(text.contains("instrument: {synth: {oscillators: {a: {}}}}"), "{text}");
    let again = Session::open(&dir.path().join("song.yaml"), true).unwrap();
    assert_eq!(synth(&again), json!({"oscillators": {"a": {}}}));
}

#[test]
fn fields_are_set_by_their_paths_as_one_step_and_the_matrix_is_edited() {
    let (_d, mut s) = open();
    edit(&mut s, json!({"op": "synth.add", "track": "lead"})).unwrap();
    let r = edit(
        &mut s,
        json!({"op": "synth.set", "track": "lead", "values": {
            "filter.cutoff_hz": 900,
            "filter.slope_db_per_octave": 24,
            "envelopes.amp.release_ms": 600,
            "oscillators.sub": {"wave": "sine", "octave": -1, "filter": false},
            "lfos.lfo1": {"rate_beats": "1/2", "retrigger": true},
            "macros.tone": 40,
        }}),
    )
    .unwrap();
    assert!(r["label"].as_str().unwrap().starts_with("Set synth of lead: filter.cutoff_hz 900, "), "{}", r["label"]);
    let got = synth(&s);
    assert_eq!(got["filter"], json!({"slope_db_per_octave": 24, "cutoff_hz": 900.0}));
    assert_eq!(got["oscillators"]["sub"], json!({"wave": "sine", "octave": -1, "filter": false}));
    assert_eq!(got["lfos"]["lfo1"], json!({"rate_beats": "1/2", "retrigger": true}));
    assert_eq!(got["macros"], json!({"tone": 40.0}));
    // One undo step for the whole set.
    s.undo(Origin::User, false).unwrap();
    assert_eq!(synth(&s), json!({"oscillators": {"a": {}}}));
    s.undo(Origin::User, true).unwrap();
    assert_eq!(synth(&s)["macros"], json!({"tone": 40.0}));
    // The matrix: an entry added, its amount changed, a wrong one refused, one removed.
    let r = edit(&mut s, json!({"op": "synth.mod", "track": "lead", "source": "macros.tone", "target": "filter.cutoff_hz", "amount": 3})).unwrap();
    assert_eq!(r["label"], json!("Modulate filter.cutoff_hz by macros.tone by 3 on lead"));
    edit(&mut s, json!({"op": "synth.mod", "track": "lead", "source": "lfo1", "target": "pitch", "amount": 0.1})).unwrap();
    edit(&mut s, json!({"op": "synth.mod", "track": "lead", "source": "macros.tone", "target": "filter.cutoff_hz", "amount": 2})).unwrap();
    let entries = synth(&s)["modulation"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["amount"], json!(2.0));
    let e = edit(&mut s, json!({"op": "synth.mod", "track": "lead", "source": "lfo2", "target": "pitch", "amount": 1})).unwrap_err();
    assert!(e.contains("source lfo2"), "{e}");
    let e = edit(&mut s, json!({"op": "synth.mod", "track": "lead", "source": "lfo1", "target": "oscillators.b.pitch", "amount": 1})).unwrap_err();
    assert!(e.contains("no oscillator b"), "{e}");
    edit(&mut s, json!({"op": "synth.mod", "track": "lead", "source": "lfo1", "target": "pitch", "remove": true})).unwrap();
    assert_eq!(synth(&s)["modulation"].as_array().unwrap().len(), 1);
    // A part removed with null; the last oscillator cannot go.
    edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"oscillators.sub": null}})).unwrap();
    assert_eq!(synth(&s)["oscillators"], json!({"a": {}}));
    let e = edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"oscillators.a": null}})).unwrap_err();
    assert!(e.contains("1 to 4 oscillators"), "{e}");
    // Out of range, and an unknown field, are refused with the model's words.
    let e = edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"filter.cutoff_hz": 5}})).unwrap_err();
    assert!(e.contains("greater than or equal to 10"), "{e}");
    let e = edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"filter.cutoff": 500}})).unwrap_err();
    assert!(e.contains("filter.cutoff") && e.contains("Extra inputs"), "{e}");
    // Removing an envelope the matrix uses is refused, naming the entry.
    edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"envelopes.env2": {"attack_ms": 0, "sustain_percent": 0}}})).unwrap();
    edit(&mut s, json!({"op": "synth.mod", "track": "lead", "source": "env2", "target": "filter.cutoff_hz", "amount": 2})).unwrap();
    let e = edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"envelopes.env2": null}})).unwrap_err();
    assert!(e.contains("source env2"), "{e}");
}

#[test]
fn a_note_preview_is_a_transport_command() {
    use aaw_host::command::{Command, Kind};
    let c: Command = serde_json::from_value(json!({"op": "note.preview", "track": "lead", "pitch": "C4"})).unwrap();
    assert_eq!(c.kind(), Kind::Transport);
    assert_eq!(c.op(), "note.preview");
}

#[test]
fn lanes_reach_the_synths_fields_and_go_with_the_instrument() {
    let (_d, mut s) = open();
    edit(&mut s, json!({"op": "synth.add", "track": "lead"})).unwrap();
    edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"macros.tone": 10}})).unwrap();
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.lead", "param": "instrument.filter.cutoff_hz", "points": [{"at": 0, "value": 200}, {"at": 8, "value": 8000}]})).unwrap();
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.lead", "param": "instrument.macros.tone", "points": [{"at": 0, "value": 0}, {"at": 8, "value": 100}]})).unwrap();
    let e = edit(&mut s, json!({"op": "lane.set", "owner": "tracks.lead", "param": "instrument.filter.mode", "points": [{"at": 0, "value": 1}]})).unwrap_err();
    assert!(e.contains("mode cannot be automated"), "{e}");
    let e = edit(&mut s, json!({"op": "lane.set", "owner": "tracks.lead", "param": "instrument.filter.cutoff_hz", "points": [{"at": 0, "value": 1}]})).unwrap_err();
    assert!(e.contains("outside 10 to 20000"), "{e}");
    let e = edit(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "instrument.filter.cutoff_hz", "points": [{"at": 0, "value": 100}]})).unwrap_err();
    assert!(e.contains("has no synth"), "{e}");
    // The instrument replaced: a lane on a field the new one lacks goes, one it has stays.
    let r = edit(&mut s, json!({"op": "instrument.set", "track": "lead", "instrument": {"synth": {"oscillators": {"a": {}}}}})).unwrap();
    assert_eq!(r["also"], json!(["tracks.lead: removed lane instrument.macros.tone"]));
    let lanes = get(&s, "tracks.lead.automation");
    assert_eq!(lanes.as_array().unwrap().len(), 1);
    assert_eq!(lanes[0]["param"], json!("instrument.filter.cutoff_hz"));
    let r = edit(&mut s, json!({"op": "instrument.set", "track": "lead", "instrument": null})).unwrap();
    assert_eq!(r["also"], json!(["tracks.lead: removed lane instrument.filter.cutoff_hz"]));
    assert_eq!(get(&s, "tracks.lead.automation"), json!([]));
}
