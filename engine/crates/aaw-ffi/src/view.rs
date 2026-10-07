//! What the app draws: the arrangement of a revision, and which objects a
//! change touched. Positions are beats as floats, for display only; the song's
//! exact values stay in the host.

use crate::files::{FileView, Files};
use aaw_host::session::Doc;
use aaw_host::tree::{Item, Node};
use aaw_model::describe::{self, Initial, Kind};
use aaw_model::rules::{midi, synth_param, synth_params, target, Domain, Owner, TargetKind};
use aaw_model::value::{py_eq, Value};
use aaw_model::{step_cells, AudioClip, Curve, Effect, Lane, Project};
use num_rational::BigRational;
use num_traits::ToPrimitive;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Arrangement {
    pub revision: u64,
    pub project_sha256: String,
    pub title: String,
    pub tempo: f64,
    /// The time signature as written, `4/4`.
    pub time_signature: String,
    /// A bar in beats as the song writes them: 3 in 3/4 and 6/8, 3.5 in 7/8.
    pub bar_beats: f64,
    /// The note the time signature counts, in those beats: 1 in 3/4, 0.5 in 6/8.
    pub beat_unit: f64,
    pub length_beats: f64,
    pub tracks: Vec<TrackView>,
    pub returns: Vec<ReturnView>,
    pub master: MasterView,
    pub sections: Vec<SectionView>,
    pub patterns: Vec<PatternView>,
    /// The files the tracks' audio clips play.
    pub files: Vec<FileView>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FieldKind {
    /// A number from `min` to `max`.
    Number,
    /// A whole number from `min` to `max`, or one of `choices`.
    Integer,
    /// One of `choices`.
    Choice,
    Flag,
    /// A length in beats, written as a number or a fraction such as `3/4`.
    Beats,
    /// One of `choices`, the tracks that can key the effect, or none.
    Track,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum FieldValue {
    Number { value: f64 },
    Text { value: String },
    Flag { value: bool },
    /// Left out: an optional field that is off.
    Absent,
}

/// One field of an effect, with what a control for it needs: a device panel
/// is drawn from these without knowing the effects one by one.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FieldView {
    /// The field within its effect, such as `cutoff_hz` or `bands.0.gain_db`.
    pub name: String,
    pub label: String,
    pub kind: FieldKind,
    pub value: FieldValue,
    pub min: f64,
    pub max: f64,
    pub unit: String,
    /// Whether the value is best moved in equal ratios, as a frequency is.
    pub log: bool,
    pub choices: Vec<String>,
    /// Whether the field may be `Absent`.
    pub optional: bool,
    /// The value a reset gives: the default, or a place to start.
    pub initial: FieldValue,
    /// Whether a playing song glides to a new value. Other fields give the
    /// device new state, which the song fades through, so a control for one
    /// sends its value when the drag ends.
    pub live: bool,
    /// The parameter a lane for this field has; None where automation cannot
    /// move the field.
    pub param: Option<String>,
    /// The key of the lane that moves the field.
    pub lane: Option<u64>,
    /// The equalizer band the field belongs to.
    pub band: Option<u32>,
}

/// An effect in a chain.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EffectView {
    /// The effect's handle.
    pub key: u64,
    pub kind: String,
    pub id: Option<String>,
    pub bypass: bool,
    pub fields: Vec<FieldView>,
    /// An equalizer's bands.
    pub bands: u32,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PointView {
    pub key: u64,
    pub at: f64,
    pub value: f64,
    /// Whether the value holds until the next point instead of moving to it.
    pub hold: bool,
    /// How the segment to the next point bends, from -1 to 1: above zero it
    /// starts slowly, below zero it finishes slowly, and zero is straight.
    pub shape: f64,
}

/// An automation lane.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct LaneView {
    pub key: u64,
    pub param: String,
    pub label: String,
    pub unit: String,
    /// The range to draw and drag in. Levels show -60 to +6 dB as the faders
    /// do; the song allows more, and points outside are drawn at the edge.
    pub min: f64,
    pub max: f64,
    pub log: bool,
    pub points: Vec<PointView>,
}

/// A parameter that could have a lane and has none.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct LaneTarget {
    pub param: String,
    pub label: String,
}

/// A pad of a track's sampler.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PadView {
    pub name: String,
    pub sample: String,
    pub gain_db: f64,
    /// Whether events hold the sample for their duration.
    pub gate: bool,
    /// The sample's root note as a MIDI number, for a pad that events can
    /// play at other pitches.
    pub root: Option<i32>,
}

/// A row of a pattern's step grid: a pad's hits on the pattern's grid.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StepRow {
    pub pad: String,
    /// Each step's level: 0 for none, 1 to 9 for the row's digits, softest
    /// to hardest, and 10 for an `x`.
    pub cells: Vec<u8>,
}

/// A hit a pattern places by itself, off the grid or with a pitch or a length.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EventView {
    pub key: u64,
    /// Beats from the pattern's start, and as the song writes them, such as `4/3`.
    pub at: f64,
    pub at_text: String,
    pub pad: String,
    /// 1 to 127.
    pub velocity: u32,
    pub note: Option<String>,
    /// The note as a MIDI number.
    pub pitch: Option<i32>,
    /// How long a gated pad is held, in beats.
    pub duration: Option<f64>,
    pub duration_text: Option<String>,
    pub transpose: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PatternView {
    pub name: String,
    /// The length in beats, and as the song writes it.
    pub length_beats: f64,
    pub length_text: String,
    /// The length of a step, in beats, and as the song writes it, such as `1/4`.
    pub grid: f64,
    pub grid_text: String,
    /// How far each second step is pushed late: 0.5 is straight.
    pub swing: f64,
    /// Whether the length is a whole number of steps, as step rows need.
    pub steps: Option<u32>,
    pub rows: Vec<StepRow>,
    pub events: Vec<EventView>,
    /// How many clips play the pattern.
    pub clips: u32,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TrackView {
    /// The track's handle: the same for as long as the host runs, through
    /// renames and moves.
    pub key: u64,
    pub id: String,
    pub gain_db: f64,
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
    pub effects: Vec<EffectView>,
    pub sends: Vec<SendView>,
    pub lanes: Vec<LaneView>,
    pub lane_targets: Vec<LaneTarget>,
    pub pads: Vec<PadView>,
    pub clips: Vec<ClipView>,
    pub audio: Vec<AudioClipView>,
    /// Whether it is a MIDI track, whose clips are `note_clips` and whose
    /// `pads` are its sampler's.
    pub midi: bool,
    /// The kind of a MIDI track's instrument, such as `sampler`; None
    /// without one.
    pub instrument: Option<String>,
    /// Which notes play which of a MIDI track's sampler's pads.
    pub map: Vec<NoteMapView>,
    /// The instrument as the Sampler device, when it is one.
    pub sampler: Option<SamplerView>,
    /// The instrument as the Synth, when it is one.
    pub synth: Option<SynthView>,
    pub note_clips: Vec<NoteClipView>,
}

/// A MIDI track's Synth as the panel draws it: every part's fields as
/// controls need them, each field named by its path in the patch, so a
/// control's edit is `SynthSet` of that path.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SynthView {
    /// The patch's name, when the sound came from one.
    pub patch: Option<String>,
    /// The Synth's own fields: voices, glide, velocity and seed.
    pub fields: Vec<FieldView>,
    pub oscillators: Vec<SynthGroup>,
    pub filter: Vec<FieldView>,
    pub envelopes: Vec<SynthGroup>,
    pub lfos: Vec<SynthGroup>,
    /// A field for each macro, named `macros.NAME`.
    pub macros: Vec<FieldView>,
    pub modulation: Vec<ModulationView>,
    /// The patch's own effects, each as an effect's panel draws it; a
    /// field's `param` is `instrument.effects.REF.FIELD`.
    pub effects: Vec<EffectView>,
}

/// A part of the Synth that there are several of, by its ID: an oscillator,
/// an envelope or an LFO, with its fields.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SynthGroup {
    pub name: String,
    pub fields: Vec<FieldView>,
    /// For a wavetable oscillator, its table's cycle at 64 points, -1 to 1,
    /// for the drawing; empty for any other wave or a table that cannot be
    /// read.
    pub cycle: Vec<f64>,
}

/// An entry of the Synth's matrix, with the unit of its amount.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ModulationView {
    pub source: String,
    pub target: String,
    pub amount: f64,
    pub unit: String,
    /// The field of the panel the entry moves, by its path, for the mark on
    /// its control: an oscillator's pitch is its `semitones`. None for
    /// `pitch`, which moves every oscillator.
    pub field: Option<String>,
}

/// The field of the panel a matrix target moves, by its path in the patch:
/// `oscillators.a.pitch` moves `oscillators.a.semitones`, and every other
/// target is a field of its own name. None for `pitch`, which is every
/// oscillator's, and for a name that is no target.
pub fn mod_field(target: &str) -> Option<String> {
    let parts: Vec<&str> = target.split('.').collect();
    match parts.as_slice() {
        ["oscillators", id, "pitch"] => Some(format!("oscillators.{id}.semitones")),
        ["oscillators", _, "level_db" | "pan" | "pulse_width" | "unison_detune_cents"] => Some(target.to_string()),
        ["width_percent"] => Some(target.to_string()),
        ["filter", "cutoff_hz" | "resonance_percent" | "drive_db"] => Some(target.to_string()),
        ["envelopes", _, "attack_ms" | "decay_ms" | "sustain_percent" | "release_ms"] => Some(target.to_string()),
        _ => None,
    }
}

/// The matrix target a field of the panel stands for, by the field's path,
/// for a source dropped on its control: `oscillators.a.semitones` is the
/// target `oscillators.a.pitch`. None for a field the matrix cannot move.
#[uniffi::export]
pub fn synth_mod_target(field: String) -> Option<String> {
    let parts: Vec<&str> = field.split('.').collect();
    match parts.as_slice() {
        ["oscillators", id, "semitones"] => Some(format!("oscillators.{id}.pitch")),
        ["oscillators", _, "level_db" | "pan" | "pulse_width" | "unison_detune_cents"] => Some(field.clone()),
        ["width_percent"] => Some(field.clone()),
        ["filter", "cutoff_hz" | "resonance_percent" | "drive_db"] => Some(field.clone()),
        ["envelopes", _, "attack_ms" | "decay_ms" | "sustain_percent" | "release_ms"] => Some(field.clone()),
        _ => None,
    }
}

/// A MIDI track's Sampler device: a sampler that is empty, or one pad played
/// on every note at its pitch, as the app's panel draws it. A sampler of
/// several pads, such as a kit, is listed instead and has none of this.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SamplerView {
    /// The pad and its sample, once a sample is loaded, and the sample's
    /// file, relative to the song's folder, for measuring its pitch.
    pub pad: Option<String>,
    pub sample: Option<String>,
    pub path: String,
    /// The pad's fields as the panel's controls need them, from the model's
    /// description of a pad; none while the Sampler is empty. The start and
    /// the end go as far as the file does.
    pub fields: Vec<FieldView>,
    /// The sample's root note as a MIDI number and as the song writes it,
    /// when it has one; without one the keys play it as it is at middle C.
    pub root: Option<i32>,
    pub root_text: String,
    /// The identity of the sample's file among the arrangement's `files`,
    /// or 0 when there is none or it cannot be read, and its length.
    pub file: u64,
    pub seconds: f64,
    /// The seconds of the file the pad plays from and to; the end is the
    /// file's when the pad has none of its own.
    pub start_seconds: f64,
    pub end_seconds: f64,
}

/// Notes a sampler's pad plays: from `low` to `high`, inclusive, at their
/// pitches when `pitched`.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct NoteMapView {
    pub low: i32,
    pub high: i32,
    pub pad: String,
    pub pitched: bool,
}

/// A note of a note clip.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct NoteView {
    pub key: u64,
    pub id: String,
    pub pitch: i32,
    /// Beats from the clip's start; negative before it.
    pub at: f64,
    pub duration: f64,
    pub velocity: u32,
    /// The place and length as the song writes them, such as `1/3`.
    pub at_text: String,
    pub duration_text: String,
}

/// A clip of a MIDI track, with the notes it owns.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct NoteClipView {
    pub key: u64,
    /// How commands address the clip, such as `@12`.
    pub reference: String,
    pub id: String,
    pub at: f64,
    pub length_beats: f64,
    /// The loop's length when the clip loops: its first so many beats play
    /// again at each wrap until its end.
    pub loop_beats: Option<f64>,
    pub notes: Vec<NoteView>,
}

/// A send from a track to a return.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SendView {
    /// The return's ID.
    pub to: String,
    pub gain_db: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ClipView {
    pub key: u64,
    /// How commands address the clip, such as `@12`.
    pub reference: String,
    pub pattern: String,
    pub at: f64,
    /// The length of one repeat.
    pub pattern_beats: f64,
    pub repeats: u32,
}

/// An audio clip: part of a sample file on a track's timeline.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AudioClipView {
    pub key: u64,
    /// How commands address the clip, such as `@12`.
    pub reference: String,
    pub sample: String,
    /// The beat its start plays on.
    pub at: f64,
    /// How long it plays before it leaves, in beats: to its end, or to its
    /// file's; a looped clip's own length.
    pub length_beats: f64,
    /// The loop's length when the clip loops: its first so many beats of the
    /// file play again at each wrap until its end.
    pub loop_beats: Option<f64>,
    /// How long it sounds after that while it fades out, in beats: its fade
    /// out, as far as its file goes. A clip is drawn to where its sound ends.
    pub tail_beats: f64,
    /// The seconds of its file it plays from and to; the end is the file's
    /// when the clip has none of its own.
    pub source_start_seconds: f64,
    pub source_end_seconds: f64,
    /// Seconds of its file that go by in a beat of the song.
    pub seconds_per_beat: f64,
    pub gain_db: f64,
    pub fade_in_ms: f64,
    pub fade_out_ms: f64,
    /// `equal_power` or `linear`.
    pub fade_curve: String,
    /// The file's tempo, when the clip follows the song's.
    pub source_bpm: Option<f64>,
    /// `repitch` or `preserve_pitch`.
    pub stretch: String,
    /// The identity of its file among the arrangement's `files`, or 0 when
    /// the file cannot be read.
    pub file: u64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ReturnView {
    pub key: u64,
    pub id: String,
    pub gain_db: f64,
    pub pan: f64,
    pub mute: bool,
    pub effects: Vec<EffectView>,
    pub lanes: Vec<LaneView>,
    pub lane_targets: Vec<LaneTarget>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MasterView {
    pub gain_db: f64,
    pub effects: Vec<EffectView>,
    pub lanes: Vec<LaneView>,
    pub lane_targets: Vec<LaneTarget>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SectionView {
    pub key: u64,
    pub id: String,
    pub at: f64,
    pub length_beats: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Part {
    /// Title, tempo or length.
    Session,
    Track,
    Clip,
    Return,
    Master,
    Section,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Delta {
    Added,
    Changed,
    Removed,
}

/// An object a change affected. `key` is 0 for the session and the master.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Touch {
    pub part: Part,
    pub key: u64,
    pub delta: Delta,
}

fn field_value(v: Option<&Value>) -> FieldValue {
    match v {
        Some(Value::Float(x)) => FieldValue::Number { value: *x },
        Some(Value::Int(n)) => FieldValue::Number {
            value: n.to_f64().unwrap_or(0.0),
        },
        Some(Value::Str(s)) => FieldValue::Text { value: s.clone() },
        Some(Value::Bool(b)) => FieldValue::Flag { value: *b },
        _ => FieldValue::Absent,
    }
}

fn initial(i: Initial) -> FieldValue {
    match i {
        Initial::Number(value) => FieldValue::Number { value },
        Initial::Text(s) => FieldValue::Text { value: s.to_string() },
        Initial::Flag(value) => FieldValue::Flag { value },
        Initial::Required | Initial::Absent => FieldValue::Absent,
    }
}

/// How an effect is addressed within its chain: its ID, else its place.
fn effect_ref(e: &Effect, index: usize) -> String {
    e.id().filter(|id| !id.is_empty()).map_or_else(|| index.to_string(), str::to_string)
}

/// An owner's lanes by what they drive, as (effect index, field name) for an
/// effect's and the lane's own parameter otherwise, with each lane's handle.
struct Driven {
    effects: HashMap<(usize, String), u64>,
    /// A synth's fields by their path in the patch.
    instrument: HashMap<String, u64>,
    others: HashSet<String>,
}

impl Driven {
    fn new(owner: Owner, lanes: &[Lane], handles: &[Item]) -> Driven {
        let mut out = Driven {
            effects: HashMap::new(),
            instrument: HashMap::new(),
            others: HashSet::new(),
        };
        for (i, lane) in lanes.iter().enumerate() {
            match target(owner, &lane.param) {
                Ok(t) => match &t.kind {
                    TargetKind::Effect { index, .. } => {
                        out.effects.insert((*index, t.name()), handle(handles, i));
                    }
                    TargetKind::Instrument => {
                        out.instrument.insert(t.field.clone(), handle(handles, i));
                    }
                    _ => {
                        out.others.insert(t.key());
                    }
                },
                Err(_) => {}
            }
        }
        out
    }
}

/// An effect's fields, drawn from the model's description of its type.
/// `instrument` is set for an effect of a Synth's patch, whose lanes are the
/// track's on `instrument.effects.REF.FIELD`.
fn fields(e: &Effect, index: usize, tracks: &[String], driven: &Driven, instrument: bool) -> Vec<FieldView> {
    let dump = e.dump(false);
    let reference = effect_ref(e, index);
    let prefix = if instrument { "instrument." } else { "" };
    let field = |f: &describe::Field, value: Option<&Value>, name: String, band: Option<u32>| {
        let kind = if matches!(e, Effect::Eq(_)) { "eq" } else { e.kind() };
        let automatable = describe::automatable(kind, f.name).is_some();
        let lane = if instrument {
            driven.instrument.get(&format!("effects.{index}.{name}")).copied()
        } else {
            driven.effects.get(&(index, name.clone())).copied()
        };
        FieldView {
            label: f.label.to_string(),
            kind: match f.kind {
                Kind::Number => FieldKind::Number,
                Kind::Integer => FieldKind::Integer,
                Kind::Choice => FieldKind::Choice,
                Kind::Flag => FieldKind::Flag,
                Kind::Beats => FieldKind::Beats,
                Kind::Track => FieldKind::Track,
            },
            value: field_value(value),
            min: f.min,
            max: f.max,
            unit: f.unit.to_string(),
            log: f.log,
            choices: match f.kind {
                Kind::Track => tracks.to_vec(),
                _ => f.choices.iter().map(|c| c.to_string()).collect(),
            },
            optional: f.default == Initial::Absent,
            initial: initial(if f.default == Initial::Required || f.default == Initial::Absent { f.suggested } else { f.default }),
            live: !f.structural,
            param: automatable.then(|| format!("{prefix}effects.{reference}.{name}")),
            lane,
            band,
            name,
        }
    };
    match e {
        Effect::Eq(eq) => {
            let Some(Value::List(bands)) = dump.get("bands") else { return Vec::new() };
            (0..eq.bands.len())
                .flat_map(|j| describe::BAND.iter().map(move |f| (j, f)))
                .map(|(j, f)| field(f, bands[j].get(f.name), format!("bands.{j}.{}", f.name), Some(j as u32)))
                .collect()
        }
        _ => describe::effect(e.kind()).iter().map(|f| field(f, dump.get(f.name), f.name.to_string(), None)).collect(),
    }
}

/// `tracks` are the tracks that may key the chain's compressors; a patch's
/// chain (`instrument`) has none.
fn effects(chain: &[Effect], handles: &[Item], tracks: &[String], driven: &Driven, instrument: bool) -> Vec<EffectView> {
    chain
        .iter()
        .enumerate()
        .map(|(i, e)| EffectView {
            key: handle(handles, i),
            kind: e.kind().to_string(),
            id: e.id().filter(|id| !id.is_empty()).map(str::to_string),
            bypass: e.bypass(),
            fields: fields(e, i, tracks, driven, instrument),
            bands: match e {
                Effect::Eq(eq) => eq.bands.len() as u32,
                _ => 0,
            },
        })
        .collect()
}

/// What an effect goes by in a lane's label: its ID, or its type.
fn effect_name(e: &Effect) -> &str {
    e.id().filter(|id| !id.is_empty()).unwrap_or(e.kind())
}

/// A synth field's label and unit, by its path in the patch: `Filter
/// cutoff`, `Osc a level`, `Macro tone`, `Chorus mix` for an effect of the
/// patch, which `synth` names.
pub fn synth_label(synth: &aaw_model::Synth, path: &str) -> (String, String) {
    let parts: Vec<&str> = path.split('.').collect();
    let spec = |table: &'static [describe::Field], name: &str| table.iter().find(|f| f.name == name).map(|f| (f.label, f.unit));
    if path.starts_with("effects.") {
        return match aaw_model::rules::synth_effect_param(synth, path) {
            Ok((index, name, _, _)) => {
                let e = &synth.effects[index];
                let field = name.rsplit('.').next().unwrap_or(&name);
                let f = match e {
                    Effect::Eq(_) => spec(describe::BAND, field),
                    _ => spec(describe::effect(e.kind()), field),
                };
                let band = name.split('.').nth(1).filter(|_| name.starts_with("bands.")).and_then(|b| b.parse::<usize>().ok()).map_or(String::new(), |b| format!(" band {}", b + 1));
                (
                    format!("{}{band} {}", effect_name(e), f.map_or(field, |f| f.0).to_lowercase()).trim().to_string(),
                    f.map_or("", |f| f.1).to_string(),
                )
            }
            Err(_) => (path.to_string(), String::new()),
        };
    }
    let (part, found) = match parts.as_slice() {
        [name] => ("".to_string(), spec(describe::SYNTH, name)),
        ["oscillators", id, name] => (format!("Osc {id} "), spec(describe::OSCILLATOR, name)),
        ["filter", name] => ("Filter ".to_string(), spec(describe::SYNTH_FILTER, name)),
        ["envelopes", id, name] => (format!("Env {id} "), spec(describe::ENVELOPE, name)),
        ["lfos", id, name] => (format!("LFO {id} "), spec(describe::LFO, name)),
        ["macros", name] => (format!("Macro {name}"), Some(("", ""))),
        _ => (path.to_string(), Some(("", ""))),
    };
    match found {
        Some((label, unit)) => (format!("{part}{}", if part.is_empty() { label.to_string() } else { label.to_lowercase() }).trim().to_string(), unit.to_string()),
        None => (path.to_string(), String::new()),
    }
}

/// A lane parameter as a person reads it, with its unit.
fn lane_label(owner: Owner, kind: &TargetKind, field: &str) -> (String, String) {
    match kind {
        TargetKind::Channel if field == "pan" => ("Pan".into(), String::new()),
        TargetKind::Channel => ("Volume".into(), "dB".into()),
        TargetKind::Send(to) => (format!("Send {to}"), "dB".into()),
        TargetKind::Instrument => {
            let synth = match owner {
                Owner::Track(t) => t.midi.as_ref().and_then(|m| m.synth()),
                _ => None,
            };
            let (label, unit) = match synth {
                Some(s) => synth_label(s, field),
                None => (field.to_string(), String::new()),
            };
            (format!("Synth {}", label.to_lowercase()), unit)
        }
        TargetKind::Effect { index, band } => {
            let e = &owner.effects()[*index];
            let spec = |fields: &'static [describe::Field]| fields.iter().find(|f| f.name == field);
            match band {
                Some(b) => {
                    let f = spec(describe::BAND);
                    (
                        format!("{} band {} {}", effect_name(e), b + 1, f.map_or(field, |f| f.label)),
                        f.map_or("", |f| f.unit).to_string(),
                    )
                }
                None => {
                    let f = spec(describe::effect(e.kind()));
                    (
                        format!("{} {}", effect_name(e), f.map_or(field, |f| f.label)),
                        f.map_or("", |f| f.unit).to_string(),
                    )
                }
            }
        }
    }
}

fn lanes(owner: Owner, lanes: &[Lane], handles: &[Item]) -> Vec<LaneView> {
    lanes
        .iter()
        .enumerate()
        .filter_map(|(i, lane)| {
            let t = target(owner, &lane.param).ok()?;
            let (label, unit) = lane_label(owner, &t.kind, &t.field);
            // Levels show the range the faders cover.
            let (min, max) = match &t.kind {
                TargetKind::Channel if t.field == "gain_db" => (-60.0, 6.0),
                TargetKind::Send(_) => (-60.0, 0.0),
                _ => (t.range.low, t.range.high),
            };
            let point_handles = handles.get(i).map(|item| items(&item.node, "points")).unwrap_or(&[]);
            Some(LaneView {
                key: handle(handles, i),
                param: lane.param.clone(),
                label,
                unit,
                min,
                max,
                log: t.domain == Domain::Log,
                points: lane
                    .points
                    .iter()
                    .enumerate()
                    .map(|(j, p)| PointView {
                        key: handle(point_handles, j),
                        at: p.at_exact().to_f64().unwrap_or(0.0),
                        value: p.value,
                        hold: p.curve == Curve::Hold,
                        shape: p.shape,
                    })
                    .collect(),
            })
        })
        .collect()
}

/// The parameters of an owner that could have a lane and have none.
fn lane_targets(owner: Owner, sends: &[String], driven: &Driven) -> Vec<LaneTarget> {
    let mut params: Vec<String> = vec!["gain_db".into()];
    if !matches!(owner, Owner::Master(_)) {
        params.push("pan".into());
    }
    params.extend(sends.iter().map(|to| format!("sends.{to}.gain_db")));
    let mut out: Vec<LaneTarget> = params
        .into_iter()
        .filter_map(|param| {
            let t = target(owner, &param).ok()?;
            (!driven.others.contains(&t.key())).then(|| LaneTarget {
                label: lane_label(owner, &t.kind, &t.field).0,
                param,
            })
        })
        .collect();
    if let Owner::Track(t) = owner {
        if let Some(synth) = t.midi.as_ref().and_then(|m| m.synth()) {
            for path in synth_params(synth) {
                let param = format!("instrument.{path}");
                // A patch effect's lane is keyed by the effect's index,
                // whatever it was named by.
                if let Ok(t) = target(owner, &param) {
                    if driven.instrument.contains_key(&t.field) {
                        continue;
                    }
                    out.push(LaneTarget {
                        label: lane_label(owner, &t.kind, &t.field).0,
                        param,
                    });
                }
            }
        }
    }
    for (i, e) in owner.effects().iter().enumerate() {
        let reference = effect_ref(e, i);
        let names: Vec<String> = match e {
            Effect::Eq(eq) => (0..eq.bands.len())
                .flat_map(|j| describe::BAND.iter().map(move |f| (j, f)))
                .filter(|(_, f)| describe::automatable("eq", f.name).is_some())
                .map(|(j, f)| format!("bands.{j}.{}", f.name))
                .collect(),
            _ => describe::effect(e.kind())
                .iter()
                .filter(|f| describe::automatable(e.kind(), f.name).is_some())
                .map(|f| f.name.to_string())
                .collect(),
        };
        for name in names {
            if driven.effects.contains_key(&(i, name.clone())) {
                continue;
            }
            let param = format!("effects.{reference}.{name}");
            if let Ok(t) = target(owner, &param) {
                out.push(LaneTarget {
                    label: lane_label(owner, &t.kind, &t.field).0,
                    param,
                });
            }
        }
    }
    out
}

/// A channel's effects, lanes and what could still be automated, and a
/// MIDI track's Synth. `samples` are the project's samples, which a
/// wavetable may name, under `directory`.
fn channel(owner: Owner, node: Option<&Node>, tracks: &[String], sends: &[String], samples: &Project, directory: &Path) -> (Vec<EffectView>, Vec<LaneView>, Vec<LaneTarget>, Option<SynthView>) {
    let (effect_items, lane_items) = match node {
        Some(n) => (items(n, "effects"), items(n, "automation")),
        None => (&[][..], &[][..]),
    };
    let driven = Driven::new(owner, owner.automation(), lane_items);
    let synth = match owner {
        Owner::Track(t) => t.midi.as_ref().and_then(|m| m.synth()).map(|s| {
            let synth_items = node.and_then(|n| n.get("instrument")).and_then(|i| i.get("synth")).map(|n| items(n, "effects")).unwrap_or(&[]);
            synth_view(s, &driven, synth_items, samples, directory)
        }),
        _ => None,
    };
    (
        effects(owner.effects(), effect_items, tracks, &driven, false),
        lanes(owner, owner.automation(), lane_items),
        lane_targets(owner, sends, &driven),
        synth,
    )
}

/// A field of a synth as a control needs it. `path` is its path in the
/// patch and `value` what the patch holds there. `samples` are the
/// project's, which join the built-in tables as a `table`'s choices.
fn synth_field(synth: &aaw_model::Synth, f: &describe::Field, path: String, value: Option<&Value>, driven: &Driven, samples: &[String]) -> FieldView {
    let automatable = synth_param(synth, &path).is_ok();
    let mut choices: Vec<String> = f.choices.iter().map(|c| c.to_string()).collect();
    if f.name == "table" && path.starts_with("oscillators.") {
        choices.extend(samples.iter().cloned());
    }
    FieldView {
        label: f.label.to_string(),
        kind: match f.kind {
            Kind::Number => FieldKind::Number,
            Kind::Integer => FieldKind::Integer,
            Kind::Choice => FieldKind::Choice,
            Kind::Flag => FieldKind::Flag,
            Kind::Beats => FieldKind::Beats,
            Kind::Track => FieldKind::Track,
        },
        value: field_value(value),
        min: f.min,
        max: f.max,
        unit: f.unit.to_string(),
        log: f.log,
        choices,
        optional: f.default == Initial::Absent,
        initial: initial(if f.default == Initial::Required || f.default == Initial::Absent { f.suggested } else { f.default }),
        live: !f.structural,
        param: automatable.then(|| format!("instrument.{path}")),
        lane: driven.instrument.get(&path).copied(),
        band: None,
        name: path,
    }
}

/// The cycle a wavetable oscillator reads, at 64 points, for its drawing:
/// a built-in table's, or a sample's file read as one cycle.
fn table_cycle(o: &aaw_model::Oscillator, p: &Project, directory: &Path) -> Vec<f64> {
    use aaw_dsp::wavetable::Wavetable;
    if o.wave != aaw_model::Wave::Wavetable {
        return Vec::new();
    }
    if let Some(t) = Wavetable::builtin(&o.table) {
        return t.cycle(64);
    }
    let Some(asset) = p.samples.get(&o.table) else { return Vec::new() };
    match aaw_engine::sndfile::read(&directory.join(&asset.path)) {
        Ok(audio) => Wavetable::from_cycle(&o.table, &audio.data, audio.channels).map_or_else(|_| Vec::new(), |t| t.cycle(64)),
        Err(_) => Vec::new(),
    }
}

/// A track's Synth, every part's fields drawn from the model's description.
/// `effect_items` are the tree's items of the patch's effects, for their
/// handles.
fn synth_view(synth: &aaw_model::Synth, driven: &Driven, effect_items: &[Item], p: &Project, directory: &Path) -> SynthView {
    let dump = synth.dump(false);
    let samples: Vec<String> = p.samples.keys().cloned().collect();
    let own = |table: &'static [describe::Field], prefix: &str, holder: Option<&Value>| -> Vec<FieldView> {
        table
            .iter()
            .map(|f| {
                let path = if prefix.is_empty() { f.name.to_string() } else { format!("{prefix}.{}", f.name) };
                synth_field(synth, f, path, holder.and_then(|h| h.get(f.name)), driven, &samples)
            })
            .collect()
    };
    let groups = |table: &'static [describe::Field], part: &str| -> Vec<SynthGroup> {
        match dump.get(part) {
            Some(Value::Dict(d)) => d
                .iter()
                .filter_map(|(k, v)| k.as_str().map(|id| (id.to_string(), v)))
                .map(|(id, v)| SynthGroup {
                    fields: own(table, &format!("{part}.{id}"), Some(v)),
                    cycle: if part == "oscillators" { synth.oscillators.get(&id).map_or_else(Vec::new, |o| table_cycle(o, p, directory)) } else { Vec::new() },
                    name: id,
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    SynthView {
        patch: synth.patch.clone(),
        fields: own(describe::SYNTH, "", Some(&dump)),
        oscillators: groups(describe::OSCILLATOR, "oscillators"),
        filter: own(describe::SYNTH_FILTER, "filter", dump.get("filter")),
        envelopes: groups(describe::ENVELOPE, "envelopes"),
        lfos: groups(describe::LFO, "lfos"),
        macros: synth
            .macros
            .iter()
            .map(|(name, value)| {
                let f = describe::Field {
                    name: "macro",
                    label: "",
                    ..describe::MACRO
                };
                let mut field = synth_field(synth, &f, format!("macros.{name}"), Some(&Value::Float(*value)), driven, &samples);
                field.label = name.clone();
                field
            })
            .collect(),
        effects: effects(&synth.effects, effect_items, &[], driven, true),
        modulation: synth
            .modulation
            .iter()
            .map(|m| ModulationView {
                source: m.source.clone(),
                target: m.target.clone(),
                amount: m.amount,
                unit: aaw_model::rules::mod_target(synth, &m.target).map_or(String::new(), |t| t.unit().to_string()),
                field: mod_field(&m.target),
            })
            .collect(),
    }
}

/// The tracks that may key a compressor of `owner`: every other track.
fn keys(p: &Project, owner: Option<&str>) -> Vec<String> {
    p.tracks.iter().map(|t| t.id.clone()).filter(|id| Some(id.as_str()) != owner).collect()
}

fn items<'a>(node: &'a Node, key: &str) -> &'a [Item] {
    node.get(key).map(Node::items).unwrap_or(&[])
}

fn handle(items: &[Item], index: usize) -> u64 {
    items.get(index).map_or(0, |i| i.handle)
}

/// A sample's root note as a MIDI number.
pub fn root(p: &Project, sample: &str) -> Option<i32> {
    let note = p.samples.get(sample)?.root_note.as_deref().filter(|n| !n.is_empty())?;
    midi(note).ok().map(|n| n as i32)
}

/// The pad a MIDI track's sampler plays as the Sampler device: Some(None)
/// for an empty sampler, Some(the pad) for one pad on every note at its
/// pitch, and None for a sampler of several pads or no sampler at all.
pub fn device(track: &aaw_model::Track) -> Option<Option<(&str, &aaw_model::Pad)>> {
    let s = track.midi.as_ref()?.sampler()?;
    if s.pads.is_empty() && s.map.is_empty() {
        return Some(None);
    }
    let (name, pad) = s.pads.iter().next().filter(|_| s.pads.len() == 1)?;
    match &s.map[..] {
        [m] if m.low == 0 && m.high == 127 && m.pitched && &m.pad == name => Some(Some((name, pad))),
        _ => None,
    }
}

/// The sample a track's Sampler device plays, when it has one.
pub fn device_sample(track: &aaw_model::Track) -> Option<&str> {
    device(track).flatten().map(|(_, pad)| pad.sample.as_str())
}

/// A pad's field as the song holds it.
fn pad_value(pad: &aaw_model::Pad, name: &str) -> Option<Value> {
    Some(match name {
        "mode" => Value::str(pad.mode.as_str()),
        "gain_db" => Value::Float(pad.gain_db),
        "pan" => Value::Float(pad.pan),
        "transpose" => Value::Float(pad.transpose),
        "start_seconds" => Value::Float(pad.start_seconds),
        "end_seconds" => Value::Float(pad.end_seconds?),
        "attack_ms" => Value::Float(pad.attack_ms),
        "release_ms" => Value::Float(pad.release_ms),
        "reverse" => Value::Bool(pad.reverse),
        _ => return None,
    })
}

/// A track's Sampler device, with its file at `seconds` long and `file` its
/// identity, when the file can be read.
fn sampler(p: &Project, track: &aaw_model::Track, file: Option<(u64, f64)>) -> Option<SamplerView> {
    let loaded = device(track)?;
    let (identity, seconds) = file.unwrap_or((0, 0.0));
    let Some((name, pad)) = loaded else {
        return Some(SamplerView {
            pad: None,
            sample: None,
            path: String::new(),
            fields: Vec::new(),
            root: None,
            root_text: String::new(),
            file: 0,
            seconds: 0.0,
            start_seconds: 0.0,
            end_seconds: 0.0,
        });
    };
    let fields = describe::PAD
        .iter()
        .map(|f| {
            let value = pad_value(pad, f.name);
            FieldView {
                name: f.name.to_string(),
                label: f.label.to_string(),
                kind: match f.kind {
                    Kind::Number => FieldKind::Number,
                    Kind::Choice => FieldKind::Choice,
                    Kind::Flag => FieldKind::Flag,
                    Kind::Integer => FieldKind::Integer,
                    Kind::Beats => FieldKind::Beats,
                    Kind::Track => FieldKind::Track,
                },
                value: field_value(value.as_ref()),
                min: f.min,
                // The start and the end go as far as the file does.
                max: if f.max > 0.0 { f.max } else { seconds },
                unit: f.unit.to_string(),
                log: f.log,
                choices: f.choices.iter().map(|c| c.to_string()).collect(),
                optional: f.default == Initial::Absent,
                initial: initial(if f.default == Initial::Absent { Initial::Number(seconds) } else { f.default }),
                live: !f.structural,
                param: None,
                lane: None,
                band: None,
            }
        })
        .collect();
    let root_text = p.samples.get(&pad.sample).and_then(|s| s.root_note.clone()).unwrap_or_default();
    Some(SamplerView {
        pad: Some(name.to_string()),
        sample: Some(pad.sample.clone()),
        path: p.samples.get(&pad.sample).map(|s| s.path.clone()).unwrap_or_default(),
        fields,
        root: root(p, &pad.sample),
        root_text,
        file: identity,
        seconds,
        start_seconds: pad.start_seconds,
        end_seconds: match pad.end_seconds {
            Some(end) if identity == 0 => end,
            Some(end) => end.min(seconds),
            None => seconds,
        },
    })
}

/// A step's level as a row writes it: `.`, a digit or `x`.
pub fn step_level(c: char) -> u8 {
    match c {
        'x' => 10,
        _ => c.to_digit(10).map_or(0, |d| d as u8),
    }
}

fn patterns(p: &Project, tree: &Node) -> Vec<PatternView> {
    let float = |x: &BigRational| x.to_f64().unwrap_or(0.0);
    let nodes = tree.get("patterns");
    p.patterns
        .iter()
        .map(|(name, pattern)| {
            let event_items = nodes.and_then(|n| n.get(name)).map(|n| items(n, "events")).unwrap_or(&[]);
            let (length, grid) = (pattern.length_exact(), pattern.grid_exact());
            let steps = &length / &grid;
            PatternView {
                name: name.clone(),
                length_beats: float(&length),
                length_text: pattern.length_beats.text(),
                grid: float(&grid),
                grid_text: pattern.grid.text(),
                swing: pattern.swing,
                steps: steps.is_integer().then(|| steps.to_integer().to_u32()).flatten(),
                rows: pattern
                    .steps
                    .iter()
                    .map(|(pad, row)| StepRow {
                        pad: pad.clone(),
                        cells: step_cells(row).into_iter().map(step_level).collect(),
                    })
                    .collect(),
                events: pattern
                    .events
                    .iter()
                    .enumerate()
                    .map(|(i, e)| EventView {
                        key: handle(event_items, i),
                        at: float(&e.at_exact()),
                        at_text: e.at.text(),
                        pad: e.pad.clone(),
                        velocity: e.velocity.clamp(0, 127) as u32,
                        note: e.note.clone().filter(|n| !n.is_empty()),
                        pitch: e.note.as_deref().and_then(|n| midi(n).ok()).map(|n| n as i32),
                        duration: e.duration_exact().map(|d| float(&d)),
                        duration_text: e.duration.as_ref().map(|d| d.text()),
                        transpose: e.transpose,
                    })
                    .collect(),
                clips: p.tracks.iter().flat_map(|t| &t.clips).filter(|c| &c.pattern == name).count() as u32,
            }
        })
        .collect()
}

/// Seconds of an audio clip's file that go by in a beat of the song. A clip
/// with a tempo of its own follows the song's, so a beat of the song is a beat
/// of its file.
pub fn seconds_per_beat(clip: &AudioClip, tempo: f64) -> f64 {
    60.0 / clip.source_bpm.unwrap_or(tempo)
}

/// How long an audio clip sounds after it leaves, in beats: its fade out, as
/// far as its file goes. `file` is the file's length in seconds, when known.
pub fn tail_beats(clip: &AudioClip, tempo: f64, file: Option<f64>) -> f64 {
    let fade = clip.fade_out_ms / 1000.0 * tempo / 60.0;
    match (clip.source_end_seconds, file) {
        (Some(end), Some(file)) => fade.min(((file - end) / seconds_per_beat(clip, tempo)).max(0.0)),
        (Some(_), None) => fade,
        // It plays to its file's end, and fades over the last of it.
        (None, _) => 0.0,
    }
}

/// The arrangement of a revision of the song in `directory`.
pub fn arrangement(doc: &Doc, revision: u64, files: &Files, directory: &Path) -> Arrangement {
    let p = &doc.project;
    let tree = doc.tree();
    let float = |x: num_rational::BigRational| x.to_f64().unwrap_or(0.0);
    let track_items = items(&tree, "tracks");
    // Each file once, however many clips play it.
    let mut used: Vec<FileView> = Vec::new();
    let mut file = |sample: &str| -> Option<(u64, f64)> {
        let view = files.of(directory, p.samples.get(sample)?)?;
        let found = (view.identity, view.seconds);
        if used.iter().all(|f| f.identity != view.identity) {
            used.push(view);
        }
        Some(found)
    };
    let tracks: Vec<TrackView> = p
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let node = track_items.get(i).map(|item| &item.node);
            let clip_items = node.map(|n| items(n, "clips")).unwrap_or(&[]);
            let audio_items = node.map(|n| items(n, "audio")).unwrap_or(&[]);
            let sends: Vec<String> = t.sends.iter().map(|s| s.to.clone()).collect();
            let (effects, lanes, lane_targets, synth) = channel(Owner::Track(t), node, &keys(p, Some(&t.id)), &sends, p, directory);
            TrackView {
                key: handle(track_items, i),
                id: t.id.clone(),
                gain_db: t.gain_db,
                pan: t.pan,
                mute: t.mute,
                solo: t.solo,
                effects,
                sends: t
                    .sends
                    .iter()
                    .map(|s| SendView {
                        to: s.to.clone(),
                        gain_db: s.gain_db,
                    })
                    .collect(),
                lanes,
                lane_targets,
                pads: t
                    .midi
                    .as_ref()
                    .and_then(|m| m.sampler())
                    .map_or(&t.pads, |s| &s.pads)
                    .iter()
                    .map(|(name, pad)| PadView {
                        name: name.clone(),
                        sample: pad.sample.clone(),
                        gain_db: pad.gain_db,
                        gate: pad.mode == aaw_model::PadMode::Gate,
                        root: root(p, &pad.sample),
                    })
                    .collect(),
                midi: t.midi.is_some(),
                instrument: t.midi.as_ref().and_then(|m| m.instrument.as_ref()).map(|i| i.kind().to_string()),
                sampler: sampler(p, t, device_sample(t).and_then(&mut file)),
                synth,
                map: t
                    .midi
                    .as_ref()
                    .and_then(|m| m.sampler())
                    .map(|s| &s.map[..])
                    .unwrap_or(&[])
                    .iter()
                    .map(|m| NoteMapView {
                        low: m.low as i32,
                        high: m.high as i32,
                        pad: m.pad.clone(),
                        pitched: m.pitched,
                    })
                    .collect(),
                note_clips: t
                    .midi
                    .iter()
                    .flat_map(|m| &m.clips)
                    .enumerate()
                    .map(|(j, c)| {
                        let key = handle(clip_items, j);
                        let note_items = clip_items.get(j).map(|item| items(&item.node, "notes")).unwrap_or(&[]);
                        NoteClipView {
                            key,
                            reference: aaw_host::tree::handle_text(key),
                            id: c.id.clone(),
                            at: float(c.at_exact()),
                            length_beats: float(c.length_exact()),
                            loop_beats: c.loop_exact().map(&float),
                            notes: c
                                .notes
                                .iter()
                                .enumerate()
                                .map(|(k, n)| NoteView {
                                    key: handle(note_items, k),
                                    id: n.id.clone(),
                                    pitch: n.pitch as i32,
                                    at: float(n.at_exact()),
                                    duration: float(n.duration_exact()),
                                    velocity: n.velocity.clamp(0, 127) as u32,
                                    at_text: n.at.text(),
                                    duration_text: n.duration.text(),
                                })
                                .collect(),
                        }
                    })
                    .collect(),
                clips: t
                    .clips
                    .iter()
                    .enumerate()
                    .map(|(j, c)| {
                        let key = handle(clip_items, j);
                        ClipView {
                            key,
                            reference: aaw_host::tree::handle_text(key),
                            pattern: c.pattern.clone(),
                            at: float(c.at_exact()),
                            pattern_beats: p.patterns.get(&c.pattern).map_or(0.0, |x| float(x.length_exact())),
                            repeats: c.repeats.max(0) as u32,
                        }
                    })
                    .collect(),
                audio: t
                    .audio
                    .iter()
                    .enumerate()
                    .map(|(j, c)| {
                        let key = handle(audio_items, j);
                        let per_beat = seconds_per_beat(c, p.session.tempo);
                        let (identity, seconds) = file(&c.sample).unwrap_or((0, 0.0));
                        // A file that cannot be read has no length to end on.
                        let end = c.source_end_seconds.map_or(seconds, |end| if identity == 0 { end } else { end.min(seconds) });
                        AudioClipView {
                            key,
                            reference: aaw_host::tree::handle_text(key),
                            sample: c.sample.clone(),
                            at: float(c.at_exact()),
                            length_beats: c.length_exact().map_or_else(|| ((end - c.source_start_seconds) / per_beat).max(0.0), &float),
                            loop_beats: c.loop_exact().map(&float),
                            tail_beats: tail_beats(c, p.session.tempo, (identity != 0).then_some(seconds)),
                            source_start_seconds: c.source_start_seconds,
                            source_end_seconds: end,
                            seconds_per_beat: per_beat,
                            gain_db: c.gain_db,
                            fade_in_ms: c.fade_in_ms,
                            fade_out_ms: c.fade_out_ms,
                            fade_curve: c.fade_curve.as_str().to_string(),
                            source_bpm: c.source_bpm,
                            stretch: c.stretch.as_str().to_string(),
                            file: identity,
                        }
                    })
                    .collect(),
            }
        })
        .collect();
    let return_items = items(&tree, "returns");
    let returns = p
        .returns
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let node = return_items.get(i).map(|item| &item.node);
            let (effects, lanes, lane_targets, _) = channel(Owner::Return(r), node, &keys(p, None), &[], p, directory);
            ReturnView {
                key: handle(return_items, i),
                id: r.id.clone(),
                gain_db: r.gain_db,
                pan: r.pan,
                mute: r.mute,
                effects,
                lanes,
                lane_targets,
            }
        })
        .collect();
    let section_items = items(&tree, "sections");
    let sections = p
        .sections
        .iter()
        .enumerate()
        .map(|(i, s)| SectionView {
            key: handle(section_items, i),
            id: s.id.clone(),
            at: float(s.at_exact()),
            length_beats: float(s.length_exact()),
        })
        .collect();
    // A master compressor cannot be keyed.
    let (master_effects, master_lanes, master_targets, _) = channel(Owner::Master(&p.master), tree.get("master"), &[], &[], p, directory);
    Arrangement {
        revision,
        project_sha256: doc.sha.clone(),
        title: p.session.title.clone(),
        tempo: p.session.tempo,
        time_signature: p.session.meter().text(),
        bar_beats: p.session.meter().bar_f64(),
        beat_unit: p.session.meter().beat_f64(),
        length_beats: float(p.session.length_exact()),
        tracks,
        returns,
        master: MasterView {
            gain_db: p.session.master_gain_db,
            effects: master_effects,
            lanes: master_lanes,
            lane_targets: master_targets,
        },
        sections,
        patterns: patterns(p, &tree),
        files: used,
    }
}

fn same(a: Option<&Node>, b: Option<&Node>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => py_eq(&a.value(), &b.value()),
        (None, None) => true,
        _ => false,
    }
}

/// A map node without one of its keys.
fn without(node: &Node, key: &str) -> Node {
    let mut n = node.clone();
    if let Some(m) = n.map_mut() {
        m.shift_remove(key);
    }
    n
}

/// Compares two lists by handle. `changed` decides for items in both.
fn list_delta(part: Part, old: &[Item], new: &[Item], changed: impl Fn(&Item, &Item) -> bool, out: &mut Vec<Touch>) {
    let before: HashMap<u64, (usize, &Item)> = old.iter().enumerate().map(|(i, item)| (item.handle, (i, item))).collect();
    let after: HashSet<u64> = new.iter().map(|i| i.handle).collect();
    for (i, item) in new.iter().enumerate() {
        let delta = match before.get(&item.handle) {
            None => Delta::Added,
            Some((j, o)) if *j != i || changed(o, item) => Delta::Changed,
            Some(_) => continue,
        };
        out.push(Touch {
            part,
            key: item.handle,
            delta,
        });
    }
    for item in old.iter().filter(|i| !after.contains(&i.handle)) {
        out.push(Touch {
            part,
            key: item.handle,
            delta: Delta::Removed,
        });
    }
}

/// Every clip by handle, pattern clips and audio clips alike, with the handle
/// of its track.
fn clips(tree: &Node) -> Vec<(u64, &Item)> {
    items(tree, "tracks")
        .iter()
        .flat_map(|t| items(&t.node, "clips").iter().chain(items(&t.node, "audio")).map(move |c| (t.handle, c)))
        .collect()
}

/// The objects that differ between two revisions, for highlighting. A track's
/// clips count separately from the track. A clip also counts as changed when
/// its pattern changed, since it then plays something else.
pub fn touched(old: &Doc, new: &Doc) -> Vec<Touch> {
    let (a, b) = (old.tree(), new.tree());
    let mut out = Vec::new();
    let whole = |part: Part| Touch {
        part,
        key: 0,
        delta: Delta::Changed,
    };
    let session = |t: &Node| t.get("session").map(|s| without(s, "master_gain_db"));
    if !same(session(&a).as_ref(), session(&b).as_ref()) {
        out.push(whole(Part::Session));
    }
    let master_gain = |t: &'_ Node| t.get("session").and_then(|s| s.get("master_gain_db")).cloned();
    if !same(a.get("master"), b.get("master")) || !same(master_gain(&a).as_ref(), master_gain(&b).as_ref()) {
        out.push(whole(Part::Master));
    }
    let differs = |o: &Item, n: &Item| !same(Some(&o.node), Some(&n.node));
    list_delta(
        Part::Track,
        items(&a, "tracks"),
        items(&b, "tracks"),
        |o, n| {
            let rest = |node: &Node| without(&without(node, "clips"), "audio");
            !same(Some(&rest(&o.node)), Some(&rest(&n.node)))
        },
        &mut out,
    );

    let patterns = |t: &Node, name: &str| t.get("patterns").and_then(|p| p.get(name)).cloned();
    let (old_clips, new_clips) = (clips(&a), clips(&b));
    let before: HashMap<u64, (u64, &Item)> = old_clips.iter().map(|(track, c)| (c.handle, (*track, *c))).collect();
    let after: HashSet<u64> = new_clips.iter().map(|(_, c)| c.handle).collect();
    for (track, clip) in &new_clips {
        let delta = match before.get(&clip.handle) {
            None => Delta::Added,
            Some((old_track, o)) => {
                let pattern = clip.node.field("pattern").unwrap_or_default();
                let replayed = !same(patterns(&a, pattern).as_ref(), patterns(&b, pattern).as_ref());
                if old_track != track || differs(o, clip) || replayed {
                    Delta::Changed
                } else {
                    continue;
                }
            }
        };
        out.push(Touch {
            part: Part::Clip,
            key: clip.handle,
            delta,
        });
    }
    for (_, clip) in old_clips.iter().filter(|(_, c)| !after.contains(&c.handle)) {
        out.push(Touch {
            part: Part::Clip,
            key: clip.handle,
            delta: Delta::Removed,
        });
    }

    list_delta(Part::Return, items(&a, "returns"), items(&b, "returns"), differs, &mut out);
    list_delta(Part::Section, items(&a, "sections"), items(&b, "sections"), differs, &mut out);
    out
}
