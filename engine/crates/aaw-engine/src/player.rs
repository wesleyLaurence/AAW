//! The transport's audio-thread side: play from any frame, stop, locate while
//! playing and loop a region, while the host replaces the program after edits.
//!
//! The host thread builds each program's `Deck`, a renderer with fresh state,
//! and sends it through a lock-free queue. The player swaps decks between
//! blocks and returns replaced decks to the host to be freed.
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

use crate::program::Program;
use crate::render::{Frame, Renderer};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
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

pub enum Message {
    Program(Box<Deck>),
    Play(usize),
    Stop,
    Metronome(bool),
    Locate(usize),
    /// Start and end frames; playback reaching the end jumps to the start.
    Loop(Option<(usize, usize)>),
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
    /// Decks that could not be returned because the queue was full. They are
    /// leaked rather than freed on the audio thread.
    pub leaked: AtomicU64,
}

pub struct Player {
    deck: Box<Deck>,
    playing: bool,
    metronome: bool,
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
    garbage: rtrb::Producer<Box<Deck>>,
    shared: Arc<Shared>,
}

/// The host thread's handle on a player.
pub struct Control {
    inbox: rtrb::Producer<Message>,
    garbage: rtrb::Consumer<Box<Deck>>,
    shared: Arc<Shared>,
    program: Arc<Program>,
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
        metronome: false,
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
        self.send(Message::Play(from))
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.send(Message::Stop)
    }

    pub fn locate(&mut self, to: usize) -> Result<(), String> {
        self.send(Message::Locate(to))
    }

    /// A monitoring click, mixed only by the real-time player.
    pub fn set_metronome(&mut self, enabled: bool) -> Result<(), String> {
        self.send(Message::Metronome(enabled))
    }

    pub fn set_loop(&mut self, region: Option<(usize, usize)>) -> Result<(), String> {
        self.send(Message::Loop(region))
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

    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    /// Frees decks the audio thread has finished with.
    pub fn collect(&mut self) {
        while let Ok(deck) = self.garbage.pop() {
            drop(deck);
        }
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
        while let Ok(m) = self.inbox.pop() {
            self.handle(m);
        }
        let mut done = 0;
        while done < out.len() {
            let mut n = (out.len() - done).min(MAX_BLOCK);
            if self.playing {
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
            if self.playing {
                let pos = self.position();
                match self.region {
                    Some((start, end)) if pos == end => self.deck.renderer.jump(start),
                    _ if self.deck.renderer.finished() => self.playing = false,
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
            Message::Play(from) => {
                self.resting = false;
                self.quiet = 0;
                if self.playing {
                    self.deck.renderer.jump(from);
                } else {
                    // Voices sounding at `from` fade in; hits at `from` play in full.
                    self.deck.renderer.seek(from, true);
                    self.playing = true;
                }
            }
            Message::Stop => {
                if self.playing {
                    self.deck.renderer.release();
                    self.playing = false;
                }
            }
            Message::Locate(to) => {
                if self.playing {
                    self.deck.renderer.jump(to);
                } else {
                    self.deck.renderer.seek(to, false);
                }
            }
            Message::Loop(region) => self.region = region,
            Message::Metronome(enabled) => self.metronome = enabled,
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
        if let Err(rtrb::PushError::Full(deck)) = self.garbage.push(deck) {
            std::mem::forget(deck);
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
        let old = match &mut self.fade {
            Fade::Cross { old, .. } => Some(&mut old.renderer),
            _ => None,
        };
        self.deck.renderer.process(out, self.playing, old, |_, _, _| {});
        if self.playing {
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
        if self.playing || self.click_gain > 0.0 {
            let p = self.deck.renderer.program();
            let target = if self.metronome && self.playing { 1.0 } else { 0.0 };
            let click_start = if self.playing { heard } else { self.click_position };
            let step = 1.0 / self.fade_len.max(1) as f64;
            if target > 0.0 || self.click_gain > 0.0 {
                for (k, frame) in out.iter_mut().enumerate() {
                    self.click_gain += (target - self.click_gain).clamp(-step, step);
                    let at = click_start + k as i64;
                    if at >= 0 && at < p.total as i64 {
                        let click = metronome_sample(at as usize, p.rate, p.tempo) * self.click_gain;
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
/// A short attack and decay soften the monitoring click; the higher tone marks each 4/4 downbeat.
fn metronome_sample(frame: usize, rate: u32, tempo: f64) -> f64 {
    let period = rate as f64 * 60.0 / tempo;
    let beat = ((frame as f64 + 0.5) / period).floor();
    let onset = (beat * period).round() as usize;
    let t = frame.saturating_sub(onset) as f64 / rate as f64;
    if t >= 0.025 { return 0.0; }
    let hz = if beat as u64 % 4 == 0 { 1760.0 } else { 1320.0 };
    let envelope = (t / 0.001).min(1.0) * (1.0 - t / 0.025).powi(3);
    0.16 * envelope * (std::f64::consts::TAU * hz * t).sin()
}
