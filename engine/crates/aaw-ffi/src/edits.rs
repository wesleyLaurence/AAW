//! The edits a person makes in the app, as the host's commands. The app names
//! objects by the keys the arrangement gave it, which are their handles, and
//! positions by how far things moved; the exact beats are worked out here from
//! the song, so a clip at a third of a beat stays on its third when it moves.

use aaw_host::command::beat_value;
use aaw_host::session::{value_json, Doc};
use aaw_host::tree::{handle_text, Item, Node};
use aaw_model::{Beat, Project};
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
    }
}

/// What an edit of `count` objects is called in the change log and for undo.
pub fn label(edit: &Edit, count: usize) -> Option<String> {
    let verb = match edit {
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
