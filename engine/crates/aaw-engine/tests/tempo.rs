//! A song whose tempo steps: each hit on the frame the map gives its beat,
//! played from any beat as it renders, and a clip with a tempo of its own
//! following each step while one without plays its file as it is.

mod common;

use aaw_engine::program::{compile, Program};
use aaw_engine::render::{Frame, Renderer};
use common::write_sample;
use num_rational::BigRational;
use std::path::Path;
use std::sync::Arc;

const RATE: i64 = 48000;

/// 120 BPM, then 150 from bar 17 (beat 64), as the master's tempo lane.
const LANE: &str = "master:
  automation:
  - param: tempo
    points:
    - {at: 0, value: 120.0, curve: hold}
    - {at: 64, value: 150.0, curve: hold}
";

/// A click a beat for 96 beats on a Drum Rack.
fn clicks() -> String {
    let notes: Vec<String> = (0..96).map(|b| format!("{{pitch: 36, at: {b}, duration: 1/4, velocity: 127}}")).collect();
    format!(
        "session: {{tempo: 120, sample_rate: {RATE}, length_beats: 100, end_fade_ms: 0}}
samples:
  click: {{path: click.wav}}
tracks:
- id: drums
  type: midi
  instrument: {{sampler: {{drum_rack: true, pads: {{c: {{sample: click, attack_ms: 0}}}}, map: [{{notes: 36, pad: c}}]}}}}
  clips:
  - {{id: beat, length_beats: 96, notes: [{}]}}
",
        notes.join(", ")
    )
}

fn write_click(dir: &Path) {
    let mut click = vec![[0.0f32; 2]; 64];
    click[0] = [0.5, 0.5];
    write_sample(dir, "click.wav", click, RATE as u32);
}

fn compiled(dir: &Path, text: &str) -> (aaw_model::Project, Arc<Program>) {
    let path = dir.join("song.yaml");
    std::fs::write(&path, text).unwrap();
    let p = aaw_model::load(&path, true).unwrap();
    let program = Arc::new(compile(&p, dir).unwrap());
    (p, program)
}

/// The mix from frame `from` to the song's end.
fn mix(program: &Arc<Program>, from: usize) -> Vec<Frame> {
    let mut r = Renderer::new(program.clone(), from, 1024);
    let mut out = Vec::new();
    let mut block = vec![[0.0; 2]; 1024];
    while !r.finished() {
        let n = 1024.min(program.total + program.latency - r.position());
        let heard = r.heard();
        r.render(&mut block[..n], |_, _, _| {});
        for (k, f) in block[..n].iter().enumerate() {
            if (from as i64..program.total as i64).contains(&(heard + k as i64)) {
                out.push(*f);
            }
        }
    }
    out
}

/// The frames where a click starts: a sample after silence.
fn onsets(x: &[Frame], threshold: f64) -> Vec<usize> {
    let mut out = Vec::new();
    let mut quiet = usize::MAX;
    for (i, f) in x.iter().enumerate() {
        if f[0].abs() > threshold {
            if quiet > 200 {
                out.push(i);
            }
            quiet = 0;
        } else {
            quiet = quiet.saturating_add(1);
        }
    }
    out
}

fn beat(n: i64) -> BigRational {
    BigRational::from_integer(n.into())
}

#[test]
fn each_hit_is_on_the_frame_the_map_gives_its_beat() {
    let dir = tempfile::tempdir().unwrap();
    write_click(dir.path());
    let (p, program) = compiled(dir.path(), &format!("{}{LANE}", clicks()));
    assert_eq!(p.schema_version(), 3);
    let map = p.tempo_map();
    let want: Vec<usize> = (0..96).map(|b| map.frame(&beat(b), RATE) as usize).collect();
    // A beat is 24000 frames at 120 and 19200 at 150.
    assert_eq!(want[64], 64 * 24000);
    assert_eq!(want[65] - want[64], 19200);
    let whole = mix(&program, 0);
    assert_eq!(onsets(&whole, 1e-6), want);
    assert_eq!(program.total, map.frame(&beat(100), RATE) as usize);
    // Played from any beat, the song is the same frames as its render.
    for from in [want[63] + 5000, want[64], want[70] + 17] {
        assert_eq!(mix(&program, from), whole[from..].to_vec(), "from frame {from}");
    }
}

#[test]
fn a_song_without_a_lane_keeps_one_tempo() {
    let dir = tempfile::tempdir().unwrap();
    write_click(dir.path());
    let (p, program) = compiled(dir.path(), &clicks());
    assert_eq!(p.schema_version(), 2);
    assert_eq!(program.map.constant_tempo(), Some(120.0));
    let want: Vec<usize> = (0..96).map(|b| aaw_model::frame(&beat(b), 120.0, RATE) as usize).collect();
    assert_eq!(onsets(&mix(&program, 0), 1e-6), want);
}

/// A click every half second for 48 seconds: a file at 120 BPM.
fn write_click_track(dir: &Path) {
    let mut x = vec![[0.0f32; 2]; 48 * RATE as usize];
    for k in 0..96 {
        x[k * RATE as usize / 2] = [0.8, 0.8];
    }
    write_sample(dir, "track.wav", x, RATE as u32);
}

fn audio_song(clip: &str) -> String {
    format!(
        "session: {{tempo: 120, sample_rate: {RATE}, length_beats: 112, end_fade_ms: 0}}
samples:
  track: {{path: track.wav}}
tracks:
- id: song
  audio: [{clip}]
master:
  automation:
  - param: tempo
    points:
    - {{at: 0, value: 120.0, curve: hold}}
    - {{at: 32, value: 150.0, curve: hold}}
"
    )
}

/// The frame of each peak: the largest sample of each click, which
/// resampling spreads over a few frames.
fn peaks(x: &[Frame]) -> Vec<usize> {
    let starts = onsets(x, 0.05);
    starts
        .iter()
        .map(|&s| (s..(s + 64).min(x.len())).max_by(|a, b| x[*a][0].abs().total_cmp(&x[*b][0].abs())).unwrap())
        .collect()
}

#[test]
fn a_clip_with_its_own_tempo_follows_each_step() {
    let dir = tempfile::tempdir().unwrap();
    write_click_track(dir.path());
    let (p, program) = compiled(dir.path(), &audio_song("{sample: track, source_bpm: 120, stretch: repitch, fade_in_ms: 0}"));
    let map = p.tempo_map();
    let found = peaks(&mix(&program, 0));
    let want: Vec<i64> = (0..96).map(|b| map.frame(&beat(b), RATE)).take(found.len()).collect();
    assert_eq!(found.len(), 96, "{found:?}");
    for (b, (got, want)) in found.iter().zip(&want).enumerate() {
        assert!((*got as i64 - want).abs() <= 2, "beat {b}: the click is at {got}, the beat at {want}");
    }
}

#[test]
fn a_clip_without_a_tempo_plays_its_file_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    write_click_track(dir.path());
    let (_, program) = compiled(dir.path(), &audio_song("{sample: track, fade_in_ms: 0}"));
    let found = onsets(&mix(&program, 0), 0.05);
    let want: Vec<usize> = (0..96).map(|k| k * RATE as usize / 2).collect();
    assert_eq!(found, want);
    // Its 48 seconds are 32 beats at 120, then 80 at 150: the song's 112.
    let p = aaw_model::load(&dir.path().join("song.yaml"), true).unwrap();
    let end = p.tracks[0].audio[0].sound_end(&p.tempo_map(), Some(48.0)).unwrap();
    assert!((end - 112.0).abs() < 1e-9, "{end}");
    assert_eq!(program.total, 48 * RATE as usize);
}

#[test]
fn a_stretched_clip_keeps_each_beat_within_5_ms_through_the_step() {
    let dir = tempfile::tempdir().unwrap();
    write_click_track(dir.path());
    let (p, program) = compiled(dir.path(), &audio_song("{sample: track, source_bpm: 120, stretch: preserve_pitch, fade_in_ms: 0}"));
    let map = p.tempo_map();
    let found = peaks(&mix(&program, 0));
    assert_eq!(found.len(), 96, "{found:?}");
    for (b, got) in found.iter().enumerate() {
        let want = map.frame(&beat(b as i64), RATE);
        assert!((*got as i64 - want).abs() <= RATE / 200, "beat {b}: the click is at {got}, the beat at {want}");
    }
}
