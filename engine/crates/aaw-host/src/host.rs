//! A running session host: owns a project, answers requests on its socket one
//! at a time, saves after each change, reloads external edits, and plays the
//! song, recompiling it after each change so edits are heard during playback.
//! `run` hosts on the calling thread, as `daw host` does; `spawn` hosts on a
//! thread of its own for a process that embeds the host and watches its events.

use crate::client::Request;
use crate::command::{beat_value, json_value, Command, Kind, Origin};
use crate::capture::Capture;
use crate::record::{Recording, Take};
use crate::registry;
use crate::session::{Change, Doc, History, Session};
use aaw_engine::capture::{Tap, TapSource};
use aaw_engine::player::Shared;
use aaw_engine::program::Taps;
use aaw_engine::program::{compile_cached, Cache, Program};
use aaw_engine::realtime::Transport;
use aaw_model::{Beat, Input};
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
/// How long an agent may send nothing before its next edit's reply carries a
/// hint to work in steps the person can watch.
pub const PACE: Duration = Duration::from_secs(300);

/// The hint an agent's edit carries when `gap` passed since its last command
/// and that is at least `pace`: with a host running, the person waited.
pub fn pace_hint(gap: Duration, pace: Duration) -> Option<String> {
    if gap < pace {
        return None;
    }
    let minutes = (gap.as_secs_f64() / 60.0).round().max(1.0) as u64;
    let waited = if minutes == 1 { "a minute".to_string() } else { format!("{minutes} minutes") };
    Some(format!(
        "A host is running and nothing reached the song for {waited}: work in steps the person can watch, \
         one sound, part or clip a command as soon as it is decided, so each appears on the timeline and in \
         the activity as it is made, and plan the next while the last plays. A batch is one musical edit, \
         not the whole song."
    ))
}

/// What an agent's edit is told of the markers the person left since its
/// last command, each `(id, where and what)`: None when there are none.
pub fn marker_news(fresh: &[(String, String)]) -> Option<String> {
    const LISTED: usize = 5;
    let (first, _) = fresh.split_first()?;
    let list: Vec<String> = fresh.iter().take(LISTED).map(|(id, place)| format!("{id} {place}")).collect();
    let more = if fresh.len() > LISTED { format!(", and {} more", fresh.len() - LISTED) } else { String::new() };
    Some(if fresh.len() == 1 {
        format!(
            "The person left a marker since your last command: {} {}. daw marker list reads it with what plays there.",
            first.0, first.1
        )
    } else {
        format!(
            "The person left {} markers since your last command: {}{more}. daw marker list reads them with what plays there.",
            fresh.len(),
            list.join("; ")
        )
    })
}

/// A revision's markers, each with its handle.
fn marker_handles(doc: &Doc) -> Vec<(u64, aaw_model::Marker)> {
    if doc.project.markers.is_empty() {
        return Vec::new();
    }
    let tree = doc.tree();
    let items = tree.get("markers").map_or(&[][..], crate::tree::Node::items);
    items.iter().map(|i| i.handle).zip(doc.project.markers.iter().cloned()).collect()
}

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
    /// The devices to play and record through, and the buffer.
    pub audio: Audio,
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
    /// How long an agent may send nothing before its next edit's reply
    /// carries a hint to work in smaller steps; `PACE` outside tests.
    pub pace: Duration,
}

/// The devices a host plays and records through: a device by part of its
/// name, ignoring case, or None for the system's default, which the host
/// follows as it changes.
#[derive(Clone, Debug, PartialEq)]
pub struct Audio {
    pub output: Option<String>,
    /// Kept and reported until recording exists; nothing opens it yet.
    pub input: Option<String>,
    /// The output buffer, in frames.
    pub buffer: u32,
    /// Open the output at this rate whatever the song's, for checking.
    pub device_rate: Option<u32>,
    /// A named output that is not connected is refused rather than giving
    /// way to the default: the terminal's way, since a scripted run that
    /// must not be heard must never fall through to the speakers. The app's
    /// kept choice falls back, and is taken up when the device returns.
    pub strict_output: bool,
}

impl Default for Audio {
    fn default() -> Audio {
        Audio {
            output: None,
            input: None,
            buffer: 128,
            device_rate: None,
            strict_output: false,
        }
    }
}

impl Audio {
    fn settings(&self) -> aaw_engine::realtime::Settings {
        aaw_engine::realtime::Settings {
            output: self.output.clone(),
            buffer: self.buffer,
            device_rate: self.device_rate,
        }
    }
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
    /// Whether armed tracks are recording, the count-in included.
    pub recording: bool,
    /// The tracks armed to record, by ID.
    pub armed: Vec<String>,
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
    /// The devices as chosen now, from the options until `audio` changes them.
    audio: Audio,
    /// The chosen output while it is not connected and the default plays.
    fallback: Option<String>,
    /// The devices' generation when the output was opened.
    devices_seen: u64,
    /// When a stopped stream was last opened again, so a device that keeps
    /// failing is tried once a second and not in a loop.
    revived_at: Option<Instant>,
    metronome: bool,
    /// The tracks armed to record, by ID. Host state, as the loop is.
    armed: Vec<String>,
    /// The takes being recorded, from Record to their end.
    recording: Option<Recording>,
    /// A sound being captured from an input with no track, for the agent
    /// panel's Record Audio.
    capture: Option<Capture>,
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
    /// When the agent's last command of any kind arrived.
    agent_at: Option<Instant>,
    /// The markers the person left or reworded that the agent has not been
    /// told of, by handle.
    fresh_markers: Vec<u64>,
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

    /// Where a marker dropped now lands: the beat being heard while the song
    /// plays, to a thousandth, and else the start position; never past the
    /// song's end.
    fn marker_beat(&self) -> Json {
        let heard = match self.clock.now() {
            Some((true, beat)) => aaw_model::beat(&Beat::Float((beat * 1000.0).round() / 1000.0)).ok(),
            _ => None,
        };
        let length = self.session.project().session.length_exact();
        let beat = heard.unwrap_or_else(|| self.cue.clone()).min(length);
        crate::session::value_json(&beat_value(&beat))
    }

    /// Notes the markers a change by the person added or reworded, for the
    /// agent's next edit to be told of. `before` is the song before it.
    fn note_markers(&mut self, before: &Doc) {
        let now = self.session.doc();
        let said = |d: &Doc| -> Vec<(String, String)> { d.project.markers.iter().map(|m| (m.id.clone(), m.text.clone())).collect() };
        if said(before) == said(now) {
            return;
        }
        // By handle, since an ID is given again once its marker is gone.
        let old = marker_handles(before);
        for (handle, m) in marker_handles(now) {
            let known = old.iter().any(|(h, o)| *h == handle && o.text == m.text);
            if !known && !self.fresh_markers.contains(&handle) {
                self.fresh_markers.push(handle);
            }
        }
    }

    /// The markers the agent has not been told of, which it now is: those
    /// still in the song, each with where it is and what it says.
    fn take_marker_news(&mut self) -> Option<String> {
        if self.fresh_markers.is_empty() {
            return None;
        }
        let doc = self.session.doc().clone();
        let meter = doc.project.session.meter();
        let now = marker_handles(&doc);
        let fresh: Vec<(String, String)> = std::mem::take(&mut self.fresh_markers)
            .into_iter()
            .filter_map(|handle| {
                let (_, m) = now.iter().find(|(h, _)| *h == handle)?;
                let place = crate::map::marker_place(m.at_exact().to_f64().unwrap_or(0.0), meter);
                Some((m.id.clone(), if m.text.is_empty() { format!("in {place}") } else { format!("in {place}: {}", m.text) }))
            })
            .collect();
        marker_news(&fresh)
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

    /// The output the settings name, or the one `AAW_OUTPUT_DEVICE` does,
    /// and whether its absence is an error.
    fn chosen_output_strictly(&self) -> (Option<String>, bool) {
        let blank = |n: &String| n.trim().is_empty();
        match self.audio.output.clone().filter(|n| !blank(n)) {
            Some(name) => (Some(name), self.audio.strict_output),
            None => (std::env::var("AAW_OUTPUT_DEVICE").ok().filter(|n| !blank(n)), true),
        }
    }

    fn chosen_output(&self) -> Option<String> {
        self.chosen_output_strictly().0
    }

    /// Opens the chosen output, or the default when the chosen one is not
    /// connected, which the status then says; a device named strictly that
    /// is not there is refused with its name.
    fn open_transport(&mut self, program: Arc<Program>) -> Result<()> {
        let mut settings = self.audio.settings();
        self.devices_seen = aaw_engine::devices::generation();
        let fallback = match self.chosen_output_strictly() {
            (Some(name), strict) if aaw_engine::devices::find_output(&name)?.is_none() => {
                if strict {
                    return Err(format!("No audio output device named {}; daw device list names them", name.trim()));
                }
                Some(name)
            }
            _ => None,
        };
        if let Some(name) = &fallback {
            settings.output = None;
            if self.fallback.as_deref() != Some(name) {
                self.warn(format!("{name} is not connected; playing through the default output"));
            }
        }
        self.fallback = fallback;
        let mut t = Transport::open(program, &settings)?;
        t.control.set_loop(self.loop_frames())?;
        t.control.set_metronome(self.metronome)?;
        self.transport = Some(t);
        self.publish_clock();
        Ok(())
    }

    /// Closes the output and opens it again for `program`, at the same beat
    /// and still playing if it was: another rate or another device.
    fn reopen(&mut self, program: Arc<Program>) -> Result<()> {
        let old = self.transport.take().expect("transport");
        let (playing, frame, old_program) = (old.control.playing(), old.control.position(), old.control.program().clone());
        drop(old);
        let beats = BigRational::from_float(frame as f64 * old_program.tempo / 60.0 / old_program.rate as f64).unwrap_or_default();
        self.open_transport(program)?;
        if playing {
            let f = self.frame(&beats);
            self.transport.as_mut().expect("transport").control.play(f)?;
        }
        self.publish_clock();
        let region = self.loop_frames();
        self.transport.as_mut().expect("transport").control.set_loop(region)
    }

    /// The device the chosen output means now: the chosen one when it is
    /// connected, else the system's default.
    fn output_now(&self) -> Option<String> {
        match self.chosen_output() {
            Some(name) => match aaw_engine::devices::output_name(&name) {
                Ok(Some(full)) => Some(full),
                _ => aaw_engine::devices::default_output_name(),
            },
            None => aaw_engine::devices::default_output_name(),
        }
    }

    /// Follows the Mac's devices: when the default changes, or the chosen
    /// output comes or goes, the song moves to the device that is now meant
    /// without a break in the beat, as every Mac app follows the default.
    fn follow_devices(&mut self) {
        let generation = aaw_engine::devices::generation();
        let Some(t) = &self.transport else {
            self.devices_seen = generation;
            return;
        };
        // A stream the driver stopped, as when its device changed rate under
        // it or went away, is opened again on the device now meant.
        let dead = t.dead() && self.revived_at.is_none_or(|at| at.elapsed() > Duration::from_secs(1));
        if generation == self.devices_seen && !dead {
            return;
        }
        self.devices_seen = generation;
        let now = self.output_now();
        if !dead && (now.is_none() || now.as_deref() == Some(t.device.as_str())) {
            return;
        }
        if dead {
            self.revived_at = Some(Instant::now());
            self.warn(format!("{} stopped the stream; opening the output again", t.device));
        }
        if let Some(program) = self.program.as_ref().map(|(_, p)| p.clone()) {
            if let Err(e) = self.reopen(program) {
                self.playback_error = Some(e.clone());
                self.warn(format!("the output could not be opened again: {e}"));
            }
        }
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
            return self.reopen(p);
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
            recording: self.recording.is_some(),
            armed: self.armed.clone(),
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
        // A play from elsewhere ends the take where it stands.
        let recorded = self.end_recording(None);
        self.cue = cue;
        let program = self.program()?;
        if self.transport.is_none() {
            self.open_transport(program)?;
        }
        let f = self.frame(&self.cue);
        self.from = self.cue.clone();
        self.transport.as_mut().expect("transport").control.play(f)?;
        Ok(self.status_with(recorded))
    }

    /// The status, with the takes a recording that just ended made.
    fn status_with(&mut self, recorded: Option<Json>) -> Json {
        let mut status = self.status();
        if let (Some(takes), Json::Object(m)) = (recorded, &mut status) {
            m.insert("recorded".into(), takes);
        }
        status
    }

    /// The track with `id` that can record: an audio track with an input.
    fn recordable(&self, id: &str) -> Result<Input> {
        let p = self.session.project();
        let track = p
            .tracks
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("No track {id}; daw inspect lists them"))?;
        if track.midi.is_some() {
            return Err(format!("{id} is a MIDI track, which records no audio"));
        }
        track
            .input
            .clone()
            .ok_or_else(|| format!("{id} has no input; daw set PROJECT tracks.{id}.input names a device, a track, a group or the mix"))
    }

    /// `arm`: a track with an input armed to record, or disarmed.
    fn arm(&mut self, track: &str, armed: bool) -> Result<Json> {
        if armed {
            self.recordable(track)?;
            if !self.armed.iter().any(|t| t == track) {
                self.armed.push(track.to_string());
            }
        } else {
            self.armed.retain(|t| t != track);
        }
        Ok(self.status())
    }

    /// The device `input` names in a track's input: the audio settings'
    /// input for `default`, else `AAW_INPUT_DEVICE`, else the Mac's.
    fn input_device(&self, device: &str) -> Option<String> {
        let blank = |n: &String| n.trim().is_empty();
        match device.trim() {
            "default" | "" => self
                .audio
                .input
                .clone()
                .filter(|n| !blank(n))
                .or_else(|| std::env::var("AAW_INPUT_DEVICE").ok().filter(|n| !blank(n))),
            name => Some(name.to_string()),
        }
    }

    /// `record.start`: the tracks named armed, then every armed track
    /// recording after a count-in of `count_in` bars.
    fn record_start(&mut self, tracks: &[String], count_in: Option<u32>) -> Result<Json> {
        if self.recording.is_some() {
            return Err("Already recording; daw record stop ends the take".into());
        }
        for track in tracks {
            self.arm(track, true)?;
        }
        // A track armed earlier that lost its input or went is left out.
        let armed: Vec<(String, Input)> = self
            .armed
            .clone()
            .iter()
            .filter_map(|t| self.recordable(t).ok().map(|i| (t.clone(), i)))
            .collect();
        self.armed = armed.iter().map(|(t, _)| t.clone()).collect();
        if armed.is_empty() {
            return Err("No track is armed; daw record arm PROJECT TRACK arms one with an input, or name the tracks".into());
        }
        let program = self.program()?;
        // A source that is not heard has nothing to record: said before the
        // output is opened.
        for (track, input) in &armed {
            let (kind, id) = match input {
                Input::Track(id) => (aaw_engine::program::StemKind::Track, id),
                Input::Group(id) => (aaw_engine::program::StemKind::Group, id),
                _ => continue,
            };
            let silent = match kind {
                aaw_engine::program::StemKind::Track => program.tracks.iter().find(|t| &t.id == id).map(|t| t.silent),
                _ => program.groups.iter().find(|g| &g.id == id).map(|g| g.silent),
            };
            if silent != Some(false) || program.stem_index(kind, id).is_none() {
                return Err(format!("{id} is muted or silenced by a solo, so {track} would record nothing"));
            }
        }
        if self.transport.is_none() {
            self.open_transport(program.clone())?;
        }
        let rate = program.rate;
        let bar = (rate as f64 * 60.0 / program.tempo * program.click_beats * program.beats_per_bar.max(1) as f64).round().max(1.0) as usize;
        let preroll = count_in.unwrap_or(1) as usize * bar;
        // The output's hardware latency, in the song's frames: what the
        // person heard the song late by. The driver's own lead is already in
        // the anchor, which is where each block plays.
        let output_latency = self.output_latency(rate);
        let shared = self.transport.as_ref().expect("transport").control.shared().clone();
        let mut takes = Vec::new();
        let mut tapped = 0;
        let mut open = || -> Result<()> {
            for (track, input) in &armed {
                let take = match input {
                    Input::Device { device, channels } => {
                        let name = self.input_device(device);
                        Take::from_device(track, input, name.as_deref(), channels, rate, output_latency)?
                    }
                    Input::Track(_) | Input::Group(_) | Input::Master => {
                        let source = match input {
                            Input::Track(id) => TapSource::Stem {
                                kind: aaw_engine::program::StemKind::Track,
                                id: id.clone(),
                            },
                            Input::Group(id) => TapSource::Stem {
                                kind: aaw_engine::program::StemKind::Group,
                                id: id.clone(),
                            },
                            _ => TapSource::Master,
                        };
                        if tapped >= aaw_engine::player::MAX_TAPS {
                            return Err(format!("At most {} tracks record the song's own sound at once", aaw_engine::player::MAX_TAPS));
                        }
                        let (feed, capture) = aaw_engine::capture::feed(2, rate, crate::record::RING_SECONDS);
                        self.transport.as_mut().expect("transport").control.tap(Box::new(Tap { source, feed }))?;
                        tapped += 1;
                        Take::from_tap(track, input, capture, rate)
                    }
                };
                takes.push(take);
            }
            Ok(())
        };
        if let Err(e) = open() {
            if tapped > 0 {
                let _ = self.transport.as_mut().expect("transport").control.untap();
            }
            return Err(e);
        }
        let start = self.frame(&self.cue);
        let loop_end = self.loop_frames().map(|(_, end)| end);
        self.from = self.cue.clone();
        self.transport.as_mut().expect("transport").control.play_after(start, preroll)?;
        self.recording = Some(Recording::new(takes, start as i64, preroll, loop_end, shared));
        Ok(self.status())
    }

    /// Ends the recording, if one runs: each take written and added to the
    /// song with its clip as one edit by the person, ending at `end`, a
    /// song frame, or at each take's last frame. What was recorded, or
    /// None when nothing was.
    fn end_recording(&mut self, end: Option<i64>) -> Option<Json> {
        let recording = self.recording.take()?;
        // The player hands its taps back at its next block; their last
        // frames are drained once they have.
        if let Some(t) = &mut self.transport {
            let tapped = recording.takes.iter().filter(|take| take.device().is_none()).count();
            if tapped > 0 {
                let _ = t.control.untap();
                let waited = Instant::now();
                let mut back = 0;
                while back < tapped && waited.elapsed() < Duration::from_millis(300) {
                    std::thread::sleep(Duration::from_millis(5));
                    back += t.control.collect_taps().len();
                }
            }
        }
        let project = self.session.project().clone();
        let rate = project.session.sample_rate as u32;
        let tempo = project.session.tempo;
        let dir = self.session.dir().to_path_buf();
        let mut commands = Vec::new();
        let mut reports = Vec::new();
        let mut labels = Vec::new();
        for result in recording.finish(&dir, &project, rate, end) {
            match result {
                Ok(take) => {
                    if take.lost > 0 {
                        self.warn(format!("{}: {} frames were lost on the way to the take", take.track, take.lost));
                    }
                    let beat = BigRational::from_float(take.at as f64 * tempo / 60.0 / rate as f64).unwrap_or_default();
                    commands.push(Command::Set {
                        path: format!("samples.{}", take.id),
                        value: take.sample(),
                    });
                    let mut fields = Map::new();
                    fields.insert("sample".into(), json!(take.id));
                    fields.insert("at".into(), crate::session::value_json(&beat_value(&beat)));
                    commands.push(Command::AudioAdd {
                        track: take.track.clone(),
                        fields,
                    });
                    labels.push(format!("{} on {}", take.id, take.track));
                    reports.push(take.report(rate, tempo));
                }
                Err(e) => self.warn(format!("the take was not kept: {e}")),
            }
        }
        if commands.is_empty() {
            return Some(json!([]));
        }
        let seconds = reports.first().and_then(|r| r["seconds"].as_f64()).unwrap_or(0.0);
        let label = format!("Record {} ({seconds:.1} s)", labels.join(", "));
        let batch = Command::Batch {
            commands,
            label: Some(label),
        };
        let edited = (|| -> Result<()> {
            let _lock = lock(self.session.path())?;
            self.sync();
            let before = self.session.doc().clone();
            let (_, change) = self.session.edit(&batch, Origin::User, None, None)?;
            self.session.save()?;
            self.save_at = None;
            if let Some(c) = change {
                self.changed(&c);
            }
            self.note_markers(&before);
            Ok(())
        })();
        if let Err(e) = edited {
            self.warn(format!("the takes were written under samples/ but not added to the song: {e}"));
        }
        Some(Json::Array(reports))
    }

    /// `note.preview`: a note played now through a MIDI track's instrument,
    /// its Synth or its Sampler, and its chain, from where the stream stands,
    /// outside the timeline and the history. Opens the output if no play has
    /// yet. A Sampler plays it as the same note in a clip would; a pitch its
    /// map leaves silent is silent here too, and the reply says so. With
    /// `replace` the track's earlier previews are let go first, each through
    /// its release, so one is heard at a time, as the piano roll asks.
    fn preview(&mut self, track: &str, pitch: &Json, velocity: Option<i64>, length_beats: Option<&Json>, replace: bool) -> Result<Json> {
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
        let midi = t.midi.as_ref();
        let synth = midi.and_then(|m| m.synth()).is_some();
        if !synth && midi.and_then(|m| m.sampler()).is_none() {
            return Err(format!("{track} has no instrument to play the note; daw synth add or daw instrument add attaches one"));
        }
        let frames = (length * 60.0 / session.tempo * session.sample_rate as f64).round() as usize;
        // A Sampler's note is prepared here, as an edit's notes are.
        let voice = match synth {
            true => None,
            false => {
                let (project, dir) = (self.session.project(), self.session.dir());
                Some(aaw_engine::program::preview_voice(project, dir, &mut self.cache, track, pitch, velocity, length)?)
            }
        };
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
            .position(|p| p.id == track && p.synth.is_some() == synth)
            .ok_or_else(|| format!("{track} is not yet playing its instrument; playback keeps the previous version"))?;
        let sounds = match voice {
            None => {
                t.control.preview(index, pitch as f64, velocity as f64 / 127.0, frames, replace)?;
                true
            }
            Some(Some(voice)) => {
                t.control.preview_voice(index, voice, replace)?;
                true
            }
            Some(None) => {
                // A silent pitch still takes the place of what was sounding.
                if replace {
                    t.control.let_go_previews(index)?;
                }
                false
            }
        };
        Ok(json!({
            "sounds": sounds,
            "track": track,
            "pitch": pitch,
            "note": aaw_model::rules::note_name(pitch).unwrap_or_default(),
            "velocity": velocity,
            "length_beats": length,
            "frames": frames,
        }))
    }

    /// The output's hardware latency in the song's frames, what the person
    /// hears the song late by; 0 with no output open.
    fn output_latency(&self, rate: u32) -> u32 {
        match &self.transport {
            Some(t) => (aaw_engine::devices::latency_frames(&t.device, false).unwrap_or(0) as f64 * rate as f64 / t.device_rate as f64).round() as u32,
            None => 0,
        }
    }

    /// `capture.start`: a sound from an input with no track, into `file`.
    fn capture_start(&mut self, file: &str, input: Option<&str>, channels: &[i64]) -> Result<Json> {
        if self.capture.is_some() {
            return Err("Already capturing; capture.stop ends it".into());
        }
        if file.trim().is_empty() {
            return Err("file names where the sound is written".into());
        }
        let path = {
            let given = std::path::Path::new(file);
            if given.is_absolute() {
                given.to_path_buf()
            } else {
                self.session.dir().join(given)
            }
        };
        let channels: Vec<i64> = if channels.is_empty() { vec![1] } else { channels.to_vec() };
        if channels.iter().any(|c| *c < 1) || channels.len() > 2 {
            return Err("channels are one or two numbers counted from 1".into());
        }
        let rate = self.session.project().session.sample_rate as u32;
        let name = self.input_device(input.unwrap_or("default"));
        let output_latency = self.output_latency(rate);
        self.capture = Some(Capture::open(path, name.as_deref(), &channels, rate, output_latency)?);
        self.capture_tick();
        Ok(self.status())
    }

    /// `capture.stop`: the sound written and reported, or dropped.
    fn capture_stop(&mut self, discard: bool) -> Result<Json> {
        let capture = self.capture.take().ok_or("Nothing is being captured; capture.start begins a sound")?;
        if discard {
            return Ok(self.status());
        }
        let s = self.session.project().session.clone();
        let captured = capture.finish(s.sample_rate as u32)?;
        if captured.lost > 0 {
            self.warn(format!("{} frames were lost on the way to the sound", captured.lost));
        }
        let report = captured.report(self.session.dir(), s.tempo);
        let mut status = self.status();
        if let Json::Object(m) = &mut status {
            m.insert("captured".into(), report);
        }
        Ok(status)
    }

    /// Takes a capture's frames in, and fixes where its first frame was
    /// heard once the song plays.
    fn capture_tick(&mut self) {
        let Some(capture) = &mut self.capture else { return };
        let rate = self.session.project().session.sample_rate as u32;
        let (anchor, playing) = match &self.transport {
            Some(t) => (t.control.shared().anchor(), t.control.playing()),
            None => (None, false),
        };
        capture.drain(rate, anchor, playing);
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
                let recorded = self.end_recording(None);
                if let Some(t) = &mut self.transport {
                    t.control.stop()?;
                }
                Ok(self.status_with(recorded))
            }
            Command::Arm { track, armed } => self.arm(track, *armed),
            Command::RecordStart { tracks, count_in } => self.record_start(tracks, *count_in),
            Command::RecordStop => {
                if self.recording.is_none() {
                    return Err("Nothing is recording".into());
                }
                let recorded = self.end_recording(None);
                Ok(self.status_with(recorded))
            }
            Command::CaptureStart { file, input, channels } => self.capture_start(file, input.as_deref(), channels),
            Command::CaptureStop { discard } => self.capture_stop(*discard),
            Command::Audio { output, input, buffer } => {
                let named = |d: &Option<String>| d.clone().filter(|n| !n.trim().is_empty() && n.trim() != "default");
                let mut audio = self.audio.clone();
                if output.is_some() {
                    audio.output = named(output);
                }
                if input.is_some() {
                    audio.input = named(input);
                }
                if let Some(b) = buffer {
                    if !(16..=8192).contains(b) {
                        return Err("buffer is 16 to 8192 frames".into());
                    }
                    audio.buffer = *b;
                }
                if let (Some(name), true) = (&audio.output, audio.strict_output) {
                    if aaw_engine::devices::find_output(name)?.is_none() {
                        return Err(format!("No audio output device named {name}; daw device list names them"));
                    }
                }
                let changed = audio != self.audio;
                self.audio = audio;
                if changed && self.recording.is_some() {
                    self.end_recording(None);
                }
                if changed && self.transport.is_some() {
                    if let Some(program) = self.program.as_ref().map(|(_, p)| p.clone()) {
                        self.reopen(program)?;
                    }
                }
                Ok(self.status())
            }
            Command::NotePreview { track, pitch, velocity, length_beats, replace } => {
                self.preview(track, pitch, *velocity, length_beats.as_ref(), replace.unwrap_or(false))
            }
            Command::Locate { at } => {
                let at = beat(at)?;
                self.check_in_song(&at, "locate")?;
                // A locate ends the take where it stands.
                let recorded = self.end_recording(None);
                self.cue = at;
                let f = self.frame(&self.cue);
                if let Some(t) = &mut self.transport {
                    t.control.locate(f)?;
                }
                Ok(self.status_with(recorded))
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
        m.insert("sample_rate".into(), json!(s.sample_rate));
        m.insert("metronome".into(), json!(self.metronome));
        m.insert(
            "position".into(),
            json!({"frame": frame, "beat": round3(frame as f64 * tempo / 60.0 / rate), "seconds": round3(frame as f64 / rate)}),
        );
        m.insert("cue".into(), crate::session::value_json(&beat_value(&self.cue)));
        m.insert("armed".into(), json!(self.armed));
        m.insert(
            "recording".into(),
            match &self.recording {
                Some(r) => json!({
                    "tracks": r.tracks(),
                    "from": crate::session::value_json(&beat_value(&self.from)),
                    "count_in_frames": r.count_in,
                    "seconds": round3(r.began.elapsed().as_secs_f64()),
                    "loop_end_frame": r.loop_end,
                }),
                None => Json::Null,
            },
        );
        m.insert(
            "capture".into(),
            match &self.capture {
                Some(c) => json!({
                    "file": c.file.strip_prefix(self.session.dir()).unwrap_or(&c.file).to_string_lossy(),
                    "device": c.device(),
                    "seconds": round3(c.seconds()),
                    "level": (c.level as f64 * 1000.0).round() / 1000.0,
                    "lost_frames": c.lost(),
                    "device_stopped": c.dead(),
                    "at_beat": c.first_frame().map(|f| round3(f * tempo / 60.0 / rate)),
                }),
                None => Json::Null,
            },
        );
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
                        "device_sample_rate": r["device_sample_rate"],
                        "resampled": r["resampled"],
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
        m.insert(
            "devices".into(),
            json!({
                "output": self.chosen_output().unwrap_or_else(|| "default".into()),
                "input": self.audio.input.clone().unwrap_or_else(|| "default".into()),
                "buffer_frames": self.audio.buffer,
                "fallback": self.fallback,
            }),
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
            mut command,
            origin,
            expect,
            gesture,
        } = request;
        if origin == Origin::External {
            return Err("external is the origin of edits made to the file".into());
        }
        // A marker dropped without a beat lands where the song is playing.
        if let Command::MarkerAdd { at: at @ None, .. } = &mut command {
            *at = Some(self.marker_beat());
        }
        // How long the agent sent nothing, for the hint its next edit carries.
        let silence = match origin {
            Origin::Agent => self.agent_at.replace(Instant::now()).map(|at| at.elapsed()),
            _ => None,
        };
        match command.kind() {
            Kind::Edit | Kind::History => {
                let _lock = lock(self.session.path())?;
                self.sync();
                let before = (origin == Origin::User).then(|| self.session.doc().clone());
                let (mut reply, change) = match &command {
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
                if let Some(before) = before {
                    self.note_markers(&before);
                }
                if let (Kind::Edit, Some(gap), Json::Object(fields)) = (command.kind(), silence, &mut reply) {
                    if let Some(hint) = pace_hint(gap, self.opts.pace) {
                        fields.insert("hint".into(), json!(hint));
                    }
                }
                // The agent's edit is told of the markers the person left
                // since it last sent anything.
                if let (Kind::Edit, Origin::Agent, Json::Object(fields)) = (command.kind(), origin, &mut reply) {
                    if let Some(news) = self.take_marker_news() {
                        fields.insert("new_markers".into(), json!(news));
                    }
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
                    Command::Map { per, from, to, tracks, lanes } => {
                        self.session.map(per.as_ref(), from.as_ref(), to.as_ref(), &tracks, lanes)
                    }
                    Command::Markers => {
                        // An agent that reads the markers has been told of them.
                        if origin == Origin::Agent {
                            self.fresh_markers.clear();
                        }
                        Ok(self.session.markers())
                    }
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

    /// Takes a recording's frames in, and ends it where the loop wraps, where
    /// the song ends, or when its device stopped.
    fn record_tick(&mut self) {
        let Some(recording) = &mut self.recording else { return };
        recording.drain();
        let playing = self.transport.as_ref().is_some_and(|t| t.control.playing());
        recording.rolled |= playing;
        let (wrapped, loop_end) = (recording.wrapped(), recording.loop_end);
        let dead = recording.takes.iter().find(|t| t.dead()).map(|t| t.track.clone());
        // The player starts at its next block; until then a stopped
        // transport has not started, unless the output is not running.
        let stalled = !recording.rolled && recording.began.elapsed() > Duration::from_secs(2);
        if wrapped {
            self.end_recording(loop_end.map(|f| f as i64));
        } else if !playing && recording.rolled {
            // The song reached its end.
            let total = self.program.as_ref().map(|(_, p)| p.total as i64);
            self.end_recording(total);
        } else if stalled {
            self.warn("the output never started, so the take ends".into());
            self.end_recording(None);
        } else if let Some(track) = dead {
            self.warn(format!("{track}'s input device stopped; the take ends here"));
            self.end_recording(None);
        }
    }

    /// Housekeeping between requests. True when the host should exit.
    fn tick(&mut self) -> bool {
        self.save_due();
        self.follow_devices();
        self.record_tick();
        self.capture_tick();
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
fn open(project: &Path, opts: Options, observer: Option<Observer>, clock: Arc<Clock>, taps: Arc<Taps>) -> Result<(Host, Serving, Inbox)> {
    let entry = registry::entry(project)?;
    let (listener, claim) = registry::claim(&entry)?;
    let session = Session::open(project, true)?;
    let (tx, rx) = mpsc::channel::<(Request, mpsc::Sender<Json>)>();
    let opts_audio = opts.audio.clone();
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
        cache: Cache::with_taps(taps),
        program: None,
        transport: None,
        audio: opts_audio,
        fallback: None,
        devices_seen: aaw_engine::devices::generation(),
        revived_at: None,
        metronome: false,
        armed: Vec::new(),
        recording: None,
        capture: None,
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
        agent_at: None,
        fresh_markers: Vec::new(),
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
    // A take still running is kept, and so is a sound being captured.
    host.end_recording(None);
    if let Some(capture) = host.capture.take() {
        let rate = host.session.project().session.sample_rate as u32;
        if let Err(e) = capture.finish(rate) {
            host.warn(format!("the sound was not kept: {e}"));
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
    let (host, serving, _) = open(project, opts, None, Arc::new(Clock::default()), Arc::default())?;
    serve(host, serving, interrupted)
}

/// A host on its own thread, for a process that embeds one, such as the app.
/// Other processes reach it through its socket as they reach `daw host`.
pub struct Running {
    requests: Inbox,
    clock: Arc<Clock>,
    /// The equalizers' taps, for the app's spectrum.
    taps: Arc<Taps>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<Json>>>,
}

/// Starts a host for a project. `observer` is called on the host's thread,
/// first with `Event::Opened`, and must return promptly.
pub fn spawn(project: &Path, opts: Options, observer: impl FnMut(Event) + Send + 'static) -> Result<Running> {
    let stop = Arc::new(AtomicBool::new(false));
    let clock = Arc::new(Clock::default());
    let taps = Arc::new(Taps::default());
    let (ready_tx, ready_rx) = mpsc::channel::<Result<Inbox>>();
    let thread = {
        let (project, stop, clock, taps) = (project.to_path_buf(), stop.clone(), clock.clone(), taps.clone());
        std::thread::Builder::new()
            .name("aaw-host".into())
            .spawn(move || match open(&project, opts, Some(Box::new(observer)), clock, taps) {
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
        taps,
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

    /// The equalizers' taps, where the audio thread leaves what each puts
    /// out, for the app's spectrum.
    pub fn taps(&self) -> &Arc<Taps> {
        &self.taps
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
