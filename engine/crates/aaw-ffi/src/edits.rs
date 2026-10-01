//! The edits a person makes in the app, as the host's commands. The app names
//! objects by the keys the arrangement gave it, which are their handles, and
//! positions by how far things moved; the exact beats are worked out here from
//! the song, so a clip at a third of a beat stays on its third when it moves.

use crate::view::FieldValue;
use aaw_host::command::beat_value;
use aaw_host::session::{value_json, Doc};
use aaw_host::tree::{self, handle_text, Item, Node, Step};
use aaw_model::describe::{self, Initial};
use aaw_model::rules::{target, Owner, TargetKind};
use aaw_model::{Beat, Effect, Project};
use num_rational::BigRational;
use num_traits::Zero;
use serde_json::{json, Value as Json};

/// A row of the arrangement.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Row {
    Track { key: u64 },
    Return { key: u64 },
    Master,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Edit {
    /// Volume in dB: of a track, a return or, for the master, the song.
    Gain { row: Row, db: f64 },
    Pan { row: Row, pan: f64 },
    Mute { row: Row, on: bool },
    Solo { track: u64, on: bool },
    /// A send's level in dB; the send is added if the track has none there.
    Send { track: u64, to: String, db: f64 },
    SendRemove { track: u64, to: String },
    /// Moves clips later by `by` beats and down by `rows` tracks.
    ClipsMove { clips: Vec<u64>, by: f64, rows: i32 },
    ClipRepeats { clip: u64, repeats: u32 },
    /// Copies clips to right after the span they cover together, so that a
    /// group repeats as a group. The copies are what the edit makes.
    ClipsDuplicate { clips: Vec<u64> },
    ClipsRemove { clips: Vec<u64> },
    /// Adds an empty track at `index` among the tracks, under a free name.
    TrackAdd { index: u32 },
    ReturnAdd { index: u32 },
    Rename { row: Row, to: String },
    Remove { row: Row },
    /// Moves a track among the tracks, or a return among the returns.
    Move { row: Row, index: u32 },
    /// Adds an effect of a type to a row's chain, at `index` or last, with
    /// its required fields at a place to start. The effect is what the edit
    /// makes.
    EffectAdd { row: Row, kind: String, index: Option<u32> },
    EffectRemove { effect: u64 },
    /// Moves an effect within its chain.
    EffectMove { effect: u64, index: u32 },
    EffectBypass { effect: u64, on: bool },
    /// Sets a field of an effect, named as its `FieldView` names it.
    EffectSet { effect: u64, field: String, value: FieldValue },
    /// Adds a band to an equalizer.
    BandAdd { effect: u64 },
    /// Removes an equalizer's band, with the lanes that move it; lanes on
    /// later bands follow them down.
    BandRemove { effect: u64, band: u32 },
    /// Gives a parameter a lane: one point at the start with the value the
    /// parameter has, which changes nothing until more are added.
    LaneAdd { row: Row, param: String },
    LaneRemove { lane: u64 },
    /// Adds a point to a lane. The point is what the edit makes.
    PointAdd { lane: u64, at: f64, value: f64 },
    /// Moves a point in time, in value, or both, or changes whether it holds.
    PointSet {
        point: u64,
        at: Option<f64>,
        value: Option<f64>,
        hold: Option<bool>,
    },
    /// Removes a point; a lane's last point takes the lane with it.
    PointRemove { point: u64 },
}

type Result<T> = std::result::Result<T, String>;

/// A whole number without a fraction part, as a person would write it.
fn number(x: f64) -> Json {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        json!(x as i64)
    } else {
        json!(x)
    }
}

fn path(row: &Row, field: &str) -> Result<String> {
    match row {
        Row::Track { key } => Ok(format!("tracks.{}.{field}", handle_text(*key))),
        Row::Return { key } => Ok(format!("returns.{}.{field}", handle_text(*key))),
        Row::Master if field == "gain_db" => Ok("session.master_gain_db".into()),
        Row::Master => Err(format!("The master has no {field}")),
    }
}

fn set(row: &Row, field: &str, value: Json) -> Result<Vec<Json>> {
    Ok(vec![json!({"op": "set", "path": path(row, field)?, "value": value})])
}

fn items<'a>(node: &'a Node, key: &str) -> &'a [Item] {
    node.get(key).map(Node::items).unwrap_or(&[])
}

/// A clip in the song: its track's place among the tracks and its exact span.
struct Placed {
    track: usize,
    start: BigRational,
    end: BigRational,
}

fn placed(project: &Project, tree: &Node, key: u64) -> Result<Placed> {
    for (i, track) in items(tree, "tracks").iter().enumerate() {
        if let Some(j) = items(&track.node, "clips").iter().position(|c| c.handle == key) {
            let clip = &project.tracks[i].clips[j];
            let length = project.patterns.get(&clip.pattern).map(|p| p.length_exact()).unwrap_or_else(BigRational::zero);
            let start = clip.at_exact();
            let end = &start + length * BigRational::from_integer(clip.repeats.into());
            return Ok(Placed { track: i, start, end });
        }
    }
    Err("The clip is no longer in the song".into())
}

fn beat(x: &BigRational) -> Json {
    value_json(&beat_value(x))
}

/// A beat a pointer gave, written exactly: on the grid it is a whole number
/// or a short decimal.
fn beat_at(x: f64) -> Result<Json> {
    Ok(beat(&aaw_model::beat(&Beat::Float(x.max(0.0)))?))
}

fn owner_path(row: &Row) -> String {
    match row {
        Row::Track { key } => format!("tracks.{}", handle_text(*key)),
        Row::Return { key } => format!("returns.{}", handle_text(*key)),
        Row::Master => "master".into(),
    }
}

/// The owner a row names.
fn owner<'a>(project: &'a Project, tree: &Node, row: &Row) -> Result<Owner<'a>> {
    let place = |list: &str, key: u64| items(tree, list).iter().position(|i| i.handle == key);
    match row {
        Row::Track { key } => place("tracks", *key).map(|i| Owner::Track(&project.tracks[i])),
        Row::Return { key } => place("returns", *key).map(|i| Owner::Return(&project.returns[i])),
        Row::Master => Some(Owner::Master(&project.master)),
    }
    .ok_or_else(|| "The row is no longer in the song".to_string())
}

/// The owner whose list `list` holds the item at `loc`, with the item's place.
fn owner_of<'a>(project: &'a Project, loc: &[Step], list: &str) -> Option<(Owner<'a>, usize)> {
    match loc {
        [Step::Key(k), Step::Index(i), Step::Key(l), Step::Index(j), ..] if l == list => match k.as_str() {
            "tracks" => Some((Owner::Track(project.tracks.get(*i)?), *j)),
            "returns" => Some((Owner::Return(project.returns.get(*i)?), *j)),
            _ => None,
        },
        [Step::Key(k), Step::Key(l), Step::Index(j), ..] if k == "master" && l == list => Some((Owner::Master(&project.master), *j)),
        _ => None,
    }
}

/// A field value as the song writes it.
fn field_json(value: &FieldValue) -> Json {
    match value {
        FieldValue::Number { value } => number(*value),
        // A beat is a number where it reads as one, else a fraction.
        FieldValue::Text { value } => serde_json::from_str::<Json>(value).ok().filter(Json::is_number).unwrap_or(json!(value)),
        FieldValue::Flag { value } => json!(value),
        FieldValue::Absent => Json::Null,
    }
}

fn initial_json(i: Initial) -> Option<Json> {
    match i {
        Initial::Number(x) => Some(number(x)),
        Initial::Text(s) => Some(json!(s)),
        Initial::Flag(b) => Some(json!(b)),
        Initial::Required | Initial::Absent => None,
    }
}

/// A new band, or a new effect's required fields, at their places to start.
fn start(fields: &[describe::Field]) -> serde_json::Map<String, Json> {
    fields
        .iter()
        .filter(|f| f.default == Initial::Required)
        .filter_map(|f| Some((f.name.to_string(), initial_json(f.suggested)?)))
        .collect()
}

/// The value a parameter has without a lane.
fn static_value(project: &Project, owner: Owner, param: &str) -> Result<f64> {
    let t = target(owner, param)?;
    let missing = || format!("{param} has no value");
    match (&t.kind, owner) {
        (TargetKind::Channel, Owner::Master(_)) => Ok(project.session.master_gain_db),
        (TargetKind::Channel, Owner::Track(x)) => Ok(if t.field == "pan" { x.pan } else { x.gain_db }),
        (TargetKind::Channel, Owner::Return(x)) => Ok(if t.field == "pan" { x.pan } else { x.gain_db }),
        (TargetKind::Send(to), Owner::Track(x)) => x.sends.iter().find(|s| &s.to == to).map(|s| s.gain_db).ok_or_else(missing),
        (TargetKind::Effect { index, band }, _) => {
            let dump = owner.effects()[*index].dump(false);
            let holder = match band {
                Some(b) => match dump.get("bands") {
                    Some(aaw_model::value::Value::List(bands)) => bands.get(*b).cloned(),
                    _ => None,
                },
                None => Some(dump),
            };
            match holder.as_ref().and_then(|h| h.get(&t.field)) {
                Some(aaw_model::value::Value::Float(x)) => Ok(*x),
                _ => Err(missing()),
            }
        }
        _ => Err(missing()),
    }
}

/// The first of `track-1`, `track-2`, … that no track or return has.
fn free_name(project: &Project, stem: &str) -> String {
    let taken = |name: &str| project.tracks.iter().any(|t| t.id == name) || project.returns.iter().any(|r| r.id == name);
    (1..).map(|n| format!("{stem}-{n}")).find(|name| !taken(name)).expect("a free name")
}

/// The commands that make `edit` on the song in `doc`: none when the edit
/// changes nothing, and several for an edit of several objects.
pub fn commands(doc: &Doc, edit: &Edit) -> Result<Vec<Json>> {
    let project = &doc.project;
    let clip = |key: &u64| handle_text(*key);
    match edit {
        Edit::Gain { row, db } => set(row, "gain_db", number(*db)),
        Edit::Pan { row, pan } => set(row, "pan", number(*pan)),
        Edit::Mute { row, on } => set(row, "mute", json!(on)),
        Edit::Solo { track, on } => set(&Row::Track { key: *track }, "solo", json!(on)),
        Edit::Send { track, to, db } => Ok(vec![json!({
            "op": "send.set", "track": handle_text(*track), "to": to, "gain_db": number(*db),
        })]),
        Edit::SendRemove { track, to } => Ok(vec![json!({"op": "send.remove", "track": handle_text(*track), "to": to})]),
        Edit::ClipsMove { clips, by, rows } => {
            if *by == 0.0 && *rows == 0 {
                return Ok(Vec::new());
            }
            let tree = doc.tree();
            let tracks = items(&tree, "tracks");
            // The model's beats are positions, so the sign is put back after.
            let distance = aaw_model::beat(&Beat::Float(by.abs()))?;
            let by_exact = if *by < 0.0 { -distance } else { distance };
            clips
                .iter()
                .map(|key| {
                    let at = placed(project, &tree, *key)?;
                    let mut command = json!({"op": "clip.move", "clip": clip(key)});
                    if *by != 0.0 {
                        command["at"] = beat(&(at.start + &by_exact));
                    }
                    if *rows != 0 {
                        let to = usize::try_from(at.track as i64 + *rows as i64).ok().and_then(|i| tracks.get(i));
                        let to = to.ok_or("There is no track to move the clip to")?;
                        command["track"] = json!(handle_text(to.handle));
                    }
                    Ok(command)
                })
                .collect()
        }
        Edit::ClipRepeats { clip: key, repeats } => Ok(vec![json!({"op": "clip.repeats", "clip": clip(key), "repeats": repeats})]),
        Edit::ClipsDuplicate { clips } => {
            let tree = doc.tree();
            let spans = clips.iter().map(|key| placed(project, &tree, *key)).collect::<Result<Vec<_>>>()?;
            let (Some(start), Some(end)) = (spans.iter().map(|s| &s.start).min(), spans.iter().map(|s| &s.end).max()) else {
                return Ok(Vec::new());
            };
            let span = end - start;
            Ok(clips
                .iter()
                .zip(&spans)
                .map(|(key, s)| json!({"op": "clip.duplicate", "clip": clip(key), "at": beat(&(&s.start + &span))}))
                .collect())
        }
        Edit::ClipsRemove { clips } => Ok(clips.iter().map(|key| json!({"op": "clip.remove", "clip": clip(key)})).collect()),
        Edit::TrackAdd { index } => Ok(vec![json!({
            "op": "track.add", "id": free_name(project, "track"), "index": (*index as usize).min(project.tracks.len()),
        })]),
        Edit::ReturnAdd { index } => Ok(vec![json!({
            "op": "return.add", "id": free_name(project, "return"), "index": (*index as usize).min(project.returns.len()),
        })]),
        Edit::Rename { row, to } => match row {
            Row::Track { key } => Ok(vec![json!({"op": "track.rename", "track": handle_text(*key), "to": to})]),
            Row::Return { key } => Ok(vec![json!({"op": "return.rename", "return": handle_text(*key), "to": to})]),
            Row::Master => Err("The master cannot be renamed".into()),
        },
        Edit::Remove { row } => match row {
            Row::Track { key } => Ok(vec![json!({"op": "track.remove", "track": handle_text(*key)})]),
            Row::Return { key } => Ok(vec![json!({"op": "return.remove", "return": handle_text(*key)})]),
            Row::Master => Err("The master cannot be removed".into()),
        },
        Edit::Move { row, index } => match row {
            Row::Track { key } => Ok(vec![json!({"op": "track.move", "track": handle_text(*key), "index": index})]),
            Row::Return { key } => Ok(vec![json!({"op": "return.move", "return": handle_text(*key), "index": index})]),
            Row::Master => Err("The master cannot be moved".into()),
        },
        Edit::EffectAdd { row, kind, index } => {
            let mut command = json!({"op": "effect.add", "owner": owner_path(row), "type": kind});
            if let Some(i) = index {
                command["index"] = json!(i);
            }
            let fields = command.as_object_mut().expect("an object");
            match kind.as_str() {
                "eq" => {
                    fields.insert("bands".into(), json!([start(describe::BAND)]));
                }
                _ => fields.extend(start(describe::effect(kind))),
            }
            // A delay or reverb is all wet, as a return needs; as an insert it
            // starts mixed in under the dry signal.
            if !matches!(row, Row::Return { .. }) && matches!(kind.as_str(), "delay" | "reverb") {
                fields.insert("mix_percent".into(), json!(25));
            }
            Ok(vec![command])
        }
        Edit::EffectRemove { effect } => Ok(vec![json!({"op": "effect.remove", "effect": handle_text(*effect)})]),
        Edit::EffectMove { effect, index } => Ok(vec![json!({"op": "effect.move", "effect": handle_text(*effect), "index": index})]),
        Edit::EffectBypass { effect, on } => Ok(vec![json!({"op": "effect.bypass", "effect": handle_text(*effect), "bypass": on})]),
        Edit::EffectSet { effect, field, value } => Ok(vec![json!({
            "op": "set", "path": format!("{}.{field}", handle_text(*effect)), "value": field_json(value),
        })]),
        Edit::BandAdd { effect } => {
            let tree = doc.tree();
            let loc = tree::find(&tree, *effect).ok_or("The effect is no longer in the song")?;
            let mut bands = match tree::get(&tree, &loc).get("bands") {
                Some(bands) => aaw_host::command::node_json(bands),
                None => return Err("Only an equalizer has bands".into()),
            };
            bands.as_array_mut().ok_or("Only an equalizer has bands")?.push(Json::Object(start(describe::BAND)));
            Ok(vec![json!({"op": "set", "path": format!("{}.bands", handle_text(*effect)), "value": bands})])
        }
        Edit::BandRemove { effect, band } => {
            let tree = doc.tree();
            let loc = tree::find(&tree, *effect).ok_or("The effect is no longer in the song")?;
            let (owner, index) = owner_of(project, &loc, "effects").ok_or("The effect is no longer in the song")?;
            let Effect::Eq(eq) = &owner.effects()[index] else {
                return Err("Only an equalizer has bands".into());
            };
            let band = *band as usize;
            if band >= eq.bands.len() {
                return Err(format!("The equalizer has {} bands", eq.bands.len()));
            }
            let owner_path = tree::path_text(&tree, &loc[..loc.len() - 2]);
            let mut commands = Vec::new();
            // Lanes on the band go; lanes on later bands are named for their
            // bands' new places. They are set aside while the bands change.
            let mut moved = Vec::new();
            for lane in owner.automation() {
                let Ok(t) = target(owner, &lane.param) else { continue };
                let TargetKind::Effect { index: i, band: Some(b) } = t.kind else { continue };
                if i != index || b < band {
                    continue;
                }
                commands.push(json!({"op": "lane.remove", "owner": owner_path, "param": lane.param}));
                if b > band {
                    let reference = lane.param.split('.').nth(1).unwrap_or_default();
                    let points: Vec<Json> = lane.points.iter().map(|p| value_json(&p.dump(true))).collect();
                    moved.push(json!({
                        "op": "lane.set", "owner": owner_path,
                        "param": format!("effects.{reference}.bands.{}.{}", b - 1, t.field), "points": points,
                    }));
                }
            }
            let mut bands = aaw_host::command::node_json(tree::get(&tree, &loc).get("bands").expect("an equalizer's bands"));
            bands.as_array_mut().expect("a list").remove(band);
            commands.push(json!({"op": "set", "path": format!("{}.bands", handle_text(*effect)), "value": bands}));
            commands.extend(moved);
            Ok(commands)
        }
        Edit::LaneAdd { row, param } => {
            let tree = doc.tree();
            let value = static_value(project, owner(project, &tree, row)?, param)?;
            Ok(vec![json!({
                "op": "lane.set", "owner": owner_path(row), "param": param, "points": [{"at": 0, "value": number(value)}],
            })])
        }
        Edit::LaneRemove { lane } => Ok(vec![json!({"op": "remove", "path": handle_text(*lane)})]),
        Edit::PointAdd { lane, at, value } => {
            let tree = doc.tree();
            let loc = tree::find(&tree, *lane).ok_or("The lane is no longer in the song")?;
            let param = tree::get(&tree, &loc).field("param").ok_or("That is not a lane")?.to_string();
            Ok(vec![json!({
                "op": "point.add", "owner": tree::path_text(&tree, &loc[..loc.len() - 2]), "param": param,
                "at": beat_at(*at)?, "value": number(*value),
            })])
        }
        Edit::PointSet { point, at, value, hold } => {
            let mut command = json!({"op": "point.set", "point": handle_text(*point)});
            if let Some(at) = at {
                command["at"] = beat_at(*at)?;
            }
            if let Some(value) = value {
                command["value"] = number(*value);
            }
            if let Some(hold) = hold {
                command["curve"] = json!(if *hold { "hold" } else { "linear" });
            }
            Ok(vec![command])
        }
        Edit::PointRemove { point } => Ok(vec![json!({"op": "point.remove", "point": handle_text(*point)})]),
    }
}

/// What an edit is called in the change log and for undo, where the commands
/// that make it do not say it well: an edit of `count` clips, or of an
/// equalizer's bands, which is a new list of them to the host.
pub fn label(edit: &Edit, count: usize) -> Option<String> {
    let verb = match edit {
        Edit::BandAdd { .. } => return Some("Add a band to an equalizer".into()),
        Edit::BandRemove { band, .. } => return Some(format!("Remove band {} of an equalizer", band + 1)),
        _ if count < 2 => return None,
        Edit::ClipsMove { .. } => "Move",
        Edit::ClipsDuplicate { .. } => "Duplicate",
        Edit::ClipsRemove { .. } => "Remove",
        _ => return None,
    };
    Some(format!("{verb} {count} clips"))
}

/// The keys of the objects an edit made, from the host's reply.
pub fn made(reply: &Json) -> Vec<u64> {
    let key = |h: &Json| h.as_str()?.strip_prefix('@')?.parse().ok();
    match reply.get("handles").and_then(Json::as_array) {
        Some(handles) => handles.iter().filter_map(key).collect(),
        None => reply.get("handle").and_then(key).into_iter().collect(),
    }
}
