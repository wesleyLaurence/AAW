//! Patches through the session's commands: a factory patch attached to a
//! new track or loaded into one, a track's synth saved to the workspace
//! library and loaded into another song as the same sound, a saved patch
//! shadowing a factory one, and every factory patch rendered.

mod common;

use aaw_engine::audition::{audition, AuditionOptions};
use aaw_host::command::Origin;
use aaw_host::patches;
use aaw_host::session::Session;
use common::{edit, get, write_song};
use serde_json::{json, Value as Json};
use std::path::Path;
use std::sync::OnceLock;

/// The workspace every test saves into: a folder of this test run's, set
/// once before any test reads it, so the person's library is never written.
fn workspace() -> &'static Path {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: set once, before any test reads it; every test calls this first.
        unsafe { std::env::set_var("AAW_WORKSPACE", dir.path()) };
        dir
    })
    .path()
}

fn open() -> (tempfile::TempDir, Session) {
    workspace();
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, true).unwrap();
    (dir, s)
}

fn synth(s: &Session, track: &str) -> Json {
    get(s, &format!("tracks.{track}.instrument.synth"))
}

#[test]
fn a_factory_patch_is_attached_to_a_new_track_or_loaded_into_one() {
    let (_d, mut s) = open();
    let r = edit(&mut s, json!({"op": "synth.add", "track": "keys", "patch": "Soft Pad"})).unwrap();
    assert_eq!(r["label"], json!("Add Synth track keys with Soft Pad"));
    assert_eq!((&r["patch"], &r["factory"]), (&json!("Soft Pad"), &json!(true)));
    let got = synth(&s, "keys");
    assert_eq!(got["patch"], json!("Soft Pad"));
    assert_eq!(got["oscillators"]["b"]["detune_cents"], json!(8.0));
    assert_eq!(get(&s, "tracks.keys.instrument.synth.envelopes.amp.attack_ms"), json!(400.0));
    // The name is the first field, as the song writes it.
    let keys: Vec<&str> = got.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys[0], "patch");
    // Loaded into a track by its slug, with notes that stay, in one undo step.
    edit(&mut s, json!({"op": "clip.add", "track": "keys", "length_beats": 4, "notes": [{"pitch": 60, "duration": 1}]})).unwrap();
    let r = edit(&mut s, json!({"op": "patch.load", "track": "keys", "patch": "sub-bass"})).unwrap();
    assert_eq!(r["label"], json!("Load patch Sub Bass into keys"));
    assert_eq!(synth(&s, "keys")["patch"], json!("Sub Bass"));
    assert_eq!(synth(&s, "keys")["voices"], json!(1));
    assert_eq!(get(&s, "tracks.keys.clips.clip1.notes.n1.pitch"), json!(60));
    s.undo(Origin::User, false).unwrap();
    assert_eq!(synth(&s, "keys")["patch"], json!("Soft Pad"));
    // A lane on a field the new patch lacks goes with the old one.
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.keys", "param": "instrument.macros.tone", "points": [{"at": 0, "value": 10}]})).unwrap();
    let r = edit(&mut s, json!({"op": "patch.load", "track": "keys", "patch": "Pluck"})).unwrap();
    assert_eq!(r["also"], json!(["tracks.keys: removed lane instrument.macros.tone"]));
    // Loaded into a MIDI track with no instrument, and refused elsewhere.
    edit(&mut s, json!({"op": "track.add", "id": "empty", "type": "midi"})).unwrap();
    edit(&mut s, json!({"op": "patch.load", "track": "empty", "patch": "Hat"})).unwrap();
    assert_eq!(synth(&s, "empty")["patch"], json!("Hat"));
    let e = edit(&mut s, json!({"op": "patch.load", "track": "bass", "patch": "Hat"})).unwrap_err();
    assert!(e.contains("not a MIDI track"), "{e}");
    let e = edit(&mut s, json!({"op": "synth.add", "track": "x", "patch": "Nothing Here"})).unwrap_err();
    assert!(e.contains("No patch named Nothing Here"), "{e}");
}

#[test]
fn a_synth_is_saved_as_a_patch_and_loaded_into_another_song_as_the_same_sound() {
    let (d1, mut s) = open();
    edit(&mut s, json!({"op": "synth.add", "track": "lead"})).unwrap();
    edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"filter.cutoff_hz": 640, "oscillators.b": {"wave": "square", "octave": 1, "level_db": -14}, "envelopes.amp.release_ms": 300}})).unwrap();
    edit(&mut s, json!({"op": "synth.mod", "track": "lead", "source": "velocity", "target": "filter.cutoff_hz", "amount": 2})).unwrap();
    let name = format!("Test Save {}", std::process::id());
    let slug = patches::slug(&name);
    let r = edit(&mut s, json!({"op": "patch.save", "track": "lead", "name": name, "description": "A test", "tags": ["test", "keys"]})).unwrap();
    assert_eq!(r["label"], json!(format!("Save patch {name} from lead")));
    assert_eq!(r["changed"], json!(true), "the song now names its patch");
    assert_eq!(synth(&s, "lead")["patch"], json!(name));
    let file = patches::dir().join(format!("{slug}.yaml"));
    assert_eq!(r["file"], json!(file));
    assert_eq!(r["shadows_factory"], json!(false));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.starts_with(&format!("name: {name}\ndescription: A test\ntags:\n- test\n- keys\nsaved_by: agent\nsaved_at: '")), "{text}");
    assert!(text.contains("\nsynth:\n  oscillators:\n    a: {}\n    b:\n      wave: square\n"), "{text}");
    assert!(!text.contains("patch:"), "the name is the file's: {text}");
    // Listed among the saved patches, found by name or slug, and read back whole.
    let listing = patches::list();
    let mine = listing.patches.iter().find(|p| p.slug == slug).unwrap();
    assert!(!mine.factory && mine.file.as_deref() == Some(file.as_path()) && mine.saved_by.as_deref() == Some("agent"));
    assert_eq!(mine.tags, ["test", "keys"]);
    assert_eq!(patches::find(&name).unwrap().name, name);
    assert_eq!(patches::find(&slug).unwrap().name, name);
    assert_eq!(patches::find(&name.to_uppercase()).unwrap().name, name);
    // Saved again under the same name only with replace, which keeps the words.
    let e = edit(&mut s, json!({"op": "patch.save", "track": "lead", "name": name})).unwrap_err();
    assert!(e.contains("already saved") && e.contains("--replace"), "{e}");
    edit(&mut s, json!({"op": "synth.set", "track": "lead", "values": {"filter.cutoff_hz": 900}})).unwrap();
    let r = edit(&mut s, json!({"op": "patch.save", "track": "lead", "name": name, "replace": true})).unwrap();
    assert_eq!(r["changed"], json!(false), "the song already names its patch");
    let again = patches::find(&name).unwrap();
    assert_eq!((again.description.as_str(), again.tags.len()), ("A test", 2));
    assert_eq!(patches::find(&name).unwrap().synth.get("filter").and_then(|f| f.get("cutoff_hz")).map(|v| format!("{v:?}")), Some("Float(900.0)".into()));
    // Loaded into another song, the synth is the same mapping, and so the same sound.
    let d2 = tempfile::tempdir().unwrap();
    let path = write_song(d2.path());
    let mut other = Session::open(&path, true).unwrap();
    edit(&mut other, json!({"op": "synth.add", "track": "lead", "patch": name})).unwrap();
    assert_eq!(synth(&other, "lead"), synth(&s, "lead"));
    s.save().unwrap();
    other.save().unwrap();
    let heard = |dir: &Path| {
        audition(
            &dir.join("song.yaml"),
            &AuditionOptions { track: "lead".into(), notes: vec![48, 55], velocity: 96, length_beats: 1.0, track_chain: false, output: Some(dir.join("heard.wav")) },
        )
        .unwrap()
    };
    let (a, b) = (heard(d1.path()), heard(d2.path()));
    assert_eq!(std::fs::read(d1.path().join("heard.wav")).unwrap(), std::fs::read(d2.path().join("heard.wav")).unwrap());
    assert!(a.get("peak_dbfs").is_some() && b.get("peak_dbfs").is_some());
    // A saved patch of a factory name shadows the factory one.
    let r = edit(&mut s, json!({"op": "patch.save", "track": "lead", "name": "Bell", "description": "Mine"})).unwrap();
    assert_eq!(r["shadows_factory"], json!(true));
    let listing = patches::list();
    let bells: Vec<&patches::Patch> = listing.patches.iter().filter(|p| p.slug == "bell").collect();
    assert_eq!(bells.len(), 1);
    assert!(!bells[0].factory && bells[0].description == "Mine");
    assert_eq!(patches::find("Bell").unwrap().synth.get("filter").and_then(|f| f.get("cutoff_hz")).map(|v| format!("{v:?}")), Some("Float(900.0)".into()));
    // What is refused: a name with nothing to make a file of, and a track without a synth.
    let e = edit(&mut s, json!({"op": "patch.save", "track": "lead", "name": "***"})).unwrap_err();
    assert!(e.contains("letter or a digit"), "{e}");
    let e = edit(&mut s, json!({"op": "patch.save", "track": "bass", "name": "Bass"})).unwrap_err();
    assert!(e.contains("not a MIDI track"), "{e}");
    // A file in the library that is not a patch is named, not fatal.
    std::fs::write(patches::dir().join("broken.yaml"), "synth: {oscillators: {a: {wave: sawtooth}}}\n").unwrap();
    let listing = patches::list();
    assert!(listing.problems.iter().any(|p| p.contains("broken.yaml") && p.contains("wave")), "{:?}", listing.problems);
    assert!(listing.patches.iter().all(|p| p.slug != "broken"));
    let e = patches::find("broken").unwrap_err();
    assert!(e.contains("broken.yaml"), "{e}");
    std::fs::remove_file(patches::dir().join("broken.yaml")).unwrap();
    // A .yaml file anywhere loads by its path.
    let elsewhere = d2.path().join("Elsewhere Lead.yaml");
    std::fs::copy(&file, &elsewhere).unwrap();
    let r = edit(&mut other, json!({"op": "patch.load", "track": "lead", "patch": elsewhere.to_string_lossy()})).unwrap();
    assert_eq!(r["label"], json!(format!("Load patch {name} into lead")));
}

#[test]
fn every_factory_patch_renders_and_sits_under_full_scale() {
    let (d, mut s) = open();
    for p in patches::factory() {
        let track = format!("t-{}", p.slug);
        edit(&mut s, json!({"op": "synth.add", "track": track, "patch": p.name})).unwrap();
    }
    s.save().unwrap();
    for p in patches::factory() {
        let track = format!("t-{}", p.slug);
        // Low notes for the drums and basses, middle ones for the rest.
        let notes = if p.tags.iter().any(|t| t == "drum" || t == "bass" || t == "sub") { vec![36, 41] } else { vec![60, 64, 67] };
        let heard = audition(
            &d.path().join("song.yaml"),
            &AuditionOptions { track: track.clone(), notes, velocity: 100, length_beats: 1.5, track_chain: false, output: Some(d.path().join(format!("{}.wav", p.slug))) },
        )
        .unwrap_or_else(|e| panic!("{}: {e}", p.name));
        let peak = match heard.get("peak_dbfs") {
            Some(aaw_model::value::Value::Float(x)) => *x,
            other => panic!("{}: {other:?}", p.name),
        };
        assert!(peak > -30.0 && peak <= 0.0, "{}: peak {peak} dBFS", p.name);
        assert!(matches!(heard.get("scaled_db"), Some(aaw_model::value::Value::Float(x)) if *x == 0.0), "{}: scaled", p.name);
    }
}
