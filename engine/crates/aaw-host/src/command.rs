//! Commands: every change to the song, and the transport, history and reading
//! requests a host answers. Edits are small changes to the song tree; the
//! session validates the whole song after each one and applies it only if valid.
//!
//! Commands are JSON objects tagged by `op`, the form they take over the socket,
//! in `batch` files and in the change log. Objects added by a command take their
//! fields from the command's other keys, e.g.
//! `{"op": "clip.add", "track": "drums", "pattern": "verse", "at": 8}`.

use crate::tree::{self, Item, Loc, Node, Step};
use aaw_model::value::Value;
use aaw_model::Beat;
use num_rational::BigRational;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

pub type Fields = Map<String, Json>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    User,
    Agent,
    External,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::User => "user",
            Origin::Agent => "agent",
            Origin::External => "external",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Command {
    // Values
    #[serde(rename = "set")]
    Set { path: String, value: Json },
    #[serde(rename = "toggle")]
    Toggle { path: String },
    #[serde(rename = "remove")]
    Remove { path: String },

    // Tracks and returns
    #[serde(rename = "track.add")]
    TrackAdd {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "track.remove")]
    TrackRemove { track: String },
    #[serde(rename = "track.rename")]
    TrackRename { track: String, to: String },
    #[serde(rename = "track.move")]
    TrackMove { track: String, index: usize },
    #[serde(rename = "return.add")]
    ReturnAdd {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "return.remove")]
    ReturnRemove {
        #[serde(rename = "return")]
        id: String,
    },
    #[serde(rename = "return.rename")]
    ReturnRename {
        #[serde(rename = "return")]
        id: String,
        to: String,
    },

    // Clips
    #[serde(rename = "clip.add")]
    ClipAdd {
        track: String,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "clip.move")]
    ClipMove {
        clip: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        track: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Json>,
    },
    #[serde(rename = "clip.repeats")]
    ClipRepeats { clip: String, repeats: Json },
    #[serde(rename = "clip.duplicate")]
    ClipDuplicate {
        clip: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        track: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Json>,
    },
    #[serde(rename = "clip.remove")]
    ClipRemove { clip: String },

    // Patterns
    #[serde(rename = "pattern.add")]
    PatternAdd {
        pattern: String,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "pattern.steps")]
    PatternSteps {
        pattern: String,
        pad: String,
        /// None removes the row.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        row: Option<String>,
    },
    #[serde(rename = "pattern.duplicate")]
    PatternDuplicate { pattern: String, to: String },
    #[serde(rename = "event.add")]
    EventAdd {
        pattern: String,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "event.set")]
    EventSet {
        event: String,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "event.remove")]
    EventRemove { event: String },

    // Pads
    #[serde(rename = "pad.add")]
    PadAdd {
        track: String,
        pad: String,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "pad.set")]
    PadSet {
        track: String,
        pad: String,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "pad.remove")]
    PadRemove { track: String, pad: String },

    // Effects
    #[serde(rename = "effect.add")]
    EffectAdd {
        owner: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "effect.remove")]
    EffectRemove { effect: String },
    #[serde(rename = "effect.move")]
    EffectMove { effect: String, index: usize },
    #[serde(rename = "effect.bypass")]
    EffectBypass { effect: String, bypass: bool },

    // Sends
    #[serde(rename = "send.set")]
    SendSet {
        track: String,
        to: String,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "send.remove")]
    SendRemove { track: String, to: String },

    // Automation
    #[serde(rename = "lane.set")]
    LaneSet { owner: String, param: String, points: Json },
    #[serde(rename = "lane.remove")]
    LaneRemove { owner: String, param: String },
    #[serde(rename = "point.add")]
    PointAdd {
        owner: String,
        param: String,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "point.set")]
    PointSet {
        point: String,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "point.remove")]
    PointRemove { point: String },

    // Sections
    #[serde(rename = "section.add")]
    SectionAdd { id: String, at: Json, length_beats: Json },
    #[serde(rename = "section.move")]
    SectionMove { section: String, at: Json },
    #[serde(rename = "section.remove")]
    SectionRemove { section: String },

    // Whole-document edits
    #[serde(rename = "apply")]
    Apply { patch: Json },
    #[serde(rename = "batch")]
    Batch { commands: Vec<Command> },

    // History
    #[serde(rename = "undo")]
    Undo,
    #[serde(rename = "redo")]
    Redo,

    // Transport
    #[serde(rename = "play")]
    Play {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<Json>,
    },
    #[serde(rename = "stop")]
    Stop,
    #[serde(rename = "locate")]
    Locate { at: Json },
    /// Without `start` and `length`, turns the loop off.
    #[serde(rename = "loop")]
    Loop {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<Json>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        length: Option<Json>,
    },

    // Reading and host control
    #[serde(rename = "inspect")]
    Inspect,
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "changes")]
    Changes {
        #[serde(default)]
        since: u64,
    },
    #[serde(rename = "get")]
    Get {
        #[serde(default)]
        path: String,
    },
    /// Writes the song in canonical form.
    #[serde(rename = "fmt")]
    Fmt,
    /// Saves and shuts the host down.
    #[serde(rename = "close")]
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Edit,
    History,
    Transport,
    Read,
    Host,
}

impl Command {
    pub fn kind(&self) -> Kind {
        use Command::*;
        match self {
            Undo | Redo => Kind::History,
            Play { .. } | Stop | Locate { .. } | Loop { .. } => Kind::Transport,
            Inspect | Status | Changes { .. } | Get { .. } => Kind::Read,
            Fmt | Close => Kind::Host,
            _ => Kind::Edit,
        }
    }

    pub fn op(&self) -> String {
        match serde_json::to_value(self) {
            Ok(Json::Object(m)) => m.get("op").and_then(Json::as_str).unwrap_or_default().to_string(),
            _ => String::new(),
        }
    }

    pub fn json(&self) -> Json {
        serde_json::to_value(self).expect("commands serialize")
    }
}

/// A command applied to a tree: what changed, for the log and the reply.
#[derive(Default)]
pub struct Outcome {
    pub label: String,
    /// The object the command created, by handle.
    pub made: Option<u64>,
    /// A `set`'s previous value.
    pub before: Option<Json>,
    /// Changes the command made to keep references valid.
    pub also: Vec<String>,
}

/// Applies edits to a tree. Created objects get handles at once.
pub struct Edit<'a> {
    pub root: &'a mut Node,
    /// Whether `@N` handles may be used in paths.
    pub handles: bool,
    pub next: &'a mut u64,
    also: Vec<String>,
}

type Result<T> = std::result::Result<T, String>;

pub fn json_value(j: &Json) -> Result<Value> {
    aaw_model::json_value(&j.to_string()).map_err(|e| e.to_string())
}

fn to_json(v: &Value) -> Json {
    match v {
        Value::None => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(n) => serde_json::from_str(&n.to_string()).unwrap_or(Json::Null),
        Value::Float(f) => serde_json::Number::from_f64(*f).map_or(Json::Null, Json::Number),
        Value::Str(s) => Json::String(s.clone()),
        Value::List(items) => Json::Array(items.iter().map(to_json).collect()),
        Value::Dict(d) => Json::Object(
            d.iter()
                .map(|(k, v)| (k.as_str().unwrap_or_default().to_string(), to_json(v)))
                .collect(),
        ),
        Value::Bytes(_) | Value::Other(_) => Json::Null,
    }
}

/// A tree value as JSON, in insertion order.
pub fn node_json(n: &Node) -> Json {
    to_json(&n.value())
}

fn short(j: &Json) -> String {
    let s = j.to_string();
    if s.chars().count() > 60 {
        format!("{}…", s.chars().take(59).collect::<String>())
    } else {
        s
    }
}

fn beat_of(v: &Value) -> Option<Beat> {
    match v {
        Value::Int(n) => Some(Beat::Int(n.clone())),
        Value::Float(f) => Some(Beat::Float(*f)),
        Value::Str(s) => Some(Beat::Str(s.clone())),
        _ => None,
    }
}

/// The exact beat a leaf holds, if it holds one.
fn exact(n: Option<&Node>) -> Option<BigRational> {
    match n {
        Some(Node::Leaf(v)) => beat_of(v).and_then(|b| aaw_model::beat(&b).ok()),
        _ => None,
    }
}

/// A beat written as an integer, an exact decimal or a fraction string.
pub fn beat_value(x: &BigRational) -> Value {
    if x.is_integer() {
        return Value::Int(x.to_integer());
    }
    let f = num_traits::ToPrimitive::to_f64(x).unwrap_or(f64::NAN);
    if aaw_model::beat(&Beat::Float(f)).ok().as_ref() == Some(x) {
        Value::Float(f)
    } else {
        Value::Str(aaw_model::fraction_str(x))
    }
}

fn map_node(fields: &Fields) -> Result<Node> {
    Ok(Node::new(&json_value(&Json::Object(fields.clone()))?))
}

/// A lane parameter's effect reference and the rest: `effects.REF.REST`.
fn effect_ref(param: &str) -> Option<(&str, &str)> {
    let rest = param.strip_prefix("effects.")?;
    rest.split_once('.')
}

fn index_ref(r: &str) -> Option<usize> {
    (!r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
        .then(|| r.parse().ok())
        .flatten()
}

impl<'a> Edit<'a> {
    pub fn new(root: &'a mut Node, handles: bool, next: &'a mut u64) -> Self {
        Edit {
            root,
            handles,
            next,
            also: Vec::new(),
        }
    }

    fn fresh(&mut self) -> u64 {
        *self.next += 1;
        *self.next
    }

    fn at(&self, path: &str) -> Result<Loc> {
        tree::resolve(self.root, path, self.handles)
    }

    fn node(&self, loc: &[Step]) -> &Node {
        tree::get(self.root, loc)
    }

    fn node_mut(&mut self, loc: &[Step]) -> &mut Node {
        tree::get_mut(self.root, loc)
    }

    fn text(&self, loc: &[Step]) -> String {
        tree::path_text(self.root, loc)
    }

    /// The location of an item in a list named `list`, e.g. a clip in `clips`.
    fn member(&self, path: &str, list: &str, what: &str) -> Result<(Loc, usize)> {
        let loc = self.at(path)?;
        match loc.as_slice() {
            [owner @ .., Step::Key(k), Step::Index(i)] if k == list => Ok((owner.to_vec(), *i)),
            _ => Err(format!("{path} is not {what}")),
        }
    }

    fn list(&mut self, owner: &[Step], key: &str) -> Result<&mut Vec<Item>> {
        let name = self.text(owner);
        self.node_mut(owner)
            .get_mut(key)
            .and_then(Node::items_mut)
            .ok_or_else(|| format!("{name} has no {key}"))
    }

    fn insert(&mut self, owner: &[Step], key: &str, index: Option<usize>, node: Node) -> Result<u64> {
        let handle = self.fresh();
        let list = self.list(owner, key)?;
        let index = index.unwrap_or(list.len());
        if index > list.len() {
            return Err(format!("{key} index {index} is past the end ({} items)", list.len()));
        }
        list.insert(index, Item { handle, node });
        Ok(handle)
    }

    fn take(&mut self, owner: &[Step], key: &str, index: usize) -> Result<Item> {
        Ok(self.list(owner, key)?.remove(index))
    }

    /// Sets or, for null, removes each field of a map node.
    fn merge(&mut self, loc: &[Step], fields: &Fields) -> Result<()> {
        let mut values = Vec::new();
        for (k, v) in fields {
            values.push((k.clone(), if v.is_null() { None } else { Some(Node::new(&json_value(v)?)) }));
        }
        let name = self.text(loc);
        let map = self
            .node_mut(loc)
            .map_mut()
            .ok_or_else(|| format!("{name} is not an object"))?;
        for (k, v) in values {
            match v {
                Some(n) => {
                    map.insert(k, n);
                }
                None => {
                    map.shift_remove(&k);
                }
            }
        }
        Ok(())
    }

    fn owner(&self, path: &str) -> Result<Loc> {
        let loc = self.at(path)?;
        if self.node(&loc).get("effects").is_none() || self.node(&loc).get("automation").is_none() {
            return Err(format!("{path} is not a track, return or master"));
        }
        Ok(loc)
    }

    fn track(&self, id: &str) -> Result<Loc> {
        self.at(&format!("tracks.{id}"))
    }

    /// Rewrites or removes an owner's lanes on effects after its chain changes:
    /// `index` maps old effect positions to new ones, None where removed.
    fn remap_lanes(&mut self, owner: &[Step], index: impl Fn(usize) -> Option<usize>, removed_id: Option<&str>) -> Result<()> {
        let name = self.text(owner);
        let lanes = self.list(owner, "automation")?;
        let mut notes = Vec::new();
        lanes.retain_mut(|lane| {
            let Some(param) = lane.node.field("param").map(str::to_string) else {
                return true;
            };
            let Some((r, rest)) = effect_ref(&param) else {
                return true;
            };
            let target = match index_ref(r) {
                Some(i) => index(i).map(|j| format!("effects.{j}.{rest}")),
                None if Some(r) == removed_id => None,
                None => return true,
            };
            match target {
                Some(p) if p == param => true,
                Some(p) => {
                    notes.push(format!("{name}: lane {param} is now {p}"));
                    *lane.node.get_mut("param").expect("param") = Node::Leaf(Value::Str(p));
                    true
                }
                None => {
                    notes.push(format!("{name}: removed lane {param}"));
                    false
                }
            }
        });
        self.also.extend(notes);
        Ok(())
    }

    /// Removes lanes whose parameter is `param` exactly.
    fn drop_lanes(&mut self, owner: &[Step], param: &str) -> Result<()> {
        let name = self.text(owner);
        let lanes = self.list(owner, "automation")?;
        let before = lanes.len();
        lanes.retain(|l| l.node.field("param") != Some(param));
        if lanes.len() < before {
            self.also.push(format!("{name}: removed lane {param}"));
        }
        Ok(())
    }

    /// Every track, return and the master, as locations.
    fn owners(&self) -> Vec<Loc> {
        let mut out = Vec::new();
        for key in ["tracks", "returns"] {
            for i in 0..self.root.get(key).map_or(0, |n| n.items().len()) {
                out.push(vec![Step::Key(key.into()), Step::Index(i)]);
            }
        }
        out.push(vec![Step::Key("master".into())]);
        out
    }

    /// Removes the object at `loc`, with what refers to it where that is
    /// unambiguous: an effect's lanes, a send's lane, a return's sends.
    fn remove_at(&mut self, loc: &[Step]) -> Result<String> {
        let what = self.text(loc);
        match loc {
            [owner @ .., Step::Key(k), Step::Index(i)] => {
                let owner = owner.to_vec();
                let item = self.take(&owner, k, *i)?;
                match k.as_str() {
                    "effects" => {
                        let removed = *i;
                        let id = item.node.field("id").map(str::to_string);
                        self.remap_lanes(
                            &owner,
                            |j| match j.cmp(&removed) {
                                std::cmp::Ordering::Less => Some(j),
                                std::cmp::Ordering::Equal => None,
                                std::cmp::Ordering::Greater => Some(j - 1),
                            },
                            id.as_deref(),
                        )?;
                    }
                    "sends" => {
                        if let Some(to) = item.node.field("to") {
                            self.drop_lanes(&owner, &format!("sends.{to}.gain_db"))?;
                        }
                    }
                    "returns" => {
                        if let Some(id) = item.node.field("id").map(str::to_string) {
                            self.drop_sends(&id)?;
                        }
                    }
                    "points" => {
                        // A lane needs a point; the last one takes the lane with it.
                        let lane = owner.clone();
                        if self.node(&lane).get("points").is_some_and(|p| p.items().is_empty()) {
                            if let [lane_owner @ .., Step::Key(_), Step::Index(l)] = lane.as_slice() {
                                let param = self.node(&lane).field("param").unwrap_or_default().to_string();
                                let lane_owner = lane_owner.to_vec();
                                self.take(&lane_owner, "automation", *l)?;
                                self.also.push(format!("{}: removed lane {param}, which had no points left", self.text(&lane_owner)));
                            }
                        }
                    }
                    _ => {}
                }
            }
            [owner @ .., Step::Key(k)] => {
                let owner = owner.to_vec();
                let map = self
                    .node_mut(&owner)
                    .map_mut()
                    .ok_or_else(|| format!("{what} cannot be removed"))?;
                map.shift_remove(k.as_str());
            }
            _ => return Err("The song itself cannot be removed".into()),
        }
        Ok(what)
    }

    fn drop_sends(&mut self, return_id: &str) -> Result<()> {
        for i in 0..self.root.get("tracks").map_or(0, |n| n.items().len()) {
            let track = vec![Step::Key("tracks".into()), Step::Index(i)];
            let sends = self.list(&track, "sends")?;
            if let Some(j) = sends.iter().position(|s| s.node.field("to") == Some(return_id)) {
                let loc = [track.clone(), vec![Step::Key("sends".into()), Step::Index(j)]].concat();
                let what = self.remove_at(&loc)?;
                self.also.push(format!("removed {what}"));
            }
        }
        Ok(())
    }

    fn set_leaf(&mut self, loc: &[Step], key: &str, v: Value) {
        if let Some(m) = self.node_mut(loc).map_mut() {
            m.insert(key.to_string(), Node::Leaf(v));
        }
    }

    fn move_item(&mut self, owner: &[Step], key: &str, from: usize, to: usize) -> Result<()> {
        let list = self.list(owner, key)?;
        if to >= list.len() {
            return Err(format!("{key} index {to} is past the end ({} items)", list.len()));
        }
        let item = list.remove(from);
        list.insert(to, item);
        Ok(())
    }

    /// Where a point at `at` goes to keep a lane in time order: after points at
    /// or before it.
    fn point_slot(points: &[Item], at: Option<&BigRational>) -> usize {
        match at {
            Some(t) => points
                .iter()
                .position(|p| exact(p.node.get("at")).is_some_and(|x| &x > t))
                .unwrap_or(points.len()),
            None => points.len(),
        }
    }

    fn lane(&self, owner: &[Step], param: &str) -> Option<usize> {
        self.node(owner)
            .get("automation")?
            .items()
            .iter()
            .position(|l| l.node.field("param") == Some(param))
    }

    /// Applies one edit command.
    pub fn run(&mut self, cmd: &Command) -> Result<Outcome> {
        use Command::*;
        let mut out = Outcome::default();
        match cmd {
            Set { path, value } => {
                let new = Node::new(&json_value(value)?);
                // A missing key of a map is added, e.g. a new pad; anything else
                // must exist.
                let loc = match self.at(path) {
                    Ok(loc) => loc,
                    Err(e) => {
                        let (parent_path, last) = path.rsplit_once('.').unwrap_or(("", path.as_str()));
                        let parent = self.at(parent_path)?;
                        if !matches!(self.node(&parent), Node::Map(_)) || last.starts_with('@') {
                            return Err(e);
                        }
                        [parent, vec![Step::Key(last.to_string())]].concat()
                    }
                };
                let before = match loc.split_last() {
                    Some((Step::Key(k), parent)) => {
                        let old = self.node(parent).get(k).map(node_json);
                        self.node_mut(parent).map_mut().expect("map").insert(k.clone(), new);
                        old
                    }
                    Some((Step::Index(i), parent)) => {
                        let parent = parent.to_vec();
                        let list = self.node_mut(&parent).items_mut().expect("list");
                        let old = node_json(&list[*i].node);
                        list[*i].node = new;
                        Some(old)
                    }
                    None => return Err("Set a field of the song, not the whole song; use apply".into()),
                };
                let path = self.text(&loc);
                out.label = match &before {
                    Some(b) => format!("Set {path}: {} → {}", short(b), short(value)),
                    None => format!("Set {path}: {}", short(value)),
                };
                out.before = before;
            }
            Toggle { path } => {
                let loc = self.at(path)?;
                let now = match self.node(&loc) {
                    Node::Leaf(Value::Bool(b)) => !b,
                    _ => return Err(format!("{path} is not a boolean")),
                };
                *self.node_mut(&loc) = Node::Leaf(Value::Bool(now));
                out.label = format!("Toggle {} to {now}", self.text(&loc));
            }
            Remove { path } => {
                let loc = self.at(path)?;
                out.label = format!("Remove {}", self.remove_at(&loc)?);
            }
            TrackAdd { id, index, fields } | ReturnAdd { id, index, fields } => {
                let (key, what) = match cmd {
                    TrackAdd { .. } => ("tracks", "track"),
                    _ => ("returns", "return"),
                };
                let mut f = Fields::new();
                f.insert("id".into(), Json::String(id.clone()));
                if key == "tracks" {
                    f.insert("pads".into(), Json::Object(Map::new()));
                }
                f.extend(fields.clone());
                let node = map_node(&f)?;
                out.made = Some(self.insert(&[], key, *index, node)?);
                out.label = format!("Add {what} {id}");
            }
            TrackRemove { track } => {
                let loc = self.track(track)?;
                out.label = format!("Remove {}", self.remove_at(&loc)?);
            }
            ReturnRemove { id } => {
                let loc = self.at(&format!("returns.{id}"))?;
                out.label = format!("Remove {}", self.remove_at(&loc)?);
            }
            TrackRename { track, to } => {
                let loc = self.track(track)?;
                let old = self.node(&loc).field("id").unwrap_or_default().to_string();
                self.set_leaf(&loc, "id", Value::str(to));
                // Sidechains name tracks.
                for owner in self.owners() {
                    for e in self.list(&owner, "effects")?.iter_mut() {
                        if e.node.field("sidechain") == Some(old.as_str()) {
                            *e.node.get_mut("sidechain").expect("sidechain") = Node::Leaf(Value::str(to));
                        }
                    }
                }
                out.label = format!("Rename track {old} to {to}");
            }
            ReturnRename { id, to } => {
                let loc = self.at(&format!("returns.{id}"))?;
                self.set_leaf(&loc, "id", Value::str(to));
                let (old_lane, new_lane) = (format!("sends.{id}.gain_db"), format!("sends.{to}.gain_db"));
                for i in 0..self.root.get("tracks").map_or(0, |n| n.items().len()) {
                    let track = vec![Step::Key("tracks".into()), Step::Index(i)];
                    for s in self.list(&track, "sends")?.iter_mut() {
                        if s.node.field("to") == Some(id.as_str()) {
                            *s.node.get_mut("to").expect("to") = Node::Leaf(Value::str(to));
                        }
                    }
                    for l in self.list(&track, "automation")?.iter_mut() {
                        if l.node.field("param") == Some(old_lane.as_str()) {
                            *l.node.get_mut("param").expect("param") = Node::Leaf(Value::str(&new_lane));
                        }
                    }
                }
                out.label = format!("Rename return {id} to {to}");
            }
            TrackMove { track, index } => {
                let loc = self.track(track)?;
                let Some(Step::Index(from)) = loc.last().cloned() else { unreachable!() };
                self.move_item(&[], "tracks", from, *index)?;
                out.label = format!("Move track {track} to position {index}");
            }
            ClipAdd { track, fields } => {
                let loc = self.track(track)?;
                out.made = Some(self.insert(&loc, "clips", None, map_node(fields)?)?);
                let pattern = fields.get("pattern").and_then(Json::as_str).unwrap_or("?");
                out.label = format!("Add clip of {pattern} to {track}");
            }
            ClipMove { clip, track, at } => {
                let (owner, i) = self.member(clip, "clips", "a clip")?;
                let mut clip_loc = [owner.clone(), vec![Step::Key("clips".into()), Step::Index(i)]].concat();
                if let Some(at) = at {
                    self.set_leaf(&clip_loc, "at", json_value(at)?);
                }
                if let Some(t) = track {
                    let dest = self.track(t)?;
                    if dest != owner {
                        let item = self.take(&owner, "clips", i)?;
                        let list = self.list(&dest, "clips")?;
                        list.push(item);
                        clip_loc = [dest, vec![Step::Key("clips".into()), Step::Index(list.len() - 1)]].concat();
                    }
                }
                out.label = format!("Move clip {}", self.clip_name(&clip_loc));
            }
            ClipRepeats { clip, repeats } => {
                let (owner, i) = self.member(clip, "clips", "a clip")?;
                let loc = [owner, vec![Step::Key("clips".into()), Step::Index(i)]].concat();
                self.set_leaf(&loc, "repeats", json_value(repeats)?);
                out.label = format!("Set repeats of clip {} to {}", self.clip_name(&loc), short(repeats));
            }
            ClipDuplicate { clip, track, at } => {
                let (owner, i) = self.member(clip, "clips", "a clip")?;
                let source = self.node(&owner).get("clips").expect("clips").items()[i].node.clone();
                let at = match at {
                    Some(a) => json_value(a)?,
                    None => {
                        // Right after the original: its start plus its span.
                        let pattern = source.field("pattern").unwrap_or_default();
                        let length = exact(self.root.get("patterns").and_then(|p| p.get(pattern)).and_then(|p| p.get("length_beats")));
                        let start = exact(source.get("at"));
                        let repeats = match source.get("repeats") {
                            Some(Node::Leaf(Value::Int(n))) => Some(BigRational::from_integer(n.clone())),
                            _ => None,
                        };
                        match (start, length, repeats) {
                            (Some(s), Some(l), Some(r)) => beat_value(&(s + l * r)),
                            _ => return Err(format!("{clip}: cannot place the copy; pass at")),
                        }
                    }
                };
                let dest = match track {
                    Some(t) => self.track(t)?,
                    None => owner.clone(),
                };
                let mut copy = source;
                if let Some(m) = copy.map_mut() {
                    m.insert("at".into(), Node::Leaf(at));
                }
                let index = (dest == owner).then_some(i + 1);
                out.made = Some(self.insert(&dest, "clips", index, copy)?);
                out.label = format!("Duplicate clip {clip}");
            }
            ClipRemove { clip } => {
                let (owner, i) = self.member(clip, "clips", "a clip")?;
                let loc = [owner, vec![Step::Key("clips".into()), Step::Index(i)]].concat();
                let name = self.clip_name(&loc);
                self.remove_at(&loc)?;
                out.label = format!("Remove clip {name}");
            }
            PatternAdd { pattern, fields } => {
                let patterns = self.root.get_mut("patterns").and_then(Node::map_mut).expect("patterns");
                if patterns.contains_key(pattern) {
                    return Err(format!("Pattern {pattern} already exists"));
                }
                let node = map_node(fields)?;
                self.root.get_mut("patterns").and_then(Node::map_mut).expect("patterns").insert(pattern.clone(), node);
                out.label = format!("Add pattern {pattern}");
            }
            PatternSteps { pattern, pad, row } => {
                let loc = self.at(&format!("patterns.{pattern}.steps"))?;
                let steps = self.node_mut(&loc).map_mut().expect("steps");
                match row {
                    Some(r) => {
                        steps.insert(pad.clone(), Node::Leaf(Value::str(r)));
                        out.label = format!("Set {pattern} steps for {pad}");
                    }
                    None => {
                        if steps.shift_remove(pad).is_none() {
                            return Err(format!("Pattern {pattern} has no step row for {pad}"));
                        }
                        out.label = format!("Remove {pattern} steps for {pad}");
                    }
                }
            }
            PatternDuplicate { pattern, to } => {
                let source = self.node(&self.at(&format!("patterns.{pattern}"))?).clone();
                let patterns = self.root.get_mut("patterns").and_then(Node::map_mut).expect("patterns");
                if patterns.contains_key(to) {
                    return Err(format!("Pattern {to} already exists"));
                }
                // The copy's events are new objects with new handles.
                patterns.insert(to.clone(), Node::new(&source.value()));
                out.label = format!("Duplicate pattern {pattern} as {to}");
            }
            EventAdd { pattern, fields } => {
                let loc = self.at(&format!("patterns.{pattern}"))?;
                out.made = Some(self.insert(&loc, "events", None, map_node(fields)?)?);
                out.label = format!("Add event to {pattern}");
            }
            EventSet { event, fields } => {
                let (owner, i) = self.member(event, "events", "a pattern event")?;
                let loc = [owner, vec![Step::Key("events".into()), Step::Index(i)]].concat();
                self.merge(&loc, fields)?;
                out.label = format!("Change event {}", self.text(&loc));
            }
            EventRemove { event } => {
                let (owner, i) = self.member(event, "events", "a pattern event")?;
                let loc = [owner, vec![Step::Key("events".into()), Step::Index(i)]].concat();
                out.label = format!("Remove event {}", self.remove_at(&loc)?);
            }
            PadAdd { track, pad, fields } => {
                let loc = [self.track(track)?, vec![Step::Key("pads".into())]].concat();
                let node = map_node(fields)?;
                let pads = self.node_mut(&loc).map_mut().expect("pads");
                if pads.contains_key(pad) {
                    return Err(format!("{track} already has pad {pad}"));
                }
                pads.insert(pad.clone(), node);
                out.label = format!("Add pad {pad} to {track}");
            }
            PadSet { track, pad, fields } => {
                let loc = self.at(&format!("tracks.{track}.pads.{pad}"))?;
                self.merge(&loc, fields)?;
                out.label = format!("Change pad {track}.{pad}");
            }
            PadRemove { track, pad } => {
                let loc = self.at(&format!("tracks.{track}.pads.{pad}"))?;
                self.remove_at(&loc)?;
                out.label = format!("Remove pad {track}.{pad}");
            }
            EffectAdd { owner, index, fields } => {
                let loc = self.owner(owner)?;
                let at = index.unwrap_or(self.node(&loc).get("effects").map_or(0, |e| e.items().len()));
                out.made = Some(self.insert(&loc, "effects", Some(at), map_node(fields)?)?);
                self.remap_lanes(&loc, |j| Some(if j >= at { j + 1 } else { j }), None)?;
                let kind = fields.get("type").map_or_else(|| "effect".to_string(), |t| t.as_str().unwrap_or("effect").to_string());
                out.label = format!("Add {kind} to {}", self.text(&loc));
            }
            EffectRemove { effect } => {
                let (owner, i) = self.member(effect, "effects", "an effect")?;
                let loc = [owner, vec![Step::Key("effects".into()), Step::Index(i)]].concat();
                out.label = format!("Remove effect {}", self.remove_at(&loc)?);
            }
            EffectMove { effect, index } => {
                let (owner, from) = self.member(effect, "effects", "an effect")?;
                let to = *index;
                self.move_item(&owner, "effects", from, to)?;
                self.remap_lanes(
                    &owner,
                    |j| {
                        Some(if j == from {
                            to
                        } else if from < to && j > from && j <= to {
                            j - 1
                        } else if to < from && j >= to && j < from {
                            j + 1
                        } else {
                            j
                        })
                    },
                    None,
                )?;
                let loc = [owner, vec![Step::Key("effects".into()), Step::Index(to)]].concat();
                out.label = format!("Move effect {} to position {to}", self.text(&loc));
            }
            EffectBypass { effect, bypass } => {
                let (owner, i) = self.member(effect, "effects", "an effect")?;
                let loc = [owner, vec![Step::Key("effects".into()), Step::Index(i)]].concat();
                self.set_leaf(&loc, "bypass", Value::Bool(*bypass));
                out.label = format!("{} effect {}", if *bypass { "Bypass" } else { "Enable" }, self.text(&loc));
            }
            SendSet { track, to, fields } => {
                let loc = self.track(track)?;
                let existing = self.list(&loc, "sends")?.iter().position(|s| s.node.field("to") == Some(to.as_str()));
                match existing {
                    Some(i) => {
                        let send = [loc, vec![Step::Key("sends".into()), Step::Index(i)]].concat();
                        self.merge(&send, fields)?;
                    }
                    None => {
                        let mut f = Fields::new();
                        f.insert("to".into(), Json::String(to.clone()));
                        f.extend(fields.clone());
                        out.made = Some(self.insert(&loc, "sends", None, map_node(&f)?)?);
                    }
                }
                out.label = format!("Set send {track} → {to}");
            }
            SendRemove { track, to } => {
                let loc = self.at(&format!("tracks.{track}.sends.{to}"))?;
                self.remove_at(&loc)?;
                out.label = format!("Remove send {track} → {to}");
            }
            LaneSet { owner, param, points } => {
                let loc = self.owner(owner)?;
                let points = Node::new(&json_value(points)?);
                match self.lane(&loc, param) {
                    Some(i) => {
                        let lane = [loc.clone(), vec![Step::Key("automation".into()), Step::Index(i)]].concat();
                        self.node_mut(&lane).map_mut().expect("lane").insert("points".into(), points);
                    }
                    None => {
                        let mut lane = indexmap::IndexMap::new();
                        lane.insert("param".to_string(), Node::Leaf(Value::str(param)));
                        lane.insert("points".to_string(), points);
                        out.made = Some(self.insert(&loc, "automation", None, Node::Map(lane))?);
                    }
                }
                out.label = format!("Set lane {} {param}", self.text(&loc));
            }
            LaneRemove { owner, param } => {
                let loc = self.owner(owner)?;
                let i = self.lane(&loc, param).ok_or_else(|| format!("{owner} has no lane {param}"))?;
                self.take(&loc, "automation", i)?;
                out.label = format!("Remove lane {} {param}", self.text(&loc));
            }
            PointAdd { owner, param, fields } => {
                let loc = self.owner(owner)?;
                let point = map_node(fields)?;
                let at = exact(point.get("at"));
                match self.lane(&loc, param) {
                    Some(i) => {
                        let lane = [loc.clone(), vec![Step::Key("automation".into()), Step::Index(i)]].concat();
                        let slot = Self::point_slot(self.node(&lane).get("points").map_or(&[], Node::items), at.as_ref());
                        out.made = Some(self.insert(&lane, "points", Some(slot), point)?);
                    }
                    None => {
                        let mut lane = indexmap::IndexMap::new();
                        lane.insert("param".to_string(), Node::Leaf(Value::str(param)));
                        lane.insert("points".to_string(), Node::List(Vec::new()));
                        self.insert(&loc, "automation", None, Node::Map(lane))?;
                        let i = self.lane(&loc, param).expect("lane");
                        let lane = [loc.clone(), vec![Step::Key("automation".into()), Step::Index(i)]].concat();
                        out.made = Some(self.insert(&lane, "points", None, point)?);
                    }
                }
                out.label = format!("Add point to {} {param}", self.text(&loc));
            }
            PointSet { point, fields } => {
                let (lane, i) = self.member(point, "points", "an automation point")?;
                let loc = [lane.clone(), vec![Step::Key("points".into()), Step::Index(i)]].concat();
                self.merge(&loc, fields)?;
                if fields.contains_key("at") {
                    // Keep the lane in time order.
                    let item = self.take(&lane, "points", i)?;
                    let at = exact(item.node.get("at"));
                    let slot = Self::point_slot(self.node(&lane).get("points").map_or(&[], Node::items), at.as_ref());
                    self.list(&lane, "points")?.insert(slot, item);
                }
                out.label = format!("Change point {point}");
            }
            PointRemove { point } => {
                let (lane, i) = self.member(point, "points", "an automation point")?;
                let loc = [lane, vec![Step::Key("points".into()), Step::Index(i)]].concat();
                out.label = format!("Remove point {}", self.remove_at(&loc)?);
            }
            SectionAdd { id, at, length_beats } => {
                let mut f = Fields::new();
                f.insert("id".into(), Json::String(id.clone()));
                f.insert("at".into(), at.clone());
                f.insert("length_beats".into(), length_beats.clone());
                out.made = Some(self.insert(&[], "sections", None, map_node(&f)?)?);
                out.label = format!("Add section {id}");
            }
            SectionMove { section, at } => {
                let loc = self.at(&format!("sections.{section}"))?;
                self.set_leaf(&loc, "at", json_value(at)?);
                out.label = format!("Move section {section} to {}", short(at));
            }
            SectionRemove { section } => {
                let loc = self.at(&format!("sections.{section}"))?;
                out.label = format!("Remove {}", self.remove_at(&loc)?);
            }
            Batch { commands } => {
                let mut labels = Vec::new();
                let mut also = Vec::new();
                for (n, c) in commands.iter().enumerate() {
                    if c.kind() != Kind::Edit || matches!(c, Apply { .. }) {
                        return Err(format!("batch command {n}: {} cannot be batched", c.op()));
                    }
                    let o = self.run(c).map_err(|e| format!("batch command {n} ({}): {e}", c.op()))?;
                    labels.push(o.label);
                    also.extend(o.also);
                }
                self.also = also;
                out.label = match labels.len() {
                    1 => labels.remove(0),
                    n => format!("Batch of {n}: {}", labels.join("; ")),
                };
            }
            Apply { .. } => return Err("apply replaces the whole song; the session applies it".into()),
            other => return Err(format!("{} is not an edit", other.op())),
        }
        out.also = std::mem::take(&mut self.also);
        Ok(out)
    }

    /// A clip described by its pattern and track, for labels.
    fn clip_name(&self, loc: &[Step]) -> String {
        let n = self.node(loc);
        format!(
            "{} at {}",
            n.field("pattern").unwrap_or("?"),
            n.get("at").map_or_else(|| "?".into(), |a| short(&node_json(a)))
        )
    }
}
