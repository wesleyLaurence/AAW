//! Recording as the host governs it without a device: a track's input in
//! the song, the arm, what Record refuses, and the state the status and the
//! transport events carry. The takes themselves need an output, which
//! tests/test_host.py opens on a virtual device.

mod common;

use aaw_host::client::Request;
use aaw_host::command::Origin;
use aaw_host::host::{self, Event, Options};
use common::{cmd, write_song};
use serde_json::{json, Value as Json};
use std::sync::OnceLock;

fn registry_dir() {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::Builder::new().prefix("aaw").tempdir_in("/tmp").unwrap();
        // SAFETY: set once, before any test reads it; every test calls this first.
        unsafe { std::env::set_var("AAW_HOST_DIR", dir.path()) };
        dir
    });
}

#[test]
fn a_track_is_armed_through_its_input_and_record_refuses_what_it_cannot_take() {
    registry_dir();
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let (tx, events) = std::sync::mpsc::channel();
    let running = host::spawn(
        &path,
        Options {
            audio: host::Audio::default(),
            play: None,
            exit_on_stop: false,
            seconds: None,
            feed: false,
            prepare: false,
            pace: host::PACE,
        },
        move |e| {
            let _ = tx.send(e);
        },
    )
    .unwrap();
    let ask = |j: Json| running.request(Request::new(cmd(j), Origin::User));
    let armed_events = |events: &std::sync::mpsc::Receiver<Event>| -> Vec<Vec<String>> {
        let mut seen = Vec::new();
        while let Ok(e) = events.recv_timeout(std::time::Duration::from_millis(300)) {
            if let Event::Transport(t) = e {
                seen.push(t.armed);
            }
        }
        seen
    };
    assert_eq!(armed_events(&events), vec![Vec::<String>::new()]);

    // Nothing records without an input, and a MIDI track never does.
    let e = ask(json!({"op": "arm", "track": "drums", "armed": true})).unwrap_err();
    assert!(e.contains("drums has no input") && e.contains("tracks.drums.input"), "{e}");
    assert!(ask(json!({"op": "arm", "track": "nobody", "armed": true})).unwrap_err().contains("No track nobody"));
    ask(json!({"op": "track.add", "id": "keys", "type": "midi"})).unwrap();
    assert!(ask(json!({"op": "arm", "track": "keys", "armed": true})).unwrap_err().contains("MIDI track"));
    let e = ask(json!({"op": "set", "path": "tracks.keys.input", "value": {"master": true}})).unwrap_err();
    assert!(e.contains("not a field"), "{e}");
    let e = ask(json!({"op": "record.start"})).unwrap_err();
    assert!(e.contains("No track is armed"), "{e}");
    assert!(ask(json!({"op": "record.stop"})).unwrap_err().contains("Nothing is recording"));

    // The input is the song's: an edit, validated with the rest.
    let e = ask(json!({"op": "set", "path": "tracks.drums.input", "value": {"track": "drums"}})).unwrap_err();
    assert!(e.contains("cannot record itself"), "{e}");
    let e = ask(json!({"op": "set", "path": "tracks.drums.input", "value": {"track": "nobody"}})).unwrap_err();
    assert!(e.contains("unknown track nobody"), "{e}");
    let e = ask(json!({"op": "set", "path": "tracks.drums.input", "value": {"device": "Scarlett", "track": "bass"}})).unwrap_err();
    assert!(e.contains("one of device, track, group or master"), "{e}");
    // Channels alone are the chosen device's.
    ask(json!({"op": "set", "path": "tracks.drums.input", "value": {"channels": [1, 2]}})).unwrap();
    let got = ask(json!({"op": "get", "path": "tracks.drums.input"})).unwrap();
    assert_eq!((got["device"].clone(), got["channels"].clone()), (json!("default"), json!([1, 2])));
    let e = ask(json!({"op": "set", "path": "tracks.drums.input", "value": {"track": "bass", "channels": [1]}})).unwrap_err();
    assert!(e.contains("channels go with a device"), "{e}");
    let e = ask(json!({"op": "set", "path": "tracks.drums.input", "value": {"device": "Scarlett", "channels": [2, 2]}})).unwrap_err();
    assert!(e.contains("two different channels"), "{e}");
    let reply = ask(json!({"op": "set", "path": "tracks.drums.input", "value": {"track": "bass"}})).unwrap();
    assert!(reply["label"].as_str().unwrap().ends_with("→ {\"track\":\"bass\"}"), "{}", reply["label"]);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("input: {track: bass}") || saved.contains("input:\n    track: bass"), "{saved}");
    // `{}` is the chosen input device's first channel, and saves as such.
    ask(json!({"op": "set", "path": "tracks.bass.input", "value": {}})).unwrap();
    let got = ask(json!({"op": "get", "path": "tracks.bass.input"})).unwrap();
    assert_eq!(got["device"], json!("default"));
    assert_eq!(got["channels"], Json::Null);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("input: {device: default}"), "{saved}");

    // Armed, by the host alone: the song and its revision stand.
    let revision = ask(json!({"op": "status"})).unwrap()["revision"].clone();
    let status = ask(json!({"op": "arm", "track": "drums", "armed": true})).unwrap();
    assert_eq!(status["armed"], json!(["drums"]));
    assert_eq!(status["recording"], Json::Null);
    assert_eq!(status["revision"], revision);
    ask(json!({"op": "arm", "track": "drums", "armed": true})).unwrap();
    let status = ask(json!({"op": "arm", "track": "bass", "armed": true})).unwrap();
    assert_eq!(status["armed"], json!(["drums", "bass"]));
    let status = ask(json!({"op": "arm", "track": "drums", "armed": false})).unwrap();
    assert_eq!(status["armed"], json!(["bass"]));
    let seen = armed_events(&events);
    assert!(seen.contains(&vec!["drums".to_string(), "bass".to_string()]) && seen.last() == Some(&vec!["bass".to_string()]), "{seen:?}");
    // An input taken off disarms nothing by itself, but the track records
    // nothing at the next start.
    ask(json!({"op": "remove", "path": "tracks.bass.input"})).unwrap();
    let e = ask(json!({"op": "record.start"})).unwrap_err();
    assert!(e.contains("No track is armed"), "{e}");
    assert_eq!(ask(json!({"op": "status"})).unwrap()["armed"], json!([]));
    // A muted source has nothing to record, said before any output opens.
    ask(json!({"op": "set", "path": "tracks.drums.input", "value": {"track": "bass"}})).unwrap();
    ask(json!({"op": "set", "path": "tracks.bass.mute", "value": true})).unwrap();
    let e = ask(json!({"op": "record.start", "tracks": ["drums"]})).unwrap_err();
    assert!(e.contains("bass is muted"), "{e}");
    running.close().unwrap();
}
