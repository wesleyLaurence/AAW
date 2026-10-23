//! Devices of the person's own through a session: added by slug from a
//! manifest, their fields and lanes held to it, their caches made by the
//! Python runner and played, in a freeze too; a real-time device built
//! with rustc, checked, and run by the engine.

mod common;

use aaw_dsp::device::Made;
use aaw_host::devices::{self, State, Work};
use aaw_host::session::Session;
use common::{edit, get, write_song};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn open() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    devices::new("Tilt", &dir.path().join("devices"), false, false).unwrap();
    let s = Session::open(&path, true).unwrap();
    (dir, s)
}

fn has_python() -> bool {
    aaw_host::python::interpreter().is_ok()
}

/// An offline device's cache.
fn audio(made: &Made) -> Arc<Vec<[f32; 2]>> {
    match made {
        Made::Audio(a) => a.clone(),
        Made::Code(_) => panic!("a real-time device's code, not a cache"),
    }
}

#[test]
fn a_device_is_added_by_its_slug_and_held_to_its_manifest() {
    let (_dir, mut s) = open();
    let reply = edit(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "tilt", "gain_db": -6})).unwrap();
    assert_eq!(reply["label"], "Add tilt to tracks.bass");
    let e = get(&s, "tracks.bass.effects.1");
    assert_eq!(e["type"], "device");
    assert_eq!(e["name"], "tilt");
    assert_eq!(e["version"], "0.1");
    assert_eq!(e["fields"]["gain_db"], json!(-6.0));
    assert_eq!(e["fields"]["mode"], "stereo");
    assert_eq!(e["fields"]["invert"], false);
    // The other spelling: type device with a name.
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.drums", "type": "device", "name": "tilt", "id": "t", "fields": {"warmth_percent": 40}})).unwrap();
    assert_eq!(get(&s, "tracks.drums.effects.t.fields.warmth_percent"), json!(40.0));
    // Fields held to the manifest, in its words.
    let refused = |s: &mut Session, j: serde_json::Value| edit(s, j).unwrap_err();
    assert!(refused(&mut s, json!({"op": "set", "path": "tracks.bass.effects.1.fields.gain_db", "value": 30})).contains("gain_db is -24 to 24; 30 is outside"));
    assert!(refused(&mut s, json!({"op": "set", "path": "tracks.bass.effects.1.fields.mode", "value": "wide"})).contains("mode is one of stereo, mono"));
    assert!(refused(&mut s, json!({"op": "set", "path": "tracks.bass.effects.1.fields.invert", "value": 1})).contains("invert takes true or false"));
    assert!(refused(&mut s, json!({"op": "set", "path": "tracks.bass.effects.1.fields.colour", "value": 1})).contains("tilt has no field colour; its fields are gain_db, tone_hz, warmth_percent, mode, invert"));
    assert!(refused(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "tilt", "colour": 1})).contains("no field colour"));
    assert!(refused(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "nothing"})).contains("No device nothing"));
    edit(&mut s, json!({"op": "set", "path": "tracks.bass.effects.1.fields.gain_db", "value": 3})).unwrap();
    edit(&mut s, json!({"op": "set", "path": "tracks.bass.effects.1.fields.mode", "value": "mono"})).unwrap();
    // A lane where the manifest allows one, and not elsewhere.
    edit(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "effects.1.fields.gain_db", "points": [{"at": 0, "value": -12}, {"at": 16, "value": 0}]})).unwrap();
    assert!(refused(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "effects.1.fields.tone_hz", "points": [{"at": 0, "value": 800}]})).contains("tilt does not take a lane on tone_hz"));
    assert!(refused(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "effects.1.fields.colour", "points": [{"at": 0, "value": 1}]})).contains("no field colour"));
    assert!(refused(&mut s, json!({"op": "lane.set", "owner": "tracks.bass", "param": "effects.1.gain_db", "points": [{"at": 0, "value": 1}]})).contains("effects.REF.fields.FIELD"));
    // Not in a rack, nor in a patch.
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.drums", "type": "rack", "id": "r"})).unwrap();
    assert!(refused(&mut s, json!({"op": "effect.add", "owner": "tracks.drums.effects.r", "type": "tilt"})).contains("not in a rack or a patch"));
    // Saved as written.
    s.save().unwrap();
    let text = std::fs::read_to_string(s.path()).unwrap();
    assert!(text.contains("{type: device, name: tilt, version: '0.1', fields: {gain_db: 3.0, tone_hz: 20000.0, warmth_percent: 0.0, mode: mono,\n      invert: false}}"), "{text}");
    // Its state without a host: waiting, since nothing has run.
    let status = devices::status_headless(s.project(), s.dir());
    assert_eq!(status.len(), 2);
    assert_eq!(status[0].reference, "tracks.drums.effects.t");
    assert_eq!(status[0].state, State::Waiting);
    // One the song names that is not there is said to be missing.
    edit(&mut s, json!({"op": "apply", "patch": {"master": {"effects": [{"type": "device", "name": "gone"}]}}})).unwrap();
    let status = devices::status_headless(s.project(), s.dir());
    assert!(matches!(&status[2].state, State::Missing(e) if e.contains("No device gone")));
}

#[test]
fn a_device_s_cache_is_made_by_its_code_and_played_in_the_song_and_a_freeze() {
    if !has_python() {
        eprintln!("no Python at the checkout: skipped");
        return;
    }
    let (dir, mut s) = open();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "tilt", "gain_db": -6})).unwrap();
    let p = s.project().clone();
    let instances = devices::instances(&p, dir.path());
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].key(), ("tracks.bass".to_string(), 1));
    let identity = instances[0].identity.clone().expect("installed and heard");
    // The check runs the code once on noise.
    let installed = instances[0].installed.clone().unwrap();
    let checked = devices::check(&installed).unwrap();
    assert_eq!((&checked["ok"], &checked["frames_in"], &checked["channels"]), (&json!(true), &json!(24000), &json!(2)));
    // Settled: the cache is the input through the code, at -6 dB.
    let settled = devices::settle(&p, dir.path(), true).unwrap();
    assert_eq!(settled.status[0].state, State::Ready);
    let cache = audio(settled.caches.get(&("tracks.bass".to_string(), 1)).unwrap());
    let file = devices::cache_dir(dir.path()).join(format!("{identity}.wav"));
    assert!(file.is_file());
    let input = aaw_engine::custom::input(&p, dir.path(), "tracks.bass", 1, 4096, &Default::default()).unwrap();
    assert_eq!(cache.len(), input.len());
    let gain = 10f32.powf(-6.0 / 20.0);
    let worst = cache.iter().zip(&input).map(|(c, i)| (c[0] - i[0] * gain).abs()).fold(0.0f32, f32::max);
    assert!(worst < 1e-3, "the cache is the input at -6 dB; off by {worst}");
    assert!(input.iter().any(|f| f[0].abs() > 0.05));
    // Frozen, the track's file is the cache, since the device ends the chain.
    edit(&mut s, json!({"op": "track.freeze", "track": "bass"})).unwrap();
    let frozen = aaw_engine::sndfile::read(&dir.path().join("frozen/bass.wav")).unwrap();
    let worst = frozen.data.chunks(2).zip(cache.iter()).map(|(f, c)| (f[0] as f32 - c[0]).abs()).fold(0.0f32, f32::max);
    assert!(worst < 1e-6, "the freeze plays the cache; off by {worst}");
    edit(&mut s, json!({"op": "track.unfreeze", "track": "bass"})).unwrap();
    // A changed field is another identity: made again in the background
    // by a host's work, which says what it is doing.
    edit(&mut s, json!({"op": "set", "path": "tracks.bass.effects.1.fields.gain_db", "value": -12})).unwrap();
    let p = s.project().clone();
    let again = devices::instances(&p, dir.path());
    assert_ne!(again[0].identity.as_ref().unwrap(), &identity);
    let mut work = Work::default();
    let changed = work.update(&p, dir.path());
    assert!(changed.status && !changed.caches);
    assert_eq!(work.status[0].state, State::Computing);
    assert!(work.busy());
    let started = Instant::now();
    let mut landed = false;
    while started.elapsed() < Duration::from_secs(120) {
        let c = work.poll(&p, dir.path());
        if c.caches {
            landed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(landed, "the run landed: {:?}", work.status);
    assert_eq!(work.status[0].state, State::Ready);
    let quieter = audio(work.caches.get(&("tracks.bass".to_string(), 1)).unwrap());
    let gain = 10f32.powf(-12.0 / 20.0);
    let worst = quieter.iter().zip(&input).map(|(c, i)| (c[0] - i[0] * gain).abs()).fold(0.0f32, f32::max);
    assert!(worst < 1e-3, "the new cache is at -12 dB; off by {worst}");
    // The old cache was swept; the new one stays.
    assert!(!file.is_file(), "the old cache is swept");
    assert!(again[0].cache_file(dir.path()).unwrap().is_file());
    // Bypassed, there is nothing to make, and the work says so.
    edit(&mut s, json!({"op": "effect.bypass", "effect": "tracks.bass.effects.1", "bypass": true})).unwrap();
    let c = work.update(&s.project().clone(), dir.path());
    assert!(c.caches && c.status);
    assert_eq!(work.status[0].state, State::Off);
    assert!(work.caches.is_empty());
    // Code that fails is said to fail, and a render refuses.
    edit(&mut s, json!({"op": "effect.bypass", "effect": "tracks.bass.effects.1", "bypass": false})).unwrap();
    std::fs::write(dir.path().join("devices/tilt/device.py"), "def process(audio, sample_rate, fields):\n    raise ValueError('no sound today')\n").unwrap();
    let p: Arc<aaw_model::Project> = s.project().clone();
    let err = devices::settle(&p, dir.path(), true).unwrap_err();
    assert!(err.contains("tracks.bass.effects.1") && err.contains("no sound today"), "{err}");
    let lenient = devices::settle(&p, dir.path(), false).unwrap();
    assert!(matches!(&lenient.status[0].state, State::Failed(e) if e.contains("no sound today")));
    assert!(lenient.caches.is_empty());
}

#[test]
fn a_realtime_device_is_built_checked_and_run_in_the_song() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let made = devices::new("Live Tilt", &dir.path().join("devices"), true, false).unwrap();
    assert!(made.realtime());
    assert_eq!((made.manifest.entry.as_str(), made.manifest.source.as_deref()), ("device.wasm", Some("device.rs")));
    let source = std::fs::read_to_string(made.dir.join("device.rs")).unwrap();
    assert!(source.starts_with("//! Live Tilt: the scaffold") && source.contains("daw plugin build live-tilt"));
    // Unbuilt, it is listed and said to want building.
    assert!(made.code(48000.0, 120.0).unwrap_err().contains("device.wasm is not built yet: daw plugin build live-tilt compiles device.rs"));
    let built = match devices::build(&made) {
        Ok(b) => b,
        Err(e) if e.contains("not installed") || e.contains("Could not start") => {
            eprintln!("no Rust for WebAssembly here: skipped ({e})");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    assert_eq!((&built["ok"], &built["run"], &built["latency"], &built["channels"]), (&json!(true), &json!("realtime"), &json!(0), &json!(2)));
    assert!(built["load_percent"].as_f64().unwrap() < 50.0, "{built}");
    assert!(built["bytes"].as_u64().unwrap() < 200_000, "stripped: {built}");
    assert!(built.get("stale").is_none());

    let mut s = Session::open(&path, true).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "live-tilt", "gain_db": -6})).unwrap();
    let p = s.project().clone();
    let instances = devices::instances(&p, dir.path());
    assert!(instances[0].identity.is_none(), "a real-time device has no cache");
    let settled = devices::settle(&p, dir.path(), true).unwrap();
    assert_eq!(settled.status[0].state, State::Ready);
    assert!(matches!(settled.caches.get(&("tracks.bass".to_string(), 1)), Some(Made::Code(_))));
    // The freeze runs it: the input at -6 dB, from the first frame.
    let input = aaw_engine::custom::input(&p, dir.path(), "tracks.bass", 1, 4096, &Default::default()).unwrap();
    edit(&mut s, json!({"op": "track.freeze", "track": "bass"})).unwrap();
    let frozen = aaw_engine::sndfile::read(&dir.path().join("frozen/bass.wav")).unwrap();
    let gain = 10f32.powf(-6.0 / 20.0);
    let worst = frozen.data.chunks(2).zip(&input).map(|(f, i)| (f[0] as f32 - i[0] * gain).abs()).fold(0.0f32, f32::max);
    assert!(worst < 1e-5, "the freeze is the input at -6 dB; off by {worst}");
    assert!(input.iter().any(|f| f[0].abs() > 0.05));
    edit(&mut s, json!({"op": "track.unfreeze", "track": "bass"})).unwrap();

    // A host's work loads the code and says it is ready.
    let p = s.project().clone();
    let mut work = Work::default();
    let changed = work.update(&p, dir.path());
    assert!(changed.caches && changed.status);
    assert_eq!(work.status[0].state, State::Ready);
    assert!(!work.folders_changed(), "looked at a moment ago");

    // Code that traps once playing: a rebuild is seen in the folder, its
    // check refuses it, and once the engine runs it the work says why.
    let failing = source.replace("let target = 10f64", "if _frame > 4800 {\n            panic!(\"no sound today\");\n        }\n        let target = 10f64");
    assert_ne!(failing, source);
    std::fs::write(made.dir.join("device.rs"), failing).unwrap();
    let rebuilt = devices::read(&made.dir, made.from).unwrap();
    assert!(rebuilt.stale());
    let refused = devices::build(&rebuilt).unwrap_err();
    assert!(refused.contains("aaw_process failed"), "{refused}");
    std::thread::sleep(Duration::from_millis(1100));
    assert!(work.folders_changed());
    let changed = work.update(&p, dir.path());
    assert!(changed.caches, "the new code is loaded");
    assert_eq!(work.status[0].state, State::Ready);
    let Some(Made::Code(code)) = work.caches.get(&("tracks.bass".to_string(), 1)).cloned() else { panic!() };
    let mut running = code.instance(&aaw_model::Device { id: None, name: "live-tilt".into(), version: None, fields: Default::default(), bypass: false }, &Default::default());
    let mut x = vec![[0.1, 0.1]; 9600];
    running.process(&mut x, aaw_dsp::Clock { frame: 0, step: 1, total: 96000 });
    assert!(work.failing());
    let changed = work.poll(&p, dir.path());
    assert!(changed.status && !changed.caches);
    assert!(matches!(&work.status[0].state, State::Failed(e) if e.contains("aaw_process failed")), "{:?}", work.status);
    assert!(!work.failing(), "said once");
}

/// A MIDI track `keys` playing an instrument of the person's own, `slug`,
/// with a clip of a C held two beats from beat 4 and an E an octave up from
/// beat 6.
fn with_keys(s: &mut Session, slug: &str) {
    edit(s, json!({"op": "track.add", "id": "keys", "type": "midi"})).unwrap();
    let reply = edit(s, json!({"op": "instrument.set", "track": "keys", "instrument": slug})).unwrap();
    assert!(reply["label"].as_str().unwrap().starts_with("Attach"), "{reply}");
    edit(
        s,
        json!({"op": "clip.add", "track": "keys", "id": "phrase", "at": 4, "length_beats": 4, "notes": [
            {"pitch": 60, "at": 0, "duration": 2, "velocity": 127},
            {"pitch": 76, "at": 2, "duration": 1, "velocity": 64},
        ]}),
    )
    .unwrap();
}

/// The frames of `beats` at the test song's tempo and rate.
fn frames_at(p: &aaw_model::Project, beats: f64) -> usize {
    (beats * 60.0 / p.session.tempo * p.session.sample_rate as f64).round() as usize
}

#[test]
fn an_instrument_is_set_by_slug_held_to_its_manifest_and_kept_out_of_chains() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    devices::new("Tilt", &dir.path().join("devices"), false, false).unwrap();
    let made = devices::new("Pluck", &dir.path().join("devices"), false, true).unwrap();
    assert!(made.instrument() && !made.realtime());
    let code = std::fs::read_to_string(made.dir.join("device.py")).unwrap();
    assert!(code.starts_with("\"\"\"Pluck: the scaffold of an instrument") && code.contains("def play(notes, frames, sample_rate, fields)"));
    let mut s = Session::open(&path, true).unwrap();
    with_keys(&mut s, "pluck");
    let i = get(&s, "tracks.keys.instrument.device");
    assert_eq!((&i["name"], &i["version"], &i["fields"]["wave"], &i["fields"]["gain_db"]), (&json!("pluck"), &json!("0.1"), &json!("saw"), &json!(-12.0)));
    let refused = |s: &mut Session, j: serde_json::Value| edit(s, j).unwrap_err();
    // Its fields and lanes are its manifest's.
    assert!(refused(&mut s, json!({"op": "set", "path": "tracks.keys.instrument.device.fields.wave", "value": "triangle"})).contains("wave is one of sine, saw, square"));
    assert!(refused(&mut s, json!({"op": "set", "path": "tracks.keys.instrument.device.fields.colour", "value": 1})).contains("pluck has no field colour"));
    let lane = |param: &str| json!({"op": "set", "path": "tracks.keys.automation", "value": [{"param": param, "points": [{"at": 0, "value": 1000}]}]});
    assert!(refused(&mut s, lane("instrument.fields.attack_ms")).contains("pluck does not take a lane on attack_ms"));
    assert!(refused(&mut s, lane("instrument.cutoff")).contains("instrument.fields.FIELD"));
    edit(&mut s, lane("instrument.fields.tone_hz")).unwrap();
    edit(&mut s, json!({"op": "set", "path": "tracks.keys.instrument.device.fields.wave", "value": "sine"})).unwrap();
    // An instrument goes on a MIDI track, an effect in a chain.
    assert!(refused(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "pluck"})).contains("pluck is an instrument, which a MIDI track's notes play: daw instrument set SONG TRACK pluck"));
    assert!(refused(&mut s, json!({"op": "instrument.set", "track": "keys", "instrument": "tilt"})).contains("tilt is an effect, which goes in a chain"));
    assert!(refused(&mut s, json!({"op": "instrument.set", "track": "keys", "instrument": {"device": {"name": "tilt"}}})).contains("tilt is an effect"));
    // Replaced by the Synth, its lanes go.
    edit(&mut s, json!({"op": "synth.add", "track": "keys"})).unwrap();
    assert_eq!(get(&s, "tracks.keys.automation"), json!([]));
    // In the song's instances it comes first on its track, by its key.
    edit(&mut s, json!({"op": "instrument.set", "track": "keys", "instrument": "pluck"})).unwrap();
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.keys", "type": "tilt"})).unwrap();
    let instances = devices::instances(s.project(), dir.path());
    let keys: Vec<_> = instances.iter().filter(|i| i.owner == "tracks.keys").map(|i| (i.reference.clone(), i.is_instrument())).collect();
    assert_eq!(keys, [("tracks.keys.instrument".to_string(), true), ("tracks.keys.effects.0".to_string(), false)]);
}

#[test]
fn an_offline_instrument_plays_the_notes_from_its_cache_and_feeds_the_chain() {
    if !has_python() {
        eprintln!("no Python at the checkout: skipped");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    devices::new("Tilt", &dir.path().join("devices"), false, false).unwrap();
    let made = devices::new("Pluck", &dir.path().join("devices"), false, true).unwrap();
    let checked = devices::check(&made).unwrap();
    assert_eq!((&checked["ok"], &checked["notes"], &checked["frames_out"]), (&json!(true), &json!(4), &json!(48000)));
    let mut s = Session::open(&path, true).unwrap();
    with_keys(&mut s, "pluck");
    edit(&mut s, json!({"op": "effect.add", "owner": "tracks.keys", "type": "tilt", "gain_db": -6})).unwrap();
    let p = s.project().clone();
    let settled = devices::settle(&p, dir.path(), true).unwrap();
    assert!(settled.status.iter().all(|x| x.state == State::Ready), "{:?}", settled.status);
    let played = audio(settled.caches.get(&("tracks.keys".to_string(), aaw_engine::custom::INSTRUMENT)).unwrap());
    // Silent before beat 4, sounding through the C, and its sound is what
    // the effect after it is given.
    let at = |beats: f64| frames_at(&p, beats);
    assert!(played[..at(4.0)].iter().all(|f| f[0] == 0.0));
    let loud = played[at(4.5)..at(5.5)].iter().fold(0.0f32, |m, f| m.max(f[0].abs()));
    assert!(loud > 0.05, "{loud}");
    let input = aaw_engine::custom::input(&p, dir.path(), "tracks.keys", 0, 4096, &settled.caches).unwrap();
    let worst = input.iter().zip(played.iter()).map(|(i, c)| (i[0] - c[0]).abs()).fold(0.0f32, f32::max);
    assert!(worst < 1e-6, "the tilt is given the instrument's sound; off by {worst}");
    // A note moved is another identity.
    let before = devices::instances(&p, dir.path())[0].identity.clone().unwrap();
    edit(&mut s, json!({"op": "set", "path": "tracks.keys.clips.phrase.notes.0.velocity", "value": 30})).unwrap();
    let after = devices::instances(s.project(), dir.path())[0].identity.clone().unwrap();
    assert_ne!(before, after);
    // Code that fails refuses the render, in its words.
    std::fs::write(made.dir.join("device.py"), "def play(notes, frames, sample_rate, fields):\n    raise ValueError('no keys today')\n").unwrap();
    let err = devices::settle(s.project(), dir.path(), true).unwrap_err();
    assert!(err.contains("tracks.keys.instrument") && err.contains("no keys today"), "{err}");
}

#[test]
fn a_realtime_instrument_is_built_played_frozen_and_previewed() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let made = devices::new("Live Pluck", &dir.path().join("devices"), true, true).unwrap();
    let source = std::fs::read_to_string(made.dir.join("device.rs")).unwrap();
    assert!(source.starts_with("//! Live Pluck: the scaffold of a real-time instrument") && source.contains("pub extern \"C\" fn aaw_note"));
    let built = match devices::build(&made) {
        Ok(b) => b,
        Err(e) if e.contains("not installed") || e.contains("Could not start") => {
            eprintln!("no Rust for WebAssembly here: skipped ({e})");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    assert_eq!((&built["ok"], &built["run"]), (&json!(true), &json!("realtime")));
    assert!(built["peak"].as_f64().unwrap() > 0.05, "the check's notes sound: {built}");
    let mut s = Session::open(&path, true).unwrap();
    with_keys(&mut s, "live-pluck");
    let p = s.project().clone();
    let settled = devices::settle(&p, dir.path(), true).unwrap();
    assert!(matches!(settled.caches.get(&("tracks.keys".to_string(), aaw_engine::custom::INSTRUMENT)), Some(Made::Code(_))));
    // Frozen, the file is its notes: silent before beat 4, the C, then the E.
    edit(&mut s, json!({"op": "track.freeze", "track": "keys"})).unwrap();
    let frozen = aaw_engine::sndfile::read(&dir.path().join("frozen/keys.wav")).unwrap();
    let at = |beats: f64| frames_at(&p, beats) * 2;
    assert!(frozen.data[..at(4.0)].iter().all(|x| *x == 0.0));
    let peak = |a: f64, b: f64| frozen.data[at(a)..at(b)].iter().fold(0.0f64, |m, x| m.max(x.abs()));
    assert!(peak(4.2, 5.8) > 0.05 && peak(6.2, 6.8) > 0.02, "{} {}", peak(4.2, 5.8), peak(6.2, 6.8));
    assert!(peak(8.5, 9.0) < 1e-3, "released by then");
    edit(&mut s, json!({"op": "track.unfreeze", "track": "keys"})).unwrap();
    // The work loads it as ready.
    let mut work = Work::default();
    work.update(s.project(), dir.path());
    assert!(work.status.iter().all(|x| x.state == State::Ready), "{:?}", work.status);
    // As an effect it is refused at once.
    let refused = edit(&mut s, json!({"op": "effect.add", "owner": "tracks.bass", "type": "live-pluck"})).unwrap_err();
    assert!(refused.contains("is an instrument"));
}
