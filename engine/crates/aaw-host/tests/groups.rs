//! Groups through the session: made of tracks moved together, renamed with
//! their tracks following, removed with their tracks going back to the
//! master, sending to returns, and kept next to each other.

mod common;

use aaw_host::session::Session;
use common::{edit, get, write_song};
use serde_json::json;

fn open() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, true).unwrap();
    (dir, s)
}

#[test]
fn a_group_is_made_of_tracks_moved_together() {
    let (_d, mut s) = open();
    edit(&mut s, json!({"op": "track.add", "id": "keys"})).unwrap();
    edit(&mut s, json!({"op": "track.add", "id": "pad"})).unwrap();
    // The drums and the pad are not next to each other; the group puts them
    // together where the first of them is, in their order.
    let r = edit(&mut s, json!({"op": "group.add", "id": "kit", "tracks": ["drums", "pad"], "gain_db": -3})).unwrap();
    assert_eq!(r["label"], json!("Add group kit of drums, pad"));
    let ids: Vec<_> = get(&s, "tracks").as_array().unwrap().iter().map(|t| t["id"].clone()).collect();
    assert_eq!(ids, [json!("drums"), json!("pad"), json!("bass"), json!("keys")]);
    assert_eq!(get(&s, "tracks.drums.group"), json!("kit"));
    assert_eq!(get(&s, "tracks.pad.group"), json!("kit"));
    assert_eq!(get(&s, "tracks.bass.group"), json!(null));
    assert_eq!(get(&s, "groups.kit.gain_db"), json!(-3.0));
    // A track's group is set on the track too, when it is beside the others;
    // one that is not is refused with the move to make.
    let e = edit(&mut s, json!({"op": "set", "path": "tracks.keys.group", "value": "kit"})).unwrap_err();
    assert!(e.contains("the tracks of group kit must be next to each other"), "{e}");
    let e = edit(&mut s, json!({"op": "set", "path": "tracks.keys.group", "value": "nope"})).unwrap_err();
    assert!(e.contains("unknown group nope"), "{e}");
    edit(&mut s, json!({"op": "set", "path": "tracks.bass.group", "value": "kit"})).unwrap();
    assert_eq!(get(&s, "tracks.bass.group"), json!("kit"));
    edit(&mut s, json!({"op": "remove", "path": "tracks.bass.group"})).unwrap();
    assert_eq!(get(&s, "tracks.bass.group"), json!(null));
}

#[test]
fn a_renamed_group_keeps_its_tracks_and_a_removed_one_frees_them() {
    let (_d, mut s) = open();
    edit(&mut s, json!({"op": "group.add", "id": "kit", "tracks": ["drums"]})).unwrap();
    edit(&mut s, json!({"op": "send.set", "track": "kit", "to": "plate", "gain_db": -10})).unwrap();
    edit(&mut s, json!({"op": "lane.set", "owner": "groups.kit", "param": "sends.plate.gain_db", "points": [{"at": 0, "value": -20}]})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "groups.kit", "type": "compressor", "threshold_db": -18})).unwrap();
    let r = edit(&mut s, json!({"op": "group.rename", "group": "kit", "to": "drumbus"})).unwrap();
    assert_eq!(r["label"], json!("Rename group kit to drumbus"));
    assert_eq!(get(&s, "tracks.drums.group"), json!("drumbus"));
    assert_eq!(get(&s, "groups.drumbus.sends.plate.gain_db"), json!(-10.0));
    // Renaming the return the group sends to follows into the group.
    edit(&mut s, json!({"op": "return.rename", "return": "plate", "to": "hall"})).unwrap();
    assert_eq!(get(&s, "groups.drumbus.sends.0.to"), json!("hall"));
    assert_eq!(get(&s, "groups.drumbus.automation.0.param"), json!("sends.hall.gain_db"));
    edit(&mut s, json!({"op": "group.move", "group": "drumbus", "index": 0})).unwrap();
    // Removing the group leaves its tracks where they are, on the master.
    let r = edit(&mut s, json!({"op": "group.remove", "group": "drumbus"})).unwrap();
    assert_eq!(r["label"], json!("Remove groups.drumbus"));
    assert_eq!(get(&s, "groups"), json!([]));
    assert_eq!(get(&s, "tracks.drums.group"), json!(null));
    assert_eq!(get(&s, "tracks.0.id"), json!("drums"));
    // So does removing the return a group sends to.
    edit(&mut s, json!({"op": "group.add", "id": "kit", "tracks": ["drums"]})).unwrap();
    edit(&mut s, json!({"op": "send.set", "track": "kit", "to": "hall", "gain_db": -10})).unwrap();
    let r = edit(&mut s, json!({"op": "return.remove", "return": "hall"})).unwrap();
    assert!(r["also"].as_array().unwrap().iter().any(|a| a == "removed groups.kit.sends.hall"), "{}", r["also"]);
    edit(&mut s, json!({"op": "send.remove", "track": "kit", "to": "nowhere"})).unwrap_err();
    // Undo takes it all back in steps.
    s.undo(aaw_host::command::Origin::Agent, false).unwrap();
    assert_eq!(get(&s, "groups.kit.sends.0.to"), json!("hall"));
}

#[test]
fn inspect_lists_the_groups_with_their_tracks() {
    let (_d, mut s) = open();
    edit(&mut s, json!({"op": "group.add", "id": "kit", "tracks": ["drums", "bass"]})).unwrap();
    let summary = s.inspect();
    assert_eq!(summary["groups"][0]["id"], json!("kit"));
    assert_eq!(summary["groups"][0]["tracks"], json!(["drums", "bass"]));
}

#[test]
fn a_group_of_groups_is_made_renamed_and_removed_down_the_tree() {
    let (_d, mut s) = open();
    edit(&mut s, json!({"op": "track.add", "id": "keys"})).unwrap();
    edit(&mut s, json!({"op": "track.add", "id": "pad"})).unwrap();
    edit(&mut s, json!({"op": "group.add", "id": "kit", "tracks": ["drums"]})).unwrap();
    edit(&mut s, json!({"op": "group.add", "id": "synths", "tracks": ["keys", "pad"]})).unwrap();
    // A group of the two groups: their tracks are moved together to where
    // the first of them is, the bass between them moving after.
    let r = edit(&mut s, json!({"op": "group.add", "id": "music", "groups": ["kit", "synths"], "gain_db": -1})).unwrap();
    assert_eq!(r["label"], json!("Add group music of kit, synths"));
    let ids: Vec<_> = get(&s, "tracks").as_array().unwrap().iter().map(|t| t["id"].clone()).collect();
    assert_eq!(ids, [json!("drums"), json!("keys"), json!("pad"), json!("bass")]);
    assert_eq!(get(&s, "groups.kit.group"), json!("music"));
    assert_eq!(get(&s, "groups.synths.group"), json!("music"));
    assert_eq!(get(&s, "groups.music.group"), json!(null));
    assert_eq!(get(&s, "tracks.drums.group"), json!("kit"));
    // A cycle, a group in itself and an unknown group are refused with the names.
    let e = edit(&mut s, json!({"op": "set", "path": "groups.music.group", "value": "kit"})).unwrap_err();
    assert!(e.contains("kit: cannot be in group music, which is in kit"), "{e}");
    let e = edit(&mut s, json!({"op": "set", "path": "groups.music.group", "value": "music"})).unwrap_err();
    assert!(e.contains("a group cannot be in itself"), "{e}");
    let e = edit(&mut s, json!({"op": "set", "path": "groups.kit.group", "value": "nope"})).unwrap_err();
    assert!(e.contains("kit: unknown group nope"), "{e}");
    let e = edit(&mut s, json!({"op": "group.add", "id": "loop", "groups": ["loop"]})).unwrap_err();
    assert!(e.contains("cannot be in itself"), "{e}");
    // A track moved into the outer group's run breaks it and is refused.
    let e = edit(&mut s, json!({"op": "track.move", "track": "bass", "index": 1})).unwrap_err();
    assert!(e.contains("the tracks of group music must be next to each other"), "{e}");
    // The inner group's run too.
    let e = edit(&mut s, json!({"op": "track.move", "track": "drums", "index": 1})).unwrap_err();
    assert!(e.contains("the tracks of group synths must be next to each other"), "{e}");
    // A renamed group carries the groups in it along.
    edit(&mut s, json!({"op": "group.rename", "group": "music", "to": "all"})).unwrap();
    assert_eq!(get(&s, "groups.kit.group"), json!("all"));
    assert_eq!(get(&s, "groups.synths.group"), json!("all"));
    // Inspect shows the tree: each group's groups beside its tracks, and its group.
    let summary = s.inspect();
    let all = summary["groups"].as_array().unwrap().iter().find(|g| g["id"] == "all").unwrap();
    assert_eq!(all["groups"], json!(["kit", "synths"]));
    assert_eq!(all["tracks"], json!([]));
    let kit = summary["groups"].as_array().unwrap().iter().find(|g| g["id"] == "kit").unwrap();
    assert_eq!((kit["group"].clone(), kit["tracks"].clone()), (json!("all"), json!(["drums"])));
    // A group taken out of its group goes to the master; a group with a
    // group of its own and a track in the same group.
    edit(&mut s, json!({"op": "remove", "path": "groups.synths.group"})).unwrap();
    assert_eq!(get(&s, "groups.synths.group"), json!(null));
    edit(&mut s, json!({"op": "set", "path": "groups.synths.group", "value": "all"})).unwrap();
    edit(&mut s, json!({"op": "set", "path": "tracks.bass.group", "value": "all"})).unwrap();
    assert_eq!(get(&s, "tracks.bass.group"), json!("all"));
    // Removing a group leaves what was in it on its own group, or the master.
    edit(&mut s, json!({"op": "group.add", "id": "everything", "groups": ["all"]})).unwrap();
    edit(&mut s, json!({"op": "group.remove", "group": "all"})).unwrap();
    assert_eq!(get(&s, "groups.kit.group"), json!("everything"));
    assert_eq!(get(&s, "tracks.bass.group"), json!("everything"));
    edit(&mut s, json!({"op": "group.remove", "group": "kit"})).unwrap();
    assert_eq!(get(&s, "tracks.drums.group"), json!("everything"));
    edit(&mut s, json!({"op": "group.remove", "group": "everything"})).unwrap();
    assert_eq!(get(&s, "groups.synths.group"), json!(null));
    assert_eq!(get(&s, "tracks.drums.group"), json!(null));
    // Undo takes the last removal back, the groups in it included.
    s.undo(aaw_host::command::Origin::Agent, false).unwrap();
    assert_eq!(get(&s, "groups.synths.group"), json!("everything"));
}
