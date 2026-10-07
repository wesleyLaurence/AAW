//! Schema v1 types, validated as pydantic validated the Python model's and
//! dumped as its `model_dump(mode="json")` dumped them, in full or in the saved
//! form without defaults.

use crate::beat::{beat as exact_beat, Beat};
use crate::rules;
use crate::validate::{self as v, Bounds, Ctx, Fields, Loc};
use crate::value::{Dict, Key, Value};
use indexmap::IndexMap;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};
use regex::Regex;
use std::sync::LazyLock;

pub const ID: &str = "^[a-zA-Z][a-zA-Z0-9_-]*$";
const SHA256: &str = "^[0-9a-f]{64}$";
static ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(ID).expect("id pattern"));
static SHA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(SHA256).expect("sha pattern"));

// ---------------------------------------------------------------------------
// Dumps

/// Builds one model's dump. The saved form (`exclude_none=True,
/// exclude_defaults=True`) leaves out None and fields equal to their default.
pub(crate) struct Out {
    saved: bool,
    dict: Dict,
}

impl Out {
    pub(crate) fn new(saved: bool) -> Out {
        Out {
            saved,
            dict: Dict::new(),
        }
    }

    fn put(&mut self, key: &str, value: Value) {
        self.dict.insert(Key::str(key), value);
    }

    /// A field without a default.
    fn req(&mut self, key: &str, value: Value) {
        self.put(key, value);
    }

    fn float(&mut self, key: &str, value: f64, default: f64) {
        if !(self.saved && value == default) {
            self.put(key, Value::Float(value));
        }
    }

    fn int(&mut self, key: &str, value: i64, default: i64) {
        if !(self.saved && value == default) {
            self.put(key, Value::int(value));
        }
    }

    fn bool(&mut self, key: &str, value: bool, default: bool) {
        if !(self.saved && value == default) {
            self.put(key, Value::Bool(value));
        }
    }

    fn str(&mut self, key: &str, value: &str, default: &str) {
        if !(self.saved && value == default) {
            self.put(key, Value::str(value));
        }
    }

    fn beat(&mut self, key: &str, value: &Beat, default: &Beat) {
        if !(self.saved && value.py_eq(default)) {
            self.put(key, value.to_value());
        }
    }

    /// A field defaulting to None.
    fn opt(&mut self, key: &str, value: Option<Value>) {
        match value {
            Some(x) => self.put(key, x),
            None if !self.saved => self.put(key, Value::None),
            None => {}
        }
    }

    /// A collection defaulting to empty.
    fn coll(&mut self, key: &str, value: Value) {
        let empty = match &value {
            Value::List(l) => l.is_empty(),
            Value::Dict(d) => d.is_empty(),
            _ => false,
        };
        if !(self.saved && empty) {
            self.put(key, value);
        }
    }

    pub(crate) fn done(self) -> Value {
        Value::Dict(self.dict)
    }
}

fn list<T>(items: &[T], saved: bool, f: impl Fn(&T, bool) -> Value) -> Value {
    Value::List(items.iter().map(|x| f(x, saved)).collect())
}

fn str_map<T>(items: &IndexMap<String, T>, saved: bool, f: impl Fn(&T, bool) -> Value) -> Value {
    Value::Dict(
        items
            .iter()
            .map(|(k, x)| (Key::str(k), f(x, saved)))
            .collect(),
    )
}

fn opt_str(s: &Option<String>) -> Option<Value> {
    s.as_deref().map(Value::str)
}

// ---------------------------------------------------------------------------
// Helpers shared by field validators

/// A beat field with the `beat()` after-validator.
fn beat_field(ctx: &mut Ctx, x: &Value) -> Option<Beat> {
    let b = v::beat(ctx, x)?;
    match exact_beat(&b) {
        Ok(_) => Some(b),
        Err(msg) => {
            ctx.value_error(msg);
            None
        }
    }
}

/// A beat field that may be negative.
fn signed_beat_field(ctx: &mut Ctx, x: &Value) -> Option<Beat> {
    let b = v::beat(ctx, x)?;
    match crate::beat::signed_beat(&b) {
        Ok(_) => Some(b),
        Err(msg) => {
            ctx.value_error(msg);
            None
        }
    }
}

fn note_field(ctx: &mut Ctx, x: &Value) -> Option<String> {
    let s = v::string(ctx, x)?;
    match rules::midi(&s) {
        Ok(_) => Some(s),
        Err(msg) => {
            ctx.value_error(msg);
            None
        }
    }
}

fn id_field(ctx: &mut Ctx, x: &Value) -> Option<String> {
    v::pattern(ctx, x, &ID_RE, ID)
}

fn opt_id(ctx: &mut Ctx, x: &Value) -> Option<Option<String>> {
    v::optional(ctx, x, id_field)
}

fn opt_string(ctx: &mut Ctx, x: &Value) -> Option<Option<String>> {
    v::optional(ctx, x, v::string)
}

fn exact(b: &Beat) -> BigRational {
    exact_beat(b).expect("validated beat")
}

// ---------------------------------------------------------------------------
// Models

#[derive(Clone, Debug)]
pub struct Sample {
    pub path: String,
    pub sha256: Option<String>,
    pub source: Option<String>,
    /// The hash of `source` when the file at `path` was decoded from it.
    pub source_sha256: Option<String>,
    pub root_note: Option<String>,
}

impl Sample {
    const FIELDS: &'static [&'static str] = &["path", "sha256", "source", "source_sha256", "root_note"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Sample> {
        let f = Fields::of(ctx, x, "Sample", Self::FIELDS)?;
        let before = ctx.count();
        let path = f.req(ctx, "path", v::string);
        let sha256 = f.opt(ctx, "sha256", None, |c, x| {
            v::optional(c, x, |c, x| v::pattern(c, x, &SHA_RE, SHA256))
        });
        let source = f.opt(ctx, "source", None, opt_string);
        let source_sha256 = f.opt(ctx, "source_sha256", None, |c, x| {
            v::optional(c, x, |c, x| v::pattern(c, x, &SHA_RE, SHA256))
        });
        let root_note = f.opt(ctx, "root_note", None, |c, x| v::optional(c, x, note_field));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Sample {
            path: path?,
            sha256: sha256?,
            source: source?,
            source_sha256: source_sha256?,
            root_note: root_note?,
        })
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("path", Value::str(&self.path));
        o.opt("sha256", opt_str(&self.sha256));
        o.opt("source", opt_str(&self.source));
        o.opt("source_sha256", opt_str(&self.source_sha256));
        o.opt("root_note", opt_str(&self.root_note));
        o.done()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadMode {
    OneShot,
    Gate,
}

impl PadMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PadMode::OneShot => "one_shot",
            PadMode::Gate => "gate",
        }
    }
}

/// What a pad does with a sample whose tempo is not the session's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stretch {
    /// Plays it faster or slower, and its pitch moves with its speed.
    Repitch,
    /// Stretches it in time and keeps its pitch.
    PreservePitch,
}

impl Stretch {
    pub fn as_str(self) -> &'static str {
        match self {
            Stretch::Repitch => "repitch",
            Stretch::PreservePitch => "preserve_pitch",
        }
    }
}

/// The time stretcher a song's `preserve_pitch` pads are stretched with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stretcher {
    /// Signalsmith Stretch, built into the engine.
    Signalsmith,
    /// Rubber Band's finer engine, run as the installed `rubberband` program.
    Rubberband,
}

impl Stretcher {
    pub fn as_str(self) -> &'static str {
        match self {
            Stretcher::Signalsmith => "signalsmith",
            Stretcher::Rubberband => "rubberband",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Pad {
    pub sample: String,
    pub mode: PadMode,
    pub gain_db: f64,
    pub pan: f64,
    pub transpose: f64,
    pub start_seconds: f64,
    pub end_seconds: Option<f64>,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub choke_group: Option<String>,
    pub reverse: bool,
    pub source_bpm: Option<f64>,
    pub stretch: Stretch,
    pub mono: bool,
}

impl Pad {
    const FIELDS: &'static [&'static str] = &[
        "sample", "mode", "gain_db", "pan", "transpose", "start_seconds", "end_seconds",
        "attack_ms", "release_ms", "choke_group", "reverse", "source_bpm", "stretch", "mono",
    ];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Pad> {
        let f = Fields::of(ctx, x, "Pad", Self::FIELDS)?;
        let before = ctx.count();
        let sample = f.req(ctx, "sample", v::string);
        let mode = f.opt(ctx, "mode", PadMode::OneShot, |c, x| {
            v::literal_str(c, x, &["one_shot", "gate"]).map(|m| {
                if m == "gate" {
                    PadMode::Gate
                } else {
                    PadMode::OneShot
                }
            })
        });
        let gain_db = f.opt(ctx, "gain_db", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-96", "24")));
        let pan = f.opt(ctx, "pan", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-1", "1")));
        let transpose = f.opt(ctx, "transpose", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-36", "36")));
        let start_seconds = f.opt(ctx, "start_seconds", 0.0, |c, x| v::float(c, x, Bounds::ge("0")));
        let end_seconds = f.opt(ctx, "end_seconds", None, |c, x| {
            v::optional(c, x, |c, x| v::float(c, x, Bounds::gt("0")))
        });
        let attack_ms = f.opt(ctx, "attack_ms", 0.3, |c, x| v::float(c, x, Bounds::ge_le("0", "10000")));
        let release_ms = f.opt(ctx, "release_ms", 8.0, |c, x| v::float(c, x, Bounds::ge_le("0", "10000")));
        let choke_group = f.opt(ctx, "choke_group", None, opt_string);
        let reverse = f.opt(ctx, "reverse", false, v::boolean);
        let source_bpm = f.opt(ctx, "source_bpm", None, |c, x| {
            v::optional(c, x, |c, x| v::float(c, x, Bounds::ge_le("20", "400")))
        });
        let stretch = f.opt(ctx, "stretch", Stretch::Repitch, |c, x| {
            v::literal_str(c, x, &["repitch", "preserve_pitch"]).map(|m| {
                if m == "preserve_pitch" {
                    Stretch::PreservePitch
                } else {
                    Stretch::Repitch
                }
            })
        });
        let mono = f.opt(ctx, "mono", false, v::boolean);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        let pad = Pad {
            sample: sample?,
            mode: mode?,
            gain_db: gain_db?,
            pan: pan?,
            transpose: transpose?,
            start_seconds: start_seconds?,
            end_seconds: end_seconds?,
            attack_ms: attack_ms?,
            release_ms: release_ms?,
            choke_group: choke_group?,
            reverse: reverse?,
            source_bpm: source_bpm?,
            stretch: stretch?,
            mono: mono?,
        };
        if pad.end_seconds.is_some_and(|end| end <= pad.start_seconds) {
            ctx.value_error("end_seconds must exceed start_seconds");
            return None;
        }
        Some(pad)
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("sample", Value::str(&self.sample));
        o.str("mode", self.mode.as_str(), "one_shot");
        o.float("gain_db", self.gain_db, 0.0);
        o.float("pan", self.pan, 0.0);
        o.float("transpose", self.transpose, 0.0);
        o.float("start_seconds", self.start_seconds, 0.0);
        o.opt("end_seconds", self.end_seconds.map(Value::Float));
        o.float("attack_ms", self.attack_ms, 0.3);
        o.float("release_ms", self.release_ms, 8.0);
        o.opt("choke_group", opt_str(&self.choke_group));
        o.bool("reverse", self.reverse, false);
        o.opt("source_bpm", self.source_bpm.map(Value::Float));
        o.str("stretch", self.stretch.as_str(), "repitch");
        o.bool("mono", self.mono, false);
        o.done()
    }
}

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

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("at", self.at.to_value());
        o.req("pad", Value::str(&self.pad));
        o.int("velocity", self.velocity, 100);
        o.opt("note", opt_str(&self.note));
        o.opt("duration", self.duration.as_ref().map(Beat::to_value));
        o.float("transpose", self.transpose, 0.0);
        o.done()
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

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("length_beats", self.length_beats.to_value());
        o.beat("grid", &self.grid, &Beat::Str("1/4".into()));
        o.coll(
            "steps",
            Value::Dict(
                self.steps
                    .iter()
                    .map(|(k, r)| (Key::str(k), Value::str(r)))
                    .collect(),
            ),
        );
        o.coll("events", list(&self.events, saved, Event::dump));
        o.float("swing", self.swing, 0.5);
        o.done()
    }
}

/// Python `str(Fraction)`.
pub fn fraction_str(x: &BigRational) -> String {
    if x.denom() == &1.into() {
        x.numer().to_string()
    } else {
        format!("{}/{}", x.numer(), x.denom())
    }
}

#[derive(Clone, Debug)]
pub struct Clip {
    pub pattern: String,
    pub at: Beat,
    pub repeats: i64,
    pub velocity_scale: f64,
}

impl Clip {
    const FIELDS: &'static [&'static str] = &["pattern", "at", "repeats", "velocity_scale"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Clip> {
        let f = Fields::of(ctx, x, "Clip", Self::FIELDS)?;
        let before = ctx.count();
        let pattern = f.req(ctx, "pattern", v::string);
        let at = f.opt(ctx, "at", Beat::int(0), beat_field);
        let repeats = f.opt(ctx, "repeats", 1, |c, x| v::int(c, x, Bounds::ge_le("1", "10000")));
        let velocity_scale = f.opt(ctx, "velocity_scale", 1.0, |c, x| v::float(c, x, Bounds::gt_le("0", "2")));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Clip {
            pattern: pattern?,
            at: at?,
            repeats: repeats?,
            velocity_scale: velocity_scale?,
        })
    }

    pub fn at_exact(&self) -> BigRational {
        exact(&self.at)
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("pattern", Value::str(&self.pattern));
        o.beat("at", &self.at, &Beat::int(0));
        o.int("repeats", self.repeats, 1);
        o.float("velocity_scale", self.velocity_scale, 1.0);
        o.done()
    }
}

/// The shape of an audio clip's fades.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FadeCurve {
    /// A quarter of a sine: two such fades of unlike audio sum to a steady level.
    EqualPower,
    /// A straight line: two such fades of the same audio sum to a steady level.
    Linear,
}

impl FadeCurve {
    pub fn as_str(self) -> &'static str {
        match self {
            FadeCurve::EqualPower => "equal_power",
            FadeCurve::Linear => "linear",
        }
    }
}

/// Part of a sample file on a track's timeline.
#[derive(Clone, Debug)]
pub struct AudioClip {
    pub sample: String,
    /// The beat `source_start_seconds` plays on.
    pub at: Beat,
    pub source_start_seconds: f64,
    /// None plays to the end of the file.
    pub source_end_seconds: Option<f64>,
    /// How long before `at` the clip starts: a cut placed ahead of the beat, so
    /// the hit on the beat is whole. A clip that ends where another of its track
    /// begins leaves from where that one starts.
    pub lead_ms: f64,
    pub gain_db: f64,
    pub fade_in_ms: f64,
    /// The fade out follows where the clip leaves, so a clip that starts there
    /// fades in under it.
    pub fade_out_ms: f64,
    pub fade_curve: FadeCurve,
    pub source_bpm: Option<f64>,
    pub stretch: Stretch,
    /// With it, the first `loop_beats` of the clip play again and again from
    /// the clip's start until `length_beats`, each repetition a copy of the
    /// clip with its own lead and fades, the last cut off where the clip ends.
    pub loop_beats: Option<Beat>,
    /// How long a looped clip lasts on the timeline. Only with `loop_beats`.
    pub length_beats: Option<Beat>,
}

impl AudioClip {
    const FIELDS: &'static [&'static str] = &[
        "sample", "at", "source_start_seconds", "source_end_seconds", "lead_ms", "gain_db", "fade_in_ms",
        "fade_out_ms", "fade_curve", "source_bpm", "stretch", "loop_beats", "length_beats",
    ];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<AudioClip> {
        let f = Fields::of(ctx, x, "AudioClip", Self::FIELDS)?;
        let before = ctx.count();
        let sample = f.req(ctx, "sample", v::string);
        let at = f.opt(ctx, "at", Beat::int(0), beat_field);
        let source_start_seconds = f.opt(ctx, "source_start_seconds", 0.0, |c, x| v::float(c, x, Bounds::ge("0")));
        let source_end_seconds = f.opt(ctx, "source_end_seconds", None, |c, x| {
            v::optional(c, x, |c, x| v::float(c, x, Bounds::gt("0")))
        });
        let lead_ms = f.opt(ctx, "lead_ms", 0.0, |c, x| v::float(c, x, Bounds::ge_le("0", "1000")));
        let gain_db = f.opt(ctx, "gain_db", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-96", "24")));
        let fade_in_ms = f.opt(ctx, "fade_in_ms", 0.3, |c, x| v::float(c, x, Bounds::ge_le("0", "10000")));
        let fade_out_ms = f.opt(ctx, "fade_out_ms", 8.0, |c, x| v::float(c, x, Bounds::ge_le("0", "10000")));
        let fade_curve = f.opt(ctx, "fade_curve", FadeCurve::EqualPower, |c, x| {
            v::literal_str(c, x, &["equal_power", "linear"]).map(|m| {
                if m == "linear" {
                    FadeCurve::Linear
                } else {
                    FadeCurve::EqualPower
                }
            })
        });
        let source_bpm = f.opt(ctx, "source_bpm", None, |c, x| {
            v::optional(c, x, |c, x| v::float(c, x, Bounds::ge_le("20", "400")))
        });
        let stretch = f.opt(ctx, "stretch", Stretch::Repitch, |c, x| {
            v::literal_str(c, x, &["repitch", "preserve_pitch"]).map(|m| {
                if m == "preserve_pitch" {
                    Stretch::PreservePitch
                } else {
                    Stretch::Repitch
                }
            })
        });
        let loop_beats = f.opt(ctx, "loop_beats", None, |c, x| v::optional(c, x, beat_field));
        let length_beats = f.opt(ctx, "length_beats", None, |c, x| v::optional(c, x, beat_field));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        let clip = AudioClip {
            sample: sample?,
            at: at?,
            source_start_seconds: source_start_seconds?,
            source_end_seconds: source_end_seconds?,
            lead_ms: lead_ms?,
            gain_db: gain_db?,
            fade_in_ms: fade_in_ms?,
            fade_out_ms: fade_out_ms?,
            fade_curve: fade_curve?,
            source_bpm: source_bpm?,
            stretch: stretch?,
            loop_beats: loop_beats?,
            length_beats: length_beats?,
        };
        if clip.source_end_seconds.is_some_and(|end| end <= clip.source_start_seconds) {
            ctx.value_error("source_end_seconds must exceed source_start_seconds");
            return None;
        }
        match (&clip.loop_beats, &clip.length_beats) {
            (Some(l), Some(n)) => {
                if !exact(l).is_positive() || !exact(n).is_positive() {
                    ctx.value_error("loop_beats and length_beats must be positive");
                    return None;
                }
            }
            (Some(_), None) => {
                ctx.value_error("A looped audio clip needs length_beats, how long it plays");
                return None;
            }
            (None, Some(_)) => {
                ctx.value_error("length_beats is how far a loop repeats; it needs loop_beats");
                return None;
            }
            (None, None) => {}
        }
        Some(clip)
    }

    pub fn at_exact(&self) -> BigRational {
        exact(&self.at)
    }

    /// The loop's length, when the clip loops.
    pub fn loop_exact(&self) -> Option<BigRational> {
        self.loop_beats.as_ref().map(exact)
    }

    /// How long a looped clip lasts on the timeline.
    pub fn length_exact(&self) -> Option<BigRational> {
        self.length_beats.as_ref().map(exact)
    }

    /// Seconds of the clip's file that go by in a beat of the song at
    /// `tempo`: a clip with a tempo of its own follows the song's, so a beat
    /// of the song is a beat of its file.
    pub fn seconds_per_beat(&self, tempo: f64) -> f64 {
        60.0 / self.source_bpm.unwrap_or(tempo)
    }

    /// The clip as the engine plays it: itself, or a looped clip as its
    /// repetitions, each a copy from the clip's start to the loop's end that
    /// starts a loop later, the last cut off where the clip ends. Each copy
    /// keeps the clip's lead and fades, so a repetition leaves from where the
    /// next starts, as two clips that meet do.
    pub fn repetitions(&self, tempo: f64) -> Vec<AudioClip> {
        let (Some(every), Some(length)) = (self.loop_exact(), self.length_exact()) else {
            return vec![self.clone()];
        };
        let per_beat = self.seconds_per_beat(tempo);
        let start = self.at_exact();
        let mut out = Vec::new();
        let mut from = BigRational::zero();
        while from < length {
            let beats = every.clone().min(&length - &from);
            let end = self.source_start_seconds + beats.to_f64().unwrap_or(0.0) * per_beat;
            out.push(AudioClip {
                at: Beat::from_exact(&(&start + &from)),
                source_end_seconds: Some(self.source_end_seconds.map_or(end, |own| own.min(end))),
                loop_beats: None,
                length_beats: None,
                ..self.clone()
            });
            from += &every;
        }
        out
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("sample", Value::str(&self.sample));
        o.beat("at", &self.at, &Beat::int(0));
        o.float("source_start_seconds", self.source_start_seconds, 0.0);
        o.opt("source_end_seconds", self.source_end_seconds.map(Value::Float));
        o.float("lead_ms", self.lead_ms, 0.0);
        o.float("gain_db", self.gain_db, 0.0);
        o.float("fade_in_ms", self.fade_in_ms, 0.3);
        o.float("fade_out_ms", self.fade_out_ms, 8.0);
        o.str("fade_curve", self.fade_curve.as_str(), "equal_power");
        o.opt("source_bpm", self.source_bpm.map(Value::Float));
        o.str("stretch", self.stretch.as_str(), "repitch");
        o.opt("loop_beats", self.loop_beats.as_ref().map(Beat::to_value));
        o.opt("length_beats", self.length_beats.as_ref().map(Beat::to_value));
        o.done()
    }
}

/// A MIDI pitch, 0 to 127, given as a number or as a note name such as C4
/// (60), which is stored as its number.
fn pitch_field(ctx: &mut Ctx, x: &Value) -> Option<i64> {
    if let Value::Str(s) = x {
        if s.trim().starts_with(|c: char| c.is_ascii_alphabetic()) {
            return match rules::midi(s.trim()) {
                Ok(n) => Some(n),
                Err(msg) => {
                    ctx.value_error(msg);
                    None
                }
            };
        }
    }
    v::int(ctx, x, Bounds::ge_le("0", "127"))
}

/// The number in an ID such as `n12` or `clip3` that starts with `prefix`.
fn id_number(id: &str, prefix: &str) -> Option<u64> {
    id.strip_prefix(prefix)
        .filter(|n| !n.is_empty() && !n.starts_with('0') && n.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|n| n.parse().ok())
}

/// Fills in the IDs left empty with `prefix` and the next number after the
/// highest such ID there is.
fn assign_ids<'a>(ids: impl Iterator<Item = &'a mut String>, prefix: &str) {
    let mut ids: Vec<&mut String> = ids.collect();
    let mut next = ids.iter().filter_map(|id| id_number(id, prefix)).max().unwrap_or(0);
    for id in ids.iter_mut().filter(|id| id.is_empty()) {
        next += 1;
        **id = format!("{prefix}{next}");
    }
}

/// A note of a note clip.
#[derive(Clone, Debug)]
pub struct Note {
    /// Unique in its clip. A note written without one is given the next free
    /// `nN` when the song is validated.
    pub id: String,
    pub pitch: i64,
    /// Beats from its clip's start. A note before the start, which a clip
    /// shortened from its left edge leaves, is negative.
    pub at: Beat,
    pub duration: Beat,
    pub velocity: i64,
}

impl Note {
    const FIELDS: &'static [&'static str] = &["id", "pitch", "at", "duration", "velocity"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Note> {
        let f = Fields::of(ctx, x, "Note", Self::FIELDS)?;
        let before = ctx.count();
        let id = f.opt(ctx, "id", String::new(), id_field);
        let pitch = f.req(ctx, "pitch", pitch_field);
        let at = f.opt(ctx, "at", Beat::int(0), signed_beat_field);
        let duration = f.req(ctx, "duration", beat_field);
        let velocity = f.opt(ctx, "velocity", 100, |c, x| v::int(c, x, Bounds::ge_le("1", "127")));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        let note = Note {
            id: id?,
            pitch: pitch?,
            at: at?,
            duration: duration?,
            velocity: velocity?,
        };
        if !note.duration_exact().is_positive() {
            ctx.value_error("A note's duration must be positive");
            return None;
        }
        Some(note)
    }

    pub fn at_exact(&self) -> BigRational {
        crate::beat::signed_beat(&self.at).expect("validated beat")
    }

    pub fn duration_exact(&self) -> BigRational {
        exact(&self.duration)
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("id", Value::str(&self.id));
        o.req("pitch", Value::int(self.pitch));
        o.beat("at", &self.at, &Beat::int(0));
        o.req("duration", self.duration.to_value());
        o.int("velocity", self.velocity, 100);
        o.done()
    }
}

/// A clip of a MIDI track: notes it owns, placed on the song's timeline.
#[derive(Clone, Debug)]
pub struct NoteClip {
    /// Unique among the song's note clips. A clip written without one is
    /// given the next free `clipN` when the song is validated.
    pub id: String,
    pub at: Beat,
    pub length_beats: Beat,
    /// With it, the notes of the clip's first `loop_beats` play again and
    /// again from the clip's start until its end, the last repetition cut off
    /// there; a note at or after the loop's end is kept and does not play.
    pub loop_beats: Option<Beat>,
    /// In the order they were written. A note that starts before the clip or
    /// at or after its end, or its loop's, is kept and does not play.
    pub notes: Vec<Note>,
}

impl NoteClip {
    const FIELDS: &'static [&'static str] = &["id", "at", "length_beats", "loop_beats", "notes"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<NoteClip> {
        let f = Fields::of(ctx, x, "NoteClip", Self::FIELDS)?;
        let before = ctx.count();
        let id = f.opt(ctx, "id", String::new(), id_field);
        let at = f.opt(ctx, "at", Beat::int(0), beat_field);
        let length_beats = f.req(ctx, "length_beats", beat_field);
        let loop_beats = f.opt(ctx, "loop_beats", None, |c, x| v::optional(c, x, beat_field));
        let notes = f.opt(ctx, "notes", Vec::new(), |c, x| v::list(c, x, 0, None, Note::validate));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        let mut clip = NoteClip {
            id: id?,
            at: at?,
            length_beats: length_beats?,
            loop_beats: loop_beats?,
            notes: notes?,
        };
        if !clip.length_exact().is_positive() {
            ctx.value_error("A clip's length must be positive");
            return None;
        }
        if clip.loop_exact().is_some_and(|l| !l.is_positive()) {
            ctx.value_error("A clip's loop_beats must be positive");
            return None;
        }
        let mut seen = std::collections::HashSet::new();
        if let Some(n) = clip.notes.iter().find(|n| !n.id.is_empty() && !seen.insert(n.id.as_str())) {
            ctx.value_error(format!("Two notes of a clip have the ID {}", n.id));
            return None;
        }
        assign_ids(clip.notes.iter_mut().map(|n| &mut n.id), "n");
        Some(clip)
    }

    pub fn at_exact(&self) -> BigRational {
        exact(&self.at)
    }

    pub fn length_exact(&self) -> BigRational {
        exact(&self.length_beats)
    }

    /// The loop's length, when the clip loops.
    pub fn loop_exact(&self) -> Option<BigRational> {
        self.loop_beats.as_ref().map(exact)
    }

    /// How many beats of the clip's notes play: its loop, or the whole clip.
    pub fn plays_until(&self) -> BigRational {
        self.loop_exact().unwrap_or_else(|| self.length_exact())
    }

    /// Whether a note of the clip plays: it starts inside the clip and, when
    /// the clip loops, inside the loop.
    pub fn plays(&self, note: &Note) -> bool {
        let at = note.at_exact();
        !at.is_negative() && at < self.plays_until()
    }

    /// Where each repetition of the clip starts, from the clip's start, and
    /// how much of it plays: one of the clip's length, or a loop each until
    /// the end, the last cut off there.
    pub fn repetitions(&self) -> Vec<(BigRational, BigRational)> {
        let length = self.length_exact();
        let Some(every) = self.loop_exact() else {
            return vec![(BigRational::zero(), length)];
        };
        let mut out = Vec::new();
        let mut from = BigRational::zero();
        while from < length {
            out.push((from.clone(), every.clone().min(&length - &from)));
            from += &every;
        }
        out
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("id", Value::str(&self.id));
        o.beat("at", &self.at, &Beat::int(0));
        o.req("length_beats", self.length_beats.to_value());
        o.opt("loop_beats", self.loop_beats.as_ref().map(Beat::to_value));
        o.coll("notes", list(&self.notes, saved, Note::dump));
        o.done()
    }
}

/// Which notes play a pad of a sampler.
#[derive(Clone, Debug)]
pub struct NoteMap {
    /// The lowest and highest note, inclusive.
    pub low: i64,
    pub high: i64,
    /// Whether it was written as a range rather than one note.
    pub range: bool,
    pub pad: String,
    /// Whether the pad plays at the note's pitch, repitched from its sample's
    /// root note, or from middle C when it has none, rather than as it is.
    pub pitched: bool,
}

impl NoteMap {
    const FIELDS: &'static [&'static str] = &["notes", "pad", "pitched"];

    fn notes(ctx: &mut Ctx, x: &Value) -> Option<(i64, i64, bool)> {
        match x {
            Value::List(_) => {
                let both = v::list(ctx, x, 2, Some(2), pitch_field)?;
                if both[0] > both[1] {
                    ctx.value_error("A range of notes runs from the lower to the higher");
                    return None;
                }
                Some((both[0], both[1], true))
            }
            other => pitch_field(ctx, other).map(|n| (n, n, false)),
        }
    }

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<NoteMap> {
        let f = Fields::of(ctx, x, "NoteMap", Self::FIELDS)?;
        let before = ctx.count();
        let notes = f.req(ctx, "notes", Self::notes);
        let pad = f.req(ctx, "pad", v::string);
        let pitched = f.opt(ctx, "pitched", false, v::boolean);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        let (low, high, range) = notes?;
        Some(NoteMap {
            low,
            high,
            range,
            pad: pad?,
            pitched: pitched?,
        })
    }

    pub fn contains(&self, pitch: i64) -> bool {
        (self.low..=self.high).contains(&pitch)
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        let notes = if self.range {
            Value::List(vec![Value::int(self.low), Value::int(self.high)])
        } else {
            Value::int(self.low)
        };
        o.req("notes", notes);
        o.req("pad", Value::str(&self.pad));
        o.bool("pitched", self.pitched, false);
        o.done()
    }
}

/// A sampler that plays notes: its pads, and which notes play each.
#[derive(Clone, Debug, Default)]
pub struct Sampler {
    pub pads: IndexMap<String, Pad>,
    pub map: Vec<NoteMap>,
}

impl Sampler {
    const FIELDS: &'static [&'static str] = &["pads", "map"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Sampler> {
        let f = Fields::of(ctx, x, "Sampler", Self::FIELDS)?;
        let before = ctx.count();
        let pads = f.opt(ctx, "pads", IndexMap::new(), |c, x| v::str_dict(c, x, Pad::validate));
        let map = f.opt(ctx, "map", Vec::new(), |c, x| v::list(c, x, 0, Some(128), NoteMap::validate));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Sampler { pads: pads?, map: map? })
    }

    /// The map entry a note plays, if any.
    pub fn entry(&self, pitch: i64) -> Option<&NoteMap> {
        self.map.iter().find(|m| m.contains(pitch))
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.coll("pads", str_map(&self.pads, saved, Pad::dump));
        o.coll("map", list(&self.map, saved, NoteMap::dump));
        o.done()
    }
}

// ---------------------------------------------------------------------------
// The Synth

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wave {
    Sine,
    Triangle,
    Saw,
    Square,
    Pulse,
    Noise,
    /// One cycle read from a table: a built-in one, or a sample in the
    /// project, named by the oscillator's `table`.
    Wavetable,
}

impl Wave {
    pub const NAMES: [&'static str; 7] = ["sine", "triangle", "saw", "square", "pulse", "noise", "wavetable"];

    pub fn as_str(self) -> &'static str {
        match self {
            Wave::Sine => "sine",
            Wave::Triangle => "triangle",
            Wave::Saw => "saw",
            Wave::Square => "square",
            Wave::Pulse => "pulse",
            Wave::Noise => "noise",
            Wave::Wavetable => "wavetable",
        }
    }

    fn from_str(s: &str) -> Wave {
        match s {
            "sine" => Wave::Sine,
            "triangle" => Wave::Triangle,
            "square" => Wave::Square,
            "pulse" => Wave::Pulse,
            "noise" => Wave::Noise,
            "wavetable" => Wave::Wavetable,
            _ => Wave::Saw,
        }
    }
}

/// The wavetables built into the Synth, which an oscillator's `table` names
/// when it is not a sample of the project.
pub const WAVETABLES: [&str; 6] = ["organ", "bright", "hollow", "vowel", "fold", "steps"];

/// The most sub-oscillators an oscillator's unison stacks.
pub const MAX_UNISON: i64 = 16;

/// An oscillator of the Synth: one of up to four, each with its own wave,
/// level, pan and tuning.
#[derive(Clone, Debug, PartialEq)]
pub struct Oscillator {
    pub wave: Wave,
    /// The wavetable a `wavetable` wave reads: one of `WAVETABLES`, or the
    /// ID of a sample in the project whose file is one cycle.
    pub table: String,
    pub level_db: f64,
    pub pan: f64,
    pub octave: i64,
    pub semitones: f64,
    pub detune_cents: f64,
    /// The high part of a pulse's cycle, in percent.
    pub pulse_width: f64,
    /// Where in its cycle the wave starts, in percent; None starts each note
    /// at a random place, from the patch's seed.
    pub phase: Option<f64>,
    /// How many copies of the wave play at once, spread in detune and across
    /// the stereo field; 1 is one.
    pub unison: i64,
    /// How far the outermost copies are detuned either side of the pitch.
    pub unison_detune_cents: f64,
    /// How far the copies are spread across the stereo field from the pan.
    pub unison_width_percent: f64,
    /// Whether it goes through the filter.
    pub filter: bool,
}

impl Oscillator {
    const FIELDS: &'static [&'static str] = &[
        "wave", "table", "level_db", "pan", "octave", "semitones", "detune_cents", "pulse_width", "phase", "unison",
        "unison_detune_cents", "unison_width_percent", "filter",
    ];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Oscillator> {
        let f = Fields::of(ctx, x, "Oscillator", Self::FIELDS)?;
        let before = ctx.count();
        let wave = f.opt(ctx, "wave", Wave::Saw, |c, x| v::literal_str(c, x, &Wave::NAMES).map(Wave::from_str));
        let table = f.opt(ctx, "table", WAVETABLES[0].to_string(), |c, x| {
            let s = v::string(c, x)?;
            if WAVETABLES.contains(&s.as_str()) || ID_RE.is_match(&s) {
                Some(s)
            } else {
                c.value_error(format!(
                    "table is a built-in wavetable ({}) or the ID of a sample in the project",
                    WAVETABLES.join(", ")
                ));
                None
            }
        });
        let level_db = f.opt(ctx, "level_db", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-96", "24")));
        let pan = f.opt(ctx, "pan", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-1", "1")));
        let octave = f.opt(ctx, "octave", 0, |c, x| v::int(c, x, Bounds::ge_le("-4", "4")));
        let semitones = f.opt(ctx, "semitones", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-36", "36")));
        let detune_cents = f.opt(ctx, "detune_cents", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-100", "100")));
        let pulse_width = f.opt(ctx, "pulse_width", 50.0, |c, x| v::float(c, x, Bounds::ge_le("1", "99")));
        let phase = f.opt(ctx, "phase", None, |c, x| v::optional(c, x, |c, x| v::float(c, x, Bounds::ge_le("0", "100"))));
        let unison = f.opt(ctx, "unison", 1, |c, x| v::int(c, x, Bounds::ge_le("1", "16")));
        let unison_detune_cents = f.opt(ctx, "unison_detune_cents", 15.0, |c, x| v::float(c, x, Bounds::ge_le("0", "100")));
        let unison_width_percent = f.opt(ctx, "unison_width_percent", 100.0, |c, x| v::float(c, x, Bounds::ge_le("0", "100")));
        let filter = f.opt(ctx, "filter", true, v::boolean);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Oscillator {
            wave: wave?,
            table: table?,
            level_db: level_db?,
            pan: pan?,
            octave: octave?,
            semitones: semitones?,
            detune_cents: detune_cents?,
            pulse_width: pulse_width?,
            phase: phase?,
            unison: unison?,
            unison_detune_cents: unison_detune_cents?,
            unison_width_percent: unison_width_percent?,
            filter: filter?,
        })
    }

    /// The sample of the project the oscillator's table names, if it is not
    /// a built-in one; it is read when the wave is `wavetable`, and must be
    /// in the project either way.
    pub fn sample_table(&self) -> Option<&str> {
        (!WAVETABLES.contains(&self.table.as_str())).then_some(self.table.as_str())
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.str("wave", self.wave.as_str(), "saw");
        o.str("table", &self.table, WAVETABLES[0]);
        o.float("level_db", self.level_db, 0.0);
        o.float("pan", self.pan, 0.0);
        o.int("octave", self.octave, 0);
        o.float("semitones", self.semitones, 0.0);
        o.float("detune_cents", self.detune_cents, 0.0);
        o.float("pulse_width", self.pulse_width, 50.0);
        o.opt("phase", self.phase.map(Value::Float));
        o.int("unison", self.unison, 1);
        o.float("unison_detune_cents", self.unison_detune_cents, 15.0);
        o.float("unison_width_percent", self.unison_width_percent, 100.0);
        o.bool("filter", self.filter, true);
        o.done()
    }
}

impl Default for Oscillator {
    /// A plain saw at full level, in tune, through the filter.
    fn default() -> Self {
        Oscillator {
            wave: Wave::Saw,
            table: WAVETABLES[0].to_string(),
            level_db: 0.0,
            pan: 0.0,
            octave: 0,
            semitones: 0.0,
            detune_cents: 0.0,
            pulse_width: 50.0,
            phase: None,
            unison: 1,
            unison_detune_cents: 15.0,
            unison_width_percent: 100.0,
            filter: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SynthFilterMode {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
}

impl SynthFilterMode {
    pub const NAMES: [&'static str; 4] = ["lowpass", "highpass", "bandpass", "notch"];

    pub fn as_str(self) -> &'static str {
        match self {
            SynthFilterMode::Lowpass => "lowpass",
            SynthFilterMode::Highpass => "highpass",
            SynthFilterMode::Bandpass => "bandpass",
            SynthFilterMode::Notch => "notch",
        }
    }
}

/// The Synth's filter: one for each voice, after the oscillators that go
/// through it.
#[derive(Clone, Debug, PartialEq)]
pub struct SynthFilter {
    pub enabled: bool,
    pub mode: SynthFilterMode,
    /// 12 or 24.
    pub slope_db_per_octave: i64,
    pub cutoff_hz: f64,
    /// 0 is a Q of a half; 100 rings just short of self-oscillation.
    pub resonance_percent: f64,
    /// Gain into a soft clip before the filter.
    pub drive_db: f64,
    /// How far the cutoff follows the note: 100 moves it an octave an octave
    /// from middle C.
    pub keytrack_percent: f64,
}

impl SynthFilter {
    const FIELDS: &'static [&'static str] = &[
        "enabled", "mode", "slope_db_per_octave", "cutoff_hz", "resonance_percent", "drive_db", "keytrack_percent",
    ];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<SynthFilter> {
        let f = Fields::of(ctx, x, "SynthFilter", Self::FIELDS)?;
        let before = ctx.count();
        let enabled = f.opt(ctx, "enabled", true, v::boolean);
        let mode = f.opt(ctx, "mode", SynthFilterMode::Lowpass, |c, x| {
            v::literal_str(c, x, &SynthFilterMode::NAMES).map(|m| match m {
                "highpass" => SynthFilterMode::Highpass,
                "bandpass" => SynthFilterMode::Bandpass,
                "notch" => SynthFilterMode::Notch,
                _ => SynthFilterMode::Lowpass,
            })
        });
        let slope = f.opt(ctx, "slope_db_per_octave", 12, |c, x| v::literal_int(c, x, &[12, 24]));
        let cutoff_hz = f.opt(ctx, "cutoff_hz", 20000.0, |c, x| v::float(c, x, Bounds::ge_le("10", "20000")));
        let resonance_percent = f.opt(ctx, "resonance_percent", 0.0, |c, x| v::float(c, x, Bounds::ge_le("0", "100")));
        let drive_db = f.opt(ctx, "drive_db", 0.0, |c, x| v::float(c, x, Bounds::ge_le("0", "24")));
        let keytrack_percent = f.opt(ctx, "keytrack_percent", 0.0, |c, x| v::float(c, x, Bounds::ge_le("0", "100")));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(SynthFilter {
            enabled: enabled?,
            mode: mode?,
            slope_db_per_octave: slope?,
            cutoff_hz: cutoff_hz?,
            resonance_percent: resonance_percent?,
            drive_db: drive_db?,
            keytrack_percent: keytrack_percent?,
        })
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.bool("enabled", self.enabled, true);
        o.str("mode", self.mode.as_str(), "lowpass");
        o.int("slope_db_per_octave", self.slope_db_per_octave, 12);
        o.float("cutoff_hz", self.cutoff_hz, 20000.0);
        o.float("resonance_percent", self.resonance_percent, 0.0);
        o.float("drive_db", self.drive_db, 0.0);
        o.float("keytrack_percent", self.keytrack_percent, 0.0);
        o.done()
    }
}

impl Default for SynthFilter {
    /// A lowpass open all the way, which passes the oscillators as they are.
    fn default() -> Self {
        SynthFilter {
            enabled: true,
            mode: SynthFilterMode::Lowpass,
            slope_db_per_octave: 12,
            cutoff_hz: 20000.0,
            resonance_percent: 0.0,
            drive_db: 0.0,
            keytrack_percent: 0.0,
        }
    }
}

/// An envelope of the Synth: `amp` shapes each voice's level, and any other
/// moves what the matrix wires it to.
#[derive(Clone, Debug, PartialEq)]
pub struct SynthEnvelope {
    pub attack_ms: f64,
    pub decay_ms: f64,
    pub sustain_percent: f64,
    pub release_ms: f64,
}

impl SynthEnvelope {
    const FIELDS: &'static [&'static str] = &["attack_ms", "decay_ms", "sustain_percent", "release_ms"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<SynthEnvelope> {
        let f = Fields::of(ctx, x, "SynthEnvelope", Self::FIELDS)?;
        let before = ctx.count();
        let ms = |ctx: &mut Ctx, name: &str, default: f64| f.opt(ctx, name, default, |c, x| v::float(c, x, Bounds::ge_le("0", "20000")));
        let attack_ms = ms(ctx, "attack_ms", 1.0);
        let decay_ms = ms(ctx, "decay_ms", 100.0);
        let sustain_percent = f.opt(ctx, "sustain_percent", 100.0, |c, x| v::float(c, x, Bounds::ge_le("0", "100")));
        let release_ms = ms(ctx, "release_ms", 50.0);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(SynthEnvelope {
            attack_ms: attack_ms?,
            decay_ms: decay_ms?,
            sustain_percent: sustain_percent?,
            release_ms: release_ms?,
        })
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.float("attack_ms", self.attack_ms, 1.0);
        o.float("decay_ms", self.decay_ms, 100.0);
        o.float("sustain_percent", self.sustain_percent, 100.0);
        o.float("release_ms", self.release_ms, 50.0);
        o.done()
    }
}

impl Default for SynthEnvelope {
    /// A plain envelope: at once, held, and a short release.
    fn default() -> Self {
        SynthEnvelope {
            attack_ms: 1.0,
            decay_ms: 100.0,
            sustain_percent: 100.0,
            release_ms: 50.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LfoShape {
    Sine,
    Triangle,
    Saw,
    Square,
    SampleHold,
}

impl LfoShape {
    pub const NAMES: [&'static str; 5] = ["sine", "triangle", "saw", "square", "sample_hold"];

    pub fn as_str(self) -> &'static str {
        match self {
            LfoShape::Sine => "sine",
            LfoShape::Triangle => "triangle",
            LfoShape::Saw => "saw",
            LfoShape::Square => "square",
            LfoShape::SampleHold => "sample_hold",
        }
    }
}

/// A low-frequency oscillator of the Synth, bipolar, which moves what the
/// matrix wires it to.
#[derive(Clone, Debug, PartialEq)]
pub struct Lfo {
    pub shape: LfoShape,
    /// Cycles a second, unless `rate_beats` is given.
    pub rate_hz: f64,
    /// One cycle in so many beats at the song's tempo, in place of `rate_hz`.
    pub rate_beats: Option<Beat>,
    /// Where in its cycle it starts, in percent.
    pub phase_percent: f64,
    /// Whether each note starts the cycle over; otherwise the LFO runs from
    /// the start of the song and every voice shares it.
    pub retrigger: bool,
}

impl Lfo {
    const FIELDS: &'static [&'static str] = &["shape", "rate_hz", "rate_beats", "phase_percent", "retrigger"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Lfo> {
        let f = Fields::of(ctx, x, "Lfo", Self::FIELDS)?;
        let before = ctx.count();
        let shape = f.opt(ctx, "shape", LfoShape::Sine, |c, x| {
            v::literal_str(c, x, &LfoShape::NAMES).map(|s| match s {
                "triangle" => LfoShape::Triangle,
                "saw" => LfoShape::Saw,
                "square" => LfoShape::Square,
                "sample_hold" => LfoShape::SampleHold,
                _ => LfoShape::Sine,
            })
        });
        let rate_hz = f.opt(ctx, "rate_hz", 1.0, |c, x| v::float(c, x, Bounds::ge_le("0.01", "100")));
        let rate_beats = f.opt(ctx, "rate_beats", None, |c, x| v::optional(c, x, beat_field));
        let phase_percent = f.opt(ctx, "phase_percent", 0.0, |c, x| v::float(c, x, Bounds::ge_le("0", "100")));
        let retrigger = f.opt(ctx, "retrigger", false, v::boolean);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        let lfo = Lfo {
            shape: shape?,
            rate_hz: rate_hz?,
            rate_beats: rate_beats?,
            phase_percent: phase_percent?,
            retrigger: retrigger?,
        };
        if let Some(beats) = &lfo.rate_beats {
            let b = exact_beat(beats).expect("validated beat");
            if !b.is_positive() || b > BigRational::from_integer(64.into()) {
                ctx.value_error("rate_beats must be more than 0 and at most 64");
                return None;
            }
        }
        Some(lfo)
    }

    /// Beats a cycle takes, when the rate is in beats.
    pub fn beats_exact(&self) -> Option<BigRational> {
        self.rate_beats.as_ref().map(exact)
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.str("shape", self.shape.as_str(), "sine");
        o.float("rate_hz", self.rate_hz, 1.0);
        o.opt("rate_beats", self.rate_beats.as_ref().map(Beat::to_value));
        o.float("phase_percent", self.phase_percent, 0.0);
        o.bool("retrigger", self.retrigger, false);
        o.done()
    }
}

/// An entry of the Synth's modulation matrix: a source moving a target by
/// `amount` at full modulation, in the target's unit.
#[derive(Clone, Debug, PartialEq)]
pub struct Modulation {
    pub source: String,
    pub target: String,
    pub amount: f64,
}

impl Modulation {
    const FIELDS: &'static [&'static str] = &["source", "target", "amount"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Modulation> {
        let f = Fields::of(ctx, x, "Modulation", Self::FIELDS)?;
        let before = ctx.count();
        let source = f.req(ctx, "source", v::string);
        let target = f.req(ctx, "target", v::string);
        let amount = f.req(ctx, "amount", |c, x| v::float(c, x, Bounds::ge_le("-100", "100")));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Modulation {
            source: source?,
            target: target?,
            amount: amount?,
        })
    }

    pub fn dump(&self, _saved: bool) -> Value {
        let mut o = Out::new(false);
        o.req("source", Value::str(&self.source));
        o.req("target", Value::str(&self.target));
        o.req("amount", Value::Float(self.amount));
        o.done()
    }
}

/// How the Synth places its voices across the stereo field, by
/// `width_percent`: each note on the other side from the last, by its
/// pitch, or at a random place from the seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WidthMode {
    Alternate,
    Pitch,
    Random,
}

impl WidthMode {
    pub const NAMES: [&'static str; 3] = ["alternate", "pitch", "random"];

    pub fn as_str(self) -> &'static str {
        match self {
            WidthMode::Alternate => "alternate",
            WidthMode::Pitch => "pitch",
            WidthMode::Random => "random",
        }
    }

    fn from_str(s: &str) -> WidthMode {
        match s {
            "pitch" => WidthMode::Pitch,
            "random" => WidthMode::Random,
            _ => WidthMode::Alternate,
        }
    }
}

/// The Synth: a polyphonic synthesizer whose whole sound is this mapping.
#[derive(Clone, Debug, PartialEq)]
pub struct Synth {
    /// The name of the patch the sound came from; nothing reads it.
    pub patch: Option<String>,
    /// 1 to 16; 1 is monophonic.
    pub voices: i64,
    /// How long a new note slides from the last one's pitch.
    pub glide_ms: f64,
    /// How much a note's velocity moves its level: 100 is linear, 0 none.
    pub velocity_percent: f64,
    /// How far the voices are placed across the stereo field, either side
    /// of each oscillator's pan: 0 keeps every note where its oscillators
    /// are, 100 reaches the edges.
    pub width_percent: f64,
    /// Where each note lands within the width.
    pub width_mode: WidthMode,
    /// What random phases and the `random` source come from.
    pub seed: i64,
    /// One to four, by an ID the patch chooses, played in this order.
    pub oscillators: IndexMap<String, Oscillator>,
    pub filter: SynthFilter,
    /// `amp` is always there and shapes the level; up to three more.
    pub envelopes: IndexMap<String, SynthEnvelope>,
    /// Up to four.
    pub lfos: IndexMap<String, Lfo>,
    pub modulation: Vec<Modulation>,
    /// Up to eight knobs, 0 to 100, that do nothing but through the matrix.
    pub macros: IndexMap<String, f64>,
    /// The patch's own effects, the song's effect kinds in a chain run on
    /// the sum of the voices before the track's inserts. A compressor here
    /// has no sidechain.
    pub effects: Vec<Effect>,
}

impl Synth {
    const FIELDS: &'static [&'static str] = &[
        "patch", "voices", "glide_ms", "velocity_percent", "width_percent", "width_mode", "seed", "oscillators", "filter",
        "envelopes", "lfos", "modulation", "macros", "effects",
    ];
    pub const MAX_OSCILLATORS: usize = 4;
    pub const MAX_ENVELOPES: usize = 4;
    pub const MAX_LFOS: usize = 4;
    pub const MAX_MACROS: usize = 8;
    pub const MAX_MODULATION: usize = 32;

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Synth> {
        let f = Fields::of(ctx, x, "Synth", Self::FIELDS)?;
        let before = ctx.count();
        let patch = f.opt(ctx, "patch", None, opt_string);
        let voices = f.opt(ctx, "voices", 8, |c, x| v::int(c, x, Bounds::ge_le("1", "16")));
        let glide_ms = f.opt(ctx, "glide_ms", 0.0, |c, x| v::float(c, x, Bounds::ge_le("0", "5000")));
        let velocity_percent = f.opt(ctx, "velocity_percent", 100.0, |c, x| v::float(c, x, Bounds::ge_le("0", "100")));
        let width_percent = f.opt(ctx, "width_percent", 0.0, |c, x| v::float(c, x, Bounds::ge_le("0", "100")));
        let width_mode = f.opt(ctx, "width_mode", WidthMode::Alternate, |c, x| {
            v::literal_str(c, x, &WidthMode::NAMES).map(WidthMode::from_str)
        });
        let seed = f.opt(ctx, "seed", 0, |c, x| v::int(c, x, Bounds::ge_le("0", "4294967295")));
        let oscillators = f.req(ctx, "oscillators", |c, x| id_dict(c, x, Oscillator::validate));
        let filter = f.opt(ctx, "filter", SynthFilter::default(), SynthFilter::validate);
        let envelopes = f.opt(ctx, "envelopes", IndexMap::new(), |c, x| id_dict(c, x, SynthEnvelope::validate));
        let lfos = f.opt(ctx, "lfos", IndexMap::new(), |c, x| id_dict(c, x, Lfo::validate));
        let modulation = f.opt(ctx, "modulation", Vec::new(), |c, x| {
            v::list(c, x, 0, Some(Self::MAX_MODULATION), Modulation::validate)
        });
        let macros = f.opt(ctx, "macros", IndexMap::new(), |c, x| {
            id_dict(c, x, |c, x| v::float(c, x, Bounds::ge_le("0", "100")))
        });
        let effects = effects_field(ctx, &f);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        let mut synth = Synth {
            patch: patch?,
            voices: voices?,
            glide_ms: glide_ms?,
            velocity_percent: velocity_percent?,
            width_percent: width_percent?,
            width_mode: width_mode?,
            seed: seed?,
            oscillators: oscillators?,
            filter: filter?,
            envelopes: envelopes?,
            lfos: lfos?,
            modulation: modulation?,
            macros: macros?,
            effects: effects?,
        };
        // The amp envelope is always there, first.
        if !synth.envelopes.contains_key("amp") {
            synth.envelopes.shift_insert(0, "amp".into(), SynthEnvelope::default());
        }
        let check = || -> Result<(), String> {
            let count = |what: &str, n: usize, least: usize, most: usize| {
                if n < least || n > most {
                    return Err(format!("A synth has {least} to {most} {what}, not {n}"));
                }
                Ok(())
            };
            count("oscillators", synth.oscillators.len(), 1, Self::MAX_OSCILLATORS)?;
            count("envelopes", synth.envelopes.len(), 1, Self::MAX_ENVELOPES)?;
            count("LFOs", synth.lfos.len(), 0, Self::MAX_LFOS)?;
            count("macros", synth.macros.len(), 0, Self::MAX_MACROS)?;
            if let Some(e) = synth.effects.iter().find(|e| e.sidechain().is_some()) {
                return Err(format!(
                    "A compressor inside a patch has no sidechain ({}): the patch's effects hear only the synth",
                    e.sidechain().unwrap_or_default()
                ));
            }
            let ids: Vec<&str> = synth.effects.iter().filter_map(Effect::id).filter(|i| !i.is_empty()).collect();
            if ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len() {
                return Err("The patch's effect IDs must be unique".into());
            }
            rules::synth_references(&synth)
        };
        if let Err(msg) = check() {
            ctx.value_error(msg);
            return None;
        }
        Some(synth)
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.opt("patch", opt_str(&self.patch));
        o.int("voices", self.voices, 8);
        o.float("glide_ms", self.glide_ms, 0.0);
        o.float("velocity_percent", self.velocity_percent, 100.0);
        o.float("width_percent", self.width_percent, 0.0);
        o.str("width_mode", self.width_mode.as_str(), "alternate");
        o.int("seed", self.seed, 0);
        o.req("oscillators", str_map(&self.oscillators, saved, Oscillator::dump));
        let filter = self.filter.dump(saved);
        if !(saved && self.filter == SynthFilter::default()) {
            o.req("filter", filter);
        }
        // The plain amp envelope alone is what a patch has when it says nothing.
        let plain_amp = self.envelopes.len() == 1 && self.envelopes.get("amp") == Some(&SynthEnvelope::default());
        if !(saved && plain_amp) {
            o.coll("envelopes", str_map(&self.envelopes, saved, SynthEnvelope::dump));
        }
        o.coll("lfos", str_map(&self.lfos, saved, Lfo::dump));
        o.coll("modulation", list(&self.modulation, saved, Modulation::dump));
        o.coll(
            "macros",
            Value::Dict(self.macros.iter().map(|(k, x)| (Key::str(k), Value::Float(*x))).collect()),
        );
        o.coll("effects", list(&self.effects, saved, Effect::dump));
        o.done()
    }
}

impl Default for Synth {
    /// The plain saw: one oscillator, the filter open, the plain envelope.
    fn default() -> Self {
        let mut oscillators = IndexMap::new();
        oscillators.insert("a".to_string(), Oscillator::default());
        let mut envelopes = IndexMap::new();
        envelopes.insert("amp".to_string(), SynthEnvelope::default());
        Synth {
            patch: None,
            voices: 8,
            glide_ms: 0.0,
            velocity_percent: 100.0,
            width_percent: 0.0,
            width_mode: WidthMode::Alternate,
            seed: 0,
            oscillators,
            filter: SynthFilter::default(),
            envelopes,
            lfos: IndexMap::new(),
            modulation: Vec::new(),
            macros: IndexMap::new(),
            effects: Vec::new(),
        }
    }
}

/// `dict[ID, T]`: a mapping whose keys are IDs, as a synth's parts are.
fn id_dict<T>(ctx: &mut Ctx, x: &Value, item: impl FnMut(&mut Ctx, &Value) -> Option<T>) -> Option<IndexMap<String, T>> {
    let out = v::str_dict(ctx, x, item)?;
    if let Some(bad) = out.keys().find(|k| !ID_RE.is_match(k)) {
        ctx.value_error(format!("{} is not an ID; use letters, digits, - and _, starting with a letter", crate::pyfmt::str_repr(bad)));
        return None;
    }
    Some(out)
}

/// What a MIDI track's notes play: a sampler, or the Synth.
#[derive(Clone, Debug)]
pub enum Instrument {
    Sampler(Sampler),
    Synth(Synth),
}

impl Instrument {
    const FIELDS: &'static [&'static str] = &["sampler", "synth"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Instrument> {
        let f = Fields::of(ctx, x, "Instrument", Self::FIELDS)?;
        let before = ctx.count();
        let sampler = f.opt(ctx, "sampler", None, |c, x| v::optional(c, x, Sampler::validate));
        let synth = f.opt(ctx, "synth", None, |c, x| v::optional(c, x, Synth::validate));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        match (sampler?, synth?) {
            (Some(s), None) => Some(Instrument::Sampler(s)),
            (None, Some(s)) => Some(Instrument::Synth(s)),
            (None, None) => {
                ctx.value_error("An instrument is a sampler or a synth");
                None
            }
            (Some(_), Some(_)) => {
                ctx.value_error("An instrument is a sampler or a synth, not both");
                None
            }
        }
    }

    /// An instrument from a value on its own, as validation reads it: for
    /// what checks an instrument before the song around it is validated.
    pub fn parse(x: &Value) -> Result<Instrument, crate::validate::ValidationError> {
        let mut ctx = Ctx::default();
        match Self::validate(&mut ctx, x) {
            Some(i) if ctx.errors.is_empty() => Ok(i),
            _ => Err(crate::validate::ValidationError { errors: ctx.errors }),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Instrument::Sampler(_) => "sampler",
            Instrument::Synth(_) => "synth",
        }
    }

    pub fn sampler(&self) -> Option<&Sampler> {
        match self {
            Instrument::Sampler(s) => Some(s),
            Instrument::Synth(_) => None,
        }
    }

    pub fn synth(&self) -> Option<&Synth> {
        match self {
            Instrument::Synth(s) => Some(s),
            Instrument::Sampler(_) => None,
        }
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        match self {
            Instrument::Sampler(s) => {
                o.req("sampler", s.dump(saved));
                o.opt("synth", None);
            }
            Instrument::Synth(s) => {
                o.opt("sampler", None);
                o.req("synth", s.dump(saved));
            }
        }
        o.done()
    }
}

/// What a MIDI track has in place of pads, pattern clips and audio clips.
#[derive(Clone, Debug, Default)]
pub struct Midi {
    /// None plays nothing; the notes are kept.
    pub instrument: Option<Instrument>,
    pub clips: Vec<NoteClip>,
}

impl Midi {
    pub fn sampler(&self) -> Option<&Sampler> {
        self.instrument.as_ref().and_then(Instrument::sampler)
    }

    pub fn synth(&self) -> Option<&Synth> {
        self.instrument.as_ref().and_then(Instrument::synth)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    Highpass,
    Lowpass,
}

#[derive(Clone, Debug)]
pub struct Filter {
    pub id: Option<String>,
    pub mode: FilterMode,
    pub cutoff_hz: f64,
    pub slope_db_per_octave: i64,
    pub bypass: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandShape {
    Bell,
    LowShelf,
    HighShelf,
}

impl BandShape {
    pub fn as_str(self) -> &'static str {
        match self {
            BandShape::Bell => "bell",
            BandShape::LowShelf => "low_shelf",
            BandShape::HighShelf => "high_shelf",
        }
    }
}

#[derive(Clone, Debug)]
pub struct EqBand {
    pub shape: BandShape,
    pub freq_hz: f64,
    pub gain_db: f64,
    pub q: f64,
}

impl EqBand {
    const FIELDS: &'static [&'static str] = &["shape", "freq_hz", "gain_db", "q"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<EqBand> {
        let f = Fields::of(ctx, x, "EqBand", Self::FIELDS)?;
        let before = ctx.count();
        let shape = f.req(ctx, "shape", |c, x| {
            v::literal_str(c, x, &["bell", "low_shelf", "high_shelf"]).map(|s| match s {
                "bell" => BandShape::Bell,
                "low_shelf" => BandShape::LowShelf,
                _ => BandShape::HighShelf,
            })
        });
        let freq_hz = f.req(ctx, "freq_hz", |c, x| v::float(c, x, Bounds::ge_le("20", "20000")));
        let gain_db = f.req(ctx, "gain_db", |c, x| v::float(c, x, Bounds::ge_le("-24", "24")));
        let q = f.opt(ctx, "q", 0.71, |c, x| v::float(c, x, Bounds::ge_le("0.1", "18")));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(EqBand {
            shape: shape?,
            freq_hz: freq_hz?,
            gain_db: gain_db?,
            q: q?,
        })
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("shape", Value::str(self.shape.as_str()));
        o.req("freq_hz", Value::Float(self.freq_hz));
        o.req("gain_db", Value::Float(self.gain_db));
        o.float("q", self.q, 0.71);
        o.done()
    }
}

#[derive(Clone, Debug)]
pub struct Eq {
    pub id: Option<String>,
    pub bands: Vec<EqBand>,
    pub bypass: bool,
}

#[derive(Clone, Debug)]
pub struct Compressor {
    pub id: Option<String>,
    pub threshold_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub knee_db: f64,
    pub makeup_db: f64,
    pub sidechain: Option<String>,
    pub bypass: bool,
}

#[derive(Clone, Debug)]
pub struct Limiter {
    pub id: Option<String>,
    pub ceiling_db: f64,
    pub release_ms: f64,
    pub lookahead_ms: f64,
    pub bypass: bool,
}

#[derive(Clone, Debug)]
pub struct Delay {
    pub id: Option<String>,
    pub time_beats: Beat,
    pub feedback_percent: f64,
    pub lowcut_hz: Option<f64>,
    pub highcut_hz: Option<f64>,
    pub ping_pong: bool,
    pub mix_percent: f64,
    pub bypass: bool,
}

impl Delay {
    pub fn time_exact(&self) -> BigRational {
        exact(&self.time_beats)
    }
}

#[derive(Clone, Debug)]
pub struct Reverb {
    pub id: Option<String>,
    pub decay_seconds: f64,
    pub predelay_ms: f64,
    pub damping_hz: f64,
    pub lowcut_hz: f64,
    pub width_percent: f64,
    pub mix_percent: f64,
    pub seed: i64,
    pub bypass: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaturationMode {
    /// A soft clip, `tanh`: odd harmonics.
    Soft,
    /// A hard clip at full scale.
    Hard,
    /// An asymmetric curve: even harmonics too, as a valve's.
    Tube,
}

impl SaturationMode {
    pub const NAMES: [&'static str; 3] = ["soft", "hard", "tube"];

    pub fn as_str(self) -> &'static str {
        match self {
            SaturationMode::Soft => "soft",
            SaturationMode::Hard => "hard",
            SaturationMode::Tube => "tube",
        }
    }
}

/// A stereo chorus: each channel through a delay of `delay_ms` moved by
/// `depth_ms` either side at `rate_hz`, the right channel a quarter cycle
/// behind the left, mixed under the dry signal.
#[derive(Clone, Debug)]
pub struct Chorus {
    pub id: Option<String>,
    pub rate_hz: f64,
    pub depth_ms: f64,
    pub delay_ms: f64,
    pub mix_percent: f64,
    pub bypass: bool,
}

/// Saturation: the signal driven by `drive_db` into a curve, trimmed by
/// `output_db` and mixed under the dry signal.
#[derive(Clone, Debug)]
pub struct Saturation {
    pub id: Option<String>,
    pub mode: SaturationMode,
    pub drive_db: f64,
    pub output_db: f64,
    pub mix_percent: f64,
    pub bypass: bool,
}

#[derive(Clone, Debug)]
pub enum Effect {
    Filter(Filter),
    Eq(Eq),
    Compressor(Compressor),
    Limiter(Limiter),
    Delay(Delay),
    Reverb(Reverb),
    Chorus(Chorus),
    Saturation(Saturation),
}

pub const EFFECT_TYPES: [&str; 8] = ["filter", "eq", "compressor", "limiter", "delay", "reverb", "chorus", "saturation"];

impl PartialEq for Effect {
    /// Two effects are equal when the song would write them the same.
    fn eq(&self, other: &Effect) -> bool {
        crate::value::py_eq(&self.dump(false), &other.dump(false))
    }
}

impl Effect {
    pub fn kind(&self) -> &'static str {
        match self {
            Effect::Filter(_) => "filter",
            Effect::Eq(_) => "eq",
            Effect::Compressor(_) => "compressor",
            Effect::Limiter(_) => "limiter",
            Effect::Delay(_) => "delay",
            Effect::Reverb(_) => "reverb",
            Effect::Chorus(_) => "chorus",
            Effect::Saturation(_) => "saturation",
        }
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            Effect::Filter(e) => e.id.as_deref(),
            Effect::Eq(e) => e.id.as_deref(),
            Effect::Compressor(e) => e.id.as_deref(),
            Effect::Limiter(e) => e.id.as_deref(),
            Effect::Delay(e) => e.id.as_deref(),
            Effect::Reverb(e) => e.id.as_deref(),
            Effect::Chorus(e) => e.id.as_deref(),
            Effect::Saturation(e) => e.id.as_deref(),
        }
    }

    pub fn bypass(&self) -> bool {
        match self {
            Effect::Filter(e) => e.bypass,
            Effect::Eq(e) => e.bypass,
            Effect::Compressor(e) => e.bypass,
            Effect::Limiter(e) => e.bypass,
            Effect::Delay(e) => e.bypass,
            Effect::Reverb(e) => e.bypass,
            Effect::Chorus(e) => e.bypass,
            Effect::Saturation(e) => e.bypass,
        }
    }

    /// The sidechain track of a compressor, if it names one.
    pub fn sidechain(&self) -> Option<&str> {
        match self {
            Effect::Compressor(c) => c.sidechain.as_deref().filter(|s| !s.is_empty()),
            _ => None,
        }
    }

    /// The discriminated union on `type`.
    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Effect> {
        let Value::Dict(d) = x else {
            ctx.error(
                "model_attributes_type",
                "Input should be a valid dictionary or object to extract fields from",
            );
            return None;
        };
        let Some(tag) = d.get(&Key::str("type")) else {
            ctx.error(
                "union_tag_not_found",
                "Unable to extract tag using discriminator 'type'",
            );
            return None;
        };
        let kind = match tag {
            Value::Str(s) if EFFECT_TYPES.contains(&s.as_str()) => s.clone(),
            other => {
                ctx.error(
                    "union_tag_invalid",
                    format!(
                        "Input tag '{}' found using 'type' does not match any of the expected tags: 'filter', 'eq', 'compressor', 'limiter', 'delay', 'reverb', 'chorus', 'saturation'",
                        crate::value::py_str(other)
                    ),
                );
                return None;
            }
        };
        ctx.at(Loc::Key(kind.clone()), |c| Self::member(c, x, &kind))
    }

    fn member(ctx: &mut Ctx, x: &Value, kind: &str) -> Option<Effect> {
        let before = ctx.count();
        let fields: &'static [&'static str] = match kind {
            "filter" => &["type", "id", "mode", "cutoff_hz", "slope_db_per_octave", "bypass"],
            "eq" => &["type", "id", "bands", "bypass"],
            "compressor" => &[
                "type", "id", "threshold_db", "ratio", "attack_ms", "release_ms", "knee_db",
                "makeup_db", "sidechain", "bypass",
            ],
            "limiter" => &["type", "id", "ceiling_db", "release_ms", "lookahead_ms", "bypass"],
            "delay" => &[
                "type", "id", "time_beats", "feedback_percent", "lowcut_hz", "highcut_hz",
                "ping_pong", "mix_percent", "bypass",
            ],
            "chorus" => &["type", "id", "rate_hz", "depth_ms", "delay_ms", "mix_percent", "bypass"],
            "saturation" => &["type", "id", "mode", "drive_db", "output_db", "mix_percent", "bypass"],
            _ => &[
                "type", "id", "decay_seconds", "predelay_ms", "damping_hz", "lowcut_hz",
                "width_percent", "mix_percent", "seed", "bypass",
            ],
        };
        let f = Fields::member(x, fields);
        let id = f.opt(ctx, "id", None, opt_id);
        let float = |ctx: &mut Ctx, name: &str, default: f64, b: Bounds| {
            f.opt(ctx, name, default, |c, x| v::float(c, x, b))
        };
        let effect = match kind {
            "filter" => {
                let mode = f.req(ctx, "mode", |c, x| {
                    v::literal_str(c, x, &["highpass", "lowpass"]).map(|m| {
                        if m == "highpass" {
                            FilterMode::Highpass
                        } else {
                            FilterMode::Lowpass
                        }
                    })
                });
                let cutoff_hz = f.req(ctx, "cutoff_hz", |c, x| v::float(c, x, Bounds::ge_le("10", "20000")));
                let slope = f.opt(ctx, "slope_db_per_octave", 12, |c, x| v::literal_int(c, x, &[12, 24, 36, 48]));
                let bypass = f.opt(ctx, "bypass", false, v::boolean);
                (|| {
                    Some(Effect::Filter(Filter {
                        id: id?,
                        mode: mode?,
                        cutoff_hz: cutoff_hz?,
                        slope_db_per_octave: slope?,
                        bypass: bypass?,
                    }))
                })()
            }
            "eq" => {
                let bands = f.req(ctx, "bands", |c, x| v::list(c, x, 1, Some(16), EqBand::validate));
                let bypass = f.opt(ctx, "bypass", false, v::boolean);
                (|| {
                    Some(Effect::Eq(Eq {
                        id: id?,
                        bands: bands?,
                        bypass: bypass?,
                    }))
                })()
            }
            "compressor" => {
                let threshold_db = f.req(ctx, "threshold_db", |c, x| v::float(c, x, Bounds::ge_le("-60", "0")));
                let ratio = float(ctx, "ratio", 4.0, Bounds::ge_le("1", "20"));
                let attack_ms = float(ctx, "attack_ms", 10.0, Bounds::ge_le("0", "500"));
                let release_ms = float(ctx, "release_ms", 120.0, Bounds::ge_le("1", "5000"));
                let knee_db = float(ctx, "knee_db", 6.0, Bounds::ge_le("0", "24"));
                let makeup_db = float(ctx, "makeup_db", 0.0, Bounds::ge_le("-24", "24"));
                let sidechain = f.opt(ctx, "sidechain", None, opt_string);
                let bypass = f.opt(ctx, "bypass", false, v::boolean);
                (|| {
                    Some(Effect::Compressor(Compressor {
                        id: id?,
                        threshold_db: threshold_db?,
                        ratio: ratio?,
                        attack_ms: attack_ms?,
                        release_ms: release_ms?,
                        knee_db: knee_db?,
                        makeup_db: makeup_db?,
                        sidechain: sidechain?,
                        bypass: bypass?,
                    }))
                })()
            }
            "limiter" => {
                let ceiling_db = float(ctx, "ceiling_db", -1.0, Bounds::ge_le("-24", "-0.1"));
                let release_ms = float(ctx, "release_ms", 60.0, Bounds::ge_le("1", "2000"));
                let lookahead_ms = float(ctx, "lookahead_ms", 3.0, Bounds::ge_le("0.5", "20"));
                let bypass = f.opt(ctx, "bypass", false, v::boolean);
                (|| {
                    Some(Effect::Limiter(Limiter {
                        id: id?,
                        ceiling_db: ceiling_db?,
                        release_ms: release_ms?,
                        lookahead_ms: lookahead_ms?,
                        bypass: bypass?,
                    }))
                })()
            }
            "delay" => {
                let time_beats = f.req(ctx, "time_beats", v::beat);
                let feedback_percent = float(ctx, "feedback_percent", 35.0, Bounds::ge_le("0", "95"));
                let cut = |ctx: &mut Ctx, name: &str| {
                    f.opt(ctx, name, None, |c, x| {
                        v::optional(c, x, |c, x| v::float(c, x, Bounds::ge_le("10", "20000")))
                    })
                };
                let lowcut_hz = cut(ctx, "lowcut_hz");
                let highcut_hz = cut(ctx, "highcut_hz");
                let ping_pong = f.opt(ctx, "ping_pong", false, v::boolean);
                let mix_percent = float(ctx, "mix_percent", 100.0, Bounds::ge_le("0", "100"));
                let bypass = f.opt(ctx, "bypass", false, v::boolean);
                (|| {
                    Some(Effect::Delay(Delay {
                        id: id?,
                        time_beats: time_beats?,
                        feedback_percent: feedback_percent?,
                        lowcut_hz: lowcut_hz?,
                        highcut_hz: highcut_hz?,
                        ping_pong: ping_pong?,
                        mix_percent: mix_percent?,
                        bypass: bypass?,
                    }))
                })()
            }
            "chorus" => {
                let rate_hz = float(ctx, "rate_hz", 0.8, Bounds::ge_le("0.05", "10"));
                let depth_ms = float(ctx, "depth_ms", 3.0, Bounds::ge_le("0", "20"));
                let delay_ms = float(ctx, "delay_ms", 12.0, Bounds::ge_le("1", "40"));
                let mix_percent = float(ctx, "mix_percent", 50.0, Bounds::ge_le("0", "100"));
                let bypass = f.opt(ctx, "bypass", false, v::boolean);
                (|| {
                    Some(Effect::Chorus(Chorus {
                        id: id?,
                        rate_hz: rate_hz?,
                        depth_ms: depth_ms?,
                        delay_ms: delay_ms?,
                        mix_percent: mix_percent?,
                        bypass: bypass?,
                    }))
                })()
            }
            "saturation" => {
                let mode = f.opt(ctx, "mode", SaturationMode::Soft, |c, x| {
                    v::literal_str(c, x, &SaturationMode::NAMES).map(|m| match m {
                        "hard" => SaturationMode::Hard,
                        "tube" => SaturationMode::Tube,
                        _ => SaturationMode::Soft,
                    })
                });
                let drive_db = float(ctx, "drive_db", 12.0, Bounds::ge_le("0", "36"));
                let output_db = float(ctx, "output_db", 0.0, Bounds::ge_le("-24", "24"));
                let mix_percent = float(ctx, "mix_percent", 100.0, Bounds::ge_le("0", "100"));
                let bypass = f.opt(ctx, "bypass", false, v::boolean);
                (|| {
                    Some(Effect::Saturation(Saturation {
                        id: id?,
                        mode: mode?,
                        drive_db: drive_db?,
                        output_db: output_db?,
                        mix_percent: mix_percent?,
                        bypass: bypass?,
                    }))
                })()
            }
            _ => {
                let decay_seconds = float(ctx, "decay_seconds", 1.5, Bounds::ge_le("0.1", "12"));
                let predelay_ms = float(ctx, "predelay_ms", 10.0, Bounds::ge_le("0", "250"));
                let damping_hz = float(ctx, "damping_hz", 6000.0, Bounds::ge_le("500", "20000"));
                let lowcut_hz = float(ctx, "lowcut_hz", 100.0, Bounds::ge_le("20", "2000"));
                let width_percent = float(ctx, "width_percent", 100.0, Bounds::ge_le("0", "100"));
                let mix_percent = float(ctx, "mix_percent", 100.0, Bounds::ge_le("0", "100"));
                let seed = f.opt(ctx, "seed", 0, |c, x| v::int(c, x, Bounds::ge_le("0", "4294967295")));
                let bypass = f.opt(ctx, "bypass", false, v::boolean);
                (|| {
                    Some(Effect::Reverb(Reverb {
                        id: id?,
                        decay_seconds: decay_seconds?,
                        predelay_ms: predelay_ms?,
                        damping_hz: damping_hz?,
                        lowcut_hz: lowcut_hz?,
                        width_percent: width_percent?,
                        mix_percent: mix_percent?,
                        seed: seed?,
                        bypass: bypass?,
                    }))
                })()
            }
        };
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        let effect = effect?;
        if let Effect::Delay(d) = &effect {
            let time = match exact_beat(&d.time_beats) {
                Ok(t) => t,
                Err(msg) => {
                    ctx.value_error(msg);
                    return None;
                }
            };
            if !(time.is_positive() && time <= BigRational::from_integer(16.into())) {
                ctx.value_error("delay time_beats must be greater than 0 and at most 16");
                return None;
            }
            if let (Some(low), Some(high)) = (d.lowcut_hz, d.highcut_hz) {
                if low != 0.0 && high != 0.0 && low >= high {
                    ctx.value_error("delay lowcut_hz must be below highcut_hz");
                    return None;
                }
            }
        }
        Some(effect)
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("type", Value::str(self.kind()));
        o.opt("id", self.id().map(Value::str));
        match self {
            Effect::Filter(e) => {
                o.req(
                    "mode",
                    Value::str(match e.mode {
                        FilterMode::Highpass => "highpass",
                        FilterMode::Lowpass => "lowpass",
                    }),
                );
                o.req("cutoff_hz", Value::Float(e.cutoff_hz));
                o.int("slope_db_per_octave", e.slope_db_per_octave, 12);
            }
            Effect::Eq(e) => o.req("bands", list(&e.bands, saved, EqBand::dump)),
            Effect::Compressor(e) => {
                o.req("threshold_db", Value::Float(e.threshold_db));
                o.float("ratio", e.ratio, 4.0);
                o.float("attack_ms", e.attack_ms, 10.0);
                o.float("release_ms", e.release_ms, 120.0);
                o.float("knee_db", e.knee_db, 6.0);
                o.float("makeup_db", e.makeup_db, 0.0);
                o.opt("sidechain", opt_str(&e.sidechain));
            }
            Effect::Limiter(e) => {
                o.float("ceiling_db", e.ceiling_db, -1.0);
                o.float("release_ms", e.release_ms, 60.0);
                o.float("lookahead_ms", e.lookahead_ms, 3.0);
            }
            Effect::Delay(e) => {
                o.req("time_beats", e.time_beats.to_value());
                o.float("feedback_percent", e.feedback_percent, 35.0);
                o.opt("lowcut_hz", e.lowcut_hz.map(Value::Float));
                o.opt("highcut_hz", e.highcut_hz.map(Value::Float));
                o.bool("ping_pong", e.ping_pong, false);
                o.float("mix_percent", e.mix_percent, 100.0);
            }
            Effect::Reverb(e) => {
                o.float("decay_seconds", e.decay_seconds, 1.5);
                o.float("predelay_ms", e.predelay_ms, 10.0);
                o.float("damping_hz", e.damping_hz, 6000.0);
                o.float("lowcut_hz", e.lowcut_hz, 100.0);
                o.float("width_percent", e.width_percent, 100.0);
                o.float("mix_percent", e.mix_percent, 100.0);
                o.int("seed", e.seed, 0);
            }
            Effect::Chorus(e) => {
                o.float("rate_hz", e.rate_hz, 0.8);
                o.float("depth_ms", e.depth_ms, 3.0);
                o.float("delay_ms", e.delay_ms, 12.0);
                o.float("mix_percent", e.mix_percent, 50.0);
            }
            Effect::Saturation(e) => {
                o.str("mode", e.mode.as_str(), "soft");
                o.float("drive_db", e.drive_db, 12.0);
                o.float("output_db", e.output_db, 0.0);
                o.float("mix_percent", e.mix_percent, 100.0);
            }
        }
        o.bool("bypass", self.bypass(), false);
        o.done()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    Linear,
    Hold,
}

#[derive(Clone, Debug)]
pub struct Point {
    pub at: Beat,
    pub value: f64,
    pub curve: Curve,
    /// How a linear segment bends: above zero it starts slowly and finishes
    /// fast, below zero the other way, and at zero it is straight.
    pub shape: f64,
}

impl Point {
    const FIELDS: &'static [&'static str] = &["at", "value", "curve", "shape"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Point> {
        let f = Fields::of(ctx, x, "Point", Self::FIELDS)?;
        let before = ctx.count();
        let at = f.req(ctx, "at", beat_field);
        let value = f.req(ctx, "value", |c, x| v::float(c, x, Bounds::NONE));
        let curve = f.opt(ctx, "curve", Curve::Linear, |c, x| {
            v::literal_str(c, x, &["linear", "hold"]).map(|s| {
                if s == "hold" {
                    Curve::Hold
                } else {
                    Curve::Linear
                }
            })
        });
        let shape = f.opt(ctx, "shape", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-1", "1")));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Point {
            at: at?,
            value: value?,
            curve: curve?,
            shape: shape?,
        })
    }

    pub fn at_exact(&self) -> BigRational {
        exact(&self.at)
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("at", self.at.to_value());
        o.req("value", Value::Float(self.value));
        o.str(
            "curve",
            match self.curve {
                Curve::Linear => "linear",
                Curve::Hold => "hold",
            },
            "linear",
        );
        o.float("shape", self.shape, 0.0);
        o.done()
    }
}

#[derive(Clone, Debug)]
pub struct Lane {
    pub param: String,
    pub points: Vec<Point>,
}

impl Lane {
    const FIELDS: &'static [&'static str] = &["param", "points"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Lane> {
        let f = Fields::of(ctx, x, "Lane", Self::FIELDS)?;
        let before = ctx.count();
        let param = f.req(ctx, "param", v::string);
        let points = f.req(ctx, "points", |c, x| v::list(c, x, 1, Some(10000), Point::validate));
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        let lane = Lane {
            param: param?,
            points: points?,
        };
        let times: Vec<BigRational> = lane.points.iter().map(Point::at_exact).collect();
        if times.windows(2).any(|w| w[1] < w[0]) {
            ctx.value_error(format!("{}: automation points must be in time order", lane.param));
            return None;
        }
        if times.windows(3).any(|w| w[0] == w[2]) {
            ctx.value_error(format!("{}: at most two points may share a position", lane.param));
            return None;
        }
        Some(lane)
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("param", Value::str(&self.param));
        o.req("points", list(&self.points, saved, Point::dump));
        o.done()
    }
}

#[derive(Clone, Debug)]
pub struct Send {
    pub to: String,
    pub gain_db: f64,
    pub pre_fader: bool,
}

impl Send {
    const FIELDS: &'static [&'static str] = &["to", "gain_db", "pre_fader"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Send> {
        let f = Fields::of(ctx, x, "Send", Self::FIELDS)?;
        let before = ctx.count();
        let to = f.req(ctx, "to", v::string);
        let gain_db = f.opt(ctx, "gain_db", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-96", "12")));
        let pre_fader = f.opt(ctx, "pre_fader", false, v::boolean);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Send {
            to: to?,
            gain_db: gain_db?,
            pre_fader: pre_fader?,
        })
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("to", Value::str(&self.to));
        o.float("gain_db", self.gain_db, 0.0);
        o.bool("pre_fader", self.pre_fader, false);
        o.done()
    }
}

fn effects_field(ctx: &mut Ctx, f: &Fields) -> Option<Vec<Effect>> {
    f.opt(ctx, "effects", Vec::new(), |c, x| v::list(c, x, 0, Some(32), Effect::validate))
}

fn automation_field(ctx: &mut Ctx, f: &Fields) -> Option<Vec<Lane>> {
    f.opt(ctx, "automation", Vec::new(), |c, x| v::list(c, x, 0, Some(64), Lane::validate))
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: String,
    pub gain_db: f64,
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
    pub pads: IndexMap<String, Pad>,
    pub clips: Vec<Clip>,
    pub audio: Vec<AudioClip>,
    pub effects: Vec<Effect>,
    pub sends: Vec<Send>,
    pub automation: Vec<Lane>,
    /// A MIDI track's instrument and note clips. Such a track has no pads,
    /// pattern clips or audio clips of its own; it is written with
    /// `type: midi` and its note clips under `clips`.
    pub midi: Option<Midi>,
}

static NO_PADS: LazyLock<IndexMap<String, Pad>> = LazyLock::new(IndexMap::new);

impl Track {
    const FIELDS: &'static [&'static str] = &[
        "id", "gain_db", "pan", "mute", "solo", "pads", "clips", "audio", "effects", "sends", "automation",
    ];
    const MIDI_FIELDS: &'static [&'static str] = &[
        "id", "type", "gain_db", "pan", "mute", "solo", "instrument", "clips", "effects", "sends", "automation",
    ];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Track> {
        let midi = match x {
            Value::Dict(d) => match d.get(&Key::str("type")) {
                Some(tag) => {
                    ctx.key("type", |c| v::literal_str(c, tag, &["midi"]))?;
                    true
                }
                None => false,
            },
            _ => false,
        };
        let known = if midi { Self::MIDI_FIELDS } else { Self::FIELDS };
        let f = Fields::of(ctx, x, "Track", known)?;
        let before = ctx.count();
        let id = f.req(ctx, "id", id_field);
        let gain_db = f.opt(ctx, "gain_db", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-96", "24")));
        let pan = f.opt(ctx, "pan", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-1", "1")));
        let mute = f.opt(ctx, "mute", false, v::boolean);
        let solo = f.opt(ctx, "solo", false, v::boolean);
        let (pads, clips, audio, notes) = if midi {
            let instrument = f.opt(ctx, "instrument", None, |c, x| v::optional(c, x, Instrument::validate));
            let clips = f.opt(ctx, "clips", Vec::new(), |c, x| v::list(c, x, 0, None, NoteClip::validate));
            let notes = match (instrument, clips) {
                (Some(instrument), Some(clips)) => Some(Some(Midi { instrument, clips })),
                _ => None,
            };
            (Some(IndexMap::new()), Some(Vec::new()), Some(Vec::new()), notes)
        } else {
            let pads = f.req(ctx, "pads", |c, x| v::str_dict(c, x, Pad::validate));
            let clips = f.opt(ctx, "clips", Vec::new(), |c, x| v::list(c, x, 0, None, Clip::validate));
            let audio = f.opt(ctx, "audio", Vec::new(), |c, x| v::list(c, x, 0, None, AudioClip::validate));
            (pads, clips, audio, Some(None))
        };
        let effects = effects_field(ctx, &f);
        let sends = f.opt(ctx, "sends", Vec::new(), |c, x| v::list(c, x, 0, Some(16), Send::validate));
        let automation = automation_field(ctx, &f);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Track {
            id: id?,
            gain_db: gain_db?,
            pan: pan?,
            mute: mute?,
            solo: solo?,
            pads: pads?,
            clips: clips?,
            audio: audio?,
            effects: effects?,
            sends: sends?,
            automation: automation?,
            midi: notes?,
        })
    }

    pub fn sidechains(&self) -> Vec<&str> {
        self.effects.iter().filter_map(Effect::sidechain).collect()
    }

    /// The pads its sounds come from: a MIDI track's sampler's, or its own.
    pub fn sound_pads(&self) -> &IndexMap<String, Pad> {
        match &self.midi {
            Some(m) => m.sampler().map_or(&NO_PADS, |s| &s.pads),
            None => &self.pads,
        }
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("id", Value::str(&self.id));
        if self.midi.is_some() {
            o.req("type", Value::str("midi"));
        }
        o.float("gain_db", self.gain_db, 0.0);
        o.float("pan", self.pan, 0.0);
        o.bool("mute", self.mute, false);
        o.bool("solo", self.solo, false);
        match &self.midi {
            Some(m) => {
                o.opt("instrument", m.instrument.as_ref().map(|i| i.dump(saved)));
                o.coll("clips", list(&m.clips, saved, NoteClip::dump));
            }
            None => {
                o.req("pads", str_map(&self.pads, saved, Pad::dump));
                o.coll("clips", list(&self.clips, saved, Clip::dump));
                o.coll("audio", list(&self.audio, saved, AudioClip::dump));
            }
        }
        o.coll("effects", list(&self.effects, saved, Effect::dump));
        o.coll("sends", list(&self.sends, saved, Send::dump));
        o.coll("automation", list(&self.automation, saved, Lane::dump));
        o.done()
    }
}

#[derive(Clone, Debug)]
pub struct Return {
    pub id: String,
    pub gain_db: f64,
    pub pan: f64,
    pub mute: bool,
    pub effects: Vec<Effect>,
    pub automation: Vec<Lane>,
}

impl Return {
    const FIELDS: &'static [&'static str] = &["id", "gain_db", "pan", "mute", "effects", "automation"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Return> {
        let f = Fields::of(ctx, x, "Return", Self::FIELDS)?;
        let before = ctx.count();
        let id = f.req(ctx, "id", id_field);
        let gain_db = f.opt(ctx, "gain_db", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-96", "24")));
        let pan = f.opt(ctx, "pan", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-1", "1")));
        let mute = f.opt(ctx, "mute", false, v::boolean);
        let effects = effects_field(ctx, &f);
        let automation = automation_field(ctx, &f);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Return {
            id: id?,
            gain_db: gain_db?,
            pan: pan?,
            mute: mute?,
            effects: effects?,
            automation: automation?,
        })
    }

    pub fn sidechains(&self) -> Vec<&str> {
        self.effects.iter().filter_map(Effect::sidechain).collect()
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("id", Value::str(&self.id));
        o.float("gain_db", self.gain_db, 0.0);
        o.float("pan", self.pan, 0.0);
        o.bool("mute", self.mute, false);
        o.coll("effects", list(&self.effects, saved, Effect::dump));
        o.coll("automation", list(&self.automation, saved, Lane::dump));
        o.done()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Master {
    pub effects: Vec<Effect>,
    pub automation: Vec<Lane>,
}

impl Master {
    const FIELDS: &'static [&'static str] = &["effects", "automation"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Master> {
        let f = Fields::of(ctx, x, "Master", Self::FIELDS)?;
        let before = ctx.count();
        let effects = effects_field(ctx, &f);
        let automation = automation_field(ctx, &f);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Master {
            effects: effects?,
            automation: automation?,
        })
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.coll("effects", list(&self.effects, saved, Effect::dump));
        o.coll("automation", list(&self.automation, saved, Lane::dump));
        o.done()
    }
}

#[derive(Clone, Debug)]
pub struct Section {
    pub id: String,
    pub at: Beat,
    pub length_beats: Beat,
}

impl Section {
    const FIELDS: &'static [&'static str] = &["id", "at", "length_beats"];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Section> {
        let f = Fields::of(ctx, x, "Section", Self::FIELDS)?;
        let before = ctx.count();
        let id = f.req(ctx, "id", v::string);
        let at = f.req(ctx, "at", v::beat);
        let length_beats = f.req(ctx, "length_beats", v::beat);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        let s = Section {
            id: id?,
            at: at?,
            length_beats: length_beats?,
        };
        let check = || -> Result<(), String> {
            exact_beat(&s.at)?;
            if !exact_beat(&s.length_beats)?.is_positive() {
                return Err("Section length must be positive".into());
            }
            Ok(())
        };
        if let Err(msg) = check() {
            ctx.value_error(msg);
            return None;
        }
        Some(s)
    }

    pub fn at_exact(&self) -> BigRational {
        exact(&self.at)
    }

    pub fn length_exact(&self) -> BigRational {
        exact(&self.length_beats)
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.req("id", Value::str(&self.id));
        o.req("at", self.at.to_value());
        o.req("length_beats", self.length_beats.to_value());
        o.done()
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    pub title: String,
    pub tempo: f64,
    pub time_signature: String,
    pub sample_rate: i64,
    pub length_beats: Beat,
    pub master_gain_db: f64,
    pub end_fade_ms: f64,
    pub stretcher: Stretcher,
}

impl Session {
    const FIELDS: &'static [&'static str] = &[
        "title", "tempo", "time_signature", "sample_rate", "length_beats", "master_gain_db", "end_fade_ms",
        "stretcher",
    ];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Session> {
        let f = Fields::of(ctx, x, "Session", Self::FIELDS)?;
        let before = ctx.count();
        let title = f.opt(ctx, "title", "Untitled".to_string(), v::string);
        let tempo = f.opt(ctx, "tempo", 144.0, |c, x| v::float(c, x, Bounds::ge_le("20", "400")));
        let time_signature = f.opt(ctx, "time_signature", "4/4".to_string(), |c, x| match x {
            Value::Str(s) => match crate::Meter::parse(s) {
                Ok(m) => Some(m.text()),
                Err(msg) => {
                    c.error("value_error", msg);
                    None
                }
            },
            _ => {
                c.error("string_type", "Input should be a valid string");
                None
            }
        });
        let sample_rate = f.opt(ctx, "sample_rate", 48000, |c, x| v::literal_int(c, x, &[44100, 48000]));
        let length_beats = f.opt(ctx, "length_beats", Beat::int(16), |c, x| {
            let b = v::beat(c, x)?;
            match exact_beat(&b) {
                Ok(l) if l.is_positive() => Some(b),
                Ok(_) => {
                    c.value_error("Session length must be positive");
                    None
                }
                Err(msg) => {
                    c.value_error(msg);
                    None
                }
            }
        });
        let master_gain_db = f.opt(ctx, "master_gain_db", -6.0, |c, x| v::float(c, x, Bounds::ge_le("-96", "24")));
        let end_fade_ms = f.opt(ctx, "end_fade_ms", 20.0, |c, x| v::float(c, x, Bounds::ge_le("0", "10000")));
        let stretcher = f.opt(ctx, "stretcher", Stretcher::Signalsmith, |c, x| {
            v::literal_str(c, x, &["signalsmith", "rubberband"]).map(|m| {
                if m == "rubberband" {
                    Stretcher::Rubberband
                } else {
                    Stretcher::Signalsmith
                }
            })
        });
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Session {
            title: title?,
            tempo: tempo?,
            time_signature: time_signature?,
            sample_rate: sample_rate?,
            length_beats: length_beats?,
            master_gain_db: master_gain_db?,
            end_fade_ms: end_fade_ms?,
            stretcher: stretcher?,
        })
    }

    pub fn length_exact(&self) -> BigRational {
        exact(&self.length_beats)
    }

    /// The song's one time signature, read from the field validation held
    /// to the form `Meter` reads.
    pub fn meter(&self) -> crate::Meter {
        crate::Meter::parse(&self.time_signature).unwrap_or_default()
    }

    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.str("title", &self.title, "Untitled");
        o.float("tempo", self.tempo, 144.0);
        o.str("time_signature", &self.time_signature, "4/4");
        o.int("sample_rate", self.sample_rate, 48000);
        o.beat("length_beats", &self.length_beats, &Beat::int(16));
        o.float("master_gain_db", self.master_gain_db, -6.0);
        o.float("end_fade_ms", self.end_fade_ms, 20.0);
        o.str("stretcher", self.stretcher.as_str(), "signalsmith");
        o.done()
    }
}

#[derive(Clone, Debug)]
pub struct Project {
    pub session: Session,
    pub samples: IndexMap<String, Sample>,
    pub patterns: IndexMap<String, Pattern>,
    pub tracks: Vec<Track>,
    pub returns: Vec<Return>,
    pub sections: Vec<Section>,
    pub master: Master,
}

impl Project {
    const FIELDS: &'static [&'static str] = &[
        "schema_version", "session", "samples", "patterns", "tracks", "returns", "sections", "master",
    ];

    /// `Project.model_validate`: the whole tree, then the reference rules.
    pub fn validate(x: &Value) -> Result<Project, crate::validate::ValidationError> {
        let mut ctx = Ctx::default();
        let project = Self::fields(&mut ctx, x);
        match project {
            Some(p) if ctx.errors.is_empty() => match rules::references(&p) {
                Ok(()) => Ok(p),
                Err(msg) => {
                    ctx.value_error(msg);
                    Err(crate::validate::ValidationError { errors: ctx.errors })
                }
            },
            _ => Err(crate::validate::ValidationError { errors: ctx.errors }),
        }
    }

    fn fields(ctx: &mut Ctx, x: &Value) -> Option<Project> {
        let f = Fields::of(ctx, x, "Project", Self::FIELDS)?;
        let before = ctx.count();
        // The version a song is written with follows from what it holds.
        let schema = f.opt(ctx, "schema_version", 1, |c, x| v::literal_int(c, x, &[1, 2]));
        let session = f.req(ctx, "session", Session::validate);
        let samples = f.opt(ctx, "samples", IndexMap::new(), |c, x| v::str_dict(c, x, Sample::validate));
        let patterns = f.opt(ctx, "patterns", IndexMap::new(), |c, x| v::str_dict(c, x, Pattern::validate));
        let tracks = f.opt(ctx, "tracks", Vec::new(), |c, x| v::list(c, x, 0, None, Track::validate));
        let returns = f.opt(ctx, "returns", Vec::new(), |c, x| v::list(c, x, 0, None, Return::validate));
        let sections = f.opt(ctx, "sections", Vec::new(), |c, x| v::list(c, x, 0, None, Section::validate));
        let master = f.opt(ctx, "master", Master::default(), Master::validate);
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        schema?;
        let mut tracks = tracks?;
        let clips = || tracks.iter().filter_map(|t| t.midi.as_ref()).flat_map(|m| &m.clips);
        let mut seen = std::collections::HashSet::new();
        if let Some(c) = clips().find(|c| !c.id.is_empty() && !seen.insert(c.id.as_str())) {
            ctx.value_error(format!("Two note clips have the ID {}", c.id));
            return None;
        }
        assign_ids(tracks.iter_mut().filter_map(|t| t.midi.as_mut()).flat_map(|m| &mut m.clips).map(|c| &mut c.id), "clip");
        Some(Project {
            session: session?,
            samples: samples?,
            patterns: patterns?,
            tracks,
            returns: returns?,
            sections: sections?,
            master: master?,
        })
    }

    /// 2 for a song with a MIDI track, which an engine that knows only
    /// version 1 refuses; otherwise 1, so that such a song saves as it did.
    pub fn schema_version(&self) -> i64 {
        if self.tracks.iter().any(|t| t.midi.is_some()) {
            2
        } else {
            1
        }
    }

    /// `model_dump(mode="json")`, or with `saved` the form `save` writes, minus
    /// the leading `schema_version` of a version 1 song.
    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.int("schema_version", self.schema_version(), 1);
        o.req("session", self.session.dump(saved));
        o.coll("samples", str_map(&self.samples, saved, Sample::dump));
        o.coll("patterns", str_map(&self.patterns, saved, Pattern::dump));
        o.coll("tracks", list(&self.tracks, saved, Track::dump));
        o.coll("returns", list(&self.returns, saved, Return::dump));
        o.coll("sections", list(&self.sections, saved, Section::dump));
        let master = self.master.dump(saved);
        let default_master = self.master.effects.is_empty() && self.master.automation.is_empty();
        if !(saved && default_master) {
            o.req("master", master);
        }
        o.done()
    }

    pub fn track(&self, id: &str) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    /// Tracks with sidechain sources first; otherwise document order.
    pub fn render_order(&self) -> Vec<&Track> {
        let mut done: Vec<&str> = Vec::new();
        let mut order: Vec<&Track> = Vec::new();
        while order.len() < self.tracks.len() {
            for t in &self.tracks {
                if !done.contains(&t.id.as_str()) && t.sidechains().iter().all(|s| done.contains(s)) {
                    done.push(&t.id);
                    order.push(t);
                    break;
                }
            }
        }
        order
    }

    /// Tracks with a send to `return_id`, in document order.
    pub fn senders(&self, return_id: &str) -> Vec<&str> {
        self.tracks
            .iter()
            .filter(|t| t.sends.iter().any(|s| s.to == return_id))
            .map(|t| t.id.as_str())
            .collect()
    }

    /// Every track whose audio feeds a track or return's detectors, directly or not.
    pub fn sidechain_sources(&self, channel_id: &str) -> Vec<String> {
        let direct: Vec<&str> = match self.track(channel_id) {
            Some(t) => t.sidechains(),
            None => self
                .returns
                .iter()
                .find(|r| r.id == channel_id)
                .map(Return::sidechains)
                .unwrap_or_default(),
        };
        let mut found: Vec<String> = Vec::new();
        let mut stack: Vec<String> = direct.into_iter().map(str::to_string).collect();
        while let Some(s) = stack.pop() {
            if !found.contains(&s) {
                if let Some(t) = self.track(&s) {
                    stack.extend(t.sidechains().into_iter().map(str::to_string));
                }
                found.push(s);
            }
        }
        found
    }
}

impl Default for Project {
    /// `Project(session=Session())`.
    fn default() -> Self {
        Project {
            session: Session {
                title: "Untitled".into(),
                tempo: 144.0,
                time_signature: "4/4".into(),
                sample_rate: 48000,
                length_beats: Beat::int(16),
                master_gain_db: -6.0,
                end_fade_ms: 20.0,
                stretcher: Stretcher::Signalsmith,
            },
            samples: IndexMap::new(),
            patterns: IndexMap::new(),
            tracks: Vec::new(),
            returns: Vec::new(),
            sections: Vec::new(),
            master: Master::default(),
        }
    }
}
