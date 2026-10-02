//! The `daw` binary: JSON in and out over the Rust model, engine and host.
//!
//! Commands that read or change a song go to the host that owns it when one is
//! running (`daw host`, or `daw play` while it plays), so the change is heard and
//! recorded in its history. Otherwise they run headless: load, apply, save, exit.
//!
//! The sample library and the perception tools are Python: `samples`, `listen`,
//! `compare` and `check` run there, with this process's arguments and output.
//!
//! Results print to stdout as JSON. Errors print `{"error", "command"}` to stderr
//! with exit status 1.

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
    Project,
    Sampler,
    Effects,
    Automation,
    Edit,
    Beats,
    Joins,
    Export,
}

#[derive(Subcommand)]
enum Top {
    /// Create DIRECTORY/song.yaml, an empty song.
    Init {
        directory: PathBuf,
        #[arg(long, default_value_t = 144.0)]
        tempo: f64,
        #[arg(long, default_value_t = 16)]
        bars: i64,
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
    /// The authoring contract: the song's schema and what its fields mean.
    Describe {
        #[arg(value_enum, default_value = "project")]
        topic: Topic,
    },
    /// The sample library: scan, search, analyze, inspect, audition, import.
    #[command(disable_help_flag = true)]
    Samples(Forwarded),
    /// Measure a saved render or WAV; writes analysis JSON and images.
    #[command(disable_help_flag = true)]
    Listen(Forwarded),
    /// Compare two renders: actual and loudness-matched differences.
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
    /// Part of the song by path, e.g. tracks.drums.clips; list items start with
    /// the reference commands use for them.
    Get { project: Song, path: Option<String> },
    /// Undo the last change, whoever made it.
    Undo { project: Song },
    /// Redo the last undone change.
    Redo { project: Song },
    /// Set any value by path, e.g. tracks.drums.gain_db -4.5. VALUE is JSON,
    /// or text when it does not parse as JSON.
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
    Clip(ClipCmd),
    #[command(subcommand)]
    Audio(AudioCmd),
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
}

/// Tracks.
#[derive(Subcommand)]
enum TrackCmd {
    /// Add a track; --index places it.
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

/// Audio clips: parts of a sample file on a track, addressed by reference: @N
/// while a host runs, else tracks.T.audio.I.
#[derive(Subcommand)]
enum AudioCmd {
    /// Add an audio clip: --at, --source-start-seconds, --source-end-seconds,
    /// --lead-ms, --fade-in-ms, --fade-out-ms, --source-bpm and --stretch are optional.
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

/// Clips, addressed by reference: @N while a host runs, else tracks.T.clips.I.
#[derive(Subcommand)]
enum ClipCmd {
    /// Add a clip: --at, --repeats and --velocity-scale are optional.
    Add {
        project: Song,
        track: String,
        pattern: String,
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
    /// Copy a clip, by default right after the original.
    Duplicate {
        project: Song,
        clip: String,
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        at: Option<String>,
    },
    Remove { project: Song, clip: String },
}

/// Patterns and their events.
#[derive(Subcommand)]
enum PatternCmd {
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
    Add {
        project: Song,
        pattern: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Change an event's fields; a null value resets one.
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
    Add {
        project: Song,
        track: String,
        pad: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Change a pad's fields; a null value resets one.
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

/// Sends from a track to a return.
#[derive(Subcommand)]
enum SendCmd {
    /// Add or change a send: --gain-db, --pre-fader.
    Set {
        project: Song,
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
    Set { project: Song, owner: String, param: String, points: String },
    Remove { project: Song, owner: String, param: String },
    #[command(subcommand)]
    Point(PointCmd),
}

/// Automation points, addressed by reference.
#[derive(Subcommand)]
enum PointCmd {
    /// Add a point in time order, creating the lane if needed: --at, --value, --curve, --shape.
    Add {
        project: Song,
        owner: String,
        param: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Change a point's --at, --value, --curve or --shape.
    Move {
        project: Song,
        point: String,
        #[command(flatten)]
        f: FieldArgs,
    },
    /// Remove a point; a lane's last point takes the lane with it.
    Remove { project: Song, point: String },
}

/// Sections.
#[derive(Subcommand)]
enum SectionCmd {
    Add { project: Song, id: String, at: String, length: String },
    Move { project: Song, section: String, at: String },
    Remove { project: Song, section: String },
}

type Result<T> = std::result::Result<T, String>;

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
            return Err(format!("Expected --FIELD VALUE, got {}", args[i]));
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
        Top::Init { directory, tempo, bars } => {
            let path = project::create(directory, *tempo, *bars, None)?;
            Ok(json!({"project": std::fs::canonicalize(&path).map_err(text)?}))
        }
        Top::Projects { all } => projects(*all),
        Top::Move { project, new_folder } => edit(project, C::Move { to: absolute(new_folder)? }),
        Top::Copy { project, new_folder } => edit(project, C::Copy { to: absolute(new_folder)? }),
        Top::Describe { topic } => {
            let name = topic.to_possible_value().expect("a topic has a name");
            contract::describe(name.get_name()).ok_or_else(|| "Unknown topic".to_string())
        }
        Top::Samples(_)
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
            ClipCmd::Add { project, track, pattern, f } => {
                let mut all = Fields::new();
                all.insert("pattern".into(), json!(pattern));
                all.extend(fields(f)?);
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
            ClipCmd::Duplicate { project, clip, track, at } => edit(
                project,
                C::ClipDuplicate {
                    clip: clip.clone(),
                    track: track.clone(),
                    at: at.as_deref().map(parse_value),
                },
            ),
            ClipCmd::Remove { project, clip } => edit(project, C::ClipRemove { clip: clip.clone() }),
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
            SectionCmd::Move { project, section, at } => edit(
                project,
                C::SectionMove {
                    section: section.clone(),
                    at: parse_value(at),
                },
            ),
            SectionCmd::Remove { project, section } => edit(project, C::SectionRemove { section: section.clone() }),
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
        Top::Get { .. } => "get".into(),
        Top::Undo { .. } => "undo".into(),
        Top::Redo { .. } => "redo".into(),
        Top::Set { .. } => "set".into(),
        Top::Toggle { .. } => "toggle".into(),
        Top::Remove { .. } => "remove".into(),
        Top::Batch { .. } => "batch".into(),
        Top::Track(_) => group("track", ""),
        Top::Return(_) => group("return", ""),
        Top::Clip(_) => group("clip", ""),
        Top::Audio(_) => group("audio", ""),
        Top::Pattern(_) => group("pattern", ""),
        Top::Pad(_) => group("pad", ""),
        Top::Effect(_) => group("effect", ""),
        Top::Send(_) => group("send", ""),
        Top::Lane(_) => group("lane", ""),
        Top::Section(_) => group("section", ""),
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
            println!("{}", serde_json::to_string_pretty(&result).expect("json"));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", json!({"error": error, "command": name(&cli.command)}));
            ExitCode::FAILURE
        }
    }
}
