//! Devices of the person's own in the engine: what one is given, rendered
//! for the host to run its code on, and how its cache plays in its place.

mod common;

use aaw_dsp::device::Made;
use aaw_engine::custom::{self, Caches};
use aaw_engine::freeze::freeze_track;
use aaw_engine::offline::{render, RenderOptions};
use aaw_engine::sndfile;
use aaw_model::value::{Dict, Key, Value};
use aaw_model::Project;
use common::write_song;
use std::path::Path;
use std::sync::Arc;

fn yaml(text: &str) -> Value {
    aaw_model::yaml_load::load(text).unwrap()
}

/// The song with its document edited and validated again.
fn with(p: &Project, f: impl FnOnce(&mut Dict)) -> Project {
    let Value::Dict(mut d) = p.dump(false) else { panic!() };
    f(&mut d);
    Project::validate(&Value::Dict(d)).unwrap_or_else(|e| panic!("{e}"))
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

fn push(d: &mut Dict, list: &str, item: Value) {
    match d.get_mut(&Key::str(list)) {
        Some(Value::List(items)) => items.push(item),
        _ => {
            d.insert(Key::str(list), Value::List(vec![item]));
        }
    }
}

fn close(a: &[[f32; 2]], b: &[[f32; 2]], tol: f32, what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: lengths");
    let worst = a.iter().zip(b).map(|(x, y)| (x[0] - y[0]).abs().max((x[1] - y[1]).abs())).fold(0.0f32, f32::max);
    assert!(worst <= tol, "{what}: differ by {worst}");
}

fn frames(path: &Path) -> Vec<[f32; 2]> {
    let a = sndfile::read(path).unwrap();
    a.data.chunks(a.channels).map(|f| [f[0] as f32, f[f.len() - 1] as f32]).collect()
}

#[test]
fn a_device_is_given_the_chain_before_it_and_plays_its_cache_in_its_place() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let p: Project = with(&aaw_model::load(&path, false).unwrap(), |d| {
        let bass = track(d, "bass");
        push(bass, "effects", yaml("{type: utility, gain_db: -6}"));
        push(bass, "effects", yaml("{type: device, name: half, fields: {gain_db: -6}}"));
        push(bass, "automation", yaml("{param: effects.0.gain_db, points: [{at: 0, value: -6}, {at: 8, value: 0}]}"));
        push(bass, "automation", yaml("{param: effects.1.fields.gain_db, points: [{at: 0, value: -6}, {at: 8, value: -12}]}"));
    });
    // The cut: the utility with its lane; the device and its lane go.
    let cut = custom::cut(&p, "tracks.bass", 1).unwrap();
    let cut_bass = cut.track("bass").unwrap();
    assert_eq!(cut_bass.effects.len(), 1);
    assert_eq!(cut_bass.automation.len(), 1);
    assert_eq!(cut_bass.automation[0].param, "effects.0.gain_db");
    assert!(custom::cut(&p, "tracks.bass", 0).unwrap_err().contains("is a utility, not a device"));
    assert!(custom::cut(&p, "tracks.bass", 5).unwrap_err().contains("no effect at 5"));
    assert!(custom::cut(&p, "nothing.x", 0).unwrap_err().contains("tracks.T"));
    // What the device is given is the cut chain's output before the fader,
    // whatever the block size.
    let input = custom::input(&p, dir.path(), "tracks.bass", 1, 4096, &Caches::new()).unwrap();
    let expected = freeze_track(&cut, dir.path(), "bass", 4096, &Caches::new()).unwrap();
    close(&input, &expected, 0.0, "the input is the cut chain's output");
    let small = custom::input(&p, dir.path(), "tracks.bass", 1, 1000, &Caches::new()).unwrap();
    close(&input, &small, 0.0, "block size");
    assert!(input.iter().any(|f| f[0].abs() > 0.01), "the bass sounds");
    // Without a cache the device passes through: the chain sounds as cut.
    let through = freeze_track(&p, dir.path(), "bass", 4096, &Caches::new()).unwrap();
    close(&through, &input, 0.0, "pass-through");
    // With a cache, the device plays it: here the input at half.
    let cache: Arc<Vec<[f32; 2]>> = Arc::new(input.iter().map(|f| [f[0] * 0.5, f[1] * 0.5]).collect());
    let mut caches = Caches::new();
    caches.insert(("tracks.bass".into(), 1), Made::Audio(cache.clone()));
    let played = freeze_track(&p, dir.path(), "bass", 4096, &caches).unwrap();
    close(&played, &cache, 1e-6, "the cache plays");
    // Bypassed, the device is not run: the cache is not heard.
    let bypassed = with(&p, |d| {
        let bass = track(d, "bass");
        let Some(Value::List(effects)) = bass.get_mut(&Key::str("effects")) else { panic!() };
        effects[1] = yaml("{type: device, name: half, bypass: true}");
        let Some(Value::List(lanes)) = bass.get_mut(&Key::str("automation")) else { panic!() };
        lanes.pop();
    });
    let silent = freeze_track(&bypassed, dir.path(), "bass", 4096, &caches).unwrap();
    close(&silent, &input, 0.0, "bypassed");
    // A cache keyed for another place in the chain is not this device's.
    let mut elsewhere = Caches::new();
    elsewhere.insert(("tracks.bass".into(), 0), Made::Audio(cache));
    let unmoved = freeze_track(&p, dir.path(), "bass", 4096, &elsewhere).unwrap();
    close(&unmoved, &input, 0.0, "another index");
}

#[test]
fn a_device_on_the_master_or_a_group_is_given_what_reaches_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let plain = render(&path, &RenderOptions { output: Some(dir.path().join("plain")), ..Default::default() }).unwrap();
    assert!(plain.get("render_id").is_some());
    let mix = frames(&dir.path().join("plain/mix.wav"));
    let p = with(&aaw_model::load(&path, false).unwrap(), |d| {
        let Some(Value::Dict(master)) = d.get_mut(&Key::str("master")) else { panic!() };
        push(master, "effects", yaml("{type: device, name: mute}"));
    });
    // The master device's input is the mix before the end fade: the same
    // frames as the plain mix until the fade begins.
    let input = custom::input(&p, dir.path(), "master", 0, 4096, &Caches::new()).unwrap();
    assert_eq!(input.len(), mix.len());
    let half = mix.len() / 2;
    close(&input[..half], &mix[..half], 1e-6, "the master's input is the mix");
    // With a cache of silence the mix is silent; without one, as it was.
    aaw_model::save(&p, &path).unwrap();
    let mut caches = Caches::new();
    caches.insert(("master".into(), 0), Made::Audio(Arc::new(vec![[0.0f32; 2]; input.len()])));
    render(&path, &RenderOptions { output: Some(dir.path().join("muted")), devices: caches, ..Default::default() }).unwrap();
    let muted = frames(&dir.path().join("muted/mix.wav"));
    assert!(muted.iter().all(|f| f == &[0.0, 0.0]));
    render(&path, &RenderOptions { output: Some(dir.path().join("through")), ..Default::default() }).unwrap();
    close(&frames(&dir.path().join("through/mix.wav")), &mix, 0.0, "pass-through on the master");
    // A group of the bass with a device: its input is the bass as it
    // leaves its fader, which is the bass's stem of the plain render.
    let p = with(&aaw_model::load(&path, false).unwrap(), |d| {
        let Some(Value::Dict(master)) = d.get_mut(&Key::str("master")) else { panic!() };
        master.insert(Key::str("effects"), Value::List(Vec::new()));
        push(d, "groups", yaml("{id: g, effects: [{type: device, name: mute}]}"));
        track(d, "bass").insert(Key::str("group"), Value::str("g"));
    });
    let input = custom::input(&p, dir.path(), "groups.g", 0, 4096, &Caches::new()).unwrap();
    // A render's stems carry the master gain, -6 dB unless set; the
    // group's input comes before it.
    let master = 10f32.powf(-6.0 / 20.0);
    let bass: Vec<[f32; 2]> = frames(&dir.path().join("plain/stems/bass.wav")).iter().map(|f| [f[0] / master, f[1] / master]).collect();
    close(&input[..half], &bass[..half], 1e-5, "the group's input is its track's stem");
}
