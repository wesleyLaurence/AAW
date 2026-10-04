//! A running session host: owns a project, answers requests on its socket one
//! at a time, saves after each change, reloads external edits, and plays the
//! song, recompiling it after each change so edits are heard during playback.
//! `run` hosts on the calling thread, as `daw host` does; `spawn` hosts on a
//! thread of its own for a process that embeds the host and watches its events.

use crate::client::Request;
use crate::command::{beat_value, json_value, Command, Kind, Origin};
use crate::registry;
use crate::session::{Change, Doc, History, Session};
use aaw_engine::player::Shared;
use aaw_engine::program::{compile_cached, Cache, Program};
use aaw_engine::realtime::Transport;
use aaw_model::Beat;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};
use serde_json::{json, Map, Value as Json};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, String>;

/// How often the host checks song.yaml for external edits.
const SYNC_EVERY: Duration = Duration::from_millis(200);
/// How long after a gesture's last edit the host saves.
const SAVE_AFTER: Duration = Duration::from_millis(250);

const CLOSING: &str = "The host is closing";

/// Holds the project's advisory lock, which every writer without a host takes.
pub fn lock(project: &Path) -> Result<File> {
    let dir = match project.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let file = File::options()
        .append(true)
        .create(true)
        .open(dir.join(".daw.lock"))
        .map_err(|e| e.to_string())?;
    file.lock().map_err(|e| e.to_string())?;
    Ok(file)
}

pub struct Options {
    /// Output buffer in frames.
    pub buffer: u32,
    /// Start playing from this beat.
    pub play: Option<Json>,
    /// Exit once playback stops: `daw play` hosting for its own duration.
    pub exit_on_stop: bool,
    /// Stop after this many seconds of playback.
    pub seconds: Option<f64>,
    /// Print each change to stderr as a JSON line.
    pub feed: bool,
    /// Compile each revision as it lands, so the first play starts at once.
    pub prepare: bool,
}

/// What a host tells the process that embeds it.
#[derive(Clone)]
pub enum Event {
    /// The song as opened, before any change.
    Opened(Doc),
    /// A new revision of the song, with what undo and redo would do after it.
    Changed { change: Change, doc: Doc, history: History },
    /// A revision's program, once it is compiled: after `Opened` or the
    /// revision's `Changed`, for a host that prepares or plays each revision.
    /// A revision that does not compile has none.
    Compiled { revision: u64, program: Arc<Program> },
    /// The play state, the start position or the loop changed.
    Transport(TransportState),
    /// song.yaml holds an external edit that does not load, or it was fixed.
    Invalid(Option<String>),
    /// The project was saved under another name and the song is now at this
    /// path. The `Changed` that records it follows.
    Moved(PathBuf),
    Warning(String),
    /// The host has saved and shut down.
    Closed,
}

/// The transport as a display needs it, in beats.
#[derive(Clone, Debug, PartialEq)]
pub struct TransportState {
    pub metronome: bool,
    pub playing: bool,
    /// Where play starts.
    pub cue: f64,
    /// Loop start and length.
    pub region: Option<(f64, f64)>,
}

/// The playhead, readable from any thread without waiting for the host.
#[derive(Default)]
pub struct Clock {
    /// What the audio thread publishes, and beats per frame of its program.
    source: Mutex<Option<(Arc<Shared>, f64)>>,
}

impl Clock {
    /// Whether the song is playing and the beat that is being heard: where
    /// the audio thread is, less what the output has yet to play. None until
    /// the output has been opened by a first play.
    pub fn now(&self) -> Option<(bool, f64)> {
        let source = self.source.lock().unwrap_or_else(|e| e.into_inner());
        source.as_ref().map(|(shared, beats_per_frame)| {
            let playing = shared.playing.load(Relaxed);
            let ahead = if playing { shared.output_latency.load(Relaxed) } else { 0 };
            (playing, shared.position.load(Relaxed).saturating_sub(ahead) as f64 * beats_per_frame)
        })
    }

    fn set(&self, source: Option<(Arc<Shared>, f64)>) {
        *self.source.lock().unwrap_or_else(|e| e.into_inner()) = source;
    }
}

type Observer = Box<dyn FnMut(Event) + Send>;
type Inbox = mpsc::Sender<(Request, mpsc::Sender<Json>)>;

/// A path the host answers at: its registration and the thread that
/// listens on its socket.
struct Door {
    claim: registry::Claim,
    listening: JoinHandle<()>,
    /// Set to stop this door alone; `done` stops them all.
    shut: Arc<AtomicBool>,
}

struct Host {
    session: Session,
    /// The song's path as the registry has it, which each reply carries.
    canonical: PathBuf,
    /// Where the host answers: the project's path, and the paths it had
    /// before it was saved under another name.
    doors: Vec<Door>,
    inbox: Inbox,
    done: Arc<AtomicBool>,
    /// Whether the project's window is in front in the app.
    front: bool,
    opts: Options,
    observer: Option<Observer>,
    clock: Arc<Clock>,
    reported_transport: Option<TransportState>,
    cache: Cache,
    /// The compiled song and the SHA it was compiled from.
    program: Option<(String, Arc<Program>)>,
    transport: Option<Transport>,
    metronome: bool,
    /// Where play starts, in beats.
    cue: BigRational,
    /// Loop start and length, in beats.
    region: Option<(BigRational, BigRational)>,
    playback_error: Option<String>,
    reported_invalid: Option<String>,
    /// What the person has selected in the app, by handle.
    selection: Vec<u64>,
    /// When to save a gesture's edits, which are not saved one by one.
    save_at: Option<Instant>,
    heard: bool,
    stopped_at: Option<Instant>,
    closing: bool,
    from: BigRational,
}

fn beat(j: &Json) -> Result<BigRational> {
    let b = match json_value(j)? {
        aaw_model::value::Value::Int(n) => Beat::Int(n),
        aaw_model::value::Value::Float(f) => Beat::Float(f),
        aaw_model::value::Value::Str(s) => Beat::Str(s),
        other => return Err(format!("Invalid beat value {}", aaw_model::value::py_repr(&other))),
    };
    aaw_model::beat(&b)
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

impl Host {
    fn emit(&self, line: Json) {
        if self.opts.feed {
            eprintln!("{line}");
        }
    }

    fn notify(&mut self, event: Event) {
        if let Some(observer) = &mut self.observer {
            observer(event);
        }
    }

    fn warn(&mut self, message: String) {
        eprintln!("{}", json!({"warning": message}));
        self.notify(Event::Warning(message));
    }

    fn frame(&self, beats: &BigRational) -> usize {
        let s = &self.session.project().session;
        aaw_model::frame(beats, s.tempo, s.sample_rate).max(0) as usize
    }

    fn check_in_song(&self, beats: &BigRational, what: &str) -> Result<()> {
        if beats >= &self.session.project().session.length_exact() {
            return Err(format!("{what} {} is at or after the session end", aaw_model::fraction_str(beats)));
        }
        Ok(())
    }

    /// The program for the current revision, compiling it if needed. A compile
    /// redoes only what the revision changed: each track's voices, prepared
    /// sample audio and reverb kernels are kept from the last one.
    fn program(&mut self) -> Result<Arc<Program>> {
        let sha = self.session.doc().sha.clone();
        if let Some((s, p)) = &self.program {
            if *s == sha {
                return Ok(p.clone());
            }
        }
        let p = Arc::new(compile_cached(self.session.project(), self.session.dir(), &mut self.cache)?);
        self.program = Some((sha, p.clone()));
        Ok(p)
    }

    fn loop_frames(&self) -> Option<(usize, usize)> {
        self.region
            .as_ref()
            .map(|(start, length)| (self.frame(start), self.frame(&(start + length))))
    }

    fn open_transport(&mut self, program: Arc<Program>) -> Result<()> {
        let mut t = Transport::open(program, self.opts.buffer)?;
        t.control.set_loop(self.loop_frames())?;
        t.control.set_metronome(self.metronome)?;
        self.transport = Some(t);
        self.publish_clock();
        Ok(())
    }

    /// Points the clock at the open output and its program's tempo.
    fn publish_clock(&self) {
        self.clock.set(self.transport.as_ref().map(|t| {
            let p = t.control.program();
            (t.control.shared().clone(), p.tempo / 60.0 / p.rate as f64)
        }));
    }

    /// Sends the current revision to the open output.
    fn reload(&mut self) -> Result<()> {
        let p = self.program()?;
        let t = self.transport.as_mut().expect("transport");
        if p.rate == t.rate {
            t.control.load(p)?;
        } else {
            // Another sample rate needs another stream, at the same beat.
            let old = self.transport.take().expect("transport");
            let (playing, frame, old_program) = (old.control.playing(), old.control.position(), old.control.program().clone());
            drop(old);
            let beats =
                BigRational::from_float(frame as f64 * old_program.tempo / 60.0 / old_program.rate as f64).unwrap_or_default();
            self.open_transport(p)?;
            if playing {
                let f = self.frame(&beats);
                self.transport.as_mut().expect("transport").control.play(f)?;
            }
        }
        self.publish_clock();
        let region = self.loop_frames();
        self.transport.as_mut().expect("transport").control.set_loop(region)
    }

    /// Sends the current revision to the audio thread, or with `prepare` and
    /// no output open yet, compiles it for the first play.
    fn refresh(&mut self) {
        let result = if self.transport.is_some() {
            self.reload()
        } else if self.opts.prepare {
            self.program().map(|_| ())
        } else {
            return;
        };
        self.playback_error = result.err();
        match (self.playback_error.clone(), self.program.clone()) {
            (Some(e), _) => self.warn(format!("playback keeps the previous version: {e}")),
            (None, Some((_, program))) => self.notify(Event::Compiled {
                revision: self.session.revision(),
                program,
            }),
            (None, None) => {}
        }
    }

    /// Reports a change and plays it.
    fn changed(&mut self, change: &Change) {
        self.emit(json!({"change": change}));
        self.notify(Event::Changed {
            change: change.clone(),
            doc: self.session.doc().clone(),
            history: self.session.history(),
        });
        self.refresh();
    }

    fn sync(&mut self) {
        if let Some(c) = self.session.sync() {
            self.changed(&c);
        }
        let invalid = self.session.invalid().map(str::to_string);
        if invalid != self.reported_invalid {
            if let Some(e) = &invalid {
                self.emit(json!({"warning": format!("song.yaml does not load; edits wait until it is fixed: {e}")}));
            }
            self.notify(Event::Invalid(invalid.clone()));
            self.reported_invalid = invalid;
        }
    }

    fn transport_state(&self) -> TransportState {
        let beats = |x: &BigRational| x.to_f64().unwrap_or(0.0);
        TransportState {
            metronome: self.metronome,
            playing: self.transport.as_ref().is_some_and(|t| t.control.playing()),
            cue: beats(&self.cue),
            region: self.region.as_ref().map(|(start, length)| (beats(start), beats(length))),
        }
    }

    /// Tells the observer when the transport's state has changed.
    fn report_transport(&mut self) {
        if self.observer.is_none() {
            return;
        }
        let state = self.transport_state();
        if self.reported_transport.as_ref() != Some(&state) {
            self.reported_transport = Some(state.clone());
            self.notify(Event::Transport(state));
        }
    }

    fn play(&mut self, from: Option<&Json>) -> Result<Json> {
        let cue = match from {
            Some(f) => beat(f)?,
            None => self.cue.clone(),
        };
        self.check_in_song(&cue, "--from")?;
        self.cue = cue;
        let program = self.program()?;
        if self.transport.is_none() {
            self.open_transport(program)?;
        }
        let f = self.frame(&self.cue);
        self.from = self.cue.clone();
        self.transport.as_mut().expect("transport").control.play(f)?;
        Ok(self.status())
    }

    /// `note.preview`: a note played now through a MIDI track's Synth and its
    /// chain, from where the stream stands, outside the timeline and the
    /// history. Opens the output if no play has yet.
    fn preview(&mut self, track: &str, pitch: &Json, velocity: Option<i64>, length_beats: Option<&Json>) -> Result<Json> {
        let pitch = match pitch {
            Json::Number(n) => n.as_i64().ok_or_else(|| format!("pitch {n} is not a whole number"))?,
            Json::String(s) => s.parse::<i64>().or_else(|_| aaw_model::rules::midi(s))?,
            other => return Err(format!("pitch must be a MIDI number or a note name, not {other}")),
        };
        if !(0..=127).contains(&pitch) {
            return Err(format!("pitch {pitch} is outside 0 to 127"));
        }
        let velocity = velocity.unwrap_or(100);
        if !(1..=127).contains(&velocity) {
            return Err("velocity is 1 to 127".into());
        }
        let length = match length_beats {
            Some(l) => beat(l).map_err(|e| format!("length_beats: {e}"))?.to_f64().unwrap_or(0.0),
            None => 1.0,
        };
        if !(length > 0.0 && length <= 64.0) {
            return Err("length_beats is more than 0 and at most 64".into());
        }
        let session = &self.session.project().session;
        let t = self.session.project().track(track).ok_or_else(|| format!("Unknown track: {track}"))?;
        if t.midi.as_ref().and_then(|m| m.synth()).is_none() {
            return Err(format!("{track} has no synth to play the note; daw synth add attaches one"));
        }
        let frames = (length * 60.0 / session.tempo * session.sample_rate as f64).round() as usize;
        let program = self.program()?;
        if self.transport.is_none() {
            self.open_transport(program.clone())?;
        }
        let t = self.transport.as_mut().expect("transport");
        // The track's place in the program the output plays.
        let playing = t.control.program().clone();
        let index = playing
            .tracks
            .iter()
            .position(|p| p.id == track && p.synth.is_some())
            .ok_or_else(|| format!("{track} is not yet playing its synth; playback keeps the previous version"))?;
        t.control.preview(index, pitch as f64, velocity as f64 / 127.0, frames)?;
        Ok(json!({
            "track": track,
            "pitch": pitch,
            "note": aaw_model::rules::note_name(pitch).unwrap_or_default(),
            "velocity": velocity,
            "length_beats": length,
            "frames": frames,
        }))
    }

    fn transport(&mut self, cmd: &Command) -> Result<Json> {
        match cmd {
            Command::Play { from } => self.play(from.as_ref()),
            Command::Metronome { enabled } => {
                if let Some(t) = &mut self.transport {
                    t.control.set_metronome(*enabled)?;
                }
                self.metronome = *enabled;
                Ok(self.status())
            }
            Command::Stop => {
                if let Some(t) = &mut self.transport {
                    t.control.stop()?;
                }
                Ok(self.status())
            }
            Command::NotePreview { track, pitch, velocity, length_beats } => self.preview(track, pitch, *velocity, length_beats.as_ref()),
            Command::Locate { at } => {
                let at = beat(at)?;
                self.check_in_song(&at, "locate")?;
                self.cue = at;
                let f = self.frame(&self.cue);
                if let Some(t) = &mut self.transport {
                    t.control.locate(f)?;
                }
                Ok(self.status())
            }
            Command::Loop { start, length } => {
                self.region = match (start, length) {
                    (None, None) => None,
                    (Some(s), Some(l)) => {
                        let (s, l) = (beat(s)?, beat(l)?);
                        if !l.is_positive() {
                            return Err("Loop length must be positive".into());
                        }
                        if &s + &l > self.session.project().session.length_exact() {
                            return Err("The loop ends after the session end".into());
                        }
                        Some((s, l))
                    }
                    _ => return Err("A loop needs a start and a length".into()),
                };
                let frames = self.loop_frames();
                if let Some(t) = &mut self.transport {
                    t.control.set_loop(frames)?;
                }
                Ok(self.status())
            }
            _ => unreachable!("not a transport command"),
        }
    }

    fn status(&mut self) -> Json {
        let mut m: Map<String, Json> = self.session.status();
        m.insert("pid".into(), json!(std::process::id()));
        let s = self.session.project().session.clone();
        let (playing, frame, tempo, rate) = match &self.transport {
            Some(t) => {
                let p = t.control.program();
                (t.control.playing(), t.control.position(), p.tempo, p.rate as f64)
            }
            None => (false, self.frame(&self.cue), s.tempo, s.sample_rate as f64),
        };
        m.insert("playing".into(), json!(playing));
        m.insert("metronome".into(), json!(self.metronome));
        m.insert(
            "position".into(),
            json!({"frame": frame, "beat": round3(frame as f64 * tempo / 60.0 / rate), "seconds": round3(frame as f64 / rate)}),
        );
        m.insert("cue".into(), crate::session::value_json(&beat_value(&self.cue)));
        m.insert(
            "loop".into(),
            match &self.region {
                Some((start, length)) => json!({
                    "start": crate::session::value_json(&beat_value(start)),
                    "length": crate::session::value_json(&beat_value(length)),
                }),
                None => Json::Null,
            },
        );
        m.insert(
            "audio".into(),
            match &self.transport {
                Some(t) => {
                    let r = crate::session::value_json(&t.report());
                    json!({
                        "device": r["device"],
                        "sample_rate": r["sample_rate"],
                        "buffer_frames": r["buffer_frames"],
                        "output_latency_frames": r["output_latency_frames"],
                        "budget_ms": r["budget_ms"],
                        "mean_callback_ms": r["mean_callback_ms"],
                        "max_callback_ms": r["max_callback_ms"],
                        "over_budget_callbacks": r["over_budget_callbacks"],
                        "xruns": r["xruns"],
                    })
                }
                None => Json::Null,
            },
        );
        if let Some((_, p)) = &self.program {
            m.insert("latency_frames".into(), json!(p.latency));
        }
        m.insert("playback_error".into(), json!(self.playback_error));
        m.insert("selection".into(), json!(self.selected()));
        m.insert("front".into(), json!(self.front));
        m.insert("untitled".into(), json!(crate::project::is_untitled(&self.canonical)));
        Json::Object(m)
    }

    /// The selection as references and paths, without what has been removed.
    fn selected(&mut self) -> Vec<Json> {
        let root = self.session.doc().tree();
        let located: Vec<(u64, Json)> = self
            .selection
            .iter()
            .filter_map(|h| self.session.located(&root, *h).map(|j| (*h, j)))
            .collect();
        self.selection = located.iter().map(|(h, _)| *h).collect();
        located.into_iter().map(|(_, j)| j).collect()
    }

    fn handle(&mut self, request: Request) -> Result<Json> {
        let Request {
            command,
            origin,
            expect,
            gesture,
        } = request;
        if origin == Origin::External {
            return Err("external is the origin of edits made to the file".into());
        }
        match command.kind() {
            Kind::Edit | Kind::History => {
                let _lock = lock(self.session.path())?;
                self.sync();
                let (reply, change) = match &command {
                    Command::Undo => self.session.undo(origin, false).map(|(r, c)| (r, Some(c)))?,
                    Command::Redo => self.session.undo(origin, true).map(|(r, c)| (r, Some(c)))?,
                    _ => self.session.edit(&command, origin, expect.as_deref(), gesture.as_deref())?,
                };
                // A drag sends many edits a second; they are saved once it pauses.
                if gesture.is_some() && matches!(command.kind(), Kind::Edit) {
                    self.save_at = Some(Instant::now() + SAVE_AFTER);
                } else {
                    self.session.save()?;
                    self.save_at = None;
                }
                if let Some(c) = change {
                    self.changed(&c);
                }
                Ok(reply)
            }
            Kind::Transport => {
                self.sync();
                self.transport(&command)
            }
            Kind::Read => {
                self.sync();
                match command {
                    Command::Inspect => Ok(self.session.inspect()),
                    Command::Status => Ok(self.status()),
                    Command::Changes { since } => Ok(self.session.changes(since)),
                    Command::Get { path } => self.session.get(&path),
                    Command::Notes { path, from, to } => self.session.notes(&path, from.as_ref(), to.as_ref()),
                    Command::MidiExport { clip, file } => self.session.export_midi(&clip, &file),
                    _ => unreachable!("not a read"),
                }
            }
            Kind::Host => match command {
                Command::Select { items } => {
                    let root = self.session.doc().tree();
                    let mut selection = Vec::new();
                    for item in &items {
                        let handle = self.session.handle(&root, item)?;
                        if !selection.contains(&handle) {
                            selection.push(handle);
                        }
                    }
                    self.selection = selection;
                    Ok(json!({"selection": self.selected()}))
                }
                Command::Front { front } => {
                    self.front = front;
                    Ok(json!({"front": front}))
                }
                Command::Move { ref to } => self.relocate(to, false, origin, command.json()),
                Command::Copy { ref to } => self.relocate(to, true, origin, command.json()),
                Command::Release { project } => self.release(Path::new(&project)),
                Command::Fmt => {
                    let _lock = lock(self.session.path())?;
                    self.session.write()?;
                    if self.reported_invalid.take().is_some() {
                        self.notify(Event::Invalid(None));
                    }
                    Ok(json!({"project": self.session.path(), "formatted": true}))
                }
                Command::Close => {
                    self.closing = true;
                    Ok(json!({"closed": true, "revision": self.session.revision()}))
                }
                _ => unreachable!("not a host command"),
            },
        }
    }

    /// Starts answering at a registration.
    fn listen_at(&mut self, listener: UnixListener, claim: registry::Claim) {
        let shut = Arc::new(AtomicBool::new(false));
        let listening = {
            let (tx, done, shut) = (self.inbox.clone(), self.done.clone(), shut.clone());
            std::thread::spawn(move || listen(listener, tx, done, shut))
        };
        self.doors.push(Door { claim, listening, shut });
    }

    /// Saves the project as the folder `to`: a move, or a copy that leaves
    /// the original. The host carries on there with its history and what is
    /// playing, registers under the new path and keeps answering at the old.
    fn relocate(&mut self, to: &str, copy: bool, origin: Origin, command: Json) -> Result<Json> {
        let _lock = lock(self.session.path())?;
        self.sync();
        self.save_at = None;
        let from = std::fs::canonicalize(self.session.dir()).map_err(|e| e.to_string())?;
        let folder = crate::project::target(Path::new(to), &from)?;
        let file = self.session.path().file_name().map(PathBuf::from).unwrap_or_default();
        // Claimed before anything moves, so that a refusal changes nothing.
        let entry = registry::entry(&folder.join(&file))?;
        let (listener, claim) = registry::claim(&entry)?;
        let (reply, change) = self.session.relocate(&folder, copy, origin, command)?;
        self.canonical = std::fs::canonicalize(self.session.path()).unwrap_or_else(|_| entry.project.clone());
        let stale: Vec<String> = self
            .doors
            .iter()
            .filter_map(|door| {
                let said = door.claim.moved(&self.canonical);
                said.err().map(|e| format!("{} was not updated: {e}", door.claim.entry().info.display()))
            })
            .collect();
        stale.into_iter().for_each(|message| self.warn(message));
        self.listen_at(listener, claim);
        // The folder as the file system names it, if that is not as it was written.
        if self.canonical != entry.project {
            match registry::entry(&self.canonical).and_then(|e| registry::claim(&e)) {
                Ok((listener, claim)) => self.listen_at(listener, claim),
                Err(e) => self.warn(format!("{} is not registered: {e}", self.canonical.display())),
            }
        }
        self.emit(json!({"host": {"project": self.canonical, "pid": std::process::id(), "revision": change.revision}}));
        self.notify(Event::Moved(self.session.path().to_path_buf()));
        self.changed(&change);
        Ok(reply)
    }

    /// Stops answering for a path the project had, so that the project there
    /// can have a host of its own.
    fn release(&mut self, project: &Path) -> Result<Json> {
        let entry = registry::entry(project)?;
        let released = entry.project != self.canonical
            && self.doors.iter().any(|door| door.claim.entry().socket == entry.socket);
        if released {
            let (gone, kept): (Vec<Door>, Vec<Door>) =
                std::mem::take(&mut self.doors).into_iter().partition(|door| door.claim.entry().socket == entry.socket);
            self.doors = kept;
            for door in gone {
                door.shut.store(true, Relaxed);
                let _ = door.listening.join();
            }
        }
        Ok(json!({"released": released, "project": entry.project}))
    }

    /// Saves a gesture's edits once it has paused.
    fn save_due(&mut self) {
        if self.save_at.is_none_or(|at| Instant::now() < at) {
            return;
        }
        self.save_at = None;
        let saved = lock(self.session.path()).and_then(|_lock| {
            // An edit of the file made meanwhile is loaded, not overwritten.
            self.sync();
            self.session.save()
        });
        if let Err(e) = saved {
            self.warn(format!("song.yaml was not saved: {e}"));
        }
    }

    /// Housekeeping between requests. True when the host should exit.
    fn tick(&mut self) -> bool {
        self.save_due();
        if let Some(t) = &mut self.transport {
            t.control.collect();
            let played = t.control.shared().played.load(Relaxed);
            if played > 0 {
                self.heard = true;
            }
            if let Some(limit) = self.opts.seconds {
                if played as f64 >= limit * t.rate as f64 && t.control.playing() {
                    let _ = t.control.stop();
                }
            }
            if self.opts.exit_on_stop && self.heard && !t.control.playing() {
                // Let the stop's fade-out play.
                let since = *self.stopped_at.get_or_insert_with(Instant::now);
                return since.elapsed() > Duration::from_millis(50);
            }
            self.stopped_at = None;
        }
        false
    }
}

/// Reads one request from a connection and writes the host's reply.
fn answer(stream: UnixStream, requests: Inbox) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut line = String::new();
    let Ok(read) = stream.try_clone() else { return };
    if BufReader::new(read).read_line(&mut line).is_err() {
        return;
    }
    let reply = match serde_json::from_str::<Request>(&line) {
        Err(e) => json!({"error": format!("Bad request: {e}")}),
        Ok(request) => {
            let (tx, rx) = mpsc::channel();
            if requests.send((request, tx)).is_err() {
                json!({"error": CLOSING})
            } else {
                rx.recv().unwrap_or_else(|_| json!({"error": CLOSING}))
            }
        }
    };
    let mut stream = stream;
    let _ = writeln!(stream, "{reply}");
}

fn listen(listener: UnixListener, requests: Inbox, done: Arc<AtomicBool>, shut: Arc<AtomicBool>) {
    let _ = listener.set_nonblocking(true);
    while !done.load(Relaxed) && !shut.load(Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let tx = requests.clone();
                std::thread::spawn(move || answer(stream, tx));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// What a host with its socket open serves from.
struct Serving {
    rx: mpsc::Receiver<(Request, mpsc::Sender<Json>)>,
}

/// Claims the project, loads it and starts listening on its socket.
fn open(project: &Path, opts: Options, observer: Option<Observer>, clock: Arc<Clock>) -> Result<(Host, Serving, Inbox)> {
    let entry = registry::entry(project)?;
    let (listener, claim) = registry::claim(&entry)?;
    let session = Session::open(project, true)?;
    let (tx, rx) = mpsc::channel::<(Request, mpsc::Sender<Json>)>();
    let mut host = Host {
        session,
        canonical: entry.project.clone(),
        doors: Vec::new(),
        inbox: tx.clone(),
        done: Arc::new(AtomicBool::new(false)),
        front: false,
        opts,
        observer,
        clock,
        reported_transport: None,
        cache: Cache::default(),
        program: None,
        transport: None,
        metronome: false,
        cue: BigRational::zero(),
        region: None,
        playback_error: None,
        reported_invalid: None,
        selection: Vec::new(),
        save_at: None,
        heard: false,
        stopped_at: None,
        closing: false,
        from: BigRational::zero(),
    };
    host.listen_at(listener, claim);
    host.emit(json!({"host": {
        "project": host.session.path(),
        "pid": std::process::id(),
        "socket": entry.socket,
        "revision": 0,
    }}));
    let doc = host.session.doc().clone();
    host.notify(Event::Opened(doc));
    host.report_transport();
    Ok((host, Serving { rx }, tx))
}

/// Answers requests until `interrupted` is set, a `close` request arrives or,
/// with `exit_on_stop`, playback stops. Then saves, unregisters and returns a
/// summary, with the audio report when the song played.
fn serve(mut host: Host, serving: Serving, interrupted: Arc<AtomicBool>) -> Result<Json> {
    let Serving { rx } = serving;
    let started = match host.opts.play.clone() {
        Some(from) => host.play(Some(&from)).map(|_| ()),
        None => {
            host.refresh();
            Ok(())
        }
    };
    let mut last_sync = Instant::now();
    while started.is_ok() {
        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok((request, reply)) => {
                let result = host.handle(request);
                let _ = reply.send(match result {
                    Ok(v) => json!({"ok": v, "project": host.canonical}),
                    Err(e) => json!({"error": e}),
                });
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if last_sync.elapsed() >= SYNC_EVERY {
            host.sync();
            last_sync = Instant::now();
        }
        let finished = host.tick();
        host.report_transport();
        if finished || host.closing || interrupted.load(Relaxed) {
            break;
        }
    }
    host.done.store(true, Relaxed);
    let doors = std::mem::take(&mut host.doors);
    let claims: Vec<registry::Claim> = doors
        .into_iter()
        .map(|door| {
            let _ = door.listening.join();
            door.claim
        })
        .collect();
    // Answer requests that arrived while closing.
    while let Ok((_, reply)) = rx.try_recv() {
        let _ = reply.send(json!({"error": CLOSING}));
    }
    drop(claims);
    if let Some(t) = &mut host.transport {
        if t.control.playing() {
            let _ = t.control.stop();
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    let saved = host.session.save();
    host.clock.set(None);
    host.notify(Event::Closed);
    started?;
    saved?;
    let mut summary = Map::new();
    summary.insert("project".into(), json!(host.session.path()));
    summary.insert("revision".into(), json!(host.session.revision()));
    summary.insert("project_sha256".into(), json!(host.session.doc().sha));
    if let Some(t) = host.transport.take() {
        if let Json::Object(report) = crate::session::value_json(&t.report()) {
            summary.insert("from_beat".into(), crate::session::value_json(&beat_value(&host.from)));
            summary.extend(report);
        }
        t.close();
    }
    Ok(Json::Object(summary))
}

/// Hosts a project on the calling thread until `interrupted` is set, a `close`
/// request arrives or, with `exit_on_stop`, playback stops. Returns a summary,
/// with the audio report when the song played.
pub fn run(project: &Path, opts: Options, interrupted: Arc<AtomicBool>) -> Result<Json> {
    let (host, serving, _) = open(project, opts, None, Arc::new(Clock::default()))?;
    serve(host, serving, interrupted)
}

/// A host on its own thread, for a process that embeds one, such as the app.
/// Other processes reach it through its socket as they reach `daw host`.
pub struct Running {
    requests: Inbox,
    clock: Arc<Clock>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<Json>>>,
}

/// Starts a host for a project. `observer` is called on the host's thread,
/// first with `Event::Opened`, and must return promptly.
pub fn spawn(project: &Path, opts: Options, observer: impl FnMut(Event) + Send + 'static) -> Result<Running> {
    let stop = Arc::new(AtomicBool::new(false));
    let clock = Arc::new(Clock::default());
    let (ready_tx, ready_rx) = mpsc::channel::<Result<Inbox>>();
    let thread = {
        let (project, stop, clock) = (project.to_path_buf(), stop.clone(), clock.clone());
        std::thread::Builder::new()
            .name("aaw-host".into())
            .spawn(move || match open(&project, opts, Some(Box::new(observer)), clock) {
                Ok((host, serving, requests)) => {
                    let _ = ready_tx.send(Ok(requests));
                    serve(host, serving, stop)
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e.clone()));
                    Err(e)
                }
            })
            .map_err(|e| e.to_string())?
    };
    let requests = ready_rx.recv().map_err(|_| "The host stopped while opening".to_string())??;
    Ok(Running {
        requests,
        clock,
        stop,
        thread: Some(thread),
    })
}

impl Running {
    /// Sends a request and waits for the host's answer, as a client does over
    /// the socket.
    pub fn request(&self, request: Request) -> Result<Json> {
        let (tx, rx) = mpsc::channel();
        self.requests.send((request, tx)).map_err(|_| CLOSING.to_string())?;
        let reply = rx.recv().map_err(|_| CLOSING.to_string())?;
        match reply.get("error") {
            Some(e) => Err(e.as_str().unwrap_or_default().to_string()),
            None => Ok(reply.get("ok").cloned().unwrap_or(Json::Null)),
        }
    }

    pub fn clock(&self) -> &Arc<Clock> {
        &self.clock
    }

    fn join(&mut self) -> Result<Json> {
        self.stop.store(true, Relaxed);
        match self.thread.take() {
            Some(t) => t.join().map_err(|_| "The host thread panicked".to_string())?,
            None => Err(CLOSING.into()),
        }
    }

    /// Saves, unregisters and stops the host. Returns the summary `run` returns.
    pub fn close(mut self) -> Result<Json> {
        self.join()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.join();
    }
}
