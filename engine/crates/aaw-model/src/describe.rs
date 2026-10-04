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

/// An equalizer's fields are its bands'.
pub const BAND: &[Field] = &[
    required(choice("shape", "Shape", &["bell", "low_shelf", "high_shelf"], Initial::Required), Initial::Text("bell")),
    required(log(number("freq_hz", "Frequency", 20.0, 20000.0, 0.0, "Hz")), Initial::Number(1000.0)),
    required(number("gain_db", "Gain", -24.0, 24.0, 0.0, "dB"), Initial::Number(0.0)),
    log(number("q", "Q", 0.1, 18.0, 0.71, "")),
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

const LIMITER: &[Field] = &[
    number("ceiling_db", "Ceiling", -24.0, -0.1, -1.0, "dB"),
    log(number("release_ms", "Release", 1.0, 2000.0, 60.0, "ms")),
    structural(number("lookahead_ms", "Look-ahead", 0.5, 20.0, 3.0, "ms")),
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

/// The fields of an effect type, in the order the document writes them,
/// without `type`, `id` and `bypass`, which every effect has. An equalizer's
/// are its bands': see `BAND`.
pub fn effect(kind: &str) -> &'static [Field] {
    match kind {
        "filter" => FILTER,
        "compressor" => COMPRESSOR,
        "limiter" => LIMITER,
        "delay" => DELAY,
        "reverb" => REVERB,
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
            assert!(!fields.is_empty(), "{kind}");
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
