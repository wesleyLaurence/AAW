//! Note clips through the engine, with generated audio: a phrase of chords,
//! overlapping and repeated notes sounds as the same hits written as a
//! pattern do, a gated note releases at its note-off or its clip's end, the
//! level follows velocity, and playback equals a render.

mod common;

use aaw_engine::player::channel;
use aaw_engine::program::compile;
use aaw_engine::render::{Frame, Renderer};
use common::write_sample;
use std::f64::consts::PI;
use std::path::Path;
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

const HEAD: &str = r#"
session: {tempo: 120, length_beats: 16, sample_rate: 48000, master_gain_db: 0, end_fade_ms: 0}
samples:
  tone: {path: tone.wav, root_note: A3}
  level: {path: level.wav}
"#;

const PADS: &str = "{t: {sample: tone, mode: gate, attack_ms: 2, release_ms: 30}, d: {sample: level, mode: gate, release_ms: 10}}";

/// Chords, a note over another, the same pitch twice at once, and a hit of a
/// pad that is not pitched.
const NOTES: &str = r#"
    - {pitch: 48, duration: 1}
    - {pitch: 52, duration: 1, velocity: 64}
    - {pitch: 55, duration: 1}
    - {pitch: 60, at: 1/3, duration: 2/3}
    - {pitch: 60, at: 1/2, duration: 1}
    - {pitch: 43, at: 2, duration: 1.5}
    - {pitch: 30, at: 3, duration: 1/4, velocity: 30}"#;

/// The same hits as a pattern's events, in the order the notes start.
const EVENTS: &str = r#"
    - {at: 0, pad: t, note: C3, duration: 1}
    - {at: 0, pad: t, note: E3, duration: 1, velocity: 64}
    - {at: 0, pad: t, note: G3, duration: 1}
    - {at: 1/3, pad: t, note: C4, duration: 2/3}
    - {at: 1/2, pad: t, note: C4, duration: 1}
    - {at: 2, pad: t, note: G2, duration: 1.5}
    - {at: 3, pad: d, duration: 1/4, velocity: 30}"#;

fn write_audio(dir: &Path) {
    let tone: Vec<[f32; 2]> = (0..48000)
        .map(|i| {
            let x = (2.0 * PI * 220.0 * i as f64 / 48000.0).sin() * 0.3;
            [x as f32, (x * 0.5) as f32]
        })
        .collect();
    write_sample(dir, "tone.wav", tone, 48000);
    write_sample(dir, "level.wav", vec![[0.5, 0.5]; 48000], 48000);
}

fn midi_song(extra: &str) -> String {
    format!(
        "{HEAD}tracks:\n- id: keys\n  type: midi\n  instrument:\n    sampler:\n      pads: {PADS}\n      map: [{{notes: [40, 80], pad: t, pitched: true}}, {{notes: 30, pad: d}}]\n  clips:\n  - at: 1\n    length_beats: 4\n    notes:{NOTES}\n{extra}"
    )
}

fn compiled(dir: &Path, yaml: &str) -> Arc<aaw_engine::program::Program> {
    let path = dir.join("song.yaml");
    std::fs::write(&path, yaml).unwrap();
    let p = aaw_model::load(&path, true).unwrap_or_else(|e| panic!("{e}"));
    Arc::new(compile(&p, dir).unwrap())
}

fn render(p: &Arc<aaw_engine::program::Program>) -> Vec<Frame> {
    let mut r = Renderer::new(p.clone(), 0, 4096);
    let mut out = vec![[0.0; 2]; p.total];
    for chunk in out.chunks_mut(4096) {
        r.render(chunk, |_, _, _| {});
    }
    out
}

#[test]
fn notes_sound_as_the_same_hits_written_as_a_pattern() {
    let dir = tempfile::tempdir().unwrap();
    write_audio(dir.path());
    let notes = render(&compiled(dir.path(), &midi_song("")));
    let pattern = format!("{HEAD}patterns:\n  p:\n    length_beats: 4\n    events:{EVENTS}\ntracks:\n- id: keys\n  pads: {PADS}\n  clips: [{{pattern: p, at: 1}}]\n");
    let events = render(&compiled(dir.path(), &pattern));
    assert!(notes.iter().any(|f| f[0].abs() > 0.1), "the phrase is silent");
    assert_eq!(notes, events);
}

#[test]
fn a_gated_note_releases_at_its_note_off_or_its_clips_end_at_its_velocity() {
    let dir = tempfile::tempdir().unwrap();
    write_audio(dir.path());
    // A clip a beat long holding a note four beats long, and a short soft hit.
    let extra = "  - at: 8\n    length_beats: 1\n    notes: [{pitch: 30, duration: 4}]\n  - at: 12\n    length_beats: 2\n    notes: [{pitch: 30, duration: 1/4, velocity: 30}]\n";
    let out = render(&compiled(dir.path(), &midi_song(extra)));
    let beat = 24000;
    let release = 480;
    // Velocity 30 of 127, linear in level.
    let level = 0.5 * 30.0 / 127.0;
    let held = 12 * beat + 3000;
    assert!((out[held][0] - level).abs() < 1e-6, "{} against {level}", out[held][0]);
    // A quarter of a beat later, the note-off, and the release after it.
    let off = 12 * beat + beat / 4;
    assert!(out[off + release / 2][0] > 0.0 && out[off + release / 2][0] < level);
    assert!(out[off + release + 1..13 * beat].iter().all(|f| f[0] == 0.0));
    // The long note is let go where its clip ends, on beat 9.
    assert!((out[8 * beat + 100][0] - 0.5 * 100.0 / 127.0).abs() < 1e-6);
    assert!(out[9 * beat - 1][0] > 0.3);
    assert!(out[9 * beat + release + 1..10 * beat].iter().all(|f| f[0] == 0.0));
}

#[test]
fn a_sample_with_no_root_note_plays_as_it_is_at_middle_c() {
    let dir = tempfile::tempdir().unwrap();
    write_audio(dir.path());
    let song = midi_song("");
    let unrooted = render(&compiled(dir.path(), &song.replace("tone: {path: tone.wav, root_note: A3}", "tone: {path: tone.wav}")));
    let middle_c = render(&compiled(dir.path(), &song.replace("root_note: A3", "root_note: C4")));
    assert!(unrooted.iter().any(|f| f[0].abs() > 0.1), "the phrase is silent");
    assert_eq!(unrooted, middle_c);
}

#[test]
fn without_an_instrument_the_track_is_silent_and_swapping_one_is_heard() {
    let dir = tempfile::tempdir().unwrap();
    write_audio(dir.path());
    let song = midi_song("");
    let (start, end) = (song.find("  instrument:").unwrap(), song.find("  clips:").unwrap());
    let bare = render(&compiled(dir.path(), &format!("{}  instrument: null\n{}", &song[..start], &song[end..])));
    assert!(bare.iter().all(|f| *f == [0.0; 2]));
    // The same notes, every one mapped to the level pad as it is.
    let flat = song.replace("map: [{notes: [40, 80], pad: t, pitched: true}, {notes: 30, pad: d}]", "map: [{notes: [0, 127], pad: d}]");
    let out = render(&compiled(dir.path(), &flat));
    let chord = 0.5 * (100.0 + 64.0 + 100.0) / 127.0;
    assert!((out[24000 + 100][0] - chord).abs() < 1e-6, "the chord's three notes: {}", out[24000 + 100][0]);
}

#[test]
fn playing_a_midi_track_equals_its_render() {
    let dir = tempfile::tempdir().unwrap();
    write_audio(dir.path());
    let p = compiled(dir.path(), &midi_song(""));
    let (mut control, mut player) = channel(p.clone());
    control.play(0).unwrap();
    let mut out = vec![[0.0; 2]; p.total];
    assert_no_alloc::assert_no_alloc(|| {
        for chunk in out.chunks_mut(128) {
            player.render(chunk);
        }
    });
    assert_eq!(out, render(&p));
}
