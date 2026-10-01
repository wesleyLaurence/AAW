//! What the app draws: the arrangement of a revision, and which objects a
//! change touched. Positions are beats as floats, for display only; the song's
//! exact values stay in the host.

use aaw_host::session::Doc;
use aaw_host::tree::{Item, Node};
use aaw_model::describe::{self, Initial, Kind};
use aaw_model::rules::{target, Domain, Owner, TargetKind};
use aaw_model::value::{py_eq, Value};
use aaw_model::{Curve, Effect, Lane, Project};
use num_traits::ToPrimitive;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Arrangement {
    pub revision: u64,
    pub project_sha256: String,
    pub title: String,
    pub tempo: f64,
    pub beats_per_bar: u32,
    pub length_beats: f64,
    pub tracks: Vec<TrackView>,
    pub returns: Vec<ReturnView>,
    pub master: MasterView,
    pub sections: Vec<SectionView>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FieldKind {
    /// A number from `min` to `max`.
    Number,
    /// A whole number from `min` to `max`, or one of `choices`.
    Integer,
    /// One of `choices`.
    Choice,
    Flag,
    /// A length in beats, written as a number or a fraction such as `3/4`.
    Beats,
    /// One of `choices`, the tracks that can key the effect, or none.
    Track,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum FieldValue {
    Number { value: f64 },
    Text { value: String },
    Flag { value: bool },
    /// Left out: an optional field that is off.
    Absent,
}

/// One field of an effect, with what a control for it needs: a device panel
/// is drawn from these without knowing the effects one by one.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FieldView {
    /// The field within its effect, such as `cutoff_hz` or `bands.0.gain_db`.
    pub name: String,
    pub label: String,
    pub kind: FieldKind,
    pub value: FieldValue,
    pub min: f64,
    pub max: f64,
    pub unit: String,
    /// Whether the value is best moved in equal ratios, as a frequency is.
    pub log: bool,
    pub choices: Vec<String>,
    /// Whether the field may be `Absent`.
    pub optional: bool,
    /// The value a reset gives: the default, or a place to start.
    pub initial: FieldValue,
    /// Whether a playing song glides to a new value. Other fields give the
    /// device new state, which the song fades through, so a control for one
    /// sends its value when the drag ends.
    pub live: bool,
    /// The parameter a lane for this field has; None where automation cannot
    /// move the field.
    pub param: Option<String>,
    /// The key of the lane that moves the field.
    pub lane: Option<u64>,
    /// The equalizer band the field belongs to.
    pub band: Option<u32>,
}

/// An effect in a chain.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EffectView {
    /// The effect's handle.
    pub key: u64,
    pub kind: String,
    pub id: Option<String>,
    pub bypass: bool,
    pub fields: Vec<FieldView>,
    /// An equalizer's bands.
    pub bands: u32,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PointView {
    pub key: u64,
    pub at: f64,
    pub value: f64,
    /// Whether the value holds until the next point instead of moving to it.
    pub hold: bool,
}

/// An automation lane.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct LaneView {
    pub key: u64,
    pub param: String,
    pub label: String,
    pub unit: String,
    /// The range to draw and drag in. Levels show -60 to +6 dB as the faders
    /// do; the song allows more, and points outside are drawn at the edge.
    pub min: f64,
    pub max: f64,
    pub log: bool,
    pub points: Vec<PointView>,
}

/// A parameter that could have a lane and has none.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct LaneTarget {
    pub param: String,
    pub label: String,
}

/// A pad of a track's sampler.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PadView {
    pub name: String,
    pub sample: String,
    pub gain_db: f64,
    /// Whether events hold the sample for their duration.
    pub gate: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TrackView {
    /// The track's handle: the same for as long as the host runs, through
    /// renames and moves.
    pub key: u64,
    pub id: String,
    pub gain_db: f64,
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
    pub effects: Vec<EffectView>,
    pub sends: Vec<SendView>,
    pub lanes: Vec<LaneView>,
    pub lane_targets: Vec<LaneTarget>,
    pub pads: Vec<PadView>,
    pub clips: Vec<ClipView>,
}

/// A send from a track to a return.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SendView {
    /// The return's ID.
    pub to: String,
    pub gain_db: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ClipView {
    pub key: u64,
    /// How commands address the clip, such as `@12`.
    pub reference: String,
    pub pattern: String,
    pub at: f64,
    /// The length of one repeat.
    pub pattern_beats: f64,
    pub repeats: u32,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ReturnView {
    pub key: u64,
    pub id: String,
    pub gain_db: f64,
    pub pan: f64,
    pub mute: bool,
    pub effects: Vec<EffectView>,
    pub lanes: Vec<LaneView>,
    pub lane_targets: Vec<LaneTarget>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MasterView {
    pub gain_db: f64,
    pub effects: Vec<EffectView>,
    pub lanes: Vec<LaneView>,
    pub lane_targets: Vec<LaneTarget>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SectionView {
    pub key: u64,
    pub id: String,
    pub at: f64,
    pub length_beats: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Part {
    /// Title, tempo or length.
    Session,
    Track,
    Clip,
    Return,
    Master,
    Section,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Delta {
    Added,
    Changed,
    Removed,
}

/// An object a change affected. `key` is 0 for the session and the master.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Touch {
    pub part: Part,
    pub key: u64,
    pub delta: Delta,
}

fn field_value(v: Option<&Value>) -> FieldValue {
    match v {
        Some(Value::Float(x)) => FieldValue::Number { value: *x },
        Some(Value::Int(n)) => FieldValue::Number {
            value: n.to_f64().unwrap_or(0.0),
        },
        Some(Value::Str(s)) => FieldValue::Text { value: s.clone() },
        Some(Value::Bool(b)) => FieldValue::Flag { value: *b },
        _ => FieldValue::Absent,
    }
}

fn initial(i: Initial) -> FieldValue {
    match i {
        Initial::Number(value) => FieldValue::Number { value },
        Initial::Text(s) => FieldValue::Text { value: s.to_string() },
        Initial::Flag(value) => FieldValue::Flag { value },
        Initial::Required | Initial::Absent => FieldValue::Absent,
    }
}

/// How an effect is addressed within its chain: its ID, else its place.
fn effect_ref(e: &Effect, index: usize) -> String {
    e.id().filter(|id| !id.is_empty()).map_or_else(|| index.to_string(), str::to_string)
}

/// An owner's lanes by what they drive, as (effect index, field name) for an
/// effect's and the lane's own parameter otherwise, with each lane's handle.
struct Driven {
    effects: HashMap<(usize, String), u64>,
    others: HashSet<String>,
}

impl Driven {
    fn new(owner: Owner, lanes: &[Lane], handles: &[Item]) -> Driven {
        let mut out = Driven {
            effects: HashMap::new(),
            others: HashSet::new(),
        };
        for (i, lane) in lanes.iter().enumerate() {
            match target(owner, &lane.param) {
                Ok(t) => match &t.kind {
                    TargetKind::Effect { index, .. } => {
                        out.effects.insert((*index, t.name()), handle(handles, i));
                    }
                    _ => {
                        out.others.insert(t.key());
                    }
                },
                Err(_) => {}
            }
        }
        out
    }
}

/// An effect's fields, drawn from the model's description of its type.
fn fields(e: &Effect, index: usize, tracks: &[String], driven: &Driven) -> Vec<FieldView> {
    let dump = e.dump(false);
    let reference = effect_ref(e, index);
    let field = |f: &describe::Field, value: Option<&Value>, name: String, band: Option<u32>| {
        let kind = if matches!(e, Effect::Eq(_)) { "eq" } else { e.kind() };
        let automatable = describe::automatable(kind, f.name).is_some();
        FieldView {
            label: f.label.to_string(),
            kind: match f.kind {
                Kind::Number => FieldKind::Number,
                Kind::Integer => FieldKind::Integer,
                Kind::Choice => FieldKind::Choice,
                Kind::Flag => FieldKind::Flag,
                Kind::Beats => FieldKind::Beats,
                Kind::Track => FieldKind::Track,
            },
            value: field_value(value),
            min: f.min,
            max: f.max,
            unit: f.unit.to_string(),
            log: f.log,
            choices: match f.kind {
                Kind::Track => tracks.to_vec(),
                _ => f.choices.iter().map(|c| c.to_string()).collect(),
            },
            optional: f.default == Initial::Absent,
            initial: initial(if f.default == Initial::Required || f.default == Initial::Absent { f.suggested } else { f.default }),
            live: !f.structural,
            param: automatable.then(|| format!("effects.{reference}.{name}")),
            lane: driven.effects.get(&(index, name.clone())).copied(),
            band,
            name,
        }
    };
    match e {
        Effect::Eq(eq) => {
            let Some(Value::List(bands)) = dump.get("bands") else { return Vec::new() };
            (0..eq.bands.len())
                .flat_map(|j| describe::BAND.iter().map(move |f| (j, f)))
                .map(|(j, f)| field(f, bands[j].get(f.name), format!("bands.{j}.{}", f.name), Some(j as u32)))
                .collect()
        }
        _ => describe::effect(e.kind()).iter().map(|f| field(f, dump.get(f.name), f.name.to_string(), None)).collect(),
    }
}

/// `tracks` are the tracks that may key the chain's compressors.
fn effects(chain: &[Effect], handles: &[Item], tracks: &[String], driven: &Driven) -> Vec<EffectView> {
    chain
        .iter()
        .enumerate()
        .map(|(i, e)| EffectView {
            key: handle(handles, i),
            kind: e.kind().to_string(),
            id: e.id().filter(|id| !id.is_empty()).map(str::to_string),
            bypass: e.bypass(),
            fields: fields(e, i, tracks, driven),
            bands: match e {
                Effect::Eq(eq) => eq.bands.len() as u32,
                _ => 0,
            },
        })
        .collect()
}

/// What an effect goes by in a lane's label: its ID, or its type.
fn effect_name(e: &Effect) -> &str {
    e.id().filter(|id| !id.is_empty()).unwrap_or(e.kind())
}

/// A lane parameter as a person reads it, with its unit.
fn lane_label(owner: Owner, kind: &TargetKind, field: &str) -> (String, String) {
    match kind {
        TargetKind::Channel if field == "pan" => ("Pan".into(), String::new()),
        TargetKind::Channel => ("Volume".into(), "dB".into()),
        TargetKind::Send(to) => (format!("Send {to}"), "dB".into()),
        TargetKind::Effect { index, band } => {
            let e = &owner.effects()[*index];
            let spec = |fields: &'static [describe::Field]| fields.iter().find(|f| f.name == field);
            match band {
                Some(b) => {
                    let f = spec(describe::BAND);
                    (
                        format!("{} band {} {}", effect_name(e), b + 1, f.map_or(field, |f| f.label)),
                        f.map_or("", |f| f.unit).to_string(),
                    )
                }
                None => {
                    let f = spec(describe::effect(e.kind()));
                    (
                        format!("{} {}", effect_name(e), f.map_or(field, |f| f.label)),
                        f.map_or("", |f| f.unit).to_string(),
                    )
                }
            }
        }
    }
}

fn lanes(owner: Owner, lanes: &[Lane], handles: &[Item]) -> Vec<LaneView> {
    lanes
        .iter()
        .enumerate()
        .filter_map(|(i, lane)| {
            let t = target(owner, &lane.param).ok()?;
            let (label, unit) = lane_label(owner, &t.kind, &t.field);
            // Levels show the range the faders cover.
            let (min, max) = match &t.kind {
                TargetKind::Channel if t.field == "gain_db" => (-60.0, 6.0),
                TargetKind::Send(_) => (-60.0, 0.0),
                _ => (t.range.low, t.range.high),
            };
            let point_handles = handles.get(i).map(|item| items(&item.node, "points")).unwrap_or(&[]);
            Some(LaneView {
                key: handle(handles, i),
                param: lane.param.clone(),
                label,
                unit,
                min,
                max,
                log: t.domain == Domain::Log,
                points: lane
                    .points
                    .iter()
                    .enumerate()
                    .map(|(j, p)| PointView {
                        key: handle(point_handles, j),
                        at: p.at_exact().to_f64().unwrap_or(0.0),
                        value: p.value,
                        hold: p.curve == Curve::Hold,
                    })
                    .collect(),
            })
        })
        .collect()
}

/// The parameters of an owner that could have a lane and have none.
fn lane_targets(owner: Owner, sends: &[String], driven: &Driven) -> Vec<LaneTarget> {
    let mut params: Vec<String> = vec!["gain_db".into()];
    if !matches!(owner, Owner::Master(_)) {
        params.push("pan".into());
    }
    params.extend(sends.iter().map(|to| format!("sends.{to}.gain_db")));
    let mut out: Vec<LaneTarget> = params
        .into_iter()
        .filter_map(|param| {
            let t = target(owner, &param).ok()?;
            (!driven.others.contains(&t.key())).then(|| LaneTarget {
                label: lane_label(owner, &t.kind, &t.field).0,
                param,
            })
        })
        .collect();
    for (i, e) in owner.effects().iter().enumerate() {
        let reference = effect_ref(e, i);
        let names: Vec<String> = match e {
            Effect::Eq(eq) => (0..eq.bands.len())
                .flat_map(|j| describe::BAND.iter().map(move |f| (j, f)))
                .filter(|(_, f)| describe::automatable("eq", f.name).is_some())
                .map(|(j, f)| format!("bands.{j}.{}", f.name))
                .collect(),
            _ => describe::effect(e.kind())
                .iter()
                .filter(|f| describe::automatable(e.kind(), f.name).is_some())
                .map(|f| f.name.to_string())
                .collect(),
        };
        for name in names {
            if driven.effects.contains_key(&(i, name.clone())) {
                continue;
            }
            let param = format!("effects.{reference}.{name}");
            if let Ok(t) = target(owner, &param) {
                out.push(LaneTarget {
                    label: lane_label(owner, &t.kind, &t.field).0,
                    param,
                });
            }
        }
    }
    out
}

/// A channel's effects, lanes and what could still be automated.
fn channel(owner: Owner, node: Option<&Node>, tracks: &[String], sends: &[String]) -> (Vec<EffectView>, Vec<LaneView>, Vec<LaneTarget>) {
    let (effect_items, lane_items) = match node {
        Some(n) => (items(n, "effects"), items(n, "automation")),
        None => (&[][..], &[][..]),
    };
    let driven = Driven::new(owner, owner.automation(), lane_items);
    (
        effects(owner.effects(), effect_items, tracks, &driven),
        lanes(owner, owner.automation(), lane_items),
        lane_targets(owner, sends, &driven),
    )
}

/// The tracks that may key a compressor of `owner`: every other track.
fn keys(p: &Project, owner: Option<&str>) -> Vec<String> {
    p.tracks.iter().map(|t| t.id.clone()).filter(|id| Some(id.as_str()) != owner).collect()
}

fn items<'a>(node: &'a Node, key: &str) -> &'a [Item] {
    node.get(key).map(Node::items).unwrap_or(&[])
}

fn handle(items: &[Item], index: usize) -> u64 {
    items.get(index).map_or(0, |i| i.handle)
}

/// The arrangement of a revision.
pub fn arrangement(doc: &Doc, revision: u64) -> Arrangement {
    let p = &doc.project;
    let tree = doc.tree();
    let float = |x: num_rational::BigRational| x.to_f64().unwrap_or(0.0);
    let track_items = items(&tree, "tracks");
    let tracks = p
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let node = track_items.get(i).map(|item| &item.node);
            let clip_items = node.map(|n| items(n, "clips")).unwrap_or(&[]);
            let sends: Vec<String> = t.sends.iter().map(|s| s.to.clone()).collect();
            let (effects, lanes, lane_targets) = channel(Owner::Track(t), node, &keys(p, Some(&t.id)), &sends);
            TrackView {
                key: handle(track_items, i),
                id: t.id.clone(),
                gain_db: t.gain_db,
                pan: t.pan,
                mute: t.mute,
                solo: t.solo,
                effects,
                sends: t
                    .sends
                    .iter()
                    .map(|s| SendView {
                        to: s.to.clone(),
                        gain_db: s.gain_db,
                    })
                    .collect(),
                lanes,
                lane_targets,
                pads: t
                    .pads
                    .iter()
                    .map(|(name, pad)| PadView {
                        name: name.clone(),
                        sample: pad.sample.clone(),
                        gain_db: pad.gain_db,
                        gate: pad.mode == aaw_model::PadMode::Gate,
                    })
                    .collect(),
                clips: t
                    .clips
                    .iter()
                    .enumerate()
                    .map(|(j, c)| {
                        let key = handle(clip_items, j);
                        ClipView {
                            key,
                            reference: aaw_host::tree::handle_text(key),
                            pattern: c.pattern.clone(),
                            at: float(c.at_exact()),
                            pattern_beats: p.patterns.get(&c.pattern).map_or(0.0, |x| float(x.length_exact())),
                            repeats: c.repeats.max(0) as u32,
                        }
                    })
                    .collect(),
            }
        })
        .collect();
    let return_items = items(&tree, "returns");
    let returns = p
        .returns
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let node = return_items.get(i).map(|item| &item.node);
            let (effects, lanes, lane_targets) = channel(Owner::Return(r), node, &keys(p, None), &[]);
            ReturnView {
                key: handle(return_items, i),
                id: r.id.clone(),
                gain_db: r.gain_db,
                pan: r.pan,
                mute: r.mute,
                effects,
                lanes,
                lane_targets,
            }
        })
        .collect();
    let section_items = items(&tree, "sections");
    let sections = p
        .sections
        .iter()
        .enumerate()
        .map(|(i, s)| SectionView {
            key: handle(section_items, i),
            id: s.id.clone(),
            at: float(s.at_exact()),
            length_beats: float(s.length_exact()),
        })
        .collect();
    // A master compressor cannot be keyed.
    let (master_effects, master_lanes, master_targets) = channel(Owner::Master(&p.master), tree.get("master"), &[], &[]);
    Arrangement {
        revision,
        project_sha256: doc.sha.clone(),
        title: p.session.title.clone(),
        tempo: p.session.tempo,
        // The schema allows 4/4 only.
        beats_per_bar: 4,
        length_beats: float(p.session.length_exact()),
        tracks,
        returns,
        master: MasterView {
            gain_db: p.session.master_gain_db,
            effects: master_effects,
            lanes: master_lanes,
            lane_targets: master_targets,
        },
        sections,
    }
}

fn same(a: Option<&Node>, b: Option<&Node>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => py_eq(&a.value(), &b.value()),
        (None, None) => true,
        _ => false,
    }
}

/// A map node without one of its keys.
fn without(node: &Node, key: &str) -> Node {
    let mut n = node.clone();
    if let Some(m) = n.map_mut() {
        m.shift_remove(key);
    }
    n
}

/// Compares two lists by handle. `changed` decides for items in both.
fn list_delta(part: Part, old: &[Item], new: &[Item], changed: impl Fn(&Item, &Item) -> bool, out: &mut Vec<Touch>) {
    let before: HashMap<u64, (usize, &Item)> = old.iter().enumerate().map(|(i, item)| (item.handle, (i, item))).collect();
    let after: HashSet<u64> = new.iter().map(|i| i.handle).collect();
    for (i, item) in new.iter().enumerate() {
        let delta = match before.get(&item.handle) {
            None => Delta::Added,
            Some((j, o)) if *j != i || changed(o, item) => Delta::Changed,
            Some(_) => continue,
        };
        out.push(Touch {
            part,
            key: item.handle,
            delta,
        });
    }
    for item in old.iter().filter(|i| !after.contains(&i.handle)) {
        out.push(Touch {
            part,
            key: item.handle,
            delta: Delta::Removed,
        });
    }
}

/// Every clip by handle, with the handle of its track.
fn clips(tree: &Node) -> Vec<(u64, &Item)> {
    items(tree, "tracks")
        .iter()
        .flat_map(|t| items(&t.node, "clips").iter().map(move |c| (t.handle, c)))
        .collect()
}

/// The objects that differ between two revisions, for highlighting. A track's
/// clips count separately from the track. A clip also counts as changed when
/// its pattern changed, since it then plays something else.
pub fn touched(old: &Doc, new: &Doc) -> Vec<Touch> {
    let (a, b) = (old.tree(), new.tree());
    let mut out = Vec::new();
    let whole = |part: Part| Touch {
        part,
        key: 0,
        delta: Delta::Changed,
    };
    let session = |t: &Node| t.get("session").map(|s| without(s, "master_gain_db"));
    if !same(session(&a).as_ref(), session(&b).as_ref()) {
        out.push(whole(Part::Session));
    }
    let master_gain = |t: &'_ Node| t.get("session").and_then(|s| s.get("master_gain_db")).cloned();
    if !same(a.get("master"), b.get("master")) || !same(master_gain(&a).as_ref(), master_gain(&b).as_ref()) {
        out.push(whole(Part::Master));
    }
    let differs = |o: &Item, n: &Item| !same(Some(&o.node), Some(&n.node));
    list_delta(
        Part::Track,
        items(&a, "tracks"),
        items(&b, "tracks"),
        |o, n| !same(Some(&without(&o.node, "clips")), Some(&without(&n.node, "clips"))),
        &mut out,
    );

    let patterns = |t: &Node, name: &str| t.get("patterns").and_then(|p| p.get(name)).cloned();
    let (old_clips, new_clips) = (clips(&a), clips(&b));
    let before: HashMap<u64, (u64, &Item)> = old_clips.iter().map(|(track, c)| (c.handle, (*track, *c))).collect();
    let after: HashSet<u64> = new_clips.iter().map(|(_, c)| c.handle).collect();
    for (track, clip) in &new_clips {
        let delta = match before.get(&clip.handle) {
            None => Delta::Added,
            Some((old_track, o)) => {
                let pattern = clip.node.field("pattern").unwrap_or_default();
                let replayed = !same(patterns(&a, pattern).as_ref(), patterns(&b, pattern).as_ref());
                if old_track != track || differs(o, clip) || replayed {
                    Delta::Changed
                } else {
                    continue;
                }
            }
        };
        out.push(Touch {
            part: Part::Clip,
            key: clip.handle,
            delta,
        });
    }
    for (_, clip) in old_clips.iter().filter(|(_, c)| !after.contains(&c.handle)) {
        out.push(Touch {
            part: Part::Clip,
            key: clip.handle,
            delta: Delta::Removed,
        });
    }

    list_delta(Part::Return, items(&a, "returns"), items(&b, "returns"), differs, &mut out);
    list_delta(Part::Section, items(&a, "sections"), items(&b, "sections"), differs, &mut out);
    out
}
