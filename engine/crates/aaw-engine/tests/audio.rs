//! Audio clips as voices: where a clip's audio lands, its lead before the beat,
//! its fades and their curves, and how two clips meet.

mod common;

use aaw_engine::program::{compile, Voice};
use common::write_sample;
use std::path::Path;

const RATE: usize = 48000;

/// The voices of the track of a song at 120 BPM whose audio clips are `clips`.
/// `ramp` is two seconds whose every sample is its own time in seconds; `ones`
/// is two seconds of full level, half as much on the right.
fn voices(dir: &Path, tempo: f64, clips: &str) -> Result<Vec<Voice>, String> {
    let ramp: Vec<[f32; 2]> = (0..2 * RATE).map(|i| [i as f32 / RATE as f32; 2]).collect();
    write_sample(dir, "ramp.wav", ramp, RATE as u32);
    write_sample(dir, "ones.wav", vec![[1.0, 0.5]; 2 * RATE], RATE as u32);
    let yaml = format!(
        "session: {{tempo: {tempo}, length_beats: 16}}\nsamples: {{ramp: {{path: ramp.wav}}, ones: {{path: ones.wav}}}}\ntracks:\n- id: song\n  pads: {{}}\n  audio: {clips}\n"
    );
    let path = dir.join("song.yaml");
    std::fs::write(&path, yaml).unwrap();
    let project = aaw_model::load(&path, true).map_err(|e| e.to_string())?;
    Ok(compile(&project, dir)?.tracks[0].voices.to_vec())
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
fn a_clip_outside_its_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let error = voices(dir.path(), 120.0, "[{sample: ramp, source_start_seconds: 5}]").unwrap_err();
    assert_eq!(error, "song: ramp: audio clip outside sample");
    // An end past the file's is the file's end.
    let v = &voices(dir.path(), 120.0, "[{sample: ramp, source_start_seconds: 1, source_end_seconds: 9}]").unwrap()[0];
    assert_eq!(v.length, RATE);
}
