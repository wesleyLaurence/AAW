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

#[test]
fn the_patchs_effects_are_edited_as_a_chain_and_automated_from_the_track() {
    let (dir, mut s) = open();
    edit(&mut s, json!({"op": "synth.add", "track": "lead"})).unwrap();
    // Added as to any chain, by the synth's path; set by path; one undo step each.
    let r = edit(&mut s, json!({"op": "effect.add", "owner": "tracks.lead.instrument.synth", "type": "chorus", "id": "wide", "mix_percent": 40})).unwrap();
    assert_eq!(r["label"], json!("Add chorus to tracks.lead.instrument.synth"));
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.lead.instrument.synth", "type": "saturation", "mode": "tube"})).unwrap();
    edit(&mut s, json!({"op": "set", "path": "tracks.lead.instrument.synth.effects.wide.rate_hz", "value": 2})).unwrap();
    let got = synth(&s);
    assert_eq!(got["effects"], json!([{"type": "chorus", "id": "wide", "rate_hz": 2.0, "mix_percent": 40.0}, {"type": "saturation", "mode": "tube"}]));
    // Lanes reach them as instrument.effects.REF.FIELD, by ID or index, and
    // one that names a field the effect lacks or a sidechain is refused.
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.lead", "param": "instrument.effects.wide.mix_percent", "points": [{"at": 0, "value": 0}, {"at": 8, "value": 100}]})).unwrap();
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.lead", "param": "instrument.effects.1.drive_db", "points": [{"at": 0, "value": 6}]})).unwrap();
    let e = edit(&mut s, json!({"op": "lane.set", "owner": "tracks.lead", "param": "instrument.effects.1.mode", "points": [{"at": 0, "value": 1}]})).unwrap_err();
    assert!(e.contains("mode cannot be automated"), "{e}");
    // The same field by its index is the same lane.
    let e = edit(&mut s, json!({"op": "lane.set", "owner": "tracks.lead", "param": "instrument.effects.0.mix_percent", "points": [{"at": 0, "value": 50}]})).unwrap_err();
    assert!(e.contains("more than one lane"), "{e}");
    let e = edit(&mut s, json!({"op": "effect.add", "owner": "tracks.lead.instrument.synth", "type": "compressor", "threshold_db": -20, "sidechain": "bass"})).unwrap_err();
    assert!(e.contains("no sidechain"), "{e}");
    // Moving an effect carries its lanes' indices; removing one takes its lanes with it.
    let r = edit(&mut s, json!({"op": "effect.move", "effect": "tracks.lead.instrument.synth.effects.1", "index": 0})).unwrap();
    assert_eq!(r["also"], json!(["tracks.lead: lane instrument.effects.1.drive_db is now instrument.effects.0.drive_db"]));
    assert_eq!(synth(&s)["effects"][0]["type"], json!("saturation"));
    let r = edit(&mut s, json!({"op": "effect.remove", "effect": "tracks.lead.instrument.synth.effects.wide"})).unwrap();
    assert_eq!(r["also"], json!(["tracks.lead: removed lane instrument.effects.wide.mix_percent"]));
    let lanes = get(&s, "tracks.lead.automation");
    assert_eq!(lanes.as_array().unwrap().len(), 1);
    assert_eq!(lanes[0]["param"], json!("instrument.effects.0.drive_db"));
    // Bypassed as any effect, and saved and read back.
    edit(&mut s, json!({"op": "effect.bypass", "effect": "tracks.lead.instrument.synth.effects.0", "bypass": true})).unwrap();
    assert_eq!(synth(&s)["effects"][0]["bypass"], json!(true));
    s.save().unwrap();
    let again = Session::open(&dir.path().join("song.yaml"), true).unwrap();
    assert_eq!(synth(&again)["effects"][0]["mode"], json!("tube"));
    // A patch loaded without the effect takes the lane with it.
    let r = edit(&mut s, json!({"op": "instrument.set", "track": "lead", "instrument": {"synth": {"oscillators": {"a": {}}}}})).unwrap();
    assert_eq!(r["also"], json!(["tracks.lead: removed lane instrument.effects.0.drive_db"]));
    // Unison and a table are fields like any other; a wavetable names a sample.
    edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"oscillators.a.unison": 5, "oscillators.a.unison_detune_cents": 22, "oscillators.b": {"wave": "wavetable", "table": "fold"}}})).unwrap();
    assert_eq!(synth(&s)["oscillators"]["b"], json!({"wave": "wavetable", "table": "fold"}));
    edit(&mut s, json!({"op": "synth.mod", "track": "lead", "source": "velocity", "target": "oscillators.a.unison_detune_cents", "amount": 10})).unwrap();
    let e = edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"oscillators.b.table": "nothing"}})).unwrap_err();
    assert!(e.contains("wavetable nothing"), "{e}");
    edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"oscillators.b.table": "tone"}})).unwrap();
    assert_eq!(get(&s, "tracks.lead.instrument.synth.oscillators.b.table"), json!("tone"));
}
