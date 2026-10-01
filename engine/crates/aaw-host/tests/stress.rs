//! Real-time stress: a thread renders 128-frame blocks on the audio schedule,
//! with allocation checking, while random commands change the song and each
//! revision is compiled and swapped in: clips, levels, knobs, sends, lanes,
//! and effects added, bypassed and removed, through a sidechained compressor
//! and a reverb. Counts blocks that overran their budget.

mod common;

use aaw_engine::player::channel;
use aaw_engine::program::{compile_cached, Cache};
use aaw_host::command::Origin;
use aaw_host::session::Session;
use common::{cmd, write_song};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

/// A deterministic sequence for choosing edits.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

#[test]
fn random_edits_during_playback_never_allocate_on_the_audio_thread() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let mut s = Session::open(&path, true).unwrap();
    let mut cache = Cache::default();
    let program = Arc::new(compile_cached(s.project(), s.dir(), &mut cache).unwrap());
    let rate = program.rate as f64;
    let (mut control, mut player) = channel(program);
    control.set_loop(Some((0, (rate * 16.0) as usize))).unwrap();
    control.play(0).unwrap();

    let done = Arc::new(AtomicBool::new(false));
    let (blocks, over, worst) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    let audio = {
        let (done, blocks, over, worst) = (done.clone(), blocks.clone(), over.clone(), worst.clone());
        std::thread::spawn(move || {
            let mut block = vec![[0.0; 2]; 128];
            let budget = Duration::from_secs_f64(128.0 / rate);
            let mut deadline = Instant::now();
            while !done.load(Relaxed) {
                let t = Instant::now();
                assert_no_alloc::assert_no_alloc(|| player.render(&mut block));
                let took = t.elapsed();
                blocks.fetch_add(1, Relaxed);
                worst.fetch_max(took.as_nanos() as u64, Relaxed);
                if took > budget {
                    over.fetch_add(1, Relaxed);
                }
                deadline += budget;
                if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
            }
        })
    };

    let mut rng = Lcg(5);
    let mut applied = 0;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        let n = rng.next(1000) as f64;
        let edit = match rng.next(18) {
            9 => json!({"op": "set", "path": "tracks.bass.effects.0.threshold_db", "value": -(n / 20.0)}),
            10 => json!({"op": "set", "path": "returns.plate.effects.0.mix_percent", "value": n / 10.0}),
            11 => json!({"op": "set", "path": "returns.plate.effects.0.decay_seconds", "value": 0.3 + n / 2000.0}),
            12 => json!({"op": "effect.bypass", "effect": "tracks.bass.effects.0", "bypass": n < 500.0}),
            13 => json!({"op": "effect.add", "owner": "tracks.drums", "type": "filter", "mode": "highpass", "cutoff_hz": 40.0 + n}),
            14 => json!({"op": "effect.remove", "effect": "tracks.drums.effects.0"}),
            15 => json!({"op": "send.set", "track": "drums", "to": "plate", "gain_db": -(n / 30.0)}),
            16 => json!({"op": "point.add", "owner": "tracks.bass", "param": "gain_db", "at": rng.next(32), "value": -(n / 100.0)}),
            17 => json!({"op": "effect.add", "owner": "master", "type": "limiter", "index": 0}),
            0 => json!({"op": "set", "path": "tracks.drums.gain_db", "value": -(n / 50.0)}),
            1 => json!({"op": "set", "path": "tracks.bass.pan", "value": n / 1000.0 - 0.5}),
            2 => json!({"op": "toggle", "path": "tracks.drums.mute"}),
            3 => json!({"op": "clip.move", "clip": "tracks.bass.clips.1", "at": rng.next(24)}),
            4 => json!({"op": "pad.set", "track": "bass", "pad": "t", "transpose": rng.next(13) as f64 - 6.0}),
            5 => json!({"op": "set", "path": "session.tempo", "value": 90 + rng.next(60)}),
            6 => json!({"op": "clip.add", "track": "drums", "pattern": "beat", "at": rng.next(28)}),
            7 => json!({"op": "set", "path": "session.master_gain_db", "value": -(n / 100.0)}),
            _ => json!({"op": "undo"}),
        };
        let result = match edit["op"].as_str() {
            Some("undo") => s.undo(Origin::Agent, false).map(|_| ()),
            _ => s.edit(&cmd(edit), Origin::Agent, None, None).map(|_| ()),
        };
        if result.is_ok() {
            applied += 1;
            let p = Arc::new(compile_cached(s.project(), s.dir(), &mut cache).unwrap());
            control.load(p).unwrap();
        }
        control.collect();
        std::thread::sleep(Duration::from_millis(10));
    }
    done.store(true, Relaxed);
    audio.join().expect("the audio thread allocated or panicked");
    control.collect();
    let (blocks, over) = (blocks.load(Relaxed), over.load(Relaxed));
    println!(
        "{applied} edits, {blocks} blocks, {over} over budget, worst {:.3} ms",
        worst.load(Relaxed) as f64 / 1e6
    );
    assert!(applied > 100, "{applied}");
    assert_eq!(control.shared().leaked.load(Relaxed), 0);
    // Rendering time alone; a preempted test thread can overrun rarely.
    assert!(over * 100 <= blocks, "{over} of {blocks} blocks over budget");
}
