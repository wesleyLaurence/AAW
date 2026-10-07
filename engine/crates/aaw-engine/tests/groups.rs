//! Groups: tracks summed through a chain, gain and pan of their own between
//! the tracks and the master. A group at rest is the sum of its tracks, its
//! latency leaves the mix and the sends aligned, mute and solo reach its
//! tracks, and its preview is its stem.

mod common;

use aaw_engine::program::{compile_scoped, Cache, Program, Scope, StemKind};
use aaw_engine::render::{Frame, Renderer};
use common::{program, write_mix_song};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The mix and stems of the session for blocks of the given sizes, each on
/// the timeline.
fn render(program: &Arc<Program>, sizes: &mut dyn FnMut() -> usize) -> (Vec<Frame>, Vec<Vec<Frame>>) {
    let mut r = Renderer::new(program.clone(), 0, 4096);
    let total = program.total as i64;
    let mut mix = Vec::new();
    let mut stems = vec![Vec::new(); program.stems().len()];
    let mut block = vec![[0.0; 2]; 4096];
    while !r.finished() {
        let n = sizes().min(program.total + program.latency - r.position());
        let heard = r.heard();
        r.render(&mut block[..n], |t, post, at| {
            for (k, f) in post.iter().enumerate() {
                if (0..total).contains(&(at + k as i64)) {
                    stems[t].push(*f);
                }
            }
        });
        for (k, f) in block[..n].iter().enumerate() {
            if (0..total).contains(&(heard + k as i64)) {
                mix.push(*f);
            }
        }
    }
    (mix, stems)
}

fn random_sizes(seed: u64) -> impl FnMut() -> usize {
    let mut s = seed;
    move || {
        s = s.wrapping_mul(2862933555777941757).wrapping_add(3037000493);
        1 + (s >> 33) as usize % 700
    }
}

/// The mix song with the bass and the layer in a group `low`, written with
/// `fields` under the group's id.
fn write_grouped_song(dir: &Path, fields: &str) -> PathBuf {
    let path = write_mix_song(dir);
    let yaml = std::fs::read_to_string(&path).unwrap();
    let yaml = yaml
        .replace("- id: bass\n", "- id: bass\n  group: low\n")
        .replace("- id: layer\n", "- id: layer\n  group: low\n")
        .replace("returns:\n", &format!("groups:\n- id: low\n{fields}returns:\n"));
    std::fs::write(&path, yaml).unwrap();
    path
}

fn close(a: &[Frame], b: &[Frame], tolerance: f64, what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: lengths");
    for (f, (x, y)) in a.iter().zip(b).enumerate() {
        assert!((x[0] - y[0]).abs() < tolerance && (x[1] - y[1]).abs() < tolerance, "{what}: frame {f}: {x:?} is not {y:?}");
    }
}

fn stem<'a>(program: &Program, stems: &'a [Vec<Frame>], name: &str) -> &'a [Frame] {
    let index = program.stems().iter().position(|(n, _)| *n == name).unwrap_or_else(|| panic!("no stem {name}"));
    &stems[index]
}

#[test]
fn a_group_at_rest_is_its_tracks_summed() {
    let dir = tempfile::tempdir().unwrap();
    let plain = program(&write_mix_song(dir.path()), |_| {});
    let grouped = program(&write_grouped_song(dir.path(), ""), |_| {});
    assert_eq!(
        grouped.stems().iter().map(|(n, k)| (*n, *k)).collect::<Vec<_>>(),
        [("hats", StemKind::Track), ("bass", StemKind::Track), ("layer", StemKind::Track), ("low", StemKind::Group), ("room", StemKind::Return), ("echo", StemKind::Return)]
    );
    assert_eq!(grouped.groups[0].members, ["bass", "layer"]);
    assert_eq!(grouped.groups[0].lead, 0);
    assert_eq!((grouped.track_offset, grouped.latency), (plain.track_offset, plain.latency));
    let (mix, stems) = render(&plain, &mut || 512);
    let (mix_g, stems_g) = render(&grouped, &mut || 512);
    // The sum is the same to rounding: the group adds its tracks first.
    close(&mix, &mix_g, 1e-12, "mix");
    for name in ["hats", "bass", "layer", "room", "echo"] {
        assert_eq!(stem(&plain, &stems, name), stem(&grouped, &stems_g, name), "{name}");
    }
    let (bass, layer) = (stem(&grouped, &stems_g, "bass"), stem(&grouped, &stems_g, "layer"));
    let summed: Vec<Frame> = bass.iter().zip(layer).map(|(a, b)| [a[0] + b[0], a[1] + b[1]]).collect();
    close(stem(&grouped, &stems_g, "low"), &summed, 1e-15, "the group's stem");
    assert!(summed.iter().any(|f| f[0] != 0.0));
}

#[test]
fn a_group_s_gain_and_pan_shape_its_sum() {
    let dir = tempfile::tempdir().unwrap();
    let at_rest = program(&write_grouped_song(dir.path(), ""), |_| {});
    let shaped = program(&write_grouped_song(dir.path(), "  gain_db: -6\n  pan: 1\n"), |_| {});
    let (_, stems) = render(&at_rest, &mut || 512);
    let (_, stems_s) = render(&shaped, &mut || 512);
    let gain = 10f64.powf(-6.0 / 20.0);
    let expected: Vec<Frame> = stem(&at_rest, &stems, "low").iter().map(|f| [0.0, f[1] * gain]).collect();
    close(stem(&shaped, &stems_s, "low"), &expected, 1e-12, "the group's stem");
    // Its tracks' stems are before the group, so they do not change.
    assert_eq!(stem(&at_rest, &stems, "bass"), stem(&shaped, &stems_s, "bass"));
}

#[test]
fn a_group_s_latency_leaves_the_mix_and_the_sends_aligned() {
    let dir = tempfile::tempdir().unwrap();
    let at_rest = program(&write_grouped_song(dir.path(), ""), |_| {});
    // A limiter the signal never reaches is a delay of its look-ahead.
    let limited = program(&write_grouped_song(dir.path(), "  effects: [{type: limiter, ceiling_db: -0.1}]\n"), |_| {});
    assert_eq!(limited.groups[0].lead, 144);
    // The bass limiter's 72, then the group's 144; the hats wait for both.
    assert_eq!((limited.track_offset, limited.latency), (216, 360));
    let aligns: Vec<(usize, usize)> = limited.tracks.iter().map(|t| (t.delay, t.align)).collect();
    assert_eq!(aligns, [(0, 216), (0, 0), (72, 0)]);
    let (mix, stems) = render(&at_rest, &mut || 512);
    let (mix_l, stems_l) = render(&limited, &mut || 512);
    close(&mix, &mix_l, 1e-9, "mix");
    for name in ["hats", "bass", "layer", "low", "room", "echo"] {
        close(stem(&at_rest, &stems, name), stem(&limited, &stems_l, name), 1e-9, name);
    }
    // And the blocks the stream is cut into change nothing.
    let random = render(&limited, &mut random_sizes(5));
    assert_eq!(mix_l, random.0);
    assert_eq!(stems_l, random.1);
}

#[test]
fn mute_and_solo_reach_a_group_s_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_grouped_song(dir.path(), "");
    let silent = |x: &[Frame]| x.iter().all(|f| f[0] == 0.0 && f[1] == 0.0);
    // A muted group silences its tracks and their sends.
    let muted = program(&path, |p| p.groups[0].mute = true);
    let (_, stems) = render(&muted, &mut || 512);
    assert!(silent(stem(&muted, &stems, "low")) && silent(stem(&muted, &stems, "bass")));
    assert!(!silent(stem(&muted, &stems, "hats")));
    let hats_only = program(&path, |p| p.tracks[1].sends.clear());
    let (_, hats_stems) = render(&hats_only, &mut || 512);
    assert_eq!(stem(&muted, &stems, "room"), stem(&hats_only, &hats_stems, "room"), "the bass sends nothing");
    // A soloed group is heard with its tracks; the rest is not.
    let soloed = program(&path, |p| p.groups[0].solo = true);
    let (_, stems) = render(&soloed, &mut || 512);
    assert!(silent(stem(&soloed, &stems, "hats")));
    assert!(!silent(stem(&soloed, &stems, "bass")) && !silent(stem(&soloed, &stems, "low")));
    // A soloed track in a group is heard through its group, alone.
    let bass_solo = program(&path, |p| p.tracks[1].solo = true);
    let (_, stems) = render(&bass_solo, &mut || 512);
    assert!(silent(stem(&bass_solo, &stems, "hats")) && silent(stem(&bass_solo, &stems, "layer")));
    assert_eq!(stem(&bass_solo, &stems, "low"), stem(&bass_solo, &stems, "bass"));
}

#[test]
fn a_group_s_preview_is_its_stem_and_a_track_s_is_before_the_group() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_grouped_song(dir.path(), "  sends: [{to: echo, gain_db: -6}]\n  effects: [{type: limiter, ceiling_db: -0.1}]\n");
    let p = aaw_model::load(&path, true).unwrap();
    let full = program(&path, |_| {});
    let (_, stems) = render(&full, &mut || 512);
    for name in ["low", "bass", "echo"] {
        let scoped = Arc::new(compile_scoped(&p, dir.path(), &mut Cache::default(), Scope::Channel(name)).unwrap());
        assert_eq!(scoped.stems().len(), 1, "{name}");
        let (_, preview) = render(&scoped, &mut || 512);
        close(&preview[0], stem(&full, &stems, name), 1e-12, name);
    }
    assert!(stem(&full, &stems, "echo").iter().any(|f| f[0] != 0.0));
}

#[test]
fn stems_of_the_ungrouped_tracks_the_groups_and_the_returns_sum_to_the_mix() {
    let dir = tempfile::tempdir().unwrap();
    let p = program(&write_grouped_song(dir.path(), "  effects: [{type: limiter, ceiling_db: -0.1}]\n"), |p| p.master.effects.clear());
    let (mix, stems) = render(&p, &mut || 1000);
    let summed: Vec<&str> = p.stems().iter().filter(|(n, k)| *k != StemKind::Track || !["bass", "layer"].contains(n)).map(|(n, _)| *n).collect();
    assert_eq!(summed, ["hats", "low", "room", "echo"]);
    for (f, m) in mix.iter().enumerate() {
        let env = p.envelope(f);
        let sum: f64 = summed.iter().map(|n| stem(&p, &stems, n)[f][0]).sum();
        assert!((sum * env - m[0]).abs() < 1e-9, "frame {f}: {} is not {}", sum * env, m[0]);
    }
}
