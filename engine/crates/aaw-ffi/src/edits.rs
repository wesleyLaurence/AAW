//! The edits a person makes in the app, as the host's commands. The app names
//! objects by the keys the arrangement gave it, which are their handles, and
//! positions by how far things moved; the exact beats are worked out here from
//! the song, so a clip at a third of a beat stays on its third when it moves.

use crate::files::Files;
use crate::view::{seconds_per_beat, tail_beats, FieldValue};
use aaw_host::command::{beat_value, ident, node_json, unique};
use aaw_host::session::{value_json, Doc};
use aaw_host::tree::{self, handle_text, Item, Node, Step};
use aaw_model::describe::{self, Initial};
use aaw_model::rules::{midi, note_name, target, Owner, TargetKind};
use aaw_model::{AudioClip, Beat, Effect, Project, Sample};
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};
use serde_json::{json, Value as Json};
use std::path::Path;

/// A row of the arrangement.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Row {
    Track { key: u64 },
    Return { key: u64 },
    Master,
}

/// A note as Copy took it: its place and length as the song wrote them.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct NoteCopy {
    pub pitch: i32,
    pub at: String,
    pub duration: String,
    pub velocity: u32,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Edit {
    /// The session tempo in quarter notes per minute.
    Tempo { bpm: f64 },
    /// Volume in dB: of a track, a return or, for the master, the song.
    Gain { row: Row, db: f64 },
    Pan { row: Row, pan: f64 },
    Mute { row: Row, on: bool },
    Solo { track: u64, on: bool },
    /// A send's level in dB; the send is added if the track has none there.
    Send { track: u64, to: String, db: f64 },
    SendRemove { track: u64, to: String },
    /// Moves clips later by `by` beats and down by `rows` tracks: pattern
    /// clips and audio clips alike. The song grows to hold an audio clip
    /// that would end past its end, here and in the edits below.
    ClipsMove { clips: Vec<u64>, by: f64, rows: i32 },
    ClipRepeats { clip: u64, repeats: u32 },
    /// Copies clips to right after the span they cover together, so that a
    /// group repeats as a group. The copies are what the edit makes.
    ClipsDuplicate { clips: Vec<u64> },
    ClipsRemove { clips: Vec<u64> },
    /// Adds the clips `copied` took, so that the earliest starts at `at`, on
    /// `track` or else on the track it was copied from, and the others as far
    /// after it and below it as they were. The copies are what the edit makes.
    ClipsPaste { copied: String, at: f64, track: Option<u64> },
    /// Moves a note clip's start, its end or both to a beat. Its notes stay
    /// where they are in the song: those the start passes are kept, before
    /// the clip, and do not play.
    ClipTrim {
        clip: u64,
        start: Option<f64>,
        end: Option<f64>,
    },
    /// Sets a note clip's length, as typed; its notes stay where they are.
    ClipLength { clip: u64, beats: String },
    /// Adds a note to a note clip at a beat of the clip, one step of `grid`
    /// long: on the step under the beat, exactly, unless `free`. The note is
    /// what the edit makes.
    NoteAdd {
        clip: u64,
        at: f64,
        free: bool,
        grid: String,
        pitch: i32,
    },
    /// Moves notes later by `steps` steps of `grid` and `by` beats, and up by
    /// `semitones`.
    NotesMove {
        notes: Vec<u64>,
        steps: i32,
        grid: String,
        by: f64,
        semitones: i32,
    },
    /// Ends `grabbed` at a beat of its clip, on the nearest line of `grid`
    /// unless `free`, and the other notes as much later or earlier.
    NotesEnd {
        notes: Vec<u64>,
        grabbed: u64,
        end: f64,
        free: bool,
        grid: String,
    },
    /// Sets fields of notes, as typed: a pitch as a number or a name such as
    /// `C4`, beats such as `1/3` or `1.975`.
    NotesSet {
        notes: Vec<u64>,
        pitch: Option<String>,
        at: Option<String>,
        duration: Option<String>,
        velocity: Option<u32>,
    },
    NotesRemove { notes: Vec<u64> },
    /// Copies notes to right after the span they cover together, in their
    /// clip. The copies are what the edit makes.
    NotesDuplicate { notes: Vec<u64> },
    /// Adds copied notes to a note clip, the earliest at `at`, a beat of that
    /// clip, and the others as far after it as they were; without `at`, right
    /// after the span they covered. The copies are what the edit makes.
    NotesPaste { notes: Vec<NoteCopy>, clip: u64, at: Option<f64> },
    /// Takes a MIDI track's instrument off; its notes are kept.
    InstrumentRemove { track: u64 },
    /// Moves an audio clip's start, its end or both to a beat. Its audio
    /// stays where it is on the timeline, and an edge goes no further than
    /// the file does. The end is where its sound ends: the clip leaves its
    /// fade out before that, and a fade out that would not fit is shortened.
    AudioTrim {
        clip: u64,
        start: Option<f64>,
        end: Option<f64>,
    },
    /// Sets an audio clip's fades, in milliseconds. Its sound still ends
    /// where it did, so a longer fade out starts earlier.
    AudioFade {
        clip: u64,
        fade_in_ms: Option<f64>,
        fade_out_ms: Option<f64>,
    },
    /// Makes two of each audio clip among `clips` that plays at a beat. The
    /// later halves are what the edit makes.
    AudioSplit { clips: Vec<u64>, at: f64 },
    /// Sets a field of an audio clip: `gain_db`, `fade_curve`, `source_bpm`
    /// or `stretch`. An absent `source_bpm` has the clip play at its file's
    /// own tempo.
    AudioSet { clip: u64, field: String, value: FieldValue },
    /// Adds an empty track at `index` among the tracks, under a free name: a
    /// MIDI track of note clips with no instrument when `midi`.
    TrackAdd { index: u32, midi: bool },
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
    /// Adds a clip of a new, empty pattern one bar long to a track, or to a
    /// MIDI track an empty note clip one bar long. The clip is what the edit
    /// makes.
    ClipNew { track: u64, at: f64 },
    /// Gives a clip a copy of its pattern, so that editing it leaves the
    /// other clips that play the pattern as they are.
    ClipOwnPattern { clip: u64 },
    /// Adds a sample that `library::import` copied into the project, as a
    /// pad of a track or, without one, of a new track at `index`. Pad and
    /// track are named after `name` as far as IDs allow. A new track is what
    /// the edit makes. On a MIDI track the sample becomes the instrument, in
    /// place of the one it had: a sampler that plays it on every note, from
    /// its root note when it has one, else as it is.
    SampleAdd {
        asset: crate::library::Asset,
        name: String,
        track: Option<u64>,
        index: u32,
    },
    /// Adds a sample that `library::import` copied into the project as an
    /// audio clip at a beat: on a track or, without one, on a new track at
    /// `index` named after `name`. The clip is the last of what the edit
    /// makes, after a new track.
    SampleClip {
        asset: crate::library::Asset,
        name: String,
        track: Option<u64>,
        index: u32,
        at: f64,
    },
    /// Makes a note clip of a MIDI file's notes at a beat: on a MIDI track
    /// or, without one, on a new MIDI track at `index` named after the
    /// file. The file is read where it is; its tempo is not taken. The clip
    /// is the last of what the edit makes, after a new track.
    MidiClip { path: String, track: Option<u64>, index: u32, at: f64 },
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

/// A clip in the song, a pattern clip or an audio clip: its track's place
/// among the tracks and its exact span. An audio clip's span is to where its
/// sound ends, after its fade out.
struct Placed<'a> {
    track: usize,
    start: BigRational,
    end: BigRational,
    /// The clip, when it is an audio clip.
    audio: Option<&'a AudioClip>,
    /// Whether it is a note clip.
    notes: bool,
}

/// A length in beats that was worked out from seconds of a file, exactly: to a
/// millionth of a beat, so that a beat written from it is a short decimal and
/// not the fraction a float's last bits make.
fn span(beats: f64) -> BigRational {
    aaw_model::beat(&Beat::Float((beats.max(0.0) * 1e6).round() / 1e6)).unwrap_or_default()
}

/// The beats a fade of so many milliseconds lasts.
fn fade_beats(ms: f64, tempo: f64) -> f64 {
    ms / 1000.0 * tempo / 60.0
}

/// The song an edit is worked out from, with the files its audio clips play.
struct Song<'a> {
    project: &'a Project,
    files: &'a Files,
    directory: &'a Path,
}

impl<'a> Song<'a> {
    /// The length of a sample's file in seconds, when it can be read.
    fn seconds(&self, sample: &str) -> Option<f64> {
        self.files.of(self.directory, self.project.samples.get(sample)?).map(|f| f.seconds)
    }

    /// The seconds of its file an audio clip plays to: its own end, or the
    /// file's.
    fn source_end(&self, clip: &AudioClip) -> Result<f64> {
        match (clip.source_end_seconds, self.seconds(&clip.sample)) {
            (Some(end), Some(file)) => Ok(end.min(file)),
            (Some(end), None) => Ok(end),
            (None, Some(file)) => Ok(file),
            (None, None) => Err(format!("The file of sample {} cannot be read", clip.sample)),
        }
    }

    /// The beat a second of an audio clip's file plays on.
    fn beat_of(&self, clip: &AudioClip, seconds: f64) -> f64 {
        let at = clip.at_exact().to_f64().unwrap_or(0.0);
        at + (seconds - clip.source_start_seconds) / seconds_per_beat(clip, self.project.session.tempo)
    }

    fn placed(&self, tree: &Node, key: u64) -> Result<Placed<'a>> {
        let project = self.project;
        for (i, track) in items(tree, "tracks").iter().enumerate() {
            if let Some(j) = items(&track.node, "clips").iter().position(|c| c.handle == key) {
                if let Some(midi) = &project.tracks[i].midi {
                    let clip = midi.clips.get(j).ok_or("The clip is no longer in the song")?;
                    let start = clip.at_exact();
                    let end = &start + clip.length_exact();
                    return Ok(Placed { track: i, start, end, audio: None, notes: true });
                }
                let clip = &project.tracks[i].clips[j];
                let length = project.patterns.get(&clip.pattern).map(|p| p.length_exact()).unwrap_or_else(BigRational::zero);
                let start = clip.at_exact();
                let end = &start + length * BigRational::from_integer(clip.repeats.into());
                return Ok(Placed { track: i, start, end, audio: None, notes: false });
            }
            if let Some(j) = items(&track.node, "audio").iter().position(|c| c.handle == key) {
                let clip = &project.tracks[i].audio[j];
                let start = clip.at_exact();
                let tempo = project.session.tempo;
                let beats = (self.source_end(clip)? - clip.source_start_seconds) / seconds_per_beat(clip, tempo)
                    + tail_beats(clip, tempo, self.seconds(&clip.sample));
                let end = &start + span(beats);
                return Ok(Placed { track: i, start, end, audio: Some(clip), notes: false });
            }
        }
        Err("The clip is no longer in the song".into())
    }

    /// The command that lengthens the song to the end of the bar an audio
    /// clip's sound ends in, when that is past the song's end.
    fn lengthen(&self, end: &BigRational) -> Option<Json> {
        // What runs past the end by less than the song's end fade is taken by
        // that fade, as a short fade out after a clip that ends on the song's
        // end is. A file's length in beats is seldom a round number, too.
        let session = &self.project.session;
        let slack = fade_beats(session.end_fade_ms, session.tempo).max(1e-6);
        let end = end - BigRational::from_float(slack).unwrap_or_default();
        if end <= self.project.session.length_exact() {
            return None;
        }
        let bar = BigRational::from_integer(4.into());
        let length = (end / &bar).ceil() * bar;
        Some(json!({"op": "set", "path": "session.length_beats", "value": beat(&length)}))
    }

    /// `commands`, after the song is made long enough for an audio clip that
    /// ends at `end`: then they are one step, named `label`.
    fn grown(&self, mut commands: Vec<Json>, end: Option<&BigRational>, label: impl FnOnce() -> String) -> Vec<Json> {
        match end.and_then(|end| self.lengthen(end)) {
            Some(longer) => {
                commands.insert(0, longer);
                batch(commands, label())
            }
            None => commands,
        }
    }
}

impl Song<'_> {
    /// The commands that copy clips `by` beats later and `rows` tracks down:
    /// a pattern or note clip as the host duplicates it, an audio clip as the
    /// clip again on another beat. The song grows to hold an audio clip's.
    fn copies(&self, tree: &Node, clips: &[u64], spans: &[Placed], by: &BigRational, rows: i64, verb: &str) -> Result<Vec<Json>> {
        let tracks = items(tree, "tracks");
        let mut furthest = None;
        let mut sample = "";
        let commands = clips
            .iter()
            .zip(spans)
            .map(|(key, s)| {
                let at = beat(&(&s.start + by));
                let to = usize::try_from(s.track as i64 + rows).ok().and_then(|i| tracks.get(i));
                let to = to.ok_or("There is no track to copy the clip to")?;
                let Some(audio) = s.audio else {
                    let mut command = json!({"op": "clip.duplicate", "clip": handle_text(*key), "at": at});
                    if rows != 0 {
                        command["track"] = json!(handle_text(to.handle));
                    }
                    return Ok(command);
                };
                // A copy of an audio clip is the clip again on another beat.
                let loc = tree::find(tree, *key).ok_or("The clip is no longer in the song")?;
                let mut copy = node_json(tree::get(tree, &loc));
                let fields = copy.as_object_mut().ok_or("An audio clip is an object")?;
                fields.retain(|_, value| !value.is_null());
                fields.insert("op".into(), json!("audio.add"));
                fields.insert("track".into(), json!(handle_text(to.handle)));
                fields.insert("at".into(), at);
                sample = &audio.sample;
                furthest = later(furthest.take(), &s.end + by);
                Ok(copy)
            })
            .collect::<Result<Vec<_>>>()?;
        let label = || match clips.len() {
            1 => format!("{verb} audio clip {sample}"),
            n => format!("{verb} {n} clips"),
        };
        let commands = self.grown(commands, furthest.as_ref(), label);
        // One audio clip's copy is an `audio.add` to the host, which this names.
        Ok(match commands.as_slice() {
            [only] if only["op"] == "audio.add" => batch(commands, label()),
            _ => commands,
        })
    }
}

/// The later of two ends.
fn later(a: Option<BigRational>, b: BigRational) -> Option<BigRational> {
    Some(match a {
        Some(a) if a > b => a,
        _ => b,
    })
}

/// The commands that list a copied file among the song's samples, unless it
/// is there already, and the sample's ID.
fn listed(project: &Project, asset: &crate::library::Asset, stem: &str) -> (String, Vec<Json>) {
    // A file that is in the song already is the same sample.
    if let Some((id, _)) = project.samples.iter().find(|(_, s)| s.path == asset.path) {
        return (id.clone(), Vec::new());
    }
    let id = unique(stem, |n| project.samples.contains_key(n));
    let mut entry = json!({"path": asset.path, "sha256": asset.sha256, "source": asset.source});
    if let Some(sha) = &asset.source_sha256 {
        entry["source_sha256"] = json!(sha);
    }
    if let Some(root) = &asset.root_note {
        entry["root_note"] = json!(root);
    }
    let command = json!({"op": "set", "path": format!("samples.{id}"), "value": entry});
    (id, vec![command])
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

/// Whether a track is a MIDI track.
fn is_midi(project: &Project, tree: &Node, key: u64) -> bool {
    let place = items(tree, "tracks").iter().position(|i| i.handle == key);
    place.and_then(|i| project.tracks.get(i)).is_some_and(|t| t.midi.is_some())
}

/// A note clip in the song, with its key.
fn note_clip<'a>(project: &'a Project, tree: &Node, key: u64) -> Result<(u64, &'a aaw_model::NoteClip)> {
    let gone = || "The clip is no longer in the song".to_string();
    match tree::find(tree, key).ok_or_else(gone)?.as_slice() {
        [Step::Key(k), Step::Index(t), Step::Key(l), Step::Index(c)] if k == "tracks" && l == "clips" => {
            let midi = project.tracks.get(*t).and_then(|t| t.midi.as_ref()).ok_or("That is not a note clip")?;
            Ok((key, midi.clips.get(*c).ok_or_else(gone)?))
        }
        _ => Err("That is not a note clip".into()),
    }
}

/// A note in the song, with its clip's key.
fn note<'a>(project: &'a Project, tree: &Node, key: u64) -> Result<(u64, &'a aaw_model::Note)> {
    let gone = || "The note is no longer in the song".to_string();
    let loc = tree::find(tree, key).ok_or_else(gone)?;
    match loc.as_slice() {
        [Step::Key(k), Step::Index(t), Step::Key(l), Step::Index(c), Step::Key(m), Step::Index(n)]
            if k == "tracks" && l == "clips" && m == "notes" =>
        {
            let clip = project.tracks.get(*t).and_then(|t| t.midi.as_ref()).and_then(|m| m.clips.get(*c)).ok_or_else(gone)?;
            let clip_key = tree::handle_at(tree, &loc[..4]);
            Ok((clip_key, clip.notes.get(*n).ok_or_else(gone)?))
        }
        _ => Err("That is not a note".into()),
    }
}

/// The exact length of a grid's step, as typed: `1/4`, `1/3`.
fn grid_exact(grid: &str) -> Result<BigRational> {
    let grid = typed_exact(grid)?;
    if grid <= BigRational::zero() {
        return Err("A step has a length".into());
    }
    Ok(grid)
}

/// A beat that may be negative, as a command writes a distance.
fn signed_beat(x: &BigRational) -> Json {
    value_json(&beat_value(x))
}

/// "1 note", "3 notes".
fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

fn batch(commands: Vec<Json>, label: String) -> Vec<Json> {
    vec![json!({"op": "batch", "commands": commands, "label": label})]
}

/// The commands that make `edit` on the song in `doc`, which is in
/// `directory`: none when the edit changes nothing, and several for an edit
/// of several objects.
pub fn commands(doc: &Doc, edit: &Edit, files: &Files, directory: &Path) -> Result<Vec<Json>> {
    let project = &doc.project;
    let song = Song { project, files, directory };
    let placed = |tree: &Node, key: u64| song.placed(tree, key);
    let clip = |key: &u64| handle_text(*key);
    match edit {
        Edit::Tempo { bpm } => Ok(vec![json!({"op": "set", "path": "session.tempo", "value": number(*bpm)})]),
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
            let mut furthest = None;
            let mut sample = "";
            let commands = clips
                .iter()
                .map(|key| {
                    let at = placed(&tree, *key)?;
                    let op = if at.audio.is_some() { "audio.move" } else { "clip.move" };
                    let mut command = json!({"op": op, "clip": clip(key)});
                    if *by != 0.0 {
                        command["at"] = beat(&(&at.start + &by_exact));
                    }
                    if *rows != 0 {
                        let to = usize::try_from(at.track as i64 + *rows as i64).ok().and_then(|i| tracks.get(i));
                        let to = to.ok_or("There is no track to move the clip to")?;
                        command["track"] = json!(handle_text(to.handle));
                    }
                    if let Some(audio) = at.audio {
                        sample = &audio.sample;
                        furthest = later(furthest.take(), at.end + &by_exact);
                    }
                    Ok(command)
                })
                .collect::<Result<Vec<_>>>()?;
            let label = || match clips.len() {
                1 => format!("Move audio clip {sample}"),
                n => format!("Move {n} clips"),
            };
            Ok(song.grown(commands, furthest.as_ref(), label))
        }
        Edit::ClipRepeats { clip: key, repeats } => Ok(vec![json!({"op": "clip.repeats", "clip": clip(key), "repeats": repeats})]),
        Edit::ClipsDuplicate { clips } => {
            let tree = doc.tree();
            let spans = clips.iter().map(|key| placed(&tree, *key)).collect::<Result<Vec<_>>>()?;
            let (Some(start), Some(end)) = (spans.iter().map(|s| &s.start).min(), spans.iter().map(|s| &s.end).max()) else {
                return Ok(Vec::new());
            };
            let span = end - start;
            song.copies(&tree, clips, &spans, &span, 0, "Duplicate")
        }
        Edit::ClipsPaste { copied, at, track } => {
            let tree = doc.tree();
            let copied: Json = serde_json::from_str(copied).map_err(|e| e.to_string())?;
            let clips = copied.as_array().ok_or("Nothing was copied")?;
            let start_of = |c: &Json| typed_exact(c["start"].as_str().unwrap_or_default());
            let starts = clips.iter().map(start_of).collect::<Result<Vec<_>>>()?;
            let (Some(first), Some(top)) = (starts.iter().min(), clips.iter().filter_map(|c| c["track"].as_i64()).min()) else {
                return Ok(Vec::new());
            };
            let shift = aaw_model::beat(&Beat::Float(at.max(0.0)))? - first;
            let tracks = items(&tree, "tracks");
            let rows = match track {
                Some(key) => tracks.iter().position(|i| i.handle == *key).ok_or("The track is no longer in the song")? as i64 - top,
                None => 0,
            };
            let mut furthest = None;
            let mut commands = Vec::new();
            for (c, start) in clips.iter().zip(&starts) {
                let to = usize::try_from(c["track"].as_i64().unwrap_or(0) + rows).ok().and_then(|i| tracks.get(i));
                let to = to.ok_or("There is no track to paste the clip on")?;
                let mut command = c["fields"].clone();
                let fields = command.as_object_mut().ok_or("A copied clip is an object")?;
                let kind = c["kind"].as_str().unwrap_or_default();
                fields.insert("op".into(), json!(if kind == "audio" { "audio.add" } else { "clip.add" }));
                fields.insert("track".into(), json!(handle_text(to.handle)));
                fields.insert("at".into(), beat(&(start + &shift)));
                if kind == "audio" {
                    furthest = later(furthest.take(), start + &shift + typed_exact(c["length"].as_str().unwrap_or("0"))?);
                }
                commands.push(command);
            }
            let label = match commands.len() {
                1 => "Paste a clip".to_string(),
                n => format!("Paste {n} clips"),
            };
            if let Some(longer) = furthest.as_ref().and_then(|end| song.lengthen(end)) {
                commands.insert(0, longer);
            }
            Ok(batch(commands, label))
        }
        Edit::ClipTrim { clip: key, start, end } => {
            if start.is_none() && end.is_none() {
                return Ok(Vec::new());
            }
            let tree = doc.tree();
            if !placed(&tree, *key)?.notes {
                return Err("That is not a note clip".into());
            }
            let mut command = json!({"op": "clip.trim", "clip": clip(key)});
            if let Some(start) = start {
                command["start"] = beat_at(*start)?;
            }
            if let Some(end) = end {
                command["end"] = beat_at(*end)?;
            }
            Ok(vec![command])
        }
        Edit::ClipLength { clip: key, beats } => {
            let tree = doc.tree();
            let (_, c) = note_clip(project, &tree, *key)?;
            if typed_exact(beats)? == c.length_exact() {
                return Ok(Vec::new());
            }
            Ok(vec![json!({"op": "clip.resize", "clip": clip(key), "length_beats": typed_beat(beats)})])
        }
        Edit::NoteAdd { clip: key, at, free, grid, pitch } => {
            let tree = doc.tree();
            let (_, c) = note_clip(project, &tree, *key)?;
            let grid = grid_exact(grid)?;
            let at = if *free {
                aaw_model::signed_beat(&Beat::Float(*at))?
            } else {
                let step = (at / grid.to_f64().unwrap_or(1.0) + 1e-9).floor();
                BigRational::from_float(step).unwrap_or_default() * &grid
            };
            if at.is_negative() || at >= c.length_exact() {
                return Err("A note is added inside its clip".into());
            }
            Ok(vec![json!({
                "op": "note.add", "clip": clip(key), "pitch": pitch, "at": beat(&at), "duration": beat(&grid),
            })])
        }
        Edit::NotesMove { notes, steps, grid, by, semitones } => {
            if notes.is_empty() || (*steps == 0 && *by == 0.0 && *semitones == 0) {
                return Ok(Vec::new());
            }
            let paths: Vec<String> = notes.iter().map(|k| handle_text(*k)).collect();
            let mut commands = Vec::new();
            if *steps != 0 || *by != 0.0 {
                let distance = aaw_model::beat(&Beat::Float(by.abs()))?;
                let by = if *by < 0.0 { -distance } else { distance };
                let shift = grid_exact(grid)? * BigRational::from_integer((*steps).into()) + by;
                if !shift.is_zero() {
                    commands.push(json!({"op": "note.move", "notes": paths, "by": signed_beat(&shift)}));
                }
            }
            if *semitones != 0 {
                commands.push(json!({"op": "note.transpose", "notes": paths, "by": semitones}));
            }
            Ok(match commands.len() {
                2 => batch(commands, format!("Move {}", count(notes.len(), "note"))),
                _ => commands,
            })
        }
        Edit::NotesEnd { notes, grabbed, end, free, grid } => {
            let tree = doc.tree();
            let (_, n) = note(project, &tree, *grabbed)?;
            let grid = grid_exact(grid)?;
            let end = if *free {
                aaw_model::signed_beat(&Beat::Float((end * 1000.0).round() / 1000.0))?
            } else {
                let line = (end / grid.to_f64().unwrap_or(1.0)).round();
                BigRational::from_float(line).unwrap_or_default() * &grid
            };
            let by = end - (n.at_exact() + n.duration_exact());
            if by.is_zero() {
                return Ok(Vec::new());
            }
            let mut commands = Vec::new();
            for key in notes.iter().chain(std::iter::once(grabbed)).fold(Vec::new(), |mut seen, k| {
                if !seen.contains(k) {
                    seen.push(*k);
                }
                seen
            }) {
                let (_, n) = note(project, &tree, key)?;
                let duration = n.duration_exact() + &by;
                if !duration.is_positive() {
                    return Err(format!("Note {} would end before it starts", n.id));
                }
                commands.push(json!({"op": "note.set", "note": handle_text(key), "duration": beat(&duration)}));
            }
            Ok(match commands.len() {
                1 => commands,
                n => batch(commands, format!("Lengthen {n} notes")),
            })
        }
        Edit::NotesSet { notes, pitch, at, duration, velocity } => {
            let mut fields = serde_json::Map::new();
            if let Some(pitch) = pitch {
                let pitch = pitch.trim();
                fields.insert("pitch".into(), pitch.parse::<i64>().map_or_else(|_| json!(pitch), |n| json!(n)));
            }
            if let Some(at) = at {
                fields.insert("at".into(), typed_beat(at));
            }
            if let Some(duration) = duration {
                fields.insert("duration".into(), typed_beat(duration));
            }
            if let Some(velocity) = velocity {
                fields.insert("velocity".into(), json!(velocity));
            }
            if fields.is_empty() {
                return Ok(Vec::new());
            }
            let commands: Vec<Json> = notes
                .iter()
                .map(|key| {
                    let mut command = json!({"op": "note.set", "note": handle_text(*key)});
                    command.as_object_mut().expect("an object").extend(fields.clone());
                    command
                })
                .collect();
            Ok(match commands.len() {
                0 | 1 => commands,
                n => batch(commands, format!("Change {n} notes")),
            })
        }
        Edit::NotesRemove { notes } => match notes.is_empty() {
            true => Ok(Vec::new()),
            false => Ok(vec![json!({"op": "note.remove", "notes": notes.iter().map(|k| handle_text(*k)).collect::<Vec<_>>()})]),
        },
        Edit::NotesDuplicate { notes } => {
            let tree = doc.tree();
            let found = notes.iter().map(|key| note(project, &tree, *key)).collect::<Result<Vec<_>>>()?;
            let Some((clip_key, _)) = found.first() else { return Ok(Vec::new()) };
            if found.iter().any(|(c, _)| c != clip_key) {
                return Err("Notes are copied within one clip".into());
            }
            let (Some(start), Some(end)) = (
                found.iter().map(|(_, n)| n.at_exact()).min(),
                found.iter().map(|(_, n)| n.at_exact() + n.duration_exact()).max(),
            ) else {
                return Ok(Vec::new());
            };
            let span = end - start;
            let copies: Vec<Json> = found
                .iter()
                .map(|(_, n)| {
                    json!({
                        "pitch": n.pitch, "at": signed_beat(&(n.at_exact() + &span)),
                        "duration": beat(&n.duration_exact()), "velocity": n.velocity,
                    })
                })
                .collect();
            Ok(vec![json!({"op": "note.add", "clip": handle_text(*clip_key), "notes": copies})])
        }
        Edit::NotesPaste { notes, clip: key, at } => {
            let tree = doc.tree();
            note_clip(project, &tree, *key)?;
            let places = notes
                .iter()
                .map(|n| Ok((aaw_model::signed_beat(&Beat::Str(n.at.clone()))?, typed_exact(&n.duration)?)))
                .collect::<Result<Vec<_>>>()?;
            let (Some(start), Some(end)) = (places.iter().map(|p| &p.0).min(), places.iter().map(|p| &p.0 + &p.1).max()) else {
                return Ok(Vec::new());
            };
            let shift = match at {
                Some(at) => aaw_model::signed_beat(&Beat::Float((at * 1000.0).round() / 1000.0))? - start,
                None => end - start,
            };
            let copies: Vec<Json> = notes
                .iter()
                .zip(&places)
                .map(|(n, (at, duration))| {
                    json!({"pitch": n.pitch, "at": signed_beat(&(at + &shift)), "duration": beat(duration), "velocity": n.velocity})
                })
                .collect();
            Ok(vec![json!({"op": "note.add", "clip": clip(key), "notes": copies})])
        }
        Edit::InstrumentRemove { track } => Ok(vec![json!({"op": "instrument.set", "track": handle_text(*track), "instrument": null})]),
        Edit::ClipsRemove { clips } => {
            let tree = doc.tree();
            let commands: Vec<Json> = clips
                .iter()
                .map(|key| match placed(&tree, *key).ok().and_then(|at| at.audio) {
                    // A removal by path is named by the path, so an audio clip's says what it is.
                    Some(audio) if clips.len() == 1 => batch(
                        vec![json!({"op": "remove", "path": clip(key)})],
                        format!("Remove audio clip {}", audio.sample),
                    )
                    .remove(0),
                    Some(_) => json!({"op": "remove", "path": clip(key)}),
                    None => json!({"op": "clip.remove", "clip": clip(key)}),
                })
                .collect();
            Ok(commands)
        }
        Edit::AudioTrim { clip: key, start, end } => {
            if start.is_none() && end.is_none() {
                return Ok(Vec::new());
            }
            let tree = doc.tree();
            let at = placed(&tree, *key)?;
            let audio = at.audio.ok_or("That is not an audio clip")?;
            let tempo = project.session.tempo;
            let mut trim = json!({"op": "audio.trim", "clip": clip(key)});
            let mut commands = Vec::new();
            let mut sounds = at.end;
            // No earlier than its file starts, or the song.
            let first = start.map(|start| start.max(song.beat_of(audio, 0.0)).max(0.0));
            if let Some(first) = first {
                trim["start"] = beat_at(first)?;
            }
            if let Some(end) = end {
                // No later than its file ends.
                let last = song.seconds(&audio.sample).map_or(f64::INFINITY, |file| song.beat_of(audio, file));
                let end = end.min(last);
                let from = first.unwrap_or_else(|| at.start.to_f64().unwrap_or(0.0));
                let leaves = if end >= last - 1e-6 {
                    // Its file ends there, and it fades over the last of it.
                    last
                } else {
                    // It leaves its fade out before where its sound ends, and
                    // a fade longer than half of what is left is shortened.
                    let longest = ((end - from) / 2.0).max(0.0) / fade_beats(1.0, tempo);
                    let mut fade = audio.fade_out_ms;
                    if fade > longest {
                        fade = (longest * 10.0).floor() / 10.0;
                        commands.push(json!({"op": "set", "path": format!("{}.fade_out_ms", clip(key)), "value": number(fade)}));
                    }
                    end - fade_beats(fade, tempo)
                };
                trim["end"] = beat_at(leaves)?;
                sounds = aaw_model::beat(&Beat::Float(end.max(0.0)))?;
            }
            commands.insert(0, trim);
            let label = format!("Trim audio clip {}", audio.sample);
            Ok(match commands.len() {
                1 => song.grown(commands, Some(&sounds), || label),
                _ => {
                    commands.splice(0..0, song.lengthen(&sounds));
                    batch(commands, label)
                }
            })
        }
        Edit::AudioFade { clip: key, fade_in_ms, fade_out_ms } => {
            let tree = doc.tree();
            let at = placed(&tree, *key)?;
            let audio = at.audio.ok_or("That is not an audio clip")?;
            let tempo = project.session.tempo;
            // To a tenth of a millisecond, within what a fade can be.
            let ms = |ms: f64| (ms.clamp(0.0, 10_000.0) * 10.0).round() / 10.0;
            let set = |field: &str, ms: f64| json!({"op": "set", "path": format!("{}.{field}", clip(key)), "value": number(ms)});
            let mut commands = Vec::new();
            if let Some(fade) = fade_in_ms {
                commands.push(set("fade_in_ms", ms(*fade)));
            }
            if let Some(fade) = fade_out_ms {
                let fade = ms(*fade);
                commands.push(set("fade_out_ms", fade));
                // Its sound ends where it did, so it leaves this fade before
                // that; at its file's end it fades over the last of the file.
                let sounds = at.end.to_f64().unwrap_or(0.0);
                let last = song.seconds(&audio.sample).map_or(f64::INFINITY, |file| song.beat_of(audio, file));
                if sounds < last - 1e-6 {
                    let leaves = sounds - fade_beats(fade, tempo);
                    if leaves <= at.start.to_f64().unwrap_or(0.0) {
                        return Err(format!("A fade out of {fade} ms is longer than audio clip {}", audio.sample));
                    }
                    commands.push(json!({"op": "audio.trim", "clip": clip(key), "end": beat_at(leaves)?}));
                }
            }
            // A field set by its path is named by the path, so this says what it is.
            let what = match (fade_in_ms, fade_out_ms) {
                (Some(_), Some(_)) => "fades",
                (Some(_), None) => "fade in",
                (None, Some(_)) => "fade out",
                (None, None) => return Ok(Vec::new()),
            };
            Ok(batch(commands, format!("Set the {what} of audio clip {}", audio.sample)))
        }
        Edit::AudioSplit { clips, at } => {
            let tree = doc.tree();
            let at = aaw_model::beat(&Beat::Float(at.max(0.0)))?;
            let commands: Vec<Json> = clips
                .iter()
                .filter(|key| {
                    // Inside what it plays before it leaves: its fade out is not split.
                    placed(&tree, **key).is_ok_and(|p| {
                        let leaves = p.audio.and_then(|a| Some(song.beat_of(a, song.source_end(a).ok()?)));
                        p.start < at && leaves.is_some_and(|leaves| at.to_f64().is_some_and(|at| at < leaves - 1e-9))
                    })
                })
                .map(|key| json!({"op": "audio.split", "clip": clip(key), "at": beat(&at)}))
                .collect();
            if commands.is_empty() {
                return Err("No audio clip to split plays at the start position".into());
            }
            Ok(commands)
        }
        Edit::AudioSet { clip: key, field, value } => {
            const FIELDS: [&str; 4] = ["gain_db", "fade_curve", "source_bpm", "stretch"];
            if !FIELDS.contains(&field.as_str()) {
                return Err(format!("An audio clip's {field} is not set here"));
            }
            let tree = doc.tree();
            let at = placed(&tree, *key)?;
            let audio = at.audio.ok_or("That is not an audio clip")?;
            let path = format!("{}.{field}", clip(key));
            let command = match value {
                FieldValue::Absent => json!({"op": "remove", "path": path}),
                value => json!({"op": "set", "path": path, "value": field_json(value)}),
            };
            // At another tempo of its own the clip plays for other beats.
            let mut end = None;
            if field == "source_bpm" {
                let bpm = match value {
                    FieldValue::Number { value } if *value > 0.0 => *value,
                    _ => project.session.tempo,
                };
                let mut retimed = audio.clone();
                retimed.source_bpm = Some(bpm);
                let beats = (song.source_end(audio)? - audio.source_start_seconds) * bpm / 60.0
                    + tail_beats(&retimed, project.session.tempo, song.seconds(&audio.sample));
                end = Some(&at.start + span(beats));
            }
            Ok(song.grown(vec![command], end.as_ref(), || format!("Set the tempo of audio clip {}", audio.sample)))
        }
        Edit::TrackAdd { index, midi } => {
            let mut command = json!({
                "op": "track.add", "id": free_name(project, if *midi { "midi" } else { "track" }),
                "index": (*index as usize).min(project.tracks.len()),
            });
            if *midi {
                command["type"] = json!("midi");
            }
            Ok(vec![command])
        }
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
            if t.midi.is_some() {
                return Ok(vec![json!({"op": "clip.add", "track": handle_text(*track), "at": beat(&at), "length_beats": beat(&length)})]);
            }
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
            let at = placed(&tree, *key)?;
            let j = items(&items(&tree, "tracks")[at.track].node, "clips")
                .iter()
                .position(|c| c.handle == *key)
                .ok_or("An audio clip has no pattern")?;
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
            let stem = ident(name, "sample");
            let midi = track.is_some_and(|key| is_midi(project, &tree, key));
            // On a MIDI track a new sample plays as it is at middle C, as in
            // Ableton, whatever pitch it measures at; one the song has keeps
            // the root note the song gives it.
            let unrooted = crate::library::Asset { root_note: None, ..asset.clone() };
            let (sample, mut commands) = listed(project, if midi { &unrooted } else { asset }, &stem);
            let label = match track {
                Some(key) if midi => {
                    let place = items(&tree, "tracks").iter().position(|i| i.handle == *key);
                    let t = &project.tracks[place.ok_or("The track is no longer in the song")?];
                    let instrument = json!({"sampler": {
                        "pads": {stem.clone(): {"sample": sample}},
                        "map": [{"notes": [0, 127], "pad": stem, "pitched": true}],
                    }});
                    commands.push(json!({"op": "instrument.set", "track": handle_text(*key), "instrument": instrument}));
                    match t.midi.as_ref().and_then(|m| m.instrument.as_ref()) {
                        Some(_) => format!("Replace the instrument of {} with a sampler of {sample}", t.id),
                        None => format!("Attach a sampler of {sample} to {}", t.id),
                    }
                }
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
        Edit::MidiClip { path, track, index, at } => {
            let mut import = json!({"op": "midi.import", "file": path, "at": beat_at(*at)?, "index": index});
            if let Some(key) = track {
                import["track"] = json!(handle_text(*key));
            }
            Ok(vec![import])
        }
        Edit::SampleClip { asset, name, track, index, at } => {
            let tree = doc.tree();
            let stem = ident(name, "sample");
            let (sample, mut commands) = listed(project, asset, &stem);
            let start = aaw_model::beat(&Beat::Float(at.max(0.0)))?;
            let add = |track: Json| json!({"op": "audio.add", "track": track, "sample": sample, "at": beat(&start)});
            let label = match track {
                Some(key) => {
                    let place = items(&tree, "tracks").iter().position(|i| i.handle == *key);
                    let t = &project.tracks[place.ok_or("The track is no longer in the song")?];
                    commands.push(add(json!(handle_text(*key))));
                    format!("Add audio clip of {sample} to {}", t.id)
                }
                None => {
                    let taken = |n: &str| project.tracks.iter().any(|t| t.id == n) || project.returns.iter().any(|r| r.id == n);
                    let id = unique(&stem, taken);
                    commands.push(json!({"op": "track.add", "id": id, "index": (*index as usize).min(project.tracks.len())}));
                    commands.push(add(json!(id)));
                    format!("Add track {id} with audio clip {sample}")
                }
            };
            // The clip plays its whole file, at the file's own tempo.
            let file = Sample {
                path: asset.path.clone(),
                sha256: Some(asset.sha256.clone()),
                source: None,
                source_sha256: None,
                root_note: None,
            };
            let end = files.of(directory, &file).map(|file| {
                let beats = file.seconds * project.session.tempo / 60.0;
                &start + span(beats)
            });
            if let Some(longer) = end.as_ref().and_then(|end| song.lengthen(end)) {
                commands.insert(0, longer);
            }
            Ok(batch(commands, label))
        }
    }
}

/// What Copy takes of clips, for `Edit::ClipsPaste`: each clip's track,
/// where it starts, its kind and its fields, as JSON. A paste adds clips with
/// these fields, so it works after the clips are changed or gone.
pub fn copied(doc: &Doc, clips: &[u64], files: &Files, directory: &Path) -> Result<String> {
    let song = Song { project: &doc.project, files, directory };
    let tree = doc.tree();
    let mut out = Vec::new();
    for key in clips {
        let at = song.placed(&tree, *key)?;
        let loc = tree::find(&tree, *key).ok_or("The clip is no longer in the song")?;
        let mut fields = node_json(tree::get(&tree, &loc));
        let map = fields.as_object_mut().ok_or("A clip is an object")?;
        map.retain(|_, value| !value.is_null());
        // A pasted note clip is given an ID of its own; its notes keep theirs.
        map.remove("id");
        map.remove("at");
        let kind = if at.audio.is_some() { "audio" } else if at.notes { "notes" } else { "pattern" };
        out.push(json!({
            "track": at.track, "kind": kind, "fields": fields,
            "start": aaw_model::fraction_str(&at.start), "length": aaw_model::fraction_str(&(&at.end - &at.start)),
        }));
    }
    Ok(Json::Array(out).to_string())
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
        Edit::AudioSplit { .. } => "Split",
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
