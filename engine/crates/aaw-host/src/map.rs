//! `daw map`: the song as a grid of tracks by bars, for an agent that cannot
//! see the window. A cell is a bar unless asked otherwise. A letter is a clip,
//! and the same letter is the same music wherever it is; `#` is two clips of a
//! track sounding at once, `:` a clip with nothing starting in the cell, and
//! `.` nothing.

use crate::tree::Step;
use aaw_model::{Curve, Lane, Project, Stretch};
use num_traits::ToPrimitive;
use serde_json::{json, Value as Json};
use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

type Result<T> = std::result::Result<T, String>;

/// How far apart two beats may be and still be the same beat: an audio clip's
/// end comes back from seconds.
const EPS: f64 = 1e-6;
/// 4/4 is the only time signature.
const BAR: f64 = 4.0;
/// The letters given to music in the order it first appears; what is left
/// over is `*`.
const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
/// Bars listed for a letter in the text before the rest are counted.
const LISTED: usize = 6;

/// What `daw map` was asked for.
pub struct Options {
    /// Beats a cell.
    pub per: f64,
    pub from: f64,
    /// The song's end, or the last clip's when that is later, unless given.
    pub to: Option<f64>,
    /// Track IDs to keep; none keeps every track.
    pub tracks: Vec<String>,
    pub lanes: bool,
}

/// A clip as the map sees it.
struct Placed {
    /// Its content: the same key is the same music.
    key: String,
    loc: Vec<Step>,
    start: f64,
    end: f64,
    /// Where something starts in it: a note, a hit.
    onsets: Vec<f64>,
    /// The pitched ones, for the track's range.
    pitches: Vec<(f64, i64)>,
    /// Audio sounds throughout, so every cell it covers takes its letter.
    continuous: bool,
    repeats: i64,
    /// What the legend says of the music.
    what: String,
}

/// The cells of `from..to`, each `per` beats, the last one perhaps shorter.
struct Grid {
    from: f64,
    to: f64,
    per: f64,
}

impl Grid {
    fn len(&self) -> usize {
        (((self.to - self.from) / self.per) - EPS).ceil().max(1.0) as usize
    }

    fn cell(&self, i: usize) -> (f64, f64) {
        let start = self.from + i as f64 * self.per;
        (start, (start + self.per).min(self.to))
    }
}

fn number(x: f64) -> String {
    let s = format!("{x:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// A song beat as a DAW counts it: bar, then the beat of the bar and the
/// sixteenth of the beat when it is not on the bar, from 1; a beat off the
/// sixteenths is written as the beat commands take.
pub fn place(beat: f64) -> String {
    let bar = (beat / BAR + EPS).floor();
    let rest = beat - bar * BAR;
    let sixteenths = rest * 4.0;
    if rest.abs() < EPS {
        format!("{}", bar as i64 + 1)
    } else if (rest - rest.round()).abs() < EPS {
        format!("{}.{}", bar as i64 + 1, rest.round() as i64 + 1)
    } else if (sixteenths - sixteenths.round()).abs() < EPS {
        let s = sixteenths.round() as i64;
        format!("{}.{}.{}", bar as i64 + 1, s / 4 + 1, s % 4 + 1)
    } else {
        format!("beat {}", number(beat))
    }
}

fn f(x: &num_rational::BigRational) -> f64 {
    x.to_f64().unwrap_or(f64::NAN)
}

fn beats_text(beats: f64) -> String {
    let n = number(beats);
    if n == "1" {
        "1 beat".into()
    } else {
        format!("{n} beats")
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn span(pitches: &[i64]) -> Option<String> {
    let low = *pitches.iter().min()?;
    let high = *pitches.iter().max()?;
    let name = |p: i64| aaw_model::rules::note_name(p).unwrap_or_default();
    Some(if low == high { name(low) } else { format!("{}–{}", name(low), name(high)) })
}

/// Every clip of track `ti`, by where it starts.
fn placed(p: &Project, dir: &Path, ti: usize) -> Vec<Placed> {
    let t = &p.tracks[ti];
    let at = |kind: &str, i: usize| {
        vec![Step::Key("tracks".into()), Step::Index(ti), Step::Key(kind.into()), Step::Index(i)]
    };
    let mut out = Vec::new();
    for (i, c) in t.clips.iter().enumerate() {
        let Some(pattern) = p.patterns.get(&c.pattern) else { continue };
        let length = f(&pattern.length_exact());
        let start = f(&c.at_exact());
        let mut events: Vec<String> = pattern
            .expanded()
            .iter()
            .map(|e| {
                format!(
                    "{}:{}:{}:{}:{}:{}",
                    aaw_model::fraction_str(&e.at_exact()),
                    e.pad,
                    e.velocity,
                    e.note.as_deref().unwrap_or(""),
                    e.duration_exact().map(|d| aaw_model::fraction_str(&d)).unwrap_or_default(),
                    e.transpose
                )
            })
            .collect();
        events.sort();
        let hits = pattern.expanded();
        let mut onsets = Vec::new();
        let mut pitches = Vec::new();
        for r in 0..c.repeats {
            let base = start + r as f64 * length;
            for e in &hits {
                let at = base + f(&e.at_exact());
                onsets.push(at);
                if let Some(n) = e.note.as_deref().and_then(|n| aaw_model::rules::midi(n).ok()) {
                    pitches.push((at, n));
                }
            }
        }
        out.push(Placed {
            key: format!("pattern|{}|{}|{}", aaw_model::fraction_str(&pattern.length_exact()), c.velocity_scale, events.join(",")),
            loc: at("clips", i),
            start,
            end: start + length * c.repeats as f64,
            onsets,
            pitches,
            continuous: false,
            repeats: c.repeats,
            what: format!(
                "pattern {}, {}, {}",
                c.pattern,
                beats_text(length),
                plural(hits.len(), "hit", "hits")
            ),
        });
    }
    for (i, c) in t.audio.iter().enumerate() {
        let start = f(&c.at_exact());
        let file = p
            .samples
            .get(&c.sample)
            .and_then(|s| aaw_engine::sndfile::info(&dir.join(&s.path)).ok())
            .map(|info| info.seconds());
        // A file that cannot be read is given a beat, so it is still seen.
        let per_beat = 60.0 / c.source_bpm.unwrap_or(p.session.tempo);
        let mut until = match (c.source_end_seconds, file) {
            (Some(end), Some(file)) => end.min(file),
            (Some(end), None) => end,
            (None, Some(file)) => file,
            (None, None) => c.source_start_seconds + per_beat,
        };
        let (mut beats, mut onsets, mut looped) = (((until - c.source_start_seconds) / per_beat).max(0.0), vec![start], String::new());
        if let (Some(every), Some(length)) = (c.loop_exact(), c.length_exact()) {
            // The loop: so many beats of the file, played again at each wrap.
            let (every, length) = (f(&every), f(&length));
            until = until.min(c.source_start_seconds + every * per_beat);
            beats = length;
            onsets = c.repetitions(p.session.tempo).iter().map(|r| f(&r.at_exact())).collect();
            looped = format!(", loops every {}", beats_text(every));
        }
        let repeats = onsets.len() as i64;
        out.push(Placed {
            key: format!(
                "audio|{}|{:.3}|{:.3}|{:?}|{}|{:?}",
                c.sample,
                c.source_start_seconds,
                until,
                c.source_bpm,
                matches!(c.stretch, Stretch::PreservePitch),
                c.loop_exact().map(|l| aaw_model::fraction_str(&l))
            ),
            loc: at("audio", i),
            start,
            end: start + beats,
            onsets,
            pitches: Vec::new(),
            continuous: true,
            repeats,
            what: format!(
                "audio {} {}–{} s, {}{looped}{}",
                c.sample,
                number(c.source_start_seconds),
                number(until),
                beats_text(beats),
                if file.is_none() { ", its file unread" } else { "" }
            ),
        });
    }
    if let Some(midi) = &t.midi {
        for (i, c) in midi.clips.iter().enumerate() {
            let start = f(&c.at_exact());
            let length = c.length_exact();
            let playing: Vec<&aaw_model::Note> = c.notes.iter().filter(|n| c.plays(n)).collect();
            let mut notes: Vec<(num_rational::BigRational, i64, num_rational::BigRational, i64)> = playing
                .iter()
                .map(|n| (n.at_exact(), n.pitch, n.duration_exact(), n.velocity))
                .collect();
            notes.sort();
            let key = notes
                .iter()
                .map(|(at, pitch, d, v)| format!("{}:{pitch}:{}:{v}", aaw_model::fraction_str(at), aaw_model::fraction_str(d)))
                .collect::<Vec<_>>()
                .join(",");
            // Each note once a repetition, where it starts inside what plays.
            let repetitions = c.repetitions();
            let placed: Vec<(f64, i64)> = repetitions
                .iter()
                .flat_map(|(from, plays)| {
                    playing.iter().filter(move |n| n.at_exact() < *plays).map(move |n| (start + f(from) + f(&n.at_exact()), n.pitch))
                })
                .collect();
            let looped = c.loop_exact().map(|l| format!(", loops every {}", beats_text(f(&l)))).unwrap_or_default();
            out.push(Placed {
                key: format!("notes|{}|{:?}|{key}", aaw_model::fraction_str(&length), c.loop_exact().map(|l| aaw_model::fraction_str(&l))),
                loc: at("clips", i),
                start,
                end: start + f(&length),
                onsets: placed.iter().map(|(at, _)| *at).collect(),
                what: format!(
                    "notes {}, {}{looped}, {}{}",
                    c.id,
                    beats_text(f(&length)),
                    plural(playing.len(), "note", "notes"),
                    span(&playing.iter().map(|n| n.pitch).collect::<Vec<_>>()).map(|s| format!(" {s}")).unwrap_or_default()
                ),
                pitches: placed,
                continuous: false,
                repeats: repetitions.len() as i64,
            });
        }
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

/// What a track's clips make of one cell.
fn cell(clips: &[&Placed], letter: &dyn Fn(&Placed) -> char, (cs, ce): (f64, f64)) -> char {
    let here: Vec<&Placed> = clips.iter().copied().filter(|c| c.start < ce - EPS && c.end > cs + EPS).collect();
    if here.is_empty() {
        return '.';
    }
    for (i, a) in here.iter().enumerate() {
        for b in &here[i + 1..] {
            let (lo, hi) = (a.start.max(b.start).max(cs), a.end.min(b.end).min(ce));
            if lo < hi - EPS {
                return '#';
            }
        }
    }
    let sounding = |c: &&&Placed| c.continuous || c.onsets.iter().any(|&o| o >= cs - EPS && o < ce - EPS);
    let cover = |c: &&&Placed| c.end.min(ce) - c.start.max(cs);
    here.iter()
        .filter(sounding)
        .max_by(|a, b| cover(a).total_cmp(&cover(b)).then(b.start.total_cmp(&a.start)))
        .map_or(':', |c| letter(c))
}

/// Whether a lane's value moves anywhere in `cs..ce`: a ramp between two
/// values crosses it, or a value jumps inside it.
fn moves(lane: &Lane, (cs, ce): (f64, f64)) -> bool {
    lane.points.windows(2).any(|w| {
        let (a, b) = (f(&w[0].at_exact()), f(&w[1].at_exact()));
        if (w[0].value - w[1].value).abs() < 1e-9 {
            return false;
        }
        if matches!(w[0].curve, Curve::Hold) || b - a < EPS {
            b >= cs - EPS && b < ce - EPS
        } else {
            a < ce - EPS && b > cs + EPS
        }
    })
}

fn lane_cells(lane: &Lane, grid: &Grid) -> String {
    (0..grid.len()).map(|i| if moves(lane, grid.cell(i)) { '~' } else { '-' }).collect()
}

/// The bar numbers over the grid, at every fourth cell where they fit.
fn ruler(grid: &Grid) -> String {
    let mut row: Vec<char> = vec![' '; grid.len()];
    let mut free = 0;
    for i in (0..grid.len()).step_by(4) {
        let label: Vec<char> = place(grid.cell(i).0).chars().collect();
        if i < free {
            continue;
        }
        for (k, ch) in label.iter().enumerate() {
            if i + k < row.len() {
                row[i + k] = *ch;
            } else {
                row.push(*ch);
            }
        }
        free = i + label.len() + 1;
    }
    row.into_iter().collect::<String>().trim_end().to_string()
}

/// The sections over the grid: each one's ID where it starts, cut to its
/// cells, and dashes to its end.
fn sections(p: &Project, grid: &Grid) -> Option<String> {
    let mut row = vec![' '; grid.len()];
    let mut any = false;
    let mut list: Vec<&aaw_model::Section> = p.sections.iter().collect();
    list.sort_by(|a, b| f(&a.at_exact()).total_cmp(&f(&b.at_exact())));
    for s in list {
        let (start, end) = (f(&s.at_exact()), f(&(s.at_exact() + s.length_exact())));
        let cells: Vec<usize> = (0..grid.len())
            .filter(|&i| {
                let (cs, ce) = grid.cell(i);
                cs < end - EPS && ce > start + EPS
            })
            .collect();
        // A dash after the name, where there is room, keeps it apart from
        // the next section's.
        let room = if cells.len() > 1 { cells.len() - 1 } else { 1 };
        let mut name = s.id.chars().take(room);
        for &i in &cells {
            row[i] = name.next().unwrap_or('-');
            any = true;
        }
    }
    any.then(|| row.into_iter().collect::<String>().trim_end().to_string())
}

/// The arrangement map of a song, `refer` naming a clip at a location as
/// commands do.
pub fn map(p: &Project, dir: &Path, options: &Options, refer: &dyn Fn(&[Step]) -> String) -> Result<Json> {
    if options.per <= 0.0 || !options.per.is_finite() {
        return Err("--per is a positive number of bars, bar or beat".into());
    }
    for id in &options.tracks {
        if p.track(id).is_none() {
            let ids: Vec<&str> = p.tracks.iter().map(|t| t.id.as_str()).collect();
            return Err(format!("No track {id}; the tracks are {}", ids.join(", ")));
        }
    }
    let shown: Vec<usize> = (0..p.tracks.len())
        .filter(|&i| options.tracks.is_empty() || options.tracks.contains(&p.tracks[i].id))
        .collect();
    let clips: Vec<Vec<Placed>> = shown.iter().map(|&ti| placed(p, dir, ti)).collect();
    let song_end = f(&p.session.length_exact());
    let last = clips.iter().flatten().map(|c| c.end).fold(song_end, f64::max);
    let to = options.to.unwrap_or(last);
    if to <= options.from + EPS {
        return Err(format!("--to {} is not after --from {}", number(to), number(options.from)));
    }
    let grid = Grid { from: options.from, to, per: options.per };

    // Letters in reading order: down the tracks, along each one.
    let mut letters: HashMap<&str, char> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for c in clips.iter().flatten() {
        if c.start < to - EPS && c.end > options.from + EPS && !letters.contains_key(c.key.as_str()) {
            let mark = LETTERS.get(order.len()).map_or('*', |&b| b as char);
            letters.insert(&c.key, mark);
            order.push(&c.key);
        }
    }
    let letter = |c: &Placed| letters.get(c.key.as_str()).copied().unwrap_or('*');

    let mut rows: Vec<(String, String)> = vec![("bar".into(), ruler(&grid))];
    if let Some(s) = sections(p, &grid) {
        rows.push(("sections".into(), s));
    }
    let mut details = Vec::new();
    for (k, &ti) in shown.iter().enumerate() {
        let t = &p.tracks[ti];
        let mine: Vec<&Placed> = clips[k].iter().collect();
        let cells: String = (0..grid.len()).map(|i| cell(&mine, &letter, grid.cell(i))).collect();
        let name = match (t.mute, t.solo) {
            (true, _) => format!("{} (muted)", t.id),
            (_, true) => format!("{} (solo)", t.id),
            _ => t.id.clone(),
        };
        rows.push((name, cells.clone()));
        if options.lanes {
            for lane in &t.automation {
                let cells = lane_cells(lane, &grid);
                rows.push((format!("  {}", lane.param), cells));
            }
        }

        // A line a track: what plays it, its level, its range and how busy it is.
        let in_range = |o: &f64| *o >= grid.from - EPS && *o < grid.to - EPS;
        let onsets = mine.iter().filter(|c| !c.continuous).flat_map(|c| c.onsets.iter()).filter(|o| in_range(o)).count();
        let pitches: Vec<i64> = mine
            .iter()
            .flat_map(|c| c.pitches.iter())
            .filter(|(o, _)| in_range(o))
            .map(|&(_, p)| p)
            .collect();
        let bars = cells
            .chars()
            .enumerate()
            .filter(|(_, ch)| *ch != '.')
            .map(|(i, _)| {
                let (cs, ce) = grid.cell(i);
                (ce - cs) / BAR
            })
            .sum::<f64>();
        let mut parts = Vec::new();
        let instrument = match &t.midi {
            Some(midi) => match &midi.instrument {
                Some(aaw_model::Instrument::Synth(s)) => match &s.patch {
                    Some(name) => format!("synth \"{name}\""),
                    None => "synth".into(),
                },
                Some(aaw_model::Instrument::Sampler(s)) => format!("sampler of {}", plural(s.pads.len(), "pad", "pads")),
                None => "no instrument".into(),
            },
            None if !t.pads.is_empty() => format!("pads {}", t.pads.keys().cloned().collect::<Vec<_>>().join(" ")),
            None => "audio".into(),
        };
        parts.push(instrument);
        parts.push(format!("{} dB", number(t.gain_db)));
        if let Some(s) = span(&pitches) {
            parts.push(s);
        }
        if mine.iter().any(|c| !c.continuous) {
            let word = if t.midi.is_some() { "notes" } else { "hits" };
            parts.push(if bars > 0.0 {
                format!("{} {word}/bar", number((onsets as f64 / bars * 10.0).round() / 10.0))
            } else {
                format!("no {word} here")
            });
        }
        details.push(format!("{}: {}", t.id, parts.join("; ")));
    }
    if options.lanes && options.tracks.is_empty() {
        for r in &p.returns {
            for lane in &r.automation {
                let cells = lane_cells(lane, &grid);
                rows.push((format!("{} {}", r.id, lane.param), cells));
            }
        }
        for lane in &p.master.automation {
            let cells = lane_cells(lane, &grid);
            rows.push((format!("master {}", lane.param), cells));
        }
    }

    // The legend: each letter's music and where it is.
    let mut legend = serde_json::Map::new();
    let mut lines = Vec::new();
    for key in &order {
        let mark = letters[key];
        let mut placements: Vec<(usize, &Placed)> = Vec::new();
        for (k, list) in clips.iter().enumerate() {
            placements.extend(list.iter().filter(|c| c.key == *key).map(|c| (shown[k], c)));
        }
        let what = &placements[0].1.what;
        let mut by_track: Vec<(usize, Vec<String>)> = Vec::new();
        for (ti, c) in &placements {
            let at = if c.repeats > 1 { format!("{} ×{}", place(c.start), c.repeats) } else { place(c.start) };
            match by_track.iter_mut().find(|(t, _)| t == ti) {
                Some((_, list)) => list.push(at),
                None => by_track.push((*ti, vec![at])),
            }
        }
        // Tracks that have it in the same places are named together.
        let mut groups: Vec<(Vec<&str>, Vec<String>)> = Vec::new();
        for (ti, ats) in by_track {
            let id = p.tracks[ti].id.as_str();
            match groups.iter_mut().find(|(_, a)| *a == ats) {
                Some((ids, _)) => ids.push(id),
                None => groups.push((vec![id], ats)),
            }
        }
        let (mut listed, mut covered) = (0, 0);
        let mut wheres = Vec::new();
        for (ids, ats) in &groups {
            let room = LISTED.saturating_sub(listed);
            if room == 0 {
                break;
            }
            let here: Vec<&str> = ats.iter().take(room).map(String::as_str).collect();
            listed += here.len();
            covered += here.len() * ids.len();
            let word = if ats.len() == 1 { "bar" } else { "bars" };
            wheres.push(format!("{} {word} {}", ids.join(", "), here.join(", ")));
        }
        let mut line = format!("{mark}  {what}; {}", wheres.join("; "));
        if placements.len() > covered {
            let _ = write!(line, ", +{} more", placements.len() - covered);
        }
        lines.push(line);
        let mut playing: Vec<&Placed> = placements.iter().map(|(_, c)| *c).collect();
        playing.sort_by(|a, b| a.start.total_cmp(&b.start));
        let refs: Vec<String> = playing.iter().map(|c| refer(&c.loc)).collect();
        legend.insert(mark.to_string(), json!(refs.join(" ")));
    }

    let width = rows.iter().map(|(n, _)| n.chars().count()).max().unwrap_or(0) + 2;
    let mut text: Vec<String> = rows.iter().map(|(name, cells)| format!("{name:<width$}{cells}")).collect();
    if grid.to > song_end + EPS {
        text.push(format!("The song ends where bar {} begins; nothing after it is heard.", place(song_end)));
    }
    for part in [lines, details] {
        if !part.is_empty() {
            text.push(String::new());
            text.extend(part);
        }
    }
    // A line an item, so that printed JSON keeps the grid's columns.
    Ok(json!({
        "map": text,
        "clips": legend,
        "from": grid.from,
        "to": grid.to,
        "per_beats": grid.per,
    }))
}
