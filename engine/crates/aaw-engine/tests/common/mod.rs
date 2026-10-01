//! Test songs written with generated audio: one of voices only, with overlaps,
//! gates, a choke group, repitching and a source at another rate, and one with
//! every effect, a sidechain, sends, returns, automation and a master chain.

#![allow(dead_code)]

use aaw_engine::program::{compile, Program};
use aaw_engine::wav::float_wav_bytes;
use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A deterministic pseudo-random sequence for test audio.
pub fn noise(n: usize, seed: u64) -> Vec<f64> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 11) as f64 / (1u64 << 53) as f64 - 0.5) * 0.4
        })
        .collect()
}

pub fn write_sample(dir: &Path, name: &str, frames: Vec<[f32; 2]>, rate: u32) {
    std::fs::write(dir.join(name), float_wav_bytes(&frames, rate)).unwrap();
}

/// A song with overlapping voices, gates, a choke group, repitching and a
/// source at another rate.
pub fn write_song(dir: &Path) -> PathBuf {
    let tone: Vec<[f32; 2]> = (0..24000)
        .map(|i| {
            let x = (2.0 * PI * 220.0 * i as f64 / 48000.0).sin() * 0.3;
            [x as f32, (x * 0.5) as f32]
        })
        .collect();
    write_sample(dir, "tone.wav", tone, 48000);
    let n = noise(8820, 7);
    write_sample(dir, "hit.wav", n.iter().map(|x| [*x as f32, *x as f32]).collect(), 44100);
    let yaml = r#"
session: {tempo: 128, length_beats: 16, end_fade_ms: 50}
samples:
  tone: {path: tone.wav, root_note: A3}
  hit: {path: hit.wav}
patterns:
  bass:
    length_beats: 4
    events:
    - {at: 0, pad: t, note: C3, duration: 1}
    - {at: '1/3', pad: t, note: E3, duration: '2/3'}
    - {at: 2, pad: t, note: G2, duration: 1.5}
  hats:
    length_beats: 4
    steps: {h: x.x. x3x. xx.x 9.x.}
    swing: 0.6
tracks:
- id: bass
  pan: -0.4
  pads: {t: {sample: tone, mode: gate, attack_ms: 2, release_ms: 30}}
  clips: [{pattern: bass, repeats: 4}]
- id: hats
  gain_db: -4
  pads: {h: {sample: hit, choke_group: hh, pan: 0.3, transpose: 3, release_ms: 5}}
  clips: [{pattern: hats, at: 1, repeats: 3, velocity_scale: 0.9}]
- id: layer
  pads: {t: {sample: tone, mono: true, reverse: true}}
  clips: [{pattern: bass, at: 4, repeats: 2}]
"#;
    let path = dir.join("song.yaml");
    std::fs::write(&path, yaml).unwrap();
    path
}

/// The same tracks with every effect: inserts with latency, a sidechain,
/// pre- and post-fader sends, returns, lanes of each kind and a master limiter.
pub fn write_mix_song(dir: &Path) -> PathBuf {
    let path = write_song(dir);
    let voices = std::fs::read_to_string(&path).unwrap();
    let (head, _) = voices.split_once("tracks:").unwrap();
    let mix = r#"tracks:
- id: hats
  gain_db: -4
  pads: {h: {sample: hit, choke_group: hh, pan: 0.3, transpose: 3, release_ms: 5}}
  clips: [{pattern: hats, at: 1, repeats: 3, velocity_scale: 0.9}]
  effects:
  - {type: filter, mode: highpass, cutoff_hz: 300}
  sends: [{to: room, gain_db: -8}, {to: echo, gain_db: -12, pre_fader: true}]
- id: bass
  pan: -0.4
  pads: {t: {sample: tone, mode: gate, attack_ms: 2, release_ms: 30}}
  clips: [{pattern: bass, repeats: 4}]
  effects:
  - type: eq
    id: tone
    bands: [{shape: bell, freq_hz: 400, gain_db: -3}, {shape: high_shelf, freq_hz: 5000, gain_db: 2}]
  - {type: compressor, threshold_db: -30, ratio: 8, attack_ms: 1, release_ms: 150, sidechain: hats}
  - {type: filter, id: sweep, mode: lowpass, cutoff_hz: 8000, slope_db_per_octave: 24}
  - {type: limiter, ceiling_db: -3, lookahead_ms: 1.5}
  sends: [{to: room, gain_db: -10}]
  automation:
  - param: effects.sweep.cutoff_hz
    points: [{at: 2, value: 300}, {at: 6, value: 9000}]
  - param: gain_db
    points: [{at: 0, value: -3}, {at: 8, value: -9, curve: hold}, {at: 12, value: -2}]
  - param: sends.room.gain_db
    points: [{at: 4, value: -30}, {at: 4, value: -6}]
- id: layer
  pads: {t: {sample: tone, mono: true, reverse: true}}
  clips: [{pattern: bass, at: 4, repeats: 2}]
  effects:
  - {type: compressor, threshold_db: -24, sidechain: bass}
returns:
- id: room
  effects:
  - {type: reverb, decay_seconds: 0.6, predelay_ms: 5}
  - {type: compressor, threshold_db: -30, sidechain: hats}
  automation:
  - param: pan
    points: [{at: 0, value: -0.5}, {at: 16, value: 0.5}]
- id: echo
  gain_db: -3
  effects:
  - {type: delay, time_beats: 3/4, feedback_percent: 40, highcut_hz: 4000, ping_pong: true}
master:
  effects: [{type: limiter, ceiling_db: -1}]
  automation:
  - param: gain_db
    points: [{at: 14, value: -6}, {at: 16, value: -20}]
"#;
    std::fs::write(&path, format!("{head}{mix}")).unwrap();
    path
}

pub fn song(dir: &Path) -> Arc<Program> {
    program(&write_song(dir), |_| {})
}

/// The song at `path` compiled after `edit` changes it.
pub fn program(path: &Path, edit: impl FnOnce(&mut aaw_model::Project)) -> Arc<Program> {
    let mut p = aaw_model::load(path, true).unwrap();
    edit(&mut p);
    Arc::new(compile(&p, path.parent().unwrap()).unwrap())
}

