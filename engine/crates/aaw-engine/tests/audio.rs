//! Audio clips as voices: where a clip's audio lands, its lead before the beat,
//! its fades and their curves, and how two clips meet.

mod common;

use aaw_engine::program::{compile, Program, Voice};
use aaw_engine::render::{Frame, Renderer};
use common::write_sample;
use std::path::Path;
use std::sync::Arc;

const RATE: usize = 48000;

/// A song at `tempo` of one track whose audio clips are `clips`, compiled.
/// `ramp` is two seconds whose every sample is its own time in seconds; `ones`
/// is two seconds of full level, half as much on the right.
fn program(dir: &Path, tempo: f64, clips: &str) -> Result<Program, String> {
    let ramp: Vec<[f32; 2]> = (0..2 * RATE).map(|i| [i as f32 / RATE as f32; 2]).collect();
    write_sample(dir, "ramp.wav", ramp, RATE as u32);
    write_sample(dir, "ones.wav", vec![[1.0, 0.5]; 2 * RATE], RATE as u32);
    let yaml = format!(
        "session: {{tempo: {tempo}, length_beats: 16}}\nsamples: {{ramp: {{path: ramp.wav}}, ones: {{path: ones.wav}}}}\ntracks:\n- id: song\n  pads: {{}}\n  audio: {clips}\n"
    );
    let path = dir.join("song.yaml");
    std::fs::write(&path, yaml).unwrap();
    let project = aaw_model::load(&path, true).map_err(|e| e.to_string())?;
    compile(&project, dir)
}

/// The voices of the track of a song at `tempo` whose audio clips are `clips`.
fn voices(dir: &Path, tempo: f64, clips: &str) -> Result<Vec<Voice>, String> {
    Ok(program(dir, tempo, clips)?.tracks[0].voices.to_vec())
}

/// The mix of the song, rendered.
fn rendered(dir: &Path, tempo: f64, clips: &str) -> Vec<Frame> {
    let p = Arc::new(program(dir, tempo, clips).unwrap());
    let mut r = Renderer::new(p.clone(), 0, 4096);
    let mut out = vec![[0.0; 2]; p.total];
    for chunk in out.chunks_mut(4096) {
        r.render(chunk, |_, _, _| {});
    }
    out
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn a_clip_puts_its_start_in_the_file_on_its_beat() {
    let dir = tempfile::tempdir().unwrap();
    // Beat 2 is the first second. Half a second into the file plays there, and
    // the clip starts 5 ms sooner with the file 5 ms sooner.
    let v = &voices(dir.path(), 120.0, "[{sample: ramp, at: 2, source_start_seconds: 0.5, lead_ms: 5}]").unwrap()[0];
    assert_eq!(v.start, RATE as i64 - 240);
    assert!(near(v.sample(240, 0), 0.5) && near(v.sample(240, 1), 0.5));
    assert!(near(v.sample(240 + 4800, 0), 0.6));
    // It plays to the end of the file, at full level on both sides.
    assert_eq!(v.length, 2 * RATE - 24000 + 240);
    assert_eq!((v.pan, v.velocity, v.gain), ([1.0, 1.0], 1.0, 1.0));
    // A lead cannot start a clip before its file does, or before the song.
    let early = &voices(dir.path(), 120.0, "[{sample: ramp, at: 2, source_start_seconds: 0.002, lead_ms: 5}]").unwrap()[0];
    assert_eq!((early.start, early.sample(0, 0)), (RATE as i64 - 96, 0.0));
    let first = &voices(dir.path(), 120.0, "[{sample: ramp, source_start_seconds: 0.5, lead_ms: 5}]").unwrap()[0];
    assert!(first.start == 0 && near(first.sample(48, 0), 0.501));
    // A fractional beat, and a tempo that puts beats between frames.
    let third = &voices(dir.path(), 97.3, "[{sample: ramp, at: '7/3', source_start_seconds: 0.25}]").unwrap()[0];
    assert_eq!(third.start, (7.0 / 3.0 * 60.0 / 97.3 * RATE as f64).round() as i64);
    assert!(near(third.sample(48, 0), 0.251));
}

#[test]
fn fades_take_the_clip_s_curve_and_its_gain() {
    let dir = tempfile::tempdir().unwrap();
    let clip = |curve: &str| format!("[{{sample: ones, source_end_seconds: 1, fade_in_ms: 10, fade_out_ms: 20, gain_db: -6, fade_curve: {curve}}}]");
    let gain = 10f64.powf(-6.0 / 20.0);
    let power = &voices(dir.path(), 120.0, &clip("equal_power")).unwrap()[0];
    let line = &voices(dir.path(), 120.0, &clip("linear")).unwrap()[0];
    // Halfway in: a quarter of a sine is at 0.707, a line at a half.
    assert!(near(power.sample(240, 0), gain * (0.5f64).sqrt()) && near(line.sample(240, 0), gain * 0.5));
    assert!(near(power.sample(240, 1), 0.5 * gain * (0.5f64).sqrt()));
    assert!(near(power.sample(1000, 0), gain) && near(line.sample(1000, 0), gain));
    // The fade out follows the clip's end: a second, then 20 ms more.
    assert_eq!((power.length, power.release), (RATE + 960, 960));
    assert!(near(line.sample(RATE - 1, 0), gain));
    assert!(near(line.sample(RATE + 479, 0), gain * 0.5) && near(power.sample(RATE + 479, 0), gain * (0.5f64).sqrt()));
    assert!(near(line.sample(RATE + 959, 0), 0.0));
}

#[test]
fn a_clip_leaves_from_where_the_next_one_starts() {
    let dir = tempfile::tempdir().unwrap();
    // The first ends on beat 2, where the second begins 5 ms early. The first
    // starts to fade there, not at its own end, so the two cross before the beat.
    let both = voices(
        dir.path(),
        120.0,
        "[{sample: ones, source_end_seconds: 1, fade_out_ms: 12, fade_curve: linear}, {sample: ones, at: 2, source_start_seconds: 0.5, lead_ms: 5, fade_in_ms: 12, fade_curve: linear}]",
    )
    .unwrap();
    let (a, b) = (&both[0], &both[1]);
    assert_eq!(b.start, RATE as i64 - 240);
    assert_eq!(a.length - a.release, b.start as usize);
    // Two lines across the same audio sum to it all the way through.
    for i in 0..576 {
        let sum = a.sample(b.start as usize + i, 0) + b.sample(i, 0);
        assert!((sum - 1.0).abs() < 0.002, "{i}: {sum}");
    }
    // Alone, it fades from its own end.
    let alone = &voices(dir.path(), 120.0, "[{sample: ones, source_end_seconds: 1, fade_out_ms: 12}]").unwrap()[0];
    assert_eq!(alone.length - alone.release, RATE);
    // Equal power keeps the power of unlike audio; of the same audio it rises.
    let power = voices(
        dir.path(),
        120.0,
        "[{sample: ones, source_end_seconds: 1, fade_out_ms: 12}, {sample: ones, at: 2, source_start_seconds: 0.5, lead_ms: 5, fade_in_ms: 12}]",
    )
    .unwrap();
    let (a, b) = (&power[0], &power[1]);
    let (x, y) = (a.sample(b.start as usize + 288, 0), b.sample(288, 0));
    assert!((x * x + y * y - 1.0).abs() < 0.01 && x + y > 1.4);
}

#[test]
fn a_clip_follows_the_tempo_at_its_own_pitch_or_not() {
    let dir = tempfile::tempdir().unwrap();
    let clip = |stretch: &str| format!("[{{sample: ramp, source_end_seconds: 1.5, source_bpm: 120, fade_out_ms: 0, stretch: {stretch}}}]");
    // A second and a half of the file at 126 BPM for 120: 1.05 times as fast.
    let expected = (1.5 * RATE as f64 / 1.05).round() as i64;
    for stretch in ["repitch", "preserve_pitch"] {
        let v = &voices(dir.path(), 126.0, &clip(stretch)).unwrap()[0];
        assert!((v.length as i64 - expected).abs() <= 2, "{stretch}: {} for {expected}", v.length);
    }
    // At its own tempo both are the file as it is.
    let same = &voices(dir.path(), 120.0, &clip("preserve_pitch")).unwrap()[0];
    assert!(same.length == RATE * 3 / 2 && near(same.sample(RATE, 0), 1.0));
}

#[test]
fn a_looped_clip_plays_as_copies_of_its_first_beats_and_wraps_without_a_click() {
    let dir = tempfile::tempdir().unwrap();
    // A second of the file, from 0.25 s, looped every two beats for seven at
    // 120 BPM: three repetitions and a half.
    let looped = "[{sample: ones, source_start_seconds: 0.25, loop_beats: 2, length_beats: 7}]";
    let copies = "[{sample: ones, source_start_seconds: 0.25, source_end_seconds: 1.25}, {sample: ones, at: 2, source_start_seconds: 0.25, source_end_seconds: 1.25}, {sample: ones, at: 4, source_start_seconds: 0.25, source_end_seconds: 1.25}, {sample: ones, at: 6, source_start_seconds: 0.25, source_end_seconds: 0.75}]";
    let (a, b) = (voices(dir.path(), 120.0, looped).unwrap(), voices(dir.path(), 120.0, copies).unwrap());
    assert_eq!(a.len(), 4);
    let shape = |v: &Voice| (v.start, v.length, v.attack, v.release, v.sample(100, 0).to_bits());
    assert_eq!(a.iter().map(shape).collect::<Vec<_>>(), b.iter().map(shape).collect::<Vec<_>>());
    assert_eq!(a.iter().map(|v| v.start).collect::<Vec<_>>(), [0, 48000, 96000, 144000]);
    // The last repetition is cut off where the clip ends, and the others leave
    // at the wrap, where the next starts, and fade out over the clip's 8 ms.
    assert_eq!((a[3].length, a[0].length - a[0].release, a[0].release), (24000 + 384, 48000, 384));
    assert_eq!(rendered(dir.path(), 120.0, looped), rendered(dir.path(), 120.0, copies));
    // On a ramp, the wrap jumps back to the loop's start: with the clip's fades
    // the mix moves a little a frame, without them it jumps (by a second of
    // the ramp, at the new song's -6 dB).
    let wrap = 48000;
    let step = |clips: &str| {
        let out = rendered(dir.path(), 120.0, clips);
        out[wrap - 500..wrap + 500].windows(2).map(|w| (w[1][0] - w[0][0]).abs()).fold(0.0, f64::max)
    };
    assert!(step("[{sample: ramp, source_start_seconds: 0.25, loop_beats: 2, length_beats: 6}]") < 0.05);
    assert!(step("[{sample: ramp, source_start_seconds: 0.25, loop_beats: 2, length_beats: 6, fade_in_ms: 0, fade_out_ms: 0}]") > 0.4);
    // A loop of a file at another tempo is so many beats of the file and
    // wraps on the song's beat: two beats at 100 BPM, 1.2 s, in a second at 120.
    let stretched = voices(dir.path(), 120.0, "[{sample: ramp, source_bpm: 100, loop_beats: 2, length_beats: 6, fade_in_ms: 0, fade_out_ms: 0}]").unwrap();
    assert_eq!(stretched.iter().map(|v| v.start).collect::<Vec<_>>(), [0, 48000, 96000]);
    assert!(stretched.iter().all(|v| (v.length as i64 - 48000).abs() <= 2 && near(v.sample(0, 0), 0.0)));
    assert!((stretched[1].sample(24000, 0) - 0.6).abs() < 0.01, "{}", stretched[1].sample(24000, 0));
}

#[test]
fn a_clip_outside_its_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let error = voices(dir.path(), 120.0, "[{sample: ramp, source_start_seconds: 5}]").unwrap_err();
    assert_eq!(error, "song: ramp: audio clip outside sample");
    // An end past the file's is the file's end.
    let v = &voices(dir.path(), 120.0, "[{sample: ramp, source_start_seconds: 1, source_end_seconds: 9}]").unwrap()[0];
    assert_eq!(v.length, RATE);
}
