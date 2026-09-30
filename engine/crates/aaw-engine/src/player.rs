//! The transport's audio-thread side: play from any frame, stop, locate while
//! playing and loop a region, while the host replaces the program after edits.
//!
//! The host thread builds each program's `Deck` (two renderers, so a locate can
//! start one while the other fades out) and sends it through a lock-free queue.
//! The player swaps decks between blocks with a short crossfade, so an edit is
//! heard without a click, and returns replaced decks to the host to be freed.
//! `Player::render` never allocates, frees, locks or blocks.

use crate::program::Program;
use crate::render::{Frame, Renderer};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

/// The largest block rendered at once; longer requests loop.
pub const MAX_BLOCK: usize = 4096;
/// Length of the crossfade after an edit and of the fades around a stop, locate
/// or loop jump.
pub const FADE_SECONDS: f64 = 0.005;
const QUEUE: usize = 256;

/// A program with the renderers the audio thread plays it with.
pub struct Deck {
    program: Arc<Program>,
    renderers: [Renderer; 2],
}

impl Deck {
    /// Built off the audio thread: renderers allocate their voice lists here.
    pub fn new(program: Arc<Program>) -> Box<Deck> {
        Box::new(Deck {
            renderers: [
                Renderer::new(program.clone(), 0, MAX_BLOCK),
                Renderer::new(program.clone(), 0, MAX_BLOCK),
            ],
            program,
        })
    }
}

pub enum Message {
    Program(Box<Deck>),
    Play(usize),
    Stop,
    Locate(usize),
    /// Start and end frames; playback reaching the end jumps to the start.
    Loop(Option<(usize, usize)>),
}

enum Fade {
    None,
    /// The deck's other renderer rings out without starting new voices.
    Tail { index: usize, left: usize },
    /// The previous program, crossfading into the current one.
    Cross { old: Box<Deck>, index: usize, left: usize },
}

/// What the audio thread publishes for the host.
#[derive(Default)]
pub struct Shared {
    pub position: AtomicU64,
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
    current: usize,
    playing: bool,
    fade: Fade,
    pending: Option<Box<Deck>>,
    region: Option<(usize, usize)>,
    fade_len: usize,
    inbox: rtrb::Consumer<Message>,
    garbage: rtrb::Producer<Box<Deck>>,
    shared: Arc<Shared>,
    scratch: Vec<Frame>,
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
    let fade_len = ((FADE_SECONDS * program.rate as f64).round() as usize).max(1);
    let player = Player {
        deck: Deck::new(program.clone()),
        current: 0,
        playing: false,
        fade: Fade::None,
        pending: None,
        region: None,
        fade_len,
        inbox: inbox_rx,
        garbage: garbage_tx,
        shared: shared.clone(),
        scratch: vec![[0.0; 2]; MAX_BLOCK],
    };
    let control = Control {
        inbox: inbox_tx,
        garbage: garbage_rx,
        shared,
        program,
    };
    (control, player)
}

/// A frame of one program at the same beat in another.
fn carry(frame: usize, from: &Program, to: &Program) -> usize {
    if from.tempo == to.tempo && from.rate == to.rate {
        return frame;
    }
    (frame as f64 * (to.rate as f64 / from.rate as f64) * (from.tempo / to.tempo)).round() as usize
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

    pub fn set_loop(&mut self, region: Option<(usize, usize)>) -> Result<(), String> {
        self.send(Message::Loop(region))
    }

    /// The last program sent.
    pub fn program(&self) -> &Arc<Program> {
        &self.program
    }

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
    pub fn position(&self) -> usize {
        self.deck.renderers[self.current].position()
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    /// Fills `out` with the next frames: silence while stopped.
    pub fn render(&mut self, out: &mut [Frame]) {
        while let Ok(m) = self.inbox.pop() {
            self.handle(m);
        }
        let mut done = 0;
        while done < out.len() {
            let mut n = (out.len() - done).min(MAX_BLOCK);
            if let (true, Some((_, end))) = (self.playing, self.region) {
                let pos = self.position();
                if pos < end {
                    n = n.min(end - pos);
                }
            }
            self.block(&mut out[done..done + n]);
            done += n;
            if self.playing {
                let pos = self.position();
                match self.region {
                    Some((start, end)) if pos == end => self.locate(start),
                    _ if pos >= self.deck.program.total => self.playing = false,
                    _ => {}
                }
            }
            self.settle();
        }
        self.shared.position.store(self.position() as u64, Relaxed);
        self.shared.playing.store(self.playing, Relaxed);
    }

    fn handle(&mut self, m: Message) {
        match m {
            Message::Program(deck) => {
                if matches!(self.fade, Fade::None) {
                    self.install(deck);
                } else if let Some(old) = self.pending.replace(deck) {
                    self.discard(old);
                }
            }
            Message::Play(from) => {
                if self.playing {
                    self.locate(from);
                } else {
                    // Voices sounding at `from` fade in; hits at `from` play in full.
                    self.deck.renderers[self.current].seek(from, self.fade_len);
                    self.playing = true;
                }
            }
            Message::Stop => {
                if self.playing {
                    let pos = self.position();
                    self.ring_out();
                    self.deck.renderers[self.current].seek(pos, 0);
                    self.playing = false;
                }
            }
            Message::Locate(to) => {
                if self.playing {
                    self.locate(to);
                } else {
                    self.deck.renderers[self.current].seek(to, 0);
                }
            }
            Message::Loop(region) => self.region = region,
        }
    }

    /// Lets the current renderer ring out and switches to the other one.
    fn ring_out(&mut self) {
        self.cut();
        self.deck.renderers[self.current].set_triggers(false);
        self.fade = Fade::Tail {
            index: self.current,
            left: self.fade_len,
        };
        self.current = 1 - self.current;
    }

    fn locate(&mut self, to: usize) {
        self.ring_out();
        self.deck.renderers[self.current].seek(to, self.fade_len);
    }

    fn install(&mut self, mut deck: Box<Deck>) {
        let pos = carry(self.position(), &self.deck.program, &deck.program);
        deck.renderers[0].seek(pos, 0);
        let old = std::mem::replace(&mut self.deck, deck);
        let index = std::mem::replace(&mut self.current, 0);
        if self.playing {
            self.fade = Fade::Cross {
                old,
                index,
                left: self.fade_len,
            };
        } else {
            self.discard(old);
        }
        self.shared.swaps.fetch_add(1, Relaxed);
    }

    /// Ends any fade at once.
    fn cut(&mut self) {
        if let Fade::Cross { old, .. } = std::mem::replace(&mut self.fade, Fade::None) {
            self.discard(old);
        }
    }

    /// Finishes a completed fade and installs a program that waited for it.
    fn settle(&mut self) {
        let finished = match &self.fade {
            Fade::None => false,
            Fade::Tail { left, .. } | Fade::Cross { left, .. } => *left == 0,
        };
        if finished {
            self.cut();
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
        if self.playing {
            self.deck.renderers[self.current].process(out, |_, _| {});
            self.shared.played.fetch_add(n as u64, Relaxed);
        } else {
            out.fill([0.0; 2]);
        }
        let len = self.fade_len as f64;
        let scratch = &mut self.scratch[..n];
        match &mut self.fade {
            Fade::None => {}
            Fade::Tail { index, left } => {
                self.deck.renderers[*index].process(scratch, |_, _| {});
                for (k, (o, s)) in out.iter_mut().zip(scratch.iter()).enumerate() {
                    let g = left.saturating_sub(k) as f64 / len;
                    o[0] += s[0] * g;
                    o[1] += s[1] * g;
                }
                *left = left.saturating_sub(n);
            }
            Fade::Cross { old, index, left } => {
                old.renderers[*index].process(scratch, |_, _| {});
                for (k, (o, s)) in out.iter_mut().zip(scratch.iter()).enumerate() {
                    let g = left.saturating_sub(k) as f64 / len;
                    o[0] = o[0] * (1.0 - g) + s[0] * g;
                    o[1] = o[1] * (1.0 - g) + s[1] * g;
                }
                *left = left.saturating_sub(n);
            }
        }
    }
}
