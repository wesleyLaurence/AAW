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
fn a_previewed_note_is_heard_while_stopped_through_the_chain_and_survives_a_patch_edit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("song.yaml");
    // A sine on a track with a filter insert, and a return it does not reach.
    let yaml = format!(
        "{HEAD}tracks:\n- id: lead\n  type: midi\n  instrument:\n    synth: {{oscillators: {{a: {{wave: sine, phase: 0}}}}, envelopes: {{amp: {{attack_ms: 0, release_ms: 100}}}}}}\n  effects: [{{type: filter, mode: lowpass, cutoff_hz: 8000}}]\n  clips:\n  - at: 6\n    length_beats: 2\n    notes:\n    - {{pitch: 60, duration: 1}}\n"
    );
    std::fs::write(&path, &yaml).unwrap();
    let mut cache = Cache::default();
    let p = Arc::new(compile_cached(&aaw_model::load(&path, true).unwrap(), dir.path(), &mut cache).unwrap());
    let (mut control, mut player) = channel(p.clone());
    let pull = |player: &mut aaw_engine::player::Player, n: usize| {
        let mut out = vec![[0.0; 2]; n];
        for chunk in out.chunks_mut(128) {
            player.render(chunk);
        }
        out
    };
    let peak = |x: &[Frame]| x.iter().fold(0.0f64, |m, f| m.max(f[0].abs()));
    // Stopped, resting: nothing sounds until a note is previewed.
    assert_eq!(peak(&pull(&mut player, 4800)), 0.0);
    control.preview(0, 69.0, 1.0, 24000).unwrap();
    let out = pull(&mut player, 24000);
    assert!(peak(&out[1000..]) > 0.1, "the previewed note sounds while stopped");
    let crossings = out[4000..20000].windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count();
    assert!((crossings as f64 / (16000.0 / 48000.0) - 440.0).abs() < 4.0, "{crossings} crossings of an A");
    // A knob turned while the note sounds: the same structure takes over and the note carries on.
    let mut project = aaw_model::load(&path, true).unwrap();
    if let Some(aaw_model::Instrument::Synth(s)) = project.tracks[0].midi.as_mut().unwrap().instrument.as_mut() {
        s.filter.cutoff_hz = 300.0;
    }
    let edited = Arc::new(compile_cached(&project, dir.path(), &mut cache).unwrap());
    assert_eq!(edited.structure, p.structure);
    control.load(edited).unwrap();
    control.preview(0, 57.0, 0.8, 48000).unwrap();
    let after = pull(&mut player, 2400);
    assert!(peak(&after) > 0.05, "still sounding across the take-over");
    // A wave changed: another structure, swapped through the dip, and the note is still there after it.
    if let Some(aaw_model::Instrument::Synth(s)) = project.tracks[0].midi.as_mut().unwrap().instrument.as_mut() {
        s.oscillators[0].wave = aaw_model::Wave::Triangle;
    }
    let reshaped = Arc::new(compile_cached(&project, dir.path(), &mut cache).unwrap());
    assert_ne!(reshaped.structure, p.structure);
    control.load(reshaped).unwrap();
    let swapped = pull(&mut player, 4800);
    assert!(peak(&swapped[2400..]) > 0.05, "the preview carries on under the new patch");
    // A preview while playing past the end of the song is heard too: the end fade is the timeline's.
    control.play(p.total - 10).unwrap();
    let _ = pull(&mut player, p.total);
    assert!(!player.playing());
    pull(&mut player, 60000);
    control.preview(0, 72.0, 1.0, 9600).unwrap();
    let late = pull(&mut player, 4800);
    assert!(peak(&late[500..]) > 0.05, "heard with the transport standing past the end");
    // A render has none of it.
    let whole = render(&p, 4096);
    assert!(whole[..6 * 24000 - 10].iter().all(|f| *f == [0.0; 2]));
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

#[test]
fn the_patchs_effects_run_before_the_inserts_and_count_in_the_latency() {
    let dir = tempfile::tempdir().unwrap();
    // A sine through the patch's own saturation: odd harmonics appear that the plain patch lacks.
    let plain = song("{oscillators: {a: {wave: sine, phase: 0, level_db: -3}}, envelopes: {amp: {attack_ms: 0, release_ms: 0}}}", "");
    let driven = song("{oscillators: {a: {wave: sine, phase: 0, level_db: -3}}, envelopes: {amp: {attack_ms: 0, release_ms: 0}}, effects: [{type: saturation, drive_db: 18}]}", "");
    let (a, b) = (render(&compiled(dir.path(), &plain), 4096), render(&compiled(dir.path(), &driven), 4096));
    let harmonic = |x: &[Frame], k: f64| {
        // The C3 at beat 1 (130.81 Hz), over half a second.
        let (mut re, mut im) = (0.0, 0.0);
        for (i, f) in x[26000..50000].iter().enumerate() {
            let t = std::f64::consts::TAU * 130.8128 * k * i as f64 / 48000.0;
            re += f[0] * t.cos();
            im += f[0] * t.sin();
        }
        2.0 * (re * re + im * im).sqrt() / 24000.0
    };
    assert!(harmonic(&a, 3.0) < 0.01 && harmonic(&b, 3.0) > 0.03, "{} {}", harmonic(&a, 3.0), harmonic(&b, 3.0));
    // A render equals playback in every block size with a chorus and a limiter in the patch.
    let rich = song("{oscillators: {a: {wave: saw, unison: 3}}, effects: [{type: chorus, id: wide}, {type: limiter, ceiling_db: -3}]}", "");
    let p = compiled(dir.path(), &rich);
    let whole = render(&p, 4096);
    assert_eq!(whole, render(&p, 128));
    assert!(whole[24000..48000].iter().any(|f| f[0].abs() > 0.05));
    assert!(whole.iter().all(|f| f[0].abs() <= aaw_engine::program::amplitude(-3.0) + 1e-6));
    // The limiter's look-ahead is latency the track's chain counts, so the
    // note lands where it does with the limiter on the inserts instead.
    let on_inserts = format!(
        "{HEAD}tracks:\n- id: lead\n  type: midi\n  instrument:\n    synth: {{oscillators: {{a: {{wave: saw, unison: 3}}}}, effects: [{{type: chorus, id: wide}}]}}\n  effects: [{{type: limiter, ceiling_db: -3}}]\n  clips:\n  - at: 1\n    length_beats: 6\n    notes:\n    - {{pitch: 48, duration: 1}}\n    - {{pitch: 52, duration: 1, velocity: 64}}\n    - {{pitch: 55, duration: 1}}\n    - {{pitch: 60, at: 1/3, duration: 2/3}}\n    - {{pitch: 67, at: 2, duration: 1.5}}\n    - {{pitch: 36, at: 4, duration: 1}}\n"
    );
    let q = compiled(dir.path(), &on_inserts);
    assert_eq!(p.latency, q.latency);
    assert!(p.latency > 0);
    let moved = render(&q, 4096);
    let first = |x: &[Frame]| x.iter().position(|f| f[0].abs() > 1e-6).unwrap();
    assert_eq!(first(&whole), first(&moved));
    // The output trails the transport by the latency, so the chord at beat 1 is that much in.
    assert!((first(&whole) as i64 - (24000 + p.latency as i64)).abs() <= 1, "{} against {}", first(&whole), 24000 + p.latency);
    // A lane on the patch's chorus mix, by its ID: all wet at one end and dry at the other.
    let lane = "  automation:\n  - param: instrument.effects.wide.mix_percent\n    points: [{at: 0, value: 0}, {at: 8, value: 0}]\n";
    let dry = render(&compiled(dir.path(), &song("{oscillators: {a: {wave: saw, unison: 3}}, effects: [{type: chorus, id: wide}]}", lane)), 4096);
    let none = render(&compiled(dir.path(), &song("{oscillators: {a: {wave: saw, unison: 3}}}", "")), 4096);
    assert_eq!(dry, none, "a chorus at no mix passes the voices through");
    let wet = render(&compiled(dir.path(), &song("{oscillators: {a: {wave: saw, unison: 3}}, effects: [{type: chorus, id: wide}]}", "")), 4096);
    assert_ne!(wet, none);
    // Playback never allocates with the patch's effects in the chain.
    let (mut control, mut player) = channel(p.clone());
    control.play(0).unwrap();
    let mut out = vec![[0.0; 2]; 24000];
    assert_no_alloc::assert_no_alloc(|| {
        for chunk in out.chunks_mut(128) {
            player.render(chunk);
        }
    });
    // An effect added to the patch changes the structure; a knob on it does not.
    let mut cache = Cache::default();
    let path = dir.path().join("song.yaml");
    std::fs::write(&path, song("{oscillators: {a: {}}, effects: [{type: chorus, mix_percent: 30}]}", "")).unwrap();
    let mut project = aaw_model::load(&path, true).unwrap();
    let before = compile_cached(&project, dir.path(), &mut cache).unwrap();
    if let Some(aaw_model::Instrument::Synth(s)) = project.tracks[0].midi.as_mut().unwrap().instrument.as_mut() {
        if let aaw_model::Effect::Chorus(c) = &mut s.effects[0] {
            c.mix_percent = 60.0;
        }
    }
    let turned = compile_cached(&project, dir.path(), &mut cache).unwrap();
    assert_eq!(before.structure, turned.structure);
    if let Some(aaw_model::Instrument::Synth(s)) = project.tracks[0].midi.as_mut().unwrap().instrument.as_mut() {
        s.effects.push(aaw_model::Effect::Saturation(aaw_model::Saturation {
            id: None,
            mode: aaw_model::SaturationMode::Soft,
            drive_db: 6.0,
            output_db: 0.0,
            mix_percent: 100.0,
            bypass: false,
        }));
    }
    let more = compile_cached(&project, dir.path(), &mut cache).unwrap();
    assert_ne!(turned.structure, more.structure);
}

#[test]
fn a_wavetable_oscillator_reads_a_sample_of_the_project_as_one_cycle() {
    let dir = tempfile::tempdir().unwrap();
    // One cycle of a saw, 200 frames, as a sample of the project.
    let cycle: Vec<[f32; 2]> = (0..200).map(|i| [(2.0 * i as f64 / 200.0 - 1.0) as f32; 2]).collect();
    std::fs::write(dir.path().join("cycle.wav"), aaw_engine::wav::float_wav_bytes(&cycle, 48000)).unwrap();
    let yaml = format!(
        "{HEAD}samples:\n  cycle: {{path: cycle.wav}}\ntracks:\n- id: lead\n  type: midi\n  instrument:\n    synth: {{oscillators: {{a: {{wave: wavetable, table: cycle, phase: 0}}}}, envelopes: {{amp: {{attack_ms: 0, release_ms: 0}}}}}}\n  clips:\n  - at: 0\n    length_beats: 4\n    notes:\n    - {{pitch: 69, duration: 4}}\n"
    );
    let out = render(&compiled(dir.path(), &yaml), 4096);
    // A saw at 440 Hz: its harmonics fall as 1/k, and the fundamental is there.
    let harmonic = |k: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, f) in out[4800..52800].iter().enumerate() {
            let t = std::f64::consts::TAU * 440.0 * k * i as f64 / 48000.0;
            re += f[0] * t.cos();
            im += f[0] * t.sin();
        }
        2.0 * (re * re + im * im).sqrt() / 48000.0
    };
    assert!(harmonic(1.0) > 0.1, "{}", harmonic(1.0));
    assert!((harmonic(2.0) / harmonic(1.0) - 0.5).abs() < 0.05, "{}", harmonic(2.0) / harmonic(1.0));
    // A sample that is not there is refused with the oscillator named.
    let missing = yaml.replace("table: cycle", "table: nope");
    let path = dir.path().join("song.yaml");
    std::fs::write(&path, missing).unwrap();
    let e = aaw_model::load(&path, true).map(|_| ()).unwrap_err().to_string();
    assert!(e.contains("oscillator a names wavetable nope"), "{e}");
}
