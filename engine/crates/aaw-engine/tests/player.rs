//! The transport's player: program swaps mid-playback, stops, locates and loop
//! jumps, checked against plain renders, and never allocating.

mod common;

use aaw_engine::player::{channel, Control, Player, FADE_SECONDS};
use aaw_engine::program::Program;
use aaw_engine::render::{Frame, Renderer};
use common::{program, song, write_song};
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

fn fade(p: &Program) -> usize {
    (FADE_SECONDS * p.rate as f64).round() as usize
}

/// A plain render of `n` frames from `from`.
fn plain(p: &Arc<Program>, from: usize, n: usize) -> Vec<Frame> {
    let mut r = Renderer::new(p.clone(), from, 4096);
    let mut out = vec![[0.0; 2]; n];
    for chunk in out.chunks_mut(4096) {
        r.process(chunk, |_, _| {});
    }
    out
}

/// `n` frames from the player in 128-frame callbacks, without allocating.
fn pull(player: &mut Player, n: usize) -> Vec<Frame> {
    let mut out = vec![[0.0; 2]; n];
    assert_no_alloc::assert_no_alloc(|| {
        for chunk in out.chunks_mut(128) {
            player.render(chunk);
        }
    });
    out
}

fn max_diff(a: &[Frame], b: &[Frame]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| (x[0] - y[0]).abs().max((x[1] - y[1]).abs()))
        .fold(0.0, f64::max)
}

fn started(p: &Arc<Program>, from: usize) -> (Control, Player) {
    let (mut control, player) = channel(p.clone());
    control.play(from).unwrap();
    (control, player)
}

#[test]
fn playing_from_the_start_equals_a_render() {
    let dir = tempfile::tempdir().unwrap();
    let p = song(dir.path());
    let (_c, mut player) = started(&p, 0);
    let n = p.total + 1000;
    let out = pull(&mut player, n);
    assert_eq!(&out[..p.total], &plain(&p, 0, p.total)[..]);
    assert!(out[p.total..].iter().all(|f| *f == [0.0; 2]));
    assert!(!player.playing(), "stops at the session end");
}

#[test]
fn playing_mid_song_fades_in_only_the_sounding_voices() {
    let dir = tempfile::tempdir().unwrap();
    let p = song(dir.path());
    let from = 60000;
    let (_c, mut player) = started(&p, from);
    let out = pull(&mut player, 48000);
    let reference = plain(&p, from, 48000);
    let f = fade(&p);
    assert_eq!(&out[f..], &reference[f..]);
    assert!(out[0] == [0.0; 2] && reference[0] != [0.0; 2], "the chased voice fades in");
}

#[test]
fn swapping_in_the_same_song_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let p = program(&path, |_| {});
    let (mut control, mut player) = started(&p, 0);
    let mut out = pull(&mut player, 30000);
    control.load(program(&path, |_| {})).unwrap();
    out.extend(pull(&mut player, 100000));
    assert!(max_diff(&out, &plain(&p, 0, 130000)) < 1e-15);
}

#[test]
fn an_edit_is_heard_after_the_crossfade() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let before = program(&path, |_| {});
    let after = program(&path, |p| {
        p.tracks[0].gain_db = -6.0;
        p.tracks[1].clips[0].at = aaw_model::Beat::int(2);
    });
    let (mut control, mut player) = started(&before, 0);
    let head = pull(&mut player, 40000);
    control.load(after.clone()).unwrap();
    let tail = pull(&mut player, 100000);
    let f = fade(&before);
    assert_eq!(&head[..], &plain(&before, 0, 40000)[..]);
    assert_eq!(&tail[f..], &plain(&after, 40000 + f, 100000 - f)[..]);
    // Within the crossfade the output moves between the two versions.
    let (old, new) = (plain(&before, 40000, f), plain(&after, 40000, f));
    for k in 0..f {
        let g = (f - k) as f64 / f as f64;
        let expected = old[k][0] * g + new[k][0] * (1.0 - g);
        assert!((tail[k][0] - expected).abs() < 1e-12);
    }
}

#[test]
fn a_tempo_change_keeps_the_beat() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let before = program(&path, |_| {});
    let after = program(&path, |p| p.session.tempo = 96.0);
    let (mut control, mut player) = started(&before, 0);
    pull(&mut player, 45000); // beat 2 at 128 BPM
    control.load(after.clone()).unwrap();
    pull(&mut player, 128);
    // Beat 2 at 96 BPM is frame 60000.
    assert_eq!(player.position(), 60000 + 128);
}

#[test]
fn loops_jump_at_the_exact_frame() {
    let dir = tempfile::tempdir().unwrap();
    let p = song(dir.path());
    let (start, end) = (22500, 67500); // beats 1 to 3
    let (mut control, mut player) = started(&p, 0);
    control.set_loop(Some((start, end))).unwrap();
    let out = pull(&mut player, end + 2 * (end - start) + 1000);
    let f = fade(&p);
    let reference = plain(&p, 0, end);
    assert_eq!(&out[..end], &reference[..]);
    let lap = plain(&p, start, end - start);
    for k in 0..2 {
        let at = end + k * (end - start);
        assert_eq!(&out[at + f..at + (end - start)], &lap[f..], "lap {k}");
    }
    assert!(player.playing());
}

#[test]
fn stop_fades_out_then_is_silent_and_play_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let p = song(dir.path());
    let f = fade(&p);
    let (mut control, mut player) = started(&p, 0);
    pull(&mut player, 50000);
    control.stop().unwrap();
    let out = pull(&mut player, 4000);
    assert!(out[..f].iter().any(|x| *x != [0.0; 2]));
    assert!(out[f..].iter().all(|x| *x == [0.0; 2]));
    assert!(!player.playing());
    assert_eq!(player.position(), 50000);
    control.play(90000).unwrap();
    let out = pull(&mut player, 20000);
    assert_eq!(&out[f..], &plain(&p, 90000, 20000)[f..]);
}

#[test]
fn locating_while_playing_rings_out_the_old_position() {
    let dir = tempfile::tempdir().unwrap();
    let p = song(dir.path());
    let f = fade(&p);
    let (mut control, mut player) = started(&p, 0);
    pull(&mut player, 70000);
    control.locate(12345).unwrap();
    let out = pull(&mut player, 30000);
    assert_eq!(&out[f..], &plain(&p, 12345, 30000)[f..]);
}

#[test]
fn many_swaps_and_moves_never_allocate_or_leak() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let versions: Vec<Arc<Program>> = (0..4)
        .map(|i| program(&path, |p| p.tracks[0].gain_db = -(i as f64)))
        .collect();
    let (mut control, mut player) = started(&versions[0], 0);
    control.set_loop(Some((10000, 200000))).unwrap();
    for i in 0..200 {
        // Deck building allocates on this thread; `pull` asserts the player does not.
        control.load(versions[i % 4].clone()).unwrap();
        if i % 7 == 0 {
            control.locate(i * 997 % 300000).unwrap();
        }
        if i % 31 == 0 {
            control.stop().unwrap();
            control.play(i * 1500).unwrap();
        }
        pull(&mut player, 128 * (1 + i % 5));
        control.collect();
    }
    assert_eq!(control.shared().leaked.load(std::sync::atomic::Ordering::Relaxed), 0);
}
