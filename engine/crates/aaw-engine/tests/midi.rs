//! Note clips through the engine, with generated audio: a phrase of chords,
//! overlapping and repeated notes sounds as the same hits written as a
//! pattern do, a gated note releases at its note-off or its clip's end, the
//! level follows velocity, playback equals a render, and a note previewed
//! through the sampler sounds as the same note in a clip does.

mod common;

use aaw_engine::player::channel;
use aaw_engine::program::{compile, compile_cached, preview_voice, Cache};
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
fn a_looped_clip_renders_as_the_clip_and_its_copies() {
    let dir = tempfile::tempdir().unwrap();
    write_audio(dir.path());
    let track = "tracks:\n- id: keys\n  type: midi\n  instrument:\n    sampler:\n      pads: {PADS}\n      map: [{notes: [40, 80], pad: t, pitched: true}, {notes: 30, pad: d}]\n  clips:\n".replace("{PADS}", PADS);
    // Four beats looped to fourteen: three repetitions and a half. A note held
    // across the wrap is cut there, and one that starts at the loop's end is
    // kept and does not play.
    let held = "    - {pitch: 30, at: 3.5, duration: 2}\n";
    let looped = format!("{HEAD}{track}  - at: 0\n    length_beats: 14\n    loop_beats: 4\n    notes:{NOTES}\n{held}    - {{pitch: 60, at: 4, duration: 1}}\n");
    let copies: String = [0, 4, 8, 12]
        .iter()
        .map(|at| format!("  - at: {at}\n    length_beats: {}\n    notes:{NOTES}\n{held}", if *at == 12 { 2 } else { 4 }))
        .collect();
    let copies = format!("{HEAD}{track}{copies}");
    // The schedule is the same notes at the same places.
    let notes = |yaml: &str| {
        let p = aaw_model::parse(yaml).unwrap_or_else(|e| panic!("{e}"));
        aaw_model::schedule::track_notes(&p, p.tracks[0].midi.as_ref().unwrap())
            .iter()
            .map(|n| format!("{}:{}:{}:{}", n.at, n.beats, n.pitch, n.velocity))
            .collect::<Vec<_>>()
    };
    assert_eq!(notes(&looped), notes(&copies));
    // The held note is cut at the wrap, half a beat long, in each repetition
    // but the last, where it does not start; the note at the loop's end never plays.
    let scheduled = notes(&looped);
    assert_eq!(scheduled.iter().filter(|n| n.ends_with(":1/2:30:100")).count(), 3);
    assert!(!scheduled.iter().any(|n| n.ends_with(":60:100") && n.starts_with("4:")));
    assert_eq!(scheduled.len(), 3 * 8 + 5);
    let (a, b) = (compiled(dir.path(), &looped), compiled(dir.path(), &copies));
    let (x, y) = (render(&a), render(&b));
    assert!(x.iter().any(|f| f[0].abs() > 0.1), "the loop is silent");
    let differs = x.iter().zip(&y).position(|(p, q)| p != q);
    assert_eq!(differs.map(|i| (i, x[i], y[i])), None);
    assert_eq!(x.len(), y.len());
    // The second repetition sounds like the first, once the cut note's 10 ms
    // release from the wrap is over.
    let beat = 24000;
    assert_eq!(x[1000..3 * beat], x[4 * beat + 1000..7 * beat]);
    // Locating into the fourth repetition plays the fourth: the same as the copies from there.
    let from = 12 * beat + 100;
    let play = |p: &Arc<aaw_engine::program::Program>| {
        let mut r = Renderer::new(p.clone(), from, 4096);
        let mut out = vec![[0.0; 2]; p.total - from];
        for chunk in out.chunks_mut(4096) {
            r.render(chunk, |_, _, _| {});
        }
        out
    };
    assert_eq!(play(&a), play(&b));
    assert!(play(&a).iter().any(|f| f[0].abs() > 0.1));
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

#[test]
fn a_note_previewed_through_the_sampler_sounds_while_stopped_and_through_an_edit() {
    let dir = tempfile::tempdir().unwrap();
    write_audio(dir.path());
    let path = dir.path().join("song.yaml");
    std::fs::write(&path, midi_song("")).unwrap();
    let mut project = aaw_model::load(&path, true).unwrap();
    let mut cache = Cache::default();
    let p = Arc::new(compile_cached(&project, dir.path(), &mut cache).unwrap());
    let (mut control, mut player) = channel(p.clone());
    let pull = |player: &mut aaw_engine::player::Player, n: usize| {
        let mut out = vec![[0.0; 2]; n];
        assert_no_alloc::assert_no_alloc(|| {
            for chunk in out.chunks_mut(128) {
                player.render(chunk);
            }
        });
        out
    };
    let peak = |x: &[Frame]| x.iter().fold(0.0f64, |m, f| m.max(f[0].abs()));
    // A pitch the map leaves silent has no voice.
    assert!(preview_voice(&project, dir.path(), &mut cache, "keys", 20, 100, 1.0).unwrap().is_none());
    assert!(preview_voice(&project, dir.path(), &mut cache, "nobody", 60, 100, 1.0).is_err());
    // The level pad, gated, held a quarter of a beat at full velocity: as a note in a clip.
    assert_eq!(peak(&pull(&mut player, 4800)), 0.0);
    let voice = preview_voice(&project, dir.path(), &mut cache, "keys", 30, 127, 0.25).unwrap().unwrap();
    control.preview_voice(0, voice).unwrap();
    let out = pull(&mut player, 24000);
    assert!((out[100][0] - 0.5).abs() < 1e-6, "{}", out[100][0]);
    assert!(out[6000 + 480 + 1..].iter().all(|f| f[0] == 0.0), "released at its note-off");
    // A pitched note: the tone's A3 played as A4.
    let voice = preview_voice(&project, dir.path(), &mut cache, "keys", 69, 100, 2.0).unwrap().unwrap();
    control.preview_voice(0, voice).unwrap();
    let out = pull(&mut player, 12000);
    let crossings = out[1000..11000].windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count();
    assert!((crossings as f64 / (10000.0 / 48000.0) - 440.0).abs() < 10.0, "{crossings} crossings of an A");
    // A fader moved while it sounds: the same structure takes over and the note carries on.
    project.tracks[0].gain_db = -6.0;
    let edited = Arc::new(compile_cached(&project, dir.path(), &mut cache).unwrap());
    assert_eq!(edited.structure, p.structure);
    control.load(edited).unwrap();
    assert!(peak(&pull(&mut player, 4800)[2400..]) > 0.05, "still sounding across the take-over");
    // An effect added: another structure, swapped through the dip, and the note is still there.
    std::fs::write(&path, midi_song("").replace("  clips:", "  gain_db: -6\n  effects: [{type: filter, mode: lowpass, cutoff_hz: 8000}]\n  clips:")).unwrap();
    let project = aaw_model::load(&path, true).unwrap();
    let reshaped = Arc::new(compile_cached(&project, dir.path(), &mut cache).unwrap());
    assert_ne!(reshaped.structure, p.structure);
    control.load(reshaped).unwrap();
    assert!(peak(&pull(&mut player, 4800)[2400..]) > 0.05, "the preview carries on after the swap");
    // More previews than slots: the oldest gives its place, freed by the host.
    for _ in 0..20 {
        let voice = preview_voice(&project, dir.path(), &mut cache, "keys", 30, 64, 0.25).unwrap().unwrap();
        control.preview_voice(0, voice).unwrap();
        pull(&mut player, 128);
    }
    assert_eq!(control.shared().leaked.load(std::sync::atomic::Ordering::Relaxed), 0);
    // A render has none of it.
    assert!(render(&p)[..24000 - 10].iter().all(|f| *f == [0.0; 2]));
}
