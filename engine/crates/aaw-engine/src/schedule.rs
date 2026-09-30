//! Event scheduling: every hit's start frame, from exact beats, as `engine.schedule`.

use aaw_model::{frame, Event, PadMode, Project};
use num_rational::BigRational;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Trigger {
    pub start: i64,
    /// Index into `Project::tracks`.
    pub track: usize,
    pub track_id: String,
    pub pad: String,
    pub event: Event,
    pub velocity_scale: f64,
    /// Where a gated or choked voice releases.
    pub cutoff: Option<i64>,
}

/// All triggers, sorted by (start, track, pad), with choke cutoffs applied.
pub fn schedule(p: &Project) -> Vec<Trigger> {
    let rate = p.session.sample_rate;
    let tempo = p.session.tempo;
    let mut triggers = Vec::new();
    for (ti, t) in p.tracks.iter().enumerate() {
        for c in &t.clips {
            let pattern = &p.patterns[&c.pattern];
            let events = pattern.expanded();
            let length = pattern.length_exact();
            for r in 0..c.repeats {
                let base = c.at_exact() + BigRational::from_integer(r.into()) * &length;
                for e in &events {
                    let at = &base + e.at_exact();
                    let start = frame(&at, tempo, rate);
                    let cutoff = match (e.duration_exact(), t.pads[&e.pad].mode) {
                        (Some(d), PadMode::Gate) => Some(frame(&(&at + d), tempo, rate)),
                        _ => None,
                    };
                    triggers.push(Trigger {
                        start,
                        track: ti,
                        track_id: t.id.clone(),
                        pad: e.pad.clone(),
                        event: e.clone(),
                        velocity_scale: c.velocity_scale,
                        cutoff,
                    });
                }
            }
        }
    }
    triggers.sort_by(|a, b| (a.start, &a.track_id, &a.pad).cmp(&(b.start, &b.track_id, &b.pad)));
    // Choke groups are local to a track. The incoming hit releases the preceding voice.
    let mut last: HashMap<(usize, String), usize> = HashMap::new();
    for i in 0..triggers.len() {
        let pad = &p.tracks[triggers[i].track].pads[&triggers[i].pad];
        let Some(group) = pad.choke_group.as_deref().filter(|g| !g.is_empty()) else {
            continue;
        };
        let key = (triggers[i].track, group.to_string());
        if let Some(&j) = last.get(&key) {
            let start = triggers[i].start;
            let old = &mut triggers[j].cutoff;
            *old = Some(old.map_or(start, |c| c.min(start)));
        }
        last.insert(key, i);
    }
    triggers
}
