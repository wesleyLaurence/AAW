//! Real-time devices of the person's own: WebAssembly modules meeting the
//! device ABI, written here as text, made ready and run as the engine runs
//! them, alone and in a song.

mod common;

use aaw_dsp::device::{Live, Made};
use aaw_dsp::{Clock, Frame};
use aaw_engine::custom::Caches;
use aaw_engine::freeze::freeze_track;
use aaw_engine::wasm::{Code, Field};
use aaw_model::value::{Dict, Key, Value};
use aaw_model::{FieldValue, Project};
use common::write_song;
use std::collections::BTreeMap;
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

const RATE: f64 = 48000.0;

/// A module that multiplies its input by its first field, with `before`
/// run first in `aaw_process`, `exports` added, and `abi` its version. Its
/// fields are at 16, its left at 1024 and its right at 2048; `$calls`
/// counts the calls of `aaw_process`.
fn module(before: &str, exports: &str, abi: i32) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
  (memory (export "memory") 1)
  (global $calls (mut i32) (i32.const 0))
  (func (export "aaw_abi") (result i32) (i32.const {abi}))
  (func (export "aaw_init") (param f64 f64))
  (func (export "aaw_fields") (result i32) (i32.const 16))
  (func (export "aaw_left") (result i32) (i32.const 1024))
  (func (export "aaw_right") (result i32) (i32.const 2048))
  {exports}
  (func (export "aaw_process") (param $n i32) (param $frame i64) (param $rolling i32)
    (local $i i32) (local $g f32) (local $at i32)
    (global.set $calls (i32.add (global.get $calls) (i32.const 1)))
    {before}
    (local.set $g (f32.demote_f64 (f64.load (i32.const 16))))
    (block $done (loop $next
      (br_if $done (i32.ge_s (local.get $i) (local.get $n)))
      (local.set $at (i32.shl (local.get $i) (i32.const 2)))
      (f32.store (i32.add (i32.const 1024) (local.get $at)) (f32.mul (local.get $g) (f32.load (i32.add (i32.const 1024) (local.get $at)))))
      (f32.store (i32.add (i32.const 2048) (local.get $at)) (f32.mul (local.get $g) (f32.load (i32.add (i32.const 2048) (local.get $at)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next))))
)"#
    ))
    .unwrap()
}

fn gain_field() -> Vec<Field> {
    vec![Field {
        name: "gain".into(),
        default: FieldValue::Number(1.0),
        min: 0.0,
        max: 2.0,
        log: false,
        choices: vec![],
    }]
}

fn code(bytes: &[u8]) -> Result<Code, String> {
    Code::new(bytes, gain_field(), RATE, 120.0, false)
}

fn at(gain: f64) -> BTreeMap<String, FieldValue> {
    BTreeMap::from([("gain".to_string(), FieldValue::Number(gain))])
}

fn rolling(frame: i64) -> Clock {
    Clock { frame, step: 1, total: 1 << 30 }
}

fn ramp(n: usize) -> Vec<Frame> {
    (0..n).map(|i| [(i % 100) as f64 / 100.0, -((i % 50) as f64) / 50.0]).collect()
}

#[test]
fn a_module_plays_its_fields_in_blocks_of_any_size_without_allocating() {
    let code = code(&module("", "", 1)).unwrap();
    assert_eq!(code.latency(), 0);
    let mut running = code.instance_at(&at(0.5));
    let input = ramp(1000);
    let mut x = input.clone();
    // A block longer than the ABI's 128 frames is given in pieces.
    assert_no_alloc::assert_no_alloc(|| {
        running.process(&mut x[..700], rolling(0));
        running.process(&mut x[700..], rolling(700));
    });
    for (y, x) in x.iter().zip(&input) {
        assert!((y[0] - 0.5 * x[0]).abs() < 1e-6 && (y[1] - 0.5 * x[1]).abs() < 1e-6);
    }
    assert!(code.failure().is_none());
    // A choice is given its index and a switch 1 or 0; a missing field its
    // default, and a number outside the range its edge.
    let mut fields = gain_field();
    fields.push(Field { name: "mode".into(), default: FieldValue::Text("b".into()), min: 0.0, max: 1.0, log: false, choices: vec!["a".into(), "b".into()] });
    let two = Code::new(&module("", "", 1), fields, RATE, 120.0, false).unwrap();
    let mut y = vec![[1.0, 1.0]; 4];
    two.instance_at(&BTreeMap::new()).process(&mut y, rolling(0));
    assert_eq!(y[0], [1.0, 1.0], "the default gain");
    let mut y = vec![[1.0, 1.0]; 4];
    two.instance_at(&at(9.0)).process(&mut y, rolling(0));
    assert_eq!(y[0], [2.0, 2.0], "clamped to the field's max");
}

#[test]
fn a_module_that_does_not_meet_the_abi_is_refused_in_words() {
    let refused = |bytes: Vec<u8>| code(&bytes).unwrap_err();
    assert!(refused(module("", "", 2)).contains("device ABI 2"), "{}", refused(module("", "", 2)));
    let imports = wat::parse_str(r#"(module (import "env" "log" (func)) (memory (export "memory") 1))"#).unwrap();
    let imports = refused(imports);
    assert!(imports.contains("imports log from env; a device imports nothing but the math functions"), "{imports}");
    let other = wat::parse_str(r#"(module (import "wasi" "_sinf" (func (param f32) (result f32))) (memory (export "memory") 1))"#).unwrap();
    assert!(refused(other).contains("imports _sinf from wasi"));
    let typed = wat::parse_str(r#"(module (import "env" "_sinf" (func (param f64) (result f64))) (memory (export "memory") 1))"#).unwrap();
    let typed = refused(typed);
    assert!(typed.contains("does not start"), "{typed}");
    let bare = wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "aaw_abi") (result i32) (i32.const 1)))"#).unwrap();
    assert!(refused(bare).contains("exports no aaw_init"));
    let far = wat::parse_str(
        r#"(module (memory (export "memory") 1)
  (func (export "aaw_abi") (result i32) (i32.const 1))
  (func (export "aaw_init") (param f64 f64))
  (func (export "aaw_fields") (result i32) (i32.const 16))
  (func (export "aaw_left") (result i32) (i32.const 65500))
  (func (export "aaw_right") (result i32) (i32.const 2048))
  (func (export "aaw_process") (param i32 i64 i32)))"#,
    )
    .unwrap();
    assert!(refused(far).contains("aaw_left gave 65500"));
    assert!(refused(vec![0, 1, 2]).contains("does not compile"));
    // A latency outside 0 to 10 seconds, and one that is read.
    assert!(refused(module("", r#"(func (export "aaw_latency") (result i32) (i32.const -1))"#, 1)).contains("aaw_latency gave -1"));
    let late = code(&module("", r#"(func (export "aaw_latency") (result i32) (i32.const 64))"#, 1)).unwrap();
    assert_eq!(late.latency(), 64);
    // One that traps at once fails when it is made ready.
    assert!(refused(module("unreachable", "", 1)).contains("aaw_process failed"));
}

#[test]
fn a_module_calls_the_engine_s_math_without_allocating() {
    // Each sample becomes its sine and its power of two, through env.
    let bytes = wat::parse_str(
        r#"(module
  (import "env" "_sinf" (func $sin (param f32) (result f32)))
  (import "env" "_powf" (func $pow (param f32 f32) (result f32)))
  (memory (export "memory") 1)
  (func (export "aaw_abi") (result i32) (i32.const 1))
  (func (export "aaw_init") (param f64 f64))
  (func (export "aaw_fields") (result i32) (i32.const 16))
  (func (export "aaw_left") (result i32) (i32.const 1024))
  (func (export "aaw_right") (result i32) (i32.const 2048))
  (func (export "aaw_process") (param $n i32) (param $frame i64) (param $rolling i32)
    (local $at i32)
    (block $done (loop $next
      (br_if $done (i32.ge_s (local.get $at) (i32.shl (local.get $n) (i32.const 2))))
      (f32.store (i32.add (i32.const 1024) (local.get $at)) (call $sin (f32.load (i32.add (i32.const 1024) (local.get $at)))))
      (f32.store (i32.add (i32.const 2048) (local.get $at)) (call $pow (f32.const 2) (f32.load (i32.add (i32.const 2048) (local.get $at)))))
      (local.set $at (i32.add (local.get $at) (i32.const 4)))
      (br $next))))
)"#,
    )
    .unwrap();
    let code = Code::new(&bytes, vec![], RATE, 120.0, false).unwrap();
    let mut running = code.instance_at(&BTreeMap::new());
    let input = ramp(300);
    let mut x = input.clone();
    assert_no_alloc::assert_no_alloc(|| running.process(&mut x, rolling(0)));
    for (y, x) in x.iter().zip(&input) {
        assert!((y[0] - x[0].sin()).abs() < 1e-6 && (y[1] - 2f64.powf(x[1])).abs() < 1e-6, "{y:?} from {x:?}");
    }
    assert!(code.failure().is_none());
}

#[test]
fn a_module_that_traps_stalls_or_gives_nonsense_is_stopped_and_passes_through() {
    // Traps after frame 1000: from then on the input passes through.
    let trapping = code(&module("(if (i64.ge_s (local.get $frame) (i64.const 1000)) (then unreachable))", "", 1)).unwrap();
    let mut running = trapping.instance_at(&at(0.5));
    let mut x = vec![[1.0, 1.0]; 2048];
    running.process(&mut x, rolling(0));
    assert_eq!(x[0], [0.5, 0.5]);
    assert_eq!(x[1023], [0.5, 0.5], "before the trap");
    assert_eq!(x[1024], [1.0, 1.0], "the chunk that trapped passes through");
    assert_eq!(x[2047], [1.0, 1.0]);
    assert!(trapping.failure().unwrap().contains("aaw_process failed"));
    // Every later instance of failed code passes through from the start.
    let mut again = trapping.instance_at(&at(0.5));
    let mut y = vec![[1.0, 1.0]; 4];
    again.process(&mut y, rolling(0));
    assert_eq!(y[0], [1.0, 1.0]);

    // A loop that never ends is stopped at its deadline.
    let spinning = code(&module("(if (i64.ge_s (local.get $frame) (i64.const 128)) (then (loop $l (br $l))))", "", 1)).unwrap();
    let started = std::time::Instant::now();
    let mut x = vec![[1.0, 1.0]; 256];
    spinning.instance_at(&at(0.5)).process(&mut x, rolling(0));
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(spinning.failure().unwrap().contains("past its deadline"), "{:?}", spinning.failure());
    assert_eq!(x[255], [1.0, 1.0]);

    // Samples that are not numbers end it too.
    let nan = code(&module("(if (i64.ge_s (local.get $frame) (i64.const 128)) (then (f64.store (i32.const 16) (f64.const nan))))", "", 1)).unwrap();
    let mut x = vec![[1.0, 1.0]; 256];
    nan.instance_at(&at(0.5)).process(&mut x, rolling(0));
    assert!(nan.failure().unwrap().contains("not numbers"));
    assert_eq!(x[200], [1.0, 1.0]);

    // A failed device with latency still delays by it, as its chain expects.
    let late = code(&module(
        "(if (i64.ge_s (local.get $frame) (i64.const 128)) (then unreachable))",
        r#"(func (export "aaw_latency") (result i32) (i32.const 3))"#,
        1,
    ))
    .unwrap();
    let input = ramp(256);
    let mut x = input.clone();
    late.instance_at(&at(1.0)).process(&mut x, rolling(0));
    assert_eq!(x[128], [0.0, 0.0]);
    assert_eq!(x[131], input[128], "three frames late");
}

#[test]
fn an_instance_continues_from_the_one_it_replaces() {
    // The output is the number of calls so far, so the state shows.
    let counting = r#"(f32.store (i32.const 1024) (f32.convert_i32_s (global.get $calls)))
    (local.set $n (i32.const 0))"#;
    let code = code(&module(counting, "", 1)).unwrap();
    let mut old = code.instance_at(&at(1.0));
    let mut x = vec![[0.0, 0.0]; 128];
    for k in 0..3 {
        old.process(&mut x, rolling(k * 128));
    }
    // Made ready, the code was run once, and each instance starts afresh;
    // this one has been called three times.
    assert_eq!(x[0][0], 3.0);
    let mut new = code.instance_at(&at(1.0));
    assert_no_alloc::assert_no_alloc(|| new.take_over(old.as_mut()));
    new.process(&mut x, rolling(384));
    assert_eq!(x[0][0], 4.0, "the state carried over");
    let mut fresh = code.instance_at(&at(1.0));
    fresh.process(&mut x, rolling(0));
    assert_eq!(x[0][0], 1.0);
    // Same bytes, same signature; other bytes, another.
    let same = self::code(&module(counting, "", 1)).unwrap();
    assert_eq!(Live::signature(&code), Live::signature(&same));
    assert_ne!(Live::signature(&code), Live::signature(&self::code(&module("", "", 1)).unwrap()));
}

/// The track `id` of a document.
fn track<'a>(d: &'a mut Dict, id: &str) -> &'a mut Dict {
    let Some(Value::List(tracks)) = d.get_mut(&Key::str("tracks")) else { panic!() };
    for t in tracks {
        if let Value::Dict(t) = t {
            if matches!(t.get(&Key::str("id")), Some(Value::Str(s)) if s == id) {
                return t;
            }
        }
    }
    panic!("no track {id}")
}

fn with(p: &Project, f: impl FnOnce(&mut Dict)) -> Project {
    let Value::Dict(mut d) = p.dump(false) else { panic!() };
    f(&mut d);
    Project::validate(&Value::Dict(d)).unwrap_or_else(|e| panic!("{e}"))
}

fn set(d: &mut Dict, key: &str, text: &str) {
    d.insert(Key::str(key), aaw_model::yaml_load::load(text).unwrap());
}

#[test]
fn a_module_in_a_song_plays_in_its_place_with_its_lanes() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let plain = aaw_model::load(&path, false).unwrap();
    let dry = freeze_track(&plain, dir.path(), "bass", 4096, &Caches::new()).unwrap();
    let p = with(&plain, |d| {
        let bass = track(d, "bass");
        set(bass, "effects", "[{type: device, name: gain, fields: {gain: 0.5}}]");
    });
    let rate = p.session.sample_rate as f64;
    let code = Arc::new(Code::new(&module("", "", 1), gain_field(), rate, p.session.tempo, false).unwrap());
    let mut caches = Caches::new();
    caches.insert(("tracks.bass".into(), 0), Made::Code(code.clone()));
    // Without its code the device passes through; with it, the gain.
    let through = freeze_track(&p, dir.path(), "bass", 4096, &Caches::new()).unwrap();
    assert_eq!(through, dry);
    let wet = freeze_track(&p, dir.path(), "bass", 4096, &caches).unwrap();
    let worst = wet.iter().zip(&dry).map(|(w, d)| (w[0] - 0.5 * d[0]).abs().max((w[1] - 0.5 * d[1]).abs())).fold(0.0f32, f32::max);
    assert!(worst < 1e-6, "at half: {worst}");
    assert!(dry.iter().any(|f| f[0].abs() > 0.01));
    // A lane moves the field from 1 to 0 over the song's 16 beats.
    let laned = with(&p, |d| {
        let bass = track(d, "bass");
        set(bass, "automation", "[{param: effects.0.fields.gain, points: [{at: 0, value: 1}, {at: 16, value: 0}]}]");
    });
    let moved = freeze_track(&laned, dir.path(), "bass", 4096, &caches).unwrap();
    let span = 16.0 * 60.0 / laned.session.tempo * rate;
    for f in (0..dry.len()).step_by(997) {
        let g = (1.0 - f as f64 / span).clamp(0.0, 1.0) as f32;
        assert!((moved[f][0] - g * dry[f][0]).abs() < 0.002, "frame {f}");
    }
    assert!(code.failure().is_none());
}

/// An instrument: its left is its first field times the velocity of the
/// note held, over 127, changing at the frame the note starts or ends, and
/// its right the key of the last note started, over 127. One change a call.
fn instrument_module() -> Vec<u8> {
    wat::parse_str(
        r#"(module
  (memory (export "memory") 1)
  (global $level (mut f32) (f32.const 0))
  (global $next (mut f32) (f32.const 0))
  (global $key (mut f32) (f32.const 0))
  (global $from (mut i32) (i32.const 100000))
  (func (export "aaw_abi") (result i32) (i32.const 1))
  (func (export "aaw_init") (param f64 f64))
  (func (export "aaw_fields") (result i32) (i32.const 16))
  (func (export "aaw_left") (result i32) (i32.const 1024))
  (func (export "aaw_right") (result i32) (i32.const 2048))
  (func (export "aaw_note") (param $at i32) (param $key i32) (param $velocity i32)
    (global.set $from (local.get $at))
    (global.set $next (f32.div (f32.convert_i32_s (local.get $velocity)) (f32.const 127)))
    (if (i32.gt_s (local.get $velocity) (i32.const 0))
      (then (global.set $key (f32.div (f32.convert_i32_s (local.get $key)) (f32.const 127))))))
  (func (export "aaw_process") (param $n i32) (param $frame i64) (param $rolling i32)
    (local $i i32) (local $g f32) (local $at i32)
    (local.set $g (f32.demote_f64 (f64.load (i32.const 16))))
    (block $done (loop $next
      (br_if $done (i32.ge_s (local.get $i) (local.get $n)))
      (if (i32.eq (local.get $i) (global.get $from))
        (then (global.set $level (global.get $next)) (global.set $from (i32.const 100000))))
      (local.set $at (i32.shl (local.get $i) (i32.const 2)))
      (f32.store (i32.add (i32.const 1024) (local.get $at)) (f32.add (f32.load (i32.add (i32.const 1024) (local.get $at))) (f32.mul (local.get $g) (global.get $level))))
      (f32.store (i32.add (i32.const 2048) (local.get $at)) (global.get $key))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next))))
)"#,
    )
    .unwrap()
}

#[test]
fn an_instrument_is_given_its_notes_at_their_frames() {
    use aaw_dsp::instrument::NoteEvent;
    // An instrument's code must take notes; an effect's need not.
    let refused = Code::new(&module("", "", 1), gain_field(), RATE, 120.0, true).unwrap_err();
    assert!(refused.contains("exports no aaw_note"), "{refused}");
    let code = Code::new(&instrument_module(), gain_field(), RATE, 120.0, true).unwrap();
    assert!(code.instrument());
    let mut running = code.instance_at(&at(1.0));
    let mut x = vec![[0.0, 0.0]; 400];
    let notes = [
        NoteEvent { at: 5, key: 127, velocity: 127 },
        NoteEvent { at: 200, key: 127, velocity: 0 },
    ];
    // Given in pieces of 128 frames, each note at its frame within its piece.
    assert_no_alloc::assert_no_alloc(|| running.play(&mut x, rolling(0), &notes));
    assert_eq!(x[4][0], 0.0);
    assert_eq!(x[5], [1.0, 1.0]);
    assert_eq!(x[199], [1.0, 1.0]);
    assert_eq!(x[200], [0.0, 1.0], "let go at 200");
    assert!(code.failure().is_none());
}

#[test]
fn an_instrument_in_a_song_plays_its_notes_its_lanes_and_a_preview() {
    use aaw_engine::program::{compile_cached, Cache};
    use aaw_engine::render::Renderer;
    let dir = tempfile::tempdir().unwrap();
    let yaml = r#"
session: {tempo: 120, length_beats: 8}
tracks:
- id: keys
  type: midi
  instrument: {device: {name: tone, fields: {gain: 0.5}}}
  clips: [{length_beats: 8, notes: [{pitch: 127, at: 1, duration: 2, velocity: 127}]}]
"#;
    let p = Project::validate(&aaw_model::yaml_load::load(yaml).unwrap()).unwrap();
    let code = Arc::new(Code::new(&instrument_module(), gain_field(), 48000.0, 120.0, true).unwrap());
    let mut caches = Caches::new();
    caches.insert(("tracks.keys".into(), aaw_engine::custom::INSTRUMENT), Made::Code(code.clone()));
    // Without its code nothing sounds; with it, the note at half its gain
    // from beat 1 to beat 3, 24000 frames a beat.
    let silent = freeze_track(&p, dir.path(), "keys", 4096, &Caches::new()).unwrap();
    assert!(silent.iter().all(|f| f[0] == 0.0));
    let played = freeze_track(&p, dir.path(), "keys", 1000, &caches).unwrap();
    assert_eq!(played[23999][0], 0.0);
    assert_eq!(played[24000][0], 0.5);
    assert_eq!(played[71999][0], 0.5);
    assert_eq!(played[72000][0], 0.0);
    // A lane moves its gain from 1 to 0 over the eight beats: at beat 2,
    // three quarters.
    let laned = with(&p, |d| {
        let keys = track(d, "keys");
        set(keys, "automation", "[{param: instrument.fields.gain, points: [{at: 0, value: 1}, {at: 8, value: 0}]}]");
    });
    let moved = freeze_track(&laned, dir.path(), "keys", 1000, &caches).unwrap();
    assert!((moved[48000][0] - 0.75).abs() < 0.01, "{}", moved[48000][0]);
    // A note previewed while the transport stands still sounds through it.
    let mut cache = Cache::default();
    cache.set_devices(caches.clone());
    let program = Arc::new(compile_cached(&p, dir.path(), &mut cache).unwrap());
    let mut r = Renderer::new(program, 0, 512);
    r.preview(0, 127.0, 1.0, 600, true);
    assert!(r.previewing());
    let mut mix = vec![[0.0, 0.0]; 512];
    r.process(&mut mix, false, None, |_, _, _| {});
    assert!(mix[10][0] > 0.1, "{:?}", mix[10]);
    r.process(&mut mix, false, None, |_, _, _| {});
    assert!(!r.previewing());
    assert!(mix[100][0] == 0.0, "let go after 600 frames");
    assert!(code.failure().is_none());
}

#[test]
fn an_instrument_carries_its_note_through_an_edit_and_lets_go_of_one_removed() {
    use aaw_engine::program::{compile_cached, Cache};
    use aaw_engine::render::Renderer;
    let dir = tempfile::tempdir().unwrap();
    let song = |gain: f64, notes: &str| {
        let yaml = format!(
            "session: {{tempo: 120, length_beats: 8}}\ntracks:\n- id: keys\n  type: midi\n  instrument: {{device: {{name: tone, fields: {{gain: {gain}}}}}}}\n  clips: [{{length_beats: 8, notes: {notes}}}]\n"
        );
        Project::validate(&aaw_model::yaml_load::load(&yaml).unwrap()).unwrap()
    };
    let code = Arc::new(Code::new(&instrument_module(), gain_field(), 48000.0, 120.0, true).unwrap());
    let mut cache = Cache::default();
    let mut caches = Caches::new();
    caches.insert(("tracks.keys".into(), aaw_engine::custom::INSTRUMENT), Made::Code(code.clone()));
    cache.set_devices(caches);
    let held = "[{pitch: 127, at: 0, duration: 4, velocity: 127}]";
    let a = Arc::new(compile_cached(&song(0.5, held), dir.path(), &mut cache).unwrap());
    let b = Arc::new(compile_cached(&song(1.0, held), dir.path(), &mut cache).unwrap());
    let c = Arc::new(compile_cached(&song(1.0, "[{pitch: 127, at: 6, duration: 1, velocity: 127}]"), dir.path(), &mut cache).unwrap());
    assert_eq!(a.structure, b.structure);
    assert_eq!(b.structure, c.structure);
    let mut old = Renderer::new(a, 0, 512);
    let mut mix = vec![[0.0, 0.0]; 512];
    old.process(&mut mix, true, None, |_, _, _| {});
    // The mix through the fader and pan: what half the gain is there.
    let half = mix[511][0];
    assert!(half > 0.1);
    // A field changed: the note held carries on at the new gain.
    let mut new = Renderer::new(b, 0, 512);
    new.take_over(&mut old);
    new.process(&mut mix, true, Some(&mut old), |_, _, _| {});
    new.end_take_over();
    assert!((mix[511][0] - 2.0 * half).abs() < 1e-9, "{} against {half}", mix[511][0]);
    // The note removed: let go where the edit lands.
    let mut last = Renderer::new(c, 0, 512);
    last.take_over(&mut new);
    last.process(&mut mix, true, Some(&mut new), |_, _, _| {});
    assert_eq!(mix[0][0], 0.0, "{:?}", &mix[..4]);
    assert!(code.failure().is_none());
}
