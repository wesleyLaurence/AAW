//! A track frozen and unfrozen through a session: the file under `frozen/`,
//! the field, what is refused while it is frozen and what is not.

mod common;

use aaw_host::command::Origin;
use aaw_host::session::Session;
use common::{cmd, edit, get, write_song};
use serde_json::json;

fn open() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, true).unwrap();
    (dir, s)
}

#[test]
fn a_track_is_frozen_to_a_file_and_unfrozen() {
    let (dir, mut s) = open();
    let reply = edit(&mut s, json!({"op": "track.freeze", "track": "bass"})).unwrap();
    assert_eq!(reply["label"], "Freeze bass");
    assert_eq!(reply["changed"], true);
    assert_eq!(get(&s, "tracks.bass.frozen"), json!({"path": "frozen/bass.wav"}));
    let info = aaw_engine::sndfile::info(&dir.path().join("frozen/bass.wav")).unwrap();
    assert_eq!(info.rate, 48000);
    assert_eq!(info.channels, 2);
    // The whole song, 32 beats at 120, whatever the track's clips cover.
    assert_eq!(info.frames, 32 * 48000 / 2);
    // The content is kept: the clips, the compressor and the send are there.
    assert_eq!(get(&s, "tracks.bass.clips").as_array().unwrap().len(), 3);
    assert_eq!(get(&s, "tracks.bass.effects.0.type"), "compressor");
    // Saved with the song.
    s.save().unwrap();
    assert!(std::fs::read_to_string(s.path()).unwrap().contains("frozen:\n    path: frozen/bass.wav"));

    // The file plays in the content's place: the program compiles with no
    // inserts on bass.
    let program = aaw_engine::program::compile(s.project(), dir.path()).unwrap();
    let bass = program.tracks.iter().find(|t| t.id == "bass").unwrap();
    assert!(bass.chain.devices.is_empty());
    assert_eq!(bass.voices.len(), 1);

    let unfrozen = edit(&mut s, json!({"op": "track.unfreeze", "track": "bass"})).unwrap();
    assert_eq!(unfrozen["label"], "Unfreeze bass");
    assert_eq!(get(&s, "tracks.bass.frozen"), json!(null));
    assert!(dir.path().join("frozen/bass.wav").is_file(), "the file is left");
    assert_eq!(edit(&mut s, json!({"op": "track.unfreeze", "track": "bass"})).unwrap_err(), "bass is not frozen");
    assert!(edit(&mut s, json!({"op": "track.freeze", "track": "nothing"})).unwrap_err().starts_with("No track nothing"));
}

#[test]
fn a_frozen_track_s_content_cannot_change_and_its_mixer_can() {
    let (_dir, mut s) = open();
    edit(&mut s, json!({"op": "track.freeze", "track": "bass"})).unwrap();
    let refused = |s: &mut Session, j: serde_json::Value| edit(s, j).unwrap_err();
    let content = "bass is frozen: daw track unfreeze bass before changing its clips, pads, instrument, effects or their automation";
    assert_eq!(refused(&mut s, json!({"op": "clip.move", "clip": "tracks.bass.clips.0", "at": 4})), content);
    assert_eq!(refused(&mut s, json!({"op": "set", "path": "tracks.bass.effects.0.threshold_db", "value": -30})), content);
    assert_eq!(refused(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "filter", "mode": "lowpass", "cutoff_hz": 800})), content);
    assert_eq!(refused(&mut s, json!({"op": "set", "path": "tracks.bass.pads.t.gain_db", "value": -3})), content);
    // The pattern its clips play, and the sample its pad names.
    assert_eq!(refused(&mut s, json!({"op": "set", "path": "patterns.bass.length_beats", "value": 8})), content);
    assert_eq!(refused(&mut s, json!({"op": "set", "path": "samples.tone.root_note", "value": "C3"})), content);
    // A lane on an effect is content; one on the fader is not.
    assert_eq!(
        refused(&mut s, json!({"op": "point.add", "owner": "tracks.bass", "param": "effects.0.threshold_db", "at": 0, "value": -20})),
        content
    );
    edit(&mut s, json!({"op": "point.add", "owner": "tracks.bass", "param": "gain_db", "at": 0, "value": -6})).unwrap();
    // The tempo, while anything is frozen.
    assert_eq!(
        refused(&mut s, json!({"op": "set", "path": "session.tempo", "value": 100})),
        "bass is frozen: unfreeze before changing the tempo, the sample rate or the stretcher"
    );
    // The fader, pan, mute, solo, sends and the input are live; so are the
    // other tracks and the pattern they alone play.
    edit(&mut s, json!({"op": "set", "path": "tracks.bass.gain_db", "value": -3})).unwrap();
    edit(&mut s, json!({"op": "set", "path": "tracks.bass.pan", "value": 0.2})).unwrap();
    edit(&mut s, json!({"op": "toggle", "path": "tracks.bass.mute"})).unwrap();
    edit(&mut s, json!({"op": "set", "path": "tracks.bass.sends.0.gain_db", "value": -6})).unwrap();
    edit(&mut s, json!({"op": "set", "path": "tracks.bass.input", "value": {"track": "drums"}})).unwrap();
    edit(&mut s, json!({"op": "clip.move", "clip": "tracks.drums.clips.0", "at": 4})).unwrap();
    edit(&mut s, json!({"op": "set", "path": "patterns.beat.swing", "value": 0.55})).unwrap();
    // Renamed, the track is still held to its file.
    edit(&mut s, json!({"op": "track.rename", "track": "bass", "to": "low"})).unwrap();
    assert_eq!(get(&s, "tracks.low.frozen"), json!({"path": "frozen/bass.wav"}));
    assert_eq!(
        refused(&mut s, json!({"op": "clip.move", "clip": "tracks.low.clips.0", "at": 4})),
        "low is frozen: daw track unfreeze low before changing its clips, pads, instrument, effects or their automation"
    );
    // Undo takes the freeze back with the rest.
    for _ in 0..10 {
        s.undo(Origin::Agent, false).unwrap();
    }
    assert_eq!(get(&s, "tracks.bass.frozen"), json!(null));
    edit(&mut s, json!({"op": "clip.move", "clip": "tracks.bass.clips.0", "at": 4})).unwrap();
}

#[test]
fn a_frozen_track_freezes_again_and_a_frozen_song_renders() {
    let (dir, mut s) = open();
    edit(&mut s, json!({"op": "track.freeze", "track": "drums"})).unwrap();
    let first = std::fs::read(dir.path().join("frozen/drums.wav")).unwrap();
    // Frozen again, from its content, the file is written anew and the
    // song is as it was.
    let again = edit(&mut s, json!({"op": "track.freeze", "track": "drums"})).unwrap();
    assert_eq!(again["changed"], false);
    assert_eq!(std::fs::read(dir.path().join("frozen/drums.wav")).unwrap(), first);
    // A render of the frozen song has the file in its identity.
    let opts = aaw_engine::offline::RenderOptions {
        output: Some(dir.path().join("out")),
        ..Default::default()
    };
    let report = aaw_engine::offline::render(s.path(), &opts).unwrap();
    assert!(report.get("render_id").is_some());
    let _ = cmd(json!({"op": "track.unfreeze", "track": "drums"}));
}
