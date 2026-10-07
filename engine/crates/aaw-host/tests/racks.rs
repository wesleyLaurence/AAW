//! Effect racks through the session's commands: a track's chain saved to the
//! workspace library and added to a chain in another song as the same
//! effects, ids that the chain has given a number, a sidechain kept only
//! where the song allows it, lanes following the effects a rack pushes
//! along, and what is refused.

mod common;

use aaw_host::command::Origin;
use aaw_host::racks;
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

fn kinds(s: &Session, owner: &str) -> Vec<String> {
    get(s, &format!("{owner}.effects"))
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap().to_string())
        .collect()
}

fn ids(s: &Session, owner: &str) -> Vec<Json> {
    get(s, &format!("{owner}.effects")).as_array().unwrap().iter().map(|e| e.get("id").cloned().unwrap_or(Json::Null)).collect()
}

#[test]
fn a_chain_is_saved_as_a_rack_and_added_to_a_chain_in_another_song() {
    let (_d1, mut s) = open();
    // A chain of three on the drums: a compressor keyed by a percussion
    // track, an EQ and a limiter.
    edit(&mut s, json!({"op": "track.add", "id": "perc"})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.drums", "type": "compressor", "id": "glue", "threshold_db": -18, "ratio": 4, "sidechain": "perc"})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.drums", "type": "eq", "bands": [{"shape": "highpass", "freq_hz": 40}]})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.drums", "type": "limiter", "id": "top", "ceiling_db": -0.5})).unwrap();
    let name = format!("Test Glue {}", std::process::id());
    let slug = aaw_host::patches::slug(&name);
    let r = edit(&mut s, json!({"op": "rack.save", "owner": "tracks.drums", "name": name, "description": "A test", "tags": ["drums", "bus"]})).unwrap();
    assert_eq!(r["label"], json!(format!("Save rack {name} from tracks.drums")));
    assert_eq!(r["changed"], json!(false), "saving a rack changes nothing in the song");
    assert_eq!(r["effects"], json!(["compressor", "eq", "limiter"]));
    let file = racks::dir().join(format!("{slug}.yaml"));
    assert_eq!(r["file"], json!(file));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.starts_with(&format!("name: {name}\ndescription: A test\ntags:\n- drums\n- bus\nsaved_by: agent\nsaved_at: '")), "{text}");
    assert!(text.contains("\neffects:\n") && text.contains("sidechain: perc") && text.contains("ceiling_db: -0.5"), "{text}");
    // Listed, found by name or slug, and searched by name, tag and kind.
    let listing = racks::list();
    let mine = listing.racks.iter().find(|r| r.slug == slug).unwrap();
    assert_eq!((mine.file.as_deref(), mine.saved_by.as_deref()), (Some(file.as_path()), Some("agent")));
    assert_eq!(mine.kinds(), ["compressor", "eq", "limiter"]);
    assert!(racks::matches(mine, "glue bus") && racks::matches(mine, "LIMITER") && !racks::matches(mine, "reverb"));
    assert_eq!(racks::find(&name).unwrap().name, name);
    assert_eq!(racks::find(&slug).unwrap().name, name);
    // Saved again under the same name only with replace, which keeps the words.
    let e = edit(&mut s, json!({"op": "rack.save", "owner": "tracks.drums", "name": name})).unwrap_err();
    assert!(e.contains("already saved") && e.contains("--replace"), "{e}");
    edit(&mut s, json!({"op": "set", "path": "tracks.drums.effects.top.ceiling_db", "value": -2})).unwrap();
    edit(&mut s, json!({"op": "rack.save", "owner": "tracks.drums", "name": name, "replace": true})).unwrap();
    let again = racks::find(&name).unwrap();
    assert_eq!((again.description.as_str(), again.tags.len()), ("A test", 2));
    assert!(racks::text_of(&again).contains("ceiling_db: -2"));
    // Added to a return in another song: the same effects, in one undo step,
    // but the sidechain is dropped, since that song has no perc.
    let d2 = tempfile::tempdir().unwrap();
    let path = write_song(d2.path());
    let mut other = Session::open(&path, true).unwrap();
    let r = edit(&mut other, json!({"op": "rack.load", "owner": "returns.plate", "rack": name})).unwrap();
    assert_eq!(r["label"], json!(format!("Add rack {name} to returns.plate")));
    assert_eq!(r["rack"], json!(name));
    assert_eq!(r["effects"], json!(["compressor", "eq", "limiter"]));
    assert_eq!(r["handles"].as_array().map(Vec::len), Some(3));
    assert_eq!(kinds(&other, "returns.plate"), ["reverb", "compressor", "eq", "limiter"]);
    assert_eq!(r["also"], json!(["returns.plate: the rack's compressor was keyed by perc, which this chain cannot be; set its sidechain"]));
    assert!(get(&other, "returns.plate.effects.glue").get("sidechain").is_none());
    assert_eq!(get(&other, "returns.plate.effects.top.ceiling_db"), json!(-2.0));
    other.undo(Origin::User, false).unwrap();
    assert_eq!(kinds(&other, "returns.plate"), ["reverb"]);
    // On a track of the same song, the key is kept; on the key itself, dropped.
    let r = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.bass", "rack": slug, "index": 0})).unwrap();
    assert!(r.get("also").is_none(), "{}", r["also"]);
    assert_eq!(kinds(&s, "tracks.bass"), ["compressor", "eq", "limiter", "compressor"]);
    assert_eq!(get(&s, "tracks.bass.effects.glue.sidechain"), json!("perc"));
    let r = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.perc", "rack": name})).unwrap();
    assert_eq!(r["also"], json!(["tracks.perc: the rack's compressor was keyed by perc, which this chain cannot be; set its sidechain"]));
    // On the master, no sidechain; in a Synth's patch, none either.
    edit(&mut s, json!({"op": "rack.load", "owner": "master", "rack": name})).unwrap();
    assert!(get(&s, "master.effects.glue").get("sidechain").is_none());
    edit(&mut s, json!({"op": "synth.add", "track": "lead"})).unwrap();
    let r = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.lead.instrument.synth", "rack": name})).unwrap();
    assert_eq!(r["label"], json!(format!("Add rack {name} to tracks.lead.instrument.synth")));
    assert_eq!(kinds(&s, "tracks.lead.instrument.synth"), ["compressor", "eq", "limiter"]);
    assert!(get(&s, "tracks.lead.instrument.synth.effects.glue").get("sidechain").is_none());
    // Added again where the ids are taken, each is given a number.
    let r = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.bass", "rack": name})).unwrap();
    assert_eq!(ids(&s, "tracks.bass"), json!(["glue", null, "top", null, "glue-2", null, "top-2"]).as_array().unwrap().clone());
    assert_eq!(r["also"].as_array().unwrap().len(), 2);
    assert!(r["also"][0].as_str().unwrap().contains("glue of the rack is glue-2 here"), "{}", r["also"]);
    edit(&mut s, json!({"op": "rack.load", "owner": "tracks.bass", "rack": name})).unwrap();
    assert_eq!(ids(&s, "tracks.bass")[7], json!("glue-3"));
    // A rack's chain is saved from a group and from the master too.
    edit(&mut s, json!({"op": "group.add", "id": "bus", "tracks": ["drums", "bass"]})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "groups.bus", "type": "saturation", "drive_db": 3})).unwrap();
    let group_rack = format!("Test Warm {}", std::process::id());
    let r = edit(&mut s, json!({"op": "rack.save", "owner": "groups.bus", "name": group_rack})).unwrap();
    assert_eq!(r["effects"], json!(["saturation"]));
    let r = edit(&mut s, json!({"op": "rack.save", "owner": "master", "name": format!("Test Master {}", std::process::id())})).unwrap();
    assert_eq!(r["effects"], json!(["compressor", "eq", "limiter"]));
    // What is refused: a name with nothing to make a file of, an empty chain,
    // an owner without a chain, a rack that is not there, an index past the end.
    let e = edit(&mut s, json!({"op": "rack.save", "owner": "tracks.drums", "name": "***"})).unwrap_err();
    assert!(e.contains("letter or a digit"), "{e}");
    edit(&mut s, json!({"op": "track.add", "id": "empty"})).unwrap();
    let e = edit(&mut s, json!({"op": "rack.save", "owner": "tracks.empty", "name": "Nothing"})).unwrap_err();
    assert!(e.contains("no effects to save"), "{e}");
    let e = edit(&mut s, json!({"op": "rack.save", "owner": "samples.hit", "name": "Nothing"})).unwrap_err();
    assert!(e.contains("not a track, return, master or synth"), "{e}");
    let e = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.empty", "rack": "Nothing Here"})).unwrap_err();
    assert!(e.contains("No rack named Nothing Here"), "{e}");
    let e = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.empty", "rack": name, "index": 1})).unwrap_err();
    assert!(e.contains("past the end"), "{e}");
    // A file in the library that is not a rack is named, not fatal.
    std::fs::write(racks::dir().join("broken.yaml"), "effects: [{type: limiter, ceiling_db: 3}]\n").unwrap();
    let listing = racks::list();
    assert!(listing.problems.iter().any(|p| p.contains("broken.yaml") && p.contains("ceiling_db")), "{:?}", listing.problems);
    assert!(listing.racks.iter().all(|r| r.slug != "broken"));
    std::fs::remove_file(racks::dir().join("broken.yaml")).unwrap();
    // A .yaml file anywhere loads by its path.
    let elsewhere = d2.path().join("Elsewhere Glue.yaml");
    std::fs::copy(&file, &elsewhere).unwrap();
    let r = edit(&mut other, json!({"op": "rack.load", "owner": "tracks.drums", "rack": elsewhere.to_string_lossy()})).unwrap();
    assert_eq!(r["label"], json!(format!("Add rack {name} to tracks.drums")));
}

#[test]
fn lanes_follow_the_effects_a_rack_pushes_along() {
    let (_d, mut s) = open();
    // The bass has a compressor at 0 with a lane on it by index.
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "effects.0.threshold_db", "points": [{"at": 0, "value": -20}, {"at": 16, "value": -10}]})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.drums", "type": "utility", "gain_db": -3})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.drums", "type": "filter", "mode": "lowpass", "cutoff_hz": 8000})).unwrap();
    let name = format!("Test Trim {}", std::process::id());
    edit(&mut s, json!({"op": "rack.save", "owner": "tracks.drums", "name": name})).unwrap();
    let r = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.bass", "rack": name, "index": 0})).unwrap();
    assert_eq!(kinds(&s, "tracks.bass"), ["utility", "filter", "compressor"]);
    assert_eq!(r["also"], json!(["tracks.bass: lane effects.0.threshold_db is now effects.2.threshold_db"]));
    assert_eq!(get(&s, "tracks.bass.automation")[1]["param"], json!("effects.2.threshold_db"));
    s.undo(Origin::User, false).unwrap();
    assert_eq!(get(&s, "tracks.bass.automation")[1]["param"], json!("effects.0.threshold_db"));
    assert_eq!(kinds(&s, "tracks.bass"), ["compressor"]);
}
