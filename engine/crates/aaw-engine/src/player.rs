//! The transport's audio-thread side: play from any frame, stop, locate while
//! playing and loop a region, while the host replaces the program after edits.
//!
//! The host thread builds each program's `Deck`, a renderer with fresh state,
//! and sends it through a lock-free queue. The player swaps decks between
//! blocks and returns replaced decks to the host to be freed, as it does the
//! voices of notes previewed through a Sampler once later ones replace them.
//!
//! A program with the same channels and devices takes over without a break:
//! its devices continue from the old ones' state, its levels and knobs glide
//! from the old values over 5 ms, and voices that changed ring out under the
//! new ones. A program with another structure, such as an added effect or a
//! removed track, is swapped in at the bottom of a 5 ms fade out and back in,
//! with the devices that are still there keeping their state.
//!
//! The stream runs while the transport is stopped, so tails ring out after a
//! stop, and rests once its output has been silent for a second.
//! `Player::render` never allocates, frees, locks or blocks.

use crate::capture::{ChunkTime, Tap, TapSource};
use crate::program::{Program, Voice};
use crate::render::{Frame, Renderer};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering::{Acquire, Relaxed, Release}};
use std::sync::Arc;

pub use crate::render::FADE_SECONDS;

/// The largest block rendered at once; longer requests loop.
pub const MAX_BLOCK: usize = 4096;
const QUEUE: usize = 256;
/// Output below this, about -180 dBFS, counts as silence.
const QUIET: f64 = 1e-9;

/// A program with the renderer the audio thread plays it with.
pub struct Deck {
    renderer: Renderer,
}

impl Deck {
    /// Built off the audio thread: the renderer allocates its state here.
    pub fn new(program: Arc<Program>) -> Box<Deck> {
        Box::new(Deck {
            renderer: Renderer::new(program, 0, MAX_BLOCK),
        })
    }
}

/// The most taps a player holds at once, one for each track recording the
/// song's own sound.
pub const MAX_TAPS: usize = 8;

pub enum Message {
    Program(Box<Deck>),
    /// Play from a frame, after `preroll` frames of the metronome alone
    /// with the song standing at that frame: a count-in.
    Play { from: usize, preroll: usize },
    Stop,
    Metronome(bool),
    /// Whether playback rolls on past the song's end until a stop, through
    /// bars that hold nothing, the metronome with it; else it stops there.
    RollOn(bool),
    Locate(usize),
    /// Start and end frames; playback reaching the end jumps to the start.
    Loop(Option<(usize, usize)>),
    /// A note played now through a track's Synth, outside the timeline: the
    /// track's place among the program's tracks, its pitch as a MIDI number,
    /// its velocity 0 to 1 and how many frames it is held; `replace` lets
    /// go the track's earlier previews first.
    Preview {
        track: usize,
        pitch: f64,
        velocity: f64,
        frames: usize,
        replace: bool,
    },
    /// A note played now through a track's Sampler, outside the timeline:
    /// the track's place among the program's tracks, and the voice
    /// `program::preview_voice` made for it; `replace` as for `Preview`.
    PreviewVoice { track: usize, pitch: i64, voice: Voice, replace: bool },
    /// Lets go the notes previewed through a track, each through its
    /// release, with nothing played in their place.
    LetGoPreviews { track: usize },
    /// Lets go the notes of one pitch previewed through a track and leaves
    /// the others held: a key lifted on the typing keyboard.
    LetGoPreview { track: usize, pitch: i64 },
    /// Records a stem or the mix into the tap's feed while the transport
    /// rolls. Beyond `MAX_TAPS` the tap is handed straight back.
    Tap(Box<Tap>),
    /// Takes every tap out, each handed back to the host.
    Untap,
}

/// What the audio thread hands back to the host to be freed.
pub enum Garbage {
    Deck(Box<Deck>),
    Voice(Voice),
    Tap(Box<Tap>),
}

enum Fade {
    None,
    /// Voices of the previous program ringing out under the current one.
    Cross { old: Box<Deck>, left: usize },
    /// The output falling to silence, where `next` replaces the program.
    Fall { next: Box<Deck>, left: usize },
    /// The output rising again after that.
    Rise { left: usize },
}

/// What the audio thread publishes for the host.
#[derive(Default)]
pub struct Shared {
    /// The timeline frame at the output, which trails the transport by the
    /// program's latency.
    pub position: AtomicU64,
    /// Frames between a callback and the device playing what it wrote, which
    /// the driver reports; what is heard trails `position` by this much.
    pub output_latency: AtomicU64,
    pub playing: AtomicBool,
    /// Frames rendered while playing.
    pub played: AtomicU64,
    pub swaps: AtomicU64,
    /// Decks and preview voices that could not be returned because the queue
    /// was full. They are leaked rather than freed on the audio thread.
    pub leaked: AtomicU64,
    /// How many times playback has jumped back to the loop's start.
    pub wraps: AtomicU64,
    /// The anchor: the instant, in nanoseconds of the audio clock, at which
    /// the first frame of the last block rendered plays, and the timeline
    /// frame that plays then, a count-in's frames before the start counted
    /// negative from it. Written under `anchor_seq`, odd while it changes.
    anchor_seq: AtomicU64,
    anchor_ns: AtomicU64,
    anchor_frame: AtomicI64,
}

impl Shared {
    fn publish_anchor(&self, ns: u64, frame: i64) {
        let seq = self.anchor_seq.load(Relaxed);
        self.anchor_seq.store(seq + 1, Release);
        self.anchor_ns.store(ns, Relaxed);
        self.anchor_frame.store(frame, Relaxed);
        self.anchor_seq.store(seq + 2, Release);
    }

    /// The anchor as last published, `(ns, frame)`: None before the output
    /// has rendered a timed block. A recording maps an input block captured
    /// at `t` to timeline frame `frame + (t - ns) * rate`.
    pub fn anchor(&self) -> Option<(u64, i64)> {
        for _ in 0..1000 {
            let before = self.anchor_seq.load(Acquire);
            if before == 0 {
                return None;
            }
            if before % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let (ns, frame) = (self.anchor_ns.load(Relaxed), self.anchor_frame.load(Relaxed));
            if self.anchor_seq.load(Acquire) == before {
                return Some((ns, frame));
            }
        }
        None
    }
}

pub struct Player {
    deck: Box<Deck>,
    playing: bool,
    /// Frames of count-in left before the song rolls, the metronome alone
    /// sounding, with the song standing where play was asked from.
    preroll: usize,
    /// Boxed, since each crosses the queue in a message and goes back whole.
    #[allow(clippy::vec_box)]
    taps: Vec<Box<Tap>>,
    metronome: bool,
    /// Rolls on past the song's end rather than stopping there.
    roll_on: bool,
    click_gain: f64,
    click_position: i64,
    fade: Fade,
    pending: Option<Box<Deck>>,
    region: Option<(usize, usize)>,
    fade_len: usize,
    /// Frames of silence in a row while stopped, and whether the stream rests.
    quiet: usize,
    resting: bool,
    inbox: rtrb::Consumer<Message>,
    garbage: rtrb::Producer<Garbage>,
    shared: Arc<Shared>,
}

/// The host thread's handle on a player.
pub struct Control {
    inbox: rtrb::Producer<Message>,
    garbage: rtrb::Consumer<Garbage>,
    shared: Arc<Shared>,
    program: Arc<Program>,
    /// Where the last play sent asked to start; None once a stop is sent.
    asked: Option<usize>,
}

/// A connected player and control, stopped at frame 0 of `program`.
pub fn channel(program: Arc<Program>) -> (Control, Player) {
    let (inbox_tx, inbox_rx) = rtrb::RingBuffer::new(QUEUE);
    let (garbage_tx, garbage_rx) = rtrb::RingBuffer::new(QUEUE);
    let shared = Arc::new(Shared::default());
    let deck = Deck::new(program.clone());
    let player = Player {
        fade_len: deck.renderer.fade(),
        deck,
        playing: false,
        preroll: 0,
        taps: Vec::with_capacity(MAX_TAPS),
        metronome: false,
        roll_on: false,
        click_gain: 0.0,
        click_position: 0,
        fade: Fade::None,
        pending: None,
        region: None,
        quiet: 0,
        resting: true,
        inbox: inbox_rx,
        garbage: garbage_tx,
        shared: shared.clone(),
    };
    let control = Control {
        inbox: inbox_tx,
        garbage: garbage_rx,
        shared,
        program,
        asked: None,
    };
    (control, player)
}

impl Control {
    fn send(&mut self, m: Message) -> Result<(), String> {
        self.inbox
            .push(m)
            .map_err(|_| "The audio thread is not taking messages".to_string())
    }

    /// Replaces the program. Playback continues at the same beat.
    pub fn load(&mut self, program: Arc<Program>) -> Result<(), String> {
        self.collect();
        self.send(Message::Program(Deck::new(program.clone())))?;
        self.program = program;
        Ok(())
    }

    pub fn play(&mut self, from: usize) -> Result<(), String> {
        self.play_after(from, 0)
    }

    /// Plays from `from` after `preroll` frames of count-in, the metronome
    /// alone with the song standing at `from`, as a take starts.
    pub fn play_after(&mut self, from: usize, preroll: usize) -> Result<(), String> {
        self.send(Message::Play { from, preroll })?;
        self.asked = Some(from);
        Ok(())
    }

    /// Records a stem or the mix into `tap` while the transport rolls, until
    /// `untap`. Up to `MAX_TAPS` at once; more are handed back unused.
    pub fn tap(&mut self, tap: Box<Tap>) -> Result<(), String> {
        self.collect();
        self.send(Message::Tap(tap))
    }

    /// Takes every tap out of the player; each comes back through `collect`
    /// to be freed, its feed's last blocks drained first by the host.
    pub fn untap(&mut self) -> Result<(), String> {
        self.send(Message::Untap)
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.send(Message::Stop)?;
        self.asked = None;
        Ok(())
    }

    pub fn locate(&mut self, to: usize) -> Result<(), String> {
        self.send(Message::Locate(to))
    }

    /// A monitoring click, mixed only by the real-time player.
    pub fn set_metronome(&mut self, enabled: bool) -> Result<(), String> {
        self.send(Message::Metronome(enabled))
    }

    /// Whether playback rolls on past the song's end until a stop, as a
    /// window's transport does, or stops at the end, as a `daw play` that
    /// hosts for its own duration does. It stops there until told.
    pub fn set_roll_on(&mut self, enabled: bool) -> Result<(), String> {
        self.send(Message::RollOn(enabled))
    }

    pub fn set_loop(&mut self, region: Option<(usize, usize)>) -> Result<(), String> {
        self.send(Message::Loop(region))
    }

    /// Plays a note now through the Synth of the program's track at `track`,
    /// whether or not the transport rolls: `note.preview`. The note is
    /// heard through the track's chain and fader and is never recorded.
    /// With `replace` the track's earlier previews are let go first, each
    /// through its release, so one is heard at a time.
    pub fn preview(&mut self, track: usize, pitch: f64, velocity: f64, frames: usize, replace: bool) -> Result<(), String> {
        self.send(Message::Preview {
            track,
            pitch,
            velocity,
            frames,
            replace,
        })
    }

    /// Plays a note now through the Sampler of the program's track at
    /// `track`, whether or not the transport rolls: `note.preview` on a
    /// track with a Sampler, with the voice `program::preview_voice` made.
    /// `replace` as for `preview`.
    pub fn preview_voice(&mut self, track: usize, pitch: i64, voice: Voice, replace: bool) -> Result<(), String> {
        self.collect();
        self.send(Message::PreviewVoice { track, pitch, voice, replace })
    }

    /// Lets go the notes previewed through the program's track at `track`,
    /// each through its release, as `replace` does without a note to play:
    /// a replaced preview whose pitch the Sampler's map leaves silent.
    pub fn let_go_previews(&mut self, track: usize) -> Result<(), String> {
        self.send(Message::LetGoPreviews { track })
    }

    /// Lets go the notes of `pitch` previewed through the program's track at
    /// `track`, each through its release, and leaves the track's other
    /// previews held: `note.off`, a key of the typing keyboard lifted.
    pub fn let_go_preview(&mut self, track: usize, pitch: i64) -> Result<(), String> {
        self.send(Message::LetGoPreview { track, pitch })
    }

    /// The last program sent.
    pub fn program(&self) -> &Arc<Program> {
        &self.program
    }

    /// The timeline frame at the output.
    pub fn position(&self) -> usize {
        self.shared.position.load(Relaxed) as usize
    }

    pub fn playing(&self) -> bool {
        self.shared.playing.load(Relaxed)
    }

    /// Where a play sent but not yet taken by the audio thread starts, as
    /// when the driver stopped the stream before its first callback: the
    /// output opened again plays from there.
    pub fn play_unheard(&self) -> Option<usize> {
        let unread = self.inbox.slots() < self.inbox.buffer().capacity();
        self.asked.filter(|_| unread && !self.playing())
    }

    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    /// Frees decks and voices the audio thread has finished with.
    pub fn collect(&mut self) {
        while let Ok(garbage) = self.garbage.pop() {
            drop(garbage);
        }
    }

    /// Frees what the audio thread has finished with, and hands over the
    /// taps among it, whose feeds the host drains once more.
    pub fn collect_taps(&mut self) -> Vec<Box<Tap>> {
        let mut taps = Vec::new();
        while let Ok(garbage) = self.garbage.pop() {
            match garbage {
                Garbage::Tap(t) => taps.push(t),
                other => drop(other),
            }
        }
        taps
    }
}

impl Player {
    /// The transport's position: the frame of the voices starting now.
    pub fn position(&self) -> usize {
        self.deck.renderer.position()
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    /// Fills `out` with the next frames of the stream.
    pub fn render(&mut self, out: &mut [Frame]) {
        self.render_at(out, None);
    }

    /// Fills `out` with the next frames of the stream, and with `at`, the
    /// instant its first frame plays on the audio clock, publishes the
    /// anchor a recording maps its input by.
    pub fn render_at(&mut self, out: &mut [Frame], at: Option<u64>) {
        while let Ok(m) = self.inbox.pop() {
            self.handle(m);
        }
        if let Some(ns) = at {
            let frame = self.deck.renderer.heard() - self.preroll as i64;
            self.shared.publish_anchor(ns, frame);
        }
        let mut done = 0;
        while done < out.len() {
            let mut n = (out.len() - done).min(MAX_BLOCK);
            if self.playing && self.preroll > 0 {
                // The count-in ends on a block's edge, where the song rolls.
                n = n.min(self.preroll);
            } else if self.playing {
                // A block ends where the loop does, and where the song does.
                let pos = self.position();
                let program = self.deck.renderer.program();
                let last = program.total + program.latency;
                let end = self.region.map_or(last, |(_, end)| if pos < end { end } else { last });
                if pos < end {
                    n = n.min(end - pos);
                }
            }
            // A fade ends on a block's edge, where the decks change hands.
            if let Fade::Cross { left, .. } | Fade::Fall { left, .. } | Fade::Rise { left } = &self.fade {
                n = n.min((*left).max(1));
            }
            self.block(&mut out[done..done + n]);
            done += n;
            if self.playing && self.preroll > 0 {
                self.preroll -= n;
            } else if self.playing {
                let pos = self.position();
                match self.region {
                    Some((start, end)) if pos == end => {
                        self.deck.renderer.jump(start);
                        self.shared.wraps.fetch_add(1, Relaxed);
                    }
                    _ if self.deck.renderer.finished() && !self.roll_on => self.playing = false,
                    _ => {}
                }
            }
            self.settle();
        }
        self.shared.position.store(self.deck.renderer.heard().max(0) as u64, Relaxed);
        self.shared.playing.store(self.playing, Relaxed);
    }

    fn handle(&mut self, m: Message) {
        match m {
            Message::Program(deck) => {
                if matches!(self.fade, Fade::None) {
                    self.install(deck);
                    return;
                }
                let waiting = match &mut self.fade {
                    // The newest program is the one to fall to.
                    Fade::Fall { next, .. } => Some(std::mem::replace(next, deck)),
                    _ => self.pending.replace(deck),
                };
                if let Some(older) = waiting {
                    self.discard(older);
                }
            }
            Message::Play { from, preroll } => {
                self.resting = false;
                self.quiet = 0;
                if self.playing && self.preroll == 0 {
                    self.deck.renderer.jump(from);
                } else {
                    // Voices sounding at `from` fade in; hits at `from` play in full.
                    self.deck.renderer.seek(from, true);
                    self.playing = true;
                }
                self.preroll = preroll;
            }
            Message::Stop => {
                if self.playing {
                    self.deck.renderer.release();
                    self.playing = false;
                }
                self.preroll = 0;
            }
            Message::Locate(to) => {
                if self.playing {
                    self.deck.renderer.jump(to);
                } else {
                    self.deck.renderer.seek(to, false);
                }
                self.preroll = 0;
            }
            Message::Tap(tap) => {
                if self.taps.len() < MAX_TAPS {
                    self.taps.push(tap);
                } else {
                    self.throw(Garbage::Tap(tap));
                }
            }
            Message::Untap => {
                while let Some(tap) = self.taps.pop() {
                    self.throw(Garbage::Tap(tap));
                }
            }
            Message::Loop(region) => self.region = region,
            Message::Metronome(enabled) => self.metronome = enabled,
            Message::RollOn(enabled) => self.roll_on = enabled,
            Message::Preview { track, pitch, velocity, frames, replace } => {
                // A resting stream wakes for the note.
                self.resting = false;
                self.quiet = 0;
                self.deck.renderer.preview(track, pitch, velocity, frames, replace);
            }
            Message::PreviewVoice { track, pitch, voice, replace } => {
                self.resting = false;
                self.quiet = 0;
                // The voice it takes the place of may hold the last of its audio.
                if let Some(old) = self.deck.renderer.preview_voice(track, pitch, voice, replace) {
                    self.throw(Garbage::Voice(old));
                }
            }
            Message::LetGoPreviews { track } => self.deck.renderer.let_go_previews(track),
            Message::LetGoPreview { track, pitch } => self.deck.renderer.let_go_preview(track, pitch),
        }
    }

    /// Starts a program taking over, with no fade in progress.
    fn install(&mut self, mut deck: Box<Deck>) {
        let same = deck.renderer.program().structure == self.deck.renderer.program().structure;
        if self.resting {
            // Nothing sounds, so nothing can click.
            deck.renderer.replace(&mut self.deck.renderer);
            let old = std::mem::replace(&mut self.deck, deck);
            self.discard(old);
        } else if same {
            let rings = deck.renderer.take_over(&mut self.deck.renderer);
            let old = std::mem::replace(&mut self.deck, deck);
            if rings {
                self.fade = Fade::Cross {
                    old,
                    left: self.fade_len,
                };
            } else {
                self.discard(old);
            }
        } else {
            self.fade = Fade::Fall {
                next: deck,
                left: self.fade_len,
            };
            return;
        }
        self.shared.swaps.fetch_add(1, Relaxed);
    }

    /// Finishes a completed fade and installs a program that waited for it.
    fn settle(&mut self) {
        let finished = match &self.fade {
            Fade::None => false,
            Fade::Cross { left, .. } | Fade::Fall { left, .. } | Fade::Rise { left } => *left == 0,
        };
        if finished {
            match std::mem::replace(&mut self.fade, Fade::None) {
                Fade::Cross { old, .. } => {
                    self.deck.renderer.end_take_over();
                    self.discard(old);
                }
                Fade::Fall { mut next, .. } => {
                    next.renderer.replace(&mut self.deck.renderer);
                    let old = std::mem::replace(&mut self.deck, next);
                    self.discard(old);
                    self.shared.swaps.fetch_add(1, Relaxed);
                    self.fade = Fade::Rise { left: self.fade_len };
                }
                Fade::Rise { .. } | Fade::None => {}
            }
        }
        if matches!(self.fade, Fade::None) {
            if let Some(deck) = self.pending.take() {
                self.install(deck);
            }
        }
    }

    fn discard(&mut self, deck: Box<Deck>) {
        self.throw(Garbage::Deck(deck));
    }

    fn throw(&mut self, garbage: Garbage) {
        if let Err(rtrb::PushError::Full(garbage)) = self.garbage.push(garbage) {
            std::mem::forget(garbage);
            self.shared.leaked.fetch_add(1, Relaxed);
        }
    }

    fn block(&mut self, out: &mut [Frame]) {
        let n = out.len();
        if self.resting {
            out.fill([0.0; 2]);
            return;
        }
        let heard = self.deck.renderer.heard();
        // During a count-in the song stands still and only the click sounds.
        let moving = self.playing && self.preroll == 0;
        // Past the song's end the transport moves on over nothing, and what
        // still rings is heard whole, as after a stop.
        let past = moving && self.deck.renderer.finished();
        let rolling = moving && !past;
        let old = match &mut self.fade {
            Fade::Cross { old, .. } => Some(&mut old.renderer),
            _ => None,
        };
        // Each tap's stem, by its place among the program's stems now.
        let taps = &mut self.taps;
        let program = self.deck.renderer.program().clone();
        let mut places = [None; MAX_TAPS];
        for (k, tap) in taps.iter().enumerate() {
            if let TapSource::Stem { kind, id } = &tap.source {
                places[k] = program.stem_index(*kind, id);
            }
        }
        self.deck.renderer.process(out, rolling, old, |stem, block, at| {
            if !rolling {
                return;
            }
            for (k, tap) in taps.iter_mut().enumerate() {
                if places[k] == Some(stem) {
                    // A stem carries the master's gain and end fade, as the
                    // render writes it.
                    let p = &program;
                    tap.feed.push_frames_scaled(ChunkTime::Frame(at), block, |i| {
                        let f = at + i as i64;
                        if f < 0 || f >= p.total as i64 { 0.0 } else { p.envelope(f as usize) }
                    });
                }
            }
        });
        if rolling {
            for tap in taps.iter_mut().filter(|t| t.source == TapSource::Master) {
                tap.feed.push_frames(ChunkTime::Frame(heard), out);
            }
        }
        if past {
            self.deck.renderer.pass(n);
        }
        if moving {
            self.shared.played.fetch_add(n as u64, Relaxed);
        }
        let len = self.fade_len as f64;
        match &mut self.fade {
            Fade::None => {}
            Fade::Cross { left, .. } => *left = left.saturating_sub(n),
            Fade::Fall { left, .. } => {
                for (k, o) in out.iter_mut().enumerate() {
                    let g = left.saturating_sub(k) as f64 / len;
                    *o = [o[0] * g, o[1] * g];
                }
                *left = left.saturating_sub(n);
            }
            Fade::Rise { left } => {
                for (k, o) in out.iter_mut().enumerate() {
                    let g = 1.0 - left.saturating_sub(k) as f64 / len;
                    *o = [o[0] * g, o[1] * g];
                }
                *left = left.saturating_sub(n);
            }
        }
        // Monitor after all song processing, at the timeline heard from the
        // renderer (including its latency). No click reaches offline exports.
        // A count-in clicks whatever the metronome, the bars before the
        // start counted back from it.
        if self.playing || self.click_gain > 0.0 {
            let p = &program;
            let target = if (self.metronome && moving) || (self.playing && self.preroll > 0) { 1.0 } else { 0.0 };
            let click_start = if self.playing { heard - self.preroll as i64 } else { self.click_position };
            let step = 1.0 / self.fade_len.max(1) as f64;
            if target > 0.0 || self.click_gain > 0.0 {
                let bar = (p.rate as f64 * 60.0 / p.tempo * p.click_beats * p.beats_per_bar.max(1) as f64).round().max(1.0) as i64;
                for (k, frame) in out.iter_mut().enumerate() {
                    self.click_gain += (target - self.click_gain).clamp(-step, step);
                    let mut at = click_start + k as i64;
                    if at < 0 {
                        // Before the song: the same bars, counted back.
                        at += ((-at + bar - 1) / bar) * bar;
                    }
                    if self.roll_on || at < p.total.max(bar as usize) as i64 {
                        let click = metronome_sample(at as usize, p.rate, p.tempo, p.click_beats, p.beats_per_bar) * self.click_gain;
                        frame[0] += click;
                        frame[1] += click;
                    }
                }
            }
            self.click_position = click_start + n as i64;
        }
        if self.playing { return; }
        // Stopped: once the tails have died away, the stream rests.
        if out.iter().all(|f| f[0].abs() < QUIET && f[1].abs() < QUIET) {
            self.quiet += n;
            let second = self.deck.renderer.program().rate as usize;
            if self.quiet >= second && matches!(self.fade, Fade::None) && self.pending.is_none() {
                self.resting = true;
            }
        } else {
            self.quiet = 0;
        }
    }
}

/// Derive each onset from its absolute beat, so rounded periods never drift.
/// The click falls on the note the time signature counts, `click_beats`
/// quarter notes apart, and a short attack and decay soften it; the higher
/// tone marks the first of each bar's `beats_per_bar`.
fn metronome_sample(frame: usize, rate: u32, tempo: f64, click_beats: f64, beats_per_bar: u32) -> f64 {
    let period = rate as f64 * 60.0 / tempo * click_beats;
    let beat = ((frame as f64 + 0.5) / period).floor();
    let onset = (beat * period).round() as usize;
    let t = frame.saturating_sub(onset) as f64 / rate as f64;
    if t >= 0.025 { return 0.0; }
    let hz = if beat as u64 % beats_per_bar.max(1) as u64 == 0 { 1760.0 } else { 1320.0 };
    let envelope = (t / 0.001).min(1.0) * (1.0 - t / 0.025).powi(3);
    0.16 * envelope * (std::f64::consts::TAU * hz * t).sin()
}
