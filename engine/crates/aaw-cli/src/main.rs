//! The `daw` binary: JSON in and out over the Rust model, engine and host.
//!
//! Commands that read or change a song go to the host that owns it when one is
//! running (`daw host`, or `daw play` while it plays), so the change is heard and
//! recorded in its history. Otherwise they run headless: load, apply, save, exit.
//!
//! The sample library and the perception tools are Python: `samples`,
//! `reference`, `listen`, `compare` and `check` run there, with this process's
//! arguments and output.
//!
//! Results print to stdout as JSON. Errors print `{"error", "command"}` to stderr
//! with exit status 1.

use aaw_engine::audition::{audition, AuditionOptions};
use aaw_engine::offline::{render, RenderOptions};
use aaw_engine::program::{compile_cached, Cache};
use aaw_engine::realtime::{benchmark, PlayOptions};
use aaw_host::client::{self, Request};
use aaw_host::command::{Command, Fields, Kind, Origin};
use aaw_host::host::{self, lock};
use aaw_host::session::Session;
use aaw_host::{project, registry};
use aaw_model::schedule::schedule;
use aaw_model::validate::ValidationError;
use aaw_model::{contract, frame, Beat, ModelError};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

// Debug builds abort when the audio callback allocates.
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

#[derive(Parser)]
#[command(name = "daw", about = "Agent DAW. Emits JSON; run `daw describe` for the authoring contract.")]
struct Cli {
    /// Who makes the change, as the change log records it.
    #[arg(long, global = true, value_enum, default_value = "agent")]
    origin: Who,
    /// Refuse an edit unless the song's project_sha256 is this one.
    #[arg(long, global = true)]
    expect: Option<String>,
    #[command(subcommand)]
    command: Top,
}

#[derive(Clone, Copy, ValueEnum)]
enum Who {
    Agent,
    User,
}

/// Fields of the object a command adds or changes, as `--name value` pairs,
/// e.g. `--gain-db -3 --release-ms 40`. Values are JSON where they parse as JSON
/// and text otherwise; a flag without a value is true.
#[derive(Args)]
struct FieldArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, value_name = "--FIELD VALUE")]
    fields: Vec<String>,
}

/// A command's PROJECT: the song's file, or the folder that holds `song.yaml`.
#[derive(Clone)]
struct Song(PathBuf);

impl From<std::ffi::OsString> for Song {
    fn from(path: std::ffi::OsString) -> Song {
        Song(project::song_file(Path::new(&path)))
    }
}

impl std::ops::Deref for Song {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

/// The arguments of a command that runs in Python, passed on as given.
#[derive(Args)]
struct Forwarded {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, value_name = "ARGS")]
    args: Vec<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Topic {
    Start,
    Project,
    Sampler,
    Synth,
    Midi,
    Effects,
    Automation,
    Edit,
    Check,
    Samples,
    Beats,
    Joins,
    Export,
    Listen,
    Reference,
}

#[derive(Subcommand)]
enum Top {
    /// Create DIRECTORY/song.yaml, an empty song.
    Init {
        directory: PathBuf,
        #[arg(long, default_value_t = 144.0)]
        tempo: f64,
        /// How many bars long, in the time signature.
        #[arg(long, default_value_t = 16)]
        bars: i64,
        /// The song's time signature, such as 3/4 or 6/8; 4/4 unless given.
        #[arg(long, value_name = "N/D")]
        time_signature: Option<String>,
    },
    /// The projects open in the app or another host, with each one's title
    /// and whether its window is in front.
    Projects {
        /// Also the projects the app knows that are not open.
        #[arg(long)]
        all: bool,
    },
    /// Move the project's folder to NEW_FOLDER and name the song after it. A
    /// running host carries on there and still answers at the old path.
    Move { project: Song, new_folder: PathBuf },
    /// Copy the project's folder to NEW_FOLDER and name the copy after it. A
    /// running host carries on in the copy and leaves the original as it was.
    Copy { project: Song, new_folder: PathBuf },
    /// The authoring contract: what a topic's fields mean and what each takes.
    /// Without a topic, the topics; `daw describe start` makes a first song.
    Describe {
        #[arg(value_enum)]
        topic: Option<Topic>,
        /// Print the JSON Schema of the topic's models as well; without a
        /// topic, the schema of the whole song.
        #[arg(long)]
        schema: bool,
    },
    /// The sample library: folders, search by name and by what a sample
    /// measures as, analyze, like, beats, inspect, audition, import (daw
    /// describe samples).
    #[command(disable_help_flag = true)]
    Samples(Forwarded),
    /// Songs kept as what good sounds like: add, list, show, sections, remove
    /// (daw describe reference).
    #[command(disable_help_flag = true)]
    Reference(Forwarded),
    /// Measure a saved render or WAV; writes analysis JSON and images. With
    /// --section ID, one section with its third octaves, resonances, hits
    /// and what each compressor and limiter took off; with
    /// --overlap A B, two of its stems by band and section; with
    /// --write-translation, the mix in mono and through a small speaker's
    /// band as two files to hear (daw describe listen).
    #[command(disable_help_flag = true)]
    Listen(Forwarded),
    /// Compare two renders: actual and loudness-matched differences; or a
    /// render with a reference, section by section: RENDER --reference NAME.
    #[command(disable_help_flag = true)]
    Compare(Forwarded),
    /// `inspect`, plus each sample's root note against its measured pitch and
    /// warnings about automation.
    #[command(disable_help_flag = true)]
    Check(Forwarded),
    /// The song in beats and seconds: where its sounds are, conversions, its length.
    #[command(disable_help_flag = true)]
    Timeline(Forwarded),
    /// Check a render's joins between parts of a song, and its length.
    #[command(disable_help_flag = true)]
    Joins(Forwarded),
    /// Write the song's render as a named WAV, AAC or MP3 file, at a stated level.
    #[command(disable_help_flag = true)]
    Export(Forwarded),
    /// Rewrite a project in canonical form.
    Fmt { project: Song },
    /// Validate and atomically replace project fields from a JSON merge patch.
    /// Requires --expect. --label names the edit in the change log and for undo.
    Apply {
        project: Song,
        patch: PathBuf,
        #[arg(long)]
        label: Option<String>,
    },
    /// Validate documents with the Rust model and print each one's canonical
    /// YAML and fingerprints, or its errors. Reads only; samples are not checked.
    Model { paths: Vec<PathBuf> },
    /// Render the mix and stems, one track or return as its stem, or one section.
    Render {
        project: Song,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        section: Option<String>,
        /// Frames rendered at a time; the output does not depend on it.
        #[arg(long, hide = true)]
        block_size: Option<usize>,
    },
    /// List every scheduled hit: its start frame, track, pad and release frame.
    Schedule { project: Song },

    /// Run a session host for the project until interrupted or closed. Other
    /// `daw` commands reach it; each change prints to stderr as it lands.
    Host {
        project: Song,
        /// Output buffer in frames.
        #[arg(long, default_value_t = 128)]
        buffer: u32,
    },
    /// Ask the project's host to save and exit.
    Close { project: Song },
    /// Play from a beat. With a host running, the host plays; otherwise this
    /// command hosts the project until playback stops.
    Play {
        project: Song,
        /// Start position in beats; sounding samples are picked up mid-sample.
        #[arg(long)]
        from: Option<String>,
        /// Stop after this many seconds (without a running host).
        #[arg(long)]
        seconds: Option<f64>,
        /// Output buffer in frames (without a running host).
        #[arg(long, default_value_t = 128)]
        buffer: u32,
        /// Time the playback path without a device instead of playing.
        #[arg(long)]
        benchmark: bool,
    },
    /// Stop playback, with a short fade.
    Stop { project: Song },
    /// Move the playhead to a beat; while stopped, play starts there.
    Locate { project: Song, beat: String },
    /// Loop START LENGTH (beats), or `loop PROJECT off`.
    Loop {
        project: Song,
        start: String,
        length: Option<String>,
    },
    /// Transport, revision, undo and redo state.
    Status { project: Song },
    /// The host's change log after a revision, with each change's origin.
    Changes {
        project: Song,
        #[arg(long, default_value_t = 0)]
        since: u64,
    },
    /// Summary of the song, with a reference for each clip.
    Inspect { project: Song },
    /// The song as a grid of tracks by bars: a letter where a clip plays, the
    /// same letter for the same music, # where two clips of a track sound at
    /// once, : where a clip holds and nothing starts, . for nothing; then what
    /// each letter is and a line a track. `clips` names each letter's clips.
    Map {
        project: Song,
        /// A cell: a number of bars, bar (the default) or beat.
        #[arg(long)]
        per: Option<String>,
        /// The first beat shown.
        #[arg(long)]
        from: Option<String>,
        /// The beat to stop at; the song's end, or its last clip's, unless given.
        #[arg(long)]
        to: Option<String>,
        /// Keep only these tracks: --track drums --track bass, or drums,bass.
        #[arg(long = "track", value_delimiter = ',')]
        tracks: Vec<String>,
        /// A row a lane of automation, ~ where its value moves.
        #[arg(long)]
        lanes: bool,
    },
    /// Part of the song by path, e.g. tracks.drums.clips; list items start with
    /// the reference commands use for them.
    Get { project: Song, path: Option<String> },
    /// Undo the last change, whoever made it.
    Undo { project: Song },
    /// Redo the last undone change.
    Redo { project: Song },
    /// Set any value by path, e.g. tracks.drums.gain_db -4.5. VALUE is JSON,
    /// or text when it does not parse as JSON.
    #[command(after_help = set_help())]
    Set {
        project: Song,
        path: String,
        #[arg(allow_hyphen_values = true)]
        value: String,
    },
    /// Flip a boolean, e.g. tracks.drums.mute.
    Toggle { project: Song, path: String },
    /// Remove an object or map entry by path, or reset a field to its default.
    Remove { project: Song, path: String },
    /// Apply a JSON list of commands as one step. --label names the step in
    /// the change log and for undo.
    Batch {
        project: Song,
        file: PathBuf,
        #[arg(long)]
        label: Option<String>,
    },
    #[command(subcommand)]
    Track(TrackCmd),
    #[command(subcommand)]
    Return(ReturnCmd),
    #[command(subcommand)]
    Group(GroupCmd),
    #[command(subcommand)]
    Clip(ClipCmd),
    #[command(subcommand)]
    Audio(AudioCmd),
    #[command(subcommand)]
    Note(NoteCmd),
    #[command(subcommand)]
    Instrument(InstrumentCmd),
    #[command(subcommand)]
    Synth(SynthCmd),
    #[command(subcommand)]
    Patch(PatchCmd),
    #[command(subcommand)]
    Rack(RackCmd),
    #[command(subcommand)]
    Midi(MidiCmd),
    #[command(subcommand)]
    Pattern(PatternCmd),
    #[command(subcommand)]
    Pad(PadCmd),
    #[command(subcommand)]
    Effect(EffectCmd),
    #[command(subcommand)]
    Send(SendCmd),
    #[command(subcommand)]
    Lane(LaneCmd),
    #[command(subcommand)]
    Section(SectionCmd),
    #[command(subcommand)]
    Marker(MarkerCmd),
    #[command(subcommand)]
    Range(RangeCmd),
}

/// Tracks.
#[derive(Subcommand)]
enum TrackCmd {
    /// Add a track; --index places it, and --type midi makes a MIDI track.
    #[command(after_help = track_help())]
    Add {
        project: Song,
        id: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    Remove { project: Song, track: String },
    /// Rename a track and the sidechains that name it.
    Rename { project: Song, track: String, to: String },
    Move { project: Song, track: String, index: usize },
}

/// Returns.
#[derive(Subcommand)]
enum ReturnCmd {
    #[command(after_help = fields_help("Return", &["id"]))]
    Add {
        project: Song,
        id: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Remove a return with the sends to it and their lanes.
    Remove { project: Song, id: String },
    /// Rename a return, its sends and their lanes.
    Rename { project: Song, id: String, to: String },
    Move { project: Song, id: String, index: usize },
}

/// Groups: tracks summed through a chain, gain and pan of their own, as a
/// drum bus. A track names its group with group; daw set tracks.T.group G
/// moves one track, and daw remove tracks.T.group takes it out.
#[derive(Subcommand)]
enum GroupCmd {
    /// Add a group; --tracks a,b puts those tracks in it, moved together.
    #[command(after_help = group_help())]
    Add {
        project: Song,
        id: String,
        /// The tracks in the group, by ID, comma-separated.
        #[arg(long, value_delimiter = ',')]
        tracks: Vec<String>,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Remove a group; its tracks go to the master again.
    Remove { project: Song, group: String },
    /// Rename a group and the tracks' group naming it.
    Rename { project: Song, group: String, to: String },
    Move { project: Song, group: String, index: usize },
}

/// Audio clips: parts of a sample file on a track, addressed by reference: @N
/// while a host runs, else tracks.T.audio.I.
#[derive(Subcommand)]
enum AudioCmd {
    /// Add an audio clip: --at, --source-start-seconds, --source-end-seconds,
    /// --lead-ms, --fade-in-ms, --fade-out-ms, --source-bpm and --stretch are optional.
    #[command(after_help = fields_help("AudioClip", &["sample"]))]
    Add {
        project: Song,
        track: String,
        sample: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Move a clip to another beat and/or track; its audio moves with it.
    Move {
        project: Song,
        clip: String,
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        at: Option<String>,
    },
    /// Make two clips of one at a beat inside it.
    Split {
        project: Song,
        clip: String,
        #[arg(long)]
        at: String,
    },
    /// Move a clip's start or end to a beat; its audio stays where it is.
    Trim {
        project: Song,
        clip: String,
        #[arg(long)]
        start: Option<String>,
        #[arg(long)]
        end: Option<String>,
    },
    /// Crossfade a clip with the one before it on its track.
    Crossfade {
        project: Song,
        clip: String,
        /// The fade out of the clip before, 12 ms unless given.
        #[arg(long)]
        ms: Option<f64>,
        /// The fade in of this clip, a millisecond under the lead unless given.
        #[arg(long)]
        in_ms: Option<f64>,
        /// How long before the beat this clip starts, 5 ms unless given.
        #[arg(long)]
        lead_ms: Option<f64>,
    },
    /// Remove a range of beats from a track's audio clips and close the gap.
    Cut {
        project: Song,
        track: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        ms: Option<f64>,
        #[arg(long)]
        in_ms: Option<f64>,
        #[arg(long)]
        lead_ms: Option<f64>,
    },
}

/// Clips, addressed by reference: @N while a host runs, else tracks.T.clips.I,
/// or a note clip's ID, tracks.T.clips.ID.
#[derive(Subcommand)]
enum ClipCmd {
    /// Add a clip: `add SONG TRACK PATTERN`, then --at, --repeats and
    /// --velocity-scale if wanted. On a MIDI track, a note clip, with no
    /// pattern: --length-beats, and --at, --id and --notes if wanted.
    #[command(after_help = clip_help())]
    Add {
        project: Song,
        track: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Move a clip to another beat and/or track.
    Move {
        project: Song,
        clip: String,
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        at: Option<String>,
    },
    Repeats { project: Song, clip: String, repeats: String },
    /// Copy a clip, by default right after the original, or after the copies
    /// already in a row there, so that duplicating again lays them in a row.
    /// --times N makes N copies one after another as one step. A note clip's
    /// copy owns its notes; --id names it.
    Duplicate {
        project: Song,
        clip: String,
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        times: Option<String>,
    },
    Remove { project: Song, clip: String },
    /// Set a note clip's length; its notes stay where they are. On a looped
    /// audio clip (tracks.T.audio.N), how long it plays.
    Resize { project: Song, clip: String, length: String },
    /// Loop a note clip or an audio clip: its first BEATS play again and again
    /// until its end, so `loop CLIP 8` then `resize CLIP 64` plays a two-bar
    /// phrase eight times. `off` plays the clip once again. An audio clip
    /// keeps its length, or takes --length BEATS.
    Loop {
        project: Song,
        clip: String,
        beats: String,
        #[arg(long)]
        length: Option<String>,
    },
    /// Move a note clip's start or end to a song beat. Its notes stay where
    /// they are in the song; those the start passes are kept and do not play.
    Trim {
        project: Song,
        clip: String,
        #[arg(long)]
        start: Option<String>,
        #[arg(long)]
        end: Option<String>,
    },
    /// Make one clip of two or more clips of one track that plays what they
    /// played. Note clips become one note clip from the first's start to the
    /// last's end, with every note that played and a loop laid out as notes;
    /// pattern clips and audio clips (tracks.T.audio.N) join only where they
    /// meet and are one music: the same pattern, or the same file played on
    /// from where the one before leaves, as `audio split` left them. The
    /// first clip is kept, with its ID and reference.
    Join {
        project: Song,
        #[arg(required = true, num_args = 2..)]
        clips: Vec<String>,
    },
}

/// Notes of note clips, addressed as CLIP.notes.ID, e.g.
/// tracks.keys.clips.clip1.notes.n3, or by @N while a host runs.
#[derive(Subcommand)]
enum NoteCmd {
    /// Add a note: --pitch (a number or a name such as C4), --duration,
    /// optionally --at and --velocity; or --notes with a JSON list of them.
    #[command(after_help = fields_help("Note", &[]))]
    Add {
        project: Song,
        clip: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Change a note's --pitch, --at, --duration or --velocity.
    #[command(after_help = fields_help("Note", &["id"]))]
    Set {
        project: Song,
        note: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Move notes by exact beats; a clip stands for all of its notes.
    Move {
        project: Song,
        #[arg(required = true)]
        notes: Vec<String>,
        #[arg(long, allow_hyphen_values = true)]
        by: String,
    },
    /// Move notes up or down by semitones; a clip stands for all of its notes.
    Transpose {
        project: Song,
        #[arg(required = true)]
        notes: Vec<String>,
        #[arg(long, allow_hyphen_values = true)]
        by: i64,
    },
    /// Remove notes; a clip stands for all of its notes.
    Remove {
        project: Song,
        #[arg(required = true)]
        notes: Vec<String>,
    },
    /// The notes of a note clip, or of a MIDI track's clips, with their names
    /// and song beats; --from and --to keep those starting in a range of song beats.
    List {
        project: Song,
        path: String,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
    },
}

/// The instrument of a MIDI track. Replacing or removing it leaves the notes.
#[derive(Subcommand)]
enum InstrumentCmd {
    /// Attach or replace the instrument, e.g. '{"sampler": {"pads": {...}, "map": [...]}}'.
    Set { project: Song, track: String, instrument: String },
    /// Remove the instrument; the notes are kept and play nothing.
    Remove { project: Song, track: String },
    /// Map NOTES (a note, a name, or [LOW, HIGH]) to a pad of the sampler;
    /// --pitched plays the pad at the notes' pitches.
    Map {
        project: Song,
        track: String,
        notes: String,
        pad: String,
        #[arg(long)]
        pitched: bool,
    },
}

/// The Synth on a MIDI track: a polyphonic synthesizer whose sound is a
/// patch in the song. See `daw describe synth`.
#[derive(Subcommand)]
enum SynthCmd {
    /// Attach the plain saw, or a patch, to a MIDI track, or make a new MIDI
    /// track with it.
    Add {
        project: Song,
        track: String,
        /// A patch's name, from `daw patch list`, or a .yaml file.
        #[arg(long)]
        patch: Option<String>,
    },
    /// The patch as the song holds it, with its modulation listed; fields at
    /// their defaults are left out, as `daw get` leaves them, and `daw
    /// describe synth` lists the defaults.
    Show { project: Song, track: String },
    /// Set fields by their path in the patch, as one undo step: PATH VALUE
    /// pairs, e.g. filter.cutoff_hz 900 envelopes.amp.release_ms 600. A
    /// value is a number, a word, a JSON object, or null to remove a part.
    Set {
        project: Song,
        track: String,
        #[arg(required = true, num_args = 2.., allow_hyphen_values = true, value_name = "PATH VALUE")]
        pairs: Vec<String>,
    },
    /// Add a matrix entry or change its amount: SOURCE TARGET AMOUNT, in the
    /// target's unit; --remove takes the entry out.
    Mod {
        project: Song,
        track: String,
        source: String,
        target: String,
        #[arg(allow_hyphen_values = true)]
        amount: Option<String>,
        #[arg(long)]
        remove: bool,
    },
    /// Render notes through the patch to a WAV under renders/auditions and
    /// measure it, so there is something to hear.
    Audition {
        project: Song,
        track: String,
        /// Notes to play one after another, as names or numbers: C2,G2,C3.
        #[arg(long, default_value = "C4")]
        notes: String,
        #[arg(long, default_value_t = 100)]
        velocity: i64,
        #[arg(long, default_value_t = 2.0)]
        length_beats: f64,
        /// Through the track's effects as well.
        #[arg(long)]
        track_chain: bool,
        /// Where to write the file, instead of under renders/auditions.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Also play the notes now through the running host, one after
        /// another, as the panel's keys do (note.preview). Needs a host.
        #[arg(long)]
        play: bool,
    },
}

/// Patches: a Synth's sound as a YAML file, saved in the workspace library
/// (~/Music/AAW/library/patches, or under AAW_WORKSPACE) beside the factory
/// patches built into daw. A patch loaded into any song is the same sound.
#[derive(Subcommand)]
enum PatchCmd {
    /// The patches, factory and saved, with their names, tags and files;
    /// WORDS keep those with every word in the name or a tag.
    List {
        #[arg(trailing_var_arg = true)]
        words: Vec<String>,
    },
    /// A patch as its file holds it.
    Show { patch: String },
    /// Save a track's synth as a patch named NAME, and name the song's patch
    /// after it. Over a patch already saved under that name only with --replace.
    Save {
        project: Song,
        track: String,
        name: String,
        #[arg(long)]
        description: Option<String>,
        /// Words, separated by commas.
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        #[arg(long)]
        replace: bool,
    },
    /// Load a patch into a MIDI track, in place of its whole synth; the
    /// notes stay. PATCH is a name from `daw patch list` or a .yaml file.
    Load { project: Song, track: String, patch: String },
}

/// Effect racks: a chain of effects as a YAML file, saved in the workspace
/// library (~/Music/AAW/library/racks, or under AAW_WORKSPACE), added to a
/// track, group, return, master or Synth patch in any song.
#[derive(Subcommand)]
enum RackCmd {
    /// The saved racks, with their names, tags, effects and files; WORDS
    /// keep those with every word in the name, a tag or an effect's kind.
    List {
        #[arg(trailing_var_arg = true)]
        words: Vec<String>,
    },
    /// A rack as its file holds it.
    Show { rack: String },
    /// Save a chain as a rack named NAME: OWNER is tracks.T, groups.G,
    /// returns.R, master or tracks.T.instrument.synth, as effect add takes
    /// it. Over a rack already saved under that name only with --replace.
    Save {
        project: Song,
        owner: String,
        name: String,
        #[arg(long)]
        description: Option<String>,
        /// Words, separated by commas.
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        #[arg(long)]
        replace: bool,
    },
    /// Add a rack's effects to a chain, at --index or its end, as one undo
    /// step. RACK is a name from `daw rack list` or a .yaml file. An effect
    /// whose id the chain has is given a number; a compressor's sidechain is
    /// kept only where the song allows it and the track is there.
    Load {
        project: Song,
        owner: String,
        rack: String,
        /// Where in the chain, 0 first; last unless given.
        #[arg(long)]
        index: Option<usize>,
    },
}

/// Standard MIDI files of one part: its notes and velocities, as a note clip.
#[derive(Subcommand)]
enum MidiCmd {
    /// Make a note clip of a MIDI file's notes at --at, 0 unless given: on
    /// --track, a MIDI track, or on a new MIDI track named after the file.
    /// The file's tempo is not taken. What else the file has is left out and
    /// counted in the reply.
    Import {
        project: Song,
        file: PathBuf,
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        at: Option<String>,
    },
    /// Write the notes of a note clip that play as a type 0 MIDI file.
    Export { project: Song, clip: String, file: PathBuf },
}

/// Patterns and their events.
#[derive(Subcommand)]
enum PatternCmd {
    #[command(after_help = fields_help("Pattern", &[]))]
    Add {
        project: Song,
        pattern: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Set a pad's step row, e.g. "x... x... x... x...", or --clear it.
    Steps {
        project: Song,
        pattern: String,
        pad: String,
        row: Option<String>,
        #[arg(long)]
        clear: bool,
    },
    Duplicate { project: Song, pattern: String, to: String },
    #[command(subcommand)]
    Event(EventCmd),
}

/// Pattern events, addressed by reference.
#[derive(Subcommand)]
enum EventCmd {
    /// Add an event: --at and --pad, optionally --note, --duration, --velocity, --transpose.
    #[command(after_help = fields_help("Event", &[]))]
    Add {
        project: Song,
        pattern: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Change an event's fields; a null value resets one.
    #[command(after_help = fields_help("Event", &[]))]
    Set {
        project: Song,
        event: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    Remove { project: Song, event: String },
}

/// Pads.
#[derive(Subcommand)]
enum PadCmd {
    /// Add a pad: --sample, and any pad field.
    #[command(after_help = fields_help("Pad", &[]))]
    Add {
        project: Song,
        track: String,
        pad: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Change a pad's fields; a null value resets one.
    #[command(after_help = fields_help("Pad", &[]))]
    Set {
        project: Song,
        track: String,
        pad: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    Remove { project: Song, track: String, pad: String },
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

/// Effects on tracks.T, returns.R or master, addressed by path, id or reference.
#[derive(Subcommand)]
enum EffectCmd {
    /// Add an effect: --type, optionally --id, --index and parameters.
    #[command(after_help = effect_help())]
    Add {
        project: Song,
        owner: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Remove an effect and the lanes that automate it.
    Remove { project: Song, effect: String },
    /// Move an effect in its chain; lanes addressing effects by index follow.
    Move { project: Song, effect: String, index: usize },
    Bypass {
        project: Song,
        effect: String,
        #[arg(value_enum, default_value = "on")]
        state: OnOff,
    },
}

/// Sends from a track or a group to a return.
#[derive(Subcommand)]
enum SendCmd {
    /// Add or change a send of a track or a group: --gain-db, --pre-fader.
    #[command(after_help = fields_help("Send", &["to"]))]
    Set {
        project: Song,
        /// The track or group that sends.
        track: String,
        to: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Remove a send and the lane that automates it.
    Remove { project: Song, track: String, to: String },
}

/// Automation lanes on tracks.T, returns.R or master.
#[derive(Subcommand)]
enum LaneCmd {
    /// Replace or create a whole lane from a JSON list of points.
    #[command(after_help = lane_help())]
    Set { project: Song, owner: String, param: String, points: String },
    Remove { project: Song, owner: String, param: String },
    #[command(subcommand)]
    Point(PointCmd),
}

/// Automation points, addressed by reference.
#[derive(Subcommand)]
enum PointCmd {
    /// Add a point in time order, creating the lane if needed: --at, --value, --curve, --shape.
    #[command(after_help = lane_help())]
    Add {
        project: Song,
        owner: String,
        param: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Change a point's --at, --value, --curve or --shape.
    #[command(after_help = fields_help("Point", &[]))]
    Move {
        project: Song,
        point: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Remove a point; a lane's last point takes the lane with it.
    Remove { project: Song, point: String },
}

/// Sections: labels over beats, and with --with-content the beats under them.
#[derive(Subcommand)]
enum SectionCmd {
    Add { project: Song, id: String, at: String, length: String },
    /// Move a section's label to AT; with --with-content, its beats too,
    /// over what is there, as `range copy` puts them.
    Move {
        project: Song,
        section: String,
        at: String,
        #[arg(long)]
        with_content: bool,
    },
    /// Remove a section's label; with --with-content, its beats too, closing
    /// the gap as `range delete` does.
    Remove {
        project: Song,
        section: String,
        #[arg(long)]
        with_content: bool,
    },
    /// The section and what is under it again, right after it or at --to,
    /// pushing what follows later; the copy is named --id, or after the
    /// section. Clips across its edges are cut there.
    Duplicate {
        project: Song,
        section: String,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        id: Option<String>,
    },
}

/// Markers: the notes the person leaves at beats while listening, with M in
/// the app. `list` reads them with what plays at each.
#[derive(Subcommand)]
enum MarkerCmd {
    /// The markers in time order: each one's id, beat, the bar it is in and
    /// the beat of that bar, the section over it, its text and the clips
    /// sounding there.
    List { project: Song },
    /// Leave a marker at beat AT; without AT, where a running host is
    /// playing, or at its start position while it is stopped.
    Add {
        project: Song,
        at: Option<String>,
        /// What it says of that place, in a few words.
        #[arg(long)]
        text: Option<String>,
        /// Its ID; the next free mN unless given.
        #[arg(long)]
        id: Option<String>,
    },
    /// Move a marker to beat AT.
    Move { project: Song, marker: String, at: String },
    /// Change what a marker says; "" clears it.
    Text { project: Song, marker: String, text: String },
    /// Remove the markers named, or with --all every one.
    Remove {
        project: Song,
        markers: Vec<String>,
        #[arg(long)]
        all: bool,
    },
}

/// Ranges of beats across every track, or those named with --track: the
/// clips, audio, automation and sections in them. A clip across an edge is
/// cut there; a pattern clip only between repeats. Positions are beats.
#[derive(Subcommand)]
enum RangeCmd {
    /// Put a copy of LENGTH beats from START at --to, over what is there;
    /// --insert opens time for it instead. A note clip's copy owns its notes.
    Copy {
        project: Song,
        start: String,
        length: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        insert: bool,
        #[arg(long = "track")]
        tracks: Vec<String>,
    },
    /// Open LENGTH empty beats at AT: everything from there moves later and
    /// the song grows.
    Insert {
        project: Song,
        at: String,
        length: String,
        #[arg(long = "track")]
        tracks: Vec<String>,
    },
    /// Remove LENGTH beats from START and close the gap: the song shrinks.
    Delete {
        project: Song,
        start: String,
        length: String,
        #[arg(long = "track")]
        tracks: Vec<String>,
    },
    /// Remove what is in LENGTH beats from START; nothing moves.
    Clear {
        project: Song,
        start: String,
        length: String,
        #[arg(long = "track")]
        tracks: Vec<String>,
    },
}

type Result<T> = std::result::Result<T, String>;

/// A field as a flag with what it takes, for a command's --help.
fn flag_line(name: &str, line: &str) -> String {
    format!("  --{:<24} {line}\n", name.replace('_', "-"))
}

/// The fields a command takes, for its --help, from the schema: those of
/// `model` that are values, leaving out `skip`, which the command takes
/// another way.
fn takes(model: &str, skip: &[&str]) -> String {
    contract::model_fields(model)
        .iter()
        .filter(|(name, line)| !skip.contains(&name.as_str()) && !line.starts_with("list of") && !line.starts_with("map of"))
        .map(|(name, line)| flag_line(name, line))
        .collect()
}

fn fields_help(model: &str, skip: &[&str]) -> String {
    format!("Fields:\n{}", takes(model, skip))
}

fn track_help() -> String {
    format!(
        "Fields:\n{}{}{}",
        flag_line("type", "midi makes a MIDI track, which holds note clips and an instrument"),
        flag_line("index", "where among the tracks, 0 first; last unless given"),
        takes("Track", &["id"])
    )
}

fn group_help() -> String {
    format!(
        "Fields:\n{}{}{}",
        flag_line("tracks", "the tracks in it, comma-separated; moved together to where the first of them is"),
        flag_line("index", "where among the groups, 0 first; last unless given"),
        takes("Group", &["id"])
    )
}

fn clip_help() -> String {
    format!(
        "Fields of a pattern clip:\n{}\nFields of a note clip, on a MIDI track:\n{}",
        takes("Clip", &["pattern"]),
        takes("NoteClip", &[])
    )
}

fn effect_help() -> String {
    let mut out = String::from("Each type, with the fields it takes; --type is required:\n");
    for kind in aaw_model::EFFECT_TYPES {
        let model = format!("{}{}", kind[..1].to_ascii_uppercase(), &kind[1..]);
        out.push_str(&format!("{kind}\n{}", takes(&model, &["type"])));
        if kind == "eq" {
            out.push_str(&flag_line("bands", "a JSON list of bands, each:"));
            for (name, line) in contract::model_fields("EqBand") {
                out.push_str(&format!("      {name}: {line}\n"));
            }
        }
    }
    out.push_str("--index places it in the chain, 0 first; last unless given. daw describe effects says what each does.");
    out
}

fn lane_help() -> String {
    let automation = contract::describe("automation").expect("automation is a topic");
    let mut out = String::from(
        "PARAM on a track or return: gain_db, pan, sends.RETURN.gain_db (a track's), effects.REF.FIELD; \
         on the master: gain_db, effects.REF.FIELD; on a MIDI track with a synth: instrument.FIELD \
         (daw describe synth). REF is an effect's id or index in the chain.\nEffect fields a lane can move:\n",
    );
    for (kind, params) in automation["automatable"]["effects"].as_object().into_iter().flatten() {
        let names: Vec<&String> = params.as_object().into_iter().flatten().map(|(f, _)| f).collect();
        let names: Vec<String> = names.iter().map(|f| if kind == "eq" { format!("bands.N.{f}") } else { f.to_string() }).collect();
        out.push_str(&format!("  {kind}: {}\n", names.join(", ")));
    }
    out.push_str("A point is {at, value, curve, shape}:\n");
    for (name, line) in contract::model_fields("Point") {
        out.push_str(&format!("  {name}: {line}\n"));
    }
    out
}

fn set_help() -> String {
    let lookup = |model: &str, field: &str| {
        contract::model_fields(model).into_iter().find(|(n, _)| n == field).map(|(_, l)| l).unwrap_or_default()
    };
    let paths = [
        ("session.tempo", "Session", "tempo"),
        ("session.length_beats", "Session", "length_beats"),
        ("session.master_gain_db", "Session", "master_gain_db"),
        ("session.title", "Session", "title"),
        ("tracks.T.gain_db", "Track", "gain_db"),
        ("tracks.T.pan", "Track", "pan"),
        ("tracks.T.mute", "Track", "mute"),
        ("tracks.T.pads.P.gain_db", "Pad", "gain_db"),
        ("samples.S.root_note", "Sample", "root_note"),
    ];
    let mut out = String::from("Common paths:\n");
    for (path, model, field) in paths {
        out.push_str(&format!("  {path:<26} {}\n", lookup(model, field)));
    }
    out.push_str(
        "T is a track's ID, P a pad's, S a sample's. Any field daw describe lists is set by its path, \
         and an effect's as tracks.T.effects.REF.FIELD, REF its id or index.",
    );
    out
}

fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// A command-line value: JSON where it parses, text otherwise.
fn parse_value(text: &str) -> Json {
    serde_json::from_str(text).unwrap_or_else(|_| Json::String(text.to_string()))
}

fn fields(f: &FieldArgs) -> Result<Fields> {
    let args = &f.fields;
    let mut out = Fields::new();
    let mut i = 0;
    while i < args.len() {
        let Some(flag) = args[i].strip_prefix("--") else {
            return Err(positional(&args[i]));
        };
        let (name, value) = match flag.split_once('=') {
            Some((n, v)) => (n, v.to_string()),
            None if i + 1 < args.len() && !args[i + 1].starts_with("--") => {
                i += 1;
                (flag, args[i].clone())
            }
            None => (flag, "true".to_string()),
        };
        out.insert(name.replace('-', "_"), parse_value(&value));
        i += 1;
    }
    Ok(out)
}

/// The error for a word given where a `--FIELD VALUE` pair was wanted,
/// naming the flag when the word is a value one takes.
fn positional(word: &str) -> String {
    if aaw_model::EFFECT_TYPES.contains(&word) {
        return format!("`{word}` is an effect type; use --type {word}");
    }
    if word == "midi" {
        return "Use --type midi for a MIDI track".into();
    }
    format!("Expected --FIELD VALUE, got `{word}`; fields are flags such as --gain-db -3, and the command's --help lists them")
}

fn take_index(fields: &mut Fields) -> Result<Option<usize>> {
    match fields.shift_remove("index") {
        None => Ok(None),
        Some(v) => v
            .as_u64()
            .map(|n| Some(n as usize))
            .ok_or_else(|| format!("--index must be a non-negative integer, not {v}")),
    }
}

fn errors_json(e: &ValidationError) -> Json {
    Json::Array(
        e.errors
            .iter()
            .map(|x| json!({"loc": x.loc_text(), "type": x.kind, "msg": x.msg}))
            .collect(),
    )
}

fn model_entry(path: &Path) -> Json {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return json!({"path": path, "valid": false, "kind": "io", "error": e.to_string()}),
    };
    match aaw_model::parse(&text) {
        Ok(p) => json!({
            "path": path,
            "valid": true,
            "yaml": aaw_model::to_yaml(&p),
            "project_sha256": aaw_model::project_hash(&p),
            "fingerprints": aaw_model::fingerprints(&p),
        }),
        Err(ModelError::Validation(e)) => json!({
            "path": path,
            "valid": false,
            "kind": "validation",
            "error": e.to_string(),
            "errors": errors_json(&e),
        }),
        Err(e) => json!({"path": path, "valid": false, "kind": "yaml", "error": e.to_string()}),
    }
}

/// An engine result as JSON, keeping its key order.
/// A patch as `daw patch list` prints it: everything but the mapping.
fn patch_json(p: &aaw_host::patches::Patch) -> Json {
    json!({
        "name": p.name,
        "slug": p.slug,
        "description": p.description,
        "tags": p.tags,
        "factory": p.factory,
        "file": p.file,
        "saved_by": p.saved_by,
        "saved_at": p.saved_at,
    })
}

/// A rack as `daw rack list` prints it: everything but the effects' fields.
fn rack_json(r: &aaw_host::racks::Rack) -> Json {
    json!({
        "name": r.name,
        "slug": r.slug,
        "description": r.description,
        "tags": r.tags,
        "kinds": r.kinds(),
        "file": r.file,
        "saved_by": r.saved_by,
        "saved_at": r.saved_at,
    })
}

fn value(v: aaw_model::value::Value) -> Json {
    aaw_host::session::value_json(&v)
}

struct PlayArgs {
    seconds: Option<f64>,
    buffer: u32,
}

fn interrupted() -> Result<Arc<AtomicBool>> {
    let flag = Arc::new(AtomicBool::new(false));
    let f = flag.clone();
    ctrlc::set_handler(move || f.store(true, Ordering::Relaxed)).map_err(text)?;
    Ok(flag)
}

fn no_host(project: &Path, what: &str) -> String {
    format!(
        "No host is running for {}; {what} exists only while a host runs (start one with `daw host` or `daw play`)",
        project.display()
    )
}

/// Runs a command without a host.
fn headless(project: &Path, request: Request, play: Option<PlayArgs>) -> Result<Json> {
    let Request {
        command, origin, expect, ..
    } = request;
    match command.kind() {
        Kind::Edit => {
            let _lock = lock(project)?;
            let mut s = Session::open(project, false)?;
            let (reply, _) = s.edit(&command, origin, expect.as_deref(), None)?;
            s.write()?;
            Ok(reply)
        }
        Kind::Read => {
            let s = Session::open(project, false)?;
            match command {
                Command::Inspect => Ok(s.inspect()),
                Command::Get { path } => s.get(&path),
                Command::Notes { path, from, to } => s.notes(&path, from.as_ref(), to.as_ref()),
                Command::Map { per, from, to, tracks, lanes } => s.map(per.as_ref(), from.as_ref(), to.as_ref(), &tracks, lanes),
                Command::Markers => Ok(s.markers()),
                Command::MidiExport { clip, file } => s.export_midi(&clip, &file),
                Command::Status => {
                    let mut m = s.status();
                    m.insert("playing".into(), json!(false));
                    Ok(Json::Object(m))
                }
                _ => Err(no_host(project, "the change log")),
            }
        }
        Kind::History => Err(no_host(project, "undo history")),
        Kind::Transport => match (command, play) {
            (Command::Play { from }, Some(p)) => host::run(
                project,
                host::Options {
                    buffer: p.buffer,
                    play: Some(from.unwrap_or(json!(0))),
                    exit_on_stop: true,
                    seconds: p.seconds,
                    feed: true,
                    prepare: false,
                    pace: host::PACE,
                },
                interrupted()?,
            ),
            _ => Err(no_host(project, "the transport")),
        },
        Kind::Host => match command {
            Command::Fmt => {
                let _lock = lock(project)?;
                let p = aaw_model::load(project, true).map_err(text)?;
                aaw_model::save(&p, project).map_err(text)?;
                Ok(json!({"project": project, "formatted": true}))
            }
            Command::Move { ref to } | Command::Copy { ref to } => {
                let _lock = lock(project)?;
                let copy = matches!(command, Command::Copy { .. });
                let mut s = Session::open(project, false)?;
                let (reply, _) = s.relocate(Path::new(to), copy, origin, command.json())?;
                Ok(reply)
            }
            _ => Err(format!("No host is running for {}", project.display())),
        },
    }
}

/// Sends a command to the project's host, or runs it headless.
fn route(cli: &Cli, project: &Path, command: Command, play: Option<PlayArgs>) -> Result<Json> {
    let request = Request {
        command,
        origin: match cli.origin {
            Who::Agent => Origin::Agent,
            Who::User => Origin::User,
        },
        expect: cli.expect.clone(),
        gesture: None,
    };
    // A move's own reply says where the project went.
    let moving = matches!(request.command, Command::Move { .. } | Command::Copy { .. });
    match client::ask(project, &request)? {
        Some(reply) => {
            // A project saved under another name since the path was given:
            // the command landed there, and the agent is told where that is.
            let now = reply.moved().filter(|_| !moving).map(Path::to_path_buf);
            let mut result = reply.result;
            if let Some(now) = now {
                notice(&reply.asked, &now);
                if let Json::Object(fields) = &mut result {
                    fields.insert("project".into(), json!(now));
                }
            }
            Ok(result)
        }
        None => headless(project, request, play),
    }
}

fn notice(asked: &Path, now: &Path) {
    let notice = format!("{} is now at {}", project::shown(asked).display(), project::shown(now).display());
    eprintln!("{}", json!({"notice": notice, "project": now}));
}

/// The song file of a command that reads the project's files itself, such
/// as `render`: the path given, or where the project was saved since, if
/// the host that has it open still answers for the path.
fn files(project: &Song) -> PathBuf {
    match registry::moved_to(project) {
        Some(now) => {
            notice(project, &now);
            now
        }
        None => project.0.clone(),
    }
}

/// An argument that names a folder, as the host needs it: absolute.
fn absolute(path: &Path) -> Result<String> {
    let path = std::path::absolute(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

/// `daw projects`: every project a host has open, and with `all` the ones
/// the app's index knows besides.
fn projects(all: bool) -> Result<Json> {
    let status = Request::new(Command::Status, Origin::Agent);
    let mut rows: Vec<Json> = Vec::new();
    let mut open: Vec<PathBuf> = Vec::new();
    for listed in registry::list().iter().filter(|l| l.now.is_none()) {
        // A registration nothing answers at is a host that has gone.
        let Ok(Some(reply)) = client::ask_socket(listed, &status) else {
            continue;
        };
        let s = &reply.result;
        rows.push(json!({
            "project": project::shown(&listed.project),
            "title": s["title"],
            "open": true,
            "front": s["front"],
            "untitled": s["untitled"],
            "revision": s["revision"],
            "playing": s["playing"],
            "pid": s["pid"],
        }));
        open.push(listed.project.clone());
    }
    // The window in front first.
    rows.sort_by_key(|row| row["front"] != json!(true));
    if all {
        for known in project::known() {
            let song = project::resolved(&known.song).unwrap_or_else(|_| known.song.clone());
            if open.contains(&song) {
                continue;
            }
            rows.push(json!({
                "project": project::shown(&known.song),
                "title": known.title,
                "open": false,
                "untitled": known.untitled,
                "missing": !known.song.exists(),
                "opened": known.opened,
            }));
        }
    }
    Ok(json!({"projects": rows}))
}

fn run(cli: &Cli) -> Result<Json> {
    use Command as C;
    let edit = |project: &Song, c: Command| route(cli, project, c, None);
    match &cli.command {
        Top::Init { directory, tempo, bars, time_signature } => {
            let path = project::create(directory, *tempo, *bars, None, time_signature.as_deref())?;
            Ok(json!({"project": std::fs::canonicalize(&path).map_err(text)?}))
        }
        Top::Projects { all } => projects(*all),
        Top::Move { project, new_folder } => edit(project, C::Move { to: absolute(new_folder)? }),
        Top::Copy { project, new_folder } => edit(project, C::Copy { to: absolute(new_folder)? }),
        Top::Describe { topic, schema } => {
            let Some(topic) = topic else {
                return Ok(if *schema { contract::describe_with_schema("project") } else { Some(contract::topics()) }
                    .expect("the song has a schema"));
            };
            let name = topic.to_possible_value().expect("a topic has a name");
            let described = if *schema { contract::describe_with_schema } else { contract::describe };
            described(name.get_name()).ok_or_else(|| "Unknown topic".to_string())
        }
        Top::Samples(_)
        | Top::Reference(_)
        | Top::Listen(_)
        | Top::Compare(_)
        | Top::Check(_)
        | Top::Timeline(_)
        | Top::Joins(_)
        | Top::Export(_) => {
            unreachable!("Python commands are forwarded before this")
        }
        Top::Render {
            project,
            output,
            track,
            section,
            block_size,
        } => {
            let defaults = RenderOptions::default();
            render(
                &files(project),
                &RenderOptions {
                    output: output.clone(),
                    track: track.clone(),
                    section: section.clone(),
                    block_size: block_size.unwrap_or(defaults.block_size),
                },
            )
            .map(value)
        }
        Top::Schedule { project } => {
            let p = aaw_model::load(&files(project), false).map_err(text)?;
            let triggers: Vec<Json> = schedule(&p)
                .iter()
                .map(|t| json!({"start": t.start, "track": t.track_id, "pad": t.pad, "cutoff": t.cutoff}))
                .collect();
            Ok(Json::Array(triggers))
        }
        Top::Model { paths } => Ok(Json::Array(paths.iter().map(|p| model_entry(p)).collect())),
        Top::Fmt { project } => {
            let mut reply = route(cli, project, C::Fmt, None)?;
            // The path as it was given, unless the project has moved from it.
            let at = reply["project"].as_str().map(|p| project::resolved(Path::new(p)));
            if at == Some(project::resolved(project)) {
                reply["project"] = json!(&project.0);
            }
            Ok(reply)
        }
        Top::Apply { project, patch, label } => {
            if cli.expect.is_none() {
                return Err("apply requires --expect with the project SHA256 from inspect".into());
            }
            let body = std::fs::read_to_string(patch).map_err(text)?;
            let patch: Json = serde_json::from_str(&body).map_err(text)?;
            let label = label.clone();
            let reply = edit(project, C::Apply { patch, label })?;
            // The new fingerprint, and the revision while a host runs.
            let mut out = serde_json::Map::new();
            for key in ["revision", "project_sha256"] {
                if let Some(v) = reply.get(key) {
                    out.insert(key.into(), v.clone());
                }
            }
            Ok(Json::Object(out))
        }
        Top::Host { project, buffer } => host::run(
            project,
            host::Options {
                buffer: *buffer,
                play: None,
                exit_on_stop: false,
                seconds: None,
                feed: true,
                prepare: false,
                pace: host::PACE,
            },
            interrupted()?,
        ),
        Top::Close { project } => edit(project, C::Close),
        Top::Play {
            project,
            from,
            seconds,
            buffer,
            benchmark: bench,
        } => {
            if *bench {
                let project = &files(project);
                let p = aaw_model::load(project, true).map_err(text)?;
                let from_beat = aaw_model::beat(&Beat::Str(from.clone().unwrap_or_else(|| "0".into())))?;
                let start = frame(&from_beat, p.session.tempo, p.session.sample_rate).max(0) as usize;
                // What an edit costs the host before the audio thread has it: a
                // compile with everything the last one made, and a renderer.
                let directory = project.parent().unwrap_or(Path::new("."));
                let mut cache = Cache::default();
                let started = std::time::Instant::now();
                let program = Arc::new(compile_cached(&p, directory, &mut cache)?);
                let cold = started.elapsed();
                let started = std::time::Instant::now();
                compile_cached(&p, directory, &mut cache)?;
                let warm = started.elapsed();
                let started = std::time::Instant::now();
                drop(aaw_engine::player::Deck::new(program.clone()));
                let deck = started.elapsed();
                if start >= program.total {
                    return Err("--from is at or after the session end".into());
                }
                let opts = PlayOptions {
                    from: start,
                    seconds: *seconds,
                    buffer: *buffer,
                };
                // What the app's waveforms cost: each track's peaks over the
                // whole song, one track at a time.
                let peaks: Vec<std::time::Duration> = program
                    .tracks
                    .iter()
                    .map(|t| {
                        let started = std::time::Instant::now();
                        drop(aaw_engine::peaks::peaks(&t.voices, program.total));
                        started.elapsed()
                    })
                    .collect();
                let mut report = value(benchmark(program, &opts));
                let ms = |d: std::time::Duration| json!((d.as_secs_f64() * 1e5).round() / 100.0);
                report["compile_ms"] = ms(cold);
                report["recompile_ms"] = ms(warm);
                report["renderer_ms"] = ms(deck);
                report["peaks_ms"] = json!({
                    "tracks": peaks.len(),
                    "all": ms(peaks.iter().sum()),
                    "slowest_track": ms(peaks.iter().max().copied().unwrap_or_default()),
                });
                return Ok(report);
            }
            let play = PlayArgs {
                seconds: *seconds,
                buffer: *buffer,
            };
            route(cli, project, C::Play { from: from.as_deref().map(parse_value) }, Some(play))
        }
        Top::Stop { project } => edit(project, C::Stop),
        Top::Locate { project, beat } => edit(project, C::Locate { at: parse_value(beat) }),
        Top::Loop { project, start, length } => {
            let c = match (start.as_str(), length) {
                ("off", None) => C::Loop { start: None, length: None },
                (_, Some(l)) => C::Loop {
                    start: Some(parse_value(start)),
                    length: Some(parse_value(l)),
                },
                _ => return Err("Use `loop PROJECT START LENGTH` or `loop PROJECT off`".into()),
            };
            edit(project, c)
        }
        Top::Status { project } => edit(project, C::Status),
        Top::Changes { project, since } => edit(project, C::Changes { since: *since }),
        Top::Inspect { project } => edit(project, C::Inspect),
        Top::Map {
            project,
            per,
            from,
            to,
            tracks,
            lanes,
        } => edit(
            project,
            C::Map {
                per: per.as_deref().map(parse_value),
                from: from.as_deref().map(parse_value),
                to: to.as_deref().map(parse_value),
                tracks: tracks.clone(),
                lanes: *lanes,
            },
        ),
        Top::Get { project, path } => edit(project, C::Get { path: path.clone().unwrap_or_default() }),
        Top::Undo { project } => edit(project, C::Undo),
        Top::Redo { project } => edit(project, C::Redo),
        Top::Set { project, path, value } => edit(
            project,
            C::Set {
                path: path.clone(),
                value: parse_value(value),
            },
        ),
        Top::Toggle { project, path } => edit(project, C::Toggle { path: path.clone() }),
        Top::Remove { project, path } => edit(project, C::Remove { path: path.clone() }),
        Top::Batch { project, file, label } => {
            let body = std::fs::read_to_string(file).map_err(text)?;
            let parsed: Json = serde_json::from_str(&body).map_err(text)?;
            // A list of commands, or an object with them and perhaps a label.
            let (list, named) = match parsed {
                Json::Object(mut m) => (
                    m.remove("commands").unwrap_or(Json::Null),
                    m.remove("label").and_then(|l| l.as_str().map(str::to_string)),
                ),
                other => (other, None),
            };
            let commands: Vec<Command> =
                serde_json::from_value(list).map_err(|e| format!("{}: {e}", file.display()))?;
            edit(
                project,
                C::Batch {
                    commands,
                    label: label.clone().or(named),
                },
            )
        }
        Top::Track(t) => match t {
            TrackCmd::Add { project, id, f } => {
                let mut fields = fields(f)?;
                let index = take_index(&mut fields)?;
                edit(project, C::TrackAdd { id: id.clone(), index, fields })
            }
            TrackCmd::Remove { project, track } => edit(project, C::TrackRemove { track: track.clone() }),
            TrackCmd::Rename { project, track, to } => edit(
                project,
                C::TrackRename {
                    track: track.clone(),
                    to: to.clone(),
                },
            ),
            TrackCmd::Move { project, track, index } => edit(
                project,
                C::TrackMove {
                    track: track.clone(),
                    index: *index,
                },
            ),
        },
        Top::Return(r) => match r {
            ReturnCmd::Add { project, id, f } => {
                let mut fields = fields(f)?;
                let index = take_index(&mut fields)?;
                edit(project, C::ReturnAdd { id: id.clone(), index, fields })
            }
            ReturnCmd::Remove { project, id } => edit(project, C::ReturnRemove { id: id.clone() }),
            ReturnCmd::Rename { project, id, to } => edit(
                project,
                C::ReturnRename {
                    id: id.clone(),
                    to: to.clone(),
                },
            ),
            ReturnCmd::Move { project, id, index } => edit(
                project,
                C::ReturnMove {
                    id: id.clone(),
                    index: *index,
                },
            ),
        },
        Top::Group(g) => match g {
            GroupCmd::Add { project, id, tracks, f } => {
                let mut fields = fields(f)?;
                let index = take_index(&mut fields)?;
                edit(
                    project,
                    C::GroupAdd {
                        id: id.clone(),
                        index,
                        tracks: tracks.clone(),
                        fields,
                    },
                )
            }
            GroupCmd::Remove { project, group } => edit(project, C::GroupRemove { group: group.clone() }),
            GroupCmd::Rename { project, group, to } => edit(
                project,
                C::GroupRename {
                    group: group.clone(),
                    to: to.clone(),
                },
            ),
            GroupCmd::Move { project, group, index } => edit(
                project,
                C::GroupMove {
                    group: group.clone(),
                    index: *index,
                },
            ),
        },
        Top::Audio(a) => match a {
            AudioCmd::Add { project, track, sample, f } => {
                let mut all = Fields::new();
                all.insert("sample".into(), json!(sample));
                all.extend(fields(f)?);
                edit(project, C::AudioAdd { track: track.clone(), fields: all })
            }
            AudioCmd::Move { project, clip, track, at } => edit(
                project,
                C::AudioMove {
                    clip: clip.clone(),
                    track: track.clone(),
                    at: at.as_deref().map(parse_value),
                },
            ),
            AudioCmd::Split { project, clip, at } => edit(
                project,
                C::AudioSplit {
                    clip: clip.clone(),
                    at: parse_value(at),
                },
            ),
            AudioCmd::Trim { project, clip, start, end } => edit(
                project,
                C::AudioTrim {
                    clip: clip.clone(),
                    start: start.as_deref().map(parse_value),
                    end: end.as_deref().map(parse_value),
                },
            ),
            AudioCmd::Crossfade { project, clip, ms, in_ms, lead_ms } => edit(
                project,
                C::AudioCrossfade {
                    clip: clip.clone(),
                    ms: *ms,
                    in_ms: *in_ms,
                    lead_ms: *lead_ms,
                },
            ),
            AudioCmd::Cut { project, track, from, to, ms, in_ms, lead_ms } => edit(
                project,
                C::AudioCut {
                    track: track.clone(),
                    from: parse_value(from),
                    to: parse_value(to),
                    ms: *ms,
                    in_ms: *in_ms,
                    lead_ms: *lead_ms,
                },
            ),
        },
        Top::Clip(c) => match c {
            ClipCmd::Add { project, track, f } => {
                // A pattern, if one is given, comes before the fields.
                let mut all = Fields::new();
                let mut rest = FieldArgs { fields: f.fields.clone() };
                if rest.fields.first().is_some_and(|a| !a.starts_with("--")) {
                    all.insert("pattern".into(), json!(rest.fields.remove(0)));
                }
                all.extend(fields(&rest)?);
                edit(project, C::ClipAdd { track: track.clone(), fields: all })
            }
            ClipCmd::Move { project, clip, track, at } => edit(
                project,
                C::ClipMove {
                    clip: clip.clone(),
                    track: track.clone(),
                    at: at.as_deref().map(parse_value),
                },
            ),
            ClipCmd::Repeats { project, clip, repeats } => edit(
                project,
                C::ClipRepeats {
                    clip: clip.clone(),
                    repeats: parse_value(repeats),
                },
            ),
            ClipCmd::Duplicate { project, clip, track, at, id, times } => edit(
                project,
                C::ClipDuplicate {
                    clip: clip.clone(),
                    track: track.clone(),
                    at: at.as_deref().map(parse_value),
                    id: id.clone(),
                    times: times.as_deref().map(parse_value),
                },
            ),
            ClipCmd::Remove { project, clip } => edit(project, C::ClipRemove { clip: clip.clone() }),
            ClipCmd::Resize { project, clip, length } => edit(
                project,
                C::ClipResize {
                    clip: clip.clone(),
                    length_beats: parse_value(length),
                },
            ),
            ClipCmd::Trim { project, clip, start, end } => edit(
                project,
                C::ClipTrim {
                    clip: clip.clone(),
                    start: start.as_deref().map(parse_value),
                    end: end.as_deref().map(parse_value),
                },
            ),
            ClipCmd::Join { project, clips } => edit(project, C::ClipJoin { clips: clips.clone() }),
            ClipCmd::Loop { project, clip, beats, length } => edit(
                project,
                C::ClipLoop {
                    clip: clip.clone(),
                    loop_beats: if beats.eq_ignore_ascii_case("off") { Json::Null } else { parse_value(beats) },
                    length: length.as_deref().map(parse_value),
                },
            ),
        },
        Top::Synth(s) => match s {
            SynthCmd::Add { project, track, patch } => edit(
                project,
                C::SynthAdd {
                    track: track.clone(),
                    index: None,
                    patch: patch.clone(),
                },
            ),
            SynthCmd::Show { project, track } => {
                let reply = edit(project, C::Get { path: format!("tracks.{track}.instrument.synth") })?;
                if reply.is_null() {
                    return Err(format!("{track} has no synth; daw synth add attaches one"));
                }
                Ok(reply)
            }
            SynthCmd::Set { project, track, pairs } => {
                if pairs.len() % 2 != 0 {
                    return Err("Give PATH VALUE pairs".into());
                }
                let mut values = Fields::new();
                for pair in pairs.chunks(2) {
                    values.insert(pair[0].clone(), parse_value(&pair[1]));
                }
                edit(project, C::SynthSet { track: track.clone(), values })
            }
            SynthCmd::Mod { project, track, source, target, amount, remove } => {
                let amount = match amount {
                    Some(a) => Some(json!(a.parse::<f64>().map_err(|_| format!("The amount must be a number, not {a}"))?)),
                    None => None,
                };
                edit(
                    project,
                    C::SynthMod {
                        track: track.clone(),
                        source: source.clone(),
                        target: target.clone(),
                        amount,
                        remove: *remove,
                    },
                )
            }
            SynthCmd::Audition { project, track, notes, velocity, length_beats, track_chain, output, play } => {
                let notes: Vec<i64> = notes
                    .split(',')
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(|n| n.parse::<i64>().or_else(|_| aaw_model::rules::midi(n)))
                    .collect::<Result<_>>()?;
                let file = files(project);
                let mut reply = audition(
                    &file,
                    &AuditionOptions {
                        track: track.clone(),
                        notes: notes.clone(),
                        velocity: *velocity,
                        length_beats: *length_beats,
                        track_chain: *track_chain,
                        output: output.clone(),
                    },
                )
                .map(value)?;
                if *play {
                    // The same notes, heard now through the host, each held
                    // for its length before the next starts.
                    let tempo = aaw_model::load(&file, true).map_err(text)?.session.tempo;
                    let played: Vec<i64> = if notes.is_empty() { vec![60] } else { notes };
                    for (i, pitch) in played.iter().enumerate() {
                        if i > 0 {
                            std::thread::sleep(Duration::from_secs_f64(*length_beats * 60.0 / tempo));
                        }
                        edit(
                            project,
                            C::NotePreview {
                                track: track.clone(),
                                pitch: json!(pitch),
                                velocity: Some(*velocity),
                                length_beats: Some(json!(length_beats)),
                            },
                        )?;
                    }
                    reply["played"] = json!(true);
                }
                Ok(reply)
            }
        },
        Top::Patch(p) => match p {
            PatchCmd::List { words } => {
                let query = words.join(" ");
                let listing = aaw_host::patches::list();
                let mut out = json!({
                    "directory": aaw_host::patches::dir(),
                    "patches": listing.patches.iter().filter(|p| aaw_host::patches::matches(p, &query)).map(patch_json).collect::<Vec<_>>(),
                });
                if !listing.problems.is_empty() {
                    out["problems"] = json!(listing.problems);
                }
                Ok(out)
            }
            PatchCmd::Show { patch } => {
                let p = aaw_host::patches::find(patch)?;
                let mut out = patch_json(&p);
                out["synth"] = value(p.synth.clone());
                Ok(out)
            }
            PatchCmd::Save { project, track, name, description, tags, replace } => edit(
                project,
                C::PatchSave {
                    track: track.clone(),
                    name: name.clone(),
                    description: description.clone(),
                    tags: tags.clone(),
                    replace: *replace,
                },
            ),
            PatchCmd::Load { project, track, patch } => edit(
                project,
                C::PatchLoad {
                    track: track.clone(),
                    patch: patch.clone(),
                },
            ),
        },
        Top::Rack(r) => match r {
            RackCmd::List { words } => {
                let query = words.join(" ");
                let listing = aaw_host::racks::list();
                let mut out = json!({
                    "directory": aaw_host::racks::dir(),
                    "racks": listing.racks.iter().filter(|r| aaw_host::racks::matches(r, &query)).map(rack_json).collect::<Vec<_>>(),
                });
                if !listing.problems.is_empty() {
                    out["problems"] = json!(listing.problems);
                }
                Ok(out)
            }
            RackCmd::Show { rack } => {
                let r = aaw_host::racks::find(rack)?;
                let mut out = rack_json(&r);
                out["effects"] = value(r.effects.clone());
                Ok(out)
            }
            RackCmd::Save { project, owner, name, description, tags, replace } => edit(
                project,
                C::RackSave {
                    owner: owner.clone(),
                    name: name.clone(),
                    description: description.clone(),
                    tags: tags.clone(),
                    replace: *replace,
                },
            ),
            RackCmd::Load { project, owner, rack, index } => edit(
                project,
                C::RackLoad {
                    owner: owner.clone(),
                    rack: rack.clone(),
                    index: *index,
                },
            ),
        },
        Top::Midi(m) => match m {
            MidiCmd::Import { project, file, track, at } => edit(
                project,
                C::MidiImport {
                    file: absolute(file)?,
                    track: track.clone(),
                    at: at.as_deref().map(parse_value),
                    index: None,
                },
            ),
            MidiCmd::Export { project, clip, file } => edit(
                project,
                C::MidiExport {
                    clip: clip.clone(),
                    file: absolute(file)?,
                },
            ),
        },
        Top::Note(n) => match n {
            NoteCmd::Add { project, clip, f } => {
                let mut fields = fields(f)?;
                let notes = match fields.shift_remove("notes") {
                    None => Vec::new(),
                    Some(Json::Array(items)) => items
                        .into_iter()
                        .map(|n| match n {
                            Json::Object(m) => Ok(m),
                            other => Err(format!("--notes lists notes as objects, not {other}")),
                        })
                        .collect::<Result<_>>()?,
                    Some(other) => return Err(format!("--notes must be a JSON list of notes, not {other}")),
                };
                edit(project, C::NoteAdd { clip: clip.clone(), notes, fields })
            }
            NoteCmd::Set { project, note, f } => edit(
                project,
                C::NoteSet {
                    note: note.clone(),
                    fields: fields(f)?,
                },
            ),
            NoteCmd::Move { project, notes, by } => edit(
                project,
                C::NoteMove {
                    notes: notes.clone(),
                    by: parse_value(by),
                },
            ),
            NoteCmd::Transpose { project, notes, by } => edit(project, C::NoteTranspose { notes: notes.clone(), by: *by }),
            NoteCmd::Remove { project, notes } => edit(project, C::NoteRemove { notes: notes.clone() }),
            NoteCmd::List { project, path, from, to } => edit(
                project,
                C::Notes {
                    path: path.clone(),
                    from: from.as_deref().map(parse_value),
                    to: to.as_deref().map(parse_value),
                },
            ),
        },
        Top::Instrument(i) => match i {
            InstrumentCmd::Set { project, track, instrument } => {
                let instrument: Json =
                    serde_json::from_str(instrument).map_err(|e| format!("The instrument must be JSON: {e}"))?;
                edit(project, C::InstrumentSet { track: track.clone(), instrument })
            }
            InstrumentCmd::Remove { project, track } => edit(
                project,
                C::InstrumentSet {
                    track: track.clone(),
                    instrument: Json::Null,
                },
            ),
            InstrumentCmd::Map { project, track, notes, pad, pitched } => {
                let mut fields = Fields::new();
                fields.insert("notes".into(), parse_value(notes));
                fields.insert("pad".into(), json!(pad));
                if *pitched {
                    fields.insert("pitched".into(), json!(true));
                }
                edit(project, C::InstrumentMap { track: track.clone(), fields })
            }
        },
        Top::Pattern(p) => match p {
            PatternCmd::Add { project, pattern, f } => edit(
                project,
                C::PatternAdd {
                    pattern: pattern.clone(),
                    fields: fields(f)?,
                },
            ),
            PatternCmd::Steps {
                project,
                pattern,
                pad,
                row,
                clear,
            } => {
                if row.is_some() == *clear {
                    return Err("Give a step row or --clear".into());
                }
                edit(
                    project,
                    C::PatternSteps {
                        pattern: pattern.clone(),
                        pad: pad.clone(),
                        row: row.clone(),
                    },
                )
            }
            PatternCmd::Duplicate { project, pattern, to } => edit(
                project,
                C::PatternDuplicate {
                    pattern: pattern.clone(),
                    to: to.clone(),
                },
            ),
            PatternCmd::Event(e) => match e {
                EventCmd::Add { project, pattern, f } => edit(
                    project,
                    C::EventAdd {
                        pattern: pattern.clone(),
                        fields: fields(f)?,
                    },
                ),
                EventCmd::Set { project, event, f } => edit(
                    project,
                    C::EventSet {
                        event: event.clone(),
                        fields: fields(f)?,
                    },
                ),
                EventCmd::Remove { project, event } => edit(project, C::EventRemove { event: event.clone() }),
            },
        },
        Top::Pad(p) => match p {
            PadCmd::Add { project, track, pad, f } => edit(
                project,
                C::PadAdd {
                    track: track.clone(),
                    pad: pad.clone(),
                    fields: fields(f)?,
                },
            ),
            PadCmd::Set { project, track, pad, f } => edit(
                project,
                C::PadSet {
                    track: track.clone(),
                    pad: pad.clone(),
                    fields: fields(f)?,
                },
            ),
            PadCmd::Remove { project, track, pad } => edit(
                project,
                C::PadRemove {
                    track: track.clone(),
                    pad: pad.clone(),
                },
            ),
        },
        Top::Effect(e) => match e {
            EffectCmd::Add { project, owner, f } => {
                let mut fields = fields(f)?;
                let index = take_index(&mut fields)?;
                if !fields.contains_key("type") {
                    return Err("effect add needs --type".into());
                }
                edit(
                    project,
                    C::EffectAdd {
                        owner: owner.clone(),
                        index,
                        fields,
                    },
                )
            }
            EffectCmd::Remove { project, effect } => edit(project, C::EffectRemove { effect: effect.clone() }),
            EffectCmd::Move { project, effect, index } => edit(
                project,
                C::EffectMove {
                    effect: effect.clone(),
                    index: *index,
                },
            ),
            EffectCmd::Bypass { project, effect, state } => edit(
                project,
                C::EffectBypass {
                    effect: effect.clone(),
                    bypass: matches!(state, OnOff::On),
                },
            ),
        },
        Top::Send(s) => match s {
            SendCmd::Set { project, track, to, f } => edit(
                project,
                C::SendSet {
                    track: track.clone(),
                    to: to.clone(),
                    fields: fields(f)?,
                },
            ),
            SendCmd::Remove { project, track, to } => edit(
                project,
                C::SendRemove {
                    track: track.clone(),
                    to: to.clone(),
                },
            ),
        },
        Top::Lane(l) => match l {
            LaneCmd::Set {
                project,
                owner,
                param,
                points,
            } => edit(
                project,
                C::LaneSet {
                    owner: owner.clone(),
                    param: param.clone(),
                    points: serde_json::from_str(points).map_err(|e| format!("points must be a JSON list: {e}"))?,
                },
            ),
            LaneCmd::Remove { project, owner, param } => edit(
                project,
                C::LaneRemove {
                    owner: owner.clone(),
                    param: param.clone(),
                },
            ),
            LaneCmd::Point(p) => match p {
                PointCmd::Add { project, owner, param, f } => edit(
                    project,
                    C::PointAdd {
                        owner: owner.clone(),
                        param: param.clone(),
                        fields: fields(f)?,
                    },
                ),
                PointCmd::Move { project, point, f } => edit(
                    project,
                    C::PointSet {
                        point: point.clone(),
                        fields: fields(f)?,
                    },
                ),
                PointCmd::Remove { project, point } => edit(project, C::PointRemove { point: point.clone() }),
            },
        },
        Top::Section(s) => match s {
            SectionCmd::Add { project, id, at, length } => edit(
                project,
                C::SectionAdd {
                    id: id.clone(),
                    at: parse_value(at),
                    length_beats: parse_value(length),
                },
            ),
            SectionCmd::Move { project, section, at, with_content } => edit(
                project,
                C::SectionMove {
                    section: section.clone(),
                    at: parse_value(at),
                    with_content: *with_content,
                },
            ),
            SectionCmd::Remove { project, section, with_content } => edit(
                project,
                C::SectionRemove {
                    section: section.clone(),
                    with_content: *with_content,
                },
            ),
            SectionCmd::Duplicate { project, section, to, id } => edit(
                project,
                C::SectionDuplicate {
                    section: section.clone(),
                    to: to.as_deref().map(parse_value),
                    id: id.clone(),
                },
            ),
        },
        Top::Marker(m) => match m {
            MarkerCmd::List { project } => edit(project, C::Markers),
            MarkerCmd::Add { project, at, text, id } => edit(
                project,
                C::MarkerAdd {
                    at: at.as_deref().map(parse_value),
                    text: text.clone().unwrap_or_default(),
                    id: id.clone(),
                },
            ),
            MarkerCmd::Move { project, marker, at } => edit(
                project,
                C::MarkerMove {
                    marker: marker.clone(),
                    at: parse_value(at),
                },
            ),
            MarkerCmd::Text { project, marker, text } => edit(
                project,
                C::MarkerText {
                    marker: marker.clone(),
                    text: text.clone(),
                },
            ),
            MarkerCmd::Remove { project, markers, all } => edit(
                project,
                C::MarkerRemove {
                    markers: markers.clone(),
                    all: *all,
                },
            ),
        },
        Top::Range(r) => match r {
            RangeCmd::Copy { project, start, length, to, insert, tracks } => edit(
                project,
                C::RangeCopy {
                    start: parse_value(start),
                    length: parse_value(length),
                    to: parse_value(to),
                    insert: *insert,
                    tracks: tracks.clone(),
                },
            ),
            RangeCmd::Insert { project, at, length, tracks } => edit(
                project,
                C::RangeInsert {
                    at: parse_value(at),
                    length: parse_value(length),
                    tracks: tracks.clone(),
                },
            ),
            RangeCmd::Delete { project, start, length, tracks } => edit(
                project,
                C::RangeDelete {
                    start: parse_value(start),
                    length: parse_value(length),
                    tracks: tracks.clone(),
                },
            ),
            RangeCmd::Clear { project, start, length, tracks } => edit(
                project,
                C::RangeClear {
                    start: parse_value(start),
                    length: parse_value(length),
                    tracks: tracks.clone(),
                },
            ),
        },
    }
}

fn name(top: &Top) -> String {
    let group = |g: &str, s: &str| format!("{g} {s}");
    match top {
        Top::Init { .. } => "init".into(),
        Top::Projects { .. } => "projects".into(),
        Top::Move { .. } => "move".into(),
        Top::Copy { .. } => "copy".into(),
        Top::Describe { .. } => "describe".into(),
        Top::Samples(_) => "samples".into(),
        Top::Reference(_) => "reference".into(),
        Top::Listen(_) => "listen".into(),
        Top::Compare(_) => "compare".into(),
        Top::Check(_) => "check".into(),
        Top::Timeline(_) => "timeline".into(),
        Top::Joins(_) => "joins".into(),
        Top::Export(_) => "export".into(),
        Top::Fmt { .. } => "fmt".into(),
        Top::Apply { .. } => "apply".into(),
        Top::Model { .. } => "model".into(),
        Top::Render { .. } => "render".into(),
        Top::Schedule { .. } => "schedule".into(),
        Top::Host { .. } => "host".into(),
        Top::Close { .. } => "close".into(),
        Top::Play { .. } => "play".into(),
        Top::Stop { .. } => "stop".into(),
        Top::Locate { .. } => "locate".into(),
        Top::Loop { .. } => "loop".into(),
        Top::Status { .. } => "status".into(),
        Top::Changes { .. } => "changes".into(),
        Top::Inspect { .. } => "inspect".into(),
        Top::Map { .. } => "map".into(),
        Top::Get { .. } => "get".into(),
        Top::Undo { .. } => "undo".into(),
        Top::Redo { .. } => "redo".into(),
        Top::Set { .. } => "set".into(),
        Top::Toggle { .. } => "toggle".into(),
        Top::Remove { .. } => "remove".into(),
        Top::Batch { .. } => "batch".into(),
        Top::Track(_) => group("track", ""),
        Top::Return(_) => group("return", ""),
        Top::Group(_) => group("group", ""),
        Top::Clip(_) => group("clip", ""),
        Top::Audio(_) => group("audio", ""),
        Top::Note(_) => group("note", ""),
        Top::Midi(_) => group("midi", ""),
        Top::Instrument(_) => group("instrument", ""),
        Top::Synth(_) => group("synth", ""),
        Top::Patch(_) => group("patch", ""),
        Top::Rack(_) => group("rack", ""),
        Top::Pattern(_) => group("pattern", ""),
        Top::Pad(_) => group("pad", ""),
        Top::Effect(_) => group("effect", ""),
        Top::Send(_) => group("send", ""),
        Top::Lane(_) => group("lane", ""),
        Top::Section(_) => group("section", ""),
        Top::Marker(_) => group("marker", ""),
        Top::Range(_) => group("range", ""),
    }
    .trim()
    .to_string()
}

/// Runs a Python command with this process's arguments, input and output, and
/// returns its exit status. What that command asks of `daw` in turn, as `check`
/// asks for `inspect`, comes back to this binary.
fn forward(command: &str, args: &[String]) -> Result<ExitCode> {
    let mut python = aaw_host::python::command(command)?;
    if std::env::var_os("AAW_DAW").is_none() {
        python.env("AAW_DAW", std::env::current_exe().map_err(text)?);
    }
    let status = python
        .args(args)
        .status()
        .map_err(|e| format!("Could not start Python: {e}"))?;
    Ok(match status.code() {
        Some(0) => ExitCode::SUCCESS,
        Some(code) => ExitCode::from(code.clamp(1, 255) as u8),
        None => ExitCode::FAILURE,
    })
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Top::Samples(f)
    | Top::Reference(f)
    | Top::Listen(f)
    | Top::Compare(f)
    | Top::Check(f)
    | Top::Timeline(f)
    | Top::Joins(f)
    | Top::Export(f) = &cli.command
    {
        let command = name(&cli.command);
        return forward(&command, &f.args).unwrap_or_else(|error| {
            eprintln!("{}", json!({"error": error, "command": command}));
            ExitCode::FAILURE
        });
    }
    match run(&cli) {
        Ok(result) => {
            use std::io::Write;
            let text = serde_json::to_string_pretty(&result).expect("json");
            match writeln!(std::io::stdout().lock(), "{text}") {
                Ok(()) => ExitCode::SUCCESS,
                // A reader that has stopped, as `head` does, ends the command
                // quietly with the status a shell gives a closed pipe.
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::from(141),
                Err(e) => {
                    eprintln!("{}", json!({"error": e.to_string(), "command": name(&cli.command)}));
                    ExitCode::FAILURE
                }
            }
        }
        Err(error) => {
            eprintln!("{}", json!({"error": error, "command": name(&cli.command)}));
            ExitCode::FAILURE
        }
    }
}
