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
    Group { key: u64 },
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

/// A pattern's event as Copy took it: its pad, and its place and length as
/// the song wrote them.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct EventCopy {
    pub pad: String,
    pub at: String,
    pub duration: Option<String>,
    pub velocity: u32,
    pub note: Option<String>,
    pub transpose: f64,
}

/// Where an automation point goes, as a drag of several puts each.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PointPlace {
    pub point: u64,
    pub at: f64,
    pub value: f64,
}

/// A field of the Synth with the value to set it to, by its path in the
/// patch, for an edit that sets several at once, as a drag of the filter's
/// corner sets the cutoff and the resonance.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SynthFieldValue {
    pub field: String,
    pub value: FieldValue,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Edit {
    /// The session tempo in quarter notes per minute.
    Tempo { bpm: f64 },
    /// The song's time signature as written, `3/4`; the host refuses one
    /// the model does not read.
    TimeSignature { text: String },
    /// Volume in dB: of a track, a group, a return or, for the master, the song.
    Gain { row: Row, db: f64 },
    Pan { row: Row, pan: f64 },
    Mute { row: Row, on: bool },
    /// Solo of a track or a group.
    Solo { row: Row, on: bool },
    /// A send's level in dB, of a track or a group; the send is added if the
    /// row has none there.
    Send { row: Row, to: String, db: f64 },
    SendRemove { row: Row, to: String },
    /// Groups tracks, as ⌘G does: a new group under a free name holding the
    /// tracks, which are moved together to where the first of them is. The
    /// group is what the edit makes.
    GroupAdd { tracks: Vec<u64> },
    /// Takes a group away, as ⇧⌘G does; its tracks go to the master again.
    Ungroup { group: u64 },
    /// Puts a track in a group, or with none takes it out of its group.
    TrackGroup { track: u64, group: Option<u64> },
    /// Moves clips later by `by` beats and down by `rows` tracks: pattern
    /// clips and audio clips alike. The song grows to hold an audio clip
    /// that would end past its end, here and in the edits below.
    ClipsMove { clips: Vec<u64>, by: f64, rows: i32 },
    ClipRepeats { clip: u64, repeats: u32 },
    /// Copies clips to right after the span they cover together, so that a
    /// group repeats as a group. The copies are what the edit makes.
    ClipsDuplicate { clips: Vec<u64> },
    /// Copies clips to as far from them as `ClipsMove` would move them,
    /// leaving them where they are, as an Option-drag does. The copies are
    /// what the edit makes.
    ClipsCopy { clips: Vec<u64>, by: f64, rows: i32 },
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
    /// Makes one clip of clips of one track that plays what they played, as
    /// ⌘J does: note clips into one note clip from the first's start to the
    /// last's end with every note that played, loops unrolled; pattern clips
    /// and audio clips only where they meet and are one music. The clip kept
    /// is what the edit makes.
    ClipsJoin { clips: Vec<u64> },
    /// Loops a note clip or an audio clip every so many beats, as typed, until
    /// its end; `off` or nothing plays it once again.
    ClipLoop { clip: u64, beats: String },
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
    /// Starts `grabbed` at a beat of its clip, on the nearest line of `grid`
    /// unless `free`, and the other notes as much later or earlier; each
    /// note's end stays where it is.
    NotesStart {
        notes: Vec<u64>,
        grabbed: u64,
        start: f64,
        free: bool,
        grid: String,
    },
    /// Copies notes of one clip to as far from them as `NotesMove` would move
    /// them, leaving them where they are. The copies are what the edit makes.
    NotesCopy {
        notes: Vec<u64>,
        steps: i32,
        grid: String,
        by: f64,
        semitones: i32,
    },
    /// Changes the velocity of notes by `by`, each within 1 to 127.
    NotesVelocity { notes: Vec<u64>, by: i32 },
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
    /// Attach an empty sampler, or create a MIDI track with one.
    InstrumentAdd { track: Option<u64> },
    /// Attaches a Synth to a MIDI track, in place of the instrument it had,
    /// or creates a MIDI track with one: the plain saw, or the patch
    /// `patch` names, after which a new track is named. What the browser's
    /// Synth and its patches do when dropped.
    SynthAdd { track: Option<u64>, patch: Option<String> },
    /// Loads a sample that `library::import` copied into the project into
    /// the Sampler on a MIDI track: one pad, named after `name`, played on
    /// every note at its pitch, as it is at middle C. Loaded over a sample
    /// the Sampler has, it takes that one's place and keeps the pad's
    /// settings, apart from the start and the end, which go back to the
    /// whole file. A track with no instrument gets a Sampler of it.
    SamplerLoad {
        track: u64,
        asset: crate::library::Asset,
        name: String,
    },
    /// Sets a field of the Sampler's pad, named as its `FieldView` names
    /// it. An absent `end_seconds` plays to the file's end.
    SamplerSet { track: u64, field: String, value: FieldValue },
    /// Sets the root note of the sample the Sampler plays, as typed, `C4`
    /// or `60`, or takes it off, so that the keys play the sample as it is
    /// at middle C. The note is the sample's, so every pad of it follows.
    SamplerRoot { track: u64, note: Option<String> },
    /// Sets a field of a MIDI track's Synth, named by its path in the
    /// patch as its `FieldView` names it, such as `filter.cutoff_hz` or
    /// `oscillators.a.wave`. An absent `phase` is a random one.
    SynthSet { track: u64, field: String, value: FieldValue },
    /// Sets several fields of a MIDI track's Synth as one step, as a drag of
    /// a drawn curve does: the filter's cutoff and resonance together, or an
    /// envelope's decay and sustain.
    SynthSetFields { track: u64, fields: Vec<SynthFieldValue> },
    /// Adds a part to a MIDI track's Synth under the next free name:
    /// `oscillators` (`a` to `d`, the plain saw), `envelopes` (`env2` on, a
    /// decay to nothing, for the matrix), `lfos` (`lfo1` on) or `macros`
    /// (`macro1` on, at 0).
    SynthPartAdd { track: u64, part: String },
    /// Takes a part off a MIDI track's Synth: `part` as above and the
    /// part's name. The host refuses the last oscillator, and a part the
    /// matrix names.
    SynthPartRemove { track: u64, part: String, name: String },
    /// Adds an entry to the Synth's matrix, from a source to a target in the
    /// target's unit, or sets the amount of the entry that is already there.
    SynthModAdd {
        track: u64,
        source: String,
        target: String,
        amount: f64,
    },
    /// Sets the amount of an entry of the Synth's matrix, by its place in
    /// the list.
    SynthModSet { track: u64, index: u32, amount: f64 },
    /// Takes an entry out of the Synth's matrix, by its place in the list.
    SynthModRemove { track: u64, index: u32 },
    /// Adds an effect of `kind` to the chain inside a MIDI track's Synth, at
    /// `index` or its end, as `daw effect add` on the synth's path does; the
    /// effect is then edited, moved, bypassed and removed as any effect is.
    SynthEffectAdd { track: u64, kind: String, index: Option<u32> },
    /// Saves a MIDI track's Synth to the library as a patch named `name`,
    /// as `daw patch save` does, and names the song's patch after it. Over
    /// a patch already saved under that name only with `replace`, which
    /// keeps the file's description and tags unless new ones are given.
    PatchSave {
        track: u64,
        name: String,
        description: Option<String>,
        tags: Vec<String>,
        replace: bool,
    },
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
    /// Moves a track among the tracks, a group among the groups or a return
    /// among the returns.
    Move { row: Row, index: u32 },
    /// Adds an effect of a type to a row's chain, at `index` or last, with
    /// its required fields at a place to start. The effect is what the edit
    /// makes.
    EffectAdd { row: Row, kind: String, index: Option<u32> },
    /// Adds a copy of an effect right after it in its chain, with every
    /// field it has and no id, as ⌘D does. The copy is what the edit makes.
    /// Only an effect of a track, a return or the master; a Synth's own
    /// effects are not copied.
    EffectDuplicate { effect: u64 },
    /// Adds a copy of an effect to a row's chain, at `index` or its end, as an
    /// Option-drag onto a header does. The copy keeps the effect's id unless
    /// that chain has it already. The copy is what the edit makes.
    EffectCopy { effect: u64, row: Row, index: Option<u32> },
    /// Adds the effect `copied` took, at `index` of a row's chain or its end,
    /// as `EffectCopy` does; it works after the effect is changed or gone.
    /// The effect is what the edit makes.
    EffectPaste { copied: String, row: Row, index: Option<u32> },
    EffectRemove { effect: u64 },
    /// Moves an effect within its chain.
    EffectMove { effect: u64, index: u32 },
    EffectBypass { effect: u64, on: bool },
    /// Sets a field of an effect, named as its `FieldView` names it.
    EffectSet { effect: u64, field: String, value: FieldValue },
    /// Adds a band to an equalizer: a bell at `freq_hz` and `gain_db`, or
    /// at the place to start when they are left out.
    BandAdd { effect: u64, freq_hz: Option<f64>, gain_db: Option<f64> },
    /// Removes an equalizer's band, with the lanes that move it; lanes on
    /// later bands follow them down.
    BandRemove { effect: u64, band: u32 },
    /// Sets an equalizer band's frequency, gain and q together, as a drag
    /// of its point does.
    BandSet { effect: u64, band: u32, freq_hz: f64, gain_db: f64, q: f64 },
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
    /// Moves points, of one lane or several, each to its place, as one step.
    PointsSet { points: Vec<PointPlace> },
    /// Removes points; a lane's last point takes the lane with it.
    PointsRemove { points: Vec<u64> },
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
    /// Moves events of one pattern later by `steps` grid steps and `by`
    /// beats, and up by `semitones` from each one's note, or from its
    /// sample's root when it has none; in pitch, only those that have one,
    /// unless none does.
    EventsMove {
        events: Vec<u64>,
        steps: i32,
        by: f64,
        semitones: i32,
    },
    /// Copies events of one pattern to as far from them as `EventsMove`
    /// would move them, leaving them where they are. The copies are what the
    /// edit makes.
    EventsCopy {
        events: Vec<u64>,
        steps: i32,
        by: f64,
        semitones: i32,
    },
    /// Copies events of one pattern to right after the span they cover
    /// together. The copies are what the edit makes.
    EventsDuplicate { events: Vec<u64> },
    /// Adds copied events to a pattern, the earliest at `at`, a beat of the
    /// pattern, and the others as far after it as they were; without `at`,
    /// right after the span they covered. The copies are what the edit makes.
    EventsPaste { events: Vec<EventCopy>, pattern: String, at: Option<f64> },
    /// Ends an event on a line of the pattern's grid: it is held from where
    /// it starts to there.
    EventEnd { event: u64, step: u32 },
    /// Sets fields of events. Beats are as typed, such as `1/3` or `0.75`;
    /// an empty duration or note takes the field away.
    EventsSet {
        events: Vec<u64>,
        at: Option<String>,
        duration: Option<String>,
        velocity: Option<u32>,
        transpose: Option<f64>,
        note: Option<String>,
    },
    EventsRemove { events: Vec<u64> },
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
    /// Adds a sample that `library::import` copied into the project as the
    /// instrument of a MIDI track, in place of the one it had, or, without a
    /// track, of a new MIDI track at `index`: a Sampler of one pad, named
    /// after `name` as far as IDs allow, played on every note at its pitch,
    /// as it is at middle C (D66). A sample the browser measured a pitch of
    /// (the asset has a root note) starts Held, so a note stops at its
    /// note-off; any other plays to its end (D85). A new track is what the
    /// edit makes. Never a pad: a sample lands as this or as an audio clip.
    SamplerAdd {
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
        Row::Group { key } => Ok(format!("groups.{}.{field}", handle_text(*key))),
        Row::Return { key } => Ok(format!("returns.{}.{field}", handle_text(*key))),
        Row::Master if field == "gain_db" => Ok("session.master_gain_db".into()),
        Row::Master => Err(format!("The master has no {field}")),
    }
}

/// The handle of a row that sends: a track or a group.
fn sender(row: &Row) -> Result<u64> {
    match row {
        Row::Track { key } | Row::Group { key } => Ok(*key),
        Row::Return { .. } => Err("A return has no sends".into()),
        Row::Master => Err("The master has no sends".into()),
    }
}

/// Whether a track, group or return has the name.
fn taken(project: &Project, name: &str) -> bool {
    project.tracks.iter().any(|t| t.id == name) || project.groups.iter().any(|g| g.id == name) || project.returns.iter().any(|r| r.id == name)
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
                // A looped clip lasts its own length; another, its audio's.
                let played = match clip.length_exact() {
                    Some(length) => length.to_f64().unwrap_or(0.0),
                    None => (self.source_end(clip)? - clip.source_start_seconds) / seconds_per_beat(clip, tempo),
                };
                let beats = played + tail_beats(clip, tempo, self.seconds(&clip.sample));
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
        Row::Group { key } => format!("groups.{}", handle_text(*key)),
        Row::Return { key } => format!("returns.{}", handle_text(*key)),
        Row::Master => "master".into(),
    }
}

/// The owner a row names.
fn owner<'a>(project: &'a Project, tree: &Node, row: &Row) -> Result<Owner<'a>> {
    let place = |list: &str, key: u64| items(tree, list).iter().position(|i| i.handle == key);
    match row {
        Row::Track { key } => place("tracks", *key).map(|i| Owner::Track(&project.tracks[i])),
        Row::Group { key } => place("groups", *key).map(|i| Owner::Group(&project.groups[i])),
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
            "groups" => Some((Owner::Group(project.groups.get(*i)?), *j)),
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
        (TargetKind::Instrument, Owner::Track(x)) => x
            .midi
            .as_ref()
            .and_then(|m| m.synth())
            .and_then(|s| aaw_model::rules::synth_value(s, &t.field))
            .ok_or_else(missing),
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

/// The first of `track-1`, `track-2`, … that no track, group or return has.
fn free_name(project: &Project, stem: &str) -> String {
    (1..).map(|n| format!("{stem}-{n}")).find(|name| !taken(project, name)).expect("a free name")
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

/// Events of one pattern, which an edit of several is of: the pattern's name,
/// the pattern and each event with its key.
fn pattern_events<'a>(project: &'a Project, tree: &Node, keys: &[u64], verb: &str) -> Result<(&'a str, &'a aaw_model::Pattern, Vec<(u64, &'a aaw_model::Event)>)> {
    let mut found = Vec::new();
    let mut of: Option<(&str, &aaw_model::Pattern)> = None;
    for key in keys {
        let (name, p, e) = event(project, tree, *key)?;
        if of.is_some_and(|(n, _)| n != name) {
            return Err(format!("Events are {verb} within one pattern"));
        }
        of = Some((name, p));
        found.push((*key, e));
    }
    let (name, p) = of.ok_or("No events were given")?;
    Ok((name, p, found))
}

/// How far `steps` steps of a pattern's grid and `by` beats go, exactly;
/// none when it is nowhere.
fn event_shift(p: &aaw_model::Pattern, steps: i32, by: f64) -> Result<Option<BigRational>> {
    if steps == 0 && by == 0.0 {
        return Ok(None);
    }
    let distance = aaw_model::beat(&Beat::Float(by.abs()))?;
    let by = if by < 0.0 { -distance } else { distance };
    Ok(Some(p.grid_exact() * BigRational::from_integer(steps.into()) + by))
}

/// The pitch each event is moved up from by `semitones`: its note, or else
/// its sample's root, and none for an event that has neither, which then
/// keeps its place in pitch. Refused when no event has a pitch.
fn pitched_events(project: &Project, name: &str, found: &[(u64, &aaw_model::Event)], semitones: i32) -> Result<std::collections::HashMap<u64, Option<i64>>> {
    if semitones == 0 {
        return Ok(Default::default());
    }
    let mut pitches = std::collections::HashMap::new();
    for (key, e) in found {
        let from = match e.note.as_deref().filter(|n| !n.is_empty()) {
            Some(note) => Some(midi(note)?),
            None => pad_root(project, name, &e.pad),
        };
        pitches.insert(*key, from);
    }
    if pitches.values().all(Option::is_none) {
        let pad = &found[0].1.pad;
        return Err(format!("Pad {pad} plays a sample without a root note, so its events have no pitch"));
    }
    Ok(pitches)
}

/// The command that adds a copy of an event to its pattern at `at`, with
/// its fields as the song wrote them.
fn event_add(tree: &Node, name: &str, key: u64, at: &BigRational) -> Result<Json> {
    let loc = tree::find(tree, key).ok_or("The event is no longer in the song")?;
    let mut command = node_json(tree::get(tree, &loc));
    let fields = command.as_object_mut().ok_or("An event is an object")?;
    fields.retain(|_, value| !value.is_null());
    fields.insert("op".into(), json!("event.add"));
    fields.insert("pattern".into(), json!(name));
    fields.insert("at".into(), beat(at));
    Ok(command)
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

/// The MIDI track a Sampler device is on, and its pad once a sample is
/// loaded. A track with no instrument counts as an empty Sampler, since a
/// sample dropped there makes one; a sampler of several pads is refused.
fn sampler_device<'a>(project: &'a Project, tree: &Node, key: u64) -> Result<(&'a aaw_model::Track, Option<(&'a str, &'a aaw_model::Pad)>)> {
    let place = items(tree, "tracks").iter().position(|i| i.handle == key).ok_or("The track is no longer in the song")?;
    let t = &project.tracks[place];
    let midi = t.midi.as_ref().ok_or_else(|| format!("{} is not a MIDI track", t.id))?;
    if midi.instrument.is_none() {
        return Ok((t, None));
    }
    let loaded = crate::view::device(t).ok_or_else(|| format!("The sampler on {} is a kit of several pads, which `daw pad set` edits", t.id))?;
    Ok((t, loaded))
}

/// `notes` and `grabbed`, each once, in that order.
fn with_grabbed(notes: &[u64], grabbed: u64) -> Vec<u64> {
    notes.iter().copied().chain(std::iter::once(grabbed)).fold(Vec::new(), |mut seen, k| {
        if !seen.contains(&k) {
            seen.push(k);
        }
        seen
    })
}

/// The ID of the track at `key`, for a command that names tracks by ID,
/// such as `note.preview`.
pub fn track_id(doc: &Doc, key: u64) -> Result<String> {
    let tree = doc.tree();
    let place = items(&tree, "tracks").iter().position(|i| i.handle == key).ok_or("The track is no longer in the song")?;
    Ok(doc.project.tracks[place].id.clone())
}

/// The MIDI track a Synth is on, and its patch.
fn synth_track<'a>(project: &'a Project, tree: &Node, key: u64) -> Result<(&'a aaw_model::Track, &'a aaw_model::Synth)> {
    let place = items(tree, "tracks").iter().position(|i| i.handle == key).ok_or("The track is no longer in the song")?;
    let t = &project.tracks[place];
    let synth = t.midi.as_ref().and_then(|m| m.synth()).ok_or_else(|| format!("{} has no Synth", t.id))?;
    Ok((t, synth))
}

/// A MIDI track by its key.
fn midi_track<'a>(project: &'a Project, tree: &Node, key: u64) -> Result<&'a aaw_model::Track> {
    let place = items(tree, "tracks").iter().position(|i| i.handle == key).ok_or("The track is no longer in the song")?;
    let t = &project.tracks[place];
    if t.midi.is_none() {
        return Err(format!("{} is not a MIDI track; a Synth goes on one", t.id));
    }
    Ok(t)
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
        Edit::TimeSignature { text } => Ok(vec![json!({"op": "set", "path": "session.time_signature", "value": text.trim()})]),
        Edit::Gain { row, db } => set(row, "gain_db", number(*db)),
        Edit::Pan { row, pan } => set(row, "pan", number(*pan)),
        Edit::Mute { row, on } => set(row, "mute", json!(on)),
        Edit::Solo { row, on } => match row {
            Row::Track { .. } | Row::Group { .. } => set(row, "solo", json!(on)),
            Row::Return { .. } => Err("A return has no solo".into()),
            Row::Master => Err("The master has no solo".into()),
        },
        Edit::Send { row, to, db } => Ok(vec![json!({
            "op": "send.set", "track": handle_text(sender(row)?), "to": to, "gain_db": number(*db),
        })]),
        Edit::SendRemove { row, to } => Ok(vec![json!({"op": "send.remove", "track": handle_text(sender(row)?), "to": to})]),
        Edit::GroupAdd { tracks } => {
            let tree = doc.tree();
            let names: Vec<&str> = tracks
                .iter()
                .map(|key| items(&tree, "tracks").iter().find(|i| i.handle == *key).and_then(|i| i.node.field("id")).ok_or_else(|| "A track is no longer in the song".to_string()))
                .collect::<Result<_>>()?;
            let id = free_name(project, "group");
            let handles: Vec<String> = tracks.iter().map(|k| handle_text(*k)).collect();
            Ok(batch(
                vec![json!({"op": "group.add", "id": id, "tracks": handles})],
                format!("Group {} as {id}", names.join(", ")),
            ))
        }
        Edit::Ungroup { group } => Ok(vec![json!({"op": "group.remove", "group": handle_text(*group)})]),
        Edit::TrackGroup { track, group } => {
            let tree = doc.tree();
            let name = |list: &str, key: u64| items(&tree, list).iter().find(|i| i.handle == key).and_then(|i| i.node.field("id").map(str::to_string));
            let track_name = name("tracks", *track).ok_or("The track is no longer in the song")?;
            let path = format!("tracks.{}.group", handle_text(*track));
            match group {
                Some(key) => {
                    let id = name("groups", *key).ok_or("The group is no longer in the song")?;
                    Ok(batch(vec![json!({"op": "set", "path": path, "value": id})], format!("Put {track_name} in group {id}")))
                }
                None => Ok(batch(vec![json!({"op": "remove", "path": path})], format!("Take {track_name} out of its group"))),
            }
        }
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
        Edit::ClipsCopy { clips, by, rows } => {
            if clips.is_empty() || (*by == 0.0 && *rows == 0) {
                return Ok(Vec::new());
            }
            let tree = doc.tree();
            let spans = clips.iter().map(|key| placed(&tree, *key)).collect::<Result<Vec<_>>>()?;
            let distance = aaw_model::beat(&Beat::Float(by.abs()))?;
            let by = if *by < 0.0 { -distance } else { distance };
            if spans.iter().any(|s| s.start.clone() + &by < BigRational::zero()) {
                return Err("A copy cannot start before the song".into());
            }
            song.copies(&tree, clips, &spans, &by, (*rows).into(), "Copy")
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
        Edit::ClipsJoin { clips } => {
            if clips.len() < 2 {
                return Ok(Vec::new());
            }
            Ok(vec![json!({"op": "clip.join", "clips": clips.iter().map(|key| clip(key)).collect::<Vec<_>>()})])
        }
        Edit::ClipLength { clip: key, beats } => {
            let tree = doc.tree();
            let (_, c) = note_clip(project, &tree, *key)?;
            if typed_exact(beats)? == c.length_exact() {
                return Ok(Vec::new());
            }
            Ok(vec![json!({"op": "clip.resize", "clip": clip(key), "length_beats": typed_beat(beats)})])
        }
        Edit::ClipLoop { clip: key, beats } => {
            let tree = doc.tree();
            let at = placed(&tree, *key)?;
            if !(at.notes || at.audio.is_some()) {
                return Err("A pattern clip repeats; a note clip or an audio clip loops".into());
            }
            let off = beats.trim().is_empty() || beats.trim().eq_ignore_ascii_case("off");
            let loop_beats = if off { Json::Null } else { typed_beat(beats) };
            Ok(vec![json!({"op": "clip.loop", "clip": clip(key), "loop_beats": loop_beats})])
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
            for key in with_grabbed(notes, *grabbed) {
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
        Edit::NotesStart { notes, grabbed, start, free, grid } => {
            let tree = doc.tree();
            let (_, n) = note(project, &tree, *grabbed)?;
            let grid = grid_exact(grid)?;
            let start = if *free {
                aaw_model::signed_beat(&Beat::Float((start * 1000.0).round() / 1000.0))?
            } else {
                let line = (start / grid.to_f64().unwrap_or(1.0)).round();
                BigRational::from_float(line).unwrap_or_default() * &grid
            };
            let by = start - n.at_exact();
            if by.is_zero() {
                return Ok(Vec::new());
            }
            let mut commands = Vec::new();
            for key in with_grabbed(notes, *grabbed) {
                let (_, n) = note(project, &tree, key)?;
                let duration = n.duration_exact() - &by;
                if !duration.is_positive() {
                    return Err(format!("Note {} would start after it ends", n.id));
                }
                let at = n.at_exact() + &by;
                commands.push(json!({"op": "note.set", "note": handle_text(key), "at": signed_beat(&at), "duration": beat(&duration)}));
            }
            Ok(match commands.len() {
                1 => commands,
                n => batch(commands, format!("Move the start of {n} notes")),
            })
        }
        Edit::NotesCopy { notes, steps, grid, by, semitones } => {
            let tree = doc.tree();
            let found = notes.iter().map(|key| note(project, &tree, *key)).collect::<Result<Vec<_>>>()?;
            let Some((clip_key, _)) = found.first() else { return Ok(Vec::new()) };
            if found.iter().any(|(c, _)| c != clip_key) {
                return Err("Notes are copied within one clip".into());
            }
            let distance = aaw_model::beat(&Beat::Float(by.abs()))?;
            let by = if *by < 0.0 { -distance } else { distance };
            let shift = grid_exact(grid)? * BigRational::from_integer((*steps).into()) + by;
            let mut copies = Vec::new();
            for (_, n) in &found {
                let pitch = n.pitch + i64::from(*semitones);
                if !(0..=127).contains(&pitch) {
                    return Err(format!("A copy of note {} would be outside MIDI notes 0 to 127", n.id));
                }
                copies.push(json!({
                    "pitch": pitch, "at": signed_beat(&(n.at_exact() + &shift)),
                    "duration": beat(&n.duration_exact()), "velocity": n.velocity,
                }));
            }
            let label = format!("Copy {}", count(found.len(), "note"));
            Ok(batch(vec![json!({"op": "note.add", "clip": handle_text(*clip_key), "notes": copies})], label))
        }
        Edit::NotesVelocity { notes, by } => {
            let tree = doc.tree();
            let mut commands = Vec::new();
            for key in notes {
                let (_, n) = note(project, &tree, *key)?;
                let velocity = (n.velocity + i64::from(*by)).clamp(1, 127);
                if velocity != n.velocity {
                    commands.push(json!({"op": "note.set", "note": handle_text(*key), "velocity": velocity}));
                }
            }
            Ok(match commands.len() {
                0 | 1 => commands,
                n => batch(commands, format!("Change the velocity of {n} notes")),
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
        Edit::InstrumentAdd { track } => {
            let instrument = json!({"sampler": {"pads": {}, "map": []}});
            Ok(match track {
                Some(key) => batch(vec![json!({"op": "instrument.set", "track": handle_text(*key), "instrument": instrument})], "Attach Sampler".into()),
                None => batch(vec![json!({"op": "track.add", "id": free_name(project, "sampler"), "type": "midi", "instrument": instrument})], "Add Sampler track".into()),
            })
        }
        Edit::SynthAdd { track, patch } => {
            let found = patch.as_deref().map(aaw_host::patches::find).transpose()?;
            Ok(match (track, &found) {
                (Some(key), Some(p)) => {
                    let t = midi_track(project, &doc.tree(), *key)?;
                    batch(
                        vec![json!({"op": "patch.load", "track": handle_text(*key), "patch": p.name})],
                        format!("Load patch {} into {}", p.name, t.id),
                    )
                }
                (Some(key), None) => {
                    let t = midi_track(project, &doc.tree(), *key)?;
                    batch(
                        vec![json!({"op": "synth.add", "track": handle_text(*key)})],
                        format!("Attach a Synth to {}", t.id),
                    )
                }
                (None, Some(p)) => {
                    let taken = |name: &str| taken(project, name);
                    let id = unique(&ident(&p.name, "synth"), taken);
                    batch(
                        vec![json!({"op": "synth.add", "track": id, "patch": p.name})],
                        format!("Add Synth track {id} with {}", p.name),
                    )
                }
                (None, None) => {
                    let id = free_name(project, "synth");
                    batch(vec![json!({"op": "synth.add", "track": id})], format!("Add Synth track {id}"))
                }
            })
        }
        Edit::InstrumentRemove { track } => Ok(vec![json!({"op": "instrument.set", "track": handle_text(*track), "instrument": null})]),
        Edit::SamplerLoad { track: key, asset, name } => {
            let tree = doc.tree();
            let (t, loaded) = sampler_device(project, &tree, *key)?;
            let stem = ident(name, "sample");
            // A sample new to the song plays as it is at middle C (D66).
            let unrooted = crate::library::Asset { root_note: None, ..asset.clone() };
            let (sample, mut commands) = listed(project, &unrooted, &stem);
            // The pad's settings carry over; the start and the end are the new file's.
            let mut pad = serde_json::Map::new();
            if let Some((_, old)) = loaded {
                for f in describe::PAD {
                    if matches!(f.name, "start_seconds" | "end_seconds") {
                        continue;
                    }
                    let value = match f.name {
                        "mode" => json!(old.mode.as_str()),
                        "gain_db" => number(old.gain_db),
                        "pan" => number(old.pan),
                        "transpose" => number(old.transpose),
                        "attack_ms" => number(old.attack_ms),
                        "release_ms" => number(old.release_ms),
                        "reverse" => json!(old.reverse),
                        _ => continue,
                    };
                    pad.insert(f.name.to_string(), value);
                }
            }
            pad.insert("sample".into(), json!(sample));
            let instrument = json!({"sampler": {
                "pads": {stem.clone(): pad},
                "map": [{"notes": [0, 127], "pad": stem, "pitched": true}],
            }});
            commands.push(json!({"op": "instrument.set", "track": handle_text(*key), "instrument": instrument}));
            Ok(batch(commands, format!("Load {sample} into the Sampler on {}", t.id)))
        }
        Edit::SamplerSet { track: key, field, value } => {
            let tree = doc.tree();
            let (t, loaded) = sampler_device(project, &tree, *key)?;
            let (pad, _) = loaded.ok_or("The Sampler has no sample yet")?;
            let spec = describe::PAD.iter().find(|f| f.name == field).ok_or_else(|| format!("A Sampler's {field} is not set here"))?;
            let path = format!("tracks.{}.instrument.sampler.pads.{pad}.{field}", handle_text(*key));
            let command = match value {
                FieldValue::Absent => json!({"op": "remove", "path": path}),
                value => json!({"op": "set", "path": path, "value": field_json(value)}),
            };
            Ok(batch(vec![command], format!("Set the Sampler's {} on {}", spec.label.to_lowercase(), t.id)))
        }
        Edit::SynthSet { track: key, field, value } => {
            let tree = doc.tree();
            let (t, synth) = synth_track(project, &tree, *key)?;
            let (label, _) = crate::view::synth_label(synth, field);
            let path = format!("tracks.{}.instrument.synth.{field}", handle_text(*key));
            let command = match value {
                FieldValue::Absent => json!({"op": "remove", "path": path}),
                value => json!({"op": "set", "path": path, "value": field_json(value)}),
            };
            Ok(batch(vec![command], format!("Set the Synth's {} on {}", label.to_lowercase(), t.id)))
        }
        Edit::SynthSetFields { track: key, fields } => {
            let tree = doc.tree();
            let (t, synth) = synth_track(project, &tree, *key)?;
            if fields.is_empty() {
                return Ok(Vec::new());
            }
            let mut values = serde_json::Map::new();
            let mut labels = Vec::new();
            for f in fields {
                values.insert(f.field.clone(), field_json(&f.value));
                labels.push(crate::view::synth_label(synth, &f.field).0.to_lowercase());
            }
            let named = match labels.len() {
                1 => labels[0].clone(),
                2 => format!("{} and {}", labels[0], labels[1]),
                n => format!("{} and {}", labels[..n - 1].join(", "), labels[n - 1]),
            };
            Ok(batch(
                vec![json!({"op": "synth.set", "track": handle_text(*key), "values": values})],
                format!("Set the Synth's {named} on {}", t.id),
            ))
        }
        Edit::SynthPartAdd { track: key, part } => {
            let tree = doc.tree();
            let (t, synth) = synth_track(project, &tree, *key)?;
            let (what, names, value): (&str, Vec<String>, Json) = match part.as_str() {
                "oscillators" => ("oscillator", ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect(), json!({})),
                "envelopes" => (
                    "envelope",
                    (2..=aaw_model::Synth::MAX_ENVELOPES).map(|n| format!("env{n}")).collect(),
                    json!({"attack_ms": 0, "decay_ms": 300, "sustain_percent": 0, "release_ms": 300}),
                ),
                "lfos" => ("LFO", (1..=aaw_model::Synth::MAX_LFOS).map(|n| format!("lfo{n}")).collect(), json!({})),
                "macros" => ("macro", (1..=aaw_model::Synth::MAX_MACROS).map(|n| format!("macro{n}")).collect(), json!(0)),
                other => return Err(format!("A Synth has no part {other}; its parts are oscillators, envelopes, lfos and macros")),
            };
            let taken = |name: &str| match part.as_str() {
                "oscillators" => synth.oscillators.contains_key(name),
                "envelopes" => synth.envelopes.contains_key(name) || synth.lfos.contains_key(name),
                "lfos" => synth.lfos.contains_key(name) || synth.envelopes.contains_key(name),
                _ => synth.macros.contains_key(name),
            };
            let name = names
                .iter()
                .find(|n| !taken(n))
                .ok_or_else(|| format!("The Synth on {} has as many {what}s as it can", t.id))?;
            let mut values = serde_json::Map::new();
            values.insert(format!("{part}.{name}"), value);
            Ok(batch(
                vec![json!({"op": "synth.set", "track": handle_text(*key), "values": values})],
                format!("Add {what} {name} to the Synth on {}", t.id),
            ))
        }
        Edit::SynthPartRemove { track: key, part, name } => {
            let tree = doc.tree();
            let (t, _) = synth_track(project, &tree, *key)?;
            let what = match part.as_str() {
                "oscillators" => "oscillator",
                "envelopes" => "envelope",
                "lfos" => "LFO",
                "macros" => "macro",
                other => return Err(format!("A Synth has no part {other}; its parts are oscillators, envelopes, lfos and macros")),
            };
            if part == "envelopes" && name == "amp" {
                return Err("The amp envelope is always there: it shapes the level".into());
            }
            let mut values = serde_json::Map::new();
            values.insert(format!("{part}.{name}"), Json::Null);
            Ok(batch(
                vec![json!({"op": "synth.set", "track": handle_text(*key), "values": values})],
                format!("Remove {what} {name} from the Synth on {}", t.id),
            ))
        }
        Edit::SynthEffectAdd { track: key, kind, index } => {
            let tree = doc.tree();
            let (t, _) = synth_track(project, &tree, *key)?;
            let mut command = json!({"op": "effect.add", "owner": format!("{}.instrument.synth", handle_text(*key)), "type": kind});
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
            // Inside a patch a delay or reverb sits under the voices.
            if matches!(kind.as_str(), "delay" | "reverb") {
                fields.insert("mix_percent".into(), json!(25));
            }
            Ok(batch(vec![command], format!("Add {kind} to the Synth on {}", t.id)))
        }
        Edit::SynthModAdd { track: key, source, target, amount } => {
            let tree = doc.tree();
            let (t, synth) = synth_track(project, &tree, *key)?;
            let had = synth.modulation.iter().any(|m| &m.source == source && &m.target == target);
            let verb = if had { format!("Set modulation of {target} by {source} to {amount}") } else { format!("Modulate {target} by {source} by {amount}") };
            Ok(batch(
                vec![json!({"op": "synth.mod", "track": handle_text(*key), "source": source, "target": target, "amount": number(*amount)})],
                format!("{verb} on {}", t.id),
            ))
        }
        Edit::SynthModSet { track: key, index, amount } => {
            let tree = doc.tree();
            let (t, synth) = synth_track(project, &tree, *key)?;
            let m = synth.modulation.get(*index as usize).ok_or("The matrix has no such entry")?;
            Ok(batch(
                vec![json!({"op": "synth.mod", "track": handle_text(*key), "source": m.source, "target": m.target, "amount": number(*amount)})],
                format!("Set modulation of {} by {} to {amount} on {}", m.target, m.source, t.id),
            ))
        }
        Edit::SynthModRemove { track: key, index } => {
            let tree = doc.tree();
            let (t, synth) = synth_track(project, &tree, *key)?;
            let m = synth.modulation.get(*index as usize).ok_or("The matrix has no such entry")?;
            let path = format!("tracks.{}.instrument.synth.modulation.{index}", handle_text(*key));
            Ok(batch(vec![json!({"op": "remove", "path": path})], format!("Remove modulation of {} by {} on {}", m.target, m.source, t.id)))
        }
        Edit::PatchSave { track: key, name, description, tags, replace } => {
            let tree = doc.tree();
            synth_track(project, &tree, *key)?;
            let mut command = json!({"op": "patch.save", "track": handle_text(*key), "name": name, "tags": tags, "replace": replace});
            if let Some(d) = description {
                command["description"] = json!(d);
            }
            Ok(vec![command])
        }
        Edit::SamplerRoot { track: key, note } => {
            let tree = doc.tree();
            let (_, loaded) = sampler_device(project, &tree, *key)?;
            let (_, pad) = loaded.ok_or("The Sampler has no sample yet")?;
            let sample = pad.sample.clone();
            let path = format!("samples.{sample}.root_note");
            let note = note.as_deref().map(str::trim).filter(|n| !n.is_empty());
            Ok(match note {
                // A number or a name, kept as a name, as the song writes it.
                Some(typed) => {
                    let name = match typed.parse::<i64>() {
                        Ok(n) => note_name(n)?,
                        Err(_) => note_name(midi(typed)?)?,
                    };
                    batch(vec![json!({"op": "set", "path": path, "value": name})], format!("Set the root note of {sample} to {name}"))
                }
                None => batch(vec![json!({"op": "remove", "path": path})], format!("Take the root note off {sample}")),
            })
        }
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
            Row::Group { key } => Ok(vec![json!({"op": "group.rename", "group": handle_text(*key), "to": to})]),
            Row::Return { key } => Ok(vec![json!({"op": "return.rename", "return": handle_text(*key), "to": to})]),
            Row::Master => Err("The master cannot be renamed".into()),
        },
        Edit::Remove { row } => match row {
            Row::Track { key } => Ok(vec![json!({"op": "track.remove", "track": handle_text(*key)})]),
            Row::Group { key } => Ok(vec![json!({"op": "group.remove", "group": handle_text(*key)})]),
            Row::Return { key } => Ok(vec![json!({"op": "return.remove", "return": handle_text(*key)})]),
            Row::Master => Err("The master cannot be removed".into()),
        },
        Edit::Move { row, index } => match row {
            Row::Track { key } => Ok(vec![json!({"op": "track.move", "track": handle_text(*key), "index": index})]),
            Row::Group { key } => Ok(vec![json!({"op": "group.move", "group": handle_text(*key), "index": index})]),
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
        Edit::EffectDuplicate { effect } => {
            let tree = doc.tree();
            let (mut fields, row, index) = effect_taken(&tree, *effect)?;
            fields.as_object_mut().expect("an object").remove("id");
            effect_copy(project, &tree, fields, &row, Some(index as u32 + 1), "Duplicate")
        }
        Edit::EffectCopy { effect, row, index } => {
            let tree = doc.tree();
            let (fields, _, _) = effect_taken(&tree, *effect)?;
            effect_copy(project, &tree, fields, row, *index, "Copy")
        }
        Edit::EffectPaste { copied, row, index } => {
            let tree = doc.tree();
            let fields: Json = serde_json::from_str(copied).map_err(|e| e.to_string())?;
            if !fields.is_object() {
                return Err("Nothing was copied".into());
            }
            effect_copy(project, &tree, fields, row, *index, "Paste")
        }
        Edit::EffectRemove { effect } => Ok(vec![json!({"op": "effect.remove", "effect": handle_text(*effect)})]),
        Edit::EffectMove { effect, index } => Ok(vec![json!({"op": "effect.move", "effect": handle_text(*effect), "index": index})]),
        Edit::EffectBypass { effect, on } => Ok(vec![json!({"op": "effect.bypass", "effect": handle_text(*effect), "bypass": on})]),
        Edit::EffectSet { effect, field, value } => Ok(vec![json!({
            "op": "set", "path": format!("{}.{field}", handle_text(*effect)), "value": field_json(value),
        })]),
        Edit::BandAdd { effect, freq_hz, gain_db } => {
            let tree = doc.tree();
            let loc = tree::find(&tree, *effect).ok_or("The effect is no longer in the song")?;
            let mut bands = match tree::get(&tree, &loc).get("bands") {
                Some(bands) => aaw_host::command::node_json(bands),
                None => return Err("Only an equalizer has bands".into()),
            };
            let mut band = start(describe::BAND);
            if let Some(hz) = freq_hz {
                band.insert("freq_hz".into(), number(*hz));
            }
            if let Some(db) = gain_db {
                band.insert("gain_db".into(), number(*db));
            }
            bands.as_array_mut().ok_or("Only an equalizer has bands")?.push(Json::Object(band));
            Ok(vec![json!({"op": "set", "path": format!("{}.bands", handle_text(*effect)), "value": bands})])
        }
        Edit::BandSet { effect, band, freq_hz, gain_db, q } => Ok([("freq_hz", freq_hz), ("gain_db", gain_db), ("q", q)]
            .iter()
            .map(|(field, value)| json!({"op": "set", "path": format!("{}.bands.{band}.{field}", handle_text(*effect)), "value": number(**value)}))
            .collect()),
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
        Edit::PointsSet { points } => {
            let commands = points
                .iter()
                .map(|p| Ok(json!({"op": "point.set", "point": handle_text(p.point), "at": beat_at(p.at)?, "value": number(p.value)})))
                .collect::<Result<Vec<_>>>()?;
            Ok(match commands.len() {
                0 | 1 => commands,
                n => batch(commands, format!("Move {n} automation points")),
            })
        }
        Edit::PointsRemove { points } => {
            // The host takes a lane away with its last point.
            let commands: Vec<Json> = points.iter().map(|key| json!({"op": "point.remove", "point": handle_text(*key)})).collect();
            Ok(match commands.len() {
                0 | 1 => commands,
                n => batch(commands, format!("Remove {n} automation points")),
            })
        }
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
        Edit::EventsMove { events, steps, by, semitones } => {
            if *steps == 0 && *by == 0.0 && *semitones == 0 {
                return Ok(Vec::new());
            }
            let tree = doc.tree();
            let (name, p, found) = pattern_events(project, &tree, events, "moved")?;
            let shift = event_shift(p, *steps, *by)?;
            let pitched = pitched_events(project, name, &found, *semitones)?;
            let mut commands = Vec::new();
            for (key, e) in &found {
                let mut command = json!({"op": "event.set", "event": handle_text(*key)});
                if let Some(shift) = &shift {
                    let at = e.at_exact() + shift;
                    if at < BigRational::zero() {
                        return Err("An event cannot start before its pattern".into());
                    }
                    command["at"] = beat(&at);
                }
                if let Some(Some(from)) = pitched.get(key) {
                    command["note"] = json!(note_name(from + i64::from(*semitones))?);
                }
                if command.as_object().is_some_and(|c| c.len() > 2) {
                    commands.push(command);
                }
            }
            Ok(match commands.len() {
                0 | 1 => commands,
                n => batch(commands, format!("Move {n} events of {name}")),
            })
        }
        Edit::EventsCopy { events, steps, by, semitones } => {
            let tree = doc.tree();
            let (name, p, found) = pattern_events(project, &tree, events, "copied")?;
            let shift = event_shift(p, *steps, *by)?.unwrap_or_else(BigRational::zero);
            let pitched = pitched_events(project, name, &found, *semitones)?;
            let commands = found
                .iter()
                .map(|(key, e)| {
                    let at = e.at_exact() + &shift;
                    if at < BigRational::zero() {
                        return Err("A copy cannot start before its pattern".to_string());
                    }
                    let mut command = event_add(&tree, name, *key, &at)?;
                    if let Some(Some(from)) = pitched.get(key) {
                        command["note"] = json!(note_name(from + i64::from(*semitones))?);
                    }
                    Ok(command)
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(match commands.len() {
                0 => commands,
                n => batch(commands, format!("Copy {} of {name}", count(n, "event"))),
            })
        }
        Edit::EventsDuplicate { events } => {
            let tree = doc.tree();
            let (name, p, found) = pattern_events(project, &tree, events, "copied")?;
            // A hit without a length takes a step of the grid.
            let (Some(start), Some(end)) = (
                found.iter().map(|(_, e)| e.at_exact()).min(),
                found.iter().map(|(_, e)| e.at_exact() + e.duration_exact().unwrap_or_else(|| p.grid_exact())).max(),
            ) else {
                return Ok(Vec::new());
            };
            let span = end - start;
            let commands = found.iter().map(|(key, e)| event_add(&tree, name, *key, &(e.at_exact() + &span))).collect::<Result<Vec<_>>>()?;
            let label = format!("Duplicate {} of {name}", count(commands.len(), "event"));
            Ok(batch(commands, label))
        }
        Edit::EventsPaste { events, pattern: name, at } => {
            let p = pattern(project, name)?;
            let places = events.iter().map(|e| typed_exact(&e.at)).collect::<Result<Vec<_>>>()?;
            let durations = events
                .iter()
                .map(|e| e.duration.as_deref().map(typed_exact).transpose())
                .collect::<Result<Vec<_>>>()?;
            let Some(start) = places.iter().min() else { return Ok(Vec::new()) };
            let ends = places.iter().zip(&durations).map(|(at, d)| at + d.clone().unwrap_or_else(|| p.grid_exact()));
            let end = ends.max().unwrap_or_else(|| start.clone());
            let shift = match at {
                Some(at) => aaw_model::beat(&Beat::Float((at.max(0.0) * 1000.0).round() / 1000.0))? - start,
                None => end - start,
            };
            let commands: Vec<Json> = events
                .iter()
                .zip(&places)
                .zip(&durations)
                .map(|((e, at), duration)| {
                    let mut command = json!({"op": "event.add", "pattern": name, "at": beat(&(at + &shift)), "pad": e.pad});
                    if e.velocity != 100 {
                        command["velocity"] = json!(e.velocity);
                    }
                    if let Some(note) = e.note.as_deref().filter(|n| !n.is_empty()) {
                        command["note"] = json!(note);
                    }
                    if let Some(duration) = duration {
                        command["duration"] = beat(duration);
                    }
                    if e.transpose != 0.0 {
                        command["transpose"] = number(e.transpose);
                    }
                    command
                })
                .collect();
            let label = format!("Paste {} into {name}", count(commands.len(), "event"));
            Ok(batch(commands, label))
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
        Edit::EventsSet { events, at, duration, velocity, transpose, note } => {
            let mut fields = serde_json::Map::new();
            if let Some(at) = at {
                fields.insert("at".into(), typed_beat(at));
            }
            if let Some(duration) = duration {
                fields.insert("duration".into(), if duration.trim().is_empty() { Json::Null } else { typed_beat(duration) });
            }
            if let Some(velocity) = velocity {
                fields.insert("velocity".into(), json!(velocity));
            }
            if let Some(transpose) = transpose {
                fields.insert("transpose".into(), number(*transpose));
            }
            if let Some(note) = note {
                fields.insert("note".into(), if note.trim().is_empty() { Json::Null } else { json!(note.trim()) });
            }
            if fields.is_empty() {
                return Ok(Vec::new());
            }
            let commands: Vec<Json> = events
                .iter()
                .map(|key| {
                    let mut command = json!({"op": "event.set", "event": handle_text(*key)});
                    command.as_object_mut().expect("an object").extend(fields.clone());
                    command
                })
                .collect();
            Ok(match commands.len() {
                0 | 1 => commands,
                n => batch(commands, format!("Change {n} events")),
            })
        }
        Edit::EventsRemove { events } => {
            let commands: Vec<Json> = events.iter().map(|key| json!({"op": "event.remove", "event": handle_text(*key)})).collect();
            Ok(match commands.len() {
                0 | 1 => commands,
                n => batch(commands, format!("Remove {n} events")),
            })
        }
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
        Edit::SamplerAdd { asset, name, track, index } => {
            let tree = doc.tree();
            let stem = ident(name, "sample");
            // A new sample plays as it is at middle C, as in Ableton, whatever
            // pitch it measures at; one the song has keeps the root note the
            // song gives it (D66). A sample measured as one pitch is played
            // as an instrument, so it starts Held; a hit plays whole (D85).
            let unrooted = crate::library::Asset { root_note: None, ..asset.clone() };
            let (sample, mut commands) = listed(project, &unrooted, &stem);
            let mut pad = json!({"sample": sample});
            if asset.root_note.is_some() {
                pad["mode"] = json!("gate");
            }
            let instrument = json!({"sampler": {
                "pads": {stem.clone(): pad},
                "map": [{"notes": [0, 127], "pad": stem, "pitched": true}],
            }});
            let label = match track {
                Some(key) => {
                    let place = items(&tree, "tracks").iter().position(|i| i.handle == *key);
                    let t = &project.tracks[place.ok_or("The track is no longer in the song")?];
                    let midi = t.midi.as_ref().ok_or_else(|| format!("{} is not a MIDI track: a sample lands on its lane as an audio clip", t.id))?;
                    commands.push(json!({"op": "instrument.set", "track": handle_text(*key), "instrument": instrument}));
                    match midi.instrument.as_ref() {
                        Some(_) => format!("Replace the instrument of {} with a Sampler of {sample}", t.id),
                        None => format!("Attach a Sampler of {sample} to {}", t.id),
                    }
                }
                None => {
                    let taken = |n: &str| taken(project, n);
                    let id = unique(&stem, taken);
                    commands.push(json!({
                        "op": "track.add", "id": id, "type": "midi", "instrument": instrument,
                        "index": (*index as usize).min(project.tracks.len()),
                    }));
                    format!("Add track {id} with a Sampler of {sample}")
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
                    let taken = |n: &str| taken(project, n);
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

/// An effect's fields as Copy takes them, as JSON, with the row it is on and
/// its place in the chain. Only an effect of a track, a return or the
/// master: a Synth's own effects live in its patch.
fn effect_taken(tree: &Node, effect: u64) -> Result<(Json, Row, usize)> {
    let loc = tree::find(tree, effect).ok_or("The effect is no longer in the song")?;
    let elsewhere = || "Only an effect on a track, a return or the master is copied; a Synth's own effects stay in its patch".to_string();
    let (row, index) = match loc.as_slice() {
        [Step::Key(list), Step::Index(i), Step::Key(l), Step::Index(j)] if l == "effects" => {
            let key = items(tree, list).get(*i).ok_or_else(elsewhere)?.handle;
            let row = match list.as_str() {
                "tracks" => Row::Track { key },
                "groups" => Row::Group { key },
                "returns" => Row::Return { key },
                _ => return Err(elsewhere()),
            };
            (row, *j)
        }
        [Step::Key(m), Step::Key(l), Step::Index(j)] if m == "master" && l == "effects" => (Row::Master, *j),
        _ => return Err(elsewhere()),
    };
    let mut fields = node_json(tree::get(tree, &loc));
    fields.as_object_mut().ok_or("An effect is an object")?.retain(|_, value| !value.is_null());
    Ok((fields, row, index))
}

/// The command that adds an effect with `fields` to a row's chain, at
/// `index` or its end, as a copy: its id goes when the chain has it already.
/// Named for `verb`, as "Duplicate compressor on tracks.drums".
fn effect_copy(project: &Project, tree: &Node, mut fields: Json, row: &Row, index: Option<u32>, verb: &str) -> Result<Vec<Json>> {
    let chain = owner(project, tree, row)?.effects();
    let map = fields.as_object_mut().expect("an object");
    if let Some(id) = map.get("id").and_then(Json::as_str) {
        if chain.iter().any(|e| e.id() == Some(id)) {
            map.remove("id");
        }
    }
    let kind = map.get("type").and_then(Json::as_str).unwrap_or("effect").to_string();
    map.insert("op".into(), json!("effect.add"));
    map.insert("owner".into(), json!(owner_path(row)));
    if let Some(i) = index {
        map.insert("index".into(), json!(i.min(chain.len() as u32)));
    }
    let place = match row {
        Row::Master => "master".to_string(),
        Row::Track { key } | Row::Group { key } | Row::Return { key } => tree::path_text(tree, &tree::find(tree, *key).ok_or("The row is no longer in the song")?),
    };
    let on = if verb == "Copy" { "to" } else { "on" };
    Ok(batch(vec![fields], format!("{verb} {kind} {on} {place}")))
}

/// What Copy takes of an effect, for `Edit::EffectPaste`: its fields as
/// JSON, so that a paste adds an effect like it after it is changed or gone.
pub fn copied_effect(doc: &Doc, effect: u64) -> Result<String> {
    let (fields, _, _) = effect_taken(&doc.tree(), effect)?;
    Ok(fields.to_string())
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
        Edit::BandSet { band, .. } => return Some(format!("Move band {} of an equalizer", band + 1)),
        _ if count < 2 => return None,
        Edit::ClipsMove { .. } => "Move",
        Edit::ClipsDuplicate { .. } => "Duplicate",
        Edit::ClipsCopy { .. } => "Copy",
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
