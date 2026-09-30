//! A test song with overlapping voices, gates, a choke group, repitching and a
//! source at another rate, written with generated audio.

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

pub fn song(dir: &Path) -> Arc<Program> {
    program(&write_song(dir), |_| {})
}

/// The song at `path` compiled after `edit` changes it.
pub fn program(path: &Path, edit: impl FnOnce(&mut aaw_model::Project)) -> Arc<Program> {
    let mut p = aaw_model::load(path, true).unwrap();
    edit(&mut p);
    Arc::new(compile(&p, path.parent().unwrap()).unwrap())
}

