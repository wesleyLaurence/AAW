//! FAUST as a real-time device's source: what its compiler made, bound to
//! a manifest's fields and wrapped in the device ABI, run by the engine;
//! and, where a FAUST compiler is, `daw plugin build` from a `.dsp`.
//!
//! `faust/stereo.wasm` and `faust/mono.wasm` were made from the `.dsp`
//! beside them by `faust -lang wasm -ftz 2 -o NAME.wasm NAME.dsp`, so the
//! wrapper is tried without a compiler.

mod common;

use aaw_dsp::{Clock, Frame};
use aaw_host::devices::{self, faust, Where};
use aaw_host::session::Session;
use common::{edit, write_song};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

const STEREO_MANIFEST: &str = r#"
format: 1
name: Stereo Test
run: realtime
fields:
  gain_db: {type: db, min: -24, max: 12, default: 0}
  cutoff_hz: {type: hz, min: 20, max: 20000, default: 20000}
  mode: {type: choice, choices: [one, two, three], default: one}
  bypass: {type: switch, default: false}
"#;

const MONO_MANIFEST: &str = "format: 1\nname: Mono Test\nrun: realtime\nfields:\n  level: {type: number, min: 0, max: 1, default: 0.5}\n";

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/faust").join(name)).unwrap()
}

/// The fixture wrapped for the manifest, as a device folder under `dir`.
fn device(dir: &Path, wasm: &str, manifest: &str) -> devices::Installed {
    let module = faust::read_module(&fixture(wasm)).unwrap();
    let m = devices::parse_manifest(manifest, "test").unwrap();
    let zones = faust::bind(&module, &m, "device.dsp").unwrap();
    let bytes = wat::parse_str(faust::wrap(&module, &zones).unwrap()).unwrap();
    let folder = dir.join("test");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("device.yaml"), manifest).unwrap();
    std::fs::write(folder.join("device.wasm"), bytes).unwrap();
    devices::read(&folder, Where::Workspace).unwrap()
}

fn noise(n: usize, seed: u32) -> Vec<Frame> {
    let mut s = seed;
    let mut next = move || {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(s >> 8) / f64::from(1u32 << 24) - 0.5
    };
    (0..n).map(|_| [next(), next()]).collect()
}

/// The device's output for `input` with these fields, the rest at their
/// defaults.
fn run(d: &devices::Installed, fields: &[(&str, aaw_model::FieldValue)], input: &[Frame]) -> Vec<Frame> {
    let code = d.code(48000.0, 120.0).unwrap();
    let values: BTreeMap<String, aaw_model::FieldValue> = fields.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
    let mut running = code.instance_at(&values);
    let mut x = input.to_vec();
    assert_no_alloc::assert_no_alloc(|| running.process(&mut x, Clock { frame: 0, step: 1, total: 1 << 30 }));
    assert!(code.failure().is_none(), "{:?}", code.failure());
    x
}

fn energy(x: &[Frame], channel: usize) -> f64 {
    x.iter().map(|f| f[channel] * f[channel]).sum()
}

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

#[test]
fn a_faust_module_is_read_bound_and_run_through_the_abi() {
    let module = faust::read_module(&fixture("stereo.wasm")).unwrap();
    assert_eq!((module.inputs, module.outputs), (2, 2));
    let names: Vec<&str> = module.controls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["cutoff_hz", "bypass", "gain_db", "mode"], "a label is a field's name in lowercase, its other characters underscores");
    let cutoff = &module.controls[0];
    assert_eq!((cutoff.label.as_str(), cutoff.unit.as_str(), cutoff.log, cutoff.min, cutoff.max), ("Cutoff Hz", "Hz", true, 20.0, 20000.0));
    assert!(module.controls[1].switch && !module.controls[3].switch);

    let dir = tempfile::tempdir().unwrap();
    let d = device(dir.path(), "stereo.wasm", STEREO_MANIFEST);
    let check = devices::check(&d).unwrap();
    assert_eq!((&check["ok"], &check["channels"], &check["latency"]), (&json!(true), &json!(2), &json!(0)), "{check}");

    use aaw_model::FieldValue::{Flag, Number, Text};
    let input = noise(4800, 7);
    // A switch is given 1 or 0: bypassed, the input comes back as it was.
    let through = run(&d, &[("bypass", Flag(true))], &input);
    for (y, x) in through.iter().zip(&input) {
        assert!((y[0] - x[0]).abs() < 1e-6 && (y[1] - x[1]).abs() < 1e-6, "{y:?} from {x:?}");
    }
    // A number is given as itself: -6 dB is half the level of 0 dB, frame
    // by frame.
    let open = run(&d, &[], &input);
    let quieter = run(&d, &[("gain_db", Number(-6.0))], &input);
    let gain = 10f64.powf(-6.0 / 20.0);
    for (q, o) in quieter.iter().zip(&open) {
        assert!((q[0] - gain * o[0]).abs() < 1e-5, "{q:?} against {o:?}");
    }
    // The cutoff low takes the most of a noise's energy away.
    let dull = run(&d, &[("cutoff_hz", Number(200.0))], &input);
    assert!(energy(&dull, 0) < 0.05 * energy(&open, 0), "{} of {}", energy(&dull, 0), energy(&open, 0));
    // A choice is given its index: the third makes the right three times
    // what it was.
    let three = run(&d, &[("mode", Text("three".into()))], &input);
    for (t, o) in three.iter().zip(&open) {
        assert!((t[1] - 3.0 * o[1]).abs() < 1e-5 && (t[0] - o[0]).abs() < 1e-6);
    }
}

#[test]
fn a_mono_faust_module_is_given_the_two_channels_mean_and_gives_both() {
    let dir = tempfile::tempdir().unwrap();
    let d = device(dir.path(), "mono.wasm", MONO_MANIFEST);
    let input = noise(1000, 3);
    let out = run(&d, &[("level", aaw_model::FieldValue::Number(0.8))], &input);
    for (y, x) in out.iter().zip(&input) {
        let mean = 0.5 * (x[0] + x[1]);
        assert!((y[0] - 0.8 * mean).abs() < 1e-5 && (y[1] - 0.8 * mean).abs() < 1e-5, "{y:?} from {x:?}");
    }
}

#[test]
fn fields_that_do_not_match_the_controls_are_refused_with_the_line_to_write() {
    let module = faust::read_module(&fixture("stereo.wasm")).unwrap();
    let refused = |manifest: &str| faust::bind(&module, &devices::parse_manifest(manifest, "test").unwrap(), "device.dsp").unwrap_err();
    let without = STEREO_MANIFEST.replace("  cutoff_hz: {type: hz, min: 20, max: 20000, default: 20000}\n", "");
    assert_eq!(
        refused(&without),
        "the control \"Cutoff Hz\" in device.dsp has no field in device.yaml; under fields add\n  cutoff_hz: {type: hz, min: 20, max: 20000, default: 20000}"
    );
    let extra = format!("{STEREO_MANIFEST}  width: {{type: percent, min: 0, max: 100, default: 50}}\n");
    let e = refused(&extra);
    assert!(e.starts_with("the field width has no control in device.dsp") && e.ends_with("its controls are cutoff_hz, bypass, gain_db, mode"), "{e}");
    let wider = STEREO_MANIFEST.replace("min: -24, max: 12", "min: -80, max: 12");
    assert_eq!(refused(&wider), "gain_db is -80 to 12 in device.yaml, and its control in device.dsp takes -60 to 12: make the field's range within the control's");
    let switch = STEREO_MANIFEST.replace("{type: switch, default: false}", "{type: number, min: 0, max: 1, default: 0}");
    assert!(refused(&switch).contains("bypass is a number, and its control in device.dsp is a checkbox or a button"));
    let four = STEREO_MANIFEST.replace("[one, two, three]", "[one, two, three, four]");
    assert!(refused(&four).contains("mode is a choice of 4, given to its control as 0 to 3; its control in device.dsp takes 0 to 2"));
}

#[test]
fn a_faust_source_is_for_a_realtime_effect() {
    let manifest = |extra: &str| format!("format: 1\nname: X\nrun: realtime\nsource: device.dsp\n{extra}");
    assert!(devices::parse_manifest(&manifest(""), "x").is_ok());
    let e = devices::parse_manifest(&manifest("kind: instrument\n"), "x").unwrap_err();
    assert!(e.contains("source device.dsp is FAUST, which builds an effect; a real-time instrument's source is Rust"), "{e}");
    let e = devices::parse_manifest("format: 1\nname: X\nrun: realtime\nsource: device.c\n", "x").unwrap_err();
    assert!(e.contains("a .rs or a .dsp file"), "{e}");
}

/// A device's build, or None where no FAUST compiler is.
fn built(d: &devices::Installed) -> Option<serde_json::Value> {
    match devices::build(d) {
        Ok(b) => Some(b),
        Err(e) if e.contains("Could not start") => {
            eprintln!("no FAUST compiler here: skipped ({e})");
            None
        }
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn a_faust_device_is_scaffolded_built_and_played_in_the_song() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let made = devices::new_faust("Faust Tilt", &dir.path().join("devices")).unwrap();
    assert!(made.realtime());
    assert_eq!((made.manifest.entry.as_str(), made.manifest.source.as_deref()), ("device.wasm", Some("device.dsp")));
    let yaml = std::fs::read_to_string(made.dir.join("device.yaml")).unwrap();
    assert!(yaml.contains("built from device.dsp by\n# daw plugin build with the FAUST compiler"), "{yaml}");
    let source = std::fs::read_to_string(made.dir.join("device.dsp")).unwrap();
    assert!(source.starts_with("// Faust Tilt: the scaffold of a real-time device in FAUST") && source.contains("daw plugin build faust-tilt"));
    assert!(made.code(48000.0, 120.0).unwrap_err().contains("device.wasm is not built yet: daw plugin build faust-tilt compiles device.dsp"));
    let Some(b) = built(&made) else { return };
    assert_eq!((&b["ok"], &b["run"], &b["channels"]), (&json!(true), &json!("realtime"), &json!(2)), "{b}");
    assert!(b["load_percent"].as_f64().unwrap() < 50.0, "{b}");

    // In a song, frozen: the input at -6 dB once the gain has glided there.
    let mut s = Session::open(&path, true).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "faust-tilt", "gain_db": -6})).unwrap();
    let p = s.project().clone();
    let input = aaw_engine::custom::input(&p, dir.path(), "tracks.bass", 1, 4096, &Default::default()).unwrap();
    edit(&mut s, json!({"op": "track.freeze", "track": "bass"})).unwrap();
    let frozen = aaw_engine::sndfile::read(&dir.path().join("frozen/bass.wav")).unwrap();
    let gain = 10f32.powf(-6.0 / 20.0);
    let settled = 9600;
    let worst = frozen.data.chunks(2).zip(&input).skip(settled).map(|(f, i)| (f[0] as f32 - i[0] * gain).abs()).fold(0.0f32, f32::max);
    assert!(worst < 1e-3, "the freeze is the input at -6 dB; off by {worst}");
    assert!(input.iter().skip(settled).any(|f| f[0].abs() > 0.05));

    // A source that does not compile says why; one whose controls and
    // fields differ says what to write.
    std::fs::write(made.dir.join("device.dsp"), "process = nothing_here;\n").unwrap();
    let e = devices::build(&devices::read(&made.dir, made.from).unwrap()).unwrap_err();
    assert!(e.starts_with("device.dsp does not build:") && e.contains("nothing_here"), "{e}");
    std::fs::write(made.dir.join("device.dsp"), source.replace("checkbox(\"invert\")", "checkbox(\"flip\")")).unwrap();
    let e = devices::build(&devices::read(&made.dir, made.from).unwrap()).unwrap_err();
    assert!(e.contains("the control \"flip\" in device.dsp has no field in device.yaml; under fields add\n  flip: {type: switch, default: false}"), "{e}");
    // The module that played is kept until a build succeeds.
    assert!(made.dir.join("device.wasm").is_file());
}

#[test]
fn a_mono_faust_source_runs_on_each_channel_and_its_math_comes_from_the_engine() {
    let dir = tempfile::tempdir().unwrap();
    let made = devices::new_faust("Wobble", dir.path()).unwrap();
    // One channel in, one out, through FAUST's tanh and sine.
    std::fs::write(
        made.dir.join("device.dsp"),
        "import(\"stdfaust.lib\");\ndrive = hslider(\"drive\", 1, 1, 10, 0.1);\nprocess = *(drive) : ma.tanh : +(0.001 * sin(hslider(\"rate_hz[unit:Hz]\", 2, 0.1, 10, 0.1)));\n",
    )
    .unwrap();
    std::fs::write(
        made.dir.join("device.yaml"),
        "format: 1\nname: Wobble\nrun: realtime\nsource: device.dsp\nfields:\n  drive: {type: number, min: 1, max: 10, default: 1}\n  rate_hz: {type: hz, min: 0.1, max: 10, default: 2}\n",
    )
    .unwrap();
    let d = devices::read(&made.dir, made.from).unwrap();
    if built(&d).is_none() {
        return;
    }
    let d = devices::read(&made.dir, made.from).unwrap();
    let input = noise(2000, 11);
    let out = run(&d, &[("drive", aaw_model::FieldValue::Number(3.0)), ("rate_hz", aaw_model::FieldValue::Number(5.0))], &input);
    let offset = 0.001 * 5f64.sin();
    for (y, x) in out.iter().zip(&input) {
        let want = [(3.0 * x[0]).tanh() + offset, (3.0 * x[1]).tanh() + offset];
        assert!((y[0] - want[0]).abs() < 1e-5 && (y[1] - want[1]).abs() < 1e-5, "each channel its own: {y:?} for {want:?}");
    }
}
