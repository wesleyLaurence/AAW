//! Event scheduling: every hit's start frame, from exact beats.

use crate::{frame, Beat, Event, Instrument, Midi, PadMode, Project, Sampler, Wave};
use num_rational::BigRational;
use num_traits::ToPrimitive;
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

/// One thing a track plays, for reading its stem's spectrum against: a peak
/// that stays put while the notes move is the sound's own, and one that
/// follows them is the music.
#[derive(Clone, Debug, PartialEq)]
pub struct Sounded {
    pub track_id: String,
    /// Its start and end, in beats. A hit without a duration ends where it starts.
    pub at: f64,
    pub until: f64,
    /// The pitch it sounds at, as a MIDI number: a Synth's note at each of
    /// its oscillators, and a sample repitched to a note. None for a hit
    /// without a note, which plays its sample as it is.
    pub pitch: Option<f64>,
    /// For a hit without a note, its pad and how far the pad's and the
    /// event's transpose move it, in semitones; empty for a note.
    pub hit: String,
}

/// Everything the tracks play, a track at a time in the order it starts.
/// Audio clips are not in it: the song does not say what a file holds.
pub fn sounded(p: &Project) -> Vec<Sounded> {
    let f = |x: &BigRational| x.to_f64().unwrap_or(f64::NAN);
    let mut out = Vec::new();
    for t in &p.tracks {
        let from = out.len();
        let mut push = |at: f64, until: f64, pitch: Option<f64>, hit: String| {
            out.push(Sounded { track_id: t.id.clone(), at, until, pitch, hit });
        };
        if let Some(midi) = &t.midi {
            for n in track_notes(p, midi) {
                let (at, until) = (f(&n.at), f(&(&n.at + &n.beats)));
                match &midi.instrument {
                    Some(Instrument::Synth(synth)) => {
                        for o in synth.oscillators.values().filter(|o| o.wave != Wave::Noise) {
                            let offset = 12.0 * o.octave as f64 + o.semitones + o.detune_cents / 100.0;
                            push(at, until, Some(n.pitch as f64 + offset), String::new());
                        }
                    }
                    Some(Instrument::Sampler(sampler)) => {
                        let Some(entry) = sampler.entry(n.pitch) else { continue };
                        let Some(pad) = sampler.pads.get(&entry.pad) else { continue };
                        if entry.pitched {
                            push(at, until, Some(n.pitch as f64 + pad.transpose), String::new());
                        } else {
                            push(at, until, None, format!("{} {}", entry.pad, pad.transpose));
                        }
                    }
                    None => {}
                }
            }
        }
        for c in &t.clips {
            let Some(pattern) = p.patterns.get(&c.pattern) else { continue };
            let (length, events) = (pattern.length_exact(), pattern.expanded());
            for r in 0..c.repeats {
                let base = c.at_exact() + BigRational::from_integer(r.into()) * &length;
                for e in &events {
                    let Some(pad) = t.pads.get(&e.pad) else { continue };
                    let at = &base + e.at_exact();
                    let until = &at + e.duration_exact().unwrap_or_else(|| BigRational::from_integer(0.into()));
                    let moved = pad.transpose + e.transpose;
                    let note = e.note.as_deref().filter(|n| !n.is_empty()).and_then(|n| crate::rules::midi(n).ok());
                    match note {
                        Some(note) => push(f(&at), f(&until), Some(note as f64 + moved), String::new()),
                        None => push(f(&at), f(&until), None, format!("{} {}", e.pad, moved)),
                    }
                }
            }
        }
        out[from..].sort_by(|a, b| a.at.total_cmp(&b.at));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_each_track_plays_has_its_pitch_or_its_pad() {
        let p = crate::parse(
            r#"
session: {tempo: 120, length_beats: 16, sample_rate: 48000}
samples:
  kick: {path: kick.wav}
  bass: {path: bass.wav, root_note: C2}
patterns:
  beat: {length_beats: 4, events: [{at: 0, pad: kick}, {at: 2, pad: kick, transpose: -2}]}
  line: {length_beats: 4, events: [{at: 1, pad: bass, note: E2, duration: 1}]}
tracks:
  - id: drums
    pads: {kick: {sample: kick}, bass: {sample: bass, transpose: 12}}
    clips: [{pattern: beat, at: 0, repeats: 2}, {pattern: line, at: 4}]
    audio: [{sample: kick, at: 8}]
  - id: lead
    type: midi
    instrument: {synth: {oscillators: {a: {}, b: {octave: -1, detune_cents: 50}, n: {wave: noise}}}}
    clips: [{id: c, length_beats: 4, notes: [{pitch: A4, duration: 2}, {pitch: C5, at: 3, duration: 4}]}]
  - id: keys
    type: midi
    instrument:
      sampler:
        pads: {tone: {sample: bass}, hit: {sample: kick, transpose: 3}}
        map: [{notes: C1, pad: hit}, {notes: [C2, C6], pad: tone, pitched: true}]
    clips: [{id: k, at: 4, length_beats: 4, notes: [{pitch: C1, duration: 1}, {pitch: G3, at: 1, duration: 1}, {pitch: C0, at: 2, duration: 1}]}]
"#,
        )
        .unwrap();
        let of = |track: &str| -> Vec<(f64, f64, Option<f64>, String)> {
            sounded(&p).into_iter().filter(|s| s.track_id == track).map(|s| (s.at, s.until, s.pitch, s.hit)).collect()
        };
        // A hit without a note is its pad and its transpose; a note is the
        // pitch the sample is moved to. The audio clip is not listed.
        assert_eq!(
            of("drums"),
            vec![
                (0.0, 0.0, None, "kick 0".into()),
                (2.0, 2.0, None, "kick -2".into()),
                (4.0, 4.0, None, "kick 0".into()),
                (5.0, 6.0, Some(52.0), String::new()),
                (6.0, 6.0, None, "kick -2".into()),
            ]
        );
        // A Synth's note at each oscillator but the noise; the last note is
        // cut at its clip's end.
        assert_eq!(
            of("lead"),
            vec![
                (0.0, 2.0, Some(69.0), String::new()),
                (0.0, 2.0, Some(57.5), String::new()),
                (3.0, 4.0, Some(72.0), String::new()),
                (3.0, 4.0, Some(60.5), String::new()),
            ]
        );
        // A sampler's drum entry is a hit, a pitched one a note, and a note
        // nothing maps is silent.
        assert_eq!(of("keys"), vec![(4.0, 5.0, None, "hit 3".into()), (5.0, 6.0, Some(55.0), String::new())]);
    }
}
