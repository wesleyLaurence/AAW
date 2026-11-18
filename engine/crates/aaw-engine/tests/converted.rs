//! Patterns converted (D190): a song of pads and pattern clips renders, once
//! converted to Drum Racks and note clips, as it did, sample for sample: the
//! voices song, the mix song with every effect, and a song of each case the
//! conversion handles.

mod common;

use aaw_engine::program::compile;
use aaw_engine::render::{Frame, Renderer};
use common::{noise, write_pattern_mix_song as write_mix_song, write_pattern_song as write_song, write_sample};

use std::path::Path;

fn render(p: &aaw_model::Project, dir: &Path) -> Vec<Frame> {
    let program = std::sync::Arc::new(compile(p, dir).unwrap_or_else(|e| panic!("{e}")));
    let mut r = Renderer::new(program.clone(), 0, 4096);
    let mut out = vec![[0.0; 2]; program.total];
    for chunk in out.chunks_mut(4096) {
        r.render(chunk, |_, _, _| {});
    }
    out
}

/// The song as written and as converted, rendered, with `edit` made to it
/// as written first.
fn both(path: &Path, edit: impl FnOnce(&mut aaw_model::Project)) -> (aaw_model::Project, Vec<Frame>, Vec<Frame>) {
    let dir = path.parent().unwrap();
    let mut old = aaw_model::parse(&std::fs::read_to_string(path).unwrap()).unwrap_or_else(|e| panic!("{e}"));
    edit(&mut old);
    let new = aaw_model::convert::converted(&old).unwrap().expect("the song has patterns");
    let (a, b) = (render(&old, dir), render(&new, dir));
    (new, a, b)
}

/// No clip's velocities scaled, which a note's whole velocity rounds.
fn unscaled(p: &mut aaw_model::Project) {
    p.tracks.iter_mut().flat_map(|t| &mut t.clips).for_each(|c| c.velocity_scale = 1.0);
}

/// The same but for the last bits of a sample, where voices are summed in
/// another order, or within `bound`.
fn assert_same(a: &[Frame], b: &[Frame]) {
    assert_within(a, b, 1e-9);
}

fn assert_within(a: &[Frame], b: &[Frame], bound: f64) {
    assert!(a.iter().any(|f| f[0].abs() > 0.01), "the song is silent");
    assert_eq!(a.len(), b.len());
    let (worst, at) = a.iter().zip(b).enumerate().fold((0.0f64, 0), |(w, at), (i, (x, y))| {
        let d = (x[0] - y[0]).abs().max((x[1] - y[1]).abs());
        if d > w { (d, i) } else { (w, at) }
    });
    assert!(worst < bound, "{worst} apart at frame {at}: {:?} against {:?}", a[at], b[at]);
}

#[test]
fn the_voices_song_sounds_the_same_converted() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let (new, a, b) = both(&path, unscaled);
    assert_same(&a, &b);
    // A clip's velocity scale is rounded into its notes' velocities: 42
    // scaled by 0.9 is 38, not 37.8, a level 0.05 dB off at most.
    let (_, a, b) = both(&path, |_| {});
    assert_within(&a, &b, 0.5 / 127.0);
    assert!(new.patterns.is_empty());
    assert!(new.tracks.iter().all(|t| t.midi.as_ref().and_then(|m| m.sampler()).is_some_and(|s| s.drum_rack)));
}

#[test]
fn the_mix_song_sounds_the_same_converted() {
    let dir = tempfile::tempdir().unwrap();
    let (_, a, b) = both(&write_mix_song(dir.path()), unscaled);
    assert_same(&a, &b);
}

#[test]
fn every_case_sounds_the_same_converted() {
    let dir = tempfile::tempdir().unwrap();
    let n = noise(6000, 3);
    write_sample(dir.path(), "hit.wav", n.iter().map(|x| [*x as f32, (*x * 0.7) as f32]).collect(), 48000);
    let tone: Vec<[f32; 2]> =
        (0..48000).map(|i| ((2.0 * std::f64::consts::PI * 220.0 * i as f64 / 48000.0).sin() * 0.3) as f32).map(|x| [x, x]).collect();
    write_sample(dir.path(), "tone.wav", tone, 48000);
    let yaml = r#"
session: {tempo: 100, length_beats: 32, sample_rate: 48000, end_fade_ms: 0}
samples:
  hit: {path: hit.wav}
  tone: {path: tone.wav, root_note: A3}
patterns:
  beat:
    length_beats: 4
    grid: 1/4
    steps: {kick: x... .... x.5. ....,  hat: x9x9 x9x9 x9x9 x9x9, ohat: ..x. ..x. ..x. ..x.}
    swing: 0.62
    events:
    - {at: 1, pad: kick, transpose: -2, velocity: 90}
    - {at: 3, pad: kick, transpose: 0.5}
    - {at: 2, pad: snare}
    - {at: 1.5, pad: ohat, velocity: 120}
    - {at: 1.5, pad: ohat, velocity: 30}
  line:
    length_beats: 2
    events:
    - {at: 0, pad: bass, note: C3, duration: 1}
    - {at: 0.5, pad: bass, note: E3, duration: 1, transpose: 0.25}
    - {at: 1.5, pad: bass, note: G2, duration: 1.5}
    - {at: 1, pad: bass, velocity: 50, duration: 1/4}
tracks:
- id: drums
  gain_db: -2
  pads:
    kick: {sample: hit, gain_db: -1}
    snare: {sample: hit, transpose: 7, pan: 0.2}
    hat: {sample: hit, transpose: 12, choke_group: hats, release_ms: 4}
    ohat: {sample: hit, transpose: 10, choke_group: hats}
    spare: {sample: tone}
  clips:
  - {pattern: beat, at: 0, repeats: 3, velocity_scale: 0.5}
  - {pattern: beat, at: 12, active: false}
  - {pattern: beat, at: 16, repeats: 2}
  audio: [{sample: tone, at: 24}]
  effects: [{type: eq, bands: [{shape: bell, freq_hz: 400, gain_db: -3}]}]
- id: bass
  pads: {bass: {sample: tone, mode: gate, release_ms: 20, transpose: -1}}
  clips: [{pattern: line, at: 2, repeats: 5}, {pattern: line, at: 20}]
"#;
    let path = dir.path().join("song.yaml");
    std::fs::write(&path, yaml).unwrap();
    let (new, a, b) = both(&path, unscaled);
    assert_same(&a, &b);
    // The drums keep their name, and the audio clip has a track of its own
    // right after them with the same chain.
    let ids: Vec<&str> = new.tracks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["drums", "drums-audio", "bass"]);
    assert_eq!(new.tracks[1].audio.len(), 1);
    assert_eq!(new.tracks[1].effects.len(), 1);
    let rack = new.tracks[0].midi.as_ref().unwrap().sampler().unwrap();
    let key = |pad: &str| rack.map.iter().find(|m| m.pad == pad).map(|m| m.low);
    // The General MIDI keys for the names it knows; the rest from C2 up.
    assert_eq!((key("kick"), key("snare"), key("hat"), key("ohat")), (Some(36), Some(38), Some(42), Some(46)));
    assert_eq!(key("spare"), Some(37));
    // A transposed hit plays a copy of its pad, moved as far.
    assert_eq!(rack.pads["kick-2"].transpose, -2.0);
    assert_eq!(rack.pads["kick+0.5"].transpose, 0.5);
    // The bass's notes are a pitched range; its hit without a note a key.
    let bass = new.tracks[2].midi.as_ref().unwrap();
    assert!(bass.sampler().unwrap().map.iter().any(|m| m.pitched && m.pad == "bass"));
    // The line's last note is gated past the pattern's end, so its clip is
    // written out rather than looped.
    assert!(bass.clips[0].loop_beats.is_none());
    assert_eq!(new.tracks[0].midi.as_ref().unwrap().clips[0].loop_beats.as_ref().map(|b| b.text()), Some("4".into()));
    assert!(!new.tracks[0].midi.as_ref().unwrap().clips[1].active);
}

#[test]
fn reading_a_song_converts_it_once_and_keeps_it_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let before = std::fs::read_to_string(&path).unwrap();
    let p = aaw_model::load(&path, true).unwrap();
    assert!(p.patterns.is_empty() && p.tracks.iter().all(|t| t.midi.is_some()));
    let kept = dir.path().join("song.before-drum-rack.yaml");
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), before);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), aaw_model::to_yaml(&p));
    // Read again, nothing changes and nothing more is kept.
    let after = std::fs::read_to_string(&path).unwrap();
    aaw_model::load(&path, true).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), after);
    assert!(!dir.path().join("song.before-drum-rack-2.yaml").exists());
}


#[test]
fn the_test_songs_are_the_pattern_songs_converted() {
    for (midi, patterns) in [
        (common::write_song as fn(&Path) -> std::path::PathBuf, write_song as fn(&Path) -> std::path::PathBuf),
        (common::write_mix_song, write_mix_song),
    ] {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let written = aaw_model::parse(&std::fs::read_to_string(midi(a.path())).unwrap()).unwrap();
        let old = aaw_model::parse(&std::fs::read_to_string(patterns(b.path())).unwrap()).unwrap();
        let converted = aaw_model::convert::converted(&old).unwrap().unwrap();
        assert_same(&render(&written, a.path()), &render(&converted, b.path()));
    }
}
