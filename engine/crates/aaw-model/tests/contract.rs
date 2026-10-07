//! The schema `daw describe` publishes must be the one validation enforces.
//!
//! A song with one of every model, giving only what each requires, is walked
//! beside the schema: every model has exactly the schema's fields, a field left
//! out takes the schema's default, a required field is missed when removed, and
//! a number or choice is accepted up to the schema's limits and refused past them.
//! A track is one of two models, and which is told by its `type`.

use aaw_model::contract;
use aaw_model::value::{Key, Value};
use aaw_model::{yaml_load, Project};
use serde_json::Value as Json;
use std::collections::BTreeSet;

const SONG: &str = r#"
session: {}
samples: {s: {path: s.wav}}
patterns: {p: {length_beats: 4, events: [{at: 0, pad: a}]}}
tracks:
  - id: t
    pads: {a: {sample: s}}
    clips: [{pattern: p}]
    audio: [{sample: s}]
    effects:
      - {type: filter, mode: lowpass, cutoff_hz: 1000}
      - {type: eq, bands: [{shape: bell, freq_hz: 1000, gain_db: 0}]}
      - {type: compressor, threshold_db: -20}
      - {type: limiter}
      - {type: delay, time_beats: 1}
      - {type: reverb}
      - {type: chorus}
      - {type: saturation}
      - {type: utility}
    sends: [{to: r}]
    automation: [{param: gain_db, points: [{at: 0, value: 0}]}]
  - id: m
    type: midi
    instrument: {sampler: {pads: {a: {sample: s}}, map: [{notes: 60, pad: a}]}}
    clips: [{length_beats: 4, notes: [{pitch: 60, duration: 1}]}]
    effects: [{type: limiter}]
    sends: [{to: r}]
    automation: [{param: gain_db, points: [{at: 0, value: 0}]}]
  - id: y
    type: midi
    instrument:
      synth:
        oscillators: {a: {}}
        lfos: {lfo1: {}}
        macros: {tone: 50}
        modulation: [{source: macros.tone, target: filter.cutoff_hz, amount: 1}]
        effects: [{type: chorus}, {type: saturation}]
    automation: [{param: instrument.filter.cutoff_hz, points: [{at: 0, value: 900}]}]
returns: [{id: r}]
sections: [{id: a, at: 0, length_beats: 4}]
master: {}
"#;

#[derive(Clone)]
enum Step {
    Key(String),
    Index(usize),
}

fn at<'a>(root: &'a Value, path: &[Step]) -> &'a Value {
    path.iter().fold(root, |node, step| match (node, step) {
        (Value::Dict(d), Step::Key(k)) => &d[&Key::str(k)],
        (Value::List(l), Step::Index(i)) => &l[*i],
        _ => panic!("the path leaves the document"),
    })
}

/// The document with the value at `path` replaced, or removed for None.
fn with(root: &Value, path: &[Step], value: Option<Value>) -> Value {
    let Some((step, rest)) = path.split_first() else {
        return value.expect("a root cannot be removed");
    };
    match (root, step) {
        (Value::Dict(d), Step::Key(k)) => {
            let mut d = d.clone();
            match (rest.is_empty(), value) {
                (true, None) => {
                    d.shift_remove(&Key::str(k));
                }
                (_, value) => {
                    let inner = with(&d[&Key::str(k)], rest, value);
                    d.insert(Key::str(k), inner);
                }
            }
            Value::Dict(d)
        }
        (Value::List(l), Step::Index(i)) => {
            let mut l = l.clone();
            l[*i] = with(&l[*i], rest, value);
            Value::List(l)
        }
        _ => panic!("the path leaves the document"),
    }
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => n.to_string().parse().ok(),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn same(value: &Value, json: &Json) -> bool {
    match (value, json) {
        (Value::None, Json::Null) => true,
        (Value::Bool(a), Json::Bool(b)) => a == b,
        (Value::Str(a), Json::String(b)) => a == b,
        (v, Json::Number(n)) => number(v).is_some_and(|x| Some(x) == n.as_f64()),
        _ => false,
    }
}

struct Walk {
    schema: &'static Json,
    song: Value,
    models: BTreeSet<String>,
    limits: usize,
    choices: usize,
    defaults: usize,
}

impl Walk {
    /// Whether the song with `value` at `path` is refused at that field.
    fn refused(&self, path: &[Step], loc: &[String], value: Option<Value>) -> bool {
        let loc = loc.join(".");
        match Project::validate(&with(&self.song, path, value)) {
            Ok(_) => false,
            Err(e) => e.errors.iter().any(|x| x.loc_text() == loc),
        }
    }

    fn resolve(&mut self, node: &'static Json) -> &'static Json {
        match node.get("$ref").and_then(Json::as_str) {
            Some(r) => {
                let name = r.strip_prefix("#/$defs/").expect("a local reference");
                self.models.insert(name.to_string());
                &self.schema["$defs"][name]
            }
            None => node,
        }
    }

    /// A field's limits and choices, which sit beside `type` or, for a field
    /// that may be null, in the other branch of its `anyOf`.
    fn field(&mut self, prop: &'static Json, path: &[Step], loc: &[String]) {
        let branches: Vec<&'static Json> = match prop.get("anyOf").and_then(Json::as_array) {
            Some(options) => options.iter().collect(),
            None => vec![prop],
        };
        for branch in branches {
            let whole = branch.get("type").and_then(Json::as_str) == Some("integer");
            let make = |x: f64| if whole { Value::int(x as i64) } else { Value::Float(x) };
            let beyond = |x: f64, up: bool| match (whole, up) {
                (true, true) => x + 1.0,
                (true, false) => x - 1.0,
                (false, true) => x.next_up(),
                (false, false) => x.next_down(),
            };
            for (key, up, open) in [
                ("minimum", false, false),
                ("maximum", true, false),
                ("exclusiveMinimum", false, true),
                ("exclusiveMaximum", true, true),
            ] {
                let Some(limit) = branch.get(key).and_then(Json::as_f64) else {
                    continue;
                };
                let name = loc.join(".");
                // An inclusive limit is accepted and the next value past it is
                // not; an exclusive one is refused and the next value inside is not.
                assert_eq!(self.refused(path, loc, Some(make(limit))), open, "{name} at its {key} {limit}");
                let other = beyond(limit, up != open);
                assert_eq!(self.refused(path, loc, Some(make(other))), !open, "{name} at {other}, past its {key}");
                self.limits += 1;
            }
            if let Some(options) = branch.get("enum").and_then(Json::as_array) {
                for option in options {
                    let value = match option {
                        Json::String(s) => Value::str(s),
                        other => Value::int(other.as_i64().expect("a choice is text or a whole number")),
                    };
                    assert!(!self.refused(path, loc, Some(value)), "{} = {option}", loc.join("."));
                }
                assert!(self.refused(path, loc, Some(Value::str("no such choice"))), "{}", loc.join("."));
                self.choices += 1;
            }
        }
    }

    /// `tag` names the field that picks this model in a union, which is missed
    /// at the union, not in the model.
    fn model(&mut self, node: &'static Json, path: &[Step], loc: &[String], tag: Option<&str>) {
        let model = self.resolve(node);
        let properties = model["properties"].as_object().expect("a model has properties");
        let Value::Dict(dumped) = at(&self.song, path) else {
            panic!("{} is not a mapping", loc.join("."));
        };
        let names: Vec<&str> = dumped.keys().filter_map(Key::as_str).collect();
        assert_eq!(names, properties.keys().map(String::as_str).collect::<Vec<_>>(), "{}", model["title"]);
        let required: Vec<&str> = model
            .get("required")
            .and_then(Json::as_array)
            .map(|r| r.iter().filter_map(Json::as_str).collect())
            .unwrap_or_default();
        for (name, prop) in properties {
            let path = [path, &[Step::Key(name.clone())]].concat();
            let loc = [loc, &[name.clone()]].concat();
            if required.contains(&name.as_str()) && tag != Some(name.as_str()) {
                assert!(self.refused(&path, &loc, None), "{} is required", loc.join("."));
            }
            // SONG gives only required fields, collections and models that
            // may be null, so every other default here was filled in.
            if let Some(default) = prop.get("default") {
                let given = optional_model(prop).is_some() && !at(&self.song, &path).is_none();
                if !given {
                    assert!(same(at(&self.song, &path), default), "{} defaults to {default}", loc.join("."));
                    self.defaults += 1;
                }
            }
            self.field(prop, &path, &loc);
            self.inside(prop, &path, &loc);
        }
    }

    /// The models inside a field: one, one or null, a list of them or a map of them.
    fn inside(&mut self, prop: &'static Json, path: &[Step], loc: &[String]) {
        if prop.get("$ref").is_some() {
            return self.model(prop, path, loc, None);
        }
        if let Some(branch) = optional_model(prop) {
            if !at(&self.song, path).is_none() {
                self.model(branch, path, loc, None);
            }
            return;
        }
        if let Some(items) = prop.get("items") {
            let Value::List(list) = at(&self.song, path) else { panic!() };
            for i in 0..list.len() {
                let path = [path, &[Step::Index(i)]].concat();
                let mut loc = [loc, &[i.to_string()]].concat();
                match items.get("discriminator") {
                    // A tagged union: the tag picks the model and is part of
                    // the location errors name.
                    Some(d) => {
                        let field = d["propertyName"].as_str().expect("a tag field");
                        let Some(Value::Str(tag)) = at(&self.song, &path).get(field).cloned() else { panic!() };
                        assert!(self.refused(&[&path[..], &[Step::Key(field.into())]].concat(), &loc, None), "{} needs its {field}", loc.join("."));
                        loc.push(tag.clone());
                        let branch = items["oneOf"].as_array().unwrap().iter().find(|b| b["$ref"] == d["mapping"][&tag]);
                        self.model(branch.expect("a branch for each tag"), &path, &loc, Some(field));
                    }
                    None if items.get("$ref").is_some() => self.model(items, &path, &loc, None),
                    // Models told apart by a `type` one has and the other has not.
                    None if items.get("anyOf").is_some() => {
                        let tag = at(&self.song, &path).get("type").cloned();
                        let wanted = |b: &&'static Json| {
                            let model = &self.schema["$defs"][b["$ref"].as_str().unwrap().trim_start_matches("#/$defs/")];
                            match (&tag, model["properties"].get("type")) {
                                (Some(Value::Str(t)), Some(declared)) => declared["const"] == Json::String(t.clone()),
                                (None, None) => true,
                                _ => false,
                            }
                        };
                        let branch = items["anyOf"].as_array().unwrap().iter().find(wanted).expect("a model for each kind");
                        self.model(branch, &path, &loc, tag.is_some().then_some("type"));
                    }
                    None => {}
                }
            }
        }
        if let Some(values) = prop.get("additionalProperties").filter(|v| v.get("$ref").is_some()) {
            let Value::Dict(d) = at(&self.song, path) else { panic!() };
            for key in d.keys().filter_map(Key::as_str).map(str::to_string).collect::<Vec<_>>() {
                let path = [path, &[Step::Key(key.clone())]].concat();
                self.model(values, &path, &[loc, &[key]].concat(), None);
            }
        }
    }
}

/// The model a field holds when it holds a model or null.
fn optional_model(prop: &'static Json) -> Option<&'static Json> {
    let options = prop.get("anyOf")?.as_array()?;
    let model = options.iter().find(|o| o.get("$ref").is_some())?;
    options.iter().any(|o| o["type"] == "null").then_some(model)
}

#[test]
fn the_schema_is_what_validation_enforces() {
    let schema = contract::schema();
    let given = yaml_load::load(SONG).unwrap();
    let project = Project::validate(&given).unwrap_or_else(|e| panic!("{e}"));
    let mut walk = Walk {
        schema,
        song: project.dump(false),
        models: BTreeSet::new(),
        limits: 0,
        choices: 0,
        defaults: 0,
    };
    // The full dump is itself a valid song, which the checks then vary.
    Project::validate(&walk.song).unwrap();
    walk.model(schema, &[], &[], None);
    let all: BTreeSet<String> = schema["$defs"].as_object().unwrap().keys().cloned().collect();
    assert_eq!(walk.models, all, "the song has one of every model");
    assert!(walk.limits >= 80 && walk.choices >= 6 && walk.defaults >= 60, "{} limits, {} choices, {} defaults", walk.limits, walk.choices, walk.defaults);
}

#[test]
fn each_topic_describes_its_models() {
    for topic in contract::TOPICS {
        let described = contract::describe(topic).unwrap();
        assert!(described["semantics"].as_object().is_some_and(|s| !s.is_empty()), "{topic}");
        // The short form says everything the semantics say, without the schema.
        let full = contract::describe_with_schema(topic).unwrap();
        assert_eq!(described["semantics"], full["semantics"], "{topic}");
        assert!(described.get("schema").is_none(), "{topic}");
    }
    assert!(contract::describe("other").is_none());
    let topics = contract::topics();
    assert_eq!(topics["topics"].as_object().unwrap().keys().collect::<Vec<_>>(), contract::TOPICS);
    let effects = contract::describe_with_schema("effects").unwrap();
    let kinds: Vec<&String> = effects["schema"].as_object().unwrap().keys().collect();
    assert_eq!(kinds, aaw_model::EFFECT_TYPES.iter().collect::<Vec<_>>());
    // A model on its own carries the models it refers to, and no others.
    assert_eq!(effects["schema"]["eq"]["$defs"].as_object().unwrap().keys().collect::<Vec<_>>(), ["EqBand"]);
    assert!(effects["schema"]["filter"].get("$defs").is_none());
    assert!(effects["routing"]["return"]["$defs"].get("Point").is_some());
    let automation = contract::describe_with_schema("automation").unwrap();
    assert_eq!(automation["automatable"]["effects"]["eq"]["q"], "log");
    assert!(automation["automatable"]["effects"].get("limiter").is_none());
}

#[test]
fn a_topic_lists_its_fields_a_line_each() {
    let midi = contract::describe("midi").unwrap();
    let text = serde_json::to_string_pretty(&midi).unwrap();
    assert!(text.len() < 8000, "{} bytes", text.len());
    let fields = &midi["fields"];
    assert_eq!(fields["tracks[].type"], "midi, required");
    assert_eq!(fields["tracks[].clips[].notes[].pitch"], "integer 0..127 or text, required");
    assert_eq!(fields["tracks[].instrument.sampler.pads"], "map of names to Pad (daw describe sampler)");
    assert_eq!(fields["tracks[].effects"], "list of effect (daw describe effects), at most 32");
    let project = contract::describe("project").unwrap();
    assert_eq!(project["fields"]["session.sample_rate"], "44100|48000, default 48000");
    assert_eq!(project["fields"]["tracks[].clips[].velocity_scale"], "number >0..2, default 1");
    assert_eq!(project["fields"]["patterns.ID.events[].duration"], "beats or null, default null");
    assert_eq!(project["fields"]["tracks[]"], "MidiTrack (daw describe midi)");
    let effects = contract::describe("effects").unwrap();
    assert_eq!(effects["fields"]["eq.bands[].q"], "number 0.1..18, default 0.71");
    assert_eq!(effects["fields"]["filter.mode"], "highpass|lowpass, required");
    // Every field of a model the topic lists has its line.
    for (name, line) in contract::model_fields("Lfo") {
        assert_eq!(contract::describe("synth").unwrap()["fields"][format!("lfos.ID.{name}")], Json::String(line));
    }
}

#[test]
fn an_unknown_field_is_answered_with_the_nearest() {
    let song = yaml_load::load(
        "session: {}\npatterns: {}\ntracks:\n  - id: drums\n    pads: {}\n    clips: []\n    volume: -3\nmaster: {effects: [{type: limiter, ceiling: -1}]}\nbpm: 90\n",
    )
    .unwrap();
    let error = Project::validate(&song).unwrap_err();
    let hint = aaw_model::hint::hint(&song, &error).unwrap();
    let lines: Vec<&str> = hint.lines().collect();
    assert_eq!(
        lines,
        [
            "`tracks.drums.volume` is not a field; did you mean `tracks.drums.gain_db`?",
            "`master.effects.0.ceiling` is not a field; did you mean `master.effects.0.ceiling_db`?",
            "`bpm` is not a field; did you mean `session.tempo`?",
        ]
    );
    let explained = aaw_model::hint::explain(&song, &error);
    assert!(explained.starts_with(&hint) && explained.ends_with(&error.to_string()));
    // A misspelling, a unit left off, and a name with nothing near it.
    let near = |yaml: &str| {
        let song = yaml_load::load(yaml).unwrap();
        aaw_model::hint::hint(&song, &Project::validate(&song).unwrap_err()).unwrap()
    };
    assert_eq!(near("session: {tempoo: 90}"), "`session.tempoo` is not a field; did you mean `session.tempo`?");
    assert_eq!(near("session: {end_fade: 9}"), "`session.end_fade` is not a field; did you mean `session.end_fade_ms`?");
    assert!(near("session: {zzz: 1}").starts_with("`session.zzz` is not a field; session has title, tempo,"));
    // Errors of other kinds have no hint.
    let song = yaml_load::load("session: {tempo: 9000}").unwrap();
    assert!(aaw_model::hint::hint(&song, &Project::validate(&song).unwrap_err()).is_none());
}
