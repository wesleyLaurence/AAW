//! The renderer's guarantees: output independent of block partition, a start
//! mid-song equal to the same frames of a render from the start, and no
//! allocation while processing.

use aaw_engine::program::{compile, Program};
use aaw_engine::render::{Frame, Renderer};
use aaw_engine::wav::float_wav_bytes;
use std::f64::consts::PI;
use std::path::Path;
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

/// A deterministic pseudo-random sequence for test audio.
fn noise(n: usize, seed: u64) -> Vec<f64> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 11) as f64 / (1u64 << 53) as f64 - 0.5) * 0.4
        })
        .collect()
}

fn write_sample(dir: &Path, name: &str, frames: Vec<[f32; 2]>, rate: u32) {
    std::fs::write(dir.join(name), float_wav_bytes(&frames, rate)).unwrap();
}

/// A song with overlapping voices, gates, a choke group, repitching and a
/// source at another rate.
fn song(dir: &Path) -> Arc<Program> {
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
    let p = aaw_model::load(&path, true).unwrap();
    Arc::new(compile(&p, dir).unwrap())
}

/// The mix and stems for consecutive blocks of the given sizes, from `from`.
fn render(program: &Arc<Program>, from: usize, sizes: &mut dyn FnMut() -> usize) -> (Vec<Frame>, Vec<Vec<Frame>>) {
    let mut r = Renderer::new(program.clone(), from, 4096);
    let mut mix = Vec::new();
    let mut stems = vec![Vec::new(); program.tracks.len()];
    let mut block = vec![[0.0; 2]; 4096];
    while !r.finished() {
        let n = sizes().min(program.total - r.position());
        r.process(&mut block[..n], |t, post| stems[t].extend_from_slice(post));
        mix.extend_from_slice(&block[..n]);
    }
    (mix, stems)
}

#[test]
fn output_does_not_depend_on_block_partition() {
    let dir = tempfile::tempdir().unwrap();
    let program = song(dir.path());
    let whole = render(&program, 0, &mut || 4096);
    let mut s = 3u64;
    let random = render(&program, 0, &mut || {
        s = s.wrapping_mul(2862933555777941757).wrapping_add(3037000493);
        1 + (s >> 33) as usize % 700
    });
    assert_eq!(whole.0, random.0);
    assert_eq!(whole.1, random.1);
    assert!(whole.0.iter().any(|f| f[0] != 0.0), "the song is silent");
}

#[test]
fn playing_from_the_middle_picks_up_sounding_voices() {
    let dir = tempfile::tempdir().unwrap();
    let program = song(dir.path());
    let whole = render(&program, 0, &mut || 512);
    // Mid-note in both the bass and a reversed layer voice.
    for from in [12345, 60000, 90001] {
        let tail = render(&program, from, &mut || 128);
        assert_eq!(&whole.0[from..], &tail.0[..], "from frame {from}");
        assert!(tail.0[..64].iter().any(|f| f[0] != 0.0), "nothing sounding at {from}");
    }
}

#[test]
fn processing_never_allocates() {
    let dir = tempfile::tempdir().unwrap();
    let program = song(dir.path());
    for from in [0, 30000] {
        let mut r = Renderer::new(program.clone(), from, 128);
        let mut block = vec![[0.0; 2]; 128];
        assert_no_alloc::assert_no_alloc(|| {
            while !r.finished() {
                r.process(&mut block, |_, _| {});
            }
        });
    }
}
