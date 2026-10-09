//! Markers: the person's notes at beats, added, moved, reworded and removed,
//! read with what plays at each, drawn in the map and kept to their beats by
//! the range edits.

mod common;

use aaw_host::command::Origin;
use aaw_host::session::Session;
use common::{cmd, edit, get, write_song};
use serde_json::{json, Value as Json};

fn open() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let s = Session::open(&path, true).unwrap();
    (dir, s)
}

fn places(s: &Session) -> Vec<(String, Json)> {
    get(s, "markers")
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["id"].as_str().unwrap().to_string(), m["at"].clone()))
        .collect()
}

fn at(id: &str, beat: Json) -> (String, Json) {
    (id.to_string(), beat)
}

#[test]
fn markers_are_kept_in_time_order_and_read_with_what_plays_there() {
    let (_d, mut s) = open();
    let r = edit(&mut s, json!({"op": "marker.add", "at": 17.5, "text": "too busy"})).unwrap();
    assert_eq!(r["label"], json!("Add marker m1 in bar 5: too busy"));
    assert_eq!(r["marker"], json!("m1"));
    assert_eq!(r["path"], json!("markers.m1"));
    // The person's, as the app sends it: it is in the log under their name.
    let (r, change) = s.edit(&cmd(json!({"op": "marker.add", "at": 2})), Origin::User, None, None).unwrap();
    assert_eq!(r["label"], json!("Add marker m2 in bar 1"));
    assert_eq!(change.unwrap().origin, Origin::User);
    edit(&mut s, json!({"op": "marker.add", "at": 32, "id": "end"})).unwrap();
    assert_eq!(places(&s), [at("m2", json!(2)), at("m1", json!(17.5)), at("end", json!(32))]);

    let read = s.markers();
    let list = read["markers"].as_array().unwrap();
    assert_eq!(list.len(), 3);
    // Beat 2 is the third beat of bar 1, under the intro, with both tracks' first clips.
    assert_eq!(list[0]["id"], json!("m2"));
    assert_eq!((&list[0]["bar"], &list[0]["beat"], &list[0]["section"], &list[0]["text"]), (&json!(1), &json!(3.0), &json!("intro"), &json!("")));
    let playing = |m: &Json| -> Vec<(String, String)> {
        m["playing"].as_array().unwrap().iter().map(|c| (c["track"].as_str().unwrap().into(), c["what"].as_str().unwrap().into())).collect()
    };
    assert_eq!(
        playing(&list[0]),
        [("drums".to_string(), "pattern beat, 4 beats, 8 hits".to_string()), ("bass".to_string(), "pattern bass, 4 beats, 2 hits".to_string())]
    );
    // Beat 17.5 is bar 5, beat 2.5, in no section, with the clips from beat 16.
    assert_eq!((&list[1]["bar"], &list[1]["beat"], &list[1]["section"], &list[1]["text"]), (&json!(5), &json!(2.5), &Json::Null, &json!("too busy")));
    let clips: Vec<&str> = list[1]["playing"].as_array().unwrap().iter().map(|c| c["clip"].as_str().unwrap()).collect();
    assert_eq!(clips.len(), 2);
    assert!(clips.iter().all(|c| s.get(c).is_ok()), "{clips:?}");
    assert_eq!(s.get(clips[0]).unwrap()["at"], json!(16));
    // The song's end is a place, and nothing plays there.
    assert_eq!((&list[2]["bar"], &list[2]["playing"]), (&json!(9), &json!([])));
    // inspect lists them as the song holds them.
    assert_eq!(s.inspect()["markers"][1], json!({"id": "m1", "at": 17.5, "text": "too busy"}));
}

#[test]
fn a_marker_is_moved_reworded_and_removed_by_its_id_or_its_handle() {
    let (_d, mut s) = open();
    for (beat, text) in [(4, "a"), (8, "b"), (12, "c"), (16, "")] {
        edit(&mut s, json!({"op": "marker.add", "at": beat, "text": text})).unwrap();
    }
    // A move keeps the list in time order and the marker its ID.
    let r = edit(&mut s, json!({"op": "marker.move", "marker": "m1", "at": "31/3"})).unwrap();
    assert_eq!(r["label"], json!("Move marker m1 to bar 3: a"));
    assert_eq!(places(&s), [at("m2", json!(8)), at("m1", json!("31/3")), at("m3", json!(12)), at("m4", json!(16))]);
    let handle = s.get("markers").unwrap()[0]["ref"].as_str().unwrap().to_string();
    assert!(handle.starts_with('@'), "{handle}");
    let r = edit(&mut s, json!({"op": "marker.text", "marker": handle, "text": "love this"})).unwrap();
    assert_eq!(r["label"], json!("Marker m2: love this"));
    assert_eq!(get(&s, "markers.m2"), json!({"id": "m2", "at": 8, "text": "love this"}));
    let r = edit(&mut s, json!({"op": "marker.text", "marker": "markers.m2", "text": ""})).unwrap();
    assert_eq!(r["label"], json!("Clear marker m2's text"));
    // set reaches a marker as it reaches anything.
    edit(&mut s, json!({"op": "set", "path": "markers.m3.text", "value": "louder"})).unwrap();
    assert_eq!(get(&s, "markers.m3.text"), json!("louder"));

    let r = edit(&mut s, json!({"op": "marker.remove", "markers": ["m3", "m1", "m3"]})).unwrap();
    assert_eq!((&r["label"], &r["removed"]), (&json!("Remove 2 markers"), &json!(["m1", "m3"])));
    // An ID is not given again while a higher one is there.
    assert_eq!(edit(&mut s, json!({"op": "marker.add", "at": 1})).unwrap()["marker"], json!("m5"));
    // One undo brings back what one command removed.
    let r = edit(&mut s, json!({"op": "marker.remove", "all": true})).unwrap();
    assert_eq!(r["label"], json!("Remove 3 markers"));
    assert_eq!(get(&s, "markers"), json!([]));
    s.undo(Origin::User, false).unwrap();
    assert_eq!(places(&s).len(), 3);
    // Removing every marker of a song that has none changes nothing.
    edit(&mut s, json!({"op": "marker.remove", "all": true})).unwrap();
    assert_eq!(edit(&mut s, json!({"op": "marker.remove", "all": true})).unwrap()["changed"], json!(false));
}

#[test]
fn what_a_marker_command_refuses() {
    let (_d, mut s) = open();
    let refused = |s: &mut Session, j: Json| edit(s, j).unwrap_err();
    // A beat is given where no host says where the song is playing.
    assert!(refused(&mut s, json!({"op": "marker.add", "text": "here"})).contains("needs a beat"));
    assert!(refused(&mut s, json!({"op": "marker.move", "marker": "m1", "at": 4})).contains("m1 is not a marker; the song has none"));
    edit(&mut s, json!({"op": "marker.add", "at": 4})).unwrap();
    assert!(refused(&mut s, json!({"op": "marker.text", "marker": "m9", "text": "x"})).contains("m9 is not a marker; the markers are m1"));
    assert!(refused(&mut s, json!({"op": "marker.text", "marker": "tracks.drums", "text": "x"})).contains("is not a marker"));
    assert!(refused(&mut s, json!({"op": "marker.add", "at": 33})).contains("Marker m2 is past the session's end"));
    assert!(refused(&mut s, json!({"op": "marker.move", "marker": "m1", "at": 40})).contains("past the session's end"));
    assert!(refused(&mut s, json!({"op": "marker.add", "at": 5, "id": "m1"})).contains("Two markers have the ID m1"));
    assert!(refused(&mut s, json!({"op": "marker.add", "at": 5, "text": "x".repeat(201)})).contains("at most 200 characters"));
    assert!(refused(&mut s, json!({"op": "marker.remove"})).contains("Name the markers"));
    // A song cannot be made shorter than its last marker.
    assert!(refused(&mut s, json!({"op": "marker.add", "at": "x"})).contains("is not a beat"));
    assert_eq!(places(&s), [at("m1", json!(4))]);
}

#[test]
fn markers_keep_to_their_beats_when_time_is_opened_closed_or_moved() {
    let (_d, mut s) = open();
    for beat in [4, 8, 12, 20, 32] {
        edit(&mut s, json!({"op": "marker.add", "at": beat})).unwrap();
    }
    let beats = |s: &Session| -> Vec<Json> { places(s).into_iter().map(|(_, at)| at).collect() };
    // Four beats opened at beat 8: the marker on beat 8 moves with what was there.
    edit(&mut s, json!({"op": "range.insert", "at": 8, "length": 4})).unwrap();
    assert_eq!(beats(&s), [json!(4), json!(12), json!(16), json!(24), json!(36)]);
    // Deleted beats take their markers with them, and say so; the rest close up.
    let r = edit(&mut s, json!({"op": "range.delete", "start": 12, "length": 8})).unwrap();
    assert!(r["removed"].as_array().unwrap().contains(&json!("markers.m2")) && r["removed"].as_array().unwrap().contains(&json!("markers.m3")), "{r}");
    assert_eq!(places(&s), [at("m1", json!(4)), at("m4", json!(16)), at("m5", json!(28))]);
    // A copy leaves them, unless it opens time for itself; a clear leaves them.
    edit(&mut s, json!({"op": "range.copy", "start": 0, "length": 8, "to": 16})).unwrap();
    edit(&mut s, json!({"op": "range.clear", "start": 0, "length": 8})).unwrap();
    assert_eq!(beats(&s), [json!(4), json!(16), json!(28)]);
    edit(&mut s, json!({"op": "range.copy", "start": 16, "length": 4, "to": 16, "insert": true})).unwrap();
    assert_eq!(beats(&s), [json!(4), json!(20), json!(32)]);
    // A range of some tracks moves no marker: the song's time did not change.
    edit(&mut s, json!({"op": "range.insert", "at": 0, "length": 4, "tracks": ["drums"]})).unwrap();
    assert_eq!(beats(&s), [json!(4), json!(20), json!(32)]);
    // A section moved with its content takes the markers under it along,
    // and the list stays in time order.
    edit(&mut s, json!({"op": "section.add", "id": "verse", "at": 0, "length_beats": 8})).unwrap();
    edit(&mut s, json!({"op": "section.move", "section": "verse", "at": 24, "with_content": true})).unwrap();
    assert_eq!(places(&s), [at("m4", json!(20)), at("m1", json!(28)), at("m5", json!(32))]);
    // And one duplicated pushes those after it later.
    edit(&mut s, json!({"op": "section.duplicate", "section": "verse"})).unwrap();
    assert_eq!(places(&s), [at("m4", json!(20)), at("m1", json!(28)), at("m5", json!(40))]);
}

#[test]
fn the_map_has_a_row_of_markers_and_a_line_for_each() {
    let (_d, mut s) = open();
    let map = |s: &Session, per: Option<Json>, from: Option<Json>, to: Option<Json>| -> Vec<String> {
        let m = s.map(per.as_ref(), from.as_ref(), to.as_ref(), &[], false).unwrap();
        m["map"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect()
    };
    // A song without markers has no row for them.
    assert!(!map(&s, None, None, None).iter().any(|l| l.starts_with("markers")));
    edit(&mut s, json!({"op": "marker.add", "at": 5.5, "text": "too busy"})).unwrap();
    edit(&mut s, json!({"op": "marker.add", "at": 6})).unwrap();
    edit(&mut s, json!({"op": "marker.add", "at": 16, "text": "love this"})).unwrap();
    edit(&mut s, json!({"op": "marker.add", "at": 32})).unwrap();
    let lines = map(&s, None, None, None);
    let row = lines.iter().find(|l| l.starts_with("markers")).expect("a row of markers");
    // Two in bar 2, one on bar 5, and the one at the song's end in its last bar.
    assert_eq!(row.trim_start_matches("markers").trim_start(), "2  !  !", "{lines:#?}");
    let bar = lines.iter().find(|l| l.starts_with("bar")).unwrap();
    assert_eq!(bar.find('1'), row.find('2').map(|i| i - 1), "the cells line up under the bars");
    let notes: Vec<&String> = lines.iter().filter(|l| l.starts_with("!  ")).collect();
    assert_eq!(notes, ["!  m1 at bar 2, beat 2.5: too busy", "!  m2 at bar 2, beat 3", "!  m3 at bar 5: love this", "!  m4 at bar 9"]);
    // A part of the song shows the markers in it.
    let part = map(&s, Some(json!("beat")), Some(json!(4)), Some(json!(8)));
    assert_eq!(part.iter().find(|l| l.starts_with("markers")).unwrap().trim_start_matches("markers").trim_start(), "!!");
    assert_eq!(part.iter().filter(|l| l.starts_with("!  ")).count(), 2);
}
