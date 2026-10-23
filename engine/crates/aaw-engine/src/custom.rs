//! Devices of the person's own, as the engine serves them: the input one
//! receives, rendered over the whole song for the host to run the device's
//! code on, whose result the device then plays from its cache.

use crate::program::{compile_scoped, Cache, Scope, StemKind};
use crate::render::Renderer;
use aaw_model::rules::{target, Owner, TargetKind};
use aaw_model::{Effect, Project};
use std::collections::HashMap;
use std::path::Path;
use aaw_dsp::device::Made;
use std::sync::Arc;

/// What the host made of the devices of the person's own, an offline
/// device's cache or a real-time device's code, by the chain's owner and
/// the device's index in it, or `INSTRUMENT` for a track's instrument.
pub type Caches = HashMap<(String, usize), Made>;

/// The index a track's instrument of the person's own is kept by among the
/// caches, beside its chain's devices: `(tracks.T, INSTRUMENT)`.
pub const INSTRUMENT: usize = usize::MAX;

/// The owner a track's MIDI effects of the person's own are kept by among
/// the caches, with their places among its `midi_effects`:
/// `(tracks.T.midi_effects, I)`.
pub fn midi_owner(track: &str) -> String {
    format!("tracks.{track}.midi_effects")
}

/// The song with what the MIDI effects of the person's own made of the
/// notes set on its tracks (`Midi::made`), so that the schedule plays it;
/// None when `caches` hold none.
pub fn attach(p: &Project, caches: &Caches) -> Option<Project> {
    let mut made: Vec<(&str, usize, &Arc<aaw_model::midi_effects::Made>)> = caches
        .iter()
        .filter_map(|((owner, i), m)| match m {
            Made::Notes(n) => Some((owner.strip_prefix("tracks.")?.strip_suffix(".midi_effects")?, *i, n)),
            _ => None,
        })
        .collect();
    if made.is_empty() {
        return None;
    }
    made.sort_by_key(|(t, i, _)| (*t, *i));
    let mut out = p.clone();
    for t in &mut out.tracks {
        if let Some(midi) = t.midi.as_mut() {
            midi.made = made.iter().filter(|(id, _, _)| *id == t.id).map(|(_, i, n)| (*i, (*n).clone())).collect();
        }
    }
    Some(out)
}

/// The chain an owner path names: the list it is in and its name.
fn split(owner: &str) -> Result<(&str, &str), String> {
    if owner == "master" {
        return Ok(("master", "master"));
    }
    match owner.split_once('.') {
        Some((list @ ("tracks" | "groups" | "returns"), name)) if !name.is_empty() => Ok((list, name)),
        _ => Err(format!("{owner}: a device's owner is tracks.T, groups.G, returns.R or master")),
    }
}

/// The song with `owner`'s chain cut before its effect at `index`, which
/// is a device of the person's own: what feeds the device. The device and
/// the effects after it go, with the lanes on them; the master's end fade
/// goes too, since the master's chain comes before it.
pub fn cut(p: &Project, owner: &str, index: usize) -> Result<Project, String> {
    let (list, name) = split(owner)?;
    let mut out = p.clone();
    let kept: Vec<bool> = {
        let who = match list {
            "tracks" => Owner::Track(p.track(name).ok_or_else(|| format!("No track {name}"))?),
            "groups" => Owner::Group(p.group(name).ok_or_else(|| format!("No group {name}"))?),
            "returns" => Owner::Return(p.returns.iter().find(|r| r.id == name).ok_or_else(|| format!("No return {name}"))?),
            _ => Owner::Master(&p.master),
        };
        match who.effects().get(index) {
            Some(Effect::Device(_)) => {}
            Some(other) => return Err(format!("{owner}.effects.{index} is a {}, not a device", other.kind())),
            None => return Err(format!("{owner} has no effect at {index}")),
        }
        who.automation()
            .iter()
            .map(|lane| !matches!(target(who, &lane.param), Ok(t) if matches!(t.kind, TargetKind::Effect { index: i, .. } if i >= index)))
            .collect()
    };
    let (effects, automation) = match list {
        "tracks" => {
            let t = out.tracks.iter_mut().find(|t| t.id == name).expect("the track");
            (&mut t.effects, &mut t.automation)
        }
        "groups" => {
            let g = out.groups.iter_mut().find(|g| g.id == name).expect("the group");
            (&mut g.effects, &mut g.automation)
        }
        "returns" => {
            let r = out.returns.iter_mut().find(|r| r.id == name).expect("the return");
            (&mut r.effects, &mut r.automation)
        }
        _ => {
            out.session.end_fade_ms = 0.0;
            (&mut out.master.effects, &mut out.master.automation)
        }
    };
    effects.truncate(index);
    let mut keep = kept.into_iter();
    automation.retain(|_| keep.next().unwrap_or(true));
    Ok(out)
}

/// What the device at `index` of `owner`'s chain is given, over the whole
/// song: stereo frames at the song's rate, frame N of the result the
/// device's input at timeline frame N, whatever the latency before it. The
/// chain's earlier effects run with their lanes, and so do the devices of
/// the person's own among them, from `caches`; the tracks that key the
/// chain's sidechains play along as they are.
pub fn input(p: &Project, directory: &Path, owner: &str, index: usize, block_size: usize, caches: &Caches) -> Result<Vec<[f32; 2]>, String> {
    if block_size == 0 {
        return Err("block_size must be positive".into());
    }
    let (list, name) = split(owner)?;
    let fed = cut(p, owner, index)?;
    let mut cache = Cache::default();
    cache.set_devices(caches.clone());
    let scope = if list == "master" { Scope::Song } else { Scope::Channel(name) };
    let program = Arc::new(compile_scoped(&fed, directory, &mut cache, scope)?);
    let stem = match list {
        "tracks" => Some(program.stem_index(StemKind::Track, name)),
        "groups" => Some(program.stem_index(StemKind::Group, name)),
        "returns" => Some(program.stem_index(StemKind::Return, name)),
        _ => None,
    };
    let stem = match stem {
        Some(found) => Some(found.ok_or_else(|| format!("{owner} has no stem"))?),
        None => None,
    };
    let total = program.total;
    if total == 0 {
        return Err("The song has no frames".into());
    }
    let mut out = vec![[0.0f32; 2]; total];
    let mut block = vec![[0.0; 2]; block_size];
    let mut renderer = Renderer::new(program.clone(), 0, block_size);
    renderer.tap_before_faders();
    let lay = |out: &mut Vec<[f32; 2]>, x: &[[f64; 2]], at: i64| {
        let lo = (-at).clamp(0, x.len() as i64) as usize;
        let hi = (total as i64 - at).clamp(0, x.len() as i64) as usize;
        for (k, f) in (lo..hi).zip(&x[lo..hi]) {
            let dst = &mut out[(at + k as i64) as usize];
            dst[0] = f[0] as f32;
            dst[1] = f[1] as f32;
        }
    };
    while !renderer.finished() {
        let n = block_size.min(program.total + program.latency - renderer.position());
        let heard = renderer.heard();
        renderer.render(&mut block[..n], |i, x, at| {
            if stem == Some(i) {
                lay(&mut out, x, at);
            }
        });
        if stem.is_none() {
            lay(&mut out, &block[..n], heard);
        }
    }
    if !out.iter().all(|f| f[0].is_finite() && f[1].is_finite()) {
        return Err(format!("{owner}.effects.{index}: the device's input has samples that are not numbers"));
    }
    Ok(out)
}
