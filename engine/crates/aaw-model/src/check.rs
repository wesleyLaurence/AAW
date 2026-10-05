//! What `daw check` says of a valid song: about the file, and about the music
//! an agent writes without hearing it. Each warning has a code, a message in
//! words, the paths of what it is about and, when it has a place in time, the
//! beat. Checks read the song, and the length of each sample's file, which an
//! audio clip without an end needs; a check that needs the audio belongs to
//! perception.

use crate::beat::signed_beat;
use crate::pyfmt::{float_repr, format_g};
use crate::rules::{note_name, owners, target, Owner, TargetKind, MIDDLE_C};
use crate::schedule::track_notes;
use crate::schema::{Instrument, Midi, Pad, Project, Stretch, Track, Wave};
use crate::{fraction_str, Beat};
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive};
use serde_json::{json, Map, Value as Json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// How far apart two beats may be and still be the same: an audio clip's end
/// comes from seconds.
const EPS: f64 = 1e-6;
/// 4/4 is the only time signature.
const BAR: f64 = 4.0;
/// C3: the notes below it are the low register `register-crowded` watches.
const LOW: i64 = 48;
/// How far a sample is repitched before it sounds artificial.
const FAR_SEMITONES: f64 = 24.0;
const HEARING_HZ: f64 = 20.0;

/// Every code `daw check` gives, with what it means, for `daw describe check`.
pub const CODES: &[(&str, &str)] = &[
    ("clips-stacked", "Two or more clips on one track start on the same beat with the same music, so it plays twice at once. Almost always a mistake: remove all but one, or move the others."),
    ("clips-overlap", "Information: two clips on one track sound at once, from the beat given. Sometimes deliberate, as a kick clip and a hat clip on one drum track."),
    ("note-retriggered", "Notes start while a note of the same pitch on the same track is still sounding, so the pitch is struck again before it ends. Shorten the earlier note or remove the later."),
    ("song-ends-inside", "An audio clip runs past session.length_beats and its end is cut off. daw timeline SONG --fit sets the length to the last sound."),
    ("track-empty", "A track has an instrument and nothing to play, notes and no instrument, or nothing at all."),
    ("register-crowded", "Two tracks hold notes in the same octave below C3 in the same bars, listed. Two low parts in one register are the most common cause of a muddy mix: move one an octave, or give those bars to one."),
    ("pad-far-from-root", "A sample is repitched more than two octaves from its root, by the note, the event's and the pad's transpose, where it sounds artificial."),
    ("note-below-hearing", "A Synth note's lowest oscillator is under 20 Hz, which is felt rather than heard."),
    ("notes-outside-clip", "Notes start before their clip or at or after its end; they are kept and do not play."),
    ("notes-unmapped", "A sampler maps no pad to notes of a clip, which are silent."),
    ("macro-unused", "A Synth macro that no modulation entry uses moves nothing."),
    ("voices-exceeded", "More notes sound at once than the Synth has voices, so the oldest are cut off."),
    ("release-cut", "The last note's release runs past the song's end, and the end fade cuts it."),
    ("stretch-without-tempo", "A pad or audio clip is to be stretched with preserve_pitch but has no source_bpm, so nothing is stretched."),
    ("stretch-audible", "A pad or audio clip is stretched more than about 8% from its source_bpm, which can be heard."),
    ("lane-on-bypassed-effect", "An automation lane moves a field of a bypassed effect."),
    ("sample-unreadable", "A sample's file is not audio the engine reads, so a render fails."),
    ("root-note-mismatch", "A sample's root_note disagrees with the pitch measured from its audio."),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Warning,
    /// Reported, and often deliberate.
    Info,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Warning {
    pub code: &'static str,
    pub level: Level,
    pub message: String,
    /// What it is about, as commands name it: `tracks.drums.clips.0`.
    pub paths: Vec<String>,
    /// The song beat it is at, when it has one.
    pub at: Option<BigRational>,
}

impl Warning {
    fn new(code: &'static str, message: String, paths: Vec<String>) -> Self {
        debug_assert!(CODES.iter().any(|(c, _)| *c == code), "{code}");
        Warning { code, level: Level::Warning, message, paths, at: None }
    }

    fn at(mut self, at: BigRational) -> Self {
        self.at = Some(at);
        self
    }

    fn info(mut self) -> Self {
        self.level = Level::Info;
        self
    }

    pub fn to_json(&self) -> Json {
        let mut o = Map::new();
        o.insert("code".into(), json!(self.code));
        o.insert("level".into(), json!(if self.level == Level::Info { "info" } else { "warning" }));
        o.insert("message".into(), json!(self.message));
        o.insert("paths".into(), json!(self.paths));
        if let Some(at) = &self.at {
            o.insert("at".into(), beat_json(at));
        }
        Json::Object(o)
    }
}

/// A beat as JSON: an integer, an exact decimal or a fraction string.
fn beat_json(x: &BigRational) -> Json {
    if x.is_integer() {
        return json!(x.to_integer().to_i64());
    }
    let f = x.to_f64().unwrap_or(f64::NAN);
    if signed_beat(&Beat::Float(f)).ok().as_ref() == Some(x) {
        json!(f)
    } else {
        json!(fraction_str(x))
    }
}

/// A beat in a message, written as commands take it.
fn beat_text(x: &BigRational) -> String {
    match beat_json(x) {
        Json::Number(n) if n.is_f64() => float_repr(n.as_f64().unwrap_or(f64::NAN)),
        Json::Number(n) => n.to_string(),
        Json::String(s) => s,
        other => other.to_string(),
    }
}

fn number(x: f64) -> String {
    format_g(x, 4)
}

fn f(x: &BigRational) -> f64 {
    x.to_f64().unwrap_or(f64::NAN)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Paths and names, separated as a sentence does.
fn and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Bars counted from 1, consecutive ones run together: `1–8, 17`.
fn bar_runs(bars: &BTreeSet<i64>) -> String {
    let mut runs: Vec<(i64, i64)> = Vec::new();
    for &b in bars {
        match runs.last_mut() {
            Some((_, end)) if *end + 1 == b => *end = b,
            _ => runs.push((b, b)),
        }
    }
    runs.iter()
        .map(|&(a, b)| if a == b { format!("{}", a + 1) } else { format!("{}–{}", a + 1, b + 1) })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Everything `daw check` warns about in a valid song. `seconds` is the
/// length of each sample's file, by sample ID; an audio clip without an end
/// whose file is not in it is left out of the checks of where clips sound.
pub fn check(p: &Project, seconds: &HashMap<String, f64>) -> Vec<Warning> {
    let mut out = stretch(p);
    out.extend(bypassed_lanes(p));
    out.extend(notes_outside(p));
    out.extend(synths(p));
    for (ti, t) in p.tracks.iter().enumerate() {
        let spans = spans(p, ti, seconds);
        let stacked = stacked(t, &spans, &mut out);
        overlaps(t, &spans, &stacked, &mut out);
        ends(p, &spans, &mut out);
        if let Some(midi) = &t.midi {
            retriggered(t, midi, &spans, &stacked, &mut out);
        }
        empty(t, &mut out);
        far_from_root(p, t, &mut out);
        below_hearing(p, t, &mut out);
    }
    crowded(p, &mut out);
    out
}

fn pad_path(t: &Track, name: &str) -> String {
    if t.midi.is_some() {
        format!("tracks.{}.instrument.sampler.pads.{name}", t.id)
    } else {
        format!("tracks.{}.pads.{name}", t.id)
    }
}

/// Pads and audio clips stretched far enough to hear, or with nothing to
/// stretch to.
fn stretch(p: &Project) -> Vec<Warning> {
    let mut out = Vec::new();
    for t in &p.tracks {
        let pads = t.sound_pads().iter().map(|(name, pad)| (name.clone(), pad_path(t, name), pad.stretch, pad.source_bpm));
        let clips = t
            .audio
            .iter()
            .enumerate()
            .map(|(i, clip)| (format!("audio.{i}"), format!("tracks.{}.audio.{i}", t.id), clip.stretch, clip.source_bpm));
        for (name, path, stretch, source_bpm) in pads.chain(clips) {
            if stretch != Stretch::PreservePitch {
                continue;
            }
            match source_bpm {
                None => out.push(Warning::new(
                    "stretch-without-tempo",
                    format!("{}.{name}: stretch is preserve_pitch but source_bpm is not set, so nothing is stretched", t.id),
                    vec![path],
                )),
                Some(bpm) => {
                    let percent = (p.session.tempo / bpm - 1.0) * 100.0;
                    if percent.abs() > 8.0 {
                        out.push(Warning::new(
                            "stretch-audible",
                            format!("{}.{name}: stretched {percent:+.1}% from {bpm} BPM; more than about 8% can be heard", t.id),
                            vec![path],
                        ));
                    }
                }
            }
        }
    }
    out
}

fn bypassed_lanes(p: &Project) -> Vec<Warning> {
    let mut out = Vec::new();
    for owner in owners(p) {
        let prefix = match owner {
            Owner::Track(t) => format!("tracks.{}", t.id),
            Owner::Return(r) => format!("returns.{}", r.id),
            Owner::Master(_) => "master".into(),
        };
        for (i, lane) in owner.automation().iter().enumerate() {
            let Ok(t) = target(owner, &lane.param) else { continue };
            if let TargetKind::Effect { index, .. } = t.kind {
                if owner.effects()[index].bypass() {
                    out.push(Warning::new(
                        "lane-on-bypassed-effect",
                        format!("{}: {} automates a bypassed effect", owner.name(), lane.param),
                        vec![format!("{prefix}.automation.{i}")],
                    ));
                }
            }
        }
    }
    out
}

/// Notes that start before their clip or at or after its end, which are kept
/// and do not play, and those the track's sampler maps to no pad, which are
/// silent.
fn notes_outside(p: &Project) -> Vec<Warning> {
    let mut out = Vec::new();
    for t in &p.tracks {
        let Some(midi) = &t.midi else { continue };
        let sampler = midi.sampler();
        for clip in &midi.clips {
            let path = format!("tracks.{}.clips.{}", t.id, clip.id);
            let length = clip.length_exact();
            let ids = |keep: &dyn Fn(&BigRational) -> bool| -> Vec<&str> {
                clip.notes.iter().filter(|n| keep(&n.at_exact())).map(|n| n.id.as_str()).collect()
            };
            for (outside, place) in [
                (ids(&|at| at.is_negative()), "before the clip's start"),
                (ids(&|at| *at >= length), "at or after the clip's end"),
            ] {
                if !outside.is_empty() {
                    out.push(Warning::new(
                        "notes-outside-clip",
                        format!("{}.{}: notes {} start {place} and do not play", t.id, clip.id, outside.join(", ")),
                        vec![path.clone()],
                    ));
                }
            }
            let Some(sampler) = sampler else { continue };
            let mut unmapped: Vec<i64> = clip
                .notes
                .iter()
                .filter(|n| clip.plays(n) && sampler.entry(n.pitch).is_none())
                .map(|n| n.pitch)
                .collect();
            unmapped.sort_unstable();
            unmapped.dedup();
            if !unmapped.is_empty() {
                let names: Vec<String> = unmapped
                    .iter()
                    .map(|n| format!("{n} ({})", note_name(*n).unwrap_or_default()))
                    .collect();
                out.push(Warning::new(
                    "notes-unmapped",
                    format!("{}.{}: the sampler maps no pad to notes {}, which are silent", t.id, clip.id, names.join(", ")),
                    vec![path],
                ));
            }
        }
    }
    out
}

/// A macro no entry uses, more notes stacked than the synth has voices, and
/// a release the song's end cuts short.
fn synths(p: &Project) -> Vec<Warning> {
    let mut out = Vec::new();
    let length = p.session.length_exact();
    for t in &p.tracks {
        let Some(midi) = &t.midi else { continue };
        let Some(synth) = midi.synth() else { continue };
        let path = format!("tracks.{}.instrument.synth", t.id);
        for name in synth.macros.keys() {
            if !synth.modulation.iter().any(|m| m.source == format!("macros.{name}")) {
                out.push(Warning::new(
                    "macro-unused",
                    format!("{}: macro {name} moves nothing; the matrix has no entry with source macros.{name}", t.id),
                    vec![format!("{path}.macros.{name}")],
                ));
            }
        }
        // The notes as they sound, in order: the most at once against the voices.
        let notes = track_notes(p, midi);
        let mut edges: Vec<(i64, i32, usize)> =
            notes.iter().enumerate().flat_map(|(i, n)| [(n.start, 1, i), (n.end, -1, i)]).collect();
        edges.sort_by_key(|&(frame, d, _)| (frame, d));
        let (mut now, mut most, mut when) = (0i64, 0i64, None);
        for (_, d, i) in edges {
            now += i64::from(d);
            if now > most {
                most = now;
                when = Some(i);
            }
        }
        if most > synth.voices {
            let mut w = Warning::new(
                "voices-exceeded",
                format!("{}: {most} notes sound at once and the synth has {} voices, so the oldest are cut off", t.id, synth.voices),
                vec![path.clone()],
            );
            if let Some(i) = when {
                w = w.at(notes[i].at.clone());
            }
            out.push(w);
        }
        let release = synth.envelopes.get("amp").map_or(0.0, |e| e.release_ms);
        let tail = release / 1000.0 * p.session.tempo / 60.0;
        if let Some(last) = notes.iter().map(|n| &n.at + &n.beats).max() {
            let ends = last + BigRational::from_float(tail).unwrap_or_default();
            if ends > length {
                out.push(
                    Warning::new(
                        "release-cut",
                        format!(
                            "{}: the last note's release ends at beat {} and the song ends at {}, so the end fade cuts it",
                            t.id,
                            format_g(ends.to_f64().unwrap_or(f64::NAN), 4),
                            p.session.length_beats.text()
                        ),
                        vec![path],
                    )
                    .at(length.clone()),
                );
            }
        }
    }
    out
}

/// A clip as the checks of where clips sound see it.
struct Span {
    path: String,
    /// Its index in the track's list, and whether that list is `audio`.
    index: usize,
    audio: bool,
    start: BigRational,
    /// None for an audio clip whose file's length is not known.
    end: Option<f64>,
    /// The same music has the same key.
    music: String,
}

/// Every clip of track `ti`: pattern clips, audio clips and note clips.
fn spans(p: &Project, ti: usize, seconds: &HashMap<String, f64>) -> Vec<Span> {
    let t = &p.tracks[ti];
    let mut out = Vec::new();
    for (i, c) in t.clips.iter().enumerate() {
        let Some(pattern) = p.patterns.get(&c.pattern) else { continue };
        let start = c.at_exact();
        let span = pattern.length_exact() * BigRational::from_integer(c.repeats.into());
        out.push(Span {
            path: format!("tracks.{}.clips.{i}", t.id),
            index: i,
            audio: false,
            end: Some(f(&(&start + span))),
            start,
            music: format!("pattern {}", c.pattern),
        });
    }
    for (i, c) in t.audio.iter().enumerate() {
        let start = c.at_exact();
        let until = c.source_end_seconds.or_else(|| seconds.get(&c.sample).copied());
        let bpm = c.source_bpm.unwrap_or(p.session.tempo);
        out.push(Span {
            path: format!("tracks.{}.audio.{i}", t.id),
            index: i,
            audio: true,
            end: until.map(|u| f(&start) + (u - c.source_start_seconds).max(0.0) * bpm / 60.0),
            start,
            music: format!("audio {} {}", c.sample, float_repr(c.source_start_seconds)),
        });
    }
    if let Some(midi) = &t.midi {
        for (i, c) in midi.clips.iter().enumerate() {
            let start = c.at_exact();
            let mut notes: Vec<String> = c
                .notes
                .iter()
                .filter(|n| c.plays(n))
                .map(|n| format!("{}:{}:{}:{}", fraction_str(&n.at_exact()), n.pitch, fraction_str(&n.duration_exact()), n.velocity))
                .collect();
            notes.sort();
            out.push(Span {
                path: format!("tracks.{}.clips.{}", t.id, c.id),
                index: i,
                audio: false,
                end: Some(f(&(&start + c.length_exact()))),
                start,
                music: format!("notes {}", notes.join(",")),
            });
        }
    }
    out
}

/// `clips-stacked`, a warning a group; returns the pairs of `spans` indices
/// that are stacked, which are not also reported as overlapping.
fn stacked(t: &Track, spans: &[Span], out: &mut Vec<Warning>) -> HashSet<(usize, usize)> {
    let mut groups: BTreeMap<(BigRational, &str), Vec<usize>> = BTreeMap::new();
    for (i, s) in spans.iter().enumerate() {
        groups.entry((s.start.clone(), s.music.as_str())).or_default().push(i);
    }
    let mut pairs = HashSet::new();
    // In the order they play.
    for ((at, _), group) in groups.into_iter().filter(|(_, g)| g.len() > 1) {
        for &a in &group {
            for &b in &group {
                if a < b {
                    pairs.insert((a, b));
                }
            }
        }
        let paths: Vec<String> = group.iter().map(|&i| spans[i].path.clone()).collect();
        let times = match group.len() {
            2 => "twice".to_string(),
            n => format!("{n} times"),
        };
        out.push(
            Warning::new(
                "clips-stacked",
                format!(
                    "{}: {} start on beat {} with the same music, so it plays {times} at once",
                    t.id,
                    and(&paths),
                    beat_text(&at)
                ),
                paths,
            )
            .at(at),
        );
    }
    pairs
}

/// `clips-overlap`: two clips of a track sounding at once, as information.
fn overlaps(t: &Track, spans: &[Span], stacked: &HashSet<(usize, usize)>, out: &mut Vec<Warning>) {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by(|&a, &b| spans[a].start.cmp(&spans[b].start).then(a.cmp(&b)));
    for (k, &a) in order.iter().enumerate() {
        for &b in &order[k + 1..] {
            if stacked.contains(&(a.min(b), a.max(b))) {
                continue;
            }
            let (Some(ea), Some(eb)) = (spans[a].end, spans[b].end) else { continue };
            let from = spans[b].start.clone();
            let until = ea.min(eb);
            if until - f(&from) > EPS {
                out.push(
                    Warning::new(
                        "clips-overlap",
                        format!(
                            "{}: {} and {} sound at once from beat {} to {}",
                            t.id,
                            spans[a].path,
                            spans[b].path,
                            beat_text(&from),
                            number(until)
                        ),
                        vec![spans[a].path.clone(), spans[b].path.clone()],
                    )
                    .at(from)
                    .info(),
                );
            }
        }
    }
}

/// `song-ends-inside`: an audio clip that sounds past the song's end. A
/// pattern or note clip that would is refused by validation.
fn ends(p: &Project, spans: &[Span], out: &mut Vec<Warning>) {
    let length = p.session.length_exact();
    for s in spans.iter().filter(|s| s.audio) {
        let Some(end) = s.end else { continue };
        if end > f(&length) + EPS {
            out.push(
                Warning::new(
                    "song-ends-inside",
                    format!(
                        "{} runs to beat {} and the song ends at {}, so its last {} beats are cut off",
                        s.path,
                        number(end),
                        beat_text(&length),
                        number(end - f(&length))
                    ),
                    vec![s.path.clone()],
                )
                .at(length.clone()),
            );
        }
    }
}

/// `note-retriggered`, a warning for each clip whose notes strike a pitch
/// still sounding. Notes of two stacked clips are already reported.
fn retriggered(t: &Track, midi: &Midi, spans: &[Span], stacked: &HashSet<(usize, usize)>, out: &mut Vec<Warning>) {
    // The index in `spans` of each note clip.
    let span_of: HashMap<usize, usize> =
        spans.iter().enumerate().filter(|(_, s)| !s.audio).map(|(i, s)| (s.index, i)).collect();
    let mut notes: Vec<(i64, BigRational, BigRational, usize)> = Vec::new();
    for (ci, c) in midi.clips.iter().enumerate() {
        let (base, length) = (c.at_exact(), c.length_exact());
        for n in c.notes.iter().filter(|n| c.plays(n)) {
            let at = n.at_exact();
            let until = (&at + n.duration_exact()).min(length.clone());
            notes.push((n.pitch, &base + at, &base + until, ci));
        }
    }
    notes.sort();
    // For each clip: how many, and the first.
    let mut struck: BTreeMap<usize, (usize, BigRational, i64)> = BTreeMap::new();
    let mut sounding: Option<(i64, BigRational, usize)> = None;
    for (pitch, start, end, ci) in notes {
        if let Some((p0, until, cj)) = &sounding {
            let pair = (span_of[cj].min(span_of[&ci]), span_of[cj].max(span_of[&ci]));
            if *p0 == pitch && start < *until && !stacked.contains(&pair) {
                let e = struck.entry(ci).or_insert((0, start.clone(), pitch));
                e.0 += 1;
                if start < e.1 {
                    (e.1, e.2) = (start.clone(), pitch);
                }
            }
        }
        match &sounding {
            Some((p0, until, _)) if *p0 == pitch && *until >= end => {}
            _ => sounding = Some((pitch, end, ci)),
        }
    }
    for (ci, (count, at, pitch)) in struck {
        let c = &midi.clips[ci];
        out.push(
            Warning::new(
                "note-retriggered",
                format!(
                    "{}.{}: {} while a note of the same pitch is still sounding, the first {} at beat {}",
                    t.id,
                    c.id,
                    plural(count, "note starts", "notes start"),
                    note_name(pitch).unwrap_or_default(),
                    beat_text(&at)
                ),
                vec![format!("tracks.{}.clips.{}", t.id, c.id)],
            )
            .at(at),
        );
    }
}

/// `track-empty`: an instrument and nothing to play, notes and no
/// instrument, or nothing.
fn empty(t: &Track, out: &mut Vec<Warning>) {
    let message = match &t.midi {
        Some(midi) => {
            let notes: usize = midi.clips.iter().map(|c| c.notes.iter().filter(|n| c.plays(n)).count()).sum();
            match (&midi.instrument, notes) {
                (Some(i), 0) => Some(format!("a {} and no notes for it to play", i.kind())),
                (None, 0) => Some("no instrument and no notes".to_string()),
                (None, n) => Some(format!("{} and no instrument, so nothing is heard", plural(n, "note", "notes"))),
                _ => None,
            }
        }
        None if !t.clips.is_empty() || !t.audio.is_empty() => None,
        None if !t.pads.is_empty() => Some("pads and no clips to play them".to_string()),
        None => Some("no pads, clips or audio".to_string()),
    };
    if let Some(m) = message {
        out.push(Warning::new("track-empty", format!("{}: {m}", t.id), vec![format!("tracks.{}", t.id)]));
    }
}

/// A sample's repitch for a hit: the pad's and the event's transpose, and the
/// note against the sample's root, or middle C when it has none.
fn shift(p: &Project, pad: &Pad, note: Option<i64>, transpose: f64) -> f64 {
    let mut semitones = pad.transpose + transpose;
    if let Some(note) = note {
        let root = p.samples[&pad.sample]
            .root_note
            .as_deref()
            .filter(|r| !r.is_empty())
            .and_then(|r| crate::rules::midi(r).ok())
            .unwrap_or(MIDDLE_C);
        semitones += (note - root) as f64;
    }
    semitones
}

/// `pad-far-from-root`, a warning a pad.
fn far_from_root(p: &Project, t: &Track, out: &mut Vec<Warning>) {
    // Each hit: the pad, its beat and its repitch.
    let mut hits: Vec<(&str, BigRational, f64)> = Vec::new();
    for c in &t.clips {
        let Some(pattern) = p.patterns.get(&c.pattern) else { continue };
        let (length, events) = (pattern.length_exact(), pattern.expanded());
        for r in 0..c.repeats {
            let base = c.at_exact() + BigRational::from_integer(r.into()) * &length;
            for e in &events {
                let Some((name, pad)) = t.pads.get_key_value(&e.pad) else { continue };
                let note = e.note.as_deref().filter(|n| !n.is_empty()).and_then(|n| crate::rules::midi(n).ok());
                hits.push((name.as_str(), &base + e.at_exact(), shift(p, pad, note, e.transpose)));
            }
        }
    }
    if let Some(sampler) = t.midi.as_ref().and_then(|m| m.sampler()) {
        for n in track_notes(p, t.midi.as_ref().expect("a sampler is a MIDI track's")) {
            let Some(entry) = sampler.entry(n.pitch) else { continue };
            let Some((name, pad)) = sampler.pads.get_key_value(&entry.pad) else { continue };
            hits.push((name.as_str(), n.at.clone(), shift(p, pad, entry.pitched.then_some(n.pitch), 0.0)));
        }
    }
    let mut far: BTreeMap<&str, (usize, BigRational, f64)> = BTreeMap::new();
    for (name, at, semitones) in hits {
        if semitones.abs() <= FAR_SEMITONES {
            continue;
        }
        let e = far.entry(name).or_insert((0, at.clone(), semitones));
        e.0 += 1;
        e.1 = e.1.clone().min(at);
        if semitones.abs() > e.2.abs() {
            e.2 = semitones;
        }
    }
    for (name, (count, at, semitones)) in far {
        let way = if semitones > 0.0 { "above" } else { "below" };
        out.push(
            Warning::new(
                "pad-far-from-root",
                format!(
                    "{}.{name}: {} the sample as far as {} semitones {way} its root, the first at beat {}; past two octaves a repitched sample sounds artificial",
                    t.id,
                    plural(count, "hit plays", "hits play"),
                    number(semitones.abs()),
                    beat_text(&at)
                ),
                vec![pad_path(t, name)],
            )
            .at(at),
        );
    }
}

/// `note-below-hearing`: Synth notes whose lowest oscillator is under 20 Hz.
fn below_hearing(p: &Project, t: &Track, out: &mut Vec<Warning>) {
    let Some(midi) = &t.midi else { return };
    let Some(synth) = midi.synth() else { return };
    // The lowest pitched oscillator's offset from the note, in semitones.
    let Some(offset) = synth
        .oscillators
        .values()
        .filter(|o| o.wave != Wave::Noise)
        .map(|o| 12.0 * o.octave as f64 + o.semitones + o.detune_cents / 100.0)
        .min_by(f64::total_cmp)
    else {
        return;
    };
    let hz = |pitch: i64| 440.0 * 2f64.powf((pitch as f64 + offset - 69.0) / 12.0);
    let low: Vec<_> = track_notes(p, midi).into_iter().filter(|n| hz(n.pitch) < HEARING_HZ).collect();
    let Some(first) = low.iter().map(|n| n.at.clone()).min() else { return };
    let lowest = low.iter().map(|n| hz(n.pitch)).fold(f64::INFINITY, f64::min);
    out.push(
        Warning::new(
            "note-below-hearing",
            format!(
                "{}: {} a fundamental under 20 Hz, the lowest {} Hz, the first at beat {}; it is felt rather than heard",
                t.id,
                plural(low.len(), "note has", "notes have"),
                number(lowest),
                beat_text(&first)
            ),
            vec![format!("tracks.{}.instrument.synth", t.id)],
        )
        .at(first),
    );
}

/// A track's pitched notes as they sound, (start, end, pitch) in beats: a
/// Synth's notes, a sampler's notes that a pitched entry plays, and pattern
/// events with a note. Drum maps and hits without a note have no register.
fn pitched(p: &Project, t: &Track) -> Vec<(f64, f64, i64)> {
    let mut out = Vec::new();
    if let Some(midi) = &t.midi {
        let keep = |pitch: i64| match &midi.instrument {
            Some(Instrument::Synth(_)) => true,
            Some(Instrument::Sampler(s)) => s.entry(pitch).is_some_and(|e| e.pitched),
            None => false,
        };
        for n in track_notes(p, midi) {
            if keep(n.pitch) {
                out.push((f(&n.at), f(&(&n.at + &n.beats)), n.pitch));
            }
        }
    }
    for c in &t.clips {
        let Some(pattern) = p.patterns.get(&c.pattern) else { continue };
        let (length, events) = (pattern.length_exact(), pattern.expanded());
        for r in 0..c.repeats {
            let base = c.at_exact() + BigRational::from_integer(r.into()) * &length;
            for e in &events {
                let Some(pitch) = e.note.as_deref().filter(|n| !n.is_empty()).and_then(|n| crate::rules::midi(n).ok()) else {
                    continue;
                };
                let at = &base + e.at_exact();
                // A hit without a duration rings for its sample; a beat stands in.
                let until = &at + e.duration_exact().unwrap_or_else(|| BigRational::from_integer(1.into()));
                out.push((f(&at), f(&until), pitch));
            }
        }
    }
    out
}

/// `register-crowded`: two tracks in one octave below C3 in the same bars,
/// a warning a pair of tracks and an octave. Muted tracks are left out.
fn crowded(p: &Project, out: &mut Vec<Warning>) {
    // Each track's low octaves, and the bars each sounds in.
    let low: Vec<(&Track, BTreeMap<i64, BTreeSet<i64>>)> = p
        .tracks
        .iter()
        .filter(|t| !t.mute)
        .map(|t| {
            let mut octaves: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
            for (start, end, pitch) in pitched(p, t) {
                if pitch >= LOW || end <= start {
                    continue;
                }
                let bars = octaves.entry(pitch.div_euclid(12)).or_default();
                let (first, last) = ((start / BAR + EPS).floor() as i64, ((end / BAR) - EPS).ceil() as i64 - 1);
                bars.extend(first..=last.max(first));
            }
            (t, octaves)
        })
        .collect();
    for (i, (a, oa)) in low.iter().enumerate() {
        for (b, ob) in &low[i + 1..] {
            for (octave, bars_a) in oa {
                let Some(bars_b) = ob.get(octave) else { continue };
                let both: BTreeSet<i64> = bars_a.intersection(bars_b).copied().collect();
                let Some(&first) = both.first() else { continue };
                let name = |n: i64| note_name(n).unwrap_or_default();
                out.push(
                    Warning::new(
                        "register-crowded",
                        format!(
                            "{} and {} both play notes from {} to {} in {} {}; two low parts in one octave muddy the mix",
                            a.id,
                            b.id,
                            name(octave * 12),
                            name(octave * 12 + 11),
                            if both.len() == 1 { "bar" } else { "bars" },
                            bar_runs(&both)
                        ),
                        vec![format!("tracks.{}", a.id), format!("tracks.{}", b.id)],
                    )
                    .at(BigRational::from_integer((first * 4).into())),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact(x: f64) -> BigRational {
        BigRational::from_float(x).unwrap_or_default()
    }

    #[test]
    fn bars_run_together() {
        let bars: BTreeSet<i64> = [0, 1, 2, 3, 7, 16, 17].into_iter().collect();
        assert_eq!(bar_runs(&bars), "1–4, 8, 17–18");
    }

    #[test]
    fn beats_are_written_as_commands_take_them() {
        let third = BigRational::new(13.into(), 3.into());
        assert_eq!((beat_json(&third), beat_text(&third)), (json!("13/3"), "13/3".into()));
        let half = exact(4.5);
        assert_eq!((beat_json(&half), beat_text(&half)), (json!(4.5), "4.5".into()));
        assert_eq!(beat_text(&exact(8.0)), "8");
    }

    #[test]
    fn every_code_is_described_once() {
        let codes: HashSet<&str> = CODES.iter().map(|(c, _)| *c).collect();
        assert_eq!(codes.len(), CODES.len());
    }
}
