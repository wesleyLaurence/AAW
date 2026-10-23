//! A track frozen to audio: the file holds what its inserts make before its
//! fader, the song with the file in the track's place sounds the same, and
//! the freeze does not depend on how it was cut into blocks.

mod common;

use aaw_engine::freeze::freeze_track;
use aaw_engine::program::{compile, Program};
use aaw_engine::render::{Frame, Renderer};
use aaw_engine::wav::float_wav_bytes;
use aaw_model::{Frozen, Project};
use common::write_mix_song;
use std::path::Path;
use std::sync::Arc;

/// The mix of a program placed on the timeline, as an export is.
fn mix(program: &Arc<Program>) -> Vec<Frame> {
    let mut r = Renderer::new(program.clone(), 0, 4096);
    let mut out = vec![[0.0; 2]; program.total];
    let mut block = vec![[0.0; 2]; 4096];
    while !r.finished() {
        let n = 4096.min(program.total + program.latency - r.position());
        let heard = r.heard();
        r.render(&mut block[..n], |_, _, _| {});
        for (k, f) in block[..n].iter().enumerate() {
            let at = heard + k as i64;
            if at >= 0 && (at as usize) < program.total {
                out[at as usize] = *f;
            }
        }
    }
    out
}

fn load(path: &Path) -> Project {
    aaw_model::load(path, true).unwrap()
}

/// `track` of the song at `path` frozen: its file written under `frozen/`
/// and the song with the field set, as the host does it.
fn frozen(path: &Path, track: &str, block: usize) -> (Project, Vec<[f32; 2]>) {
    let dir = path.parent().unwrap();
    let p = load(path);
    let frames = freeze_track(&p, dir, track, block, &Default::default()).unwrap();
    std::fs::create_dir_all(dir.join("frozen")).unwrap();
    let relative = format!("frozen/{track}.wav");
    std::fs::write(dir.join(&relative), float_wav_bytes(&frames, p.session.sample_rate as u32)).unwrap();
    let mut p = p;
    p.tracks.iter_mut().find(|t| t.id == track).unwrap().frozen = Some(Frozen { path: relative });
    (p, frames)
}

#[test]
fn a_frozen_track_sounds_as_it_did() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_mix_song(dir.path());
    let before = Arc::new(compile(&load(&path), dir.path()).unwrap());
    // bass has an equalizer, a compressor keyed by hats, an automated filter
    // and a limiter with latency, a gain lane and a send.
    let (p, frames) = frozen(&path, "bass", 4096);
    assert_eq!(frames.len(), before.total);
    let after = Arc::new(compile(&p, dir.path()).unwrap());
    let (a, b) = (mix(&before), mix(&after));
    assert_eq!(a.len(), b.len());
    let worst = a.iter().zip(&b).map(|(x, y)| (x[0] - y[0]).abs().max((x[1] - y[1]).abs())).fold(0.0, f64::max);
    assert!(worst < 1e-5, "the frozen song differs from the song by {worst}");
    let loud = a.iter().map(|f| f[0].abs()).fold(0.0, f64::max);
    assert!(loud > 0.05, "the song is silent");
    // The track's limiter no longer delays the song; the master's still does.
    assert!(after.latency < before.latency, "{} < {}", after.latency, before.latency);
    assert_eq!(after.stems().len(), before.stems().len());
    // The file is the inserts' output before the fader: the gain lane, at
    // -3 dB from the start, is not in it, so the file is louder than the stem.
    let bass = after.tracks.iter().find(|t| t.id == "bass").unwrap();
    assert!(bass.chain.devices.is_empty() && bass.chain.effects.is_empty(), "a frozen track has no inserts");
}

#[test]
fn a_freeze_does_not_depend_on_block_partition() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_mix_song(dir.path());
    let p = load(&path);
    let whole = freeze_track(&p, dir.path(), "bass", 4096, &Default::default()).unwrap();
    let small = freeze_track(&p, dir.path(), "bass", 1000, &Default::default()).unwrap();
    assert_eq!(whole, small);
}

#[test]
fn a_frozen_track_freezes_again_from_its_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_mix_song(dir.path());
    let (p, frames) = frozen(&path, "hats", 4096);
    // Frozen already, the track renders from its pads and inserts, not from
    // its file.
    let again = freeze_track(&p, dir.path(), "hats", 4096, &Default::default()).unwrap();
    assert_eq!(again, frames);
    assert!(freeze_track(&p, dir.path(), "nothing", 4096, &Default::default()).unwrap_err().contains("Unknown track"));
}

