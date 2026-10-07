//! The renderer's guarantees: output independent of block partition, a start
//! mid-song equal to the same frames of a render from the start, aligned
//! latencies, and no allocation while processing.

mod common;

use aaw_engine::program::{compile_cached, Cache, Program};
use aaw_engine::render::{Frame, Renderer};
use common::{program, song, write_mix_song};
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

/// The mix and stems of the session for consecutive blocks of the given sizes,
/// with the transport starting at `from`. Each is placed on the timeline, so
/// the frames the program's latency delays are where they belong.
fn render(program: &Arc<Program>, from: usize, sizes: &mut dyn FnMut() -> usize) -> (Vec<Frame>, Vec<Vec<Frame>>) {
    let mut r = Renderer::new(program.clone(), from, 4096);
    let total = program.total as i64;
    let first = from as i64;
    let mut mix = Vec::new();
    let mut stems = vec![Vec::new(); program.stems().len()];
    let mut block = vec![[0.0; 2]; 4096];
    while !r.finished() {
        let n = sizes().min(program.total + program.latency - r.position());
        let heard = r.heard();
        r.render(&mut block[..n], |t, post, at| {
            for (k, f) in post.iter().enumerate() {
                if (first..total).contains(&(at + k as i64)) {
                    stems[t].push(*f);
                }
            }
        });
        for (k, f) in block[..n].iter().enumerate() {
            if (first..total).contains(&(heard + k as i64)) {
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

#[test]
fn output_does_not_depend_on_block_partition() {
    let dir = tempfile::tempdir().unwrap();
    let program = song(dir.path());
    let whole = render(&program, 0, &mut || 4096);
    let random = render(&program, 0, &mut random_sizes(3));
    assert_eq!(whole.0, random.0);
    assert_eq!(whole.1, random.1);
    assert_eq!(whole.0.len(), program.total);
    assert!(whole.0.iter().any(|f| f[0] != 0.0), "the song is silent");
}

#[test]
fn effects_routing_and_automation_do_not_depend_on_block_partition() {
    let dir = tempfile::tempdir().unwrap();
    let program = program(&write_mix_song(dir.path()), |_| {});
    // The bass limiter looks 72 frames ahead, the layer it keys waits for it,
    // and the master limiter adds its 144.
    assert_eq!((program.track_offset, program.return_latency, program.latency), (72, 0, 216));
    assert_eq!(program.tracks.iter().map(|t| (t.delay, t.align)).collect::<Vec<_>>(), [(0, 72), (0, 0), (72, 0)]);
    let whole = render(&program, 0, &mut || 4096);
    let random = render(&program, 0, &mut random_sizes(8));
    assert_eq!(whole.0, random.0);
    assert_eq!(whole.1, random.1);
    assert_eq!(whole.0.len(), program.total);
    assert_eq!(whole.1.len(), 5, "three tracks and two returns");
    for (stem, (name, _)) in whole.1.iter().zip(program.stems()) {
        assert_eq!(stem.len(), program.total, "{name}");
        assert!(stem.iter().any(|f| f[0] != 0.0), "{name} is silent");
    }
}

#[test]
fn stems_sum_to_the_mix_before_master_effects() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_mix_song(dir.path());
    let p = program(&path, |p| p.master.effects.clear());
    let (mix, stems) = render(&p, 0, &mut || 1000);
    for (f, m) in mix.iter().enumerate() {
        let env = p.envelope(f);
        let sum: f64 = stems.iter().map(|s| s[f][0]).sum();
        assert!((sum * env - m[0]).abs() < 1e-12, "frame {f}: {} is not {}", sum * env, m[0]);
    }
}

#[test]
fn a_bypassed_effect_is_not_there() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_mix_song(dir.path());
    let bypass = |on: bool| {
        program(&path, move |p| {
            let aaw_model::Effect::Limiter(l) = &mut p.tracks[1].effects[3] else { panic!() };
            l.bypass = on;
        })
    };
    let without = program(&path, |p| {
        p.tracks[1].effects.pop();
    });
    assert_eq!(bypass(true).latency, without.latency);
    assert_eq!(render(&bypass(true), 0, &mut || 512), render(&without, 0, &mut || 512));
    assert_ne!(render(&bypass(false), 0, &mut || 512).0, render(&without, 0, &mut || 512).0);
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
    let voices = song(dir.path());
    let mix = program(&write_mix_song(dir.path()), |_| {});
    for program in [voices, mix] {
        for from in [0, 30000] {
            let mut r = Renderer::new(program.clone(), from, 128);
            let mut block = vec![[0.0; 2]; 128];
            assert_no_alloc::assert_no_alloc(|| {
                while !r.finished() {
                    r.render(&mut block, |_, _, _| {});
                }
                // Stopped, the stream runs on.
                for _ in 0..50 {
                    r.process(&mut block, false, None, |_, _, _| {});
                }
            });
        }
    }
}

#[test]
fn a_compile_redoes_only_what_an_edit_changed() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_mix_song(dir.path());
    let mut cache = Cache::default();
    let mut compile = |edit: &dyn Fn(&mut aaw_model::Project)| {
        let mut p = aaw_model::load(&path, true).unwrap();
        edit(&mut p);
        Arc::new(compile_cached(&p, dir.path(), &mut cache).unwrap())
    };
    let base = compile(&|_| {});
    // Levels, effects and lanes leave every track's voices as they are.
    let mixed = compile(&|p| {
        p.tracks[0].gain_db = -7.5;
        p.tracks[1].mute = true;
        p.tracks[1].effects.pop();
        p.session.master_gain_db = -3.0;
    });
    for (a, b) in base.tracks.iter().zip(&mixed.tracks) {
        assert!(Arc::ptr_eq(&a.voices, &b.voices), "{}", a.id);
    }
    assert_ne!(base.structure, mixed.structure, "an effect was removed");
    // A clip edit redoes that track alone, and shares its prepared audio.
    let moved = compile(&|p| p.tracks[0].clips[0].at = aaw_model::Beat::int(2));
    assert!(!Arc::ptr_eq(&base.tracks[0].voices, &moved.tracks[0].voices));
    assert!(Arc::ptr_eq(&base.tracks[1].voices, &moved.tracks[1].voices));
    assert!(Arc::ptr_eq(&base.tracks[0].voices[0].audio, &moved.tracks[0].voices[0].audio));
    assert_eq!(base.structure, moved.structure);
    // A compile from nothing plays the same.
    let fresh = program(&path, |p| p.tracks[0].clips[0].at = aaw_model::Beat::int(2));
    assert_eq!(render(&moved, 0, &mut || 512), render(&fresh, 0, &mut || 512));
}

#[test]
fn a_preview_is_the_channel_s_stem() {
    use aaw_engine::program::{compile_scoped, Scope};
    let dir = tempfile::tempdir().unwrap();
    let path = write_mix_song(dir.path());
    let p = aaw_model::load(&path, true).unwrap();
    let full = program(&path, |_| {});
    let (_, stems) = render(&full, 0, &mut || 512);
    for (index, (name, _)) in full.stems().iter().enumerate() {
        let scoped = compile_scoped(&p, dir.path(), &mut Cache::default(), Scope::Channel(name)).unwrap();
        let scoped = Arc::new(scoped);
        assert_eq!(scoped.stems().len(), 1, "{name}");
        let (mix, preview) = render(&scoped, 0, &mut || 512);
        assert_eq!(preview[0], stems[index], "{name}");
        // The preview's mix is that stem under the master gain and end fade.
        for (f, m) in mix.iter().enumerate().step_by(97) {
            assert!((m[0] - preview[0][f][0] * full.envelope(f)).abs() < 1e-15);
        }
    }
    assert!(compile_scoped(&p, dir.path(), &mut Cache::default(), Scope::Channel("nope")).is_err());
}

#[test]
fn an_equalizer_s_output_is_tapped_for_the_spectrum() {
    use aaw_dsp::spectrum::{Analyzer, Taps, TAP_FRAMES};
    let dir = tempfile::tempdir().unwrap();
    let path = write_mix_song(dir.path());
    let p = aaw_model::load(&path, true).unwrap();
    let taps = Arc::new(Taps::default());
    let program = Arc::new(compile_cached(&p, dir.path(), &mut Cache::with_taps(taps.clone())).unwrap());
    // The bass's equalizer has a tap; its filter and the hats' do not.
    let tap = taps.find("bass", 0).expect("an equalizer is tapped");
    assert!(taps.find("bass", 2).is_none() && taps.find("hats", 0).is_none());
    assert_eq!(tap.written(), 0);
    // The song to beat 9 of 16 through the renderer, where a bass note
    // sounds: the ring holds its last frames.
    let mut r = Renderer::new(program.clone(), 0, 4096);
    let mut block = vec![[0.0; 2]; 512];
    let played = program.total * 9 / 16 / 512 * 512;
    for _ in 0..played / 512 {
        r.render(&mut block, |_, _, _| {});
    }
    assert_eq!(tap.written() as usize, played);
    let mut frames = [0.0; TAP_FRAMES];
    tap.read(&mut frames);
    assert!(frames.iter().any(|x| x.abs() > 1e-6), "the bass has sounded");
    let levels = Analyzer::default().analyze(&tap);
    let loudest = levels.iter().cloned().fold(f32::MIN, f32::max);
    assert!(loudest > -60.0, "{loudest} dB at the loudest bin");
    // A compile that drops the equalizer drops its tap, and one that keeps
    // it keeps the same ring.
    let mut cache = Cache::with_taps(taps.clone());
    compile_cached(&p, dir.path(), &mut cache).unwrap();
    assert!(Arc::ptr_eq(&tap, &taps.find("bass", 0).unwrap()));
    let mut without = p.clone();
    without.tracks[1].effects.remove(0);
    compile_cached(&without, dir.path(), &mut cache).unwrap();
    assert!(taps.find("bass", 0).is_none());
}
