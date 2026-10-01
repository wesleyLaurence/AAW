//! What the app draws: the arrangement of a revision, and which objects a
//! change touched. Positions are beats as floats, for display only; the song's
//! exact values stay in the host.

use aaw_host::session::Doc;
use aaw_host::tree::{Item, Node};
use aaw_model::value::py_eq;
use aaw_model::{Effect, Lane};
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

/// An effect in a chain, as a header names it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EffectView {
    pub kind: String,
    pub bypass: bool,
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
    /// The returns the track sends to.
    pub sends: Vec<String>,
    /// The parameters with automation lanes.
    pub automation: Vec<String>,
    pub clips: Vec<ClipView>,
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
    pub automation: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MasterView {
    pub gain_db: f64,
    pub effects: Vec<EffectView>,
    pub automation: Vec<String>,
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

fn effects(chain: &[Effect]) -> Vec<EffectView> {
    chain
        .iter()
        .map(|e| EffectView {
            kind: e.kind().to_string(),
            bypass: e.bypass(),
        })
        .collect()
}

fn params(lanes: &[Lane]) -> Vec<String> {
    lanes.iter().map(|l| l.param.clone()).collect()
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
            let clip_items = track_items.get(i).map(|item| items(&item.node, "clips")).unwrap_or(&[]);
            TrackView {
                key: handle(track_items, i),
                id: t.id.clone(),
                gain_db: t.gain_db,
                pan: t.pan,
                mute: t.mute,
                solo: t.solo,
                effects: effects(&t.effects),
                sends: t.sends.iter().map(|s| s.to.clone()).collect(),
                automation: params(&t.automation),
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
        .map(|(i, r)| ReturnView {
            key: handle(return_items, i),
            id: r.id.clone(),
            gain_db: r.gain_db,
            pan: r.pan,
            mute: r.mute,
            effects: effects(&r.effects),
            automation: params(&r.automation),
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
            effects: effects(&p.master.effects),
            automation: params(&p.master.automation),
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
