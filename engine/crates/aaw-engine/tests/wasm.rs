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
    Code::new(bytes, gain_field(), RATE, 120.0)
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
    let two = Code::new(&module("", "", 1), fields, RATE, 120.0).unwrap();
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
    assert!(refused(imports).contains("imports log from env; a device imports nothing"));
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
    let code = Arc::new(Code::new(&module("", "", 1), gain_field(), rate, p.session.tempo).unwrap());
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
