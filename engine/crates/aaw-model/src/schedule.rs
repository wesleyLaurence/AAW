//! Event scheduling: every hit's start frame, from exact beats.

use crate::{frame, Beat, Event, Midi, PadMode, Project, Sampler};
use num_rational::BigRational;
use std::collections::HashMap;

/// A note as an instrument receives it: nothing about pads, only its pitch,
/// velocity, and the frames it starts and stops on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteOn {
    pub pitch: i64,
    pub velocity: i64,
    pub start: i64,
    /// The note-off: the note's end, or its clip's when that comes first.
    pub end: i64,
    /// Its start and length in beats, exactly.
    pub at: BigRational,
    pub beats: BigRational,
}

/// A MIDI track's notes in the order they start. A note plays from its start
/// to its end or its clip's, whichever is first; one that starts before its
/// clip or at or after its end does not play. A looped clip's notes play in
/// each repetition, cut at the loop's end, and in the last repetition only
/// those that start before the clip ends.
pub fn track_notes(p: &Project, midi: &Midi) -> Vec<NoteOn> {
    let (rate, tempo) = (p.session.sample_rate, p.session.tempo);
    let mut notes = Vec::new();
    for clip in &midi.clips {
        let start = clip.at_exact();
        for (from, plays) in clip.repetitions() {
            let base = &start + &from;
            for n in clip.notes.iter().filter(|n| clip.plays(n)) {
                let at = n.at_exact();
                if at >= plays {
                    continue;
                }
                let until = (&at + n.duration_exact()).min(plays.clone());
                let beats = &until - &at;
                notes.push(NoteOn {
                    pitch: n.pitch,
                    velocity: n.velocity,
                    start: frame(&(&base + &at), tempo, rate),
                    end: frame(&(&base + &until), tempo, rate),
                    at: &base + at,
                    beats,
                });
            }
        }
    }
    notes.sort_by_key(|n| (n.start, n.pitch));
    notes
}

/// What a sampler plays for notes: the pad each note's map entry names, at the
/// note's pitch when the entry is pitched, released at the note-off when the
/// pad is gated. A note no entry maps plays nothing.
pub fn sampler_hits(sampler: &Sampler, notes: &[NoteOn], track: usize, track_id: &str) -> Vec<Trigger> {
    let mut triggers = Vec::new();
    for n in notes {
        let Some(entry) = sampler.entry(n.pitch) else { continue };
        let pad = &sampler.pads[&entry.pad];
        triggers.push(Trigger {
            start: n.start,
            track,
            track_id: track_id.to_string(),
            pad: entry.pad.clone(),
            event: Event {
                at: Beat::Str(crate::fraction_str(&n.at)),
                pad: entry.pad.clone(),
                velocity: n.velocity,
                note: entry.pitched.then(|| crate::rules::note_name(n.pitch).expect("a pitch is 0 to 127")),
                duration: Some(Beat::Str(crate::fraction_str(&n.beats))),
                transpose: 0.0,
            },
            velocity_scale: 1.0,
            cutoff: (pad.mode == PadMode::Gate).then_some(n.end),
        });
    }
    triggers
}

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

/// One track's triggers, sorted by (start, pad), with choke cutoffs applied.
/// Choke groups are local to a track, so a track's hits depend on nothing else.
pub fn track_triggers(p: &Project, ti: usize) -> Vec<Trigger> {
    let rate = p.session.sample_rate;
    let tempo = p.session.tempo;
    let t = &p.tracks[ti];
    let mut triggers = Vec::new();
    if let Some(midi) = &t.midi {
        if let Some(sampler) = midi.sampler() {
            triggers = sampler_hits(sampler, &track_notes(p, midi), ti, &t.id);
        }
    }
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
    triggers.sort_by(|a, b| (a.start, &a.pad).cmp(&(b.start, &b.pad)));
    // The incoming hit releases the preceding voice of its group.
    let mut last: HashMap<String, usize> = HashMap::new();
    for i in 0..triggers.len() {
        let pad = &t.sound_pads()[&triggers[i].pad];
        let Some(group) = pad.choke_group.as_deref().filter(|g| !g.is_empty()) else {
            continue;
        };
        if let Some(&j) = last.get(group) {
            let start = triggers[i].start;
            let old = &mut triggers[j].cutoff;
            *old = Some(old.map_or(start, |c| c.min(start)));
        }
        last.insert(group.to_string(), i);
    }
    triggers
}

/// All triggers, sorted by (start, track, pad), with choke cutoffs applied.
pub fn schedule(p: &Project) -> Vec<Trigger> {
    let mut triggers: Vec<Trigger> = (0..p.tracks.len()).flat_map(|ti| track_triggers(p, ti)).collect();
    triggers.sort_by(|a, b| (a.start, &a.track_id, &a.pad).cmp(&(b.start, &b.track_id, &b.pad)));
    triggers
}
