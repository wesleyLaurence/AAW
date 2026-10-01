//! The edits a person makes in the app, as the host's commands. The app names
//! objects by the keys the arrangement gave it, which are their handles, and
//! positions by how far things moved; the exact beats are worked out here from
//! the song, so a clip at a third of a beat stays on its third when it moves.

use crate::view::FieldValue;
use aaw_host::command::beat_value;
use aaw_host::session::{value_json, Doc};
use aaw_host::tree::{self, handle_text, Item, Node, Step};
use aaw_model::describe::{self, Initial};
use aaw_model::rules::{midi, note_name, target, Owner, TargetKind};
use aaw_model::{Beat, Effect, Project};
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};
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
    /// Sets steps of a pad's row in a pattern to a level: 0 for none, 1 to 9
    /// for the row's digits and 10 for an `x`. A pad gets a row with its
    /// first step, and a row left without steps goes.
    Steps {
        pattern: String,
        pad: String,
        steps: Vec<u32>,
        level: u32,
    },
    /// Adds an event to a pattern at a beat: on the pattern's grid, exactly,
    /// unless `free`. With `pitch` it has that note; `steps` is how many grid
    /// steps a gated pad is held for. The event is what the edit makes.
    EventAdd {
        pattern: String,
        pad: String,
        at: f64,
        free: bool,
        pitch: Option<i32>,
        steps: u32,
    },
    /// Moves an event later by `steps` grid steps and `by` beats, and up by
    /// `semitones` from its note, or from its sample's root when it has none.
    EventMove {
        event: u64,
        steps: i32,
        by: f64,
        semitones: i32,
    },
    /// Ends an event on a line of the pattern's grid: it is held from where
    /// it starts to there.
    EventEnd { event: u64, step: u32 },
    /// Sets fields of an event. Beats are as typed, such as `1/3` or `0.75`;
    /// an empty duration or note takes the field away.
    EventSet {
        event: u64,
        at: Option<String>,
        duration: Option<String>,
        velocity: Option<u32>,
        transpose: Option<f64>,
        note: Option<String>,
    },
    EventRemove { event: u64 },
    PatternSwing { pattern: String, swing: f64 },
    /// Changes a pattern's length, as typed. Step rows grow or shrink with
    /// it, and events past the new end go.
    PatternLength { pattern: String, beats: String },
    /// Changes the length of a pattern's steps, as typed, such as `1/8`. Step
    /// rows are written on the new grid when their hits fall on it.
    PatternGrid { pattern: String, grid: String },
    /// Adds a clip of a new, empty pattern one bar long to a track. The clip
    /// is what the edit makes.
    ClipNew { track: u64, at: f64 },
    /// Gives a clip a copy of its pattern, so that editing it leaves the
    /// other clips that play the pattern as they are.
    ClipOwnPattern { clip: u64 },
    /// Adds a sample that `library::import` copied into the project, as a
    /// pad of a track or, without one, of a new track at `index`. Pad and
    /// track are named after `name` as far as IDs allow. A new track is what
    /// the edit makes.
    SampleAdd {
        asset: crate::library::Asset,
        name: String,
        track: Option<u64>,
        index: u32,
    },
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

/// `stem` if it is free, else the first free of `stem-2`, `stem-3`, ….
fn unique(stem: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(stem) {
        return stem.to_string();
    }
    (2..).map(|n| format!("{stem}-{n}")).find(|name| !taken(name)).expect("a free name")
}

/// A name as an ID allows it: lower-case letters, digits, `-` and `_`,
/// starting with a letter, and not too long to read in a header.
pub fn ident(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    let out = if out.starts_with(|c: char| c.is_ascii_lowercase()) { out.to_string() } else { format!("s-{out}") };
    let out: String = out.chars().take(24).collect();
    match out.trim_matches('-') {
        "s" | "" => "sample".to_string(),
        name => name.to_string(),
    }
}

fn pattern<'a>(project: &'a Project, name: &str) -> Result<&'a aaw_model::Pattern> {
    project.patterns.get(name).ok_or_else(|| format!("The song has no pattern {name}"))
}

/// An event in the song: its pattern's name, the pattern and the event.
fn event<'a>(project: &'a Project, tree: &Node, key: u64) -> Result<(&'a str, &'a aaw_model::Pattern, &'a aaw_model::Event)> {
    let gone = || "The event is no longer in the song".to_string();
    match tree::find(tree, key).ok_or_else(gone)?.as_slice() {
        [Step::Key(k), Step::Key(name), Step::Key(l), Step::Index(i)] if k == "patterns" && l == "events" => {
            let (name, pattern) = project.patterns.get_key_value(name).ok_or_else(gone)?;
            Ok((name.as_str(), pattern, pattern.events.get(*i).ok_or_else(gone)?))
        }
        _ => Err("That is not a pattern's event".into()),
    }
}

/// A beat as typed: a number where it reads as one, else a fraction.
fn typed_beat(text: &str) -> Json {
    serde_json::from_str::<Json>(text.trim()).ok().filter(Json::is_number).unwrap_or(json!(text.trim()))
}

/// The exact beat a typed value names.
fn typed_exact(text: &str) -> Result<BigRational> {
    let value = match typed_beat(text) {
        Json::Number(n) if n.is_i64() => Beat::Int(n.as_i64().unwrap_or(0).into()),
        Json::Number(n) => Beat::Float(n.as_f64().unwrap_or(0.0)),
        _ => Beat::Str(text.trim().to_string()),
    };
    aaw_model::beat(&value)
}

/// A step's level as a row writes it.
fn step_char(level: u32) -> Result<char> {
    match level {
        0 => Ok('.'),
        1..=9 => Ok(char::from_digit(level, 10).expect("a digit")),
        10 => Ok('x'),
        _ => Err(format!("A step's level is 0 to 10, not {level}")),
    }
}

/// A step row written in groups of a beat where a beat is a few whole steps.
fn row_text(cells: &[char], grid: &BigRational) -> String {
    let per_beat = grid.recip();
    let group = match per_beat.is_integer().then(|| per_beat.to_integer().to_usize()).flatten() {
        Some(n) if (2..=8).contains(&n) => n,
        _ => cells.len().max(1),
    };
    cells.chunks(group).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join(" ")
}

/// How many steps a pattern of `length` has on `grid`, as its step rows need.
fn step_count(length: &BigRational, grid: &BigRational) -> Result<usize> {
    let steps = length / grid;
    steps
        .is_integer()
        .then(|| steps.to_integer().to_usize())
        .flatten()
        .ok_or_else(|| "The pattern's length is not a whole number of steps, so it cannot have step rows".to_string())
}

/// The command that writes a step row, or with no steps left removes it.
fn steps_command(name: &str, pad: &str, cells: &[char], grid: &BigRational, had: bool) -> Option<Json> {
    if cells.iter().any(|c| *c != '.') {
        Some(json!({"op": "pattern.steps", "pattern": name, "pad": pad, "row": row_text(cells, grid)}))
    } else {
        had.then(|| json!({"op": "pattern.steps", "pattern": name, "pad": pad}))
    }
}

/// The root note of the sample a pattern's pad plays, as a MIDI number: on a
/// track that plays the pattern, else on any track with such a pad.
fn pad_root(project: &Project, pattern: &str, pad: &str) -> Option<i64> {
    let root = |t: &aaw_model::Track| t.pads.get(pad).and_then(|p| crate::view::root(project, &p.sample));
    let plays = |t: &&aaw_model::Track| t.clips.iter().any(|c| c.pattern == pattern);
    project.tracks.iter().filter(plays).find_map(root).or_else(|| project.tracks.iter().find_map(root)).map(i64::from)
}

fn batch(commands: Vec<Json>, label: String) -> Vec<Json> {
    vec![json!({"op": "batch", "commands": commands, "label": label})]
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
        Edit::Steps { pattern: name, pad, steps, level } => {
            let p = pattern(project, name)?;
            let grid = p.grid_exact();
            let count = step_count(&p.length_exact(), &grid)?;
            let level = step_char(*level)?;
            if let Some(step) = steps.iter().find(|s| **s as usize >= count) {
                return Err(format!("Pattern {name} has {count} steps, so no step {}", step + 1));
            }
            // An existing row keeps its spaces and bar lines.
            let written = match p.steps.get(pad) {
                Some(row) => {
                    let mut cell = 0;
                    let row: String = row
                        .chars()
                        .map(|c| {
                            if aaw_model::pyfmt::is_py_space(c) || c == '|' {
                                return c;
                            }
                            cell += 1;
                            if steps.contains(&(cell - 1)) { level } else { c }
                        })
                        .collect();
                    if aaw_model::step_cells(&row).iter().any(|c| *c != '.') {
                        Some(json!({"op": "pattern.steps", "pattern": name, "pad": pad, "row": row}))
                    } else {
                        Some(json!({"op": "pattern.steps", "pattern": name, "pad": pad}))
                    }
                }
                None => {
                    let cells: Vec<char> = (0..count as u32).map(|i| if steps.contains(&i) { level } else { '.' }).collect();
                    steps_command(name, pad, &cells, &grid, false)
                }
            };
            Ok(written.into_iter().collect())
        }
        Edit::EventAdd { pattern: name, pad, at, free, pitch, steps } => {
            let p = pattern(project, name)?;
            let grid = p.grid_exact();
            let at = if *free {
                aaw_model::beat(&Beat::Float(at.max(0.0)))?
            } else {
                // The nearest step that starts inside the pattern.
                let step = (at.max(0.0) / grid.to_f64().unwrap_or(1.0)).round();
                let last = ((p.length_exact() / &grid).ceil() - BigRational::from_integer(1.into())).max(BigRational::zero());
                BigRational::from_float(step).unwrap_or_default().min(last) * &grid
            };
            let mut command = json!({"op": "event.add", "pattern": name, "at": beat(&at), "pad": pad});
            if let Some(pitch) = pitch {
                command["note"] = json!(note_name((*pitch).into())?);
            }
            if *steps > 0 {
                command["duration"] = beat(&(grid * BigRational::from_integer((*steps).into())));
            }
            Ok(vec![command])
        }
        Edit::EventMove { event: key, steps, by, semitones } => {
            if *steps == 0 && *by == 0.0 && *semitones == 0 {
                return Ok(Vec::new());
            }
            let tree = doc.tree();
            let (name, p, e) = event(project, &tree, *key)?;
            let mut command = json!({"op": "event.set", "event": handle_text(*key)});
            if *steps != 0 || *by != 0.0 {
                let distance = aaw_model::beat(&Beat::Float(by.abs()))?;
                let by = if *by < 0.0 { -distance } else { distance };
                let at = e.at_exact() + p.grid_exact() * BigRational::from_integer((*steps).into()) + by;
                if at < BigRational::zero() {
                    return Err("An event cannot start before its pattern".into());
                }
                command["at"] = beat(&at);
            }
            if *semitones != 0 {
                let from = match e.note.as_deref().filter(|n| !n.is_empty()) {
                    Some(note) => midi(note)?,
                    None => pad_root(project, name, &e.pad)
                        .ok_or_else(|| format!("Pad {} plays a sample without a root note, so its events have no pitch", e.pad))?,
                };
                command["note"] = json!(note_name(from + i64::from(*semitones))?);
            }
            Ok(vec![command])
        }
        Edit::EventEnd { event: key, step } => {
            let tree = doc.tree();
            let (_, p, e) = event(project, &tree, *key)?;
            let duration = p.grid_exact() * BigRational::from_integer((*step).into()) - e.at_exact();
            if duration <= BigRational::zero() {
                return Err("An event ends after it starts".into());
            }
            Ok(vec![json!({"op": "event.set", "event": handle_text(*key), "duration": beat(&duration)})])
        }
        Edit::EventSet { event: key, at, duration, velocity, transpose, note } => {
            let mut command = json!({"op": "event.set", "event": handle_text(*key)});
            if let Some(at) = at {
                command["at"] = typed_beat(at);
            }
            if let Some(duration) = duration {
                command["duration"] = if duration.trim().is_empty() { Json::Null } else { typed_beat(duration) };
            }
            if let Some(velocity) = velocity {
                command["velocity"] = json!(velocity);
            }
            if let Some(transpose) = transpose {
                command["transpose"] = number(*transpose);
            }
            if let Some(note) = note {
                command["note"] = if note.trim().is_empty() { Json::Null } else { json!(note.trim()) };
            }
            Ok(vec![command])
        }
        Edit::EventRemove { event } => Ok(vec![json!({"op": "event.remove", "event": handle_text(*event)})]),
        Edit::PatternSwing { pattern: name, swing } => {
            pattern(project, name)?;
            Ok(vec![json!({"op": "set", "path": format!("patterns.{name}.swing"), "value": swing})])
        }
        Edit::PatternLength { pattern: name, beats } => {
            let p = pattern(project, name)?;
            let length = typed_exact(beats)?;
            if length == p.length_exact() {
                return Ok(Vec::new());
            }
            let grid = p.grid_exact();
            let mut commands = vec![json!({"op": "set", "path": format!("patterns.{name}.length_beats"), "value": typed_beat(beats)})];
            if !p.steps.is_empty() {
                let count = step_count(&length, &grid)?;
                for (pad, row) in &p.steps {
                    let mut cells = aaw_model::step_cells(row);
                    cells.resize(count, '.');
                    commands.extend(steps_command(name, pad, &cells, &grid, true));
                }
            }
            // Events past the new end go, last first so that the others keep their places.
            let tree = doc.tree();
            let events = tree.get("patterns").and_then(|n| n.get(name)).map(|n| items(n, "events")).unwrap_or(&[]);
            for (e, item) in p.events.iter().zip(events).rev() {
                if e.at_exact() >= length {
                    commands.push(json!({"op": "event.remove", "event": handle_text(item.handle)}));
                }
            }
            Ok(batch(commands, format!("Set the length of pattern {name} to {} beats", beats.trim())))
        }
        Edit::PatternGrid { pattern: name, grid } => {
            let p = pattern(project, name)?;
            let (old, new) = (p.grid_exact(), typed_exact(grid)?);
            if new == old {
                return Ok(Vec::new());
            }
            if new <= BigRational::zero() {
                return Err("A step has a length".into());
            }
            let mut commands = vec![json!({"op": "set", "path": format!("patterns.{name}.grid"), "value": typed_beat(grid)})];
            if !p.steps.is_empty() {
                let count = step_count(&p.length_exact(), &new)?;
                let ratio = &old / &new;
                for (pad, row) in &p.steps {
                    let cells = aaw_model::step_cells(row);
                    let mut written = vec!['.'; count];
                    for (i, c) in cells.iter().enumerate().filter(|(_, c)| **c != '.') {
                        // Where the step falls on the new grid, if it does.
                        let at = BigRational::from_integer(i.into()) * &ratio;
                        let place = at.is_integer().then(|| at.to_integer().to_usize()).flatten().filter(|k| *k < count);
                        let Some(k) = place else {
                            return Err(format!("Step {} of {pad} does not fall on a grid of {} beats", i + 1, grid.trim()));
                        };
                        written[k] = *c;
                    }
                    commands.extend(steps_command(name, pad, &written, &new, true));
                }
            }
            Ok(batch(commands, format!("Set the grid of pattern {name} to {} beats", grid.trim())))
        }
        Edit::ClipNew { track, at } => {
            let tree = doc.tree();
            let place = items(&tree, "tracks").iter().position(|i| i.handle == *track);
            let t = &project.tracks[place.ok_or("The track is no longer in the song")?];
            let at = aaw_model::beat(&Beat::Float(at.max(0.0)))?;
            // One bar, or what is left of the song.
            let left = project.session.length_exact() - &at;
            if left <= BigRational::zero() {
                return Err("A clip starts inside the song".into());
            }
            let length = left.min(BigRational::from_integer(4.into()));
            let name = (1..).map(|n| format!("{}-{n}", t.id)).find(|n| !project.patterns.contains_key(n)).expect("a free name");
            Ok(batch(
                vec![
                    json!({"op": "pattern.add", "pattern": name, "length_beats": beat(&length)}),
                    json!({"op": "clip.add", "track": handle_text(*track), "pattern": name, "at": beat(&at)}),
                ],
                format!("Add clip {name} to {}", t.id),
            ))
        }
        Edit::ClipOwnPattern { clip: key } => {
            let tree = doc.tree();
            let at = placed(project, &tree, *key)?;
            let j = items(&items(&tree, "tracks")[at.track].node, "clips").iter().position(|c| c.handle == *key).expect("the clip");
            let from = &project.tracks[at.track].clips[j].pattern;
            // A copy of `verse` is `verse-2`, and a copy of that `verse-3`.
            let stem = match from.rsplit_once('-') {
                Some((stem, n)) if !stem.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => stem,
                _ => from.as_str(),
            };
            let to = (2..).map(|n| format!("{stem}-{n}")).find(|n| !project.patterns.contains_key(n)).expect("a free name");
            Ok(batch(
                vec![
                    json!({"op": "pattern.duplicate", "pattern": from, "to": to}),
                    json!({"op": "set", "path": format!("{}.pattern", clip(key)), "value": to}),
                ],
                format!("Give clip {from} at {} its own pattern {to}", beat(&at.start)),
            ))
        }
        Edit::SampleAdd { asset, name, track, index } => {
            let tree = doc.tree();
            let stem = ident(name);
            let mut commands = Vec::new();
            // A file that is in the song already is the same sample.
            let sample = match project.samples.iter().find(|(_, s)| s.path == asset.path) {
                Some((id, _)) => id.clone(),
                None => {
                    let id = unique(&stem, |n| project.samples.contains_key(n));
                    let mut entry = json!({"path": asset.path, "sha256": asset.sha256, "source": asset.source});
                    if let Some(sha) = &asset.source_sha256 {
                        entry["source_sha256"] = json!(sha);
                    }
                    if let Some(root) = &asset.root_note {
                        entry["root_note"] = json!(root);
                    }
                    commands.push(json!({"op": "set", "path": format!("samples.{id}"), "value": entry}));
                    id
                }
            };
            let label = match track {
                Some(key) => {
                    let place = items(&tree, "tracks").iter().position(|i| i.handle == *key);
                    let t = &project.tracks[place.ok_or("The track is no longer in the song")?];
                    let pad = unique(&stem, |n| t.pads.contains_key(n));
                    commands.push(json!({"op": "pad.add", "track": handle_text(*key), "pad": pad, "sample": sample}));
                    format!("Add pad {pad} to {}", t.id)
                }
                None => {
                    let taken = |n: &str| project.tracks.iter().any(|t| t.id == n) || project.returns.iter().any(|r| r.id == n);
                    let id = unique(&stem, taken);
                    commands.push(json!({"op": "track.add", "id": id, "index": (*index as usize).min(project.tracks.len())}));
                    commands.push(json!({"op": "pad.add", "track": id, "pad": stem, "sample": sample}));
                    format!("Add track {id} with pad {stem}")
                }
            };
            Ok(batch(commands, label))
        }
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
