//! What songs before note clips held, read only to convert them (D190,
//! D191): patterns, a track's own pads and the pattern clips that play
//! them. The song model refuses them; `convert` reads them from a song's
//! text and makes a Drum Rack and note clips of them.

use crate::beat::{beat as exact_beat, Beat};
use crate::schema::{beat_field, exact, fraction_str, note_field, Pad};
use crate::validate::{self as v, Bounds, Ctx, Fields};
use crate::value::{Key, Value};
use indexmap::IndexMap;
use num_rational::BigRational;
use num_traits::Signed;

#[derive(Clone, Debug)]
pub struct Event {
    pub at: Beat,
    pub pad: String,
    pub velocity: i64,
    pub note: Option<String>,
    pub duration: Option<Beat>,
    pub transpose: f64,
}

impl Event {
    const FIELDS: &'static [&'static str] = &["at", "pad", "velocity", "note", "duration", "transpose"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Event> {
        let f = Fields::of(ctx, x, "Event", Self::FIELDS)?;
        let before = ctx.count();
        let at = f.req(ctx, "at", beat_field);
        let pad = f.req(ctx, "pad", v::string);
        let velocity = f.opt(ctx, "velocity", 100, |c, x| v::int(c, x, Bounds::ge_le("1", "127")));
        let note = f.opt(ctx, "note", None, |c, x| v::optional(c, x, note_field));
        let duration = f.opt(ctx, "duration", None, |c, x| v::optional(c, x, beat_field));
        let transpose = f.opt(ctx, "transpose", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-36", "36")));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Event {
            at: at?,
            pad: pad?,
            velocity: velocity?,
            note: note?,
            duration: duration?,
            transpose: transpose?,
        })
    }

    pub fn at_exact(&self) -> BigRational {
        exact(&self.at)
    }

    pub fn duration_exact(&self) -> Option<BigRational> {
        self.duration.as_ref().map(exact)
    }
}


#[derive(Clone, Debug)]
pub struct Pattern {
    pub length_beats: Beat,
    pub grid: Beat,
    pub steps: IndexMap<String, String>,
    pub events: Vec<Event>,
    pub swing: f64,
}

/// Step cells: whitespace and bar lines removed, as `"".join(row.split())`.
pub fn step_cells(row: &str) -> Vec<char> {
    row.chars()
        .filter(|c| !crate::pyfmt::is_py_space(*c) && *c != '|')
        .collect()
}

/// Velocity for a step digit: `round(int(c) * 127 / 9)`.
const STEP_VELOCITY: [i64; 10] = [0, 14, 28, 42, 56, 71, 85, 99, 113, 127];

impl Pattern {
    const FIELDS: &'static [&'static str] = &["length_beats", "grid", "steps", "events", "swing"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Pattern> {
        let f = Fields::of(ctx, x, "Pattern", Self::FIELDS)?;
        let before = ctx.count();
        let length_beats = f.req(ctx, "length_beats", v::beat);
        let grid = f.opt(ctx, "grid", Beat::Str("1/4".into()), v::beat);
        let steps = f.opt(ctx, "steps", IndexMap::new(), |c, x| v::str_dict(c, x, v::string));
        let events = f.opt(ctx, "events", Vec::new(), |c, x| v::list(c, x, 0, None, Event::validate));
        let swing = f.opt(ctx, "swing", 0.5, |c, x| v::float(c, x, Bounds::ge_le("0.5", "0.75")));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        let p = Pattern {
            length_beats: length_beats?,
            grid: grid?,
            steps: steps?,
            events: events?,
            swing: swing?,
        };
        if let Err(msg) = p.timing_valid() {
            ctx.value_error(msg);
            return None;
        }
        Some(p)
    }

    fn timing_valid(&self) -> Result<(), String> {
        let length = exact_beat(&self.length_beats)?;
        let grid = exact_beat(&self.grid)?;
        if !length.is_positive() || !grid.is_positive() {
            return Err("Pattern length and grid must be positive".into());
        }
        for (pad, row) in &self.steps {
            let cells = step_cells(row);
            if cells.iter().any(|c| !".x123456789".contains(*c)) {
                return Err(format!("Invalid step character in {pad}"));
            }
            if BigRational::from_integer(cells.len().into()) * &grid != length {
                return Err(format!("Step row {pad} must span length_beats exactly"));
            }
        }
        if self.events.iter().any(|e| e.at_exact() >= length) {
            return Err("Pattern event starts outside pattern".into());
        }
        Ok(())
    }

    pub fn length_exact(&self) -> BigRational {
        exact(&self.length_beats)
    }

    pub fn grid_exact(&self) -> BigRational {
        exact(&self.grid)
    }

    /// Explicit events, then step-row events in row order, as `Pattern.expanded`.
    pub fn expanded(&self) -> Vec<Event> {
        let mut result = self.events.clone();
        let grid = self.grid_exact();
        let swing = crate::beat::float_fraction(2.0 * self.swing - 1.0);
        for (pad, row) in &self.steps {
            for (i, c) in step_cells(row).into_iter().enumerate() {
                if c == '.' {
                    continue;
                }
                let mut at = BigRational::from_integer(i.into()) * &grid;
                if i % 2 == 1 {
                    at += &grid * &swing;
                }
                let velocity = if c == 'x' {
                    100
                } else {
                    STEP_VELOCITY[c.to_digit(10).expect("step digit") as usize]
                };
                result.push(Event {
                    at: Beat::Str(fraction_str(&at)),
                    pad: pad.clone(),
                    velocity,
                    note: None,
                    duration: None,
                    transpose: 0.0,
                });
            }
        }
        result
    }
}


#[derive(Clone, Debug)]
pub struct Clip {
    pub pattern: String,
    pub at: Beat,
    pub repeats: i64,
    pub velocity_scale: f64,
    /// False when the clip is deactivated: it stays where it is and plays
    /// nothing, as a deactivated clip in Ableton.
    pub active: bool,
}

impl Clip {
    const FIELDS: &'static [&'static str] = &["pattern", "at", "repeats", "velocity_scale", "active"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Clip> {
        let f = Fields::of(ctx, x, "Clip", Self::FIELDS)?;
        let before = ctx.count();
        let pattern = f.req(ctx, "pattern", v::string);
        let at = f.opt(ctx, "at", Beat::int(0), beat_field);
        let repeats = f.opt(ctx, "repeats", 1, |c, x| v::int(c, x, Bounds::ge_le("1", "10000")));
        let velocity_scale = f.opt(ctx, "velocity_scale", 1.0, |c, x| v::float(c, x, Bounds::gt_le("0", "2")));
        let active = f.opt(ctx, "active", true, v::boolean);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Clip {
            pattern: pattern?,
            at: at?,
            repeats: repeats?,
            velocity_scale: velocity_scale?,
            active: active?,
        })
    }

    pub fn at_exact(&self) -> BigRational {
        exact(&self.at)
    }
}


/// A track of pads as it was written: its pads and its pattern clips.
#[derive(Clone, Debug, Default)]
pub struct Drums {
    pub pads: IndexMap<String, Pad>,
    pub clips: Vec<Clip>,
}

impl Drums {
    pub fn is_empty(&self) -> bool {
        self.pads.is_empty() && self.clips.is_empty()
    }
}

/// What an older song holds besides what the model reads: its patterns and,
/// by the index of each track that is not a MIDI track, its pads and
/// pattern clips.
#[derive(Clone, Debug, Default)]
pub struct Legacy {
    pub patterns: IndexMap<String, Pattern>,
    pub tracks: IndexMap<usize, Drums>,
}

impl Legacy {
    /// Whether a song's text holds anything to convert: a pattern, or a
    /// track that is not a MIDI track with pads or pattern clips. An empty
    /// `pads: {}`, which every audio track was written with, is not.
    pub fn found(song: &Value) -> bool {
        let filled = |v: Option<&Value>| match v {
            Some(Value::Dict(d)) => !d.is_empty(),
            Some(Value::List(l)) => !l.is_empty(),
            Some(Value::None) | None => false,
            Some(_) => true,
        };
        let Value::Dict(d) = song else { return false };
        if filled(d.get(&Key::str("patterns"))) {
            return true;
        }
        let Some(Value::List(tracks)) = d.get(&Key::str("tracks")) else { return false };
        tracks.iter().any(|t| match t {
            Value::Dict(t) if t.get(&Key::str("type")).is_none() => filled(t.get(&Key::str("pads"))) || filled(t.get(&Key::str("clips"))),
            _ => false,
        })
    }

    /// The song without its patterns and tracks' pads and pattern clips,
    /// which the model then reads, and those read on their own.
    pub fn take(song: &Value) -> Result<(Value, Legacy), crate::validate::ValidationError> {
        let mut ctx = Ctx::default();
        let mut legacy = Legacy::default();
        let mut rest = song.clone();
        if let Value::Dict(d) = &mut rest {
            if let Some(x) = d.shift_remove(&Key::str("patterns")) {
                if let Some(p) = ctx.key("patterns", |c| v::str_dict(c, &x, Pattern::validate)) {
                    legacy.patterns = p;
                }
            }
            if let Some(Value::List(tracks)) = d.get_mut(&Key::str("tracks")) {
                for (i, t) in tracks.iter_mut().enumerate() {
                    let Value::Dict(t) = t else { continue };
                    if t.get(&Key::str("type")).is_some() {
                        continue;
                    }
                    let pads = t.shift_remove(&Key::str("pads"));
                    let clips = t.shift_remove(&Key::str("clips"));
                    let mut drums = Drums::default();
                    ctx.key("tracks", |c| {
                        c.at(crate::validate::Loc::Index(i), |c| {
                            if let Some(x) = &pads {
                                if let Some(p) = c.key("pads", |c| v::str_dict(c, x, Pad::validate)) {
                                    drums.pads = p;
                                }
                            }
                            if let Some(x) = &clips {
                                if let Some(l) = c.key("clips", |c| v::list(c, x, 0, None, Clip::validate)) {
                                    drums.clips = l;
                                }
                            }
                        })
                    });
                    if !drums.is_empty() {
                        legacy.tracks.insert(i, drums);
                    }
                }
            }
        }
        if !ctx.errors.is_empty() {
            return Err(crate::validate::ValidationError { errors: ctx.errors });
        }
        Ok((rest, legacy))
    }
}
