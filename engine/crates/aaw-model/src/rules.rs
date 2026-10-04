//! Rules that span models: note names, automation targets and the references
//! a project is checked for, with the messages the Python model gave.

use crate::beat::{float_fraction, parse_fraction};
use crate::pyfmt::{float_repr, format_g};
use crate::schema::{Effect, Lane, PadMode, Project, Return, Track};
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::LazyLock;

static NOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\A([A-Ga-g])([#b]?)(-?[0-9]+)\z").expect("note pattern"));

/// Middle C, C4: the note a pitched map entry plays a sample as it is on when
/// the sample has no root note.
pub const MIDDLE_C: i64 = 60;

/// A note name with octave, such as C2 or F#1, as a MIDI number.
pub fn midi(note: &str) -> Result<i64, String> {
    let caps = NOTE
        .captures(note)
        .ok_or_else(|| format!("Invalid note {}; use e.g. C2 or F#1", crate::pyfmt::str_repr(note)))?;
    let key = match caps[1].to_ascii_uppercase().as_str() {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        _ => 11,
    };
    let accidental = match &caps[2] {
        "#" => 1,
        "b" => -1,
        _ => 0,
    };
    let octave: i64 = caps[3]
        .parse()
        .map_err(|_| format!("Note outside MIDI range: {note}"))?;
    let n = (octave + 1) * 12 + key + accidental;
    if !(0..=127).contains(&n) {
        return Err(format!("Note outside MIDI range: {note}"));
    }
    Ok(n)
}

/// The name `midi` reads back as a MIDI number, with sharps: 30 is F#1.
pub fn note_name(midi: i64) -> Result<String, String> {
    if !(0..=127).contains(&midi) {
        return Err(format!("Note outside MIDI range: {midi}"));
    }
    const KEYS: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    Ok(format!("{}{}", KEYS[(midi % 12) as usize], midi / 12 - 1))
}

/// How a parameter's values interpolate between points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    Linear,
    Log,
}

/// A numeric field's limits; an open end excludes its bound.
#[derive(Clone, Copy, Debug)]
pub struct Range {
    pub low: f64,
    pub high: f64,
    pub low_open: bool,
    pub high_open: bool,
}

impl Range {
    const fn closed(low: f64, high: f64) -> Range {
        Range {
            low,
            high,
            low_open: false,
            high_open: false,
        }
    }

    pub fn contains(&self, value: f64) -> bool {
        let above = if self.low_open {
            value > self.low
        } else {
            value >= self.low
        };
        let below = if self.high_open {
            value < self.high
        } else {
            value <= self.high
        };
        above && below
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let low = format_g(self.low, 6) + if self.low_open { " (exclusive)" } else { "" };
        let high = format_g(self.high, 6) + if self.high_open { " (exclusive)" } else { "" };
        write!(f, "{low} to {high}")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TargetKind {
    Channel,
    Send(String),
    Effect { index: usize, band: Option<usize> },
    /// A field of a MIDI track's synth, `field` being its path in the patch,
    /// such as `filter.cutoff_hz` or `macros.tone`.
    Instrument,
}

/// A resolved lane target. `key` is canonical: effects are addressed by index.
#[derive(Clone, Debug)]
pub struct Target {
    pub kind: TargetKind,
    pub field: String,
    pub domain: Domain,
    pub range: Range,
}

impl Target {
    /// Parameter name within its effect, e.g. cutoff_hz or bands.0.gain_db.
    pub fn name(&self) -> String {
        match &self.kind {
            TargetKind::Effect { band: Some(b), .. } => format!("bands.{b}.{}", self.field),
            _ => self.field.clone(),
        }
    }

    pub fn key(&self) -> String {
        match &self.kind {
            TargetKind::Send(to) => format!("sends.{to}.{}", self.field),
            TargetKind::Effect { index, .. } => format!("effects.{index}.{}", self.name()),
            TargetKind::Instrument => format!("instrument.{}", self.field),
            TargetKind::Channel => self.field.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// The Synth's matrix and automatable fields

/// What an entry of the Synth's matrix moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModTarget {
    /// Every oscillator's pitch, in semitones.
    Pitch,
    /// One oscillator's pitch, level, pan or pulse width; the index is its
    /// place among the oscillators.
    OscPitch(usize),
    OscLevel(usize),
    OscPan(usize),
    OscPulseWidth(usize),
    /// The filter's cutoff in octaves, its resonance in points and its drive
    /// in dB.
    Cutoff,
    Resonance,
    Drive,
    /// An envelope's times in octaves (doublings) and its sustain in points,
    /// taken when the note starts.
    EnvAttack(usize),
    EnvDecay(usize),
    EnvSustain(usize),
    EnvRelease(usize),
}

impl ModTarget {
    /// The unit of an entry's amount.
    pub fn unit(self) -> &'static str {
        match self {
            ModTarget::Pitch | ModTarget::OscPitch(_) => "semitones",
            ModTarget::OscLevel(_) | ModTarget::Drive => "dB",
            ModTarget::OscPan(_) => "pan",
            ModTarget::OscPulseWidth(_) | ModTarget::Resonance | ModTarget::EnvSustain(_) => "points",
            ModTarget::Cutoff | ModTarget::EnvAttack(_) | ModTarget::EnvDecay(_) | ModTarget::EnvRelease(_) => "octaves",
        }
    }
}

/// Where an entry of the Synth's matrix takes its value from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModSource {
    /// An envelope, 0 to 1; the index is its place among the envelopes.
    Envelope(usize),
    /// An LFO, -1 to 1.
    Lfo(usize),
    /// The note's velocity, 0 to 1.
    Velocity,
    /// The note, 0 at middle C and 1 an octave up.
    Note,
    /// One value a note, 0 to 1, from the seed.
    Random,
    /// A macro, 0 to 1.
    Macro(usize),
}

/// The matrix's targets with their units, as `describe synth` lists them:
/// `ID` stands for an oscillator's or an envelope's ID.
pub const MOD_TARGETS: &[(&str, &str)] = &[
    ("pitch", "semitones"),
    ("oscillators.ID.pitch", "semitones"),
    ("oscillators.ID.level_db", "dB"),
    ("oscillators.ID.pan", "pan"),
    ("oscillators.ID.pulse_width", "points"),
    ("filter.cutoff_hz", "octaves"),
    ("filter.resonance_percent", "points"),
    ("filter.drive_db", "dB"),
    ("envelopes.ID.attack_ms", "octaves"),
    ("envelopes.ID.decay_ms", "octaves"),
    ("envelopes.ID.sustain_percent", "points"),
    ("envelopes.ID.release_ms", "octaves"),
];

/// The matrix's sources, as `describe synth` lists them.
pub const MOD_SOURCES: &[(&str, &str)] = &[
    ("ENVELOPE", "an envelope by its ID, 0 to 1"),
    ("LFO", "an LFO by its ID, -1 to 1"),
    ("velocity", "the note's velocity, 0 to 1"),
    ("note", "the note, 0 at middle C and 1 an octave up"),
    ("random", "one value a note, 0 to 1, from seed"),
    ("macros.NAME", "a macro, 0 to 1"),
];

/// The names the matrix keeps for itself, which no envelope or LFO may have.
const RESERVED_SOURCES: [&str; 3] = ["velocity", "note", "random"];

/// What a source names in a synth.
pub fn mod_source(synth: &crate::schema::Synth, source: &str) -> Result<ModSource, String> {
    if let Some(name) = source.strip_prefix("macros.") {
        return synth
            .macros
            .get_index_of(name)
            .map(ModSource::Macro)
            .ok_or_else(|| format!("source {source}: no macro {name}"));
    }
    match source {
        "velocity" => return Ok(ModSource::Velocity),
        "note" => return Ok(ModSource::Note),
        "random" => return Ok(ModSource::Random),
        _ => {}
    }
    if let Some(i) = synth.envelopes.get_index_of(source) {
        return Ok(ModSource::Envelope(i));
    }
    if let Some(i) = synth.lfos.get_index_of(source) {
        return Ok(ModSource::Lfo(i));
    }
    Err(format!("source {source}: no envelope, LFO or macro of that name; sources are an envelope or LFO by ID, velocity, note, random or macros.NAME"))
}

/// What a target names in a synth.
pub fn mod_target(synth: &crate::schema::Synth, target: &str) -> Result<ModTarget, String> {
    let parts: Vec<&str> = target.split('.').collect();
    let unknown = || {
        let names: Vec<&str> = MOD_TARGETS.iter().map(|(n, _)| *n).collect();
        format!("target {target}: not a target; targets are {}", names.join(", "))
    };
    match parts.as_slice() {
        ["pitch"] => Ok(ModTarget::Pitch),
        ["oscillators", id, field] => {
            let i = synth.oscillators.get_index_of(*id).ok_or_else(|| format!("target {target}: no oscillator {id}"))?;
            match *field {
                "pitch" => Ok(ModTarget::OscPitch(i)),
                "level_db" => Ok(ModTarget::OscLevel(i)),
                "pan" => Ok(ModTarget::OscPan(i)),
                "pulse_width" => Ok(ModTarget::OscPulseWidth(i)),
                _ => Err(unknown()),
            }
        }
        ["filter", field] => match *field {
            "cutoff_hz" => Ok(ModTarget::Cutoff),
            "resonance_percent" => Ok(ModTarget::Resonance),
            "drive_db" => Ok(ModTarget::Drive),
            _ => Err(unknown()),
        },
        ["envelopes", id, field] => {
            let i = synth.envelopes.get_index_of(*id).ok_or_else(|| format!("target {target}: no envelope {id}"))?;
            match *field {
                "attack_ms" => Ok(ModTarget::EnvAttack(i)),
                "decay_ms" => Ok(ModTarget::EnvDecay(i)),
                "sustain_percent" => Ok(ModTarget::EnvSustain(i)),
                "release_ms" => Ok(ModTarget::EnvRelease(i)),
                _ => Err(unknown()),
            }
        }
        _ => Err(unknown()),
    }
}

/// A synth's parts name each other: envelope and LFO IDs are sources and
/// may not be the matrix's own names or each other's, and every entry's
/// source and target exist.
pub fn synth_references(synth: &crate::schema::Synth) -> Result<(), String> {
    for id in synth.envelopes.keys().chain(synth.lfos.keys()) {
        if RESERVED_SOURCES.contains(&id.as_str()) {
            return Err(format!("{id} is a modulation source of its own; name the envelope or LFO something else"));
        }
    }
    if let Some(id) = synth.lfos.keys().find(|id| synth.envelopes.contains_key(*id)) {
        return Err(format!("An envelope and an LFO are both named {id}; the matrix names them by ID"));
    }
    for (i, m) in synth.modulation.iter().enumerate() {
        mod_source(synth, &m.source).map_err(|e| format!("modulation {i}: {e}"))?;
        mod_target(synth, &m.target).map_err(|e| format!("modulation {i}: {e}"))?;
    }
    Ok(())
}

/// An automatable field of a synth by its path in the patch, such as
/// `filter.cutoff_hz`, `oscillators.a.level_db` or `macros.tone`: its
/// interpolation domain and range.
pub fn synth_param(synth: &crate::schema::Synth, path: &str) -> Result<(Domain, Range), String> {
    use crate::describe::{self, Kind};
    let parts: Vec<&str> = path.split('.').collect();
    let field = |table: &'static [describe::Field], name: &str, what: &str| -> Result<(Domain, Range), String> {
        let f = table
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| format!("{path}: {what} has no field {name}"))?;
        if f.kind != Kind::Number || f.structural {
            let names: Vec<&str> = table.iter().filter(|f| f.kind == Kind::Number && !f.structural).map(|f| f.name).collect();
            return Err(format!("{path}: {name} cannot be automated; automatable: {}", names.join(", ")));
        }
        Ok((if f.log { Domain::Log } else { Domain::Linear }, Range::closed(f.min, f.max)))
    };
    match parts.as_slice() {
        [name] if synth.macros.contains_key(*name) => Err(format!("{path}: a macro is automated as macros.{name}")),
        [name] => field(describe::SYNTH, name, "the synth"),
        ["oscillators", id, name] => {
            if !synth.oscillators.contains_key(*id) {
                return Err(format!("{path}: no oscillator {id}"));
            }
            field(describe::OSCILLATOR, name, "an oscillator")
        }
        ["filter", name] => field(describe::SYNTH_FILTER, name, "the filter"),
        ["envelopes", id, name] => {
            if !synth.envelopes.contains_key(*id) {
                return Err(format!("{path}: no envelope {id}"));
            }
            field(describe::ENVELOPE, name, "an envelope")
        }
        ["lfos", id, name] => {
            if !synth.lfos.contains_key(*id) {
                return Err(format!("{path}: no LFO {id}"));
            }
            field(describe::LFO, name, "an LFO")
        }
        ["macros", name] => {
            if !synth.macros.contains_key(*name) {
                return Err(format!("{path}: no macro {name}"));
            }
            Ok((Domain::Linear, Range::closed(0.0, 100.0)))
        }
        _ => Err(format!("{path}: unknown synth field; see daw describe synth")),
    }
}

/// Every automatable path of a synth, in the patch's order.
pub fn synth_params(synth: &crate::schema::Synth) -> Vec<String> {
    use crate::describe::{self, Kind};
    let names = |table: &'static [describe::Field]| table.iter().filter(|f| f.kind == Kind::Number && !f.structural).map(|f| f.name);
    let mut out: Vec<String> = names(describe::SYNTH).map(str::to_string).collect();
    for id in synth.oscillators.keys() {
        out.extend(names(describe::OSCILLATOR).map(|n| format!("oscillators.{id}.{n}")));
    }
    out.extend(names(describe::SYNTH_FILTER).map(|n| format!("filter.{n}")));
    for id in synth.envelopes.keys() {
        out.extend(names(describe::ENVELOPE).map(|n| format!("envelopes.{id}.{n}")));
    }
    for id in synth.lfos.keys() {
        out.extend(names(describe::LFO).map(|n| format!("lfos.{id}.{n}")));
    }
    out.extend(synth.macros.keys().map(|n| format!("macros.{n}")));
    out
}

/// A synth field's value by its path, for what shows or automates it.
pub fn synth_value(synth: &crate::schema::Synth, path: &str) -> Option<f64> {
    let number = |v: Option<&crate::value::Value>| match v {
        Some(crate::value::Value::Float(x)) => Some(*x),
        Some(crate::value::Value::Int(n)) => n.to_f64(),
        _ => None,
    };
    let parts: Vec<&str> = path.split('.').collect();
    match parts.as_slice() {
        [name] => number(synth.dump(false).get(name)),
        ["macros", name] => synth.macros.get(*name).copied(),
        [group, id, name] => number(synth.dump(false).get(group)?.get(id)?.get(name)),
        ["filter", name] => number(synth.filter.dump(false).get(name)),
        _ => None,
    }
}

/// A channel that owns effects and automation lanes.
#[derive(Clone, Copy)]
pub enum Owner<'a> {
    Track(&'a Track),
    Return(&'a Return),
    Master(&'a crate::schema::Master),
}

impl<'a> Owner<'a> {
    pub fn name(&self) -> &'a str {
        match self {
            Owner::Track(t) => &t.id,
            Owner::Return(r) => &r.id,
            Owner::Master(_) => "master",
        }
    }

    pub fn effects(&self) -> &'a [Effect] {
        match self {
            Owner::Track(t) => &t.effects,
            Owner::Return(r) => &r.effects,
            Owner::Master(m) => &m.effects,
        }
    }

    pub fn automation(&self) -> &'a [Lane] {
        match self {
            Owner::Track(t) => &t.automation,
            Owner::Return(r) => &r.automation,
            Owner::Master(m) => &m.automation,
        }
    }
}

/// Automatable effect fields with their interpolation domain, in declared order.
pub fn effect_params(kind: &str) -> &'static [(&'static str, Domain)] {
    match kind {
        "filter" => &[("cutoff_hz", Domain::Log)],
        "eq" => &[("freq_hz", Domain::Log), ("gain_db", Domain::Linear), ("q", Domain::Log)],
        "compressor" => &[("threshold_db", Domain::Linear), ("makeup_db", Domain::Linear)],
        "delay" => &[("feedback_percent", Domain::Linear), ("mix_percent", Domain::Linear)],
        "reverb" => &[("mix_percent", Domain::Linear)],
        _ => &[],
    }
}

fn effect_range(kind: &str, field: &str) -> Range {
    match (kind, field) {
        ("filter", "cutoff_hz") => Range::closed(10.0, 20000.0),
        ("eq", "freq_hz") => Range::closed(20.0, 20000.0),
        ("eq", "gain_db") => Range::closed(-24.0, 24.0),
        ("eq", "q") => Range::closed(0.1, 18.0),
        ("compressor", "threshold_db") => Range::closed(-60.0, 0.0),
        ("compressor", "makeup_db") => Range::closed(-24.0, 24.0),
        ("delay", "feedback_percent") => Range::closed(0.0, 95.0),
        (_, "mix_percent") => Range::closed(0.0, 100.0),
        _ => unreachable!("not automatable: {kind}.{field}"),
    }
}

fn is_ascii_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// A lane's target: resolves gain_db, pan, sends.RETURN.gain_db,
/// effects.REF.FIELD or effects.REF.bands.N.FIELD for an owner.
pub fn target(owner: Owner, param: &str) -> Result<Target, String> {
    let parts: Vec<&str> = param.split('.').collect();
    let channel = |field: &str, range: Range| Target {
        kind: TargetKind::Channel,
        field: field.to_string(),
        domain: Domain::Linear,
        range,
    };
    match owner {
        Owner::Master(_) => {
            if parts == ["gain_db"] {
                return Ok(channel("gain_db", Range::closed(-96.0, 24.0)));
            }
        }
        _ => {
            if parts.len() == 1 && parts[0] == "gain_db" {
                return Ok(channel("gain_db", Range::closed(-96.0, 24.0)));
            }
            if parts.len() == 1 && parts[0] == "pan" {
                return Ok(channel("pan", Range::closed(-1.0, 1.0)));
            }
        }
    }
    if let Owner::Track(track) = owner {
        if parts[0] == "instrument" {
            let synth = track
                .midi
                .as_ref()
                .and_then(|m| m.synth())
                .ok_or_else(|| format!("{param}: {} has no synth to automate", track.id))?;
            let path = param.strip_prefix("instrument.").filter(|p| !p.is_empty()).ok_or_else(|| format!("{param}: expected instrument.FIELD"))?;
            let (domain, range) = synth_param(synth, path)?;
            return Ok(Target {
                kind: TargetKind::Instrument,
                field: path.to_string(),
                domain,
                range,
            });
        }
        if parts[0] == "sends" && parts.len() == 3 {
            if parts[2] != "gain_db" {
                return Err(format!("{param}: only a send's gain_db can be automated"));
            }
            if !track.sends.iter().any(|s| s.to == parts[1]) {
                return Err(format!("{param}: no send to {}", parts[1]));
            }
            return Ok(Target {
                kind: TargetKind::Send(parts[1].to_string()),
                field: "gain_db".into(),
                domain: Domain::Linear,
                range: Range::closed(-96.0, 12.0),
            });
        }
    }
    if parts[0] == "effects" && parts.len() >= 3 {
        let effects = owner.effects();
        let by_index = is_ascii_digits(parts[1])
            .then(|| parts[1].parse::<usize>().ok())
            .flatten()
            .filter(|i| *i < effects.len());
        let index = match by_index {
            Some(i) => i,
            None => effects
                .iter()
                .position(|e| e.id() == Some(parts[1]))
                .ok_or_else(|| format!("{param}: no effect with id or index {}", parts[1]))?,
        };
        let spec = &effects[index];
        let allowed = effect_params(spec.kind());
        let mut band = None;
        if let Effect::Eq(eq) = spec {
            if parts.len() != 5 || parts[2] != "bands" || !is_ascii_digits(parts[3]) {
                return Err(format!(
                    "{param}: address eq bands as effects.REF.bands.N.FIELD"
                ));
            }
            let b: usize = parts[3].parse().unwrap_or(usize::MAX);
            if b >= eq.bands.len() {
                return Err(format!("{param}: eq has {} bands", eq.bands.len()));
            }
            band = Some(b);
        } else if parts.len() != 3 {
            return Err(format!("{param}: expected effects.REF.FIELD"));
        }
        let field = parts[parts.len() - 1];
        let Some((_, domain)) = allowed.iter().find(|(name, _)| *name == field) else {
            let names: Vec<&str> = allowed.iter().map(|(n, _)| *n).collect();
            let list = if names.is_empty() {
                "none".to_string()
            } else {
                names.join(", ")
            };
            return Err(format!(
                "{param}: {} {field} cannot be automated; automatable: {list}",
                spec.kind()
            ));
        };
        return Ok(Target {
            kind: TargetKind::Effect { index, band },
            field: field.to_string(),
            domain: *domain,
            range: effect_range(spec.kind(), field),
        });
    }
    Err(format!(
        "{param}: unknown automation target; use gain_db{}{} or effects.REF.FIELD",
        if matches!(owner, Owner::Master(_)) { "" } else { ", pan" },
        match owner {
            Owner::Track(t) if t.midi.as_ref().is_some_and(|m| m.synth().is_some()) => ", sends.RETURN.gain_db, instrument.FIELD",
            Owner::Track(_) => ", sends.RETURN.gain_db",
            _ => "",
        }
    ))
}

/// What `daw check` says of a song's synths: a macro no entry uses, more
/// notes stacked than the synth has voices, and a release the song's end
/// cuts short.
pub fn synth_warnings(p: &Project) -> Vec<String> {
    let mut out = Vec::new();
    let length = p.session.length_exact();
    for t in &p.tracks {
        let Some(midi) = &t.midi else { continue };
        let Some(synth) = midi.synth() else { continue };
        for name in synth.macros.keys() {
            if !synth.modulation.iter().any(|m| m.source == format!("macros.{name}")) {
                out.push(format!("{}: macro {name} moves nothing; the matrix has no entry with source macros.{name}", t.id));
            }
        }
        // The notes as they sound, in order: the most at once against the voices.
        let notes = crate::schedule::track_notes(p, midi);
        let mut edges: Vec<(i64, i32)> = notes.iter().flat_map(|n| [(n.start, 1), (n.end, -1)]).collect();
        edges.sort();
        let (mut now, mut most) = (0i64, 0i64);
        for (_, d) in edges {
            now += i64::from(d);
            most = most.max(now);
        }
        if most > synth.voices {
            out.push(format!(
                "{}: {most} notes sound at once and the synth has {} voices, so the oldest are cut off",
                t.id, synth.voices
            ));
        }
        let release = synth.envelopes.get("amp").map_or(0.0, |e| e.release_ms);
        let tail = release / 1000.0 * p.session.tempo / 60.0;
        if let Some(last) = notes.iter().map(|n| &n.at + &n.beats).max() {
            let ends = last + BigRational::from_float(tail).unwrap_or_default();
            if ends > length {
                out.push(format!(
                    "{}: the last note's release ends at beat {} and the song ends at {}, so the end fade cuts it",
                    t.id,
                    format_g(ends.to_f64().unwrap_or(f64::NAN), 4),
                    p.session.length_beats.text()
                ));
            }
        }
    }
    out
}

/// Every owner in document order: tracks, returns, then the master.
pub fn owners(p: &Project) -> Vec<Owner<'_>> {
    p.tracks
        .iter()
        .map(Owner::Track)
        .chain(p.returns.iter().map(Owner::Return))
        .chain(std::iter::once(Owner::Master(&p.master)))
        .collect()
}

/// `Project.references`: cross-references, graphs and session bounds.
pub fn references(p: &Project) -> Result<(), String> {
    let track_ids: Vec<&str> = p.tracks.iter().map(|t| t.id.as_str()).collect();
    let return_ids: Vec<&str> = p.returns.iter().map(|r| r.id.as_str()).collect();
    if track_ids.iter().collect::<HashSet<_>>().len() != track_ids.len() {
        return Err("Track IDs must be unique".into());
    }
    let all: HashSet<&str> = track_ids.iter().chain(&return_ids).copied().collect();
    if all.len() != track_ids.len() + return_ids.len() {
        return Err("Return IDs must be unique and distinct from track IDs".into());
    }
    let channels = p
        .tracks
        .iter()
        .map(|t| (t.id.as_str(), t.sidechains()))
        .chain(p.returns.iter().map(|r| (r.id.as_str(), r.sidechains())));
    for (id, sources) in channels {
        for source in sources {
            if return_ids.contains(&source) {
                return Err(format!("{id}: sidechain must name a track, not return {source}"));
            }
            if !track_ids.contains(&source) {
                return Err(format!("{id}: unknown sidechain track {source}"));
            }
            if source == id {
                return Err(format!("{id}: a track cannot sidechain itself"));
            }
        }
    }
    for t in &p.tracks {
        for s in &t.sends {
            if !return_ids.contains(&s.to.as_str()) {
                return Err(format!("{}: send to unknown return {}", t.id, s.to));
            }
        }
        let targets: HashSet<&str> = t.sends.iter().map(|s| s.to.as_str()).collect();
        if targets.len() != t.sends.len() {
            return Err(format!("{}: at most one send per return", t.id));
        }
    }
    let graph: HashMap<&str, Vec<&str>> = p.tracks.iter().map(|t| (t.id.as_str(), t.sidechains())).collect();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    fn visit<'a>(
        node: &'a str,
        graph: &HashMap<&'a str, Vec<&'a str>>,
        visiting: &mut HashSet<&'a str>,
        visited: &mut HashSet<&'a str>,
    ) -> Result<(), String> {
        if visiting.contains(node) {
            return Err(format!("Sidechain cycle through track {node}"));
        }
        if !visited.contains(node) {
            visiting.insert(node);
            for source in &graph[node] {
                visit(source, graph, visiting, visited)?;
            }
            visiting.remove(node);
            visited.insert(node);
        }
        Ok(())
    }
    for t in &p.tracks {
        visit(&t.id, &graph, &mut visiting, &mut visited)?;
    }
    if p.master.effects.iter().any(|e| e.sidechain().is_some()) {
        return Err("Master compressor cannot use a sidechain".into());
    }
    let tempo = float_fraction(p.session.tempo);
    let ms = parse_fraction("1/1000").ok().expect("fraction");
    let ten = BigRational::from_integer(10.into());
    for owner in owners(p) {
        for e in owner.effects() {
            if let Effect::Delay(d) = e {
                let seconds = d.time_exact() * BigRational::from_integer(60.into()) / &tempo;
                if !(ms <= seconds && seconds <= ten) {
                    return Err(format!(
                        "{}: delay time must be 1 ms to 10 s at the session tempo, not {} s",
                        owner.name(),
                        format_g(seconds.to_f64().unwrap_or(f64::NAN), 4)
                    ));
                }
            }
        }
    }
    let length = p.session.length_exact();
    for owner in owners(p) {
        let name = owner.name();
        let ids: Vec<&str> = owner.effects().iter().filter_map(Effect::id).filter(|i| !i.is_empty()).collect();
        if ids.iter().collect::<HashSet<_>>().len() != ids.len() {
            return Err(format!("{name}: effect IDs must be unique"));
        }
        let mut keys = HashSet::new();
        for lane in owner.automation() {
            let t = target(owner, &lane.param).map_err(|e| format!("{name}: {e}"))?;
            if !keys.insert(t.key()) {
                return Err(format!("{name}: more than one lane for {}", lane.param));
            }
            for point in &lane.points {
                if point.at_exact() > length {
                    return Err(format!(
                        "{name}: {} point at {} is after the session end",
                        lane.param,
                        point.at.text()
                    ));
                }
                if !t.range.contains(point.value) {
                    return Err(format!(
                        "{name}: {} value {} outside {}",
                        lane.param,
                        float_repr(point.value),
                        t.range
                    ));
                }
            }
        }
    }
    let section_ids: HashSet<&str> = p.sections.iter().map(|s| s.id.as_str()).collect();
    if section_ids.len() != p.sections.len() {
        return Err("Section IDs must be unique".into());
    }
    for s in &p.sections {
        if s.at_exact() + s.length_exact() > length {
            return Err(format!("Section {} exceeds session", s.id));
        }
    }
    for t in &p.tracks {
        for pad in t.pads.values() {
            if !p.samples.contains_key(&pad.sample) {
                return Err(format!("{}: unknown sample {}", t.id, pad.sample));
            }
        }
        for clip in &t.audio {
            if !p.samples.contains_key(&clip.sample) {
                return Err(format!("{}: unknown sample {}", t.id, clip.sample));
            }
            // Its sound may run past the session's end, as a tail does; its
            // start may not.
            if clip.at_exact() >= length {
                return Err(format!("{}: audio clip starts past the session", t.id));
            }
        }
        for clip in &t.clips {
            let Some(pattern) = p.patterns.get(&clip.pattern) else {
                return Err(format!("{}: unknown pattern {}", t.id, clip.pattern));
            };
            let span = pattern.length_exact() * BigRational::from_integer(clip.repeats.into());
            if clip.at_exact() + span > length {
                return Err(format!("{}: clip exceeds session", t.id));
            }
            for e in pattern.expanded() {
                let Some(pad) = t.pads.get(&e.pad) else {
                    return Err(format!("{}: unknown pad {}", t.id, e.pad));
                };
                if e.note.as_deref().is_some_and(|n| !n.is_empty())
                    && p.samples[&pad.sample]
                        .root_note
                        .as_deref()
                        .is_none_or(str::is_empty)
                {
                    return Err(format!("{} needs root_note for pitched events", pad.sample));
                }
                if pad.mode == PadMode::Gate
                    && e.duration_exact().is_none_or(|d| d <= BigRational::from_integer(0.into()))
                {
                    return Err(format!("{}.{}: gated events need positive duration", t.id, e.pad));
                }
            }
        }
        if let Some(midi) = &t.midi {
            if let Some(sampler) = midi.sampler() {
                sampler_references(p, &t.id, sampler)?;
            }
            for clip in &midi.clips {
                if clip.at_exact() + clip.length_exact() > length {
                    return Err(format!("{}: clip {} exceeds session", t.id, clip.id));
                }
            }
        }
    }
    Ok(())
}

/// A sampler's pads name samples, its map names its pads, and a note plays one
/// pad at most.
fn sampler_references(p: &Project, track: &str, sampler: &crate::schema::Sampler) -> Result<(), String> {
    for pad in sampler.pads.values() {
        if !p.samples.contains_key(&pad.sample) {
            return Err(format!("{track}: unknown sample {}", pad.sample));
        }
    }
    for (i, m) in sampler.map.iter().enumerate() {
        if !sampler.pads.contains_key(&m.pad) {
            return Err(format!("{track}: the map names unknown pad {}", m.pad));
        }
        if let Some(other) = sampler.map[..i].iter().find(|o| o.low <= m.high && m.low <= o.high) {
            let note = m.low.max(other.low);
            return Err(format!(
                "{track}: note {note} ({}) is mapped to both {} and {}",
                note_name(note).unwrap_or_default(),
                other.pad,
                m.pad
            ));
        }
    }
    Ok(())
}

/// What `daw check` says of a song's notes: those that start before their
/// clip or at or after its end, which are kept and do not play, and those the track's
/// sampler maps to no pad, which are silent.
pub fn note_warnings(p: &Project) -> Vec<String> {
    let mut out = Vec::new();
    for t in &p.tracks {
        let Some(midi) = &t.midi else { continue };
        let sampler = midi.sampler();
        for clip in &midi.clips {
            let length = clip.length_exact();
            let ids = |keep: &dyn Fn(&BigRational) -> bool| -> Vec<&str> {
                clip.notes.iter().filter(|n| keep(&n.at_exact())).map(|n| n.id.as_str()).collect()
            };
            for (outside, place) in [
                (ids(&|at| at.is_negative()), "before the clip's start"),
                (ids(&|at| *at >= length), "at or after the clip's end"),
            ] {
                if !outside.is_empty() {
                    out.push(format!("{}.{}: notes {} start {place} and do not play", t.id, clip.id, outside.join(", ")));
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
                out.push(format!(
                    "{}.{}: the sampler maps no pad to notes {}, which are silent",
                    t.id,
                    clip.id,
                    names.join(", ")
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_matches_python() {
        assert_eq!(midi("C2"), Ok(36));
        assert_eq!(midi("F#1"), Ok(30));
        assert_eq!(midi("Bb-1"), Ok(10));
        assert_eq!(midi("c-1"), Ok(0));
        assert_eq!(midi("H2"), Err("Invalid note 'H2'; use e.g. C2 or F#1".into()));
        assert_eq!(midi("G9"), Ok(127));
        assert_eq!(midi("G#9"), Err("Note outside MIDI range: G#9".into()));
    }

    #[test]
    fn a_note_name_reads_back_as_its_number() {
        for n in 0..=127 {
            assert_eq!(midi(&note_name(n).unwrap()), Ok(n));
        }
        assert_eq!((note_name(30).unwrap(), note_name(0).unwrap(), note_name(60).unwrap()), ("F#1".into(), "C-1".into(), "C4".into()));
        assert!(note_name(128).is_err() && note_name(-1).is_err());
    }

    #[test]
    fn range_prints_with_g_format() {
        assert_eq!(Range::closed(-96.0, 24.0).to_string(), "-96 to 24");
        assert_eq!(Range::closed(0.1, 18.0).to_string(), "0.1 to 18");
    }
}
