//! Markers: the notes a person leaves at beats while listening.

use aaw_model::{parse, project_hash, to_yaml};

const SONG: &str = "session: {tempo: 120, length_beats: 32}\n";

fn with(markers: &str) -> String {
    format!("{SONG}markers: {markers}\n")
}

#[test]
fn a_marker_is_a_beat_and_a_few_words_with_an_id_given_where_left_out() {
    let p = parse(&with("[{at: 8, text: too busy}, {id: m7, at: 17/2}, {at: 30.5, text: ''}]")).unwrap();
    let read: Vec<(&str, String, &str)> = p.markers.iter().map(|m| (m.id.as_str(), m.at_exact().to_string(), m.text.as_str())).collect();
    // The next free mN after the highest there is.
    assert_eq!(read, [("m8", "8".into(), "too busy"), ("m7", "17/2".into(), ""), ("m9", "61/2".into(), "")]);
    // The saved form keeps the IDs and leaves out an empty text.
    let text = to_yaml(&p);
    assert!(text.ends_with("markers:\n- id: m8\n  at: 8\n  text: too busy\n- id: m7\n  at: 17/2\n- id: m9\n  at: 30.5\n"), "{text}");
    let again = parse(&text).unwrap();
    assert_eq!(to_yaml(&again), text);
    assert_eq!(project_hash(&again), project_hash(&p));
}

#[test]
fn a_song_without_markers_is_written_and_fingerprinted_as_before() {
    let p = parse(SONG).unwrap();
    assert!(p.markers.is_empty());
    assert!(!to_yaml(&p).contains("markers"));
    assert_eq!(project_hash(&p), project_hash(&parse(&with("[]")).unwrap()));
    // A marker is part of the song: it changes the fingerprint.
    assert_ne!(project_hash(&p), project_hash(&parse(&with("[{at: 0}]")).unwrap()));
}

#[test]
fn what_a_marker_cannot_be() {
    let refused = |markers: &str| parse(&with(markers)).unwrap_err().to_string();
    assert!(refused("[{text: nowhere}]").contains("markers.0.at"), "{}", refused("[{text: nowhere}]"));
    assert!(refused("[{id: a, at: 1}, {id: a, at: 2}]").contains("Two markers have the ID a"));
    assert!(refused("[{at: 33}]").contains("Marker m1 is past the session's end"));
    assert!(refused("[{at: -1}]").contains("markers.0.at"));
    assert!(refused("[{id: 9lives, at: 1}]").contains("markers.0.id"));
    assert!(refused("[{at: 1, note: x}]").contains("markers.0.note"));
    // At the song's end is in the song; a text is at most 200 characters.
    parse(&with("[{at: 32}]")).unwrap();
    let long = "x".repeat(201);
    assert!(refused(&format!("[{{at: 1, text: {long}}}]")).contains("at most 200 characters"));
    parse(&with(&format!("[{{at: 1, text: {}}}]", "x".repeat(200)))).unwrap();
}
