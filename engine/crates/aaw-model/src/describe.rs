//! What each effect's fields are, for anything that shows or edits them without
//! knowing the effects one by one: a label, a unit, a range and a default. A
//! device panel is drawn from these. A test holds them to what validation
//! accepts and to the defaults the model fills in.

use crate::rules::{effect_params, Domain};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A number between `min` and `max`.
    Number,
    /// A whole number between `min` and `max`, or one of `choices`.
    Integer,
    /// One of `choices`.
    Choice,
    Flag,
    /// A length in beats, written as an integer, a decimal or a fraction.
    Beats,
    /// The ID of another track.
    Track,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Initial {
    /// The field must be given; `Field::suggested` is a place to start.
    Required,
    Number(f64),
    Text(&'static str),
    Flag(bool),
    /// The field may be left out, which is its default.
    Absent,
}

#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub min: f64,
    pub max: f64,
    pub default: Initial,
    pub unit: &'static str,
    /// Whether the value is best moved in equal ratios, as a frequency is.
    pub log: bool,
    pub choices: &'static [&'static str],
    /// Whether a change gives the device other state, such as a delay's time,
    /// rather than moving a level or a knob. A playing song fades through such
    /// a change instead of gliding to it.
    pub structural: bool,
    /// The value an effect added without one starts with.
    pub suggested: Initial,
}

const fn number(name: &'static str, label: &'static str, min: f64, max: f64, default: f64, unit: &'static str) -> Field {
    Field {
        name,
        label,
        kind: Kind::Number,
        min,
        max,
        default: Initial::Number(default),
        unit,
        log: false,
        choices: &[],
        structural: false,
        suggested: Initial::Number(default),
    }
}

const fn required(mut f: Field, suggested: Initial) -> Field {
    f.default = Initial::Required;
    f.suggested = suggested;
    f
}

const fn log(mut f: Field) -> Field {
    f.log = true;
    f
}

const fn structural(mut f: Field) -> Field {
    f.structural = true;
    f
}

const fn optional(mut f: Field, suggested: f64) -> Field {
    f.default = Initial::Absent;
    f.suggested = Initial::Number(suggested);
    f
}

const fn choice(name: &'static str, label: &'static str, choices: &'static [&'static str], default: Initial) -> Field {
    Field {
        name,
        label,
        kind: Kind::Choice,
        min: 0.0,
        max: 0.0,
        default,
        unit: "",
        log: false,
        choices,
        structural: true,
        suggested: default,
    }
}

const FILTER: &[Field] = &[
    required(choice("mode", "Mode", &["highpass", "lowpass"], Initial::Required), Initial::Text("lowpass")),
    required(log(number("cutoff_hz", "Cutoff", 10.0, 20000.0, 0.0, "Hz")), Initial::Number(1000.0)),
    Field {
        kind: Kind::Integer,
        choices: &["12", "24", "36", "48"],
        structural: true,
        ..number("slope_db_per_octave", "Slope", 12.0, 48.0, 12.0, "dB/oct")
    },
];

/// An equalizer's fields are its bands'. A bell or a shelf uses `gain_db`
/// and `q`; a highpass or a lowpass uses `slope_db_per_octave` and `q` as
/// the resonance at its corner, and ignores `gain_db`.
pub const BAND: &[Field] = &[
    required(choice("shape", "Shape", &crate::schema::BandShape::NAMES, Initial::Required), Initial::Text("bell")),
    required(log(number("freq_hz", "Frequency", 20.0, 20000.0, 0.0, "Hz")), Initial::Number(1000.0)),
    number("gain_db", "Gain", -24.0, 24.0, 0.0, "dB"),
    log(number("q", "Q", 0.1, 18.0, 0.71, "")),
    Field {
        kind: Kind::Integer,
        choices: &["12", "24", "36", "48"],
        structural: true,
        ..number("slope_db_per_octave", "Slope", 12.0, 48.0, 12.0, "dB/oct")
    },
];

const COMPRESSOR: &[Field] = &[
    required(number("threshold_db", "Threshold", -60.0, 0.0, 0.0, "dB"), Initial::Number(-18.0)),
    log(number("ratio", "Ratio", 1.0, 20.0, 4.0, ":1")),
    number("attack_ms", "Attack", 0.0, 500.0, 10.0, "ms"),
    log(number("release_ms", "Release", 1.0, 5000.0, 120.0, "ms")),
    number("knee_db", "Knee", 0.0, 24.0, 6.0, "dB"),
    number("makeup_db", "Makeup", -24.0, 24.0, 0.0, "dB"),
    Field {
        kind: Kind::Track,
        default: Initial::Absent,
        suggested: Initial::Absent,
        structural: true,
        ..number("sidechain", "Sidechain", 0.0, 0.0, 0.0, "")
    },
];

/// A limiter's fields. `true_peak` adds to its latency, as `lookahead_ms`
/// sets it, so a playing song fades through a change of either.
const LIMITER: &[Field] = &[
    number("ceiling_db", "Ceiling", -24.0, -0.1, -1.0, "dB"),
    log(number("release_ms", "Release", 1.0, 2000.0, 60.0, "ms")),
    structural(number("lookahead_ms", "Look-ahead", 0.5, 20.0, 3.0, "ms")),
    flag("true_peak", "True peak", false),
];

/// A clipper's fields. `oversample` sets its latency, so a playing song
/// fades through a change of it.
const CLIPPER: &[Field] = &[
    number("ceiling_db", "Ceiling", -60.0, 24.0, -1.0, "dB"),
    number("drive_db", "Drive", 0.0, 36.0, 0.0, "dB"),
    number("knee_db", "Knee", 0.0, 24.0, 0.0, "dB"),
    Field {
        kind: Kind::Integer,
        choices: &["1", "2", "4"],
        structural: true,
        ..number("oversample", "Oversample", 1.0, 4.0, 4.0, "x")
    },
];

const DELAY: &[Field] = &[
    Field {
        kind: Kind::Beats,
        default: Initial::Required,
        suggested: Initial::Text("1/2"),
        structural: true,
        ..number("time_beats", "Time", 0.0, 16.0, 0.0, "beats")
    },
    number("feedback_percent", "Feedback", 0.0, 95.0, 35.0, "%"),
    optional(log(number("lowcut_hz", "Low cut", 10.0, 20000.0, 0.0, "Hz")), 200.0),
    optional(log(number("highcut_hz", "High cut", 10.0, 20000.0, 0.0, "Hz")), 4000.0),
    Field {
        kind: Kind::Flag,
        default: Initial::Flag(false),
        suggested: Initial::Flag(false),
        structural: true,
        ..number("ping_pong", "Ping-pong", 0.0, 1.0, 0.0, "")
    },
    number("mix_percent", "Mix", 0.0, 100.0, 100.0, "%"),
];

const REVERB: &[Field] = &[
    structural(log(number("decay_seconds", "Decay", 0.1, 12.0, 1.5, "s"))),
    structural(number("predelay_ms", "Predelay", 0.0, 250.0, 10.0, "ms")),
    structural(log(number("damping_hz", "Damping", 500.0, 20000.0, 6000.0, "Hz"))),
    structural(log(number("lowcut_hz", "Low cut", 20.0, 2000.0, 100.0, "Hz"))),
    structural(number("width_percent", "Width", 0.0, 100.0, 100.0, "%")),
    number("mix_percent", "Mix", 0.0, 100.0, 100.0, "%"),
    Field {
        kind: Kind::Integer,
        structural: true,
        ..number("seed", "Seed", 0.0, 4294967295.0, 0.0, "")
    },
];

const CHORUS: &[Field] = &[
    log(number("rate_hz", "Rate", 0.05, 10.0, 0.8, "Hz")),
    number("depth_ms", "Depth", 0.0, 20.0, 3.0, "ms"),
    number("delay_ms", "Delay", 1.0, 40.0, 12.0, "ms"),
    number("mix_percent", "Mix", 0.0, 100.0, 50.0, "%"),
];

const SATURATION: &[Field] = &[
    choice("mode", "Mode", &crate::schema::SaturationMode::NAMES, Initial::Text("soft")),
    number("drive_db", "Drive", 0.0, 36.0, 12.0, "dB"),
    number("output_db", "Output", -24.0, 24.0, 0.0, "dB"),
    number("mix_percent", "Mix", 0.0, 100.0, 100.0, "%"),
];

/// A utility's fields. `mono`, `mono_below_hz` and `invert` change the
/// device's structure, so a playing song fades through them.
const UTILITY: &[Field] = &[
    number("gain_db", "Gain", -96.0, 24.0, 0.0, "dB"),
    number("pan", "Pan", -1.0, 1.0, 0.0, ""),
    number("width_percent", "Width", 0.0, 400.0, 100.0, "%"),
    flag("mono", "Mono", false),
    optional(structural(log(number("mono_below_hz", "Mono below", 20.0, 1000.0, 0.0, "Hz"))), 120.0),
    choice("invert", "Invert", &crate::schema::Invert::NAMES, Initial::Text("none")),
];

/// The fields of a sampler's pad that the Sampler device sets, in the order
/// the document writes them: what shapes how one sample plays on the keys.
/// `start_seconds` and `end_seconds` go as far as the pad's file does, which
/// only the song knows, so their `max` here is 0 and is to be replaced by
/// the file's length. Every pad field gives the sampler new voices, so a
/// playing song fades through a change rather than gliding to it.
pub const PAD: &[Field] = &[
    choice("mode", "Mode", &["one_shot", "gate"], Initial::Text("one_shot")),
    structural(number("gain_db", "Level", -96.0, 24.0, 0.0, "dB")),
    structural(number("pan", "Pan", -1.0, 1.0, 0.0, "")),
    structural(number("transpose", "Transpose", -36.0, 36.0, 0.0, "st")),
    structural(number("start_seconds", "Start", 0.0, 0.0, 0.0, "s")),
    optional(structural(number("end_seconds", "End", 0.0, 0.0, 0.0, "s")), 0.0),
    structural(number("attack_ms", "Attack", 0.0, 10000.0, 0.3, "ms")),
    structural(number("release_ms", "Release", 0.0, 10000.0, 8.0, "ms")),
    Field {
        kind: Kind::Flag,
        default: Initial::Flag(false),
        suggested: Initial::Flag(false),
        structural: true,
        ..number("reverse", "Reverse", 0.0, 1.0, 0.0, "")
    },
];

const fn flag(name: &'static str, label: &'static str, default: bool) -> Field {
    Field {
        kind: Kind::Flag,
        default: Initial::Flag(default),
        suggested: Initial::Flag(default),
        structural: true,
        ..number(name, label, 0.0, 1.0, 0.0, "")
    }
}

const fn integer(name: &'static str, label: &'static str, min: f64, max: f64, default: f64) -> Field {
    Field {
        kind: Kind::Integer,
        ..number(name, label, min, max, default, "")
    }
}

/// The Synth's own fields, outside its parts. `voices`, `width_mode` and
/// `seed` change nothing in a voice already sounding, so they are not
/// structural.
pub const SYNTH: &[Field] = &[
    integer("voices", "Voices", 1.0, 16.0, 8.0),
    number("glide_ms", "Glide", 0.0, 5000.0, 0.0, "ms"),
    number("velocity_percent", "Velocity", 0.0, 100.0, 100.0, "%"),
    number("width_percent", "Width", 0.0, 100.0, 0.0, "%"),
    choice("width_mode", "Placement", &crate::schema::WidthMode::NAMES, Initial::Text("alternate")),
    integer("seed", "Seed", 0.0, 4294967295.0, 0.0),
];

/// An oscillator's fields. A change of wave, of its table, of its unison
/// count or of its filter routing jumps the waveform, so a playing song
/// fades through it; `phase`, which only a new note reads, is sent when a
/// change ends and is not automated, and left out starts each note at a
/// random place; the rest glide. `table`'s choices are the built-in
/// wavetables; a sample of the project is a choice too, which only the song
/// knows.
pub const OSCILLATOR: &[Field] = &[
    choice("wave", "Wave", &crate::schema::Wave::NAMES, Initial::Text("saw")),
    choice("table", "Table", &crate::schema::WAVETABLES, Initial::Text("organ")),
    number("level_db", "Level", -96.0, 24.0, 0.0, "dB"),
    number("pan", "Pan", -1.0, 1.0, 0.0, ""),
    integer("octave", "Octave", -4.0, 4.0, 0.0),
    number("semitones", "Semitones", -36.0, 36.0, 0.0, "st"),
    number("detune_cents", "Detune", -100.0, 100.0, 0.0, "cents"),
    number("pulse_width", "Pulse width", 1.0, 99.0, 50.0, "%"),
    structural(optional(number("phase", "Phase", 0.0, 100.0, 0.0, "%"), 0.0)),
    structural(integer("unison", "Unison", 1.0, 16.0, 1.0)),
    number("unison_detune_cents", "Unison detune", 0.0, 100.0, 15.0, "cents"),
    number("unison_width_percent", "Unison width", 0.0, 100.0, 100.0, "%"),
    flag("filter", "Filtered", true),
];

/// The Synth's filter's fields.
pub const SYNTH_FILTER: &[Field] = &[
    flag("enabled", "Enabled", true),
    choice("mode", "Mode", &crate::schema::SynthFilterMode::NAMES, Initial::Text("lowpass")),
    Field {
        kind: Kind::Integer,
        choices: &["12", "24"],
        structural: true,
        ..number("slope_db_per_octave", "Slope", 12.0, 24.0, 12.0, "dB/oct")
    },
    log(number("cutoff_hz", "Cutoff", 10.0, 20000.0, 20000.0, "Hz")),
    number("resonance_percent", "Resonance", 0.0, 100.0, 0.0, "%"),
    number("drive_db", "Drive", 0.0, 24.0, 0.0, "dB"),
    number("keytrack_percent", "Key track", 0.0, 100.0, 0.0, "%"),
];

/// An envelope's fields.
pub const ENVELOPE: &[Field] = &[
    number("attack_ms", "Attack", 0.0, 20000.0, 1.0, "ms"),
    number("decay_ms", "Decay", 0.0, 20000.0, 100.0, "ms"),
    number("sustain_percent", "Sustain", 0.0, 100.0, 100.0, "%"),
    number("release_ms", "Release", 0.0, 20000.0, 50.0, "ms"),
];

/// An LFO's fields. `rate_beats`, when given, replaces `rate_hz`.
pub const LFO: &[Field] = &[
    choice("shape", "Shape", &crate::schema::LfoShape::NAMES, Initial::Text("sine")),
    log(number("rate_hz", "Rate", 0.01, 100.0, 1.0, "Hz")),
    Field {
        kind: Kind::Beats,
        default: Initial::Absent,
        suggested: Initial::Text("1"),
        structural: true,
        ..number("rate_beats", "Rate in beats", 0.0, 64.0, 0.0, "beats")
    },
    number("phase_percent", "Phase", 0.0, 100.0, 0.0, "%"),
    flag("retrigger", "Retrigger", false),
];

/// A macro: a knob from 0 to 100.
pub const MACRO: Field = number("macro", "Macro", 0.0, 100.0, 0.0, "");

/// The fields of each part of the Synth, by the part's name in the patch,
/// for anything that lists them: `synth` for the Synth's own fields, then
/// `oscillators`, `filter`, `envelopes` and `lfos`.
pub const SYNTH_PARTS: &[(&str, &[Field])] = &[
    ("synth", SYNTH),
    ("oscillators", OSCILLATOR),
    ("filter", SYNTH_FILTER),
    ("envelopes", ENVELOPE),
    ("lfos", LFO),
];

/// The fields of an effect type, in the order the document writes them,
/// without `type`, `id` and `bypass`, which every effect has. An equalizer's
/// are its bands': see `BAND`.
pub fn effect(kind: &str) -> &'static [Field] {
    match kind {
        "filter" => FILTER,
        "compressor" => COMPRESSOR,
        "limiter" => LIMITER,
        "clipper" => CLIPPER,
        "delay" => DELAY,
        "reverb" => REVERB,
        "chorus" => CHORUS,
        "saturation" => SATURATION,
        "utility" => UTILITY,
        _ => &[],
    }
}

/// The interpolation domain of a field that automation can move, or None for
/// one it cannot. `kind` is the effect type; an equalizer's fields are its
/// bands'.
pub fn automatable(kind: &str, field: &str) -> Option<Domain> {
    effect_params(kind).iter().find(|(name, _)| *name == field).map(|(_, domain)| *domain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{dict, Value};
    use crate::Project;

    fn value(i: Initial) -> Option<Value> {
        match i {
            Initial::Number(x) => Some(Value::Float(x)),
            Initial::Text(s) => Some(Value::str(s)),
            Initial::Flag(b) => Some(Value::Bool(b)),
            Initial::Required | Initial::Absent => None,
        }
    }

    /// A song with one effect on its master, or the reason it is refused.
    fn check(effect: Value) -> Result<Project, String> {
        let song = dict(vec![
            ("session", dict(vec![])),
            ("master", dict(vec![("effects", Value::List(vec![effect]))])),
        ]);
        Project::validate(&song).map_err(|e| e.to_string())
    }

    /// An effect with its required fields at their suggested values, and `with`.
    fn effect_value(kind: &str, with: &[(&str, Value)]) -> Value {
        let mut fields: Vec<(&str, Value)> = vec![("type", Value::str(kind))];
        let own: Vec<(&str, Value)> = match kind {
            "eq" => {
                let band = |with: &[(&str, Value)]| {
                    let mut b: Vec<(&str, Value)> = BAND
                        .iter()
                        .filter(|f| f.default == Initial::Required)
                        .map(|f| (f.name, value(f.suggested).unwrap()))
                        .collect();
                    for (k, v) in with {
                        b.retain(|(name, _)| name != k);
                        b.push((k, v.clone()));
                    }
                    dict(b)
                };
                vec![("bands", Value::List(vec![band(with)]))]
            }
            _ => {
                let mut own: Vec<(&str, Value)> = effect(kind)
                    .iter()
                    .filter(|f| f.default == Initial::Required)
                    .map(|f| (f.name, value(f.suggested).unwrap()))
                    .collect();
                for (k, v) in with {
                    own.retain(|(name, _)| name != k);
                    own.push((k, v.clone()));
                }
                own
            }
        };
        fields.extend(own);
        dict(fields)
    }

    #[test]
    fn fields_agree_with_validation_and_defaults() {
        for kind in crate::EFFECT_TYPES {
            let fields = if kind == "eq" { BAND } else { effect(kind) };
            // An analyzer has nothing to set: the song's `type`, `id` and `bypass` alone.
            assert!(!fields.is_empty() || kind == "analyzer", "{kind}");
            // The suggested start is valid, and its dump has every field.
            let project = check(effect_value(kind, &[])).unwrap_or_else(|e| panic!("{kind}: {e}"));
            let dumped = project.master.effects[0].dump(false);
            let dumped = match kind {
                "eq" => match dumped.get("bands") {
                    Some(Value::List(bands)) => bands[0].clone(),
                    _ => panic!("an eq has bands"),
                },
                _ => dumped,
            };
            let Value::Dict(d) = &dumped else { panic!() };
            let names: Vec<&str> = d.keys().filter_map(|k| k.as_str()).filter(|k| !["type", "id", "bypass"].contains(k)).collect();
            assert_eq!(names, fields.iter().map(|f| f.name).collect::<Vec<_>>(), "{kind}");
            for f in fields {
                let got = dumped.get(f.name).unwrap();
                match f.default {
                    Initial::Number(x) => assert!(crate::value::py_eq(got, &Value::Float(x)), "{kind}.{}: {got:?}", f.name),
                    Initial::Text(s) => assert!(crate::value::py_eq(got, &Value::str(s)), "{kind}.{}", f.name),
                    Initial::Flag(b) => assert!(crate::value::py_eq(got, &Value::Bool(b)), "{kind}.{}", f.name),
                    Initial::Absent => assert!(got.is_none(), "{kind}.{}", f.name),
                    Initial::Required => assert!(check(dict(vec![("type", Value::str(kind))])).is_err()),
                }
                let with = |v: Value| check(effect_value(kind, &[(f.name, v)]));
                match f.kind {
                    Kind::Number | Kind::Integer if f.choices.is_empty() => {
                        let n = |x: f64| if f.kind == Kind::Integer { Value::int(x as i64) } else { Value::Float(x) };
                        assert!(with(n(f.min)).is_ok() && with(n(f.max)).is_ok(), "{kind}.{} at its limits", f.name);
                        let step = if f.kind == Kind::Integer { 1.0 } else { 1e-6 * f.max.abs().max(1.0) };
                        assert!(with(n(f.min - step)).is_err(), "{kind}.{} below {}", f.name, f.min);
                        assert!(with(n(f.max + step)).is_err(), "{kind}.{} above {}", f.name, f.max);
                    }
                    Kind::Integer | Kind::Choice => {
                        for c in f.choices {
                            let v = if f.kind == Kind::Integer { Value::int(c.parse().unwrap()) } else { Value::str(c) };
                            assert!(with(v).is_ok(), "{kind}.{} = {c}", f.name);
                        }
                        assert!(with(Value::str("other")).is_err(), "{kind}.{}", f.name);
                    }
                    Kind::Flag => assert!(with(Value::Bool(true)).is_ok()),
                    Kind::Beats => {
                        assert!(with(Value::str("3/4")).is_ok() && with(Value::int(f.max as i64)).is_ok());
                        assert!(with(Value::int(0)).is_err() && with(Value::Float(f.max + 0.5)).is_err());
                    }
                    Kind::Track => {}
                    Kind::Number => unreachable!(),
                }
                // What automation can move is a number.
                if automatable(kind, f.name).is_some() {
                    assert_eq!(f.kind, Kind::Number, "{kind}.{}", f.name);
                    assert!(!f.structural, "{kind}.{}", f.name);
                }
            }
            for (name, _) in effect_params(kind) {
                assert!(fields.iter().any(|f| f.name == *name), "{kind}.{name} is automatable");
            }
        }
    }

    /// A song with one pad on one track, with `with` set on the pad, or the
    /// reason it is refused.
    fn pad(with: &[(&str, Value)]) -> Result<crate::Pad, String> {
        let mut fields: Vec<(&str, Value)> = vec![("sample", Value::str("s"))];
        fields.extend(with.iter().cloned());
        let song = dict(vec![
            ("session", dict(vec![])),
            ("samples", dict(vec![("s", dict(vec![("path", Value::str("s.wav"))]))])),
            ("tracks", Value::List(vec![dict(vec![("id", Value::str("t")), ("pads", dict(vec![("p", dict(fields))]))])])),
        ]);
        Project::validate(&song).map(|p| p.tracks[0].pads["p"].clone()).map_err(|e| e.to_string())
    }

    #[test]
    fn pad_fields_agree_with_validation_and_defaults() {
        let plain = pad(&[]).unwrap();
        let dumped = plain.dump(false);
        for f in PAD {
            let got = dumped.get(f.name).unwrap();
            match f.default {
                Initial::Number(x) => assert!(crate::value::py_eq(got, &Value::Float(x)), "pad.{}: {got:?}", f.name),
                Initial::Text(s) => assert!(crate::value::py_eq(got, &Value::str(s)), "pad.{}", f.name),
                Initial::Flag(b) => assert!(crate::value::py_eq(got, &Value::Bool(b)), "pad.{}", f.name),
                Initial::Absent => assert!(got.is_none(), "pad.{}", f.name),
                Initial::Required => unreachable!("every pad field but its sample has a default"),
            }
            let with = |v: Value| pad(&[(f.name, v)]);
            match f.kind {
                Kind::Number => {
                    // An end is after a start of 0, so its least is past 0.
                    if f.name != "end_seconds" {
                        assert!(with(Value::Float(f.min)).is_ok(), "pad.{} at its least", f.name);
                    }
                    assert!(with(Value::Float(f.min - 1e-6)).is_err(), "pad.{} below {}", f.name, f.min);
                    // The file's length bounds the start and the end.
                    if f.max > 0.0 {
                        assert!(with(Value::Float(f.max)).is_ok() && with(Value::Float(f.max + 1e-3)).is_err(), "pad.{} at its most", f.name);
                    } else {
                        assert!(with(Value::Float(1e6)).is_ok(), "pad.{} is bounded by its file", f.name);
                    }
                }
                Kind::Choice => {
                    for c in f.choices {
                        assert!(with(Value::str(c)).is_ok(), "pad.{} = {c}", f.name);
                    }
                    assert!(with(Value::str("other")).is_err());
                }
                Kind::Flag => assert!(with(Value::Bool(true)).is_ok()),
                _ => unreachable!("pad.{}", f.name),
            }
            assert!(f.structural, "pad.{} gives the sampler new voices", f.name);
        }
        // An end is positive, and after the start.
        assert!(pad(&[("end_seconds", Value::Float(0.0))]).is_err());
        assert!(pad(&[("start_seconds", Value::Float(2.0)), ("end_seconds", Value::Float(1.0))]).is_err());
        assert_eq!(pad(&[("end_seconds", Value::Float(1.5))]).unwrap().end_seconds, Some(1.5));
    }
}

#[cfg(test)]
mod synth_tests {
    use super::*;
    use crate::rules::synth_param;
    use crate::value::{dict, Value};
    use crate::Project;

    /// A song with one synth of every part, `with` set at `path` in it.
    fn synth(path: &[&str], with: Option<Value>) -> Result<crate::Synth, String> {
        let mut song = crate::yaml_load::load(
            "session: {}\ntracks:\n- id: t\n  type: midi\n  instrument:\n    synth:\n      oscillators: {a: {}}\n      lfos: {l: {}}\n      macros: {m: 0}\n",
        )
        .unwrap();
        let mut node = &mut song;
        for key in ["tracks", "0", "instrument", "synth"] {
            node = match node {
                Value::Dict(d) => d.get_mut(&crate::value::Key::str(key)).unwrap(),
                Value::List(l) => &mut l[key.parse::<usize>().unwrap()],
                _ => unreachable!(),
            };
        }
        for key in &path[..path.len() - 1] {
            let Value::Dict(d) = node else { unreachable!() };
            node = d.entry(crate::value::Key::str(key)).or_insert_with(|| dict(vec![]));
        }
        let Value::Dict(d) = node else { unreachable!() };
        match with {
            Some(v) => {
                d.insert(crate::value::Key::str(path[path.len() - 1]), v);
            }
            None => {
                d.shift_remove(&crate::value::Key::str(path[path.len() - 1]));
            }
        }
        Project::validate(&song)
            .map(|p| p.tracks[0].midi.as_ref().unwrap().synth().unwrap().clone())
            .map_err(|e| e.to_string())
    }

    #[test]
    fn synth_fields_agree_with_validation_defaults_and_automation() {
        let plain = synth(&["voices"], None).unwrap();
        let dump = plain.dump(false);
        for (part, fields) in SYNTH_PARTS {
            let (prefix, holder): (Vec<&str>, Value) = match *part {
                "synth" => (vec![], dump.clone()),
                "oscillators" => (vec!["oscillators", "a"], dump.get("oscillators").unwrap().get("a").unwrap().clone()),
                "filter" => (vec!["filter"], dump.get("filter").unwrap().clone()),
                "envelopes" => (vec!["envelopes", "amp"], dump.get("envelopes").unwrap().get("amp").unwrap().clone()),
                _ => (vec!["lfos", "l"], dump.get("lfos").unwrap().get("l").unwrap().clone()),
            };
            let Value::Dict(d) = &holder else { panic!() };
            let names: Vec<&str> = d.keys().filter_map(|k| k.as_str()).filter(|k| *k != "patch").collect();
            let listed: Vec<&str> = fields.iter().map(|f| f.name).collect();
            let listed: Vec<&str> = if *part == "synth" {
                listed.into_iter().chain(["oscillators", "filter", "envelopes", "lfos", "modulation", "macros", "effects"]).collect()
            } else {
                listed
            };
            assert_eq!(names, listed, "{part}");
            for f in fields.iter() {
                let got = holder.get(f.name).unwrap();
                match f.default {
                    Initial::Number(x) => assert!(crate::value::py_eq(got, &Value::Float(x)), "{part}.{}: {got:?}", f.name),
                    Initial::Text(s) => assert!(crate::value::py_eq(got, &Value::str(s)), "{part}.{}", f.name),
                    Initial::Flag(b) => assert!(crate::value::py_eq(got, &Value::Bool(b)), "{part}.{}", f.name),
                    Initial::Absent => assert!(got.is_none(), "{part}.{}", f.name),
                    Initial::Required => unreachable!("every synth field has a default"),
                }
                let path: Vec<&str> = prefix.iter().copied().chain([f.name]).collect();
                let with = |v: Value| synth(&path, Some(v));
                match f.kind {
                    Kind::Number | Kind::Integer if f.choices.is_empty() => {
                        let n = |x: f64| if f.kind == Kind::Integer { Value::int(x as i64) } else { Value::Float(x) };
                        assert!(with(n(f.min)).is_ok() && with(n(f.max)).is_ok(), "{part}.{} at its limits", f.name);
                        let step = if f.kind == Kind::Integer { 1.0 } else { 1e-6 * f.max.abs().max(1.0) };
                        assert!(with(n(f.min - step)).is_err(), "{part}.{} below {}", f.name, f.min);
                        assert!(with(n(f.max + step)).is_err(), "{part}.{} above {}", f.name, f.max);
                    }
                    Kind::Integer | Kind::Choice => {
                        for c in f.choices {
                            let v = if f.kind == Kind::Integer { Value::int(c.parse().unwrap()) } else { Value::str(c) };
                            assert!(with(v).is_ok(), "{part}.{} = {c}", f.name);
                        }
                        assert!(with(Value::str("other")).is_err(), "{part}.{}", f.name);
                    }
                    Kind::Flag => assert!(with(Value::Bool(!matches!(f.default, Initial::Flag(true)))).is_ok()),
                    Kind::Beats => {
                        assert!(with(Value::str("1/2")).is_ok() && with(Value::int(f.max as i64)).is_ok());
                        assert!(with(Value::int(0)).is_err() && with(Value::Float(f.max + 0.5)).is_err());
                    }
                    Kind::Track | Kind::Number => unreachable!("{part}.{}", f.name),
                }
                // A lane moves exactly the numbers that are not structural.
                let automatable = synth_param(&plain, &path.join(".")).is_ok();
                assert_eq!(automatable, f.kind == Kind::Number && !f.structural, "{part}.{}", f.name);
            }
        }
        assert!(synth_param(&plain, "macros.m").is_ok() && synth_param(&plain, "macros.x").is_err());
        assert!(synth_param(&plain, "oscillators.b.level_db").is_err());
        // The patch's effects are automated as effects.REF.FIELD, by ID or index.
        let with = synth(&["effects"], Some(Value::List(vec![
            dict(vec![("type", Value::str("chorus")), ("id", Value::str("wide"))]),
            dict(vec![("type", Value::str("saturation"))]),
            dict(vec![("type", Value::str("eq")), ("bands", Value::List(vec![dict(vec![("shape", Value::str("bell")), ("freq_hz", Value::Float(500.0)), ("gain_db", Value::Float(2.0))])]))]),
        ])))
        .unwrap();
        assert!(synth_param(&with, "effects.wide.mix_percent").is_ok() && synth_param(&with, "effects.0.rate_hz").is_ok());
        assert!(synth_param(&with, "effects.1.drive_db").is_ok() && synth_param(&with, "effects.1.mode").is_err());
        assert!(synth_param(&with, "effects.2.bands.0.gain_db").is_ok() && synth_param(&with, "effects.2.gain_db").is_err());
        assert!(synth_param(&with, "effects.3.mix_percent").is_err() && synth_param(&with, "effects.nope.mix_percent").is_err());
        assert_eq!(crate::rules::synth_value(&with, "effects.wide.mix_percent"), Some(50.0));
        assert_eq!(crate::rules::synth_value(&with, "effects.2.bands.0.gain_db"), Some(2.0));
        let params = crate::rules::synth_params(&with);
        assert!(params.contains(&"effects.wide.rate_hz".to_string()) && params.contains(&"effects.1.output_db".to_string()) && params.contains(&"effects.2.bands.0.q".to_string()));
        // A patch's compressor has no sidechain, and a wavetable is a built-in table or a sample.
        let e = synth(&["effects"], Some(Value::List(vec![dict(vec![("type", Value::str("compressor")), ("threshold_db", Value::Float(-20.0)), ("sidechain", Value::str("t"))])]))).unwrap_err();
        assert!(e.contains("no sidechain"), "{e}");
        assert!(synth(&["oscillators", "a", "table"], Some(Value::str("fold"))).is_ok());
        let e = synth(&["oscillators", "a", "wave"], Some(Value::str("wavetable"))).map(|s| s.oscillators["a"].table.clone());
        assert_eq!(e.unwrap(), "organ");
        let mut both = synth(&["oscillators", "a", "wave"], Some(Value::str("wavetable"))).unwrap();
        both.oscillators[0].table = "missing".into();
        let e = synth(&["oscillators", "a"], Some(both.oscillators[0].dump(true))).unwrap_err();
        assert!(e.contains("wavetable missing"), "{e}");
        assert!(synth(&["oscillators", "a", "table"], Some(Value::str("not an id"))).unwrap_err().contains("built-in wavetable"));
    }
}
