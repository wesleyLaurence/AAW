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
use num_traits::{Signed, Zero};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value as Json};

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
    #[serde(rename = "return.move")]
    ReturnMove {
        #[serde(rename = "return")]
        id: String,
        index: usize,
    },
    /// A group of the tracks named, which are moved together to where the
    /// first of them is; a track in another group leaves it.
    #[serde(rename = "group.add")]
    GroupAdd {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tracks: Vec<String>,
        #[serde(flatten)]
        fields: Fields,
    },
    /// Removes a group; its tracks go to the master again.
    #[serde(rename = "group.remove")]
    GroupRemove { group: String },
    /// Renames a group and the tracks' `group` naming it.
    #[serde(rename = "group.rename")]
    GroupRename { group: String, to: String },
    #[serde(rename = "group.move")]
    GroupMove { group: String, index: usize },

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
        /// A note clip copy's ID; without one it is given the next free one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// How many copies, one after another; one when left out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        times: Option<Json>,
    },
    #[serde(rename = "clip.remove")]
    ClipRemove { clip: String },
    /// Sets a note clip's length; its notes stay where they are. On a looped
    /// audio clip, how long it plays.
    #[serde(rename = "clip.resize")]
    ClipResize { clip: String, length_beats: Json },
    /// Loops a note clip or an audio clip: its first `loop_beats` play again
    /// and again until its end. `off` or null plays it once again. An audio
    /// clip's length is `length`, or its current length.
    #[serde(rename = "clip.loop")]
    ClipLoop {
        clip: String,
        loop_beats: Json,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        length: Option<Json>,
    },
    /// Moves a note clip's start, its end or both to a song beat. Its notes
    /// stay where they are in the song: those the start passes are kept,
    /// before the clip, and do not play.
    #[serde(rename = "clip.trim")]
    ClipTrim {
        clip: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<Json>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end: Option<Json>,
    },
    /// Makes one clip of two or more clips of one track that plays what they
    /// played. Note clips become one note clip from the first's start to the
    /// last's end, holding every note that played, loops unrolled; pattern
    /// clips and audio clips join only where they meet and are one music: the
    /// same pattern, or the same file played on from where the one before
    /// leaves. The first clip is the one kept, with its ID and reference.
    #[serde(rename = "clip.join")]
    ClipJoin { clips: Vec<String> },

    // Notes of note clips
    /// Adds notes to a note clip: `notes`, a list of notes, or one note from
    /// the command's other keys.
    #[serde(rename = "note.add")]
    NoteAdd {
        clip: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        notes: Vec<Fields>,
        #[serde(flatten)]
        fields: Fields,
    },
    #[serde(rename = "note.set")]
    NoteSet {
        note: String,
        #[serde(flatten)]
        fields: Fields,
    },
    /// Moves notes later, or earlier for a negative `by`, by exact beats. A
    /// clip in `notes` stands for all of its notes.
    #[serde(rename = "note.move")]
    NoteMove { notes: Vec<String>, by: Json },
    /// Moves notes up, or down for a negative `by`, by semitones.
    #[serde(rename = "note.transpose")]
    NoteTranspose { notes: Vec<String>, by: i64 },
    #[serde(rename = "note.remove")]
    NoteRemove { notes: Vec<String> },

    // MIDI files
    /// Reads a MIDI file of one part into a note clip at `at`, 0 unless
    /// given: on `track`, a MIDI track, or else on a new MIDI track named
    /// after the file, at `index` among the tracks or last. The song grows
    /// to hold the clip. The reply counts what the file has that the song
    /// does not hold.
    #[serde(rename = "midi.import")]
    MidiImport {
        file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        track: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Json>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },

    // Instruments of MIDI tracks
    /// Attaches or replaces a MIDI track's instrument; null removes it. The
    /// notes are not touched.
    #[serde(rename = "instrument.set")]
    InstrumentSet { track: String, instrument: Json },
    /// Maps a note or a range of notes to a pad of a MIDI track's sampler:
    /// `notes`, `pad` and `pitched`.
    #[serde(rename = "instrument.map")]
    InstrumentMap {
        track: String,
        #[serde(flatten)]
        fields: Fields,
    },

    // The Synth
    /// Attaches the plain saw, or the patch `patch` names, to a MIDI track,
    /// or makes a new MIDI track with it, at `index` among the tracks or
    /// last.
    #[serde(rename = "synth.add")]
    SynthAdd {
        track: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        patch: Option<String>,
    },
    /// Loads a patch into a MIDI track: the patch's `synth` mapping takes
    /// the instrument's place whole, and the notes stay. `patch` is a
    /// patch's name, a saved patch's or a factory one's, or a `.yaml` file.
    #[serde(rename = "patch.load")]
    PatchLoad { track: String, patch: String },
    /// Saves a MIDI track's synth to the library as a patch named `name`,
    /// with a description and tags, and names the song's patch after it.
    /// Over a patch already saved under that name only with `replace`.
    #[serde(rename = "patch.save")]
    PatchSave {
        track: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        replace: bool,
    },
    /// Sets fields of a MIDI track's synth by their paths in the patch, as
    /// one step: `values` maps each path to its value, null removing an
    /// oscillator, an LFO or a macro.
    #[serde(rename = "synth.set")]
    SynthSet { track: String, values: Fields },
    /// Adds an entry to the synth's matrix, or changes the amount of the
    /// entry with that source and target; `remove` takes it out.
    #[serde(rename = "synth.mod")]
    SynthMod {
        track: String,
        source: String,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        amount: Option<Json>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        remove: bool,
    },

    // Audio clips
    #[serde(rename = "audio.add")]
    AudioAdd {
        track: String,
        #[serde(flatten)]
        fields: Fields,
    },
    /// Moves a clip to another beat, another track or both. Its audio moves
    /// with it, and it keeps its handle.
    #[serde(rename = "audio.move")]
    AudioMove {
        clip: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        track: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Json>,
    },
    /// Makes two clips of one at a beat inside it; they play as the one did.
    #[serde(rename = "audio.split")]
    AudioSplit { clip: String, at: Json },
    /// Moves a clip's start or end to a beat, keeping its audio where it is.
    #[serde(rename = "audio.trim")]
    AudioTrim {
        clip: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<Json>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end: Option<Json>,
    },
    /// Crossfades a clip with the one before it on its track: that one fades
    /// out over `ms` under this one, which starts `lead_ms` before its beat.
    #[serde(rename = "audio.crossfade")]
    AudioCrossfade {
        clip: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ms: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        in_ms: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lead_ms: Option<f64>,
    },
    /// Removes the beats from `from` to `to` of a track's audio clips, moves
    /// what follows earlier to close the gap, and crossfades the join.
    #[serde(rename = "audio.cut")]
    AudioCut {
        track: String,
        from: Json,
        to: Json,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ms: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        in_ms: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lead_ms: Option<f64>,
    },

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
    /// A send of a track or a group, which `track` names.
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

    // Ranges of beats across tracks
    /// Puts a copy of the beats from `start` for `length` at `to`, with the
    /// clips, audio, automation and sections in them, over what is there;
    /// `insert` opens time for it instead. `tracks` limits it to some
    /// tracks, whose lanes go with them. Clips across an edge are cut there.
    #[serde(rename = "range.copy")]
    RangeCopy {
        start: Json,
        length: Json,
        to: Json,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        insert: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tracks: Vec<String>,
    },
    /// Opens `length` empty beats at `at`: everything from there moves
    /// later, and the song grows.
    #[serde(rename = "range.insert")]
    RangeInsert {
        at: Json,
        length: Json,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tracks: Vec<String>,
    },
    /// Removes the beats from `start` for `length` and closes the gap: the
    /// song shrinks.
    #[serde(rename = "range.delete")]
    RangeDelete {
        start: Json,
        length: Json,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tracks: Vec<String>,
    },
    /// Removes what is in the beats from `start` for `length`; nothing moves.
    #[serde(rename = "range.clear")]
    RangeClear {
        start: Json,
        length: Json,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tracks: Vec<String>,
    },

    // Sections
    #[serde(rename = "section.add")]
    SectionAdd { id: String, at: Json, length_beats: Json },
    /// Moves a section's label to `at`; with `with_content`, its beats too,
    /// over what is there.
    #[serde(rename = "section.move")]
    SectionMove {
        section: String,
        at: Json,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        with_content: bool,
    },
    /// Removes a section's label; with `with_content`, its beats too, as
    /// `range.delete` does.
    #[serde(rename = "section.remove")]
    SectionRemove {
        section: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        with_content: bool,
    },
    /// The section and what is under it again at `to`, or right after it,
    /// pushing what follows later; the copy is named `id`, or after the
    /// section.
    #[serde(rename = "section.duplicate")]
    SectionDuplicate {
        section: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<Json>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    // Whole-document edits
    #[serde(rename = "apply")]
    Apply {
        patch: Json,
        /// What the patch does, for the change log and undo.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    #[serde(rename = "batch")]
    Batch {
        commands: Vec<Command>,
        /// What the batch does as a whole, for the change log and undo.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },

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
    /// Plays a note now through a MIDI track's Synth and the track's chain,
    /// outside the timeline and the undo history: `pitch` a MIDI number or
    /// a name such as `C4`, `velocity` 1 to 127 (100 unless given) and
    /// `length_beats` how long it is held (1 unless given). Monitoring, as
    /// the metronome is: never an edit of the song.
    #[serde(rename = "note.preview")]
    NotePreview {
        track: String,
        pitch: Json,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        velocity: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        length_beats: Option<Json>,
    },
    /// Monitoring only; never an edit of the song.
    #[serde(rename = "metronome")]
    Metronome { enabled: bool },
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
    /// The notes of a note clip, or of a MIDI track's clips, with their names
    /// and song beats; `from` and `to` keep those that start in a range of
    /// song beats.
    #[serde(rename = "notes")]
    Notes {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<Json>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<Json>,
    },
    /// The song as a grid of tracks by bars: where each plays, the same
    /// music under the same letter, and clips that sound at once. `per` is
    /// a number of bars a cell, `bar` or `beat`; `from` and `to` are beats;
    /// `tracks` keeps those tracks; `lanes` adds a row a lane.
    #[serde(rename = "map")]
    Map {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        per: Option<Json>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<Json>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<Json>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tracks: Vec<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        lanes: bool,
    },
    /// Writes the notes of a note clip that play as a MIDI file at `file`.
    #[serde(rename = "midi.export")]
    MidiExport { clip: String, file: String },
    /// What the person has selected in the app, as references; `status` reports
    /// it. It is host state like the loop: not saved, and not a change.
    #[serde(rename = "select")]
    Select {
        #[serde(default)]
        items: Vec<String>,
    },
    /// Whether the project's window is the one in front in the app; `status`
    /// and `daw projects` report it. Host state, as the selection is.
    #[serde(rename = "front")]
    Front { front: bool },
    /// Moves the project's folder to `to`, an absolute path where nothing is
    /// yet, and names the song after it. The host carries on at the new path
    /// with its history, and still answers at the old one.
    #[serde(rename = "project.move")]
    Move { to: String },
    /// Copies the project's folder to `to` and carries on in the copy, named
    /// after it; the original stays as it was. The host still answers at the
    /// original's path until that is released.
    #[serde(rename = "project.copy")]
    Copy { to: String },
    /// Stops answering for a path the project had before it was saved under
    /// another name, so that the project there can be opened.
    #[serde(rename = "project.release")]
    Release { project: String },
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
            Play { .. } | Stop | Locate { .. } | Loop { .. } | Metronome { .. } | NotePreview { .. } => Kind::Transport,
            Inspect | Status | Changes { .. } | Get { .. } | Notes { .. } | Map { .. } | MidiExport { .. } => Kind::Read,
            Select { .. } | Front { .. } | Move { .. } | Copy { .. } | Release { .. } | Fmt | Close => Kind::Host,
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
    /// The objects the command created, by handle.
    pub made: Vec<u64>,
    /// A `set`'s readable path and previous value.
    pub path: Option<String>,
    pub before: Option<Json>,
    /// Changes the command made to keep references valid.
    pub also: Vec<String>,
    /// What the reply says besides, such as what a MIDI file had that the
    /// song does not hold.
    pub report: Map<String, Json>,
}

/// Applies edits to a tree. Created objects get handles at once.
pub struct Edit<'a> {
    pub root: &'a mut Node,
    /// Whether `@N` handles may be used in paths.
    pub handles: bool,
    pub next: &'a mut u64,
    /// Who is editing, for what an edit writes outside the song.
    pub origin: Origin,
    pub(crate) also: Vec<String>,
}

pub(crate) type Result<T> = std::result::Result<T, String>;

pub fn json_value(j: &Json) -> Result<Value> {
    aaw_model::json_value(&j.to_string()).map_err(|e| e.to_string())
}

pub(crate) fn to_json(v: &Value) -> Json {
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

/// A `set`'s label: the path, the value before it if there was one, and the new one.
pub fn set_label(path: &str, before: Option<&Json>, value: &Json) -> String {
    match before {
        Some(b) => format!("Set {path}: {} → {}", short(b), short(value)),
        None => format!("Set {path}: {}", short(value)),
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

/// The exact beat a leaf holds, if it holds one: a note's may be negative.
pub(crate) fn exact(n: Option<&Node>) -> Option<BigRational> {
    match n {
        Some(Node::Leaf(v)) => beat_of(v).and_then(|b| aaw_model::signed_beat(&b).ok()),
        _ => None,
    }
}

/// A beat written as an integer, an exact decimal or a fraction string.
pub fn beat_value(x: &BigRational) -> Value {
    if x.is_integer() {
        return Value::Int(x.to_integer());
    }
    let f = num_traits::ToPrimitive::to_f64(x).unwrap_or(f64::NAN);
    if aaw_model::signed_beat(&Beat::Float(f)).ok().as_ref() == Some(x) {
        Value::Float(f)
    } else {
        Value::Str(aaw_model::fraction_str(x))
    }
}

pub(crate) fn number(n: Option<&Node>) -> Option<f64> {
    match n {
        Some(Node::Leaf(Value::Float(f))) => Some(*f),
        Some(Node::Leaf(Value::Int(i))) => num_traits::ToPrimitive::to_f64(i),
        _ => None,
    }
}

pub(crate) fn set(node: &mut Node, key: &str, value: Value) {
    if let Some(map) = node.map_mut() {
        map.insert(key.to_string(), Node::Leaf(value));
    }
}

/// A beat given to a command, exactly.
fn beat_at(j: &Json) -> Result<BigRational> {
    let value = json_value(j)?;
    beat_of(&value)
        .and_then(|b| aaw_model::beat(&b).ok())
        .ok_or_else(|| format!("{} is not a beat", short(j)))
}

/// A number of beats to move by, exactly, which may be negative: `-1/48`.
fn offset_at(j: &Json) -> Result<BigRational> {
    let negated = match j {
        Json::String(t) => t.trim().strip_prefix('-').map(|rest| Json::String(rest.to_string())),
        Json::Number(n) if n.as_f64().is_some_and(|x| x < 0.0) => {
            Some(serde_json::from_str(n.to_string().trim_start_matches('-')).map_err(|e| e.to_string())?)
        }
        _ => None,
    };
    match negated {
        Some(rest) => Ok(-beat_at(&rest)?),
        None => beat_at(j),
    }
}

pub(crate) fn beat_text(x: &BigRational) -> String {
    short(&to_json(&beat_value(x)))
}

/// An audio clip's place: the beat it is at, the seconds of its file it plays,
/// and how many of those seconds go by in a beat.
pub(crate) struct Span {
    pub(crate) at: BigRational,
    pub(crate) start: f64,
    pub(crate) end: Option<f64>,
    per_beat: f64,
    /// A looped clip's loop and its length on the timeline.
    pub(crate) every: Option<BigRational>,
    pub(crate) length: Option<BigRational>,
}

impl Span {
    pub(crate) fn of(clip: &Node, tempo: f64) -> Result<Span> {
        let at = exact(clip.get("at")).ok_or("An audio clip has no beat")?;
        // A clip with a tempo of its own follows the session's, so a beat of
        // the session is a beat of its file.
        let per_beat = 60.0 / number(clip.get("source_bpm")).unwrap_or(tempo);
        Ok(Span {
            at,
            start: number(clip.get("source_start_seconds")).unwrap_or(0.0),
            end: number(clip.get("source_end_seconds")),
            per_beat,
            every: exact(clip.get("loop_beats")),
            length: exact(clip.get("length_beats")),
        })
    }

    /// The seconds of the file that play on a beat.
    pub(crate) fn source(&self, beat: &BigRational) -> f64 {
        let beats = num_traits::ToPrimitive::to_f64(&(beat - &self.at)).unwrap_or(f64::NAN);
        self.start + beats * self.per_beat
    }

    /// The beat the clip ends on; None when it plays to the end of its file.
    pub(crate) fn end_beat(&self) -> Option<BigRational> {
        if let Some(length) = &self.length {
            return Some(&self.at + length);
        }
        let beats = (self.end? - self.start) / self.per_beat;
        Some(&self.at + BigRational::from_float(beats)?)
    }

    /// Beats from the clip's start to `at`, when `at` is at a wrap of a clip
    /// that loops every `every` beats; else why a looped clip is not cut there.
    pub(crate) fn wrap(&self, at: &BigRational, every: &BigRational, what: &str) -> Result<BigRational> {
        wrap_of(&self.at, at, every, what)
    }
}

/// Beats from `start` to `at`, when `at` is a whole number of loops of
/// `every` beats from it; else why a looped clip is not cut there.
pub(crate) fn wrap_of(start: &BigRational, at: &BigRational, every: &BigRational, what: &str) -> Result<BigRational> {
    let cut = at - start;
    if !(&cut / every).is_integer() {
        return Err(format!(
            "Beat {} falls inside a repetition of {what}, which loops every {} beats; a looped clip is cut only at a wrap, or turn its loop off first",
            beat_text(at),
            aaw_model::fraction_str(every)
        ));
    }
    Ok(cut)
}

fn map_node(fields: &Fields) -> Result<Node> {
    Ok(Node::new(&json_value(&Json::Object(fields.clone()))?))
}

/// A new object with the lists and maps it has when empty, so that later
/// commands of the same batch can add to them: a clip to a track just added.
fn with_empty(fields: &Fields, lists: &[&str], maps: &[&str]) -> Result<Node> {
    let mut node = map_node(fields)?;
    let map = node.map_mut().expect("an object");
    for key in lists {
        map.entry(key.to_string()).or_insert_with(|| Node::List(Vec::new()));
    }
    for key in maps {
        map.entry(key.to_string()).or_insert_with(|| Node::Map(indexmap::IndexMap::new()));
    }
    Ok(node)
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
            origin: Origin::Agent,
            also: Vec::new(),
        }
    }

    pub fn by(mut self, origin: Origin) -> Self {
        self.origin = origin;
        self
    }

    fn fresh(&mut self) -> u64 {
        *self.next += 1;
        *self.next
    }

    pub(crate) fn at(&self, path: &str) -> Result<Loc> {
        tree::resolve(self.root, path, self.handles)
    }

    pub(crate) fn node(&self, loc: &[Step]) -> &Node {
        tree::get(self.root, loc)
    }

    pub(crate) fn node_mut(&mut self, loc: &[Step]) -> &mut Node {
        tree::get_mut(self.root, loc)
    }

    pub(crate) fn text(&self, loc: &[Step]) -> String {
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

    pub(crate) fn list(&mut self, owner: &[Step], key: &str) -> Result<&mut Vec<Item>> {
        let name = self.text(owner);
        self.node_mut(owner)
            .get_mut(key)
            .and_then(Node::items_mut)
            .ok_or_else(|| format!("{name} has no {key}"))
    }

    pub(crate) fn insert(&mut self, owner: &[Step], key: &str, index: Option<usize>, node: Node) -> Result<u64> {
        let handle = self.fresh();
        let list = self.list(owner, key)?;
        let index = index.unwrap_or(list.len());
        if index > list.len() {
            return Err(format!("{key} index {index} is past the end ({} items)", list.len()));
        }
        list.insert(index, Item { handle, node });
        Ok(handle)
    }

    pub(crate) fn take(&mut self, owner: &[Step], key: &str, index: usize) -> Result<Item> {
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

    /// A chain's owner: a track, a return, the master, or a MIDI track's
    /// synth, whose patch has a chain of its own.
    fn owner(&self, path: &str) -> Result<Loc> {
        let loc = self.at(path)?;
        if self.node(&loc).get("effects").is_none() || self.lane_owner(&loc).is_none() {
            return Err(format!("{path} is not a track, return, master or synth"));
        }
        Ok(loc)
    }

    /// Where the lanes on a chain owner's effects live, and the prefix they
    /// address the chain by: an owner's own lanes as `effects.REF.FIELD`, or
    /// for a synth the track's as `instrument.effects.REF.FIELD`.
    fn lane_owner(&self, owner: &[Step]) -> Option<(Loc, &'static str)> {
        if self.node(owner).get("automation").is_some() {
            return Some((owner.to_vec(), ""));
        }
        match owner {
            [track @ .., Step::Key(i), Step::Key(s)] if i == "instrument" && s == "synth" => Some((track.to_vec(), "instrument.")),
            _ => None,
        }
    }

    pub(crate) fn track(&self, id: &str) -> Result<Loc> {
        self.at(&format!("tracks.{id}"))
    }

    /// Rewrites or removes an owner's lanes on effects after its chain changes:
    /// `index` maps old effect positions to new ones, None where removed.
    /// A synth's chain is automated from its track, as `instrument.effects`.
    fn remap_lanes(&mut self, owner: &[Step], index: impl Fn(usize) -> Option<usize>, removed_id: Option<&str>) -> Result<()> {
        let (lane_owner, prefix) = self.lane_owner(owner).ok_or_else(|| format!("{} has no chain", self.text(owner)))?;
        let name = self.text(&lane_owner);
        let lanes = self.list(&lane_owner, "automation")?;
        let mut notes = Vec::new();
        lanes.retain_mut(|lane| {
            let Some(param) = lane.node.field("param").map(str::to_string) else {
                return true;
            };
            let Some(own) = param.strip_prefix(prefix) else {
                return true;
            };
            let Some((r, rest)) = effect_ref(own) else {
                return true;
            };
            let target = match index_ref(r) {
                Some(i) => index(i).map(|j| format!("{prefix}effects.{j}.{rest}")),
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
        let mut out = self.senders();
        for i in 0..self.root.get("returns").map_or(0, |n| n.items().len()) {
            out.push(vec![Step::Key("returns".into()), Step::Index(i)]);
        }
        out.push(vec![Step::Key("master".into())]);
        out
    }

    /// Every channel that sends: the tracks, then the groups.
    fn senders(&self) -> Vec<Loc> {
        let mut out = Vec::new();
        for key in ["tracks", "groups"] {
            for i in 0..self.root.get(key).map_or(0, |n| n.items().len()) {
                out.push(vec![Step::Key(key.into()), Step::Index(i)]);
            }
        }
        out
    }

    /// The tracks in a group.
    fn tracks_in(&self, group: &str) -> Vec<Loc> {
        (0..self.root.get("tracks").map_or(0, |n| n.items().len()))
            .map(|i| vec![Step::Key("tracks".into()), Step::Index(i)])
            .filter(|loc| self.node(loc).field("group") == Some(group))
            .collect()
    }

    /// A track or a group, by ID or handle: what a send belongs to.
    fn sender(&self, id: &str) -> Result<Loc> {
        self.track(id).or_else(|e| self.at(&format!("groups.{id}")).map_err(|_| e))
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
                    "groups" => {
                        // Its tracks go to the master again.
                        if let Some(id) = item.node.field("id").map(str::to_string) {
                            for track in self.tracks_in(&id) {
                                if let Some(m) = self.node_mut(&track).map_mut() {
                                    m.shift_remove("group");
                                }
                            }
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
        for sender in self.senders() {
            let sends = self.list(&sender, "sends")?;
            if let Some(j) = sends.iter().position(|s| s.node.field("to") == Some(return_id)) {
                let loc = [sender.clone(), vec![Step::Key("sends".into()), Step::Index(j)]].concat();
                let what = self.remove_at(&loc)?;
                self.also.push(format!("removed {what}"));
            }
        }
        Ok(())
    }

    pub(crate) fn set_leaf(&mut self, loc: &[Step], key: &str, v: Value) {
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
                out.label = set_label(&path, before.as_ref(), value);
                out.path = Some(path);
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
                f.extend(fields.clone());
                let node = match key {
                    "tracks" if f.get("type").and_then(Json::as_str) == Some("midi") => {
                        with_empty(&f, &["clips", "effects", "sends", "automation"], &[])?
                    }
                    "tracks" => with_empty(&f, &["clips", "audio", "effects", "sends", "automation"], &["pads"])?,
                    _ => with_empty(&f, &["effects", "automation"], &[])?,
                };
                out.made.push(self.insert(&[], key, *index, node)?);
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
            GroupAdd { id, index, tracks: members, fields } => {
                let mut f = Fields::new();
                f.insert("id".into(), Json::String(id.clone()));
                f.extend(fields.clone());
                let node = with_empty(&f, &["effects", "sends", "automation"], &[])?;
                out.made.push(self.insert(&[], "groups", *index, node)?);
                // The tracks named join it, moved together to where the first
                // of them is, in their order.
                let mut places: Vec<usize> = Vec::new();
                for t in members {
                    let loc = self.track(t)?;
                    let Some(Step::Index(i)) = loc.last() else { unreachable!() };
                    places.push(*i);
                }
                places.sort_unstable();
                places.dedup();
                let mut names = Vec::new();
                if let Some(&first) = places.first() {
                    let list = self.list(&[], "tracks")?;
                    let mut taken: Vec<Item> = places.iter().rev().map(|&i| list.remove(i)).collect();
                    taken.reverse();
                    for (k, mut item) in taken.into_iter().enumerate() {
                        names.push(item.node.field("id").unwrap_or_default().to_string());
                        if let Some(m) = item.node.map_mut() {
                            m.insert("group".into(), Node::Leaf(Value::str(id)));
                        }
                        list.insert(first + k, item);
                    }
                }
                out.label = if names.is_empty() {
                    format!("Add group {id}")
                } else {
                    format!("Add group {id} of {}", names.join(", "))
                };
            }
            GroupRemove { group } => {
                let loc = self.at(&format!("groups.{group}"))?;
                out.label = format!("Remove {}", self.remove_at(&loc)?);
            }
            GroupRename { group, to } => {
                let loc = self.at(&format!("groups.{group}"))?;
                let old = self.name(&loc);
                self.set_leaf(&loc, "id", Value::str(to));
                for track in self.tracks_in(&old) {
                    self.set_leaf(&track, "group", Value::str(to));
                }
                out.label = format!("Rename group {old} to {to}");
            }
            GroupMove { group, index } => {
                let loc = self.at(&format!("groups.{group}"))?;
                let name = self.name(&loc);
                let Some(Step::Index(from)) = loc.last().cloned() else { unreachable!() };
                self.move_item(&[], "groups", from, *index)?;
                out.label = format!("Move group {name} to position {index}");
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
                let id = &self.name(&loc);
                self.set_leaf(&loc, "id", Value::str(to));
                let (old_lane, new_lane) = (format!("sends.{id}.gain_db"), format!("sends.{to}.gain_db"));
                for sender in self.senders() {
                    for s in self.list(&sender, "sends")?.iter_mut() {
                        if s.node.field("to") == Some(id.as_str()) {
                            *s.node.get_mut("to").expect("to") = Node::Leaf(Value::str(to));
                        }
                    }
                    for l in self.list(&sender, "automation")?.iter_mut() {
                        if l.node.field("param") == Some(old_lane.as_str()) {
                            *l.node.get_mut("param").expect("param") = Node::Leaf(Value::str(&new_lane));
                        }
                    }
                }
                out.label = format!("Rename return {id} to {to}");
            }
            TrackMove { track: id, index } | ReturnMove { id, index } => {
                let (key, what) = match cmd {
                    TrackMove { .. } => ("tracks", "track"),
                    _ => ("returns", "return"),
                };
                let loc = self.at(&format!("{key}.{id}"))?;
                let name = self.name(&loc);
                let Some(Step::Index(from)) = loc.last().cloned() else { unreachable!() };
                self.move_item(&[], key, from, *index)?;
                out.label = format!("Move {what} {name} to position {index}");
            }
            ClipAdd { track, fields } => {
                let loc = self.track(track)?;
                if self.is_midi(&loc) {
                    // Notes can be added to it by later commands of a batch,
                    // by the ID it is given here.
                    let mut f = fields.clone();
                    if !f.contains_key("id") {
                        f.insert("id".into(), Json::String(self.next_clip_id()));
                    }
                    let mut node = with_empty(&f, &["notes"], &[])?;
                    self.name_notes(&mut node);
                    out.made.push(self.insert(&loc, "clips", None, node)?);
                    let at = fields.get("at").map_or_else(|| "0".to_string(), short);
                    out.label = format!("Add note clip to {} at {at}", self.name(&loc));
                } else {
                    out.made.push(self.insert(&loc, "clips", None, map_node(fields)?)?);
                    let pattern = fields.get("pattern").and_then(Json::as_str).unwrap_or("?");
                    out.label = format!("Add clip of {pattern} to {}", self.name(&loc));
                }
            }
            ClipMove { clip, track, at } => {
                let (owner, i) = self.member(clip, "clips", "a clip")?;
                let mut clip_loc = [owner.clone(), vec![Step::Key("clips".into()), Step::Index(i)]].concat();
                if let Some(at) = at {
                    self.set_leaf(&clip_loc, "at", json_value(at)?);
                }
                if let Some(t) = track {
                    let dest = self.track(t)?;
                    if self.is_midi(&dest) != self.is_midi(&owner) {
                        return Err(if self.is_midi(&owner) {
                            format!("A note clip moves only to a MIDI track, and {t} is not one")
                        } else {
                            format!("A pattern clip cannot move to MIDI track {t}")
                        });
                    }
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
            ClipDuplicate { clip, track, at, id, times } => {
                let (owner, i) = self.member(clip, "clips", "a clip")?;
                let source = self.node(&owner).get("clips").expect("clips").items()[i].node.clone();
                let midi = self.is_midi(&owner);
                let times = match times {
                    None => 1,
                    Some(t) => match t.as_i64() {
                        Some(n @ 1..=1000) => n as usize,
                        _ => return Err(format!("times is a number of copies from 1 to 1000, not {}", short(t))),
                    },
                };
                if times > 1 && id.is_some() {
                    return Err(format!("{clip}: id names one copy; leave it out to make {times}"));
                }
                let dest = match track {
                    Some(t) => self.track(t)?,
                    None => owner.clone(),
                };
                if self.is_midi(&dest) != midi {
                    return Err(format!("{clip} cannot be copied to {}, which holds the other kind of clip", self.name(&dest)));
                }
                let span = self.clip_span(&source);
                // Copies at a beat given go one after another from it; without
                // one, each goes right after the original or, past clips that
                // already follow it there, after the last of them, so that
                // duplicating again lays the copies in a row.
                let mut next = match at {
                    Some(a) => Some(beat_at(a)?),
                    None => match (exact(source.get("at")).or_else(|| source.get("at").is_none().then(BigRational::default)), &span) {
                        (Some(s), Some(l)) => Some(s + l),
                        _ => return Err(format!("{clip}: cannot place the copy; pass at")),
                    },
                };
                let name = self.clip_name(&[owner.clone(), vec![Step::Key("clips".into()), Step::Index(i)]].concat());
                for k in 0..times {
                    let mut place = next.take().ok_or_else(|| format!("{clip}: cannot place the next copy; pass at"))?;
                    // A copy goes in the list after the clip it follows, so
                    // the list reads in the order the copies play.
                    let mut index = (dest == owner).then_some(i + 1 + k);
                    if at.is_none() {
                        let (beat, after) = self.past_run(&dest, place);
                        place = beat;
                        index = after.map(|j| j + 1).or(index);
                    }
                    // The copy owns its notes, as new objects. A note clip's copy
                    // has an ID of its own, and its notes keep theirs, which are
                    // their clip's.
                    let mut copy = Node::new(&source.value());
                    let id = match id {
                        Some(id) => Some(id.clone()),
                        None => midi.then(|| self.next_clip_id()),
                    };
                    let value = match (k, at) {
                        // A beat given for one copy is written as it was given.
                        (0, Some(a)) => json_value(a)?,
                        _ => beat_value(&place),
                    };
                    if let Some(m) = copy.map_mut() {
                        m.insert("at".into(), Node::Leaf(value));
                        if let Some(id) = id {
                            m.insert("id".into(), Node::Leaf(Value::Str(id)));
                        }
                    }
                    out.made.push(self.insert(&dest, "clips", index, copy)?);
                    next = span.as_ref().map(|l| place + l);
                }
                out.label = match times {
                    1 => format!("Duplicate clip {name}"),
                    n => format!("Duplicate clip {name} {n} times"),
                };
            }
            ClipRemove { clip } => {
                let (owner, i) = self.member(clip, "clips", "a clip")?;
                let loc = [owner, vec![Step::Key("clips".into()), Step::Index(i)]].concat();
                let name = self.clip_name(&loc);
                self.remove_at(&loc)?;
                out.label = format!("Remove clip {name}");
            }
            ClipResize { clip, length_beats } => {
                let length = beat_at(length_beats)?;
                if !length.is_positive() {
                    return Err(format!("Clip {clip} must be longer than 0 beats"));
                }
                let loc = match self.audio_clip(clip) {
                    Some(loc) => {
                        if exact(self.node(&loc).get("loop_beats")).is_none() {
                            return Err(format!(
                                "Audio clip {} does not loop, so its length is its audio's: audio trim --end moves its end, and clip loop makes it repeat",
                                self.audio_name(&loc)
                            ));
                        }
                        loc
                    }
                    None => self.note_clip(clip)?,
                };
                self.set_leaf(&loc, "length_beats", beat_value(&length));
                out.label = format!("Resize clip {} to {} beats", self.clip_name(&loc), short(length_beats));
            }
            ClipLoop { clip, loop_beats, length } => {
                let off = loop_beats.is_null() || loop_beats.as_str().is_some_and(|s| s.eq_ignore_ascii_case("off"));
                let every = if off { None } else { Some(beat_at(loop_beats)?) };
                if every.as_ref().is_some_and(|l| !l.is_positive()) {
                    return Err("A loop is longer than 0 beats, or off".into());
                }
                let audio = self.audio_clip(clip);
                let loc = match &audio {
                    Some(loc) => loc.clone(),
                    None => self.note_clip(clip)?,
                };
                let name = self.clip_name(&loc);
                match every {
                    Some(every) => {
                        if audio.is_some() {
                            // How long it plays: as given, as it did while looped, or as
                            // long as its audio.
                            let length = match (length, exact(self.node(&loc).get("length_beats"))) {
                                (Some(l), _) => beat_at(l)?,
                                (None, Some(l)) => l,
                                (None, None) => match self.span(&loc)?.end_beat() {
                                    Some(end) => end - exact(self.node(&loc).get("at")).unwrap_or_default(),
                                    None => {
                                        return Err(format!(
                                            "Audio clip {name} plays to its file's end; give length, how many beats it should play"
                                        ))
                                    }
                                },
                            };
                            if !length.is_positive() {
                                return Err(format!("Audio clip {name} must play longer than 0 beats"));
                            }
                            self.set_leaf(&loc, "length_beats", beat_value(&length));
                        } else if length.is_some() {
                            return Err(format!("Clip {name} is a note clip, whose length clip resize sets"));
                        }
                        self.set_leaf(&loc, "loop_beats", beat_value(&every));
                        out.label = format!("Loop clip {name} every {} beats", aaw_model::fraction_str(&every));
                    }
                    None => {
                        self.set_leaf(&loc, "loop_beats", Value::None);
                        if audio.is_some() {
                            self.set_leaf(&loc, "length_beats", Value::None);
                        }
                        out.label = format!("Loop clip {name} off");
                    }
                }
            }
            ClipTrim { clip, start, end } => {
                let loc = self.note_clip(clip)?;
                if start.is_none() && end.is_none() {
                    return Err("Trim needs a start or an end".into());
                }
                let at = exact(self.node(&loc).get("at")).unwrap_or_default();
                let length = exact(self.node(&loc).get("length_beats")).unwrap_or_default();
                let from = start.as_ref().map(beat_at).transpose()?.unwrap_or_else(|| at.clone());
                let to = end.as_ref().map(beat_at).transpose()?.unwrap_or_else(|| &at + &length);
                if to <= from {
                    return Err(format!("Clip {} would end before it starts", self.clip_name(&loc)));
                }
                if from != at {
                    // The notes stay where they are in the song.
                    let by = &from - &at;
                    let count = self.node(&loc).get("notes").map_or(0, |n| n.items().len());
                    for i in 0..count {
                        let note = [loc.clone(), vec![Step::Key("notes".into()), Step::Index(i)]].concat();
                        let moved = exact(self.node(&note).get("at")).unwrap_or_default() - &by;
                        self.set_leaf(&note, "at", beat_value(&moved));
                    }
                    self.set_leaf(&loc, "at", beat_value(&from));
                }
                self.set_leaf(&loc, "length_beats", beat_value(&(&to - &from)));
                out.label = format!("Trim clip {} to beats {} to {}", self.clip_name(&loc), beat_text(&from), beat_text(&to));
            }
            ClipJoin { clips } => {
                use crate::range::Kind;
                // Each clip once: its track, its list and its index there.
                let mut found: Vec<(Loc, &'static str, usize)> = Vec::new();
                for path in clips {
                    let place = match self.audio_clip(path) {
                        Some(loc) => match loc.as_slice() {
                            [owner @ .., Step::Key(_), Step::Index(i)] => (owner.to_vec(), "audio", *i),
                            _ => return Err(format!("{path} is not a clip")),
                        },
                        None => {
                            let (owner, i) = self.member(path, "clips", "a clip")?;
                            (owner, "clips", i)
                        }
                    };
                    if !found.contains(&place) {
                        found.push(place);
                    }
                }
                if found.len() < 2 {
                    return Err("Name two or more clips of one track to join".into());
                }
                let (owner, key) = (found[0].0.clone(), found[0].1);
                if found.iter().any(|(o, _, _)| *o != owner) {
                    return Err("The clips are on two tracks; clips joined are on one".into());
                }
                if found.iter().any(|(_, k, _)| *k != key) {
                    return Err("Pattern clips and audio clips are not joined into one; join each kind by itself".into());
                }
                let kind = match key {
                    "audio" => Kind::Audio,
                    _ if self.is_midi(&owner) => Kind::Note,
                    _ => Kind::Pattern,
                };
                let loc_of = |i: usize| [owner.clone(), vec![Step::Key(key.into()), Step::Index(i)]].concat();
                // In the order they play.
                let mut order: Vec<(BigRational, Option<BigRational>, usize)> = Vec::new();
                for (_, _, i) in &found {
                    let (start, end) = self.timed(kind, self.node(&loc_of(*i)))?;
                    order.push((start, end, *i));
                }
                order.sort();
                let first = loc_of(order[0].2);
                let joined = match kind {
                    Kind::Note => self.joined_notes(&order.iter().map(|(_, _, i)| loc_of(*i)).collect::<Vec<_>>())?,
                    _ => {
                        let mut acc = self.node(&first).clone();
                        for pair in order.windows(2) {
                            let (a, b) = (self.clip_name(&loc_of(pair[0].2)), self.clip_name(&loc_of(pair[1].2)));
                            let right = self.node(&loc_of(pair[1].2));
                            let Some(end) = &pair[0].1 else {
                                return Err(format!("Audio clip {a} plays to its file's end, so nothing goes on from it"));
                            };
                            if num_traits::ToPrimitive::to_f64(&(&pair[1].0 - end)).is_none_or(|d| d.abs() >= 1e-6) {
                                return Err(format!(
                                    "{b} does not start where {a} ends, at beat {}; clips joined into one meet there",
                                    beat_text(end)
                                ));
                            }
                            acc = self.merged(kind, &acc, right).ok_or_else(|| self.not_one_music(kind, &acc, right, &a, &b))?;
                        }
                        acc
                    }
                };
                let count = order.len();
                *self.node_mut(&first) = joined;
                let kept = tree::handle_at(self.root, &first);
                let mut gone: Vec<usize> = order[1..].iter().map(|(_, _, i)| *i).collect();
                gone.sort_unstable();
                for i in gone.into_iter().rev() {
                    self.take(&owner, key, i)?;
                }
                out.made.push(kept);
                out.label = format!("Join {count} clips into {}", self.clip_name(&tree::find(self.root, kept).unwrap_or(first)));
            }
            MidiImport { file, track, at, index } => {
                let path = std::path::Path::new(file);
                let shown = path.file_name().map_or_else(|| file.clone(), |n| n.to_string_lossy().into_owned());
                let bytes = std::fs::read(path).map_err(|e| format!("{shown}: {e}"))?;
                let part = crate::midi_file::read(&bytes).map_err(|e| format!("{shown}: {e}"))?;
                let start = at.as_ref().map(beat_at).transpose()?.unwrap_or_default();
                let meter = self.meter();
                let length = part.length(meter);
                let bar = meter.bar();
                let end = &start + &length;
                let song = exact(self.root.get("session").and_then(|s| s.get("length_beats")))
                    .unwrap_or_else(|| BigRational::from_integer(16.into()));
                if end > song {
                    // To the end of the bar the clip ends in, as for an audio clip.
                    let grown = (&end / &bar).ceil() * &bar;
                    self.run(&Set { path: "session.length_beats".into(), value: to_json(&beat_value(&grown)) })?;
                }
                let target = match track {
                    Some(t) => {
                        let loc = self.track(t)?;
                        if !self.is_midi(&loc) {
                            return Err(format!("A MIDI file's notes go on a MIDI track, and {} is not one", self.name(&loc)));
                        }
                        out.label = format!("Import {shown} to {}", self.name(&loc));
                        t.clone()
                    }
                    None => {
                        let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let taken: Vec<String> = ["tracks", "returns"]
                            .iter()
                            .flat_map(|k| self.root.get(k).map(Node::items).unwrap_or(&[]))
                            .filter_map(|i| i.node.field("id").map(str::to_string))
                            .collect();
                        let id = unique(&ident(&stem, "midi"), |n| taken.iter().any(|t| t == n));
                        let mut fields = Fields::new();
                        fields.insert("type".into(), Json::String("midi".into()));
                        let o = self.run(&TrackAdd { id: id.clone(), index: *index, fields })?;
                        out.made.extend(o.made);
                        out.label = format!("Import {shown} to new track {id}");
                        id
                    }
                };
                let notes: Vec<Json> = part
                    .notes
                    .iter()
                    .map(|n| {
                        serde_json::json!({
                            "pitch": n.pitch,
                            "at": to_json(&beat_value(&n.at)),
                            "duration": to_json(&beat_value(&n.duration)),
                            "velocity": n.velocity,
                        })
                    })
                    .collect();
                let mut fields = Fields::new();
                fields.insert("at".into(), to_json(&beat_value(&start)));
                fields.insert("length_beats".into(), to_json(&beat_value(&length)));
                fields.insert("notes".into(), Json::Array(notes));
                let o = self.run(&ClipAdd { track: target, fields })?;
                out.made.extend(o.made);
                let report = &mut out.report;
                report.insert("notes".into(), Json::from(part.notes.len()));
                report.insert("file_tempo".into(), part.tempo.map_or(Json::Null, |t| serde_json::json!((t * 1000.0).round() / 1000.0)));
                report.insert("left_out".into(), part.left_out_json());
                report.insert("adjusted".into(), part.adjusted_json());
            }
            NoteAdd { clip, notes, fields } => {
                let loc = self.note_clip(clip)?;
                let mut all: Vec<Fields> = notes.clone();
                if !fields.is_empty() {
                    all.push(fields.clone());
                }
                if all.is_empty() {
                    return Err("Give a note's fields, or notes".into());
                }
                for f in &all {
                    out.made.push(self.insert(&loc, "notes", None, map_node(f)?)?);
                }
                let mut clip_node = self.node(&loc).clone();
                self.name_notes(&mut clip_node);
                *self.node_mut(&loc) = clip_node;
                out.label = format!("Add {} to clip {}", count(all.len(), "note"), self.clip_name(&loc));
            }
            NoteSet { note, fields } => {
                let (clip, i) = self.member(note, "notes", "a note")?;
                let loc = [clip.clone(), vec![Step::Key("notes".into()), Step::Index(i)]].concat();
                if fields.contains_key("id") {
                    return Err("A note's ID cannot be changed".into());
                }
                self.merge(&loc, fields)?;
                let id = self.node(&loc).field("id").unwrap_or("?").to_string();
                out.label = format!("Change note {id} of clip {}", self.clip_name(&clip));
            }
            NoteMove { notes, by } => {
                let by = offset_at(by)?;
                let found = self.notes_of(notes)?;
                for (clip, i) in &found {
                    let loc = [clip.clone(), vec![Step::Key("notes".into()), Step::Index(*i)]].concat();
                    // A note moved before its clip is kept and does not play.
                    let at = exact(self.node(&loc).get("at")).unwrap_or_default() + &by;
                    self.set_leaf(&loc, "at", beat_value(&at));
                }
                out.label = format!("Move {} by {} beats", count(found.len(), "note"), signed(&aaw_model::fraction_str(&by)));
            }
            NoteTranspose { notes, by } => {
                let found = self.notes_of(notes)?;
                for (clip, i) in &found {
                    let loc = [clip.clone(), vec![Step::Key("notes".into()), Step::Index(*i)]].concat();
                    let pitch = number(self.node(&loc).get("pitch")).unwrap_or(f64::NAN) as i64 + by;
                    if !(0..=127).contains(&pitch) {
                        let id = self.node(&loc).field("id").unwrap_or("?").to_string();
                        return Err(format!("Note {id} of clip {} would be pitch {pitch}, outside 0 to 127", self.clip_name(clip)));
                    }
                    self.set_leaf(&loc, "pitch", Value::int(pitch));
                }
                out.label = format!("Transpose {} by {} semitones", count(found.len(), "note"), signed(&by.to_string()));
            }
            NoteRemove { notes } => {
                let mut found = self.notes_of(notes)?;
                // From the last, so each index still names its note.
                found.sort_by(|a, b| b.1.cmp(&a.1));
                for (clip, i) in &found {
                    self.take(clip, "notes", *i)?;
                }
                out.label = format!("Remove {}", count(found.len(), "note"));
            }
            InstrumentSet { track, instrument } => {
                let loc = self.midi_track(track)?;
                let had = !matches!(self.node(&loc).get("instrument"), None | Some(Node::Leaf(Value::None)));
                let value = json_value(instrument)?;
                let kind = match &value {
                    Value::Dict(d) => d
                        .iter()
                        .find(|(_, v)| !v.is_none())
                        .and_then(|(k, _)| k.as_str())
                        .unwrap_or("instrument")
                        .to_string(),
                    _ => "instrument".to_string(),
                };
                let removing = value.is_none();
                self.node_mut(&loc).map_mut().expect("a track").insert("instrument".into(), Node::new(&value));
                // Lanes on the synth's fields that the new instrument lacks go.
                self.drop_instrument_lanes(&loc)?;
                let name = self.name(&loc);
                out.label = match (had, removing) {
                    (_, true) => format!("Remove the instrument of {name}"),
                    (false, false) => format!("Attach a {kind} to {name}"),
                    (true, false) => format!("Replace the instrument of {name} with a {kind}"),
                };
            }
            InstrumentMap { track, fields } => {
                let loc = self.midi_track(track)?;
                let sampler = self.sampler(&loc)?;
                out.made.push(self.insert(&sampler, "map", None, map_node(fields)?)?);
                let notes = fields.get("notes").map_or_else(|| "?".to_string(), short);
                let pad = fields.get("pad").and_then(Json::as_str).unwrap_or("?");
                out.label = format!("Map notes {notes} to {pad} on {}", self.name(&loc));
            }
            SynthAdd { track, index, patch } => {
                let found = patch.as_deref().map(crate::patches::find).transpose()?;
                let instrument = match &found {
                    Some(p) => json!({"synth": to_json(&crate::patches::loaded(p))}),
                    None => json!({"synth": {"oscillators": {"a": {}}}}),
                };
                let with = found.as_ref().map_or(String::new(), |p| format!(" with {}", p.name));
                match self.track(track) {
                    Ok(loc) => {
                        if !self.is_midi(&loc) {
                            return Err(format!("{track} is not a MIDI track; a synth goes on one, or on a new track"));
                        }
                        let o = self.run(&InstrumentSet { track: track.clone(), instrument })?;
                        out.also.extend(o.also);
                        out.label = format!("Attach a Synth{with} to {}", self.name(&loc));
                    }
                    Err(_) => {
                        let mut fields = Fields::new();
                        fields.insert("type".into(), Json::String("midi".into()));
                        fields.insert("instrument".into(), instrument);
                        let o = self.run(&TrackAdd { id: track.clone(), index: *index, fields })?;
                        out.made.extend(o.made);
                        out.label = format!("Add Synth track {track}{with}");
                    }
                }
                if let Some(p) = &found {
                    out.report.insert("patch".into(), json!(p.name));
                    out.report.insert("factory".into(), json!(p.factory));
                }
            }
            PatchLoad { track, patch } => {
                let loc = self.midi_track(track)?;
                let p = crate::patches::find(patch)?;
                let instrument = json!({"synth": to_json(&crate::patches::loaded(&p))});
                let o = self.run(&InstrumentSet { track: track.clone(), instrument })?;
                out.also.extend(o.also);
                out.label = format!("Load patch {} into {}", p.name, self.name(&loc));
                out.report.insert("patch".into(), json!(p.name));
                out.report.insert("factory".into(), json!(p.factory));
            }
            PatchSave { track, name, description, tags, replace } => {
                let owner = self.midi_track(track)?;
                let loc = self.synth(track)?;
                let synth = crate::patches::validate(&self.node(&loc).value())?;
                // Written over, a patch keeps its description and tags unless new ones are given.
                let before = if *replace { crate::patches::find(name).ok().filter(|p| !p.factory) } else { None };
                let description = match (description, &before) {
                    (Some(d), _) => d.clone(),
                    (None, Some(b)) => b.description.clone(),
                    (None, None) => String::new(),
                };
                let tags = match (tags.is_empty(), &before) {
                    (true, Some(b)) => b.tags.clone(),
                    _ => tags.clone(),
                };
                let p = crate::patches::save(name, &description, &tags, &synth, self.origin.as_str(), *replace)?;
                let file = p.file.clone().unwrap_or_default();
                // The song says where the sound now comes from.
                if self.node(&loc).field("patch") != Some(p.name.as_str()) {
                    let o = self.run(&Set { path: format!("{}.patch", self.text(&loc)), value: json!(p.name) })?;
                    out.also.extend(o.also);
                }
                out.label = format!("Save patch {} from {}", p.name, self.name(&owner));
                out.report.insert("patch".into(), json!(p.name));
                out.report.insert("file".into(), json!(file));
                out.report.insert("shadows_factory".into(), json!(crate::patches::FACTORY.iter().any(|(s, _)| *s == p.slug)));
            }
            SynthSet { track, values } => {
                let loc = self.synth(track)?;
                if values.is_empty() {
                    return Err("Give a field's path and its value".into());
                }
                let base = self.text(&loc);
                let mut parts = Vec::new();
                for (field, value) in values {
                    let path = format!("{base}.{field}");
                    let o = if value.is_null() {
                        self.run(&Remove { path })?
                    } else {
                        self.run(&Set { path, value: value.clone() })?
                    };
                    out.also.extend(o.also);
                    parts.push(if value.is_null() { format!("{field} removed") } else { format!("{field} {}", short(value)) });
                }
                out.label = format!("Set synth of {track}: {}", parts.join(", "));
            }
            SynthMod { track, source, target, amount, remove } => {
                let loc = self.synth(track)?;
                let entries = self.list(&loc, "modulation")?;
                let found = entries.iter().position(|e| e.node.field("source") == Some(source.as_str()) && e.node.field("target") == Some(target.as_str()));
                if *remove {
                    let i = found.ok_or_else(|| format!("The synth of {track} has no entry from {source} to {target}"))?;
                    entries.remove(i);
                    out.label = format!("Remove modulation of {target} by {source} on {track}");
                } else {
                    let amount = amount
                        .as_ref()
                        .ok_or("Give the amount, or --remove")?
                        .as_f64()
                        .ok_or_else(|| format!("The amount must be a number, not {}", amount.as_ref().map_or(String::new(), short)))?;
                    match found {
                        Some(i) => {
                            set(&mut entries[i].node, "amount", Value::Float(amount));
                            out.label = format!("Set modulation of {target} by {source} to {amount} on {track}");
                        }
                        None => {
                            let entry = json_value(&json!({"source": source, "target": target, "amount": amount}))?;
                            out.made.push(self.insert(&loc, "modulation", None, Node::new(&entry))?);
                            out.label = format!("Modulate {target} by {source} by {amount} on {track}");
                        }
                    }
                }
            }
            AudioAdd { track, fields } => {
                let loc = self.track(track)?;
                out.made.push(self.insert(&loc, "audio", None, map_node(fields)?)?);
                let sample = fields.get("sample").and_then(Json::as_str).unwrap_or("?");
                out.label = format!("Add audio clip of {sample} to {}", self.name(&loc));
            }
            AudioMove { clip, track, at } => {
                let (owner, i) = self.member(clip, "audio", "an audio clip")?;
                let mut loc = [owner.clone(), vec![Step::Key("audio".into()), Step::Index(i)]].concat();
                if track.is_none() && at.is_none() {
                    return Err("Move needs a beat or a track".into());
                }
                if let Some(at) = at {
                    self.set_leaf(&loc, "at", beat_value(&beat_at(at)?));
                }
                if let Some(t) = track {
                    let dest = self.track(t)?;
                    if dest != owner {
                        let item = self.take(&owner, "audio", i)?;
                        let list = self.list(&dest, "audio")?;
                        list.push(item);
                        loc = [dest, vec![Step::Key("audio".into()), Step::Index(list.len() - 1)]].concat();
                    }
                }
                let sample = self.node(&loc).field("sample").unwrap_or("?").to_string();
                let beat = beat_text(&self.span(&loc)?.at);
                out.label = format!("Move audio clip {sample} to beat {beat} on {}", self.name(&loc[..2]));
            }
            AudioSplit { clip, at } => {
                let (owner, i) = self.member(clip, "audio", "an audio clip")?;
                let loc = [owner.clone(), vec![Step::Key("audio".into()), Step::Index(i)]].concat();
                let span = self.span(&loc)?;
                let at = beat_at(at)?;
                if at <= span.at || span.end_beat().is_some_and(|end| at >= end) {
                    return Err(format!("Beat {} is not inside audio clip {}", beat_text(&at), self.audio_name(&loc)));
                }
                let name = self.audio_name(&loc);
                let mut right = self.node(&loc).clone();
                if let Some(every) = &span.every {
                    // A looped clip is cut at a wrap: each half keeps the loop.
                    let cut = span.wrap(&at, every, &format!("audio clip {name}"))?;
                    self.set_leaf(&loc, "length_beats", beat_value(&cut));
                    let fields = right.map_mut().expect("an audio clip is an object");
                    fields.insert("at".into(), Node::Leaf(beat_value(&at)));
                    fields.insert("length_beats".into(), Node::Leaf(beat_value(&(span.length.clone().expect("a looped clip has a length") - &cut))));
                } else {
                    let cut = span.source(&at);
                    // The halves meet where the audio is continuous, so neither fades there.
                    self.set_leaf(&loc, "source_end_seconds", Value::Float(cut));
                    self.set_leaf(&loc, "fade_out_ms", Value::Float(0.0));
                    let fields = right.map_mut().expect("an audio clip is an object");
                    fields.insert("at".into(), Node::Leaf(beat_value(&at)));
                    fields.insert("source_start_seconds".into(), Node::Leaf(Value::Float(cut)));
                    fields.insert("fade_in_ms".into(), Node::Leaf(Value::Float(0.0)));
                }
                out.made.push(self.insert(&owner, "audio", Some(i + 1), right)?);
                out.label = format!("Split audio clip {name} at beat {}", beat_text(&at));
            }
            AudioTrim { clip, start, end } => {
                let (owner, i) = self.member(clip, "audio", "an audio clip")?;
                let loc = [owner, vec![Step::Key("audio".into()), Step::Index(i)]].concat();
                let span = self.span(&loc)?;
                let name = self.audio_name(&loc);
                if start.is_none() && end.is_none() {
                    return Err("Trim needs a start or an end".into());
                }
                if span.every.is_some() {
                    // A looped clip's end is how long it plays; its start is its loop's.
                    if start.is_some() {
                        return Err(format!("Audio clip {name} loops from its start, which cannot be trimmed; turn the loop off first (clip loop CLIP off)"));
                    }
                    let to = beat_at(end.as_ref().expect("an end"))?;
                    if to <= span.at {
                        return Err(format!("Audio clip {name} would end before it starts"));
                    }
                    self.set_leaf(&loc, "length_beats", beat_value(&(&to - &span.at)));
                    out.label = format!("Trim audio clip {name}");
                    return Ok(out);
                }
                let mut from = span.start;
                if let Some(start) = start {
                    let at = beat_at(start)?;
                    from = span.source(&at);
                    // A beat worked out from the file's start may fall a hair before it.
                    if from < -1e-6 || span.end.is_some_and(|end| from >= end) {
                        return Err(format!("Audio clip {name} has no audio at beat {}", beat_text(&at)));
                    }
                    from = from.max(0.0);
                    self.set_leaf(&loc, "at", beat_value(&at));
                    self.set_leaf(&loc, "source_start_seconds", Value::Float(from));
                }
                if let Some(end) = end {
                    let to = span.source(&beat_at(end)?);
                    if to <= from {
                        return Err(format!("Audio clip {name} would end before it starts"));
                    }
                    self.set_leaf(&loc, "source_end_seconds", Value::Float(to));
                }
                out.label = format!("Trim audio clip {name}");
            }
            AudioCrossfade { clip, ms, in_ms, lead_ms } => {
                let (owner, i) = self.member(clip, "audio", "an audio clip")?;
                let loc = [owner.clone(), vec![Step::Key("audio".into()), Step::Index(i)]].concat();
                let at = self.span(&loc)?.at;
                // The clip before it: the latest that starts earlier on the track.
                let mut before: Option<(usize, BigRational)> = None;
                for j in 0..self.list(&owner, "audio")?.len() {
                    let other = self.span(&[owner.clone(), vec![Step::Key("audio".into()), Step::Index(j)]].concat())?;
                    if other.at < at && before.as_ref().is_none_or(|(_, b)| other.at > *b) {
                        before = Some((j, other.at));
                    }
                }
                let Some((j, _)) = before else {
                    return Err(format!("No audio clip comes before {} on its track", self.audio_name(&loc)));
                };
                let leaving = [owner, vec![Step::Key("audio".into()), Step::Index(j)]].concat();
                self.crossfade(&leaving, &loc, *ms, *in_ms, *lead_ms);
                out.label = format!("Crossfade audio clips at beat {}", beat_text(&at));
            }
            AudioCut { track, from, to, ms, in_ms, lead_ms } => {
                let owner = self.track(track)?;
                let (from, to) = (beat_at(from)?, beat_at(to)?);
                if from >= to || from < BigRational::from_integer(0.into()) {
                    return Err("A cut runs from a beat to a later one".into());
                }
                let gap = &to - &from;
                let tempo = self.tempo();
                let old = std::mem::take(self.list(&owner, "audio")?);
                let mut kept: Vec<Item> = Vec::new();
                for item in old {
                    let span = Span::of(&item.node, tempo)?;
                    let end = span.end_beat();
                    if end.as_ref().is_some_and(|end| *end <= from) {
                        // Before the cut.
                        kept.push(item);
                    } else if span.at >= to {
                        // After it: earlier by its length.
                        let mut item = item;
                        set(&mut item.node, "at", beat_value(&(&span.at - &gap)));
                        kept.push(item);
                    } else if span.every.is_some() {
                        let name = item.node.field("sample").unwrap_or("?").to_string();
                        *self.list(&owner, "audio")? = kept;
                        return Err(format!(
                            "Audio clip {name} at {} loops across the cut; daw range delete cuts a looped clip at a wrap, or turn its loop off first",
                            beat_text(&span.at)
                        ));
                    } else {
                        // Across it: what it plays before the cut stays, and
                        // what it plays after starts where the cut began.
                        let mut handle = Some(item.handle);
                        if span.at < from {
                            let mut left = item.node.clone();
                            set(&mut left, "source_end_seconds", Value::Float(span.source(&from)));
                            kept.push(Item { handle: handle.take().expect("unused"), node: left });
                        }
                        if end.is_none_or(|end| end > to) {
                            let mut right = item.node;
                            set(&mut right, "at", beat_value(&from));
                            set(&mut right, "source_start_seconds", Value::Float(span.source(&to)));
                            let handle = handle.take().unwrap_or_else(|| {
                                let h = self.fresh();
                                out.made.push(h);
                                h
                            });
                            kept.push(Item { handle, node: right });
                        }
                    }
                }
                // The join: the clip that now ends where the cut began, and the
                // one that now starts there.
                let near = |end: &BigRational| num_traits::ToPrimitive::to_f64(&(end - &from)).is_some_and(|d| d.abs() < 1e-6);
                let spans: Vec<Span> = kept.iter().map(|item| Span::of(&item.node, tempo)).collect::<Result<_>>()?;
                let leaving = spans.iter().position(|s| s.at < from && s.end_beat().is_some_and(|end| near(&end)));
                let entering = spans.iter().position(|s| s.at == from);
                *self.list(&owner, "audio")? = kept;
                if let (Some(a), Some(b)) = (leaving, entering) {
                    let at = |i: usize| [owner.clone(), vec![Step::Key("audio".into()), Step::Index(i)]].concat();
                    self.crossfade(&at(a), &at(b), *ms, *in_ms, *lead_ms);
                }
                out.label = format!(
                    "Cut beats {} to {} from {} and close the gap",
                    beat_text(&from),
                    beat_text(&to),
                    self.name(&owner)
                );
            }
            PatternAdd { pattern, fields } => {
                let patterns = self.root.get_mut("patterns").and_then(Node::map_mut).expect("patterns");
                if patterns.contains_key(pattern) {
                    return Err(format!("Pattern {pattern} already exists"));
                }
                let node = with_empty(fields, &["events"], &["steps"])?;
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
                out.made.push(self.insert(&loc, "events", None, map_node(fields)?)?);
                out.label = format!("Add event to {pattern}");
            }
            EventSet { event, fields } => {
                let (owner, i) = self.member(event, "events", "a pattern event")?;
                let loc = [owner, vec![Step::Key("events".into()), Step::Index(i)]].concat();
                self.merge(&loc, fields)?;
                out.label = format!("Change event of {}", self.event_name(&loc));
            }
            EventRemove { event } => {
                let (owner, i) = self.member(event, "events", "a pattern event")?;
                let loc = [owner, vec![Step::Key("events".into()), Step::Index(i)]].concat();
                out.label = format!("Remove event of {}", self.event_name(&loc));
                self.remove_at(&loc)?;
            }
            PadAdd { track, pad, fields } => {
                let loc = self.pads(track)?;
                let node = map_node(fields)?;
                let pads = self.node_mut(&loc).map_mut().expect("pads");
                if pads.contains_key(pad) {
                    return Err(format!("{track} already has pad {pad}"));
                }
                pads.insert(pad.clone(), node);
                out.label = format!("Add pad {pad} to {track}");
            }
            PadSet { track, pad, fields } => {
                let loc = self.pad(track, pad)?;
                self.merge(&loc, fields)?;
                out.label = format!("Change pad {track}.{pad}");
            }
            PadRemove { track, pad } => {
                let loc = self.pad(track, pad)?;
                self.remove_at(&loc)?;
                out.label = format!("Remove pad {track}.{pad}");
            }
            EffectAdd { owner, index, fields } => {
                let loc = self.owner(owner)?;
                let at = index.unwrap_or(self.node(&loc).get("effects").map_or(0, |e| e.items().len()));
                out.made.push(self.insert(&loc, "effects", Some(at), map_node(fields)?)?);
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
                let loc = self.sender(track)?;
                let existing = self.list(&loc, "sends")?.iter().position(|s| s.node.field("to") == Some(to.as_str()));
                match existing {
                    Some(i) => {
                        let send = [loc.clone(), vec![Step::Key("sends".into()), Step::Index(i)]].concat();
                        self.merge(&send, fields)?;
                    }
                    None => {
                        let mut f = Fields::new();
                        f.insert("to".into(), Json::String(to.clone()));
                        f.extend(fields.clone());
                        out.made.push(self.insert(&loc, "sends", None, map_node(&f)?)?);
                    }
                }
                out.label = format!("Set send {} → {to}", self.name(&loc));
            }
            SendRemove { track, to } => {
                let sender = self.sender(track)?;
                let name = self.name(&sender);
                let loc = [sender.clone(), vec![Step::Key("sends".into())]].concat();
                let loc = self.at(&format!("{}.{to}", self.text(&loc)))?;
                self.remove_at(&loc)?;
                out.label = format!("Remove send {name} → {to}");
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
                        out.made.push(self.insert(&loc, "automation", None, Node::Map(lane))?);
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
                        out.made.push(self.insert(&lane, "points", Some(slot), point)?);
                    }
                    None => {
                        let mut lane = indexmap::IndexMap::new();
                        lane.insert("param".to_string(), Node::Leaf(Value::str(param)));
                        lane.insert("points".to_string(), Node::List(Vec::new()));
                        self.insert(&loc, "automation", None, Node::Map(lane))?;
                        let i = self.lane(&loc, param).expect("lane");
                        let lane = [loc.clone(), vec![Step::Key("automation".into()), Step::Index(i)]].concat();
                        out.made.push(self.insert(&lane, "points", None, point)?);
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
                // Named for its lane, which a person can find, not its handle.
                let param = self.node(&lane).field("param").unwrap_or_default().to_string();
                let owner = self.text(&lane[..lane.len().saturating_sub(2)]);
                out.label = format!("Change point of {owner} {param}");
            }
            PointRemove { point } => {
                let (lane, i) = self.member(point, "points", "an automation point")?;
                let param = self.node(&lane).field("param").unwrap_or_default().to_string();
                let owner = self.text(&lane[..lane.len().saturating_sub(2)]);
                let loc = [lane, vec![Step::Key("points".into()), Step::Index(i)]].concat();
                self.remove_at(&loc)?;
                out.label = format!("Remove point of {owner} {param}");
            }
            SectionAdd { id, at, length_beats } => {
                let mut f = Fields::new();
                f.insert("id".into(), Json::String(id.clone()));
                f.insert("at".into(), at.clone());
                f.insert("length_beats".into(), length_beats.clone());
                out.made.push(self.insert(&[], "sections", None, map_node(&f)?)?);
                out.label = format!("Add section {id}");
            }
            SectionMove { section, at, with_content: true } => {
                self.section_move_content(&mut out, section, &beat_at(at)?)?;
            }
            SectionMove { section, at, with_content: false } => {
                let loc = self.at(&format!("sections.{section}"))?;
                self.set_leaf(&loc, "at", json_value(at)?);
                out.label = format!("Move section {section} to {}", short(at));
            }
            SectionRemove { section, with_content: true } => {
                self.section_remove_content(&mut out, section)?;
            }
            SectionRemove { section, with_content: false } => {
                let loc = self.at(&format!("sections.{section}"))?;
                out.label = format!("Remove {}", self.remove_at(&loc)?);
            }
            SectionDuplicate { section, to, id } => {
                let to = to.as_ref().map(beat_at).transpose()?;
                self.section_duplicate(&mut out, section, to.as_ref(), id.as_deref())?;
            }
            RangeCopy { start, length, to, insert, tracks } => {
                self.range_copy(&mut out, &beat_at(start)?, &beat_at(length)?, &beat_at(to)?, *insert, tracks, None)?;
            }
            RangeInsert { at, length, tracks } => {
                self.range_insert(&mut out, &beat_at(at)?, &beat_at(length)?, tracks)?;
            }
            RangeDelete { start, length, tracks } => {
                self.range_delete(&mut out, &beat_at(start)?, &beat_at(length)?, tracks)?;
            }
            RangeClear { start, length, tracks } => {
                self.range_clear(&mut out, &beat_at(start)?, &beat_at(length)?, tracks)?;
            }
            Batch { commands, label } => {
                let mut labels = Vec::new();
                let mut also = Vec::new();
                for (n, c) in commands.iter().enumerate() {
                    if c.kind() != Kind::Edit || matches!(c, Apply { .. }) {
                        return Err(format!("batch command {n}: {} cannot be batched", c.op()));
                    }
                    let o = self.run(c).map_err(|e| format!("batch command {n} ({}): {e}", c.op()))?;
                    labels.push(o.label);
                    also.extend(o.also);
                    out.made.extend(o.made);
                    out.report.extend(o.report);
                }
                self.also = also;
                out.label = match (label, labels.len()) {
                    (Some(label), _) => label.clone(),
                    (None, 1) => labels.remove(0),
                    (None, n) => format!("Batch of {n}: {}", labels.join("; ")),
                };
            }
            Apply { .. } => return Err("apply replaces the whole song; the session applies it".into()),
            other => return Err(format!("{} is not an edit", other.op())),
        }
        // What this command noted itself, after what the commands it ran noted.
        out.also.extend(std::mem::take(&mut self.also));
        Ok(out)
    }

    /// The ID of the track or return at `loc`, for labels.
    pub(crate) fn name(&self, loc: &[Step]) -> String {
        self.node(loc).field("id").unwrap_or("?").to_string()
    }

    /// An event described by its pattern, pad and beat, which a person can
    /// find, for labels: `line: sub at 2`.
    fn event_name(&self, loc: &[Step]) -> String {
        let pattern = match loc {
            [Step::Key(_), Step::Key(name), ..] => name.as_str(),
            _ => "?",
        };
        let n = self.node(loc);
        format!(
            "{pattern}: {} at {}",
            n.field("pad").unwrap_or("?"),
            n.get("at").map_or_else(|| "?".into(), |a| short(&node_json(a)))
        )
    }

    /// A clip described by its pattern and track, for labels.
    pub(crate) fn tempo(&self) -> f64 {
        number(self.root.get("session").and_then(|s| s.get("tempo"))).unwrap_or(144.0)
    }

    /// The song's time signature, 4/4 when the song has none it reads.
    pub(crate) fn meter(&self) -> aaw_model::Meter {
        self.root
            .get("session")
            .and_then(|s| s.get("time_signature"))
            .and_then(|v| v.text())
            .and_then(|t| aaw_model::Meter::parse(t).ok())
            .unwrap_or_default()
    }

    /// Where the audio clip at `loc` is on the timeline and in its file.
    fn span(&self, loc: &[Step]) -> Result<Span> {
        Span::of(self.node(loc), self.tempo())
    }

    fn audio_name(&self, loc: &[Step]) -> String {
        let n = self.node(loc);
        format!(
            "{} at {}",
            n.field("sample").unwrap_or("?"),
            n.get("at").map_or_else(|| "?".into(), |a| short(&node_json(a)))
        )
    }

    /// The fades of a join: the entering clip starts `lead_ms` before its beat
    /// and fades in within that lead, and the leaving one fades out over `ms`
    /// from where the entering one starts.
    fn crossfade(&mut self, leaving: &[Step], entering: &[Step], ms: Option<f64>, in_ms: Option<f64>, lead_ms: Option<f64>) {
        let lead = lead_ms.unwrap_or(5.0);
        let out = ms.unwrap_or(12.0);
        let into = in_ms.unwrap_or_else(|| (lead - 1.0).max(0.0).min(out));
        self.set_leaf(entering, "lead_ms", Value::Float(lead));
        self.set_leaf(leaving, "fade_out_ms", Value::Float(out));
        self.set_leaf(entering, "fade_in_ms", Value::Float(into));
    }

    /// A clip described by its pattern, or a note clip by its ID, and its
    /// beat, for labels.
    fn clip_name(&self, loc: &[Step]) -> String {
        let n = self.node(loc);
        format!(
            "{} at {}",
            n.field("pattern").or_else(|| n.field("id")).or_else(|| n.field("sample")).unwrap_or("?"),
            n.get("at").map_or_else(|| "0".into(), |a| short(&node_json(a)))
        )
    }

    /// The next `clipN` no note clip of the song has, as the model gives.
    pub(crate) fn next_clip_id(&self) -> String {
        let ids: Vec<&str> = self
            .root
            .get("tracks")
            .map_or(&[][..], Node::items)
            .iter()
            .filter(|t| t.node.field("type") == Some("midi"))
            .flat_map(|t| t.node.get("clips").map_or(&[][..], Node::items))
            .filter_map(|c| c.node.field("id"))
            .collect();
        next_id(&ids, "clip")
    }

    /// One note clip of the note clips at `locs`, in the order they play:
    /// from the first's start to the last's end, with every note that played,
    /// where it played and cut where its clip or its loop cut it, a looped
    /// clip's repetitions laid out as notes. A note that did not play, before
    /// its clip or at or after its end or its loop's, is left out. The first
    /// clip's fields and note IDs are kept; a note of another clip keeps its
    /// ID where no note has it yet, and a repetition's copy is named anew.
    fn joined_notes(&mut self, locs: &[Loc]) -> Result<Node> {
        let starts: Vec<BigRational> = locs.iter().map(|l| exact(self.node(l).get("at")).unwrap_or_default()).collect();
        let lengths: Vec<BigRational> = locs.iter().map(|l| exact(self.node(l).get("length_beats")).unwrap_or_default()).collect();
        let start = starts.iter().min().cloned().unwrap_or_default();
        let end = starts.iter().zip(&lengths).map(|(s, l)| s + l).max().unwrap_or_default();
        let mut notes: Vec<(Option<u64>, Node)> = Vec::new();
        let mut ids: Vec<String> = Vec::new();
        for (k, loc) in locs.iter().enumerate() {
            let node = self.node(loc);
            let offset = &starts[k] - &start;
            let length = lengths[k].clone();
            let every = exact(node.get("loop_beats")).filter(|e| e.is_positive()).unwrap_or_else(|| length.clone());
            let mut base = BigRational::zero();
            while base < length {
                for n in node.get("notes").map_or(&[][..], Node::items) {
                    let at = exact(n.node.get("at")).unwrap_or_default();
                    let duration = exact(n.node.get("duration")).unwrap_or_default();
                    if at.is_negative() || at >= every || &base + &at >= length {
                        continue;
                    }
                    let stop = [&at + &duration, every.clone(), &length - &base].into_iter().min().unwrap_or_default();
                    if stop <= at {
                        continue;
                    }
                    let mut copy = n.node.clone();
                    set(&mut copy, "at", beat_value(&(&offset + &base + &at)));
                    set(&mut copy, "duration", beat_value(&(&stop - &at)));
                    let id = n.node.field("id").map(str::to_string);
                    let keeps = base.is_zero() && id.as_ref().is_some_and(|id| !ids.contains(id));
                    match (keeps, id) {
                        (true, Some(id)) => ids.push(id),
                        _ => {
                            if let Some(m) = copy.map_mut() {
                                m.shift_remove("id");
                            }
                        }
                    }
                    // A note of the first clip, played once, is the note it was.
                    let handle = (k == 0 && base.is_zero()).then_some(n.handle);
                    notes.push((handle, copy));
                }
                base += &every;
            }
        }
        let mut out = self.node(&locs[0]).clone();
        set(&mut out, "at", beat_value(&start));
        set(&mut out, "length_beats", beat_value(&(&end - &start)));
        if let Some(m) = out.map_mut() {
            m.shift_remove("loop_beats");
        }
        let items: Vec<Item> = notes
            .into_iter()
            .map(|(handle, node)| Item {
                handle: handle.unwrap_or_else(|| self.fresh()),
                node,
            })
            .collect();
        if let Some(m) = out.map_mut() {
            m.insert("notes".into(), Node::List(items));
        }
        self.name_notes(&mut out);
        Ok(out)
    }

    /// Why two clips that meet are not one music, for a refused join.
    fn not_one_music(&self, kind: crate::range::Kind, left: &Node, right: &Node, a: &str, b: &str) -> String {
        use crate::range::Kind;
        let same = |key: &str| left.get(key).map(node_json) == right.get(key).map(node_json);
        if exact(left.get("loop_beats")).is_some() || exact(right.get("loop_beats")).is_some() {
            return format!("{a} and {b} are not two halves of one loop that meet at a wrap; turn the loop off first (clip loop CLIP off)");
        }
        match kind {
            Kind::Pattern if !same("pattern") => format!("{a} and {b} play different patterns, so one clip cannot play what they play"),
            Kind::Pattern => format!("{a} and {b} are at different velocity scales, so one clip cannot play what they play"),
            Kind::Audio if !same("sample") => format!("{a} and {b} play different files, so one clip cannot play what they play"),
            Kind::Audio => {
                let (l, r) = (Span::of(left, self.tempo()), Span::of(right, self.tempo()));
                match (l, r) {
                    (Ok(l), Ok(r)) if l.end.is_none_or(|end| (end - r.start).abs() >= 1e-9) => format!(
                        "{b} does not go on in the file from where {a} leaves, so one clip cannot play what they play; a join is of clips that were one, as audio split leaves them"
                    ),
                    _ => format!("{a} and {b} differ in gain, fade curve, tempo or stretch, so one clip cannot play what they play"),
                }
            }
            Kind::Note => format!("{a} and {b} cannot be joined"),
        }
    }

    /// Gives a note clip's notes that have no ID the next free `nN`, as the
    /// model would, so that later commands of a batch can name them.
    fn name_notes(&self, clip: &mut Node) {
        let Some(notes) = clip.get_mut("notes").and_then(Node::items_mut) else { return };
        for i in 0..notes.len() {
            if notes[i].node.field("id").is_none() {
                let ids: Vec<&str> = notes.iter().filter_map(|n| n.node.field("id")).collect();
                let id = next_id(&ids, "n");
                set(&mut notes[i].node, "id", Value::Str(id));
            }
        }
    }

    pub(crate) fn is_midi(&self, track: &[Step]) -> bool {
        self.node(track).field("type") == Some("midi")
    }

    /// How many beats a pattern or note clip lasts: a note clip's length, a
    /// pattern clip's pattern by its repeats.
    pub(crate) fn clip_span(&self, clip: &Node) -> Option<BigRational> {
        if let Some(length) = exact(clip.get("length_beats")) {
            return Some(length);
        }
        let pattern = clip.field("pattern")?;
        let length = exact(self.root.get("patterns").and_then(|p| p.get(pattern)).and_then(|p| p.get("length_beats")))?;
        let repeats = match clip.get("repeats") {
            Some(Node::Leaf(Value::Int(n))) => BigRational::from_integer(n.clone()),
            None => BigRational::from_integer(1.into()),
            _ => return None,
        };
        Some(length * repeats)
    }

    /// The first beat at or after `beat` where no clip of `track` starts:
    /// past the run of clips that follow one another from it, each starting
    /// where the last ends; and the index of the last clip of the run, if any.
    fn past_run(&self, track: &[Step], mut beat: BigRational) -> (BigRational, Option<usize>) {
        let clips = self.node(track).get("clips").map_or(&[][..], Node::items);
        let mut last = None;
        loop {
            let ends = clips
                .iter()
                .enumerate()
                .filter(|(_, c)| exact(c.node.get("at")).unwrap_or_default() == beat)
                .filter_map(|(j, c)| self.clip_span(&c.node).map(|l| (&beat + l, j)))
                .max();
            match ends {
                Some((end, j)) if end > beat => (beat, last) = (end, Some(j)),
                _ => return (beat, last),
            }
        }
    }

    fn midi_track(&self, id: &str) -> Result<Loc> {
        let loc = self.track(id)?;
        if !self.is_midi(&loc) {
            return Err(format!("{id} is not a MIDI track"));
        }
        Ok(loc)
    }

    /// The audio clip a path names, if it names one.
    fn audio_clip(&self, path: &str) -> Option<Loc> {
        let (owner, i) = self.member(path, "audio", "an audio clip").ok()?;
        Some([owner, vec![Step::Key("audio".into()), Step::Index(i)]].concat())
    }

    /// The note clip a path names.
    fn note_clip(&self, path: &str) -> Result<Loc> {
        let (owner, i) = self.member(path, "clips", "a clip")?;
        if !self.is_midi(&owner) {
            return Err(format!("{path} is a pattern clip, not a note clip"));
        }
        Ok([owner, vec![Step::Key("clips".into()), Step::Index(i)]].concat())
    }

    /// The notes paths name, each as its clip and its index there: a note, or
    /// a note clip for all of its notes. Each note once.
    fn notes_of(&self, paths: &[String]) -> Result<Vec<(Loc, usize)>> {
        if paths.is_empty() {
            return Err("Name a note or a note clip".into());
        }
        let mut out: Vec<(Loc, usize)> = Vec::new();
        for path in paths {
            let loc = self.at(path)?;
            match loc.as_slice() {
                [clip @ .., Step::Key(k), Step::Index(i)] if k == "notes" => out.push((clip.to_vec(), *i)),
                [owner @ .., Step::Key(k), Step::Index(_)] if k == "clips" && self.is_midi(owner) => {
                    let n = self.node(&loc).get("notes").map_or(0, |n| n.items().len());
                    out.extend((0..n).map(|i| (loc.clone(), i)));
                }
                _ => return Err(format!("{path} is not a note or a note clip")),
            }
        }
        let mut seen = Vec::new();
        out.retain(|x| {
            let new = !seen.contains(x);
            seen.push(x.clone());
            new
        });
        Ok(out)
    }

    /// A MIDI track's sampler, made empty where the track has no instrument.
    fn sampler(&mut self, track: &[Step]) -> Result<Loc> {
        let instrument = self.node(track).get("instrument");
        if matches!(instrument, None | Some(Node::Leaf(Value::None))) {
            let empty = aaw_model::json_value(r#"{"sampler": {"pads": {}, "map": []}}"#).map_err(|e| e.to_string())?;
            self.node_mut(track).map_mut().expect("a track").insert("instrument".into(), Node::new(&empty));
        }
        let loc = [track.to_vec(), vec![Step::Key("instrument".into()), Step::Key("sampler".into())]].concat();
        match self.node(track).get("instrument").and_then(|i| i.get("sampler")) {
            Some(Node::Map(_)) => Ok(loc),
            _ => Err(format!("{} has an instrument that is not a sampler", self.name(track))),
        }
    }

    /// A MIDI track's synth.
    fn synth(&self, track: &str) -> Result<Loc> {
        let loc = self.midi_track(track)?;
        match self.node(&loc).get("instrument").and_then(|i| i.get("synth")) {
            Some(Node::Map(_)) => Ok([loc, vec![Step::Key("instrument".into()), Step::Key("synth".into())]].concat()),
            _ => Err(format!("{track} has no synth; daw synth add attaches one")),
        }
    }

    /// Removes a track's lanes on `instrument.FIELD` whose field its
    /// instrument no longer has, after the instrument is replaced.
    fn drop_instrument_lanes(&mut self, track: &[Step]) -> Result<()> {
        // The instrument as validation fills it in, so a lane on a field at
        // its default stays.
        let synth = self
            .node(track)
            .get("instrument")
            .and_then(|i| aaw_model::Instrument::parse(&i.value()).ok())
            .and_then(|i| i.synth().cloned());
        let name = self.text(track);
        let lanes = self.list(track, "automation")?;
        let mut notes = Vec::new();
        lanes.retain(|lane| {
            let Some(param) = lane.node.field("param") else { return true };
            let Some(field) = param.strip_prefix("instrument.") else { return true };
            let kept = synth.as_ref().is_some_and(|s| aaw_model::rules::synth_param(s, field).is_ok());
            if !kept {
                notes.push(format!("{name}: removed lane {param}"));
            }
            kept
        });
        self.also.extend(notes);
        Ok(())
    }

    /// Where a track's pads are: its own, or its sampler's on a MIDI track,
    /// which is made if the track has no instrument.
    fn pads(&mut self, track: &str) -> Result<Loc> {
        let loc = self.track(track)?;
        if self.is_midi(&loc) {
            let sampler = self.sampler(&loc)?;
            return Ok([sampler, vec![Step::Key("pads".into())]].concat());
        }
        Ok([loc, vec![Step::Key("pads".into())]].concat())
    }

    fn pad(&self, track: &str, pad: &str) -> Result<Loc> {
        let loc = self.track(track)?;
        if self.is_midi(&loc) {
            return self.at(&format!("{}.instrument.sampler.pads.{pad}", self.text(&loc)));
        }
        self.at(&format!("tracks.{track}.pads.{pad}"))
    }
}

/// `prefix` and one more than the highest number among IDs of that form.
fn next_id(ids: &[&str], prefix: &str) -> String {
    let n = ids
        .iter()
        .filter_map(|id| id.strip_prefix(prefix))
        .filter(|n| !n.is_empty() && !n.starts_with('0') && n.bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|n| n.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    format!("{prefix}{}", n + 1)
}

/// `stem` if it is free, else the first free of `stem-2`, `stem-3`, ….
pub fn unique(stem: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(stem) {
        return stem.to_string();
    }
    (2..).map(|n| format!("{stem}-{n}")).find(|name| !taken(name)).expect("a free name")
}

/// A name as an ID allows it: lower-case letters, digits, `-` and `_`,
/// starting with a letter, and not too long to read in a header. A name
/// with nothing of that is `fallback`.
pub fn ident(name: &str, fallback: &str) -> String {
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
        "s" | "" => fallback.to_string(),
        name => name.to_string(),
    }
}

/// "1 note", "3 notes".
fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

/// A number with its sign: +12, -1/3.
fn signed(text: &str) -> String {
    if text.starts_with('-') {
        text.to_string()
    } else {
        format!("+{text}")
    }
}
