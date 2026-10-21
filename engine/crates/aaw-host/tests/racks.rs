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
use aaw_model::value::Value;
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
    let r = edit(&mut other, json!({"op": "rack.load", "owner": "returns.plate", "rack": name, "flat": true})).unwrap();
    assert_eq!(r["label"], json!(format!("Add rack {name} to returns.plate")));
    assert_eq!(r["device"], json!(false));
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
    let r = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.bass", "rack": slug, "index": 0, "flat": true})).unwrap();
    assert!(r.get("also").is_none(), "{}", r["also"]);
    assert_eq!(kinds(&s, "tracks.bass"), ["compressor", "eq", "limiter", "compressor"]);
    assert_eq!(get(&s, "tracks.bass.effects.glue.sidechain"), json!("perc"));
    let r = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.perc", "rack": name, "flat": true})).unwrap();
    assert_eq!(r["also"], json!(["tracks.perc: the rack's compressor was keyed by perc, which this chain cannot be; set its sidechain"]));
    // On the master, no sidechain; in a Synth's patch, none either, and
    // there the effects are laid in whatever was asked, since a patch's
    // chain holds no rack.
    edit(&mut s, json!({"op": "rack.load", "owner": "master", "rack": name, "flat": true})).unwrap();
    assert!(get(&s, "master.effects.glue").get("sidechain").is_none());
    edit(&mut s, json!({"op": "synth.add", "track": "lead"})).unwrap();
    let r = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.lead.instrument.synth", "rack": name})).unwrap();
    assert_eq!(r["label"], json!(format!("Add rack {name} to tracks.lead.instrument.synth")));
    assert_eq!(kinds(&s, "tracks.lead.instrument.synth"), ["compressor", "eq", "limiter"]);
    assert!(get(&s, "tracks.lead.instrument.synth.effects.glue").get("sidechain").is_none());
    assert_eq!(r["also"][0], json!("tracks.lead.instrument.synth: the rack's effects are laid in, since a rack holds no rack"));
    assert_eq!(r["device"], json!(false));
    // Added again where the ids are taken, each is given a number.
    let r = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.bass", "rack": name, "flat": true})).unwrap();
    assert_eq!(ids(&s, "tracks.bass"), json!(["glue", null, "top", null, "glue-2", null, "top-2"]).as_array().unwrap().clone());
    assert_eq!(r["also"].as_array().unwrap().len(), 2);
    assert!(r["also"][0].as_str().unwrap().contains("glue of the rack is glue-2 here"), "{}", r["also"]);
    edit(&mut s, json!({"op": "rack.load", "owner": "tracks.bass", "rack": name, "flat": true})).unwrap();
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
    assert!(e.contains("not a track, group, return, master, synth or rack"), "{e}");
    let e = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.empty", "rack": "Nothing Here"})).unwrap_err();
    assert!(e.contains("No rack named Nothing Here"), "{e}");
    let e = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.empty", "rack": name, "index": 1, "flat": true})).unwrap_err();
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
    let r = edit(&mut other, json!({"op": "rack.load", "owner": "tracks.drums", "rack": elsewhere.to_string_lossy(), "flat": true})).unwrap();
    assert_eq!(r["label"], json!(format!("Add rack {name} to tracks.drums")));
}

fn effect_json(s: &Session, path: &str) -> Json {
    get(s, path)
}

#[test]
fn a_saved_rack_loads_as_one_device_whose_effects_are_addressed_through_it() {
    let (_d, mut s) = open();
    edit(&mut s, json!({"op": "track.add", "id": "perc"})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.drums", "type": "compressor", "id": "glue", "threshold_db": -18, "sidechain": "perc"})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.drums", "type": "utility", "id": "top", "gain_db": -0.5})).unwrap();
    let name = format!("Test Device {}", std::process::id());
    let slug = aaw_host::patches::slug(&name);
    edit(&mut s, json!({"op": "rack.save", "owner": "tracks.drums", "name": name})).unwrap();
    // Loaded, it is one rack named for the file, its id the slug, with the
    // effects inside and their ids their own; the key is judged by the chain.
    let r = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.bass", "rack": name})).unwrap();
    assert_eq!((r["device"].clone(), r["handles"].as_array().map(Vec::len)), (json!(true), None));
    assert!(r["handle"].as_str().unwrap().starts_with('@'));
    assert_eq!(kinds(&s, "tracks.bass"), ["compressor", "rack"]);
    let rack = effect_json(&s, "tracks.bass.effects.1");
    assert_eq!((rack["id"].clone(), rack["name"].clone()), (json!(slug), json!(name)));
    assert_ne!(rack["bypass"], json!(true));
    assert_eq!(kinds(&s, &format!("tracks.bass.effects.{slug}")), ["compressor", "utility"]);
    assert_eq!(get(&s, &format!("tracks.bass.effects.{slug}.effects.glue.sidechain")), json!("perc"));
    // Inside it, every path works: set, a lane by id and by index, add,
    // bypass, move and remove through the rack as the chain's owner.
    edit(&mut s, json!({"op": "set", "path": format!("tracks.bass.effects.{slug}.effects.glue.threshold_db"), "value": -24})).unwrap();
    assert_eq!(get(&s, &format!("tracks.bass.effects.{slug}.effects.0.threshold_db")), json!(-24.0));
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": format!("effects.{slug}.effects.glue.threshold_db"), "points": [{"at": 0, "value": -30}, {"at": 8, "value": -10}]})).unwrap();
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "effects.1.effects.1.gain_db", "points": [{"at": 0, "value": -3}]})).unwrap();
    let e = edit(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": format!("effects.{slug}.threshold_db"), "points": [{"at": 0, "value": -30}]})).unwrap_err();
    assert!(e.contains("a rack has no field of its own"), "{e}");
    let r = edit(&mut s, json!({"op": "effect.add", "owner": format!("tracks.bass.effects.{slug}"), "type": "utility", "gain_db": -3, "index": 0})).unwrap();
    assert_eq!(r["label"], json!(format!("Add utility to tracks.bass.effects.{slug}")));
    assert_eq!(kinds(&s, &format!("tracks.bass.effects.{slug}")), ["utility", "compressor", "utility"]);
    // The lane by index followed the effect the add pushed along; the one by id is as it was.
    let params: Vec<Json> = get(&s, "tracks.bass.automation").as_array().unwrap().iter().map(|l| l["param"].clone()).collect();
    assert_eq!(params, [json!("sends.plate.gain_db"), json!(format!("effects.{slug}.effects.glue.threshold_db")), json!("effects.1.effects.2.gain_db")]);
    assert_eq!(r["also"], json!(["tracks.bass: lane effects.1.effects.1.gain_db is now effects.1.effects.2.gain_db"]));
    edit(&mut s, json!({"op": "effect.bypass", "effect": format!("tracks.bass.effects.{slug}"), "bypass": true})).unwrap();
    assert_eq!(get(&s, "tracks.bass.effects.1.bypass"), json!(true));
    edit(&mut s, json!({"op": "effect.remove", "effect": format!("tracks.bass.effects.{slug}.effects.utility")})).unwrap_err();
    let r = edit(&mut s, json!({"op": "effect.remove", "effect": format!("tracks.bass.effects.{slug}.effects.0")})).unwrap();
    assert_eq!(r["also"], json!(["tracks.bass: lane effects.1.effects.2.gain_db is now effects.1.effects.1.gain_db"]));
    assert_eq!(kinds(&s, &format!("tracks.bass.effects.{slug}")), ["compressor", "utility"]);
    // The rack moved along its chain: a lane through it by index follows.
    let r = edit(&mut s, json!({"op": "effect.move", "effect": format!("tracks.bass.effects.{slug}"), "index": 0})).unwrap();
    assert_eq!(r["also"], json!(["tracks.bass: lane effects.1.effects.1.gain_db is now effects.0.effects.1.gain_db"]));
    assert_eq!(kinds(&s, "tracks.bass"), ["rack", "compressor"]);
    // A rack holds no rack, and a patch's chain holds none.
    let e = edit(&mut s, json!({"op": "effect.add", "owner": format!("tracks.bass.effects.{slug}"), "type": "rack"})).unwrap_err();
    assert!(e.contains("A rack holds no rack"), "{e}");
    let r = edit(&mut s, json!({"op": "rack.load", "owner": format!("tracks.bass.effects.{slug}"), "rack": name})).unwrap();
    assert_eq!(r["device"], json!(false));
    assert_eq!(kinds(&s, &format!("tracks.bass.effects.{slug}")), ["compressor", "utility", "compressor", "utility"]);
    assert_eq!(ids(&s, &format!("tracks.bass.effects.{slug}")), [json!("glue"), json!("top"), json!("glue-2"), json!("top-2")]);
    edit(&mut s, json!({"op": "synth.add", "track": "lead"})).unwrap();
    let e = edit(&mut s, json!({"op": "effect.add", "owner": "tracks.lead.instrument.synth", "type": "rack"})).unwrap_err();
    assert!(e.contains("not offered inside a patch"), "{e}");
    let e = edit(&mut s, json!({"op": "set", "path": "tracks.lead.instrument.synth.effects", "value": [{"type": "rack"}]})).unwrap_err();
    assert!(e.contains("not offered inside a patch"), "{e}");
    let e = edit(&mut s, json!({"op": "set", "path": "tracks.bass.effects.0.effects", "value": [{"type": "rack"}]})).unwrap_err();
    assert!(e.contains("holds no rack"), "{e}");
    // Saved from the rack, the file holds its effects; saved from the chain, the rack is laid out.
    let inner = format!("Test Inner {}", std::process::id());
    let r = edit(&mut s, json!({"op": "rack.save", "owner": format!("tracks.bass.effects.{slug}"), "name": inner})).unwrap();
    assert_eq!(r["effects"], json!(["compressor", "utility", "compressor", "utility"]));
    let outer = format!("Test Outer {}", std::process::id());
    let r = edit(&mut s, json!({"op": "rack.save", "owner": "tracks.bass", "name": outer})).unwrap();
    assert_eq!(r["effects"], json!(["compressor", "utility", "compressor", "utility", "compressor"]));
    let saved = racks::find(&outer).unwrap();
    let Value::List(items) = &saved.effects else { panic!() };
    assert!(items.iter().take(4).all(|e| matches!(e.get("bypass"), Some(Value::Bool(true)))), "the rack was bypassed, so its effects are saved bypassed");
    // Undo takes a device load back whole.
    let (_d2, mut other) = open();
    edit(&mut other, json!({"op": "rack.load", "owner": "returns.plate", "rack": name})).unwrap();
    assert_eq!(kinds(&other, "returns.plate"), ["reverb", "rack"]);
    assert!(get(&other, "returns.plate.effects.1.effects.0").get("sidechain").is_none());
    other.undo(Origin::User, false).unwrap();
    assert_eq!(kinds(&other, "returns.plate"), ["reverb"]);
}

#[test]
fn effects_are_wrapped_in_a_rack_and_unwrapped_with_their_lanes() {
    let (_d, mut s) = open();
    // The bass: a compressor (0, keyed), then a filter, an eq and a limiter.
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "filter", "id": "hp", "mode": "highpass", "cutoff_hz": 60})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "eq", "bands": [{"shape": "bell", "freq_hz": 800}]})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "utility", "id": "top"})).unwrap();
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "effects.hp.cutoff_hz", "points": [{"at": 0, "value": 40}, {"at": 8, "value": 200}]})).unwrap();
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "effects.2.bands.0.gain_db", "points": [{"at": 0, "value": -3}]})).unwrap();
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "effects.3.gain_db", "points": [{"at": 0, "value": -2}]})).unwrap();
    // The filter and the eq wrapped: the rack sits where the filter was,
    // the utility moves up, and every lane follows.
    let r = edit(&mut s, json!({"op": "rack.wrap", "effects": ["tracks.bass.effects.2", "tracks.bass.effects.hp"], "name": "Shape", "id": "shape"})).unwrap();
    assert_eq!(r["label"], json!("Wrap 2 effects of tracks.bass in a rack"));
    assert_eq!(r["rack"], json!("tracks.bass.effects.shape"));
    assert_eq!(kinds(&s, "tracks.bass"), ["compressor", "rack", "utility"]);
    assert_eq!(kinds(&s, "tracks.bass.effects.shape"), ["filter", "eq"]);
    assert_eq!(get(&s, "tracks.bass.effects.shape.name"), json!("Shape"));
    let params = |s: &Session| -> Vec<Json> { get(s, "tracks.bass.automation").as_array().unwrap().iter().map(|l| l["param"].clone()).collect() };
    assert_eq!(params(&s), [json!("sends.plate.gain_db"), json!("effects.2.gain_db"), json!("effects.shape.effects.hp.cutoff_hz"), json!("effects.shape.effects.1.bands.0.gain_db")]);
    assert_eq!(get(&s, "tracks.bass.automation.3.points.0.value"), json!(-3.0));
    // A rack in a rack is refused, as is wrapping across chains.
    let e = edit(&mut s, json!({"op": "rack.wrap", "effects": ["tracks.bass.effects.shape", "tracks.bass.effects.0"]})).unwrap_err();
    assert!(e.contains("A rack holds no rack"), "{e}");
    let e = edit(&mut s, json!({"op": "rack.wrap", "effects": ["tracks.bass.effects.0", "returns.plate.effects.0"]})).unwrap_err();
    assert!(e.contains("one chain"), "{e}");
    let e = edit(&mut s, json!({"op": "rack.wrap", "effects": ["tracks.bass.effects.shape.effects.0"]})).unwrap_err();
    assert!(e.contains("is a rack already"), "{e}");
    // Bypassed and unwrapped: the effects come out where it was, bypassed,
    // an id the chain has numbered, and the lanes follow once more.
    edit(&mut s, json!({"op": "effect.bypass", "effect": "tracks.bass.effects.shape", "bypass": true})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "utility", "id": "hp"})).unwrap();
    let r = edit(&mut s, json!({"op": "rack.unwrap", "effect": "tracks.bass.effects.shape"})).unwrap();
    assert_eq!(r["label"], json!("Unwrap rack tracks.bass.effects.shape on tracks.bass"));
    assert_eq!(kinds(&s, "tracks.bass"), ["compressor", "filter", "eq", "utility", "utility"]);
    assert_eq!(ids(&s, "tracks.bass"), [json!(null), json!("hp-2"), json!(null), json!("top"), json!("hp")]);
    assert_eq!((get(&s, "tracks.bass.effects.1.bypass"), get(&s, "tracks.bass.effects.2.bypass")), (json!(true), json!(true)));
    assert_eq!(params(&s), [json!("sends.plate.gain_db"), json!("effects.3.gain_db"), json!("effects.hp-2.cutoff_hz"), json!("effects.2.bands.0.gain_db")]);
    assert!(r["also"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("hp of the rack is hp-2 here")), "{}", r["also"]);
    // Both are one undo step each.
    s.undo(Origin::User, false).unwrap();
    assert_eq!(kinds(&s, "tracks.bass"), ["compressor", "rack", "utility", "utility"]);
    assert_eq!(params(&s)[2], json!("effects.shape.effects.hp.cutoff_hz"));
}

#[test]
fn an_effect_moves_into_a_rack_and_out_again_with_its_lanes() {
    let (_d, mut s) = open();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "rack", "id": "fx", "name": "FX"})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "saturation", "id": "warm", "drive_db": 6})).unwrap();
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "effects.warm.drive_db", "points": [{"at": 0, "value": 3}]})).unwrap();
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "effects.0.threshold_db", "points": [{"at": 0, "value": -20}]})).unwrap();
    assert_eq!(kinds(&s, "tracks.bass"), ["compressor", "rack", "saturation"]);
    // Into the rack: its lane goes through the rack, the chain's others stay.
    let r = edit(&mut s, json!({"op": "effect.move", "effect": "tracks.bass.effects.warm", "index": 0, "owner": "tracks.bass.effects.fx"})).unwrap();
    assert_eq!(r["label"], json!("Move effect tracks.bass.effects.fx.effects.warm into tracks.bass.effects.fx"));
    assert_eq!(kinds(&s, "tracks.bass"), ["compressor", "rack"]);
    assert_eq!(kinds(&s, "tracks.bass.effects.fx"), ["saturation"]);
    let params = |s: &Session| -> Vec<Json> { get(s, "tracks.bass.automation").as_array().unwrap().iter().map(|l| l["param"].clone()).collect() };
    assert_eq!(params(&s), [json!("sends.plate.gain_db"), json!("effects.0.threshold_db"), json!("effects.fx.effects.warm.drive_db")]);
    // The compressor too, by index, first in the rack; then out again to the chain's end.
    edit(&mut s, json!({"op": "effect.move", "effect": "tracks.bass.effects.0", "index": 0, "owner": "tracks.bass.effects.fx"})).unwrap();
    assert_eq!(kinds(&s, "tracks.bass.effects.fx"), ["compressor", "saturation"]);
    assert_eq!(params(&s), [json!("sends.plate.gain_db"), json!("effects.fx.effects.warm.drive_db"), json!("effects.fx.effects.0.threshold_db")]);
    let r = edit(&mut s, json!({"op": "effect.move", "effect": "tracks.bass.effects.fx.effects.warm", "index": 9, "owner": "tracks.bass"})).unwrap();
    assert_eq!(r["label"], json!("Move effect tracks.bass.effects.warm into tracks.bass"));
    assert_eq!(kinds(&s, "tracks.bass"), ["rack", "saturation"]);
    assert_eq!(params(&s), [json!("sends.plate.gain_db"), json!("effects.fx.effects.0.threshold_db"), json!("effects.warm.drive_db")]);
    // Across rows it is refused; within a chain the owner may be given.
    let e = edit(&mut s, json!({"op": "effect.move", "effect": "tracks.bass.effects.warm", "index": 0, "owner": "returns.plate"})).unwrap_err();
    assert!(e.contains("within its row"), "{e}");
    edit(&mut s, json!({"op": "effect.move", "effect": "tracks.bass.effects.warm", "index": 0, "owner": "tracks.bass"})).unwrap();
    assert_eq!(kinds(&s, "tracks.bass"), ["saturation", "rack"]);
    s.undo(Origin::User, false).unwrap();
    s.undo(Origin::User, false).unwrap();
    assert_eq!(kinds(&s, "tracks.bass.effects.fx"), ["compressor", "saturation"]);
}

#[test]
fn an_instrument_rack_carries_a_sampler_with_its_files_and_a_chain_into_another_song() {
    let (d, mut s) = open();
    edit(&mut s, json!({"op": "track.add", "id": "keys", "type": "midi"})).unwrap();
    edit(&mut s, json!({"op": "instrument.set", "track": "keys", "instrument": {"sampler": {"pads": {"t": {"sample": "tone", "release_ms": 40}}, "map": [{"notes": [0, 127], "pad": "t", "pitched": true}]}}})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.keys", "type": "saturation", "id": "warm", "drive_db": 6})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.keys", "type": "rack", "name": "Space", "effects": [{"type": "reverb", "mix_percent": 20}]})).unwrap();
    let name = format!("Test Keys {}", std::process::id());
    let slug = aaw_host::patches::slug(&name);
    let r = edit(&mut s, json!({"op": "instrument.rack.save", "track": "keys", "name": name, "tags": ["keys"]})).unwrap();
    assert_eq!((r["changed"].clone(), r["instrument"].clone(), r["effects"].clone()), (json!(false), json!("sampler"), json!(["saturation", "reverb"])));
    let file = aaw_host::instruments::dir().join(format!("{slug}.yaml"));
    assert_eq!(r["file"], json!(file));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("instrument:
  sampler:") && text.contains("release_ms: 40") && text.contains("samples:
  tone:
    file: "), "{text}");
    assert!(text.contains(&d.path().join("tone.wav").to_string_lossy().to_string()), "{text}");
    let saved = aaw_host::instruments::find(&name).unwrap();
    assert_eq!((saved.kind(), saved.tags.as_slice()), ("sampler", &["keys".to_string()][..]));
    assert!(aaw_host::instruments::matches(&saved, "sampler reverb keys"));
    let e = edit(&mut s, json!({"op": "instrument.rack.save", "track": "drums", "name": "X"})).unwrap_err();
    assert!(e.contains("not a MIDI track"), "{e}");
    let e = edit(&mut s, json!({"op": "instrument.rack.save", "track": "keys", "name": name})).unwrap_err();
    assert!(e.contains("already saved"), "{e}");
    // Loaded into another song: the file is brought in as a sample of the
    // song, the instrument plays it, and the chain is one rack first.
    let d2 = tempfile::tempdir().unwrap();
    let path = write_song(d2.path());
    let mut other = Session::open(&path, true).unwrap();
    edit(&mut other, json!({"op": "track.add", "id": "lead", "type": "midi"})).unwrap();
    edit(&mut other, json!({"op": "effect.add", "owner": "tracks.lead", "type": "limiter"})).unwrap();
    let r = edit(&mut other, json!({"op": "instrument.rack.load", "track": "lead", "rack": name})).unwrap();
    assert_eq!(r["label"], json!(format!("Load instrument rack {name} into lead")));
    assert_eq!((r["instrument"].clone(), r["effects"].clone()), (json!("sampler"), json!(["saturation", "reverb"])));
    // The song had tone.wav as its own sample, the same file by its hash,
    // so it is used as it is.
    assert_eq!(get(&other, "tracks.lead.instrument.sampler.pads.t.sample"), json!("tone"));
    assert_eq!(kinds(&other, "tracks.lead"), ["rack", "limiter"]);
    assert_eq!(get(&other, "tracks.lead.effects.0.name"), json!(name));
    assert_eq!(kinds(&other, &format!("tracks.lead.effects.{slug}")), ["saturation", "reverb"]);
    assert!(r["also"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("the song has the file")), "{}", r["also"]);
    other.undo(Origin::User, false).unwrap();
    assert_eq!(kinds(&other, "tracks.lead"), ["limiter"]);
    assert_eq!(get(&other, "tracks.lead.instrument"), json!(null));
    // Into a song without the file: it is copied under samples/ and listed.
    let d3 = tempfile::tempdir().unwrap();
    let path = write_song(d3.path());
    let mut third = Session::open(&path, true).unwrap();
    edit(&mut third, json!({"op": "remove", "path": "tracks.bass"})).unwrap();
    edit(&mut third, json!({"op": "remove", "path": "samples.tone"})).unwrap();
    edit(&mut third, json!({"op": "track.add", "id": "lead", "type": "midi"})).unwrap();
    let r = edit(&mut third, json!({"op": "instrument.rack.load", "track": "lead", "rack": name})).unwrap();
    assert!(r["also"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("samples.tone: brought in from")), "{}", r["also"]);
    assert_eq!(get(&third, "samples.tone.path"), json!("samples/tone.wav"));
    assert!(d3.path().join("samples/tone.wav").is_file());
    assert_eq!(get(&third, "tracks.lead.instrument.sampler.pads.t.sample"), json!("tone"));
    let e = edit(&mut third, json!({"op": "instrument.rack.load", "track": "drums", "rack": name})).unwrap_err();
    assert!(e.contains("not a MIDI track"), "{e}");
    let e = edit(&mut third, json!({"op": "instrument.rack.load", "track": "lead", "rack": "Nothing Here"})).unwrap_err();
    assert!(e.contains("No instrument rack named"), "{e}");
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
    let r = edit(&mut s, json!({"op": "rack.load", "owner": "tracks.bass", "rack": name, "index": 0, "flat": true})).unwrap();
    assert_eq!(kinds(&s, "tracks.bass"), ["utility", "filter", "compressor"]);
    assert_eq!(r["also"], json!(["tracks.bass: lane effects.0.threshold_db is now effects.2.threshold_db"]));
    assert_eq!(get(&s, "tracks.bass.automation")[1]["param"], json!("effects.2.threshold_db"));
    s.undo(Origin::User, false).unwrap();
    assert_eq!(get(&s, "tracks.bass.automation")[1]["param"], json!("effects.0.threshold_db"));
    assert_eq!(kinds(&s, "tracks.bass"), ["compressor"]);
}
