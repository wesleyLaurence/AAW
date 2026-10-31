//! `daw describe batch`: every op a batch takes, made from the host's
//! commands, with the command each is the same as and what its keys take.

use aaw_host::command;
use aaw_model::contract;
use serde_json::{json, Map, Value as Json};

/// What the keys a command takes as any JSON value take, by name. A test
/// holds every such key of an op a batch takes to a line here.
const UNTYPED: &[(&str, &str)] = &[
    ("value", "any JSON value"),
    ("at", "beats"),
    ("start", "beats"),
    ("end", "beats"),
    ("length", "beats"),
    ("length_beats", "beats"),
    ("loop_beats", "beats"),
    ("from", "beats"),
    ("to", "beats"),
    ("by", "beats"),
    ("grid", "beats, counted from the clip's start: 1/4 is a sixteenth note"),
    ("repeats", "integer >=1"),
    ("times", "integer >=1"),
    ("pitch", "a note name such as C4, or a MIDI number"),
    ("amount", "number, in the target's unit (daw describe synth, modulation)"),
    ("instrument", "an instrument object or null (daw describe midi)"),
    ("points", "list of points, each {at, value, curve, shape} (daw describe automation)"),
];

/// The objects whose fields an op takes as keys besides its own, by model,
/// for the ops that add or change one. A test holds every such op to a
/// line here.
const FIELDS: &[(&str, &[&str])] = &[
    ("track.add", &["Track"]),
    ("return.add", &["Return"]),
    ("group.add", &["Group"]),
    ("clip.add", &["Clip", "NoteClip"]),
    ("note.add", &["Note"]),
    ("note.set", &["Note"]),
    ("instrument.map", &["NoteMap"]),
    ("audio.add", &["AudioClip"]),
    ("pattern.add", &["Pattern"]),
    ("event.add", &["Event"]),
    ("event.set", &["Event"]),
    ("pad.add", &["Pad"]),
    ("pad.set", &["Pad"]),
    ("effect.add", &[]),
    ("send.set", &["Send"]),
    ("point.add", &["Point"]),
    ("point.set", &["Point"]),
];

/// The command an op is the same as: its words, but for the ops of
/// commands nested deeper than the op's name says, and `point.set`, which
/// is `lane point move`.
fn command_words(op: &str) -> Vec<&str> {
    if op == "point.set" {
        return vec!["lane", "point", "move"];
    }
    let mut words: Vec<&str> = op.split('.').collect();
    match words[0] {
        "event" => words.insert(0, "pattern"),
        "point" => words.insert(0, "lane"),
        _ => {}
    }
    words
}

pub fn same_as(cli: &clap::Command, op: &str) -> Option<String> {
    let words = command_words(op);
    let mut at = cli;
    for word in &words {
        at = at.find_subcommand(word)?;
    }
    Some(format!("daw {}", words.join(" ")))
}

/// What a key takes, in the one-line form `describe` gives fields.
fn key_line(key: &str, node: &Json, required: bool) -> String {
    let types: Vec<&str> = match &node["type"] {
        Json::String(t) => vec![t.as_str()],
        Json::Array(ts) => ts.iter().filter_map(Json::as_str).filter(|t| *t != "null").collect(),
        _ => vec![],
    };
    let mut line = match types.as_slice() {
        ["string"] => "text".to_string(),
        ["boolean"] => "true|false".to_string(),
        ["number"] => "number".to_string(),
        ["integer"] if node.get("minimum").is_some() => format!("integer >={}", node["minimum"]),
        ["integer"] => "integer".to_string(),
        ["object"] => "object".to_string(),
        ["array"] => match node["items"]["type"].as_str() {
            Some("string") => "list of text".to_string(),
            Some("integer") => "list of integers".to_string(),
            Some("object") => "list of objects".to_string(),
            _ => "list".to_string(),
        },
        _ => UNTYPED.iter().find(|(k, _)| *k == key).map_or("any JSON value", |(_, l)| l).to_string(),
    };
    if let Some(d) = node.get("default") {
        line.push_str(&format!(", default {d}"));
    }
    if required {
        line.push_str(", required");
    }
    if let Some(text) = node["description"].as_str() {
        line.push_str(&format!("; {}", text.split_whitespace().collect::<Vec<_>>().join(" ")));
    }
    line
}

/// The keys of an op besides `op`, each with its line.
fn keys(alternative: &Json) -> Map<String, Json> {
    let required = |k: &str| alternative["required"].as_array().is_some_and(|r| r.iter().any(|x| x == k));
    alternative["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, _)| *k != "op")
        .map(|(k, node)| (k.clone(), json!(key_line(k, node, required(k)))))
        .collect()
}

/// The objects whose fields an op takes as keys besides its own, in words.
fn fields(op: &str) -> Option<String> {
    let (_, models) = FIELDS.iter().find(|(o, _)| *o == op)?;
    if op == "effect.add" {
        return Some("type, an effect kind, and that kind's fields (daw describe effects); on a MIDI track a MIDI effect's (daw describe midi); a plugin's slug and its manifest's fields".into());
    }
    let named: Vec<String> =
        models.iter().map(|m| contract::field_line(&json!({"$ref": format!("#/$defs/{m}")}), false)).collect();
    let mut text = format!("{}: its fields, each a key, as the topic names them", named.join(" or "));
    if op == "track.add" {
        text.push_str("; type: midi makes a MIDI track");
    }
    Some(text)
}

/// What `daw describe batch` prints.
pub fn describe(cli: &clap::Command, with_schema: bool) -> Json {
    let mut ops = Map::new();
    for (op, alternative) in command::ops().into_iter().filter(|(op, _)| command::batchable(op)) {
        let mut entry = Map::new();
        entry.insert("command".into(), json!(same_as(cli, &op)));
        let keys = keys(&alternative);
        if !keys.is_empty() {
            entry.insert("keys".into(), Json::Object(keys));
        }
        if let Some(f) = fields(&op) {
            entry.insert("fields".into(), json!(f));
        }
        ops.insert(op, Json::Object(entry));
    }
    let mut out = json!({
        "ops": ops,
        "semantics": {
            "file": "daw batch PROJECT FILE runs a file of commands as one edit and one undo step: a JSON list of commands, or {\"label\": \"...\", \"commands\": [...]}, the label naming the step in the change log and for undo. A command is an object whose op is one of these and whose other keys are its arguments. When one command is refused, nothing of the batch is applied. daw describe start has a batch file as an example.",
            "ops": "An op is the words of the command it is the same as, joined by dots: clip.add is daw clip add. The pattern's events are event.add, event.set and event.remove, for daw pattern event; a lane's points are point.add, point.set and point.remove, for daw lane point add, move and remove.",
            "keys": "A key's line says what it takes, its default and whether it is required; beats are a number or a fraction such as \"1/3\". A key that names a clip, note, event, point or effect takes its path, such as tracks.drums.clips.0, or @N while a host runs. An op with fields takes, besides its own keys, the fields of the object it adds or changes, each a key named as describe lists it: length_beats where the command's flag is --length-beats.",
            "refused": "An op a batch does not take, or a key an op does not have, is refused with the nearest. A batch takes edits; apply, undo and redo, the transport (play, stop, locate, loop, record) and what reads the song (inspect, get, map) are run as their own commands.",
        },
    });
    if with_schema {
        out["schema"] = command::schema();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn every_op_a_batch_takes_is_described() {
        let cli = crate::Cli::command();
        let defs = &contract::schema()["$defs"];
        for (op, alternative) in command::ops().into_iter().filter(|(op, _)| command::batchable(op)) {
            assert!(same_as(&cli, &op).is_some(), "{op} names no command; teach command_words where it is");
            for (key, node) in alternative["properties"].as_object().unwrap() {
                let untyped = node.get("type").is_none();
                assert!(!untyped || UNTYPED.iter().any(|(k, _)| k == key), "{op}'s {key} takes any JSON; say what in UNTYPED");
            }
            let flattened = alternative.get("additionalProperties").is_some();
            assert_eq!(flattened, FIELDS.iter().any(|(o, _)| *o == op), "{op}: FIELDS should name exactly the ops with fields");
        }
        for (_, models) in FIELDS {
            for model in *models {
                assert!(defs.get(*model).is_some(), "{model} is not a model");
            }
        }
    }
}
