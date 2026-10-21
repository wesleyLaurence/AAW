//! MIDI effects: what a MIDI track's devices before its instrument make of
//! its notes. They run in the schedule, in exact beats, so playback, a
//! render, a freeze and `sounded` play the same notes; the clips keep the
//! notes as written.

use crate::schema::{Arpeggiator, Chord, MidiEffect, Scale, Velocity};
use num_rational::BigRational;
use num_traits::ToPrimitive;

/// A note as a MIDI effect sees it: its pitch, velocity, start and length,
/// before the frames are known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Played {
    pub pitch: i64,
    pub velocity: i64,
    pub at: BigRational,
    pub beats: BigRational,
}

impl Played {
    fn end(&self) -> BigRational {
        &self.at + &self.beats
    }
}

/// The notes through the chain, each device that is not bypassed in turn,
/// sorted by start and pitch.
pub fn apply(effects: &[MidiEffect], mut notes: Vec<Played>) -> Vec<Played> {
    sort(&mut notes);
    for effect in effects.iter().filter(|e| !e.bypass()) {
        notes = match effect {
            MidiEffect::Arpeggiator(a) => arpeggiate(a, &notes),
            MidiEffect::Chord(c) => chord(c, &notes),
            MidiEffect::Scale(s) => scale(s, &notes),
            MidiEffect::Velocity(v) => velocity(v, &notes),
        };
        sort(&mut notes);
    }
    notes
}

fn sort(notes: &mut [Played]) {
    notes.sort_by(|a, b| (&a.at, a.pitch, &a.beats, a.velocity).cmp(&(&b.at, b.pitch, &b.beats, b.velocity)));
}

fn in_range(pitch: i64) -> bool {
    (0..=127).contains(&pitch)
}

/// A number in [0, 1) from a seed and a key, the same every time: SplitMix64.
fn unit(seed: i64, key: &[u64]) -> f64 {
    let mut z = (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    for k in key {
        z = z.wrapping_add(k.wrapping_mul(0xD1B5_4A32_D192_ED03)).wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
    }
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// A key for a note, from where it is and what it is, so that a note's
/// draw does not change when another is added or removed.
fn note_key(n: &Played, occurrence: u64) -> Vec<u64> {
    let f = |x: &BigRational| {
        let s = x.to_string();
        s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
    };
    vec![f(&n.at), f(&n.beats), n.pitch as u64, occurrence]
}

/// A walk up and back without repeating the ends: a, b, c, b.
fn inner<T: Copy>(v: &[T]) -> Vec<T> {
    let mut all = v.to_vec();
    if v.len() > 2 {
        all.extend(v[1..v.len() - 1].iter().rev().copied());
    }
    all
}

/// The ends toward the middle: first, last, second, second to last.
fn converge<T: Copy>(v: &[T]) -> Vec<T> {
    let (mut i, mut j) = (0usize, v.len());
    let mut all = Vec::with_capacity(v.len());
    while i < j {
        all.push(v[i]);
        i += 1;
        if i < j {
            j -= 1;
            all.push(v[j]);
        }
    }
    all
}

/// The notes held at each step of the grid, one at a time in the order,
/// over the octaves; the pattern starts over when a note begins.
fn arpeggiate(arp: &Arpeggiator, notes: &[Played]) -> Vec<Played> {
    let mut out = Vec::new();
    if notes.is_empty() {
        return out;
    }
    let step = arp.step();
    let gate = BigRational::from_float(arp.gate_percent).unwrap_or_else(|| BigRational::from_integer(50.into())) / BigRational::from_integer(100.into());
    let length = &step * &gate;
    let first = notes.iter().map(|n| &n.at).min().expect("a note");
    let last = notes.iter().map(Played::end).max().expect("a note");
    let mut number = (first / &step).ceil();
    let mut t = &number * &step;
    let mut prev: Option<BigRational> = None;
    let mut k: Option<usize> = None;
    while t < last {
        let mut held: Vec<&Played> = notes.iter().filter(|n| n.at <= t && t < n.end()).collect();
        if held.is_empty() {
            k = None;
            prev = Some(t.clone());
            number += BigRational::from_integer(1.into());
            t = &number * &step;
            continue;
        }
        // The notes that began since the last step start the pattern over.
        let began = held.iter().any(|n| prev.as_ref().is_none_or(|p| n.at > *p));
        k = Some(match k {
            Some(k) if !began => k + 1,
            _ => 0,
        });
        held.sort_by_key(|n| n.pitch);
        let octaves = 0..arp.octaves.max(1);
        let up: Vec<(i64, &Played)> = octaves.clone().flat_map(|o| held.iter().map(move |n| (n.pitch + 12 * o, *n))).collect();
        let sequence: Vec<(i64, &Played)> = match arp.order.as_str() {
            "down" => up.iter().rev().copied().collect(),
            "up_down" => inner(&up),
            "down_up" => inner(&up.iter().rev().copied().collect::<Vec<_>>()),
            "converge" => converge(&up),
            "diverge" => converge(&up).into_iter().rev().collect(),
            "played" => {
                let mut by_start = held.clone();
                by_start.sort_by(|a, b| (&a.at, a.pitch).cmp(&(&b.at, b.pitch)));
                octaves.flat_map(|o| by_start.iter().map(move |n| (n.pitch + 12 * o, *n))).collect()
            }
            _ => up,
        };
        let index = if arp.order == "random" {
            let n = number.to_integer().to_u64().unwrap_or(0);
            (unit(arp.seed, &[n]) * sequence.len() as f64) as usize
        } else {
            k.unwrap_or(0) % sequence.len()
        };
        let (pitch, source) = sequence[index.min(sequence.len() - 1)];
        let end = (&t + &length).min(source.end());
        if in_range(pitch) && end > t {
            out.push(Played { pitch, velocity: source.velocity, at: t.clone(), beats: &end - &t });
        }
        prev = Some(t.clone());
        number += BigRational::from_integer(1.into());
        t = &number * &step;
    }
    out
}

/// Each note with the notes its shifts add; a shift of 0 adds nothing, and
/// two shifts alike add one note.
fn chord(c: &Chord, notes: &[Played]) -> Vec<Played> {
    let mut shifts: Vec<i64> = c.shifts.iter().copied().filter(|s| *s != 0).collect();
    shifts.sort_unstable();
    shifts.dedup();
    let mut out = Vec::with_capacity(notes.len() * (1 + shifts.len()));
    for n in notes {
        out.push(n.clone());
        for s in &shifts {
            if in_range(n.pitch + s) {
                out.push(Played { pitch: n.pitch + s, ..n.clone() });
            }
        }
    }
    out
}

/// Each note at the nearest note of the scale, a note halfway going down,
/// then moved by the transpose; a note moved off the keyboard is dropped.
fn scale(s: &Scale, notes: &[Played]) -> Vec<Played> {
    let root = s.root_class();
    let degrees = s.degrees();
    notes
        .iter()
        .filter_map(|n| {
            let class = (n.pitch - root).rem_euclid(12);
            let mut best: Option<i64> = None;
            for d in degrees {
                for diff in [d - class, d - class - 12, d - class + 12] {
                    let better = match best {
                        None => true,
                        Some(b) => diff.abs() < b.abs() || (diff.abs() == b.abs() && diff < b),
                    };
                    if better {
                        best = Some(diff);
                    }
                }
            }
            let pitch = n.pitch + best.unwrap_or(0) + s.transpose;
            in_range(pitch).then(|| Played { pitch, ..n.clone() })
        })
        .collect()
}

/// Each note's velocity scaled, moved, spread and held in the range.
fn velocity(v: &Velocity, notes: &[Played]) -> Vec<Played> {
    let mut seen: std::collections::HashMap<(String, i64), u64> = std::collections::HashMap::new();
    notes
        .iter()
        .map(|n| {
            let occurrence = seen.entry((n.at.to_string(), n.pitch)).or_insert(0);
            let spread = if v.random > 0 {
                let draw = unit(v.seed, &note_key(n, *occurrence));
                (v.random as f64 * (2.0 * draw - 1.0)).round()
            } else {
                0.0
            };
            *occurrence += 1;
            let value = (n.velocity as f64 * v.scale_percent / 100.0 + v.offset as f64 + spread).round() as i64;
            Played { velocity: value.clamp(v.min, v.max), ..n.clone() }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::MidiEffect;

    fn r(n: i64, d: i64) -> BigRational {
        BigRational::new(n.into(), d.into())
    }

    fn note(pitch: i64, at: BigRational, beats: BigRational) -> Played {
        Played { pitch, velocity: 100, at, beats }
    }

    fn arp(with: &str) -> MidiEffect {
        let text = format!("session: {{}}\ntracks: [{{id: t, type: midi, midi_effects: [{{type: arpeggiator{with}}}]}}]");
        let p = crate::parse(&text).unwrap();
        p.tracks[0].midi.as_ref().unwrap().midi_effects[0].clone()
    }

    fn chain(yaml: &str) -> Vec<MidiEffect> {
        let text = format!("session: {{}}\ntracks: [{{id: t, type: midi, midi_effects: {yaml}}}]");
        crate::parse(&text).unwrap().tracks[0].midi.as_ref().unwrap().midi_effects.clone()
    }

    fn played(notes: &[Played]) -> Vec<(i64, String, String, i64)> {
        notes.iter().map(|n| (n.pitch, n.at.to_string(), n.beats.to_string(), n.velocity)).collect()
    }

    #[test]
    fn an_arpeggiator_walks_a_held_chord_on_the_grid() {
        // A C major chord held two beats from beat 1: sixteenths from beat 1,
        // each half a step long, up and around.
        let chord = vec![note(60, r(1, 1), r(2, 1)), note(64, r(1, 1), r(2, 1)), note(67, r(1, 1), r(2, 1))];
        let out = apply(&[arp("")], chord.clone());
        let pitches: Vec<i64> = out.iter().map(|n| n.pitch).collect();
        assert_eq!(pitches, vec![60, 64, 67, 60, 64, 67, 60, 64]);
        assert_eq!(out[0].at, r(1, 1));
        assert_eq!(out[1].at, r(5, 4));
        assert!(out.iter().all(|n| n.beats == r(1, 8)));
        // Down, over two octaves, with a full gate.
        let out = apply(&[arp(", order: down, octaves: 2, gate_percent: 100, rate: 1/8")], chord.clone());
        let pitches: Vec<i64> = out.iter().map(|n| n.pitch).collect();
        assert_eq!(pitches, vec![79, 76, 72, 67]);
        assert!(out.iter().all(|n| n.beats == r(1, 2)));
        // Up and down does not repeat the ends.
        let out = apply(&[arp(", order: up_down")], chord.clone());
        assert_eq!(out.iter().map(|n| n.pitch).collect::<Vec<_>>(), vec![60, 64, 67, 64, 60, 64, 67, 64]);
        // Converge and diverge.
        let out = apply(&[arp(", order: converge, rate: 1/8")], chord.clone());
        assert_eq!(out.iter().map(|n| n.pitch).collect::<Vec<_>>(), vec![60, 67, 64, 60]);
        let out = apply(&[arp(", order: diverge, rate: 1/8")], chord.clone());
        assert_eq!(out.iter().map(|n| n.pitch).collect::<Vec<_>>(), vec![64, 67, 60, 64]);
    }

    #[test]
    fn an_arpeggiator_starts_over_when_a_note_begins_and_cuts_at_the_source() {
        // Two notes, then a third joining a beat later: the pattern restarts
        // with it. The last step's note is cut where its source ends.
        let notes = vec![note(60, r(0, 1), r(2, 1)), note(64, r(0, 1), r(2, 1)), note(67, r(1, 1), r(5, 4))];
        let out = apply(&[arp(", rate: 1/8, gate_percent: 200")], notes);
        assert_eq!(
            played(&out),
            vec![
                (60, "0".into(), "1".into(), 100),
                (64, "1/2".into(), "1".into(), 100),
                (60, "1".into(), "1".into(), 100),
                (64, "3/2".into(), "1/2".into(), 100),
                (67, "2".into(), "1/4".into(), 100),
            ]
        );
        // Off the grid, the first step is the next grid point; a gap ends
        // the run and the next chord starts its own.
        let notes = vec![note(60, r(1, 3), r(1, 1)), note(62, r(3, 1), r(1, 2))];
        let out = apply(&[arp(", rate: 1/8")], notes);
        assert_eq!(out.iter().map(|n| (n.pitch, n.at.to_string())).collect::<Vec<_>>(), vec![(60, "1/2".into()), (60, "1".into()), (62, "3".into())]);
        // Played order keeps the order of the starts; random is stable.
        let notes = vec![note(67, r(0, 1), r(2, 1)), note(60, r(1, 4), r(2, 1))];
        let out = apply(&[arp(", order: played, rate: 1/8")], notes.clone());
        assert_eq!(out.iter().map(|n| n.pitch).collect::<Vec<_>>(), vec![67, 67, 60, 67, 60]);
        let a = apply(&[arp(", order: random, seed: 3")], notes.clone());
        let b = apply(&[arp(", order: random, seed: 3")], notes.clone());
        assert_eq!(a, b);
        assert!(a.iter().all(|n| n.pitch == 60 || n.pitch == 67));
        // A note shorter than a step between two steps is not heard; an
        // empty chain or a bypassed device is the notes as written.
        let short = vec![note(60, r(1, 8), r(1, 16))];
        assert!(apply(&[arp("")], short.clone()).is_empty());
        assert_eq!(apply(&[arp(", bypass: true")], short.clone()), short);
        assert_eq!(apply(&[], short.clone()), short);
    }

    #[test]
    fn chord_scale_and_velocity_change_the_notes_and_keep_the_keyboard() {
        let notes = vec![note(61, r(0, 1), r(1, 1)), note(126, r(1, 1), r(1, 1))];
        let out = apply(&chain("[{type: chord, shift_1: 4, shift_2: 7, shift_3: 7}]"), notes.clone());
        assert_eq!(out.iter().map(|n| n.pitch).collect::<Vec<_>>(), vec![61, 65, 68, 126]);
        // C# snaps down to C in C major (a semitone either way goes down),
        // F# snaps to G in D major... and a transpose follows the snap.
        let out = apply(&chain("[{type: scale}]"), vec![note(61, r(0, 1), r(1, 1)), note(66, r(1, 1), r(1, 1))]);
        assert_eq!(out.iter().map(|n| n.pitch).collect::<Vec<_>>(), vec![60, 65]);
        let out = apply(&chain("[{type: scale, root: D, scale: minor, transpose: 12}]"), vec![note(66, r(0, 1), r(1, 1))]);
        assert_eq!(out.iter().map(|n| n.pitch).collect::<Vec<_>>(), vec![77]);
        let out = apply(&chain("[{type: scale, transpose: 36}]"), vec![note(120, r(0, 1), r(1, 1))]);
        assert!(out.is_empty());
        // Velocity: scaled, moved, held, and spread the same each time.
        let out = apply(&chain("[{type: velocity, scale_percent: 50, offset: 10, min: 20, max: 55}]"), notes.clone());
        assert_eq!(out.iter().map(|n| n.velocity).collect::<Vec<_>>(), vec![55, 55]);
        let a = apply(&chain("[{type: velocity, random: 20, seed: 9}]"), notes.clone());
        let b = apply(&chain("[{type: velocity, random: 20, seed: 9}]"), notes.clone());
        assert_eq!(a, b);
        assert!(a.iter().all(|n| (80..=120).contains(&n.velocity)));
        assert_ne!(a.iter().map(|n| n.velocity).collect::<Vec<_>>(), vec![100, 100]);
        // A chord then an arpeggiator arpeggiates the chord.
        let out = apply(&chain("[{type: chord, shift_1: 7}, {type: arpeggiator, rate: 1/8}]"), vec![note(60, r(0, 1), r(1, 1))]);
        assert_eq!(out.iter().map(|n| n.pitch).collect::<Vec<_>>(), vec![60, 67]);
    }
}
