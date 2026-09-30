//! The song as an editable tree: the model's full dump with a session handle on
//! every list item, so commands can address clips, effects, events and points,
//! which have no IDs, and keep addressing them while the lists around them change.
//!
//! Paths are dot-separated. A list segment is an index, the item's `id` (sends:
//! its `to`), or a handle such as `@12`; a path may also start with a handle.

use aaw_model::value::{Key, Value};
use indexmap::IndexMap;

#[derive(Clone, Debug)]
pub enum Node {
    Leaf(Value),
    Map(IndexMap<String, Node>),
    List(Vec<Item>),
}

#[derive(Clone, Debug)]
pub struct Item {
    /// 0 until the session assigns one.
    pub handle: u64,
    pub node: Node,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Key(String),
    Index(usize),
}

pub type Loc = Vec<Step>;

impl Node {
    /// A tree without handles.
    pub fn new(v: &Value) -> Node {
        match v {
            Value::Dict(d) => Node::Map(
                d.iter()
                    .map(|(k, v)| (k.as_str().unwrap_or_default().to_string(), Node::new(v)))
                    .collect(),
            ),
            Value::List(items) => Node::List(
                items
                    .iter()
                    .map(|v| Item {
                        handle: 0,
                        node: Node::new(v),
                    })
                    .collect(),
            ),
            other => Node::Leaf(other.clone()),
        }
    }

    pub fn value(&self) -> Value {
        match self {
            Node::Leaf(v) => v.clone(),
            Node::Map(m) => Value::Dict(m.iter().map(|(k, n)| (Key::str(k), n.value())).collect()),
            Node::List(items) => Value::List(items.iter().map(|i| i.node.value()).collect()),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Map(m) => m.get(key),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Node> {
        match self {
            Node::Map(m) => m.get_mut(key),
            _ => None,
        }
    }

    pub fn map_mut(&mut self) -> Option<&mut IndexMap<String, Node>> {
        match self {
            Node::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn items(&self) -> &[Item] {
        match self {
            Node::List(items) => items,
            _ => &[],
        }
    }

    pub fn items_mut(&mut self) -> Option<&mut Vec<Item>> {
        match self {
            Node::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Node::Leaf(Value::Str(s)) => Some(s),
            _ => None,
        }
    }

    /// A string field of a map node.
    pub fn field(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Node::text)
    }

    /// The name a list item goes by in paths: its `id`, or a send's `to`.
    pub fn name(&self) -> Option<&str> {
        self.field("id").or_else(|| self.field("to"))
    }
}

fn attach_into(v: &Value, handles: &mut std::slice::Iter<u64>) -> Node {
    match v {
        Value::Dict(d) => Node::Map(
            d.iter()
                .map(|(k, v)| (k.as_str().unwrap_or_default().to_string(), attach_into(v, handles)))
                .collect(),
        ),
        Value::List(items) => Node::List(
            items
                .iter()
                .map(|v| {
                    let handle = handles.next().copied().unwrap_or(0);
                    Item {
                        handle,
                        node: attach_into(v, handles),
                    }
                })
                .collect(),
        ),
        other => Node::Leaf(other.clone()),
    }
}

/// A dump with handles attached in depth-first order of list items.
pub fn attach(v: &Value, handles: &[u64]) -> Node {
    attach_into(v, &mut handles.iter())
}

/// Every handle in depth-first order of list items.
pub fn handles(n: &Node) -> Vec<u64> {
    fn walk(n: &Node, out: &mut Vec<u64>) {
        match n {
            Node::Leaf(_) => {}
            Node::Map(m) => m.values().for_each(|c| walk(c, out)),
            Node::List(items) => {
                for i in items {
                    out.push(i.handle);
                    walk(&i.node, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(n, &mut out);
    out
}

fn fresh(handle: u64, next: &mut u64) -> u64 {
    if handle != 0 {
        return handle;
    }
    *next += 1;
    *next
}

/// Handles for a validated dump, carried from the tree it was edited from:
/// maps match by key and lists by position, and new items get fresh handles.
pub fn carry(dump: &Value, edited: Option<&Node>, next: &mut u64) -> Node {
    match dump {
        Value::Dict(d) => Node::Map(
            d.iter()
                .map(|(k, v)| {
                    let k = k.as_str().unwrap_or_default();
                    (k.to_string(), carry(v, edited.and_then(|e| e.get(k)), next))
                })
                .collect(),
        ),
        Value::List(items) => {
            let before = edited.map(Node::items).unwrap_or(&[]);
            Node::List(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let old = before.get(i);
                        Item {
                            handle: fresh(old.map_or(0, |o| o.handle), next),
                            node: carry(v, old.map(|o| &o.node), next),
                        }
                    })
                    .collect(),
            )
        }
        other => Node::Leaf(other.clone()),
    }
}

fn identity(v: &Value) -> Option<String> {
    ["id", "to", "param"].iter().find_map(|k| match v.get(k) {
        Some(Value::Str(s)) => Some(format!("{k}={s}")),
        _ => None,
    })
}

/// Handles for a replaced document, such as an external edit: list items keep
/// the handle of the old item with the same ID, else of an equal old item, else
/// of the old item at the same place among those left when as many are left.
pub fn matched(dump: &Value, old: Option<&Node>, next: &mut u64) -> Node {
    match dump {
        Value::Dict(d) => Node::Map(
            d.iter()
                .map(|(k, v)| {
                    let k = k.as_str().unwrap_or_default();
                    (k.to_string(), matched(v, old.and_then(|o| o.get(k)), next))
                })
                .collect(),
        ),
        Value::List(items) => {
            let before = old.map(Node::items).unwrap_or(&[]);
            let values: Vec<Value> = before.iter().map(|i| i.node.value()).collect();
            let mut used = vec![false; before.len()];
            let mut pick: Vec<Option<usize>> = vec![None; items.len()];
            for (i, v) in items.iter().enumerate() {
                if let Some(id) = identity(v) {
                    pick[i] = (0..before.len()).find(|&j| !used[j] && identity(&values[j]).as_ref() == Some(&id));
                    pick[i].inspect(|&j| used[j] = true);
                }
            }
            for (i, v) in items.iter().enumerate() {
                if pick[i].is_none() {
                    pick[i] = (0..before.len()).find(|&j| !used[j] && aaw_model::value::py_eq(v, &values[j]));
                    pick[i].inspect(|&j| used[j] = true);
                }
            }
            let left: Vec<usize> = (0..before.len()).filter(|&j| !used[j]).collect();
            let unpicked: Vec<usize> = (0..items.len()).filter(|&i| pick[i].is_none()).collect();
            if left.len() == unpicked.len() {
                for (i, j) in unpicked.into_iter().zip(left) {
                    pick[i] = Some(j);
                }
            }
            Node::List(
                items
                    .iter()
                    .zip(pick)
                    .map(|(v, j)| {
                        let o = j.map(|j| &before[j]);
                        Item {
                            handle: fresh(o.map_or(0, |o| o.handle), next),
                            node: matched(v, o.map(|o| &o.node), next),
                        }
                    })
                    .collect(),
            )
        }
        other => Node::Leaf(other.clone()),
    }
}

/// Where a handle is, if anywhere.
pub fn find(root: &Node, handle: u64) -> Option<Loc> {
    fn walk(n: &Node, handle: u64, loc: &mut Loc) -> bool {
        match n {
            Node::Leaf(_) => false,
            Node::Map(m) => m.iter().any(|(k, c)| {
                loc.push(Step::Key(k.clone()));
                let found = walk(c, handle, loc);
                if !found {
                    loc.pop();
                }
                found
            }),
            Node::List(items) => items.iter().enumerate().any(|(i, item)| {
                loc.push(Step::Index(i));
                let found = item.handle == handle || walk(&item.node, handle, loc);
                if !found {
                    loc.pop();
                }
                found
            }),
        }
    }
    let mut loc = Vec::new();
    walk(root, handle, &mut loc).then_some(loc)
}

pub fn get<'a>(root: &'a Node, loc: &[Step]) -> &'a Node {
    loc.iter().fold(root, |n, s| match (n, s) {
        (Node::Map(m), Step::Key(k)) => &m[k.as_str()],
        (Node::List(items), Step::Index(i)) => &items[*i].node,
        _ => panic!("location does not match the tree"),
    })
}

pub fn get_mut<'a>(root: &'a mut Node, loc: &[Step]) -> &'a mut Node {
    loc.iter().fold(root, |n, s| match (n, s) {
        (Node::Map(m), Step::Key(k)) => m.get_mut(k.as_str()).expect("key"),
        (Node::List(items), Step::Index(i)) => &mut items[*i].node,
        _ => panic!("location does not match the tree"),
    })
}

/// The handle of the list item at `loc`.
pub fn handle_at(root: &Node, loc: &[Step]) -> u64 {
    match loc.split_last() {
        Some((Step::Index(i), parent)) => get(root, parent).items()[*i].handle,
        _ => 0,
    }
}

/// A readable path: list items by name where they have one, else by index.
pub fn path_text(root: &Node, loc: &[Step]) -> String {
    let mut parts = Vec::new();
    let mut n = root;
    for s in loc {
        match (n, s) {
            (Node::Map(m), Step::Key(k)) => {
                parts.push(k.clone());
                n = &m[k.as_str()];
            }
            (Node::List(items), Step::Index(i)) => {
                let item = &items[*i].node;
                parts.push(item.name().map_or_else(|| i.to_string(), str::to_string));
                n = item;
            }
            _ => break,
        }
    }
    parts.join(".")
}

pub fn handle_text(handle: u64) -> String {
    format!("@{handle}")
}

fn parse_handle(segment: &str) -> Option<u64> {
    segment.strip_prefix('@')?.parse().ok()
}

/// Resolves a path to a location. With `handles` false, `@N` is refused.
pub fn resolve(root: &Node, path: &str, handles: bool) -> Result<Loc, String> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    let mut segments = path.split('.').peekable();
    let mut loc = Vec::new();
    if let Some(h) = segments.peek().and_then(|s| parse_handle(s)) {
        if !handles {
            return Err(no_handles());
        }
        loc = find(root, h).ok_or_else(|| format!("No object has handle @{h}"))?;
        segments.next();
    }
    let mut node = get(root, &loc);
    let mut done: Vec<&str> = Vec::new();
    for seg in segments {
        let here = if done.is_empty() { "the song".to_string() } else { done.join(".") };
        let step = match node {
            Node::Map(m) => {
                if !m.contains_key(seg) {
                    return Err(format!("{here} has no {seg}"));
                }
                Step::Key(seg.to_string())
            }
            Node::List(items) => Step::Index(item(items, seg, handles).ok_or_else(|| format!("{here} has no {seg}"))??),
            Node::Leaf(_) => return Err(format!("{here} is a value, not an object")),
        };
        node = get(node, std::slice::from_ref(&step));
        loc.push(step);
        done.push(seg);
    }
    Ok(loc)
}

fn no_handles() -> String {
    "Handles such as @12 exist only while a host runs; address the object by index".into()
}

/// A list item by handle, index or name.
fn item(items: &[Item], seg: &str, handles: bool) -> Option<Result<usize, String>> {
    if let Some(h) = parse_handle(seg) {
        if !handles {
            return Some(Err(no_handles()));
        }
        return items.iter().position(|i| i.handle == h).map(Ok);
    }
    if !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_digit()) {
        return seg.parse::<usize>().ok().filter(|i| *i < items.len()).map(Ok);
    }
    items.iter().position(|i| i.node.name() == Some(seg)).map(Ok)
}
