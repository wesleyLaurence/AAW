//! Colors of tracks, groups, returns and clips: set by path or by
//! `clip.color`, kept in the file, refused when not one of the palette,
//! and reported by inspect.

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
fn a_row_and_a_clip_keep_their_colors_in_the_file() {
    let (dir, mut s) = open();
    edit(&mut s, json!({"op": "set", "path": "tracks.drums.color", "value": "red"})).unwrap();
    let r = edit(&mut s, json!({"op": "clip.color", "clip": "tracks.drums.clips.0", "color": "white"})).unwrap();
    assert_eq!(r["label"], json!("Color clip beat at 0 white"));
    assert_eq!(get(&s, "tracks.drums.color"), json!("red"));
    assert_eq!(get(&s, "tracks.drums.clips.0")["color"], json!("white"));
    // The other clip has none of its own and takes its track's.
    assert!(get(&s, "tracks.drums.clips.1").get("color").is_none());

    s.save().unwrap();
    let saved = std::fs::read_to_string(dir.path().join("song.yaml")).unwrap();
    assert!(saved.contains("color: red") && saved.contains("color: white"), "{saved}");
    let reopened = Session::open(&dir.path().join("song.yaml"), true).unwrap();
    assert_eq!(reopened.project().tracks[0].color.as_deref(), Some("red"));
    assert_eq!(reopened.project().tracks[0].midi.as_ref().unwrap().clips[0].color.as_deref(), Some("white"));

    // Taken away, the clip takes its track's again and the field is gone.
    let r = edit(&mut s, json!({"op": "clip.color", "clip": "tracks.drums.clips.0", "color": null})).unwrap();
    assert_eq!(r["label"], json!("Clear the color of clip beat at 0"));
    assert!(get(&s, "tracks.drums.clips.0").get("color").is_none());

    // Undo puts it back.
    s.undo(aaw_host::command::Origin::User, false).unwrap();
    assert_eq!(get(&s, "tracks.drums.clips.0")["color"], json!("white"));
}

#[test]
fn a_color_not_in_the_palette_is_refused() {
    let (_dir, mut s) = open();
    let e = edit(&mut s, json!({"op": "clip.color", "clip": "tracks.drums.clips.0", "color": "beige"})).unwrap_err();
    assert!(e.contains("No color \"beige\"") && e.contains("white"), "{e}");
    let e = edit(&mut s, json!({"op": "set", "path": "tracks.drums.color", "value": "#ffffff"})).unwrap_err();
    assert!(e.contains("No color"), "{e}");
    assert!(edit(&mut s, json!({"op": "clip.color", "clip": "tracks.drums", "color": "red"})).is_err());
}

#[test]
fn groups_returns_and_audio_clips_take_colors() {
    let (dir, mut s) = open();
    let song: Vec<[f32; 2]> = vec![[0.1; 2]; 48000];
    std::fs::write(dir.path().join("song.wav"), aaw_engine::wav::float_wav_bytes(&song, 48000)).unwrap();
    edit(&mut s, json!({"op": "set", "path": "samples.song", "value": {"path": "song.wav"}})).unwrap();
    edit(&mut s, json!({"op": "track.add", "type": "audio", "id": "song"})).unwrap();
    edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "song", "at": 4})).unwrap();
    edit(&mut s, json!({"op": "clip.color", "clip": "tracks.song.audio.0", "color": "sky"})).unwrap();
    assert_eq!(s.project().tracks[2].audio[0].color.as_deref(), Some("sky"));

    edit(&mut s, json!({"op": "return.add", "id": "verb"})).unwrap();
    edit(&mut s, json!({"op": "set", "path": "returns.verb.color", "value": "slate"})).unwrap();
    assert_eq!(s.project().returns[1].color.as_deref(), Some("slate"));

    edit(&mut s, json!({"op": "group.add", "id": "band", "tracks": ["drums", "bass"]})).unwrap();
    edit(&mut s, json!({"op": "set", "path": "groups.band.color", "value": "olive"})).unwrap();
    assert_eq!(s.project().groups[0].color.as_deref(), Some("olive"));

    // inspect gives the colors after the IDs and on the clips.
    let text = s.inspect().to_string();
    assert!(text.contains("\"color\":\"sky\"") && text.contains("\"color\":\"slate\""), "{text}");
}
