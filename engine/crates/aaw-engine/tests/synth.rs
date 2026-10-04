//! The Synth through the engine: a MIDI track with a synth plays its note
//! clips, a render equals playback from the start in every block size and
//! is the same bytes twice, a lane on the filter is heard, a locate chases
//! the sounding notes, and the player never allocates.

use aaw_engine::player::channel;
use aaw_engine::program::{compile, compile_cached, Cache, Program};
use aaw_engine::render::{Frame, Renderer};
use std::path::Path;
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

const HEAD: &str = r#"
session: {tempo: 120, length_beats: 8, sample_rate: 48000, master_gain_db: 0, end_fade_ms: 0}
"#;

fn song(synth: &str, automation: &str) -> String {
    format!(
        "{HEAD}tracks:\n- id: lead\n  type: midi\n  instrument:\n    synth: {synth}\n  clips:\n  - at: 1\n    length_beats: 6\n    notes:\n    - {{pitch: 48, duration: 1}}\n    - {{pitch: 52, duration: 1, velocity: 64}}\n    - {{pitch: 55, duration: 1}}\n    - {{pitch: 60, at: 1/3, duration: 2/3}}\n    - {{pitch: 67, at: 2, duration: 1.5}}\n    - {{pitch: 36, at: 4, duration: 1}}\n{automation}"
    )
}

const PATCH: &str = "{voices: 4, oscillators: {a: {wave: saw}, b: {wave: pulse, pulse_width: 25, detune_cents: 9}, sub: {wave: sine, octave: -1, level_db: -6, filter: false}}, filter: {cutoff_hz: 1200, resonance_percent: 25, slope_db_per_octave: 24, drive_db: 3}, envelopes: {amp: {attack_ms: 4, decay_ms: 200, sustain_percent: 70, release_ms: 120}, env2: {attack_ms: 0, decay_ms: 300, sustain_percent: 0}}, lfos: {lfo1: {rate_beats: 1, shape: triangle}, lfo2: {rate_hz: 6, retrigger: true}}, macros: {tone: 50}, modulation: [{source: env2, target: filter.cutoff_hz, amount: 2}, {source: lfo2, target: pitch, amount: 0.1}, {source: lfo1, target: oscillators.b.pan, amount: 0.7}, {source: macros.tone, target: filter.cutoff_hz, amount: 1}, {source: velocity, target: oscillators.a.level_db, amount: 6}]}";

fn compiled(dir: &Path, yaml: &str) -> Arc<Program> {
    let path = dir.join("song.yaml");
    std::fs::write(&path, yaml).unwrap();
    let p = aaw_model::load(&path, true).unwrap_or_else(|e| panic!("{e}"));
    Arc::new(compile(&p, dir).unwrap())
}

fn render(p: &Arc<Program>, block: usize) -> Vec<Frame> {
    let mut r = Renderer::new(p.clone(), 0, 4096);
    let mut out = vec![[0.0; 2]; p.total];
    for chunk in out.chunks_mut(block) {
        r.render(chunk, |_, _, _| {});
    }
    out
}

#[test]
fn a_render_is_the_same_in_every_block_size_and_twice() {
    let dir = tempfile::tempdir().unwrap();
    let p = compiled(dir.path(), &song(PATCH, ""));
    let whole = render(&p, 4096);
    assert!(whole[24000..48000].iter().any(|f| f[0].abs() > 0.05), "the phrase is silent");
    assert!(whole[..24000 - 10].iter().all(|f| *f == [0.0; 2]), "nothing before the first note");
    assert_eq!(whole, render(&p, 128));
    assert_eq!(whole, render(&p, 977));
    let again = compiled(dir.path(), &song(PATCH, ""));
    assert_eq!(whole, render(&again, 4096));
}

#[test]
fn the_plain_saw_plays_and_a_lane_on_the_cutoff_is_heard() {
    let dir = tempfile::tempdir().unwrap();
    let plain = render(&compiled(dir.path(), &song("{oscillators: {a: {}}}", "")), 4096);
    assert!(plain[24000..48000].iter().any(|f| f[0].abs() > 0.1), "the plain saw is silent");
    let open = song("{oscillators: {a: {}}, filter: {slope_db_per_octave: 24}}", "");
    let lane = "  automation:\n  - param: instrument.filter.cutoff_hz\n    points: [{at: 0, value: 120}, {at: 8, value: 120}]\n";
    let closed = render(&compiled(dir.path(), &song("{oscillators: {a: {}}, filter: {slope_db_per_octave: 24}}", lane)), 4096);
    let open = render(&compiled(dir.path(), &open), 4096);
    let rms = |x: &[Frame], from: usize, to: usize| (x[from..to].iter().map(|f| f[0] * f[0]).sum::<f64>() / (to - from) as f64).sqrt();
    // The chord at beat 1 under a lowpass at 120 Hz, against the filter open.
    assert!(rms(&closed, 26000, 46000) < 0.3 * rms(&open, 26000, 46000), "{} against {}", rms(&closed, 26000, 46000), rms(&open, 26000, 46000));
    // A lane whose points share one value renders exactly as the static value.
    let still = render(&compiled(dir.path(), &song("{oscillators: {a: {}}, filter: {slope_db_per_octave: 24, cutoff_hz: 120}}", "")), 4096);
    assert_eq!(still, closed);
}

#[test]
fn playback_equals_the_render_and_never_allocates() {
    let dir = tempfile::tempdir().unwrap();
    let p = compiled(dir.path(), &song(PATCH, ""));
    let (mut control, mut player) = channel(p.clone());
    control.play(0).unwrap();
    let mut out = vec![[0.0; 2]; p.total];
    assert_no_alloc::assert_no_alloc(|| {
        for chunk in out.chunks_mut(128) {
            player.render(chunk);
        }
    });
    assert_eq!(out, render(&p, 4096));
}

#[test]
fn a_locate_chases_the_notes_sounding_there() {
    let dir = tempfile::tempdir().unwrap();
    let p = compiled(dir.path(), &song(PATCH, ""));
    // Into the long note at beat 3, held to 4.5.
    let at = 3 * 24000 + 12000;
    let mut r = Renderer::new(p.clone(), at, 4096);
    let mut out = vec![[0.0; 2]; 24000];
    for chunk in out.chunks_mut(512) {
        r.render(chunk, |_, _, _| {});
    }
    assert!(out[..6000].iter().any(|f| f[0].abs() > 0.02), "the held note is picked up");
    assert!(out.iter().all(|f| f[0].is_finite()));
}

#[test]
fn a_patch_edit_keeps_the_notes_and_the_program_structure_follows_the_waves() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("song.yaml");
    std::fs::write(&path, song(PATCH, "")).unwrap();
    let mut p = aaw_model::load(&path, true).unwrap();
    let mut cache = Cache::default();
    let a = compile_cached(&p, dir.path(), &mut cache).unwrap();
    let synth = |p: &mut aaw_model::Project| match p.tracks[0].midi.as_mut().unwrap().instrument.as_mut().unwrap() {
        aaw_model::Instrument::Synth(s) => s.clone(),
        _ => unreachable!(),
    };
    let mut s = synth(&mut p);
    s.filter.cutoff_hz = 300.0;
    p.tracks[0].midi.as_mut().unwrap().instrument = Some(aaw_model::Instrument::Synth(s.clone()));
    let b = compile_cached(&p, dir.path(), &mut cache).unwrap();
    // The notes are shared, so a playing synth carries on; a knob keeps the structure.
    assert!(Arc::ptr_eq(&a.tracks[0].synth.as_ref().unwrap().notes, &b.tracks[0].synth.as_ref().unwrap().notes));
    assert_eq!(a.structure, b.structure);
    s.oscillators[0].wave = aaw_model::Wave::Square;
    p.tracks[0].midi.as_mut().unwrap().instrument = Some(aaw_model::Instrument::Synth(s));
    let c = compile_cached(&p, dir.path(), &mut cache).unwrap();
    assert_ne!(b.structure, c.structure);
}
