//! A running session host: owns a project, answers requests on its socket one
//! at a time, saves after each change, reloads external edits, and plays the
//! song, recompiling it after each change so edits are heard during playback.

use crate::client::Request;
use crate::command::{beat_value, json_value, Command, Kind, Origin};
use crate::registry;
use crate::session::{Change, Session};
use aaw_engine::program::{compile_cached, Program, SampleCache};
use aaw_engine::realtime::Transport;
use aaw_model::Beat;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use serde_json::{json, Map, Value as Json};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, String>;

/// How often the host checks song.yaml for external edits.
const SYNC_EVERY: Duration = Duration::from_millis(200);

/// Holds the project's advisory lock, as the Python CLI's writers do.
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
}

struct Host {
    session: Session,
    opts: Options,
    cache: SampleCache,
    /// The compiled song and the SHA it was compiled from.
    program: Option<(String, Arc<Program>)>,
    transport: Option<Transport>,
    /// Where play starts, in beats.
    cue: BigRational,
    /// Loop start and length, in beats.
    region: Option<(BigRational, BigRational)>,
    warned: Vec<String>,
    playback_error: Option<String>,
    reported_invalid: Option<String>,
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

    /// The program for the current revision, compiling it if needed.
    fn program(&mut self) -> Result<Arc<Program>> {
        let sha = self.session.doc().sha.clone();
        if let Some((s, p)) = &self.program {
            if *s == sha {
                return Ok(p.clone());
            }
        }
        let p = Arc::new(compile_cached(self.session.project(), self.session.dir(), &mut self.cache)?);
        for feature in &p.omitted {
            if !self.warned.contains(feature) {
                self.warned.push(feature.clone());
                eprintln!(
                    "{}",
                    json!({"warning": format!("playing without {feature}, which the Rust engine does not process yet")})
                );
            }
        }
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
        self.transport = Some(t);
        Ok(())
    }

    /// Sends the current revision to the audio thread.
    fn refresh(&mut self) {
        if self.transport.is_none() {
            return;
        }
        let result = self.program().and_then(|p| {
            let t = self.transport.as_mut().expect("transport");
            if p.rate == t.rate {
                t.control.load(p)?;
            } else {
                // Another sample rate needs another stream, at the same beat.
                let old = self.transport.take().expect("transport");
                let (playing, frame, old_program) = (old.control.playing(), old.control.position(), old.control.program().clone());
                drop(old);
                let beats = BigRational::from_float(frame as f64 * old_program.tempo / 60.0 / old_program.rate as f64)
                    .unwrap_or_default();
                self.open_transport(p)?;
                if playing {
                    let f = self.frame(&beats);
                    self.transport.as_mut().expect("transport").control.play(f)?;
                }
            }
            let region = self.loop_frames();
            self.transport.as_mut().expect("transport").control.set_loop(region)
        });
        self.playback_error = result.err();
        if let Some(e) = &self.playback_error {
            eprintln!("{}", json!({"warning": format!("playback keeps the previous version: {e}")}));
        }
    }

    fn changed(&mut self, change: &Change) {
        self.emit(json!({"change": change}));
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
            self.reported_invalid = invalid;
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

    fn transport(&mut self, cmd: &Command) -> Result<Json> {
        match cmd {
            Command::Play { from } => self.play(from.as_ref()),
            Command::Stop => {
                if let Some(t) = &mut self.transport {
                    t.control.stop()?;
                }
                Ok(self.status())
            }
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
                        "max_callback_ms": r["max_callback_ms"],
                        "xruns": r["xruns"],
                    })
                }
                None => Json::Null,
            },
        );
        if let Some((_, p)) = &self.program {
            m.insert("omitted".into(), json!(p.omitted));
        }
        m.insert("playback_error".into(), json!(self.playback_error));
        Json::Object(m)
    }

    fn handle(&mut self, request: Request) -> Result<Json> {
        let Request { command, origin, expect } = request;
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
                    _ => self.session.edit(&command, origin, expect.as_deref())?,
                };
                self.session.save()?;
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
                    _ => unreachable!("not a read"),
                }
            }
            Kind::Host => match command {
                Command::Fmt => {
                    let _lock = lock(self.session.path())?;
                    self.session.write()?;
                    self.reported_invalid = None;
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

    /// Housekeeping between requests. True when the host should exit.
    fn tick(&mut self) -> bool {
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
fn serve(stream: UnixStream, requests: mpsc::Sender<(Request, mpsc::Sender<Json>)>) {
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
                json!({"error": "The host is closing"})
            } else {
                rx.recv().unwrap_or_else(|_| json!({"error": "The host is closing"}))
            }
        }
    };
    let mut stream = stream;
    let _ = writeln!(stream, "{reply}");
}

fn listen(listener: UnixListener, requests: mpsc::Sender<(Request, mpsc::Sender<Json>)>, done: Arc<AtomicBool>) {
    let _ = listener.set_nonblocking(true);
    while !done.load(Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let tx = requests.clone();
                std::thread::spawn(move || serve(stream, tx));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Hosts a project until `interrupted` is set, a `close` request arrives or,
/// with `exit_on_stop`, playback stops. Returns a summary, with the audio
/// report when the song played.
pub fn run(project: &Path, opts: Options, interrupted: Arc<AtomicBool>) -> Result<Json> {
    let entry = registry::entry(project)?;
    let (listener, claim) = registry::claim(&entry)?;
    let session = Session::open(project, true)?;
    let (tx, rx) = mpsc::channel::<(Request, mpsc::Sender<Json>)>();
    let done = Arc::new(AtomicBool::new(false));
    let listening = {
        let done = done.clone();
        std::thread::spawn(move || listen(listener, tx, done))
    };
    let play = opts.play.clone();
    let mut host = Host {
        session,
        opts,
        cache: SampleCache::default(),
        program: None,
        transport: None,
        cue: BigRational::zero(),
        region: None,
        warned: Vec::new(),
        playback_error: None,
        reported_invalid: None,
        heard: false,
        stopped_at: None,
        closing: false,
        from: BigRational::zero(),
    };
    host.emit(json!({"host": {
        "project": host.session.path(),
        "pid": std::process::id(),
        "socket": entry.socket,
        "revision": 0,
    }}));
    let started = match &play {
        Some(from) => host.play(Some(from)).map(|_| ()),
        None => Ok(()),
    };
    let mut last_sync = Instant::now();
    while started.is_ok() {
        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok((request, reply)) => {
                let result = host.handle(request);
                let _ = reply.send(match result {
                    Ok(v) => json!({"ok": v}),
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
        if host.tick() || host.closing || interrupted.load(Relaxed) {
            break;
        }
    }
    done.store(true, Relaxed);
    let _ = listening.join();
    // Answer requests that arrived while closing.
    while let Ok((_, reply)) = rx.try_recv() {
        let _ = reply.send(json!({"error": "The host is closing"}));
    }
    drop(claim);
    if let Some(t) = &mut host.transport {
        if t.control.playing() {
            let _ = t.control.stop();
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    let saved = host.session.save();
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
        if let Some((_, p)) = &host.program {
            summary.insert("omitted".into(), json!(p.omitted));
        }
        t.close();
    }
    Ok(Json::Object(summary))
}
