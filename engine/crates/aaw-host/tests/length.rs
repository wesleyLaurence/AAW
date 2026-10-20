//! The song's length is its content: the next whole bar after the last
//! sound, settled by the host after every edit (D130).

mod common;

use aaw_host::session::Session;
use common::{edit, get, write_song};
use serde_json::json;

fn open(song: &str) -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    std::fs::write(&path, song).unwrap();
    let s = Session::open(&path, true).unwrap();
    (dir, s)
}

fn length(s: &Session) -> f64 {
    get(s, "session.length_beats").as_f64().unwrap()
}

#[test]
fn the_end_follows_the_last_clip_and_an_empty_song_is_one_bar() {
    // A song saved longer than its content opens as it was, and settles at
    // its first edit: content to beat 32 reads 32 after it.
    let (_d, mut s) = open(&common::SONG.replace("length_beats: 32", "length_beats: 128"));
    assert_eq!(length(&s), 128.0);
    edit(&mut s, json!({"op": "set", "path": "session.master_gain_db", "value": -3})).unwrap();
    assert_eq!(length(&s), 32.0);
    // A MIDI clip at bar 40 of a 32-beat song makes it 41 bars, or the
    // clip's end, whichever is later; taking it away shrinks the song back.
    edit(&mut s, json!({"op": "track.add", "id": "keys", "type": "midi"})).unwrap();
    edit(&mut s, json!({"op": "clip.add", "track": "keys", "id": "late", "at": 156, "length_beats": 2})).unwrap();
    assert_eq!(length(&s), 160.0);
    edit(&mut s, json!({"op": "clip.resize", "clip": "tracks.keys.clips.late", "length_beats": 6})).unwrap();
    assert_eq!(length(&s), 164.0);
    edit(&mut s, json!({"op": "clip.remove", "clip": "tracks.keys.clips.late"})).unwrap();
    assert_eq!(length(&s), 32.0);
    // Everything taken away leaves one bar, of the time signature.
    for clip in ["tracks.drums.clips.1", "tracks.drums.clips.0", "tracks.bass.clips.2", "tracks.bass.clips.1", "tracks.bass.clips.0"] {
        edit(&mut s, json!({"op": "clip.remove", "clip": clip})).unwrap();
    }
    edit(&mut s, json!({"op": "remove", "path": "tracks.bass.automation.0"})).unwrap();
    edit(&mut s, json!({"op": "section.remove", "section": "intro"})).unwrap();
    assert_eq!(length(&s), 4.0);
    edit(&mut s, json!({"op": "set", "path": "session.time_signature", "value": "3/4"})).unwrap();
    assert_eq!(length(&s), 3.0);
    // Nobody sets it.
    let e = edit(&mut s, json!({"op": "set", "path": "session.length_beats", "value": 64})).unwrap_err();
    assert!(e.contains("The length is the song's content"), "{e}");
}

#[test]
fn an_audio_clip_counts_its_sound_and_its_tail() {
    let (_d, mut s) = open(common::SONG);
    // tone.wav is half a second: a beat at 120. At beat 31 with a 300 ms
    // fade out, which has nothing of the file to fade over, it ends at 32.
    edit(&mut s, json!({"op": "track.add", "id": "song"})).unwrap();
    edit(&mut s, json!({"op": "audio.add", "track": "song", "sample": "tone", "at": 31})).unwrap();
    assert_eq!(length(&s), 32.0);
    edit(&mut s, json!({"op": "audio.move", "clip": "tracks.song.audio.0", "at": 31.5})).unwrap();
    assert_eq!(length(&s), 36.0);
    // Cut at its first quarter second with a 250 ms fade out, the fade
    // sounds over the rest of the file: a beat from the start again.
    edit(&mut s, json!({"op": "set", "path": "tracks.song.audio.0.source_end_seconds", "value": 0.25})).unwrap();
    edit(&mut s, json!({"op": "set", "path": "tracks.song.audio.0.fade_out_ms", "value": 250})).unwrap();
    assert_eq!(length(&s), 36.0);
    edit(&mut s, json!({"op": "audio.move", "clip": "tracks.song.audio.0", "at": 31})).unwrap();
    assert_eq!(length(&s), 32.0);
    // A looped clip lasts its own length.
    edit(&mut s, json!({"op": "clip.loop", "clip": "tracks.song.audio.0", "loop_beats": 0.5, "length": 9})).unwrap();
    assert_eq!(length(&s), 44.0);
    // A point, a section and a marker hold the end too.
    edit(&mut s, json!({"op": "clip.loop", "clip": "tracks.song.audio.0", "loop_beats": "off"})).unwrap();
    assert_eq!(length(&s), 32.0);
    edit(&mut s, json!({"op": "point.add", "owner": "tracks.bass", "param": "sends.plate.gain_db", "at": 50, "value": -6})).unwrap();
    assert_eq!(length(&s), 52.0);
    edit(&mut s, json!({"op": "remove", "path": "tracks.bass.automation.0.points.2"})).unwrap();
    edit(&mut s, json!({"op": "section.add", "id": "outro", "at": 32, "length_beats": 7})).unwrap();
    assert_eq!(length(&s), 40.0);
    edit(&mut s, json!({"op": "section.remove", "section": "outro"})).unwrap();
    edit(&mut s, json!({"op": "marker.add", "at": 33.5})).unwrap();
    assert_eq!(length(&s), 36.0);
}
