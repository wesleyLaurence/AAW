//! A session: the song held in memory as the authority, with its undo history,
//! the change log, saving and reloading of external edits.
//!
//! A host keeps one session for as long as it runs. Headless commands open one,
//! apply a single command, save and exit; they have no history or handles.

use crate::command::{json_value, node_json, Command, Edit, Kind, Origin, Outcome};
use crate::tree::{self, Node};
use aaw_model::{project_hash, Project};
use serde::Serialize;
use serde_json::{json, Map, Value as Json};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Undo steps kept; older ones are dropped.
pub const UNDO_LIMIT: usize = 1000;
/// Change log entries kept for `changes --since`.
pub const LOG_LIMIT: usize = 10000;

type Result<T> = std::result::Result<T, String>;

/// One revision of the song.
#[derive(Clone)]
pub struct Doc {
    pub project: Arc<Project>,
    /// Handles of list items in depth-first order of the full dump.
    pub handles: Arc<Vec<u64>>,
    pub sha: String,
    /// The canonical file. Unlike the SHA, it depends on the order of keys, such
    /// as pads, so it decides whether an edit changed anything.
    pub yaml: Arc<String>,
}

impl Doc {
    fn new(project: Project, handles: Vec<u64>) -> Doc {
        Doc {
            sha: project_hash(&project),
            yaml: Arc::new(aaw_model::to_yaml(&project)),
            project: Arc::new(project),
            handles: Arc::new(handles),
        }
    }

    /// The full dump with handles.
    pub fn tree(&self) -> Node {
        tree::attach(&self.project.dump(false), &self.handles)
    }
}

struct Step {
    before: Doc,
    after: Doc,
    label: String,
    origin: Origin,
}

#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub revision: u64,
    pub origin: Origin,
    pub op: String,
    pub label: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
    pub command: Json,
    pub project_sha256: String,
    /// Unix time in seconds.
    pub time: f64,
}

/// A file's identity on disk, to notice changes without reading it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
    inode: u64,
}

impl Stamp {
    fn of(path: &Path) -> Option<Stamp> {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(path).ok()?;
        Some(Stamp {
            len: m.len(),
            modified: m.modified().ok(),
            inode: m.ino(),
        })
    }
}

pub struct Session {
    path: PathBuf,
    dir: PathBuf,
    hosted: bool,
    id: String,
    doc: Doc,
    revision: u64,
    saved_revision: u64,
    undo: Vec<Step>,
    redo: Vec<Step>,
    log: VecDeque<Change>,
    next: u64,
    /// The file as last written or read, and its stamp then.
    disk_bytes: Vec<u8>,
    disk_stamp: Option<Stamp>,
    dirty: bool,
    /// Why the file on disk is not loadable, after an invalid external edit.
    invalid: Option<String>,
    digests: HashMap<PathBuf, (Stamp, String)>,
}

fn now() -> f64 {
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    (t.as_secs_f64() * 1000.0).round() / 1000.0
}

fn parse(path: &Path, bytes: &[u8]) -> Result<Project> {
    let text = std::str::from_utf8(bytes).map_err(|_| format!("{}: not UTF-8 text", path.display()))?;
    aaw_model::parse(text).map_err(|e| e.to_string())
}

fn read_project(path: &Path) -> Result<(Project, Vec<u8>)> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok((parse(path, &bytes)?, bytes))
}

impl Session {
    /// Loads a song and verifies its samples, as `model.load` does.
    pub fn open(path: &Path, hosted: bool) -> Result<Session> {
        let dir = match path.parent() {
            Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let stamp = Stamp::of(path);
        let (project, bytes) = read_project(path)?;
        aaw_model::verify_assets(&project, &dir).map_err(|e| e.to_string())?;
        let mut next = 0;
        let handles = tree::handles(&tree::carry(&project.dump(false), None, &mut next));
        let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        Ok(Session {
            path: path.to_path_buf(),
            dir,
            hosted,
            id: format!("{:x}", (t.as_nanos() as u64) ^ ((std::process::id() as u64) << 40)),
            doc: Doc::new(project, handles),
            revision: 0,
            saved_revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            log: VecDeque::new(),
            next,
            disk_bytes: bytes,
            disk_stamp: stamp,
            dirty: false,
            invalid: None,
            digests: HashMap::new(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    pub fn project(&self) -> &Arc<Project> {
        &self.doc.project
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn invalid(&self) -> Option<&str> {
        self.invalid.as_deref()
    }

    fn check_editable(&self) -> Result<()> {
        match &self.invalid {
            Some(e) => Err(format!(
                "{} was edited outside the host and does not load, so edits wait until it is fixed: {e}",
                self.path.display()
            )),
            None => Ok(()),
        }
    }

    /// The SHA-256 of a sample file, cached while the file is unchanged.
    fn digest(&mut self, path: &Path) -> Result<String> {
        let stamp = Stamp::of(path).ok_or_else(|| format!("{}: missing", path.display()))?;
        if let Some((s, d)) = self.digests.get(path) {
            if *s == stamp {
                return Ok(d.clone());
            }
        }
        let d = aaw_model::digest(path).map_err(|e| e.to_string())?;
        self.digests.insert(path.to_path_buf(), (stamp, d.clone()));
        Ok(d)
    }

    /// Checks the samples an edit adds or changes, as `daw apply` checks them.
    fn verify_changed(&mut self, new: &Project) -> Result<()> {
        let old = self.doc.project.clone();
        for (name, asset) in &new.samples {
            if old.samples.get(name).is_some_and(|o| o.path == asset.path && o.sha256 == asset.sha256) {
                continue;
            }
            let path = self.dir.join(&asset.path);
            if !path.is_file() {
                return Err(format!("Missing asset {name}"));
            }
            if let Some(sha) = &asset.sha256 {
                if &self.digest(&path)? != sha {
                    return Err(format!("Changed asset {name}"));
                }
            }
        }
        Ok(())
    }

    fn record(&mut self, origin: Origin, op: &str, label: String, also: Vec<String>, command: Json) -> Change {
        self.revision += 1;
        let change = Change {
            revision: self.revision,
            origin,
            op: op.to_string(),
            label,
            also,
            command,
            project_sha256: self.doc.sha.clone(),
            time: now(),
        };
        self.log.push_back(change.clone());
        if self.log.len() > LOG_LIMIT {
            self.log.pop_front();
        }
        change
    }

    /// Makes `project` current as one undoable step. None if nothing changed.
    fn commit(&mut self, project: Project, handles: Vec<u64>, outcome: Outcome, origin: Origin, command: Json) -> Option<Change> {
        let doc = Doc::new(project, handles);
        if doc.yaml == self.doc.yaml {
            return None;
        }
        let before = std::mem::replace(&mut self.doc, doc);
        self.undo.push(Step {
            before,
            after: self.doc.clone(),
            label: outcome.label.clone(),
            origin,
        });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        let op = command.get("op").and_then(Json::as_str).unwrap_or("edit").to_string();
        Some(self.record(origin, &op, outcome.label, outcome.also, command))
    }

    /// Applies an edit command. `expect` refuses it unless the song's SHA matches.
    pub fn edit(&mut self, cmd: &Command, origin: Origin, expect: Option<&str>) -> Result<(Json, Option<Change>)> {
        debug_assert_eq!(cmd.kind(), Kind::Edit);
        self.check_editable()?;
        if expect.is_some_and(|x| x != self.doc.sha) {
            return Err("Stale project revision; inspect and retry".into());
        }
        let (project, node, outcome) = match cmd {
            Command::Apply { patch } => {
                let merged = aaw_model::merge(&self.doc.project.dump(false), &json_value(patch)?);
                let project = Project::validate(&merged).map_err(|e| e.to_string())?;
                let node = tree::matched(&project.dump(false), Some(&self.doc.tree()), &mut self.next);
                let outcome = Outcome {
                    label: "Apply a merge patch".into(),
                    ..Outcome::default()
                };
                (project, node, outcome)
            }
            _ => {
                let mut root = self.doc.tree();
                let mut next = self.next;
                let outcome = Edit::new(&mut root, self.hosted, &mut next).run(cmd)?;
                let project = Project::validate(&root.value()).map_err(|e| e.to_string())?;
                let node = tree::carry(&project.dump(false), Some(&root), &mut next);
                self.next = next;
                (project, node, outcome)
            }
        };
        self.verify_changed(&project)?;
        let made = outcome.made.and_then(|h| tree::find(&node, h).map(|loc| (h, tree::path_text(&node, &loc))));
        let before = outcome.before.clone();
        let label = outcome.label.clone();
        let also = outcome.also.clone();
        let change = self.commit(project, tree::handles(&node), outcome, origin, cmd.json());
        let mut reply = self.stamp_reply(change.is_some());
        reply.insert("label".into(), json!(label));
        if let Some((h, path)) = made {
            if self.hosted {
                reply.insert("handle".into(), json!(tree::handle_text(h)));
            }
            reply.insert("path".into(), json!(path));
        }
        if let Some(b) = before {
            reply.insert("before".into(), b);
        }
        if !also.is_empty() {
            reply.insert("also".into(), json!(also));
        }
        Ok((Json::Object(reply), change))
    }

    fn stamp_reply(&self, changed: bool) -> Map<String, Json> {
        let mut m = Map::new();
        if self.hosted {
            m.insert("revision".into(), json!(self.revision));
        }
        m.insert("project_sha256".into(), json!(self.doc.sha));
        m.insert("changed".into(), json!(changed));
        m
    }

    /// Steps back through history. Undo and redo are themselves logged.
    pub fn undo(&mut self, origin: Origin, redo: bool) -> Result<(Json, Change)> {
        self.check_editable()?;
        let (from, verb) = if redo { (&mut self.redo, "Redo") } else { (&mut self.undo, "Undo") };
        let step = from.pop().ok_or_else(|| format!("Nothing to {}", verb.to_lowercase()))?;
        self.doc = if redo { step.after.clone() } else { step.before.clone() };
        let label = format!("{verb} {} ({})", step.label, step.origin.as_str());
        if redo {
            self.undo.push(step);
        } else {
            self.redo.push(step);
        }
        self.dirty = true;
        let op = verb.to_lowercase();
        let change = self.record(origin, &op, label.clone(), Vec::new(), json!({"op": op}));
        let mut reply = self.stamp_reply(true);
        reply.insert("label".into(), json!(label));
        Ok((Json::Object(reply), change))
    }

    /// Writes the song if it changed since the last write. Nothing is written
    /// while the file holds an invalid external edit.
    pub fn save(&mut self) -> Result<bool> {
        if !self.dirty || self.invalid.is_some() {
            return Ok(false);
        }
        let text = self.doc.yaml.clone();
        let wrote = text.as_bytes() != self.disk_bytes.as_slice() || Stamp::of(&self.path).is_none();
        if wrote {
            aaw_model::atomic_write(&self.path, &text).map_err(|e| e.to_string())?;
        }
        self.disk_bytes = text.as_bytes().to_vec();
        self.disk_stamp = Stamp::of(&self.path);
        self.dirty = false;
        self.saved_revision = self.revision;
        Ok(wrote)
    }

    /// Writes the song in canonical form, replacing whatever the file holds,
    /// including an invalid external edit.
    pub fn write(&mut self) -> Result<()> {
        self.invalid = None;
        self.dirty = true;
        self.disk_bytes.clear();
        self.save().map(|_| ())
    }

    /// Notices an external change to the file. A valid one is loaded as one
    /// undoable `external` step; an invalid one pauses edits until it is fixed.
    pub fn sync(&mut self) -> Option<Change> {
        let stamp = Stamp::of(&self.path);
        if stamp == self.disk_stamp {
            return None;
        }
        self.disk_stamp = stamp;
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) => {
                self.invalid = Some(format!("{}: {e}", self.path.display()));
                self.disk_bytes.clear();
                return None;
            }
        };
        if bytes == self.disk_bytes {
            return None;
        }
        let loaded = parse(&self.path, &bytes).and_then(|p| {
            self.verify_changed(&p)?;
            Ok(p)
        });
        self.disk_bytes = bytes;
        match loaded {
            Err(e) => {
                self.invalid = Some(e);
                None
            }
            Ok(project) => {
                self.invalid = None;
                let node = tree::matched(&project.dump(false), Some(&self.doc.tree()), &mut self.next);
                let outcome = Outcome {
                    label: "External edit of song.yaml".into(),
                    ..Outcome::default()
                };
                let dirty = self.dirty;
                let change = self.commit(project, tree::handles(&node), outcome, Origin::External, json!({"op": "reload"}));
                // The file already holds this revision.
                self.dirty = dirty && change.is_none();
                change
            }
        }
    }

    pub fn changes(&self, since: u64) -> Json {
        let changes: Vec<&Change> = self.log.iter().filter(|c| c.revision > since).collect();
        json!({"revision": self.revision, "session": self.id, "changes": changes})
    }

    fn step_json(step: Option<&Step>) -> Json {
        step.map_or(Json::Null, |s| json!({"label": s.label, "origin": s.origin}))
    }

    /// The song's side of `daw status`; the host adds the transport.
    pub fn status(&self) -> Map<String, Json> {
        let mut m = Map::new();
        m.insert("project".into(), json!(self.path));
        m.insert("host".into(), json!(self.hosted));
        if self.hosted {
            m.insert("session".into(), json!(self.id));
            m.insert("revision".into(), json!(self.revision));
            m.insert("saved_revision".into(), json!(self.saved_revision));
        }
        m.insert("project_sha256".into(), json!(self.doc.sha));
        if self.hosted {
            m.insert("undo".into(), Self::step_json(self.undo.last()));
            m.insert("redo".into(), Self::step_json(self.redo.last()));
            m.insert("invalid_external_edit".into(), json!(self.invalid));
        }
        m
    }

    /// How commands refer to the list item at `loc`: its handle while hosted,
    /// else its path.
    fn reference(&self, root: &Node, loc: &[tree::Step]) -> String {
        if self.hosted {
            tree::handle_text(tree::handle_at(root, loc))
        } else {
            tree::path_text(root, loc)
        }
    }

    /// `daw inspect`: the Python CLI's summary, plus each clip's reference and,
    /// while hosted, the revision.
    pub fn inspect(&self) -> Json {
        let p = &self.doc.project;
        let root = self.doc.tree();
        let triggers = aaw_engine::schedule::schedule(p);
        let rate = p.session.sample_rate;
        let frames = aaw_model::frame(&p.session.length_exact(), p.session.tempo, rate);
        fn types(effects: &[aaw_model::Effect]) -> Vec<&str> {
            effects.iter().map(|e| e.kind()).collect()
        }
        fn params(lanes: &[aaw_model::Lane]) -> Vec<&str> {
            lanes.iter().map(|l| l.param.as_str()).collect()
        }
        let tracks: Vec<Json> = p
            .tracks
            .iter()
            .enumerate()
            .map(|(ti, t)| {
                let clips: Vec<Json> = t
                    .clips
                    .iter()
                    .enumerate()
                    .map(|(ci, c)| {
                        let loc = [
                            tree::Step::Key("tracks".into()),
                            tree::Step::Index(ti),
                            tree::Step::Key("clips".into()),
                            tree::Step::Index(ci),
                        ];
                        json!({
                            "ref": self.reference(&root, &loc),
                            "pattern": c.pattern,
                            "at": crate::command::node_json(&Node::Leaf(c.at.to_value())),
                            "repeats": c.repeats,
                        })
                    })
                    .collect();
                json!({
                    "id": t.id,
                    "events": triggers.iter().filter(|x| x.track_id == t.id).count(),
                    "gain_db": t.gain_db,
                    "mute": t.mute,
                    "solo": t.solo,
                    "effects": types(&t.effects),
                    "sidechain": t.sidechains(),
                    "sends": t.sends.iter().map(|s| value_json(&s.dump(false))).collect::<Vec<_>>(),
                    "automation": params(&t.automation),
                    "clips": clips,
                })
            })
            .collect();
        let returns: Vec<Json> = p
            .returns
            .iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "gain_db": r.gain_db,
                    "mute": r.mute,
                    "effects": types(&r.effects),
                    "sidechain": r.sidechains(),
                    "senders": p.senders(&r.id),
                    "automation": params(&r.automation),
                })
            })
            .collect();
        let mut out = json!({
            "valid": true,
            "project_sha256": self.doc.sha,
            "session": value_json(&p.session.dump(false)),
            "duration_seconds": frames as f64 / rate as f64,
            "samples": p.samples.len(),
            "patterns": p.patterns.len(),
            "tracks": tracks,
            "returns": returns,
            "master_effects": types(&p.master.effects),
            "master_automation": params(&p.master.automation),
            "sections": p.sections.iter().map(|s| value_json(&s.dump(false))).collect::<Vec<_>>(),
        });
        if self.hosted {
            out["host"] = json!({"session": self.id, "revision": self.revision});
        }
        out
    }

    /// `daw get PATH`: part of the song in its saved form, each list item led by
    /// the reference commands use for it.
    pub fn get(&self, path: &str) -> Result<Json> {
        let full = self.doc.tree();
        let loc = tree::resolve(&full, path, self.hosted)?;
        let saved = tree::attach(&self.doc.project.dump(true), &self.doc.handles);
        let exists = {
            let mut n = Some(&saved);
            for s in &loc {
                n = n.and_then(|x| match (x, s) {
                    (Node::Map(m), tree::Step::Key(k)) => m.get(k.as_str()),
                    (Node::List(items), tree::Step::Index(i)) => items.get(*i).map(|i| &i.node),
                    _ => None,
                });
            }
            n
        };
        let node = exists.unwrap_or_else(|| tree::get(&full, &loc));
        Ok(self.view(node, &full, &loc))
    }

    fn view(&self, node: &Node, full: &Node, loc: &[tree::Step]) -> Json {
        match node {
            Node::Leaf(_) => node_json(node),
            Node::Map(m) => Json::Object(
                m.iter()
                    .map(|(k, n)| {
                        let child = [loc.to_vec(), vec![tree::Step::Key(k.clone())]].concat();
                        (k.clone(), self.view(n, full, &child))
                    })
                    .collect(),
            ),
            Node::List(items) => Json::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let child = [loc.to_vec(), vec![tree::Step::Index(i)]].concat();
                        match self.view(&item.node, full, &child) {
                            Json::Object(m) => {
                                let mut out = Map::new();
                                out.insert("ref".into(), json!(self.reference(full, &child)));
                                out.extend(m);
                                Json::Object(out)
                            }
                            other => other,
                        }
                    })
                    .collect(),
            ),
        }
    }
}

/// A model value as JSON.
pub fn value_json(v: &aaw_model::value::Value) -> Json {
    node_json(&Node::Leaf(v.clone()))
}
