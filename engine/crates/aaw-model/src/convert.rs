//! Patterns converted: each track of pads and pattern clips becomes a MIDI
//! track whose Drum Rack holds its pads, and each pattern clip a note clip
//! that plays the same hits, with the sound unchanged (D190).
//!
//! A pad keeps its name and lands on a key: the General MIDI drum map's for
//! its name when that key is free (a kick on C2, a snare on D2, a closed hat
//! on F♯2, an open hat on A♯2), else the lowest free key from C2 up. A pad
//! played with notes keeps a pitched range of those notes. A hit transposed
//! by its event plays a copy of its pad, transposed by as much, on a key of
//! its own, so every hit sounds as it did.

use crate::beat::Beat;
use crate::legacy::{Clip, Drums, Event, Legacy, Pattern};
use crate::rules::midi;
use crate::schema::*;
use crate::value::Value;
use crate::ModelError;
use indexmap::IndexMap;
use num_rational::BigRational;
use num_traits::{One, Zero};
use std::collections::{HashMap, HashSet};

/// Whether a song, as read from its text, holds anything conversion
/// changes: a pattern, or a track that is not a MIDI track with pads or
/// pattern clips.
pub fn needed(song: &Value) -> bool {
    Legacy::found(song)
}

/// A song's text read as the model reads it: converted in memory when it
/// holds patterns or a track of pads. The flag says it was converted.
pub fn read(text: &str) -> Result<(Project, bool), ModelError> {
    let value = crate::yaml_load::load(text).map_err(ModelError::Yaml)?;
    if !needed(&value) {
        return Ok((Project::validate(&value)?, false));
    }
    let (rest, legacy) = Legacy::take(&value)?;
    let project = Project::validate(&rest)?;
    let converted = converted(&project, &legacy).map_err(ModelError::Convert)?;
    Ok((converted, true))
}

/// The song with its tracks of pads converted, given what it held besides
/// what the model reads.
fn converted(p: &Project, legacy: &Legacy) -> Result<Project, String> {
    let mut clip_ids: HashSet<String> =
        p.tracks.iter().filter_map(|t| t.midi.as_ref()).flat_map(|m| m.clips.iter().map(|c| c.id.clone())).collect();
    let mut track_ids: HashSet<String> =
        p.tracks.iter().map(|t| t.id.clone()).chain(p.groups.iter().map(|g| g.id.clone())).chain(p.returns.iter().map(|r| r.id.clone())).collect();
    let mut out = p.clone();
    out.tracks = Vec::with_capacity(p.tracks.len());
    for (i, t) in p.tracks.iter().enumerate() {
        let Some(d) = legacy.tracks.get(&i) else {
            out.tracks.push(t.clone());
            continue;
        };
        let (drums, audio) = track(p, &legacy.patterns, t, d, &mut clip_ids, &mut track_ids).map_err(|e| format!("{}: {e}", t.id))?;
        out.tracks.push(drums);
        out.tracks.extend(audio);
    }
    // Validated again, so the notes get their IDs and every rule is checked
    // on what was made.
    Project::validate(&out.dump(false)).map_err(|e| format!("The converted song does not validate: {e}"))
}

/// What a hit plays: a pad as it is, moved by its event's transpose, or a
/// pad played at a pitch.
#[derive(Clone, Copy, PartialEq)]
enum Slot {
    /// The event's transpose, in semitones.
    Hit(f64),
    /// The pitch rounded to a key, and what is left over in semitones.
    Pitched(i64, f64),
}

fn slot(e: &Event) -> Result<Slot, String> {
    match e.note.as_deref().filter(|n| !n.is_empty()) {
        None => Ok(Slot::Hit(e.transpose)),
        Some(note) => {
            let moved = midi(note)? as f64 + e.transpose;
            let pitch = moved.round().clamp(0.0, 127.0);
            Ok(Slot::Pitched(pitch as i64, moved - pitch))
        }
    }
}

/// A float as a map key, with -0 the same as 0.
fn bits(x: f64) -> u64 {
    if x == 0.0 { 0.0f64.to_bits() } else { x.to_bits() }
}

/// The keys of the General MIDI drum map that a pad's name says it is.
fn general_midi(name: &str) -> Option<i64> {
    let n: String = name.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let has = |w: &str| n.contains(w);
    let hat = has("hat") || n == "hh" || n == "chh" || n == "ch";
    Some(if (hat && (has("open") || n.starts_with('o'))) || n == "oh" || n == "ohh" {
        46
    } else if hat && has("pedal") {
        44
    } else if hat {
        42
    } else if has("kick") || n == "bd" || has("bassdrum") {
        36
    } else if has("snare") || n == "sd" {
        38
    } else if has("clap") {
        39
    } else if has("rim") || has("sidestick") {
        37
    } else if has("crash") {
        49
    } else if has("ride") {
        51
    } else if has("tom") {
        if has("floor") {
            41
        } else if has("high") || has("hi") {
            50
        } else if has("mid") {
            47
        } else {
            45
        }
    } else if has("cowbell") {
        56
    } else if has("tamb") {
        54
    } else if has("shaker") || has("maraca") {
        70
    } else {
        return None;
    })
}

/// A name for a copy of a pad moved by `by` semitones: `kick-2`, `hat+0.5`.
fn moved_name(pad: &str, by: f64, taken: &IndexMap<String, Pad>, names: &IndexMap<String, Pad>) -> String {
    let base = format!("{pad}{by:+}");
    let mut name = base.clone();
    let mut n = 2;
    while taken.contains_key(&name) || names.contains_key(&name) {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

/// What a track's Drum Rack is made of: its pads, its map, and the key or
/// the shift each hit plays.
struct Rack {
    pads: IndexMap<String, Pad>,
    map: Vec<NoteMap>,
    /// For a hit, its key by (pad, transpose).
    keys: HashMap<(String, u64), i64>,
    /// For a pitched hit, how far its keys were moved, by (pad, rest, key).
    shifts: HashMap<(String, u64, i64), i64>,
    /// The copies made of pads, by (pad, how far moved).
    copies: HashMap<(String, u64), String>,
}

impl Rack {
    /// The pad named `pad` moved by `by` semitones, copied under a new name
    /// unless `by` is 0.
    fn pad(&mut self, d: &Drums, pad: &str, by: f64) -> Result<String, String> {
        let base = d.pads.get(pad).ok_or_else(|| format!("unknown pad {pad}"))?;
        if by == 0.0 {
            if !self.pads.contains_key(pad) {
                self.pads.insert(pad.to_string(), Pad { mute: false, solo: false, ..base.clone() });
            }
            return Ok(pad.to_string());
        }
        let transpose = base.transpose + by;
        if !(-36.0..=36.0).contains(&transpose) {
            return Err(format!("pad {pad} is moved {transpose:+} semitones in all, past the Sampler's 36"));
        }
        if let Some(name) = self.copies.get(&(pad.to_string(), bits(by))) {
            return Ok(name.clone());
        }
        let name = moved_name(pad, by, &self.pads, &d.pads);
        self.pads.insert(name.clone(), Pad { transpose, mute: false, solo: false, ..base.clone() });
        self.copies.insert((pad.to_string(), bits(by)), name.clone());
        Ok(name)
    }
}

/// Puts each pitched pad's notes on keys: the whole span of its notes, or
/// with `runs` each run of notes next to one another it plays.
fn pitch_ranges(
    d: &Drums,
    pitched: &IndexMap<(String, u64), (f64, std::collections::BTreeSet<i64>)>,
    runs: bool,
    r: &mut Rack,
    free: &mut [bool; 128],
) -> Result<(), String> {
    for ((pad, rest_bits), (rest, notes)) in pitched {
        let (Some(&first), Some(&last)) = (notes.first(), notes.last()) else { continue };
        let mut spans = vec![(first, last)];
        if runs {
            spans.clear();
            for &n in notes {
                match spans.last_mut() {
                    Some((_, high)) if *high + 1 == n => *high = n,
                    _ => spans.push((n, n)),
                }
            }
        }
        let base = d.pads.get(pad).ok_or_else(|| format!("unknown pad {pad}"))?.transpose;
        for (low, high) in spans {
            let shift = (0..128i64)
                .flat_map(|d| [d, -d])
                .find(|k| {
                    let (lo, hi) = (low + k, high + k);
                    lo >= 0 && hi <= 127 && (lo..=hi).all(|n| free[n as usize]) && (-36.0..=36.0).contains(&(base + rest - *k as f64))
                })
                .ok_or_else(|| format!("no free keys for pad {pad}'s notes"))?;
            let name = r.pad(d, pad, rest - shift as f64)?;
            for n in low + shift..=high + shift {
                free[n as usize] = false;
            }
            r.map.push(NoteMap { low: low + shift, high: high + shift, range: high > low, pad: name, pitched: true });
            for n in low..=high {
                r.shifts.insert((pad.clone(), *rest_bits, n), shift);
            }
        }
    }
    Ok(())
}

/// The pattern a clip plays.
fn pattern<'a>(patterns: &'a IndexMap<String, Pattern>, c: &Clip) -> Result<&'a Pattern, String> {
    patterns.get(&c.pattern).ok_or_else(|| format!("unknown pattern {}", c.pattern))
}

fn rack(patterns: &IndexMap<String, Pattern>, d: &Drums) -> Result<Rack, String> {
    // What each pad plays, in the order first played.
    let mut hits: IndexMap<String, Vec<f64>> = d.pads.keys().map(|k| (k.clone(), Vec::new())).collect();
    let mut pitched: IndexMap<(String, u64), (f64, std::collections::BTreeSet<i64>)> = IndexMap::new();
    for c in &d.clips {
        for e in pattern(patterns, c)?.expanded() {
            if !d.pads.contains_key(&e.pad) {
                return Err(format!("pattern {} plays unknown pad {}", c.pattern, e.pad));
            }
            match slot(&e)? {
                Slot::Hit(by) => {
                    let list = hits.get_mut(&e.pad).ok_or_else(|| format!("unknown pad {}", e.pad))?;
                    if !list.iter().any(|b| bits(*b) == bits(by)) {
                        list.push(by);
                    }
                }
                Slot::Pitched(key, rest) => {
                    pitched.entry((e.pad.clone(), bits(rest))).or_insert((rest, Default::default())).1.insert(key);
                }
            }
        }
    }
    // A pad played by nothing still gets a key, so it shows in the rack.
    for (pad, list) in hits.iter_mut() {
        if list.is_empty() && !pitched.keys().any(|(p, _)| p == pad) {
            list.push(0.0);
        }
    }
    // Pitched ranges first: each pad's from its lowest note to its highest,
    // or, when those leave too few keys or collide, each run of the notes it
    // plays; at their own notes when free, else moved to the nearest free
    // keys with the pad moved back to match.
    let hit_keys: usize = hits.values().map(Vec::len).sum();
    let mut placed = None;
    for runs in [false, true] {
        let mut r = Rack { pads: IndexMap::new(), map: Vec::new(), keys: HashMap::new(), shifts: HashMap::new(), copies: HashMap::new() };
        let mut free = [true; 128];
        match pitch_ranges(d, &pitched, runs, &mut r, &mut free) {
            Ok(()) if free.iter().filter(|f| **f).count() >= hit_keys => {
                placed = Some((r, free));
                break;
            }
            Ok(()) if runs => return Err("more pads and notes than keys".into()),
            Err(e) if runs => return Err(e),
            _ => {}
        }
    }
    let (mut r, mut free) = placed.expect("placed or refused");
    // Then each hit: a pad as it is on its General MIDI key when that is
    // free, the rest on the lowest free key from C2 up, then below it.
    let place = |r: &mut Rack, free: &mut [bool; 128], pad: &str, by: f64, key: i64| -> Result<(), String> {
        let name = r.pad(d, pad, by)?;
        free[key as usize] = false;
        r.map.push(NoteMap { low: key, high: key, range: false, pad: name, pitched: false });
        r.keys.insert((pad.to_string(), bits(by)), key);
        Ok(())
    };
    let mut rest = Vec::new();
    for (pad, list) in &hits {
        for &by in list {
            match general_midi(pad).filter(|k| by == 0.0 && free[*k as usize]) {
                Some(key) => place(&mut r, &mut free, pad, by, key)?,
                None => rest.push((pad.clone(), by)),
            }
        }
    }
    // Pads as they are before the copies moved from them.
    rest.sort_by_key(|(_, by)| *by != 0.0);
    for (pad, by) in rest {
        let key = (36..128).chain((0..36).rev()).find(|k| free[*k as usize]).ok_or("more pads than keys")?;
        place(&mut r, &mut free, &pad, by, key)?;
    }
    r.map.sort_by_key(|m| m.low);
    Ok(r)
}

/// An ID for a note clip from a pattern's name, unique among the song's.
fn clip_id(pattern: &str, taken: &mut HashSet<String>) -> String {
    let mut base: String = pattern.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' }).collect();
    base = base.trim_start_matches(|c: char| !c.is_ascii_alphabetic()).trim_end_matches('-').to_string();
    if base.is_empty() {
        base = "clip".into();
    }
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    taken.insert(id.clone());
    id
}

fn exact(b: &Beat) -> BigRational {
    b.exact().expect("validated beat")
}

/// The fraction with the smallest denominator in [lo, hi], 0 <= lo <= hi.
fn simplest(lo: &BigRational, hi: &BigRational) -> BigRational {
    let n = lo.floor();
    if &n == lo {
        return n;
    }
    if n < hi.floor() {
        return n + BigRational::one();
    }
    let inner = simplest(&(hi - &n).recip(), &(lo - &n).recip());
    n + inner.recip()
}

/// A hit's start as a note's: as it was written, or, for a time a pattern
/// worked out, such as a swung step's, the plainest fraction within a
/// billionth of a beat that every repetition starts on the same frame, so
/// a step swung by 0.6 starts on 13/10 rather than on the float's
/// 129999999999999999/100000000000000000.
fn start(p: &Project, written: &Beat, at: &BigRational, bases: &[BigRational]) -> Beat {
    if !matches!(written, Beat::Str(_)) {
        return written.clone();
    }
    let near = BigRational::new(1.into(), 1_000_000_000.into());
    let plain = simplest(&(at - &near).max(BigRational::zero()), &(at + &near));
    let (tempo, rate) = (p.session.tempo, p.session.sample_rate);
    let same = bases.iter().all(|b| crate::frame(&(b + at), tempo, rate) == crate::frame(&(b + &plain), tempo, rate));
    Beat::from_exact(if same { &plain } else { at })
}

/// A note clip that plays a pattern clip's hits: looped over the pattern's
/// length when it repeats, or written out when a gated hit lasts past the
/// pattern's end, which a loop would cut.
fn clip(p: &Project, patterns: &IndexMap<String, Pattern>, d: &Drums, c: &Clip, r: &Rack, id: String) -> Result<NoteClip, String> {
    let pattern = pattern(patterns, c)?;
    let length = pattern.length_exact();
    let grid = pattern.grid_exact();
    let repeats = BigRational::from_integer(c.repeats.into());
    let bases: Vec<BigRational> = (0..c.repeats).map(|k| c.at_exact() + BigRational::from_integer(k.into()) * &length).collect();
    let events = pattern.expanded();
    let mut one = Vec::new();
    for e in &events {
        let gated = d.pads[&e.pad].mode == PadMode::Gate;
        // A gated hit's end is heard, so its start stays as it is.
        let written = if gated { e.at.clone() } else { start(p, &e.at, &e.at_exact(), &bases) };
        let at = exact(&written);
        let pitch = match slot(&e)? {
            Slot::Hit(by) => r.keys[&(e.pad.clone(), bits(by))],
            Slot::Pitched(key, rest) => key + r.shifts[&(e.pad.clone(), bits(rest), key)],
        };
        // A hit with no length is as long as a step, within the pattern;
        // only a gated pad's hits have lengths that are heard.
        let duration = match e.duration_exact().filter(|d| d > &BigRational::zero()) {
            Some(_) => e.duration.clone().expect("a duration"),
            None => Beat::from_exact(&grid.clone().min(&length - &at)),
        };
        let velocity = if c.velocity_scale == 1.0 {
            e.velocity
        } else {
            ((e.velocity as f64 * c.velocity_scale).round() as i64).clamp(1, 127)
        };
        one.push((at, written, pitch, duration, velocity, gated));
    }
    // Hits of one pad at once in a choke group: the one written last chokes
    // the rest, and notes at once are played in order of their lengths, so
    // the rest are made shorter than it. A one-shot pad's length is not heard.
    for i in 0..one.len() {
        let (at, _, pitch, _, _, gated) = &one[i];
        let choked = d.pads[&events[i].pad].choke_group.as_deref().is_some_and(|g| !g.is_empty());
        let Some(last) = (i + 1..one.len()).rev().find(|j| &one[*j].0 == at && one[*j].2 == *pitch) else { continue };
        if !choked || *gated {
            continue;
        }
        let heard = exact(&one[last].3).min(&length - at);
        one[i].3 = Beat::from_exact(&(exact(&one[i].3).min(&heard / BigRational::from_integer(2.into()))));
    }
    let past = |(at, _, _, d, _, gated): &(BigRational, Beat, i64, Beat, i64, bool)| *gated && at + exact(d) > length;
    let note = |at: Beat, pitch: i64, duration: Beat, velocity: i64| Note { id: String::new(), pitch, at, duration, velocity };
    let mut notes = Vec::new();
    let (length_beats, loop_beats) = if !one.iter().any(past) {
        for (_, at, pitch, d, v, _) in &one {
            notes.push(note(at.clone(), *pitch, d.clone(), *v));
        }
        let whole = if c.repeats == 1 { pattern.length_beats.clone() } else { Beat::from_exact(&(&repeats * &length)) };
        (whole, (c.repeats > 1).then(|| pattern.length_beats.clone()))
    } else {
        let mut end = &repeats * &length;
        for k in 0..c.repeats {
            let from = BigRational::from_integer(k.into()) * &length;
            for (at, written, pitch, d, v, gated) in &one {
                let start = if k == 0 { written.clone() } else { Beat::from_exact(&(&from + at)) };
                if *gated {
                    end = end.max(&from + at + exact(d));
                }
                notes.push(note(start, *pitch, d.clone(), *v));
            }
        }
        (Beat::from_exact(&end), None)
    };
    notes.sort_by(|a, b| exact(&a.at).cmp(&exact(&b.at)).then(a.pitch.cmp(&b.pitch)));
    for (i, n) in notes.iter_mut().enumerate() {
        n.id = format!("n{}", i + 1);
    }
    Ok(NoteClip { id, at: c.at.clone(), length_beats, loop_beats, loop_offset_beats: None, notes, active: c.active, color: None })
}

/// A track of pads as a MIDI track of the same name, and the audio track
/// that keeps its audio clips and input when it has any.
fn track(
    p: &Project,
    patterns: &IndexMap<String, Pattern>,
    t: &Track,
    d: &Drums,
    clip_ids: &mut HashSet<String>,
    track_ids: &mut HashSet<String>,
) -> Result<(Track, Option<Track>), String> {
    let r = rack(patterns, d)?;
    let mut clips = Vec::new();
    for c in &d.clips {
        let id = clip_id(&c.pattern, clip_ids);
        clips.push(clip(p, patterns, d, c, &r, id)?);
    }
    let sampler = Sampler { pads: r.pads, map: r.map, drum_rack: true };
    let drums = Track {
        audio: Vec::new(),
        input: None,
        midi: Some(Midi {
            instrument: Some(Instrument::Sampler(sampler)),
            clips,
            midi_effects: Vec::new(),
            made: Default::default(),
        }),
        ..t.clone()
    };
    if t.audio.is_empty() && t.input.is_none() {
        return Ok((drums, None));
    }
    // A MIDI track holds no audio clips: they go to a track of their own
    // beside it, with the same mixer and effects, so it sounds as it did
    // through a chain that treats the sum as each part.
    let mut id = format!("{}-audio", t.id);
    let mut n = 2;
    while track_ids.contains(&id) {
        id = format!("{}-audio-{n}", t.id);
        n += 1;
    }
    track_ids.insert(id.clone());
    // A frozen track's file holds both, so the audio's own track is quiet
    // until the drums are unfrozen.
    let mute = t.mute || t.frozen.is_some();
    let audio = Track { id, frozen: None, mute, ..t.clone() };
    Ok((drums, Some(audio)))
}

/// Where the song as it was before conversion is kept: beside it, as
/// `song.before-drum-rack.yaml`, numbered when that is taken.
pub fn kept_path(song: &std::path::Path) -> std::path::PathBuf {
    let dir = song.parent().unwrap_or(std::path::Path::new(""));
    let stem = song.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "song".into());
    let mut path = dir.join(format!("{stem}.before-drum-rack.yaml"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem}.before-drum-rack-{n}.yaml"));
        n += 1;
    }
    path
}

/// Writes a converted song over `path`, with `text`, the song as it was,
/// kept beside it. Returns the text now in the file.
pub fn write_back(path: &std::path::Path, text: &str, converted: &Project) -> Result<String, String> {
    let kept = kept_path(path);
    crate::atomic_write(&kept, text).map_err(|e| format!("{}: {e}", kept.display()))?;
    let yaml = crate::to_yaml(converted);
    crate::atomic_write(path, &yaml).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(yaml)
}

/// A song read from `path` as `text`, converted and written back when it
/// holds patterns or tracks of pads, with the text as it was kept beside
/// it. Returns the song to use and, when it was converted, the text now in
/// the file.
pub fn upgrade(path: &std::path::Path, text: &str) -> Result<(Project, Option<String>), ModelError> {
    match read(text)? {
        (project, false) => Ok((project, None)),
        (project, true) => {
            let yaml = write_back(path, text, &project).map_err(ModelError::Convert)?;
            Ok((project, Some(yaml)))
        }
    }
}
