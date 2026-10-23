//! The ops the host takes, read from the enum's schema: their kinds, and a
//! batch's commands checked against them.

use aaw_host::command::{batch_commands, batchable, ops, Command, Kind};
use serde_json::{json, Map, Value as Json};

/// The least object an op's alternative accepts: its required keys, each
/// with a value of its type.
fn least(op: &str, alternative: &Json) -> Json {
    let mut object = Map::new();
    object.insert("op".into(), json!(op));
    for key in alternative["required"].as_array().unwrap().iter().filter_map(Json::as_str).filter(|k| *k != "op") {
        let node = &alternative["properties"][key];
        let value = match node["type"].as_str() {
            Some("string") => json!("x"),
            Some("boolean") => json!(false),
            Some("array") => json!([]),
            Some("object") => json!({}),
            _ => json!(0),
        };
        object.insert(key.into(), value);
    }
    Json::Object(object)
}

#[test]
fn an_ops_kind_is_its_commands() {
    let ops = ops();
    assert!(ops.len() > 100, "{}", ops.len());
    for (op, alternative) in &ops {
        let command: Command = serde_json::from_value(least(op, alternative)).unwrap_or_else(|e| panic!("{op}: {e}"));
        assert_eq!(command.op(), *op);
        assert_eq!(command.kind(), Kind::of(op), "{op}: Kind::of disagrees with Command::kind");
    }
    assert!(batchable("clip.add") && batchable("set") && batchable("event.add"));
    assert!(!batchable("apply") && !batchable("play") && !batchable("undo") && !batchable("inspect"));
}

#[test]
fn a_batch_refuses_an_op_or_a_key_with_the_nearest() {
    let ok = batch_commands(json!([{"op": "clip.add", "track": "drums", "pattern": "beat"}, {"op": "set", "path": "session.tempo", "value": 90}]));
    assert_eq!(ok.unwrap().len(), 2);
    let error = |list: Json| batch_commands(list).unwrap_err();
    assert_eq!(error(json!([{"op": "set", "path": "a", "value": 1}, {"op": "clip.ad", "track": "d"}])), "command 1: `clip.ad` is not an op; did you mean `clip.add`? daw describe batch lists them");
    assert!(error(json!([{"op": "pattern.event.add", "pattern": "p"}])).contains("did you mean `event.add`?"));
    assert!(error(json!([{"op": "lane.point.add", "owner": "tracks.a", "param": "pan"}])).contains("did you mean `point.add`?"));
    assert_eq!(error(json!([{"op": "play"}])), "command 0: a batch does not take `play`; run its command on its own");
    assert_eq!(
        error(json!([{"op": "clip.move", "clip": "@1", "beat": 8}])),
        "command 0: `beat` is not a key of clip.move, which takes clip, track, at"
    );
    assert!(error(json!([{"op": "clip.move", "clip": "@1", "att": 8}])).contains("did you mean `at`?"));
    // An op with fields takes other keys, which the song's model checks.
    assert!(batch_commands(json!([{"op": "pad.set", "track": "d", "pad": "k", "gain_db": -3}])).is_ok());
    assert!(error(json!([{"op": "clip.move"}])).contains("missing field `clip`"));
    assert!(error(json!({"op": "set"})).starts_with("A batch is a JSON list"));
}
