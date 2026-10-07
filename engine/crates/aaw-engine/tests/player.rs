//! The transport's player: program swaps mid-playback, stops, locates and loop
//! jumps, checked against plain renders, and never allocating. With effects:
//! edits that glide instead of clicking, tails that ring through edits, stops
//! and locates, and a stream that rests once it is silent.

mod common;

use aaw_engine::player::{channel, Control, Player, FADE_SECONDS};
use aaw_engine::program::{compile_cached, Cache, Program};
use aaw_engine::render::{Frame, Renderer};
use common::{program, song, write_mix_song, write_sample, write_song};
use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

fn fade(p: &Program) -> usize {
    (FADE_SECONDS * p.rate as f64).round() as usize
}

/// A plain render of `n` frames of the stream with the transport from `from`.
fn plain(p: &Arc<Program>, from: usize, n: usize) -> Vec<Frame> {
    let mut r = Renderer::new(p.clone(), from, 4096);
    let mut out = vec![[0.0; 2]; n];
    for chunk in out.chunks_mut(4096) {
        r.render(chunk, |_, _, _| {});
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

/// The largest step between one frame and the next.
fn steepest(x: &[Frame]) -> f64 {
    x.windows(2)
        .map(|w| (w[1][0] - w[0][0]).abs().max((w[1][1] - w[0][1]).abs()))
        .fold(0.0, f64::max)
}

/// The largest step as a share of the peak: a click is a step far larger than
/// a signal of that level makes on its own.
fn roughness(x: &[Frame]) -> f64 {
    steepest(x) / peak(x)
}

fn peak(x: &[Frame]) -> f64 {
    x.iter().fold(0.0, |m, f| m.max(f[0].abs()).max(f[1].abs()))
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
fn a_song_with_latency_plays_as_its_render_that_much_later() {
    let dir = tempfile::tempdir().unwrap();
    let p = program(&write_mix_song(dir.path()), |_| {});
    assert_eq!(p.latency, 216);
    let (control, mut player) = started(&p, 0);
    let out = pull(&mut player, p.total + p.latency + 128);
    assert_eq!(&out[..p.total + p.latency], &plain(&p, 0, p.total + p.latency)[..]);
    assert!(out[..p.latency].iter().all(|f| *f == [0.0; 2]));
    assert!(peak(&out[p.latency..]) > 0.01);
    assert!(!player.playing(), "stops once the last frame has come out");
    // The playhead is where the output is.
    assert_eq!(control.position(), p.total);
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
    for path in [write_song(dir.path()), write_mix_song(dir.path())] {
        let p = program(&path, |_| {});
        let (mut control, mut player) = started(&p, 0);
        let mut out = pull(&mut player, 30000);
        control.load(program(&path, |_| {})).unwrap();
        out.extend(pull(&mut player, 100000));
        assert!(max_diff(&out, &plain(&p, 0, 130000)) < 1e-12);
    }
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
    // Within the fade the level glides and the moved clip's voices ring out
    // under the new ones, which is a crossfade between the two versions.
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
    let voices = write_song(dir.path());
    let mut versions: Vec<Arc<Program>> = (0..4)
        .map(|i| program(&voices, |p| p.tracks[0].gain_db = -(i as f64)))
        .collect();
    let (mut control, mut player) = started(&versions[0], 0);
    // With effects: levels, a moved clip, and devices bypassed and back, which
    // changes the structure and the latency.
    let mix = write_mix_song(dir.path());
    let mut cache = Cache::default();
    for i in 0..6 {
        let mut p = aaw_model::load(&mix, true).unwrap();
        p.tracks[1].gain_db = -(i as f64);
        if i % 2 == 1 {
            p.tracks[0].clips[0].at = aaw_model::Beat::int(2);
        }
        if i % 3 == 2 {
            p.master.effects.clear();
            p.returns[0].effects.pop();
        }
        versions.push(Arc::new(compile_cached(&p, dir.path(), &mut cache).unwrap()));
    }
    control.set_loop(Some((10000, 200000))).unwrap();
    for i in 0..400 {
        // Deck building allocates on this thread; `pull` asserts the player does not.
        control.load(versions[if i < 100 { i % 4 } else { 4 + i % 6 }].clone()).unwrap();
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

/// A song for hearing clicks: one sustained low tone through every effect, a
/// reverb and a delay on returns and a master limiter. Its own output never
/// steps far from one frame to the next.
fn write_tone_song(dir: &Path) -> PathBuf {
    let tone: Vec<[f32; 2]> = (0..4 * 48000).map(|i| [(0.4 * (2.0 * PI * 110.0 * i as f64 / 48000.0).sin()) as f32; 2]).collect();
    write_sample(dir, "long.wav", tone, 48000);
    let yaml = r#"
session: {tempo: 120, length_beats: 8, master_gain_db: -6}
samples:
  long: {path: long.wav}
  hit: {path: long.wav}
patterns:
  drone: {length_beats: 8, events: [{at: 0, pad: t, duration: 8}]}
  pulse: {length_beats: 8, events: [{at: 6, pad: t, duration: 1}]}
tracks:
- id: drone
  pads: {t: {sample: long, mode: gate, attack_ms: 20, release_ms: 50}}
  clips: [{pattern: drone}]
  effects:
  - {type: eq, bands: [{shape: bell, freq_hz: 110, gain_db: 3}]}
  - {type: compressor, threshold_db: -20, ratio: 3}
  - {type: filter, mode: lowpass, cutoff_hz: 2000, slope_db_per_octave: 24}
  - {type: delay, time_beats: 1/3, feedback_percent: 30, mix_percent: 20, highcut_hz: 3000}
  sends: [{to: room, gain_db: -12}]
- id: pulse
  mute: true
  pads: {t: {sample: hit, mode: gate, attack_ms: 20, release_ms: 50}}
  clips: [{pattern: pulse}]
returns:
- id: room
  effects: [{type: reverb, decay_seconds: 0.8, mix_percent: 100}]
- id: echo
  effects: [{type: delay, time_beats: 1/2}]
master:
  effects: [{type: limiter, ceiling_db: -1}]
"#;
    let path = dir.join("tone.yaml");
    std::fs::write(&path, yaml).unwrap();
    path
}

/// The tone song after `edit`, compiled through a cache as a host compiles.
fn tone(path: &Path, cache: &mut Cache, edit: impl FnOnce(&mut aaw_model::Project)) -> Arc<Program> {
    let mut p = aaw_model::load(path, true).unwrap();
    edit(&mut p);
    Arc::new(compile_cached(&p, path.parent().unwrap(), cache).unwrap())
}

#[test]
fn dragging_levels_and_knobs_does_not_click() {
    use aaw_model::Effect;
    let dir = tempfile::tempdir().unwrap();
    let path = write_tone_song(dir.path());
    let mut cache = Cache::default();
    let base = tone(&path, &mut cache, |_| {});
    let (_c, mut player) = started(&base, 0);
    let reference = pull(&mut player, 3 * 48000);
    let natural = roughness(&reference);
    assert!(natural > 0.005 && natural < 0.05, "{natural}");
    // A level stepped without a glide is several times rougher.
    let mut stepped = reference.clone();
    for f in &mut stepped[60000..] {
        *f = [f[0] * 0.5, f[1] * 0.5];
    }
    assert!(roughness(&stepped) > natural * 8.0, "{} against {natural}", roughness(&stepped));

    // Every level and knob moves a large step thirty times a second.
    let (mut control, mut player) = started(&base, 0);
    let mut out = pull(&mut player, 9600);
    for step in 0..60 {
        let up = step % 2 == 0;
        let p = tone(&path, &mut cache, |p| {
            let t = &mut p.tracks[0];
            t.gain_db = if up { -9.0 } else { 0.0 };
            t.pan = if up { 0.8 } else { -0.8 };
            t.sends[0].gain_db = if up { -30.0 } else { -6.0 };
            if step % 4 < 2 {
                t.sends.push(aaw_model::Send {
                    to: "echo".into(),
                    gain_db: -6.0,
                    pre_fader: step % 8 < 4,
                });
            }
            for e in &mut t.effects {
                match e {
                    Effect::Eq(e) => e.bands[0].gain_db = if up { -9.0 } else { 9.0 },
                    Effect::Compressor(c) => (c.threshold_db, c.makeup_db) = if up { (-40.0, 6.0) } else { (-10.0, -3.0) },
                    Effect::Filter(f) => f.cutoff_hz = if up { 150.0 } else { 6000.0 },
                    Effect::Delay(d) => (d.feedback_percent, d.mix_percent, d.highcut_hz) = if up { (70.0, 60.0, Some(800.0)) } else { (10.0, 5.0, Some(9000.0)) },
                    _ => {}
                }
            }
            let Effect::Reverb(r) = &mut p.returns[0].effects[0] else { panic!() };
            r.mix_percent = if up { 40.0 } else { 100.0 };
            p.returns[0].gain_db = if up { -12.0 } else { 0.0 };
            p.session.master_gain_db = if up { -12.0 } else { -6.0 };
            p.tracks[1].mute = step % 3 != 0;
        });
        assert_eq!(p.structure, base.structure, "levels and knobs keep the structure");
        control.load(p).unwrap();
        out.extend(pull(&mut player, 1600));
    }
    assert!(max_diff(&out[9600..], &reference[9600..out.len()]) > 0.05, "the edits are heard");
    assert!(roughness(&out) < natural * 3.0, "{} against the song's own {natural}", roughness(&out));
}

#[test]
fn a_tail_rings_through_an_edit_a_locate_and_a_stop() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tone_song(dir.path());
    let mut cache = Cache::default();
    // Only the pulse sounds, for a beat from beat 6, into the reverb alone.
    let quiet = |p: &mut aaw_model::Project| {
        p.tracks[0].clips.clear();
        p.tracks[0].mute = true;
        p.tracks[1].mute = false;
        p.tracks[1].sends.push(aaw_model::Send {
            to: "room".into(),
            gain_db: 0.0,
            pre_fader: false,
        });
        p.master.effects.clear();
    };
    let base = tone(&path, &mut cache, quiet);
    let from = 7 * 24000 + 2400; // just after the pulse ends
    let tail = |control: &mut dyn FnMut(&mut Control)| {
        let (mut c, mut player) = started(&base, 6 * 24000);
        let head = pull(&mut player, from - 6 * 24000);
        control(&mut c);
        (head, pull(&mut player, 12000))
    };
    let (head, reference) = tail(&mut |_| {});
    assert!(peak(&head) > 0.05 && peak(&reference[..2400]) > 1e-3, "the reverb is ringing");

    // An edit of another track's clips, a level and a tempo-free knob.
    let edited = tone(&path, &mut cache, |p| {
        quiet(p);
        p.tracks[0].clips.push(aaw_model::Clip {
            pattern: "drone".into(),
            at: aaw_model::Beat::int(0),
            repeats: 1,
            velocity_scale: 0.5,
        });
    });
    let (_, after_edit) = tail(&mut |c| c.load(edited.clone()).unwrap());
    assert!(max_diff(&after_edit, &reference) < 1e-9, "the tail is cut or changed by {}", max_diff(&after_edit, &reference));

    // A locate to silence: the voices move, the tail stays.
    let (_, after_locate) = tail(&mut |c| c.locate(1000).unwrap());
    assert!(max_diff(&after_locate, &reference) < 1e-9);

    // A stop: the timeline stands still and the tail rings out.
    let (_, after_stop) = tail(&mut |c| c.stop().unwrap());
    assert!(max_diff(&after_stop[..4000], &reference[..4000]) < 1e-9);
}

#[test]
fn a_change_of_structure_fades_through_silence_and_keeps_tails() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tone_song(dir.path());
    let mut cache = Cache::default();
    let base = tone(&path, &mut cache, |_| {});
    // A filter added before the delay, and the master limiter removed.
    let changed = tone(&path, &mut cache, |p| {
        p.tracks[0].effects.insert(
            0,
            aaw_model::Effect::Filter(aaw_model::Filter {
                id: None,
                mode: aaw_model::FilterMode::Highpass,
                cutoff_hz: 60.0,
                slope_db_per_octave: 12,
                bypass: false,
            }),
        );
        p.master.effects.clear();
    });
    assert_ne!(base.structure, changed.structure);
    let (mut control, mut player) = started(&base, 0);
    let head = pull(&mut player, 48000);
    control.load(changed.clone()).unwrap();
    let tail = pull(&mut player, 48000);
    let f = fade(&base);
    // The output falls to silence, where the programs change hands, and rises.
    assert_eq!(tail[f], [0.0; 2]);
    assert!(peak(&tail[..f / 2]) > 0.05 && peak(&tail[2 * f..3 * f]) > 0.05);
    let all: Vec<Frame> = head.iter().chain(&tail).copied().collect();
    assert!(steepest(&all[4800..]) < 0.03, "a step of {}", steepest(&all[4800..]));
    // Devices that are still there kept their state: the delay's echoes of
    // the first second are in the second, where a fresh program has none.
    let fresh = plain(&changed, 48000, 48000);
    assert!(max_diff(&tail[4 * f..], &fresh[4 * f..]) > 1e-3);
}

#[test]
fn a_stopped_stream_rests_once_it_is_silent() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tone_song(dir.path());
    let base = tone(&path, &mut Cache::default(), |_| {});
    let (mut control, mut player) = started(&base, 0);
    pull(&mut player, 48000);
    control.stop().unwrap();
    let out = pull(&mut player, 6 * 48000);
    let f = fade(&base);
    // The delays and the reverb ring on past the fade of the voices.
    assert!(peak(&out[2 * f..24000]) > 1e-3);
    assert!(out[5 * 48000..].iter().all(|x| *x == [0.0; 2]), "resting");
    assert_eq!(player.position(), 48000);
    control.play(0).unwrap();
    assert!(peak(&pull(&mut player, 24000)) > 0.05);
}

fn empty_metronome_song(dir: &Path, tempo: f64, rate: u32, limiter: bool) -> Arc<Program> {
    metered_metronome_song(dir, tempo, rate, limiter, "4/4")
}

fn metered_metronome_song(dir: &Path, tempo: f64, rate: u32, limiter: bool, meter: &str) -> Arc<Program> {
    let path = dir.join("click.yaml");
    let effect = if limiter { "master: {effects: [{type: limiter, lookahead_ms: 5}]}" } else { "" };
    std::fs::write(&path, format!(
        "session: {{tempo: {tempo}, sample_rate: {rate}, length_beats: 32, time_signature: {meter}}}\n{effect}\n"
    )).unwrap();
    program(&path, |_| {})
}

/// The clicks of `out` as the frames they start on, and for each whether it
/// is the accented one: the accent is the higher tone, so its first
/// quarter-cycle is shorter.
fn clicks(out: &[[f64; 2]]) -> Vec<(usize, bool)> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < out.len() {
        if out[i][0] != 0.0 {
            let onset = i - 1;
            // The first zero crossing after the rise is half a cycle in.
            let crossing = (i..out.len().min(i + 200)).find(|&k| out[k][0] <= 0.0).unwrap_or(i + 200);
            found.push((onset, crossing - onset < 17));
            i = onset + 1500;
        } else {
            i += 1;
        }
    }
    found
}

#[test]
fn metronome_counts_the_time_signatures_beats() {
    let dir = tempfile::tempdir().unwrap();
    // 3/4 at 120: a click every 24000 frames, every third one accented.
    let waltz = metered_metronome_song(dir.path(), 120.0, 48000, false, "3/4");
    let (mut control, mut player) = started(&waltz, 0);
    control.set_metronome(true).unwrap();
    let out = pull(&mut player, 24000 * 6 - 100);
    let found = clicks(&out);
    assert_eq!(found.iter().map(|c| c.0).collect::<Vec<_>>(), [0, 24000, 48000, 72000, 96000, 120000]);
    assert_eq!(found.iter().map(|c| c.1).collect::<Vec<_>>(), [true, false, false, true, false, false]);
    // 6/8 at 120: the eighth is the beat, a click every 12000 frames, every
    // sixth one accented; a bar is still three quarter notes.
    let six_eight = metered_metronome_song(dir.path(), 120.0, 48000, false, "6/8");
    let (mut control, mut player) = started(&six_eight, 0);
    control.set_metronome(true).unwrap();
    let out = pull(&mut player, 12000 * 12 - 100);
    let found = clicks(&out);
    assert_eq!(found.len(), 12);
    assert_eq!(found.iter().map(|c| c.0).collect::<Vec<_>>(), (0..12).map(|k| k * 12000).collect::<Vec<_>>());
    assert_eq!(found.iter().map(|c| c.1).collect::<Vec<_>>(), (0..12).map(|k| k % 6 == 0).collect::<Vec<_>>());
    // 4/4 as before: the same clicks as the two tests above measure.
    let common = empty_metronome_song(dir.path(), 120.0, 48000, false);
    let (mut control, mut player) = started(&common, 0);
    control.set_metronome(true).unwrap();
    let found = clicks(&pull(&mut player, 24000 * 8 - 100));
    assert_eq!(found.iter().map(|c| c.1).collect::<Vec<_>>(), [true, false, false, false, true, false, false, false]);
}

#[test]
fn metronome_has_exact_beats_and_never_changes_offline_audio() {
    for rate in [44100, 48000] {
        let dir = tempfile::tempdir().unwrap();
        let p = empty_metronome_song(dir.path(), 137.5, rate, true);
        let period = rate as f64 * 60.0 / 137.5;
        let n = (period * 8.0).round() as usize + p.latency;
        let (mut control, mut player) = started(&p, 0);
        assert_eq!(peak(&plain(&p, 0, n)), 0.0);
        assert_eq!(peak(&pull(&mut player, 1000)), 0.0, "off by default");
        control.locate(0).unwrap();
        control.set_metronome(true).unwrap();
        let out = pull(&mut player, n);
        assert_eq!(peak(&out[..p.latency]), 0.0, "click follows output latency");
        for beat in 0..8 {
            let onset = (period * beat as f64).round() as usize + p.latency;
            assert_eq!(out[onset], [0.0; 2]);
            assert!(peak(&out[onset + 1..onset + 800]) > 0.02);
            let silent = onset + (rate as f64 * 0.025).ceil() as usize;
            let next = (period * (beat + 1) as f64).round() as usize + p.latency;
            assert_eq!(peak(&out[silent..next]), 0.0, "silence between beats");
        }
        let (mut other, mut partitioned) = started(&p, 0);
        other.set_metronome(true).unwrap();
        let mut blocks = vec![[0.0; 2]; n];
        assert_no_alloc::assert_no_alloc(|| {
            for chunk in blocks.chunks_mut(997) { partitioned.render(chunk); }
        });
        assert_eq!(out, blocks, "callback sizes cannot change the click");
        assert_eq!(peak(&plain(&p, 0, n)), 0.0, "monitoring does not enter render or stems");
        control.set_metronome(false).unwrap();
        pull(&mut player, 1000);
        assert_eq!(peak(&pull(&mut player, rate as usize)), 0.0);
    }
}

#[test]
fn metronome_follows_loops_locate_stop_and_tempo_changes() {
    let dir = tempfile::tempdir().unwrap();
    let p = empty_metronome_song(dir.path(), 120.0, 48000, false);
    let (mut control, mut player) = started(&p, 0);
    control.set_metronome(true).unwrap();
    control.set_loop(Some((24000, 72000))).unwrap();
    let out = pull(&mut player, 72000 + 2 * 48000);
    assert_eq!(&out[24000..72000], &out[72000..120000]);
    assert_eq!(&out[72000..120000], &out[120000..168000]);
    control.set_loop(None).unwrap();
    control.locate(12000).unwrap();
    assert_eq!(peak(&pull(&mut player, 12000)), 0.0, "off-beat locate waits for next beat");
    assert!(peak(&pull(&mut player, 1000)) > 0.02);
    control.stop().unwrap();
    pull(&mut player, 1000);
    assert_eq!(peak(&pull(&mut player, 24000)), 0.0);
    control.play(24000).unwrap();
    assert!(peak(&pull(&mut player, 24000)) > 0.02);
    let slower = empty_metronome_song(dir.path(), 96.0, 48000, false);
    control.load(slower).unwrap();
    let changed = pull(&mut player, 60000);
    assert!(peak(&changed[..1000]) > 0.02, "the same beat after a tempo edit");
    assert_eq!(peak(&changed[1200..30000]), 0.0);
    assert!(peak(&changed[30000..31000]) > 0.02, "96 BPM is 30000 frames a beat");
    control.locate(p.total - 100).unwrap();
    pull(&mut player, 300000);
    assert!(!player.playing());
    assert_eq!(peak(&pull(&mut player, 1000)), 0.0, "silent after song end");
}
