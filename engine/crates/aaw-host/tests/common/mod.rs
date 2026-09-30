//! A small song with generated audio, for session and host tests.

#![allow(dead_code)]

use aaw_engine::wav::float_wav_bytes;
use aaw_host::command::{Command, Origin};
use aaw_host::session::Session;
use serde_json::Value as Json;
use std::path::{Path, PathBuf};

pub const SONG: &str = r#"
session: {tempo: 120, length_beats: 32}
samples:
  tone: {path: tone.wav, root_note: A3}
  hit: {path: hit.wav}
patterns:
  beat:
    length_beats: 4
    steps: {h: x.x.x.x.x.x.x.x.}
  bass:
    length_beats: 4
    events:
    - {at: 0, pad: t, note: C3, duration: 1}
    - {at: 2, pad: t, note: G2, duration: 1}
tracks:
- id: drums
  pads: {h: {sample: hit}}
  clips: [{pattern: beat, repeats: 4}, {pattern: beat, at: 16, repeats: 4}]
- id: bass
  pads: {t: {sample: tone, mode: gate, release_ms: 20}}
  clips: [{pattern: bass, repeats: 2}, {pattern: bass, at: 8}, {pattern: bass, at: 16, repeats: 2}]
  effects: [{type: compressor, threshold_db: -20, sidechain: drums}]
  sends: [{to: plate, gain_db: -12}]
  automation:
  - param: sends.plate.gain_db
    points: [{at: 0, value: -24}, {at: 16, value: -6}]
returns:
- id: plate
  effects: [{type: reverb, decay_seconds: 1.5}]
sections: [{id: intro, at: 0, length_beats: 8}]
"#;

/// Writes the song and its samples into `dir`.
pub fn write_song(dir: &Path) -> PathBuf {
    let tone: Vec<[f32; 2]> = (0..24000)
        .map(|i| {
            let x = (2.0 * std::f64::consts::PI * 220.0 * i as f64 / 48000.0).sin() * 0.3;
            [x as f32; 2]
        })
        .collect();
    std::fs::write(dir.join("tone.wav"), float_wav_bytes(&tone, 48000)).unwrap();
    let hit: Vec<[f32; 2]> = (0..4800).map(|i| [(0.5 * (-(i as f64) / 400.0).exp()) as f32; 2]).collect();
    std::fs::write(dir.join("hit.wav"), float_wav_bytes(&hit, 48000)).unwrap();
    let path = dir.join("song.yaml");
    std::fs::write(&path, SONG).unwrap();
    path
}

/// Parses a command from JSON.
pub fn cmd(j: Json) -> Command {
    serde_json::from_value(j).expect("command")
}

pub fn edit(s: &mut Session, j: Json) -> Result<Json, String> {
    let (reply, _) = s.edit(&cmd(j), Origin::Agent, None)?;
    Ok(reply)
}

/// A value from `get`, without the references.
pub fn get(s: &Session, path: &str) -> Json {
    fn strip(j: Json) -> Json {
        match j {
            Json::Object(m) => Json::Object(m.into_iter().filter(|(k, _)| k != "ref").map(|(k, v)| (k, strip(v))).collect()),
            Json::Array(a) => Json::Array(a.into_iter().map(strip).collect()),
            other => other,
        }
    }
    strip(s.get(path).expect("get"))
}

pub fn refs(s: &Session, path: &str) -> Vec<String> {
    s.get(path)
        .expect("get")
        .as_array()
        .expect("list")
        .iter()
        .map(|x| x["ref"].as_str().unwrap().to_string())
        .collect()
}
