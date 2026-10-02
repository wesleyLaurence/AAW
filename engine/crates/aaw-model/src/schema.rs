//! Schema v1 types, validated as pydantic validated the Python model's and
//! dumped as its `model_dump(mode="json")` dumped them, in full or in the saved
//! form without defaults.

use crate::beat::{beat as exact_beat, Beat};
use crate::rules;
use crate::validate::{self as v, Bounds, Ctx, Fields, Loc};
use crate::value::{Dict, Key, Value};
use indexmap::IndexMap;
use num_rational::BigRational;
use num_traits::Signed;
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
    pub mono: bool,
}

impl Pad {
    const FIELDS: &'static [&'static str] = &[
        "sample", "mode", "gain_db", "pan", "transpose", "start_seconds", "end_seconds",
        "attack_ms", "release_ms", "choke_group", "reverse", "source_bpm", "mono",
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

#[derive(Clone, Debug)]
pub enum Effect {
    Filter(Filter),
    Eq(Eq),
    Compressor(Compressor),
    Limiter(Limiter),
    Delay(Delay),
    Reverb(Reverb),
}

pub const EFFECT_TYPES: [&str; 6] = ["filter", "eq", "compressor", "limiter", "delay", "reverb"];

impl Effect {
    pub fn kind(&self) -> &'static str {
        match self {
            Effect::Filter(_) => "filter",
            Effect::Eq(_) => "eq",
            Effect::Compressor(_) => "compressor",
            Effect::Limiter(_) => "limiter",
            Effect::Delay(_) => "delay",
            Effect::Reverb(_) => "reverb",
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
                        "Input tag '{}' found using 'type' does not match any of the expected tags: 'filter', 'eq', 'compressor', 'limiter', 'delay', 'reverb'",
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
}

impl Point {
    const FIELDS: &'static [&'static str] = &["at", "value", "curve"];

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
        f.finish(ctx);
        if ctx.count() > before {
            return None;
        }
        Some(Point {
            at: at?,
            value: value?,
            curve: curve?,
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
    pub effects: Vec<Effect>,
    pub sends: Vec<Send>,
    pub automation: Vec<Lane>,
}

impl Track {
    const FIELDS: &'static [&'static str] = &[
        "id", "gain_db", "pan", "mute", "solo", "pads", "clips", "effects", "sends", "automation",
    ];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Track> {
        let f = Fields::of(ctx, x, "Track", Self::FIELDS)?;
        let before = ctx.count();
        let id = f.req(ctx, "id", id_field);
        let gain_db = f.opt(ctx, "gain_db", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-96", "24")));
        let pan = f.opt(ctx, "pan", 0.0, |c, x| v::float(c, x, Bounds::ge_le("-1", "1")));
        let mute = f.opt(ctx, "mute", false, v::boolean);
        let solo = f.opt(ctx, "solo", false, v::boolean);
        let pads = f.req(ctx, "pads", |c, x| v::str_dict(c, x, Pad::validate));
        let clips = f.opt(ctx, "clips", Vec::new(), |c, x| v::list(c, x, 0, None, Clip::validate));
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
            effects: effects?,
            sends: sends?,
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
        o.bool("solo", self.solo, false);
        o.req("pads", str_map(&self.pads, saved, Pad::dump));
        o.coll("clips", list(&self.clips, saved, Clip::dump));
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
}

impl Session {
    const FIELDS: &'static [&'static str] = &[
        "title", "tempo", "time_signature", "sample_rate", "length_beats", "master_gain_db", "end_fade_ms",
    ];

    fn validate(ctx: &mut Ctx, x: &Value) -> Option<Session> {
        let f = Fields::of(ctx, x, "Session", Self::FIELDS)?;
        let before = ctx.count();
        let title = f.opt(ctx, "title", "Untitled".to_string(), v::string);
        let tempo = f.opt(ctx, "tempo", 144.0, |c, x| v::float(c, x, Bounds::ge_le("20", "400")));
        let time_signature = f.opt(ctx, "time_signature", "4/4".to_string(), |c, x| {
            v::literal_str(c, x, &["4/4"]).map(str::to_string)
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
        })
    }

    pub fn length_exact(&self) -> BigRational {
        exact(&self.length_beats)
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
        let schema = f.opt(ctx, "schema_version", 1, |c, x| v::literal_int(c, x, &[1]));
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
        Some(Project {
            session: session?,
            samples: samples?,
            patterns: patterns?,
            tracks: tracks?,
            returns: returns?,
            sections: sections?,
            master: master?,
        })
    }

    /// `model_dump(mode="json")`, or with `saved` the form `save` writes, minus
    /// the leading `schema_version`.
    pub fn dump(&self, saved: bool) -> Value {
        let mut o = Out::new(saved);
        o.int("schema_version", 1, 1);
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
