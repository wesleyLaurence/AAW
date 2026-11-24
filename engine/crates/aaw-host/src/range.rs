//! Ranges of beats across tracks: a range copied, inserted, deleted or cleared
//! with the clips, audio, automation and sections in it, and a section
//! duplicated, moved or removed with what is under it. The person's markers
//! keep to the beats they mark: they move when time is opened or closed and
//! with a section's beats, and go with beats that are deleted; a copy
//! leaves them where they are.
//!
//! A range is `[start, start + length)`. A clip that crosses an edge is cut
//! there first, so that the range holds exactly what plays in it: a note clip
//! becomes two with the note across the cut divided, and an audio clip is
//! split as `audio.split` splits it; a looped clip's right half starts as far
//! into its loop as the cut is. A lane gains a point at each edge with the value it had
//! there, so the curve outside the range is unchanged.

use crate::command::{beat_text, beat_value, exact, number, set, unique, Edit, Outcome, Span};
use crate::tree::{self, Item, Loc, Node, Step};
use aaw_model::rules::{self, Domain, Owner};
use aaw_model::value::Value;
use aaw_model::Project;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};
use serde_json::json;
use std::collections::HashSet;

type Result<T> = std::result::Result<T, String>;

/// The fades a piece of audio gets at a cut edge that no longer meets its
/// other half, since it joins other audio there: a crossfade's, without the
/// lead, which would move it.
const CUT_FADE_IN_MS: f64 = 4.0;
const CUT_FADE_OUT_MS: f64 = 12.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Note,
    Audio,
}

/// The tracks a range covers, and the groups whose lanes it covers: a group
/// named stands for itself, the groups in it and every track under it. When
/// it covers every track, the groups', returns' and master's lanes, the
/// sections and the song's length go with it.
pub(crate) struct Scope {
    tracks: Vec<Loc>,
    groups: Vec<Loc>,
    names: Vec<String>,
    all: bool,
}

impl Scope {
    /// " on drums, bass" for a label, or nothing for the whole song.
    fn suffix(&self) -> String {
        if self.all {
            String::new()
        } else {
            format!(" on {}", self.names.join(", "))
        }
    }
}

/// What a range command did, for the reply.
#[derive(Default)]
struct Report {
    made: Vec<u64>,
    split: Vec<u64>,
    moved: Vec<u64>,
    removed: Vec<String>,
    /// Audio pieces cut from one clip, left and right of the cut.
    cuts: Vec<(u64, u64)>,
}

/// A copy of what an item plays in a range, and where it goes.
struct Piece {
    track: Loc,
    key: &'static str,
    kind: Kind,
    node: Node,
}

/// A lane's point as the arithmetic needs it.
#[derive(Clone, Debug)]
struct Pt {
    at: BigRational,
    value: f64,
    hold: bool,
    shape: f64,
}

fn pt(p: &Item) -> Pt {
    Pt {
        at: exact(p.node.get("at")).unwrap_or_default(),
        value: number(p.node.get("value")).unwrap_or(0.0),
        hold: p.node.field("curve") == Some("hold"),
        shape: number(p.node.get("shape")).unwrap_or(0.0),
    }
}

fn pts(items: &[Item]) -> Vec<Pt> {
    items.iter().map(pt).collect()
}

/// The value on the segment from `a` to `b` at `at`, as the envelope plays it.
fn interp(domain: Domain, a: &Pt, b: &Pt, at: &BigRational) -> f64 {
    if a.hold {
        return a.value;
    }
    let (va, vb) = match domain {
        Domain::Log => (a.value.ln(), b.value.ln()),
        Domain::Linear => (a.value, b.value),
    };
    let span = (&b.at - &a.at).to_f64().unwrap_or(0.0);
    let mut along = if span > 0.0 { (at - &a.at).to_f64().unwrap_or(0.0) / span } else { 1.0 };
    along = along.clamp(0.0, 1.0);
    if a.shape != 0.0 {
        let power = 4f64.powf(a.shape.abs());
        along = if a.shape > 0.0 { along.powf(power) } else { 1.0 - (1.0 - along).powf(power) };
    }
    let v = along * (vb - va) + va;
    match domain {
        Domain::Log => v.exp(),
        Domain::Linear => v,
    }
}

/// The lane's value as it approaches `p` from before it.
fn left_of(pts: &[Pt], domain: Domain, p: &BigRational) -> f64 {
    let k = pts.iter().filter(|x| x.at < *p).count();
    if k == 0 {
        return pts[0].value;
    }
    if k == pts.len() {
        return pts[k - 1].value;
    }
    interp(domain, &pts[k - 1], &pts[k], p)
}

/// The lane's value at `p`.
fn right_of(pts: &[Pt], domain: Domain, p: &BigRational) -> f64 {
    let k = pts.iter().filter(|x| x.at <= *p).count();
    if k == 0 {
        return pts[0].value;
    }
    if k == pts.len() {
        return pts[k - 1].value;
    }
    interp(domain, &pts[k - 1], &pts[k], p)
}

fn point(at: &BigRational, value: f64, next: &mut u64) -> Item {
    *next += 1;
    let mut map = indexmap::IndexMap::new();
    map.insert("at".to_string(), Node::Leaf(beat_value(at)));
    map.insert("value".to_string(), Node::Leaf(Value::Float(value)));
    Item {
        handle: *next,
        node: Node::Map(map),
    }
}

fn point_at(item: &Item) -> BigRational {
    exact(item.node.get("at")).unwrap_or_default()
}

fn shift_point(item: &mut Item, by: &BigRational) {
    let at = point_at(item) + by;
    set(&mut item.node, "at", beat_value(&at));
}

/// Whether the lane plays the same without its point `i`, which is linear
/// and unshaped: a point on the line between its neighbours, or beside one
/// of the same value.
fn redundant(items: &[Item], i: usize, domain: Domain) -> bool {
    let p = pt(&items[i]);
    let before = i.checked_sub(1).map(|j| pt(&items[j]));
    let after = items.get(i + 1).map(pt);
    match (before, after) {
        (None, None) => false,
        // Before the first point the lane holds its value; a point at 0
        // beside another says nothing a song plays.
        (None, Some(b)) => p.value == b.value || (p.at == b.at && p.at.is_zero()),
        (Some(a), None) => p.value == a.value,
        (Some(a), Some(b)) => {
            if a.hold {
                a.value == p.value && p.value == b.value
            } else if a.shape != 0.0 {
                false
            } else {
                (interp(domain, &a, &b, &p.at) - p.value).abs() <= 1e-9 * p.value.abs().max(1.0)
            }
        }
    }
}

/// The lane without the points that change nothing among those an edit
/// added and those at its edges, and never more than two points at one
/// position.
fn tidy(mut items: Vec<Item>, added: &HashSet<u64>, edges: &[&BigRational], domain: Domain) -> Vec<Item> {
    let mut i = 0;
    while i < items.len() {
        let p = pt(&items[i]);
        let candidate = added.contains(&items[i].handle) || (edges.contains(&&p.at) && !p.hold && p.shape == 0.0);
        if candidate && redundant(&items, i, domain) {
            items.remove(i);
        } else {
            i += 1;
        }
    }
    // Of three or more points at one position, the first and the last stay.
    let mut i = 1;
    while i + 1 < items.len() {
        if point_at(&items[i - 1]) == point_at(&items[i]) && point_at(&items[i]) == point_at(&items[i + 1]) {
            items.remove(i);
        } else {
            i += 1;
        }
    }
    items
}

/// Clears the lane between `s` and `e`, before tidying: the curve outside
/// keeps its values at the edges and runs straight between them.
fn clear_raw(items: Vec<Item>, s: &BigRational, e: &BigRational, domain: Domain, next: &mut u64) -> (Vec<Item>, HashSet<u64>) {
    let p = pts(&items);
    let mut added = HashSet::new();
    if p.is_empty() {
        return (items, added);
    }
    let (l, r) = (left_of(&p, domain, s), right_of(&p, domain, e));
    let has_e = p.iter().any(|x| x.at == *e);
    let mut out = Vec::new();
    let mut after = Vec::new();
    for item in items {
        let at = point_at(&item);
        if at < *s {
            out.push(item);
        } else if at >= *e {
            after.push(item);
        }
    }
    let edge = point(s, l, next);
    added.insert(edge.handle);
    out.push(edge);
    if !has_e {
        let edge = point(e, r, next);
        added.insert(edge.handle);
        out.push(edge);
    }
    out.extend(after);
    (out, added)
}

/// Clears the lane between `s` and `e`.
fn lane_clear(items: Vec<Item>, s: &BigRational, e: &BigRational, domain: Domain, next: &mut u64) -> Vec<Item> {
    let (out, added) = clear_raw(items, s, e, domain, next);
    tidy(out, &added, &[s, e], domain)
}

/// Deletes the beats from `s` to `e` from the lane and closes the gap: what
/// came before keeps its values up to `s`, and what followed `e` begins there.
fn lane_delete(items: Vec<Item>, s: &BigRational, e: &BigRational, domain: Domain, next: &mut u64) -> Vec<Item> {
    let p = pts(&items);
    if p.is_empty() {
        return items;
    }
    let (l, r) = (left_of(&p, domain, s), right_of(&p, domain, e));
    let at_e = p.iter().filter(|x| x.at == *e).count();
    let len = e - s;
    let mut added = HashSet::new();
    let mut out = Vec::new();
    let mut after = Vec::new();
    for mut item in items {
        let at = point_at(&item);
        if at < *s {
            out.push(item);
        } else if at >= *e {
            shift_point(&mut item, &-len.clone());
            after.push(item);
        }
    }
    if at_e < 2 {
        let edge = point(s, l, next);
        added.insert(edge.handle);
        out.push(edge);
    }
    if at_e == 0 {
        let edge = point(s, r, next);
        added.insert(edge.handle);
        out.push(edge);
    }
    out.extend(after);
    tidy(out, &added, &[s], domain)
}

/// Opens `len` beats at `at`, before tidying: the lane holds its value there
/// across them.
fn insert_raw(items: Vec<Item>, at: &BigRational, len: &BigRational, domain: Domain, next: &mut u64) -> (Vec<Item>, HashSet<u64>) {
    let p = pts(&items);
    let mut added = HashSet::new();
    if p.is_empty() {
        return (items, added);
    }
    let (l, r) = (left_of(&p, domain, at), right_of(&p, domain, at));
    let has_at = p.iter().any(|x| x.at == *at);
    let mut out = Vec::new();
    let mut after = Vec::new();
    for mut item in items {
        if point_at(&item) < *at {
            out.push(item);
        } else {
            shift_point(&mut item, len);
            after.push(item);
        }
    }
    let edge = point(at, l, next);
    added.insert(edge.handle);
    out.push(edge);
    if !has_at {
        let edge = point(&(at + len), r, next);
        added.insert(edge.handle);
        out.push(edge);
    }
    out.extend(after);
    (out, added)
}

/// Opens `len` beats at `at`.
fn lane_insert(items: Vec<Item>, at: &BigRational, len: &BigRational, domain: Domain, next: &mut u64) -> Vec<Item> {
    let (out, added) = insert_raw(items, at, len, domain, next);
    tidy(out, &added, &[at, &(at + len)], domain)
}

/// Puts what the lane did from `s` to `e`, read from `src`, at `to` in
/// `items`: over what is there, or into time opened for it.
fn lane_copy(
    src: &[Pt],
    s: &BigRational,
    e: &BigRational,
    items: Vec<Item>,
    to: &BigRational,
    insert: bool,
    domain: Domain,
    next: &mut u64,
) -> Vec<Item> {
    if src.is_empty() {
        return items;
    }
    let len = e - s;
    let dest_end = to + &len;
    let (mut out, mut added) = if insert {
        insert_raw(items, to, &len, domain, next)
    } else {
        clear_raw(items, to, &dest_end, domain, next)
    };
    let mut pieces = Vec::new();
    if !src.iter().any(|x| x.at == *s) {
        let edge = point(to, right_of(src, domain, s), next);
        added.insert(edge.handle);
        pieces.push(edge);
    }
    let by = to - s;
    for x in src.iter().filter(|x| x.at >= *s && x.at < *e) {
        let mut item = point(&(&x.at + &by), x.value, next);
        if x.hold {
            set(&mut item.node, "curve", Value::str("hold"));
        }
        if x.shape != 0.0 {
            set(&mut item.node, "shape", Value::Float(x.shape));
        }
        pieces.push(item);
    }
    let edge = point(&dest_end, left_of(src, domain, e), next);
    added.insert(edge.handle);
    pieces.push(edge);
    // After the lane's own point at the start, before its own at the end.
    let slot = out.iter().filter(|p| point_at(p) <= *to).count();
    out.splice(slot..slot, pieces);
    tidy(out, &added, &[to, &dest_end], domain)
}

/// A range's end, after checking its length and, with `within`, that it does
/// not reach past the song.
/// The end of a range of `length` beats from `start`; a range may lie
/// past the song's end, where there is nothing, since the end is the song's
/// content (D130).
fn check_range(start: &BigRational, length: &BigRational) -> Result<BigRational> {
    if !length.is_positive() {
        return Err("LENGTH is a number of beats above zero".into());
    }
    Ok(start + length)
}

/// A track's place in the song's tracks.
fn index_of(track: &[Step]) -> usize {
    match track.last() {
        Some(Step::Index(i)) => *i,
        _ => 0,
    }
}

/// What Copy takes of the beats from `start` for `length` on `tracks`
/// (tracks or groups by ID or `@N`, or every track with none), for
/// `range.paste`; the song is not touched.
pub fn take(root: &Node, start: &BigRational, length: &BigRational, tracks: &[String]) -> Result<serde_json::Value> {
    let mut root = root.clone();
    let mut next = 0;
    let edit = Edit { root: &mut root, handles: true, next: &mut next, origin: crate::command::Origin::User, dir: None, also: Vec::new() };
    edit.taken(start, length, tracks)
}

fn beats(s: &BigRational, e: &BigRational) -> String {
    format!("beats {}–{}", beat_text(s), beat_text(e))
}

impl<'a> Edit<'a> {
    /// The tracks a range covers: those named and those under the groups
    /// named, or every track.
    fn scope(&self, tracks: &[String]) -> Result<Scope> {
        let count = self.root.get("tracks").map_or(0, |t| t.items().len());
        let mut locs: Vec<Loc> = Vec::new();
        let mut groups: Vec<Loc> = Vec::new();
        let mut names = Vec::new();
        for id in tracks {
            let loc = match self.track(id) {
                Ok(loc) => loc,
                Err(not_track) => {
                    let Ok(group) = self.at(&format!("groups.{id}")) else {
                        return Err(not_track);
                    };
                    let gid = self.name(&group);
                    if !groups.contains(&group) {
                        names.push(gid.clone());
                    }
                    for (i, g) in self.root.get("groups").map_or(&[][..], Node::items).iter().enumerate() {
                        let loc = vec![Step::Key("groups".into()), Step::Index(i)];
                        if self.chain(g.node.field("id")).contains(&gid) && !groups.contains(&loc) {
                            groups.push(loc);
                        }
                    }
                    for (i, t) in self.root.get("tracks").map_or(&[][..], Node::items).iter().enumerate() {
                        let loc = vec![Step::Key("tracks".into()), Step::Index(i)];
                        if self.chain(t.node.field("group")).contains(&gid) && !locs.contains(&loc) {
                            locs.push(loc);
                        }
                    }
                    continue;
                }
            };
            if !locs.contains(&loc) {
                names.push(self.name(&loc));
                locs.push(loc);
            }
        }
        let all = tracks.is_empty() || locs.len() == count;
        if tracks.is_empty() {
            locs = (0..count).map(|i| vec![Step::Key("tracks".into()), Step::Index(i)]).collect();
        }
        Ok(Scope { tracks: locs, groups, names, all })
    }

    /// The groups above a track or a group, from the one it is in out: the
    /// chain from `group`.
    fn chain(&self, group: Option<&str>) -> Vec<String> {
        let groups = self.root.get("groups").map_or(&[][..], Node::items);
        let mut out: Vec<String> = Vec::new();
        let mut next = group.map(str::to_string);
        while let Some(g) = next {
            if out.contains(&g) {
                break;
            }
            next = groups.iter().find(|x| x.node.field("id") == Some(g.as_str())).and_then(|x| x.node.field("group")).map(str::to_string);
            out.push(g);
        }
        out
    }

    /// The song's length as the tree holds it before the host settles it
    /// to the content after the edit (D130).
    fn song_length(&self) -> BigRational {
        exact(self.root.get("session").and_then(|s| s.get("length_beats"))).unwrap_or_else(|| BigRational::from_integer(16.into()))
    }

    /// The song as it stands, validated, for what a lane's parameter is.
    fn validated(&self) -> Result<Project> {
        Project::validate(&self.root.value()).map_err(|e| e.to_string())
    }

    /// The lists of a track that hold items on the timeline.
    fn lists(&self, track: &[Step]) -> Vec<(&'static str, Kind)> {
        let mut out = Vec::new();
        if self.node(track).get("clips").is_some() {
            out.push(("clips", Kind::Note));
        }
        if self.node(track).get("audio").is_some() {
            out.push(("audio", Kind::Audio));
        }
        out
    }

    /// Where an item starts and ends; no end for an audio clip that plays to
    /// its file's end.
    pub(crate) fn timed(&self, kind: Kind, node: &Node) -> Result<(BigRational, Option<BigRational>)> {
        match kind {
            Kind::Audio => {
                let span = Span::of(node, self.tempo())?;
                Ok((span.at.clone(), span.end_beat()))
            }
            _ => {
                let start = exact(node.get("at")).unwrap_or_default();
                let end = self.clip_span(node).map(|l| &start + l);
                Ok((start, end))
            }
        }
    }

    /// Two items of one, cut at `at`, which is inside it.
    pub(crate) fn split_node(&self, kind: Kind, node: &Node, at: &BigRational) -> Result<(Node, Node)> {
        let mut left = node.clone();
        let mut right = node.clone();
        match kind {
            Kind::Note => {
                let start = exact(node.get("at")).unwrap_or_default();
                let length = exact(node.get("length_beats")).unwrap_or_default();
                let cut = at - &start;
                set(&mut left, "length_beats", beat_value(&cut));
                set(&mut right, "at", beat_value(at));
                set(&mut right, "length_beats", beat_value(&(&length - &cut)));
                set(&mut right, "id", Value::Str(self.next_clip_id()));
                if let Some(every) = exact(node.get("loop_beats")) {
                    // Each half keeps the loop and its notes; the right
                    // starts as far into the loop as the cut is.
                    let offset = exact(node.get("loop_offset_beats")).unwrap_or_default();
                    crate::command::set_offset(&mut right, &crate::command::into_loop(&start, &offset, at, &every));
                    return Ok((left, right));
                }
                let mut lefts = Vec::new();
                let mut rights = Vec::new();
                for n in node.get("notes").map_or(&[][..], Node::items) {
                    let a = exact(n.node.get("at")).unwrap_or_default();
                    let d = exact(n.node.get("duration")).unwrap_or_default();
                    if a >= cut {
                        // From the cut on: its beat is now from the right clip's start.
                        let mut m = n.node.clone();
                        set(&mut m, "at", beat_value(&(&a - &cut)));
                        rights.push(Item { handle: n.handle, node: m });
                    } else {
                        let mut m = n.node.clone();
                        if &a + &d > cut {
                            // Across the cut: held to it on the left, and the
                            // rest from the right clip's start.
                            set(&mut m, "duration", beat_value(&(&cut - &a)));
                            let mut rest = n.node.clone();
                            set(&mut rest, "at", beat_value(&BigRational::zero()));
                            set(&mut rest, "duration", beat_value(&(&a + &d - &cut)));
                            rights.push(Item { handle: 0, node: rest });
                        }
                        lefts.push(Item { handle: n.handle, node: m });
                    }
                }
                if let Some(m) = left.map_mut() {
                    m.insert("notes".into(), Node::List(lefts));
                }
                if let Some(m) = right.map_mut() {
                    m.insert("notes".into(), Node::List(rights));
                }
            }
            Kind::Audio => {
                let span = Span::of(node, self.tempo())?;
                if let (Some(every), Some(length)) = (&span.every, &span.length) {
                    // Each half keeps the loop; the right starts as far into
                    // it as the cut is.
                    let cut = at - &span.at;
                    set(&mut left, "length_beats", beat_value(&cut));
                    set(&mut right, "at", beat_value(at));
                    set(&mut right, "length_beats", beat_value(&(length - &cut)));
                    crate::command::set_offset(&mut right, &span.into_loop(at, every));
                    return Ok((left, right));
                }
                let cut = span.source(at);
                // The halves meet where the audio is continuous, so neither fades there.
                set(&mut left, "source_end_seconds", Value::Float(cut));
                set(&mut left, "fade_out_ms", Value::Float(0.0));
                set(&mut right, "at", beat_value(at));
                set(&mut right, "source_start_seconds", Value::Float(cut));
                set(&mut right, "fade_in_ms", Value::Float(0.0));
                // The cut is at the beat: a lead on the right half would start
                // it, and end the left, that long before the beat.
                if let Some(m) = right.map_mut() {
                    m.shift_remove("lead_ms");
                }
            }
        }
        Ok((left, right))
    }

    /// Whether a clip loops, so that its halves are whole clips of their own
    /// rather than continuous audio.
    fn looped(node: &Node) -> bool {
        exact(node.get("loop_beats")).is_some()
    }

    /// Cuts every item of the scope that crosses `at`.
    fn split_scope(&mut self, scope: &Scope, at: &BigRational, report: &mut Report) -> Result<()> {
        for track in scope.tracks.clone() {
            for (key, kind) in self.lists(&track) {
                let n = self.node(&track).get(key).map_or(0, |l| l.items().len());
                // From the last, so inserting a right half moves no index still to come.
                for i in (0..n).rev() {
                    let loc = [track.clone(), vec![Step::Key(key.into()), Step::Index(i)]].concat();
                    let node = self.node(&loc).clone();
                    let (start, end) = self.timed(kind, &node)?;
                    if !(start < *at && end.as_ref().is_none_or(|e| at < e)) {
                        continue;
                    }
                    let (left, right) = self.split_node(kind, &node, at)?;
                    *self.node_mut(&loc) = left;
                    let left_handle = tree::handle_at(self.root, &loc);
                    let right_handle = self.insert(&track, key, Some(i + 1), right)?;
                    report.split.push(left_handle);
                    report.made.push(right_handle);
                    if kind == Kind::Audio && !Self::looped(&node) {
                        report.cuts.push((left_handle, right_handle));
                    }
                }
            }
        }
        Ok(())
    }

    /// Moves every item of the scope that starts at or after `from` by `by`.
    fn shift_scope(&mut self, scope: &Scope, from: &BigRational, by: &BigRational, report: &mut Report) -> Result<()> {
        for track in scope.tracks.clone() {
            for (key, kind) in self.lists(&track) {
                let n = self.node(&track).get(key).map_or(0, |l| l.items().len());
                for i in 0..n {
                    let loc = [track.clone(), vec![Step::Key(key.into()), Step::Index(i)]].concat();
                    let (start, _) = self.timed(kind, self.node(&loc))?;
                    if start >= *from {
                        self.set_leaf(&loc, "at", beat_value(&(start + by)));
                        report.moved.push(tree::handle_at(self.root, &loc));
                    }
                }
            }
        }
        Ok(())
    }

    /// Removes every item of the scope that starts in `[s, e)`.
    fn remove_scope(&mut self, scope: &Scope, s: &BigRational, e: &BigRational, report: &mut Report) -> Result<()> {
        for track in scope.tracks.clone() {
            for (key, kind) in self.lists(&track) {
                let n = self.node(&track).get(key).map_or(0, |l| l.items().len());
                for i in (0..n).rev() {
                    let loc = [track.clone(), vec![Step::Key(key.into()), Step::Index(i)]].concat();
                    let (start, _) = self.timed(kind, self.node(&loc))?;
                    if start >= *s && start < *e {
                        report.removed.push(self.text(&loc));
                        self.take(&track, key, i)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// One clip of two that meet, when they are one music: an audio clip and the rest of its file as it was
    /// playing, a note clip and the one that follows it, with a note divided
    /// at the join made whole again.
    pub(crate) fn merged(&self, kind: Kind, left: &Node, right: &Node) -> Option<Node> {
        let same = |key: &str| left.get(key).map(|n| crate::command::node_json(n)) == right.get(key).map(|n| crate::command::node_json(n));
        let mut out = left.clone();
        if Self::looped(left) || Self::looped(right) {
            // Two halves of one looped clip: the same loop of the same music,
            // the right going on in the loop from where the left leaves it.
            let every = exact(left.get("loop_beats")).unwrap_or_default();
            let length = exact(left.get("length_beats")).unwrap_or_default();
            let more = exact(right.get("length_beats")).unwrap_or_default();
            let music: &[&str] = match kind {
                Kind::Note => &["notes"],
                Kind::Audio => &["sample", "source_start_seconds", "source_end_seconds", "lead_ms", "gain_db", "fade_in_ms", "fade_out_ms", "fade_curve", "source_bpm", "stretch", "transpose"],
            };
            if !(same("loop_beats") && music.iter().all(|k| same(k)) && every.is_positive()) {
                return None;
            }
            let offset = |n: &Node| exact(n.get("loop_offset_beats")).unwrap_or_default();
            if crate::command::into_loop(&BigRational::zero(), &offset(left), &length, &every) != offset(right) {
                return None;
            }
            set(&mut out, "length_beats", beat_value(&(&length + &more)));
            return Some(out);
        }
        match kind {
            Kind::Audio => {
                let (l, r) = (Span::of(left, self.tempo()).ok()?, Span::of(right, self.tempo()).ok()?);
                let continuous = l.end.is_some_and(|end| (end - r.start).abs() < 1e-9);
                if !(continuous && same("sample") && same("source_bpm") && same("stretch") && same("transpose") && same("gain_db") && same("fade_curve")) {
                    return None;
                }
                let map = out.map_mut()?;
                for key in ["source_end_seconds", "fade_out_ms"] {
                    match right.get(key) {
                        Some(v) => {
                            map.insert(key.into(), v.clone());
                        }
                        None => {
                            map.shift_remove(key);
                        }
                    }
                }
            }
            Kind::Note => {
                let length = exact(left.get("length_beats")).unwrap_or_default();
                let more = exact(right.get("length_beats")).unwrap_or_default();
                set(&mut out, "length_beats", beat_value(&(&length + &more)));
                let mut notes: Vec<Item> = left.get("notes").map_or(&[][..], Node::items).to_vec();
                for n in right.get("notes").map_or(&[][..], Node::items) {
                    let at = exact(n.node.get("at")).unwrap_or_default() + &length;
                    let duration = exact(n.node.get("duration")).unwrap_or_default();
                    let pitch = number(n.node.get("pitch"));
                    let velocity = number(n.node.get("velocity")).unwrap_or(100.0);
                    // A note that starts at the join continues one that ends there.
                    let continues = (at == length).then(|| {
                        notes.iter().position(|m| {
                            let end = exact(m.node.get("at")).unwrap_or_default() + exact(m.node.get("duration")).unwrap_or_default();
                            end == length && number(m.node.get("pitch")) == pitch && number(m.node.get("velocity")).unwrap_or(100.0) == velocity
                        })
                    });
                    match continues.flatten() {
                        Some(i) => {
                            let held = exact(notes[i].node.get("duration")).unwrap_or_default() + duration;
                            set(&mut notes[i].node, "duration", beat_value(&held));
                        }
                        None => {
                            let mut node = n.node.clone();
                            set(&mut node, "at", beat_value(&at));
                            // Its ID stays unless a note of the left clip has it.
                            if n.node.field("id").is_some_and(|id| notes.iter().any(|m| m.node.field("id") == Some(id))) {
                                if let Some(m) = node.map_mut() {
                                    m.shift_remove("id");
                                }
                            }
                            notes.push(Item { handle: n.handle, node });
                        }
                    }
                }
                if let Some(m) = out.map_mut() {
                    m.insert("notes".into(), Node::List(notes));
                }
            }
        }
        Some(out)
    }

    /// Joins the two clips of each track of the scope that meet at `at` and
    /// are one music, after a gap closed there.
    fn merge_scope(&mut self, scope: &Scope, at: &BigRational, report: &mut Report) -> Result<()> {
        let near = |x: &BigRational| (x - at).to_f64().is_some_and(|d| d.abs() < 1e-6);
        for track in scope.tracks.clone() {
            for (key, kind) in self.lists(&track) {
                let items = self.node(&track).get(key).map_or(&[][..], Node::items);
                let (mut left, mut right) = (None, None);
                for (i, item) in items.iter().enumerate() {
                    let (start, end) = self.timed(kind, &item.node)?;
                    if left.is_none() && end.as_ref().is_some_and(near) {
                        left = Some(i);
                    }
                    if right.is_none() && start == *at {
                        right = Some(i);
                    }
                }
                let (Some(l), Some(r)) = (left, right) else { continue };
                if l == r {
                    continue;
                }
                let Some(merged) = self.merged(kind, &items[l].node, &items[r].node) else { continue };
                let gone = items[r].handle;
                let loc = [track.clone(), vec![Step::Key(key.into()), Step::Index(l)]].concat();
                *self.node_mut(&loc) = merged;
                self.take(&track, key, r)?;
                report.made.retain(|h| *h != gone);
                report.moved.retain(|h| *h != gone);
                report.cuts.retain(|(a, b)| *a != gone && *b != gone);
            }
        }
        Ok(())
    }

    /// Copies of what every item of the scope plays in `[s, e)`, cut to the
    /// range where an item crosses an edge; the items themselves are not
    /// touched. A copied audio piece fades at a cut edge, since it joins
    /// other audio there.
    fn extract_scope(&self, scope: &Scope, s: &BigRational, e: &BigRational) -> Result<Vec<Piece>> {
        let mut out = Vec::new();
        for track in &scope.tracks {
            for (key, kind) in self.lists(track) {
                for item in self.node(track).get(key).map_or(&[][..], Node::items) {
                    let (start, end) = self.timed(kind, &item.node)?;
                    if !(start < *e && end.as_ref().is_none_or(|end| end > s)) {
                        continue;
                    }
                    let mut node = item.node.clone();
                    let cut_left = start < *s;
                    if cut_left {
                        node = self.split_node(kind, &node, s)?.1;
                    }
                    let cut_right = end.is_none_or(|end| end > *e);
                    if cut_right {
                        node = self.split_node(kind, &node, e)?.0;
                    }
                    if kind == Kind::Audio && !Self::looped(&node) {
                        if cut_left {
                            set(&mut node, "fade_in_ms", Value::Float(CUT_FADE_IN_MS));
                        }
                        if cut_right {
                            set(&mut node, "fade_out_ms", Value::Float(CUT_FADE_OUT_MS));
                        }
                    }
                    out.push(Piece {
                        track: track.clone(),
                        key,
                        kind,
                        node: Node::new(&node.value()),
                    });
                }
            }
        }
        Ok(out)
    }

    /// Puts pieces into their tracks `by` beats from where they were, as new
    /// objects; a note clip's copy has an ID of its own.
    fn place(&mut self, pieces: Vec<Piece>, by: &BigRational, report: &mut Report) -> Result<()> {
        for mut piece in pieces {
            let start = exact(piece.node.get("at")).unwrap_or_default();
            set(&mut piece.node, "at", beat_value(&(start + by)));
            if piece.kind == Kind::Note {
                set(&mut piece.node, "id", Value::Str(self.next_clip_id()));
            }
            report.made.push(self.insert(&piece.track, piece.key, None, piece.node)?);
        }
        Ok(())
    }

    /// Fades the audio pieces whose cut edge no longer meets its other half.
    fn settle_cuts(&mut self, report: &Report) -> Result<()> {
        let tempo = self.tempo();
        for (l, r) in &report.cuts {
            let left = tree::find(self.root, *l);
            let right = tree::find(self.root, *r);
            let adjacent = match (&left, &right) {
                (Some(a), Some(b)) if a.len() >= 2 && a[..a.len() - 2] == b[..b.len() - 2] => {
                    let end = Span::of(self.node(a), tempo)?.end_beat();
                    let start = Span::of(self.node(b), tempo)?.at;
                    end.is_some_and(|end| (end - start).to_f64().is_some_and(|d| d.abs() < 1e-6))
                }
                _ => false,
            };
            if adjacent {
                continue;
            }
            if let Some(a) = left {
                self.set_leaf(&a, "fade_out_ms", Value::Float(CUT_FADE_OUT_MS));
            }
            if let Some(b) = right {
                self.set_leaf(&b, "fade_in_ms", Value::Float(CUT_FADE_IN_MS));
            }
        }
        Ok(())
    }

    /// The owners whose lanes a range covers: its tracks and its groups, and
    /// every group, the returns and the master when it covers every track.
    fn lane_owners(&self, scope: &Scope) -> Vec<Loc> {
        let mut out = scope.tracks.clone();
        if !scope.all {
            out.extend(scope.groups.iter().cloned());
        } else {
            for key in ["groups", "returns"] {
                for i in 0..self.root.get(key).map_or(0, |n| n.items().len()) {
                    out.push(vec![Step::Key(key.into()), Step::Index(i)]);
                }
            }
            out.push(vec![Step::Key("master".into())]);
        }
        out
    }

    fn owner_of<'p>(project: &'p Project, owner: &[Step]) -> Owner<'p> {
        match owner {
            [Step::Key(k), Step::Index(i)] if k == "tracks" => Owner::Track(&project.tracks[*i]),
            [Step::Key(k), Step::Index(i)] if k == "groups" => Owner::Group(&project.groups[*i]),
            [Step::Key(k), Step::Index(i)] if k == "returns" => Owner::Return(&project.returns[*i]),
            _ => Owner::Master(&project.master),
        }
    }

    fn lane_domain(project: &Project, owner: &[Step], param: &str) -> Domain {
        rules::target(Self::owner_of(project, owner), param).map_or(Domain::Linear, |t| t.domain)
    }

    /// Rewrites the points of every lane of the scope.
    fn edit_lanes(&mut self, scope: &Scope, project: &Project, f: impl Fn(Vec<Item>, Domain, &mut u64) -> Vec<Item>) -> Result<()> {
        for owner in self.lane_owners(scope) {
            let lanes = self.node(&owner).get("automation").map_or(0, |l| l.items().len());
            for i in 0..lanes {
                let lane = [owner.clone(), vec![Step::Key("automation".into()), Step::Index(i)]].concat();
                let param = self.node(&lane).field("param").unwrap_or_default().to_string();
                let domain = Self::lane_domain(project, &owner, &param);
                let points = std::mem::take(self.list(&lane, "points")?);
                let mut next = *self.next;
                let points = f(points, domain, &mut next);
                *self.next = next;
                *self.list(&lane, "points")? = points;
            }
        }
        Ok(())
    }

    /// The sections, each as its index, start and end.
    fn section_spans(&self) -> Vec<(usize, BigRational, BigRational)> {
        self.root
            .get("sections")
            .map_or(&[][..], Node::items)
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let at = exact(s.node.get("at")).unwrap_or_default();
                let len = exact(s.node.get("length_beats")).unwrap_or_default();
                (i, at.clone(), at + len)
            })
            .collect()
    }

    fn set_section(&mut self, i: usize, at: &BigRational, end: &BigRational) {
        let loc = vec![Step::Key("sections".into()), Step::Index(i)];
        self.set_leaf(&loc, "at", beat_value(at));
        self.set_leaf(&loc, "length_beats", beat_value(&(end - at)));
    }

    /// Takes the beats from `s` to `e` out of the sections: those inside go,
    /// those across an edge lose the beats in it, and those after move earlier.
    fn sections_delete(&mut self, s: &BigRational, e: &BigRational, report: &mut Report) -> Result<()> {
        let len = e - s;
        for (i, a, b) in self.section_spans().into_iter().rev() {
            let a2 = if a >= *e { &a - &len } else { a.clone().min(s.clone()) };
            let b2 = if b >= *e { &b - &len } else { b.clone().min(s.clone()) };
            if b2 <= a2 {
                let loc = vec![Step::Key("sections".into()), Step::Index(i)];
                report.removed.push(self.text(&loc));
                self.take(&[], "sections", i)?;
            } else if a2 != a || b2 != b {
                self.set_section(i, &a2, &b2);
            }
        }
        Ok(())
    }

    /// Opens `len` beats at `at` in the sections: one around `at` grows, and
    /// those at or after it move later.
    fn sections_insert(&mut self, at: &BigRational, len: &BigRational) {
        for (i, a, b) in self.section_spans() {
            let a2 = if a >= *at { &a + len } else { a.clone() };
            let b2 = if b > *at { &b + len } else { b.clone() };
            if a2 != a || b2 != b {
                self.set_section(i, &a2, &b2);
            }
        }
    }

    /// Removes the sections that lie inside `[s, e)`, except the one at `keep`.
    fn sections_remove_inside(&mut self, s: &BigRational, e: &BigRational, keep: Option<usize>, report: &mut Report) -> Result<()> {
        for (i, a, b) in self.section_spans().into_iter().rev() {
            if Some(i) != keep && a >= *s && b <= *e {
                let loc = vec![Step::Key("sections".into()), Step::Index(i)];
                report.removed.push(self.text(&loc));
                self.take(&[], "sections", i)?;
            }
        }
        Ok(())
    }

    /// Copies of the sections inside `[s, e)`.
    fn sections_inside(&self, s: &BigRational, e: &BigRational) -> Vec<Node> {
        self.section_spans()
            .into_iter()
            .filter(|(_, a, b)| a >= s && b <= e)
            .map(|(i, _, _)| Node::new(&self.root.get("sections").expect("sections").items()[i].node.value()))
            .collect()
    }

    /// Puts copies of sections `by` beats later, each under a free ID: the
    /// one given for `rename.0`, else its own with a number.
    fn sections_place(&mut self, clones: Vec<Node>, by: &BigRational, rename: Option<(&str, &str)>, report: &mut Report) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for mut clone in clones {
            let own = clone.field("id").unwrap_or_default().to_string();
            let taken: Vec<String> = self
                .root
                .get("sections")
                .map_or(&[][..], Node::items)
                .iter()
                .filter_map(|s| s.node.field("id").map(str::to_string))
                .collect();
            let id = match rename {
                Some((from, to)) if from == own => {
                    if taken.iter().any(|t| t == to) {
                        return Err(format!("Section ID {to} is taken"));
                    }
                    to.to_string()
                }
                _ => unique(&own, |n| taken.iter().any(|t| t == n)),
            };
            set(&mut clone, "id", Value::str(&id));
            let at = exact(clone.get("at")).unwrap_or_default() + by;
            set(&mut clone, "at", beat_value(&at));
            report.made.push(self.insert(&[], "sections", None, clone)?);
            ids.push(id);
        }
        Ok(ids)
    }

    /// The markers, each as its index and beat.
    fn marker_beats(&self) -> Vec<(usize, BigRational)> {
        self.markers().iter().enumerate().map(|(i, m)| (i, exact(m.node.get("at")).unwrap_or_default())).collect()
    }

    fn set_marker(&mut self, i: usize, at: &BigRational) {
        self.set_leaf(&[Step::Key("markers".into()), Step::Index(i)], "at", beat_value(at));
    }

    /// Opens `len` beats at `at` under the markers: those at or after it
    /// move later, as what they mark does.
    fn markers_insert(&mut self, at: &BigRational, len: &BigRational) {
        for (i, a) in self.marker_beats() {
            if a >= *at {
                self.set_marker(i, &(&a + len));
            }
        }
    }

    /// Takes the beats from `s` to `e` out from under the markers: those
    /// inside go with what they marked, and those after move earlier.
    fn markers_delete(&mut self, s: &BigRational, e: &BigRational, report: &mut Report) -> Result<()> {
        let len = e - s;
        for (i, a) in self.marker_beats().into_iter().rev() {
            if a >= *e {
                self.set_marker(i, &(&a - &len));
            } else if a >= *s {
                let loc = vec![Step::Key("markers".into()), Step::Index(i)];
                report.removed.push(self.text(&loc));
                self.take(&[], "markers", i)?;
            }
        }
        Ok(())
    }

    /// Moves the markers inside `[s, e)` by `by` beats, with the beats they
    /// mark, and keeps the list in time order.
    fn markers_move(&mut self, s: &BigRational, e: &BigRational, by: &BigRational) -> Result<()> {
        for (i, a) in self.marker_beats() {
            if a >= *s && a < *e {
                self.set_marker(i, &(&a + by));
            }
        }
        self.list(&[], "markers")?.sort_by_key(|m| exact(m.node.get("at")).unwrap_or_default());
        Ok(())
    }

    /// The reply's lists: what was split, moved and removed, by path, and
    /// what was made as the command's objects.
    fn finish(&mut self, out: &mut Outcome, report: Report) -> Result<()> {
        self.settle_cuts(&report)?;
        let paths = |root: &Node, handles: &[u64]| -> Vec<String> {
            let mut seen = HashSet::new();
            handles
                .iter()
                .filter(|h| seen.insert(**h))
                .filter_map(|h| tree::find(root, *h).map(|loc| tree::path_text(root, &loc)))
                .collect()
        };
        let split = paths(self.root, &report.split);
        let moved = paths(self.root, &report.moved);
        out.made.extend(report.made);
        out.report.insert("split".into(), json!(split));
        out.report.insert("moved".into(), json!(moved));
        out.report.insert("removed".into(), json!(report.removed));
        out.report.insert("length_beats".into(), json!(crate::command::to_json(&beat_value(&self.song_length()))));
        Ok(())
    }

    /// `range.copy`: a copy of the range at `to`, over what is there or into
    /// time opened for it. `rename` gives the copy of one section an ID.
    pub(crate) fn range_copy(
        &mut self,
        out: &mut Outcome,
        start: &BigRational,
        length: &BigRational,
        to: &BigRational,
        insert: bool,
        tracks: &[String],
        rename: Option<(&str, &str)>,
    ) -> Result<()> {
        let scope = self.scope(tracks)?;
        let end = check_range(start, length)?;
        let project = self.validated()?;
        let mut report = Report::default();
        // The source is read before anything moves, and not touched.
        let pieces = self.extract_scope(&scope, start, &end)?;
        let clones = if scope.all { self.sections_inside(start, &end) } else { Vec::new() };
        // The destination.
        let dest_end = to + length;
        self.split_scope(&scope, to, &mut report)?;
        if insert {
            self.shift_scope(&scope, to, length, &mut report)?;
            if scope.all {
                self.sections_insert(to, length);
                self.markers_insert(to, length);
            }
        } else {
            self.split_scope(&scope, &dest_end, &mut report)?;
            self.remove_scope(&scope, to, &dest_end, &mut report)?;
            if scope.all {
                self.sections_remove_inside(to, &dest_end, None, &mut report)?;
            }
        }
        let by = to - start;
        self.place(pieces, &by, &mut report)?;
        self.edit_lanes(&scope, &project, |items, domain, next| {
            let src = pts(&items);
            lane_copy(&src, start, &end, items, to, insert, domain, next)
        })?;
        let ids = if scope.all { self.sections_place(clones, &by, rename, &mut report)? } else { Vec::new() };
        out.report.insert("sections".into(), json!(ids));
        out.label = if insert {
            format!("Insert a copy of {} at {}{}", beats(start, &end), beat_text(to), scope.suffix())
        } else {
            format!("Copy {} to {}{}", beats(start, &end), beat_text(to), scope.suffix())
        };
        self.finish(out, report)
    }

    /// `range.insert`: empty time at `at`; everything from there moves later.
    pub(crate) fn range_insert(&mut self, out: &mut Outcome, at: &BigRational, length: &BigRational, tracks: &[String]) -> Result<()> {
        let scope = self.scope(tracks)?;
        if !length.is_positive() {
            return Err("LENGTH is a number of beats above zero".into());
        }
        let project = self.validated()?;
        let mut report = Report::default();
        self.split_scope(&scope, at, &mut report)?;
        self.shift_scope(&scope, at, length, &mut report)?;
        self.edit_lanes(&scope, &project, |items, domain, next| lane_insert(items, at, length, domain, next))?;
        if scope.all {
            self.sections_insert(at, length);
            self.markers_insert(at, length);
        }
        out.label = format!("Insert {} beats at {}{}", beat_text(length), beat_text(at), scope.suffix());
        self.finish(out, report)
    }

    /// `range.delete`: the range removed and the gap closed.
    pub(crate) fn range_delete(&mut self, out: &mut Outcome, start: &BigRational, length: &BigRational, tracks: &[String]) -> Result<()> {
        let scope = self.scope(tracks)?;
        let end = check_range(start, length)?;
        let project = self.validated()?;
        let mut report = Report::default();
        self.split_scope(&scope, start, &mut report)?;
        self.split_scope(&scope, &end, &mut report)?;
        self.remove_scope(&scope, start, &end, &mut report)?;
        self.shift_scope(&scope, &end, &-length.clone(), &mut report)?;
        self.merge_scope(&scope, start, &mut report)?;
        self.edit_lanes(&scope, &project, |items, domain, next| lane_delete(items, start, &end, domain, next))?;
        if scope.all {
            self.sections_delete(start, &end, &mut report)?;
            self.markers_delete(start, &end, &mut report)?;
        }
        out.label = format!("Delete {}{}", beats(start, &end), scope.suffix());
        self.finish(out, report)
    }

    /// `range.clear`: what is in the range removed; nothing moves.
    pub(crate) fn range_clear(&mut self, out: &mut Outcome, start: &BigRational, length: &BigRational, tracks: &[String]) -> Result<()> {
        let scope = self.scope(tracks)?;
        let end = check_range(start, length)?;
        let project = self.validated()?;
        let mut report = Report::default();
        self.split_scope(&scope, start, &mut report)?;
        self.split_scope(&scope, &end, &mut report)?;
        self.remove_scope(&scope, start, &end, &mut report)?;
        self.edit_lanes(&scope, &project, |items, domain, next| lane_clear(items, start, &end, domain, next))?;
        out.label = format!("Clear {}{}", beats(start, &end), scope.suffix());
        self.finish(out, report)
    }

    /// What Copy takes of a range, for `range.paste`: each track's pieces
    /// cut to the range, its place in the song and its lanes over the range,
    /// and the lanes of the groups in it, with every position from the
    /// range's start. A track with nothing in the range is taken too, so a
    /// paste empties its beats where it lands.
    fn taken(&self, start: &BigRational, length: &BigRational, tracks: &[String]) -> Result<serde_json::Value> {
        let scope = self.scope(tracks)?;
        let end = check_range(start, length)?;
        let project = self.validated()?;
        let pieces = self.extract_scope(&scope, start, &end)?;
        let mut rows = Vec::new();
        for track in &scope.tracks {
            let items: Vec<serde_json::Value> = pieces
                .iter()
                .filter(|p| p.track == *track)
                .map(|p| {
                    let mut node = p.node.clone();
                    let at = exact(node.get("at")).unwrap_or_default() - start;
                    set(&mut node, "at", beat_value(&at));
                    json!({"list": p.key, "node": crate::command::node_json(&node)})
                })
                .collect();
            rows.push(json!({"index": index_of(track), "items": items, "lanes": self.lanes_taken(track, &project, start, &end)}));
        }
        let groups: Vec<Loc> = if scope.all {
            (0..self.root.get("groups").map_or(0, |n| n.items().len())).map(|i| vec![Step::Key("groups".into()), Step::Index(i)]).collect()
        } else {
            scope.groups.clone()
        };
        let mut taken_groups = Vec::new();
        for group in &groups {
            let lanes = self.lanes_taken(group, &project, start, &end);
            if lanes.is_empty() {
                continue;
            }
            // The group is found again from the first of its tracks taken,
            // so many groups up from it.
            let id = self.name(group);
            let place = scope.tracks.iter().enumerate().find_map(|(row, t)| {
                let up = self.chain(self.node(t).field("group")).iter().position(|g| *g == id)?;
                Some((row, up + 1))
            });
            if let Some((row, up)) = place {
                taken_groups.push(json!({"track": row, "up": up, "lanes": lanes}));
            }
        }
        Ok(json!({"length": crate::command::to_json(&beat_value(length)), "tracks": rows, "groups": taken_groups}))
    }

    /// An owner's lanes over `[s, e)`, from `s`: the points in the range,
    /// with the value at `s` where no point is there and the value the lane
    /// reaches at `e`, as the lane plays them.
    fn lanes_taken(&self, owner: &[Step], project: &Project, s: &BigRational, e: &BigRational) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        for lane in self.node(owner).get("automation").map_or(&[][..], Node::items) {
            let param = lane.node.field("param").unwrap_or_default().to_string();
            let src = pts(lane.node.get("points").map_or(&[][..], Node::items));
            if src.is_empty() {
                continue;
            }
            let domain = Self::lane_domain(project, owner, &param);
            let at = |x: &BigRational| crate::command::to_json(&beat_value(x));
            let mut points = Vec::new();
            if !src.iter().any(|x| x.at == *s) {
                points.push(json!({"at": at(&BigRational::zero()), "value": right_of(&src, domain, s)}));
            }
            for x in src.iter().filter(|x| x.at >= *s && x.at < *e) {
                let mut p = json!({"at": at(&(&x.at - s)), "value": x.value});
                if x.hold {
                    p["curve"] = json!("hold");
                }
                if x.shape != 0.0 {
                    p["shape"] = json!(x.shape);
                }
                points.push(p);
            }
            points.push(json!({"at": at(&(e - s)), "value": left_of(&src, domain, e)}));
            out.push(json!({"param": param, "points": points}));
        }
        out
    }

    /// `range.paste`: what `range.take` took, over what is at `at`, on the
    /// tracks it came from or as many tracks down as `track` is from the
    /// first of them. The whole length is replaced, beats that held nothing
    /// too. A lane goes where its parameter is: to a lane of the same
    /// parameter, made if the track has none and could have it.
    pub(crate) fn range_paste(&mut self, out: &mut Outcome, copied: &serde_json::Value, at: &BigRational, track: Option<&str>) -> Result<()> {
        let length = crate::command::beat_at(&copied["length"])?;
        let end = check_range(at, &length)?;
        let rows = copied["tracks"].as_array().filter(|r| !r.is_empty()).ok_or("Nothing was copied")?;
        let indices = rows.iter().map(|r| r["index"].as_i64().ok_or_else(|| "A copied track has no place".to_string())).collect::<Result<Vec<_>>>()?;
        let top = indices.iter().copied().min().unwrap_or(0);
        let shift = match track {
            Some(id) => index_of(&self.track(id)?) as i64 - top,
            None => 0,
        };
        let count = self.root.get("tracks").map_or(0, |t| t.items().len()) as i64;
        let mut targets: Vec<Loc> = Vec::new();
        for i in &indices {
            let to = i + shift;
            if to < 0 || to >= count {
                return Err(format!("The copy covers {} tracks, and the song has no track {} below the first", rows.len(), to - shift - top));
            }
            targets.push(vec![Step::Key("tracks".into()), Step::Index(to as usize)]);
        }
        let names = targets.iter().map(|t| self.name(t)).collect();
        let scope = Scope { tracks: targets.clone(), groups: Vec::new(), names, all: false };
        let project = self.validated()?;
        let mut report = Report::default();
        let mut pieces = Vec::new();
        for (row, target) in rows.iter().zip(&targets) {
            for item in row["items"].as_array().map_or(&[][..], Vec::as_slice) {
                let (key, kind, what) = match item["list"].as_str() {
                    Some("clips") => ("clips", Kind::Note, "note clips"),
                    Some("audio") => ("audio", Kind::Audio, "audio clips"),
                    _ => return Err("A copied item is a note clip or an audio clip".into()),
                };
                if self.node(target).get(key).is_none() {
                    return Err(format!("{} takes no {what}: paste on tracks like the ones copied", self.name(target)));
                }
                let node = Node::new(&crate::command::json_value(&item["node"])?);
                pieces.push(Piece { track: target.clone(), key, kind, node });
            }
        }
        self.split_scope(&scope, at, &mut report)?;
        self.split_scope(&scope, &end, &mut report)?;
        self.remove_scope(&scope, at, &end, &mut report)?;
        self.place(pieces, at, &mut report)?;
        for (row, target) in rows.iter().zip(&targets) {
            self.paste_lanes(target, &row["lanes"], at, &length, &project)?;
        }
        for group in copied["groups"].as_array().map_or(&[][..], Vec::as_slice) {
            let (Some(row), Some(up)) = (group["track"].as_u64(), group["up"].as_u64()) else { continue };
            let Some(target) = targets.get(row as usize) else { continue };
            let chain = self.chain(self.node(target).field("group"));
            let Some(id) = (up as usize).checked_sub(1).and_then(|i| chain.get(i)) else { continue };
            let owner = self.at(&format!("groups.{id}"))?;
            self.paste_lanes(&owner, &group["lanes"], at, &length, &project)?;
        }
        out.label = format!("Paste {} beats at {}{}", beat_text(&length), beat_text(at), scope.suffix());
        self.finish(out, report)
    }

    /// Puts lanes `range.take` took on `owner` at `at`, over what is there.
    fn paste_lanes(&mut self, owner: &[Step], lanes: &serde_json::Value, at: &BigRational, length: &BigRational, project: &Project) -> Result<()> {
        for lane in lanes.as_array().map_or(&[][..], Vec::as_slice) {
            let Some(param) = lane["param"].as_str() else { continue };
            let src = Node::new(&crate::command::json_value(&lane["points"])?);
            let src = pts(src.items());
            if src.is_empty() {
                continue;
            }
            let i = match self.lane(owner, param) {
                Some(i) => i,
                None => {
                    if rules::target(Self::owner_of(project, owner), param).is_err() {
                        continue;
                    }
                    let mut map = indexmap::IndexMap::new();
                    map.insert("param".to_string(), Node::Leaf(Value::str(param)));
                    map.insert("points".to_string(), Node::List(Vec::new()));
                    if self.node(owner).get("automation").is_none() {
                        if let Some(m) = self.node_mut(owner).map_mut() {
                            m.insert("automation".into(), Node::List(Vec::new()));
                        }
                    }
                    self.insert(owner, "automation", None, Node::Map(map))?;
                    self.lane(owner, param).ok_or("The lane was not made")?
                }
            };
            let domain = Self::lane_domain(project, owner, param);
            let loc = [owner.to_vec(), vec![Step::Key("automation".into()), Step::Index(i)]].concat();
            let points = std::mem::take(self.list(&loc, "points")?);
            let mut next = *self.next;
            let points = lane_copy(&src, &BigRational::zero(), length, points, at, false, domain, &mut next);
            *self.next = next;
            *self.list(&loc, "points")? = points;
        }
        Ok(())
    }

    /// A section's index, start and length.
    fn section(&self, id: &str) -> Result<(usize, BigRational, BigRational)> {
        let loc = self.at(&format!("sections.{id}"))?;
        let Some(Step::Index(i)) = loc.last() else {
            return Err(format!("{id} is not a section"));
        };
        let at = exact(self.node(&loc).get("at")).unwrap_or_default();
        let len = exact(self.node(&loc).get("length_beats")).unwrap_or_default();
        Ok((*i, at, len))
    }

    /// `section.duplicate`: the section and what is under it again, at `to`
    /// or right after it, pushing what follows later; the copy is named `id`
    /// or after the section.
    pub(crate) fn section_duplicate(&mut self, out: &mut Outcome, section: &str, to: Option<&BigRational>, id: Option<&str>) -> Result<()> {
        let (_, at, len) = self.section(section)?;
        let to = to.cloned().unwrap_or_else(|| &at + &len);
        self.range_copy(out, &at, &len, &to, true, &[], id.map(|id| (section, id)))?;
        let made = out.report.get("sections").and_then(|s| s.as_array()).and_then(|s| s.first()).and_then(|s| s.as_str()).unwrap_or("?").to_string();
        out.label = format!("Duplicate section {section} at {} as {made}", beat_text(&to));
        Ok(())
    }

    /// `section.move` with its content: the section's beats cleared where
    /// they were and put at `at` over what is there, under the label, which
    /// moves with them.
    pub(crate) fn section_move_content(&mut self, out: &mut Outcome, section: &str, to: &BigRational) -> Result<()> {
        let (index, start, len) = self.section(section)?;
        let scope = self.scope(&[])?;
        let end = &start + &len;
        let project = self.validated()?;
        let mut report = Report::default();
        let pieces = self.extract_scope(&scope, &start, &end)?;
        self.split_scope(&scope, &start, &mut report)?;
        self.split_scope(&scope, &end, &mut report)?;
        self.remove_scope(&scope, &start, &end, &mut report)?;
        let dest_end = to + &len;
        self.split_scope(&scope, to, &mut report)?;
        self.split_scope(&scope, &dest_end, &mut report)?;
        self.remove_scope(&scope, to, &dest_end, &mut report)?;
        self.sections_remove_inside(to, &dest_end, Some(index), &mut report)?;
        let by = to - &start;
        self.place(pieces, &by, &mut report)?;
        self.edit_lanes(&scope, &project, |items, domain, next| {
            let src = pts(&items);
            let cleared = lane_clear(items, &start, &end, domain, next);
            lane_copy(&src, &start, &end, cleared, to, false, domain, next)
        })?;
        // The label moves with its beats; its index may have changed.
        let (index, _, _) = self.section(section)?;
        self.set_section(index, to, &dest_end);
        self.markers_move(&start, &end, &by)?;
        out.label = format!("Move section {section} to {} with its content", beat_text(to));
        self.finish(out, report)
    }

    /// `section.remove` with its content: its beats deleted from the song,
    /// which closes the gap and shrinks.
    pub(crate) fn section_remove_content(&mut self, out: &mut Outcome, section: &str) -> Result<()> {
        let (_, at, len) = self.section(section)?;
        self.range_delete(out, &at, &len, &[])?;
        out.label = format!("Remove section {section} with its content");
        Ok(())
    }
}
