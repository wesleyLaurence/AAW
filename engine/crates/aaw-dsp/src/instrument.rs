//! An instrument of the person's own, as the engine plays it: the track's
//! notes turned into note-ons and note-offs for each block, given to a
//! real-time instrument's code, or the output the host computed offline,
//! played by the timeline.

use crate::device::{Made, Running};
use crate::envelope::Envelope;
use crate::{Clock, Frame};
use aaw_model::schedule::NoteOn;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// A note starting or ending within a block: the frame within the block,
/// the key, and the velocity, 1 to 127 for a start and 0 for an end, as
/// MIDI has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteEvent {
    pub at: usize,
    pub key: u8,
    pub velocity: u8,
}

/// The most notes held at once, on the timeline and previewed each; past
/// that the one held longest ends.
pub const MAX_HELD: usize = 256;
/// The most events one block is given; past that the later starts are
/// dropped.
pub const MAX_EVENTS: usize = 1024;

/// A track's notes as events: where the transport is among them, the notes
/// held, the previewed notes, and the events due at the next block's start.
#[derive(Clone, Debug)]
pub struct Feed {
    notes: Arc<Vec<NoteOn>>,
    /// The next note not yet started.
    next: usize,
    /// Notes of the timeline sounding: the frame each ends at and its key.
    held: Vec<(i64, u8)>,
    /// Notes previewed and sounding: the frames until each is let go, and
    /// its key.
    previews: Vec<(usize, u8)>,
    /// Ends and starts due at the next block's first frame: of a stop, a
    /// seek or a preview.
    pending: Vec<NoteEvent>,
    events: Vec<NoteEvent>,
}

fn key(pitch: i64) -> u8 {
    pitch.clamp(0, 127) as u8
}

impl Feed {
    /// A feed of `notes`, stopped at frame 0.
    pub fn new(notes: Arc<Vec<NoteOn>>) -> Feed {
        let mut f = Feed {
            notes,
            next: 0,
            held: Vec::with_capacity(MAX_HELD),
            previews: Vec::with_capacity(MAX_HELD),
            pending: Vec::with_capacity(MAX_EVENTS),
            events: Vec::with_capacity(MAX_EVENTS),
        };
        f.seek(0);
        f
    }

    pub fn notes(&self) -> &Arc<Vec<NoteOn>> {
        &self.notes
    }

    fn due(&mut self, key: u8, velocity: u8) {
        if self.pending.len() < MAX_EVENTS {
            self.pending.push(NoteEvent { at: 0, key, velocity });
        }
    }

    /// Ends every note of the timeline held, at the next block's start, as
    /// at a stop. Does not allocate.
    pub fn release(&mut self) {
        for i in 0..self.held.len() {
            let k = self.held[i].1;
            self.due(k, 0);
        }
        self.held.clear();
    }

    /// Moves to timeline frame `at`: the notes held end, and those sounding
    /// at `at`, started before it, start again at the next block's start.
    /// Does not allocate.
    pub fn seek(&mut self, at: i64) {
        self.release();
        self.next = self.notes.partition_point(|n| n.start < at);
        let notes = self.notes.clone();
        for n in notes[..self.next].iter().filter(|n| n.end > at) {
            self.hold(n.end, key(n.pitch));
            self.due(key(n.pitch), n.velocity.clamp(1, 127) as u8);
        }
    }

    /// Holds a note of the timeline until `end`; the one held longest ends
    /// now when too many are.
    fn hold(&mut self, end: i64, key: u8) {
        if self.held.len() >= MAX_HELD {
            let (_, k) = self.held.remove(0);
            self.due(k, 0);
        }
        self.held.push((end, key));
    }

    /// Plays a note now, outside the timeline: it starts at the next
    /// block's start and ends `frames` later, whether or not the transport
    /// rolls. Does not allocate.
    pub fn preview(&mut self, pitch: f64, velocity: f64, frames: usize) {
        let k = key(pitch.round() as i64);
        if self.previews.len() >= MAX_HELD {
            let (_, old) = self.previews.remove(0);
            self.due(old, 0);
        }
        self.previews.push((frames.max(1), k));
        self.due(k, (velocity * 127.0).round().clamp(1.0, 127.0) as u8);
    }

    /// Ends the previewed notes still held, at the next block's start.
    pub fn let_go_previews(&mut self) {
        for i in 0..self.previews.len() {
            let k = self.previews[i].1;
            self.due(k, 0);
        }
        self.previews.clear();
    }

    /// Ends the previewed notes of `pitch` still held and leaves the others.
    pub fn let_go_preview(&mut self, pitch: f64) {
        let k = key(pitch.round() as i64);
        let mut i = 0;
        while i < self.previews.len() {
            if self.previews[i].1 == k {
                self.previews.remove(i);
                self.due(k, 0);
            } else {
                i += 1;
            }
        }
    }

    pub fn previewing(&self) -> bool {
        !self.previews.is_empty()
    }

    /// Whether a note is held or due to start or end.
    pub fn busy(&self) -> bool {
        !self.held.is_empty() || !self.previews.is_empty() || !self.pending.is_empty()
    }

    /// The events of the block of `n` frames from timeline frame `start`,
    /// in the order of their frames: first what is due, in the order it
    /// came, then the rest, an end before a start at the same frame. While
    /// `rolling`, the notes starting in the block start and those ending in
    /// it end, none starting at or past `total`, where every note held
    /// ends; otherwise only previews and what is due sound. Does not
    /// allocate.
    pub fn block(&mut self, start: i64, n: usize, rolling: bool, total: i64) -> &[NoteEvent] {
        self.events.clear();
        self.events.extend_from_slice(&self.pending);
        self.pending.clear();
        let due = self.events.len();
        let end = start + n as i64;
        if rolling {
            let mut i = 0;
            while i < self.held.len() {
                let (until, k) = self.held[i];
                let off = until.min(total);
                if off < end {
                    self.push(((off - start).max(0)) as usize, k, 0);
                    self.held.swap_remove(i);
                } else {
                    i += 1;
                }
            }
            while let Some(note) = self.notes.get(self.next) {
                if note.start >= end {
                    break;
                }
                let (from, until, k, velocity) = (note.start, note.end, key(note.pitch), note.velocity.clamp(1, 127) as u8);
                self.next += 1;
                if from >= total || until <= start {
                    continue;
                }
                let at = (from - start).max(0) as usize;
                self.push(at, k, velocity);
                let off = until.min(total);
                if off < end {
                    self.push(((off - start).max(at as i64)) as usize, k, 0);
                } else {
                    self.hold(until, k);
                    // A note dropped from the held ends at the block's start.
                    while let Some(e) = self.pending.pop() {
                        self.push(e.at, e.key, e.velocity);
                    }
                }
            }
        }
        let mut i = 0;
        while i < self.previews.len() {
            let (left, k) = self.previews[i];
            if left < n {
                self.push(left, k, 0);
                self.previews.remove(i);
            } else {
                self.previews[i].0 = left - n;
                i += 1;
            }
        }
        // An end before a start at the same frame, so that a note repeated
        // where the last ends sounds again.
        self.events[due..].sort_unstable_by_key(|e| (e.at, e.velocity != 0));
        &self.events
    }

    fn push(&mut self, at: usize, key: u8, velocity: u8) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(NoteEvent { at, key, velocity });
        }
    }

    /// Continues where `old` is, which plays the same notes from the same
    /// frame. Does not allocate past the capacities.
    pub fn take_over(&mut self, old: &Feed) {
        self.next = old.next;
        self.held.clear();
        self.held.extend_from_slice(&old.held);
        self.previews.clear();
        self.previews.extend_from_slice(&old.previews);
        self.pending.clear();
        self.pending.extend_from_slice(&old.pending);
    }

    /// Takes the sound over from `old`, which played other notes or stood
    /// elsewhere: what was due there and what it held end at the next
    /// block's start, before what is due here, and the notes it was
    /// previewing carry on here. Does not allocate.
    pub fn end_from(&mut self, old: &Feed) {
        let mine = self.pending.len();
        for e in &old.pending {
            self.due(e.key, e.velocity);
        }
        for (_, k) in &old.held {
            self.due(*k, 0);
        }
        let theirs = self.pending.len() - mine;
        self.pending.rotate_right(theirs);
        self.previews.clear();
        self.previews.extend_from_slice(&old.previews);
    }
}

/// An instrument of the person's own as compiled: the notes it plays, the
/// device's entry, the lanes on its fields by `fields.F`, and what the host
/// made of it, its code or its offline output, if anything yet.
#[derive(Clone, Debug)]
pub struct InstrumentPlan {
    pub notes: Arc<Vec<NoteOn>>,
    pub device: aaw_model::Device,
    pub lanes: BTreeMap<String, Arc<Envelope>>,
    pub made: Option<Made>,
    /// Frames by which its sound trails its notes.
    pub latency: usize,
    /// What decides whether another instrument's state fits this one: the
    /// device and what the host made of it.
    pub signature: u64,
}

impl InstrumentPlan {
    pub fn new(notes: Arc<Vec<NoteOn>>, device: &aaw_model::Device, lanes: BTreeMap<String, Arc<Envelope>>, made: Option<Made>) -> InstrumentPlan {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("instrument", &device.name).hash(&mut h);
        let latency = match &made {
            Some(Made::Code(live)) => {
                ("live", live.signature()).hash(&mut h);
                live.latency()
            }
            Some(Made::Audio(audio)) => {
                ("audio", Arc::as_ptr(audio) as usize).hash(&mut h);
                0
            }
            Some(Made::Notes(_)) | None => 0,
        };
        InstrumentPlan {
            notes,
            device: device.clone(),
            lanes,
            made,
            latency,
            signature: h.finish(),
        }
    }
}

/// What an instrument of the person's own sounds with.
enum Sound {
    /// An instance of its code.
    Live(Box<dyn Running>),
    /// Its output over the timeline, as the host computed it.
    Cache(Arc<Vec<[f32; 2]>>),
    /// Nothing yet: the host has not made it, or it is not installed.
    Silent,
}

/// An instrument of the person's own on a track: its sound and its notes.
pub struct Instrument {
    sound: Sound,
    feed: Feed,
    signature: u64,
    /// Where the instrument writes its block, before it is added to the
    /// track's.
    scratch: Vec<Frame>,
    /// The timeline frame last rendered, for ringing out.
    frame: i64,
}

impl Instrument {
    /// The instrument a plan calls for, stopped at frame 0, for blocks of
    /// at most `max_block` frames. Made off the audio thread.
    pub fn new(plan: &InstrumentPlan, max_block: usize) -> Instrument {
        let sound = match &plan.made {
            Some(Made::Code(live)) => Sound::Live(live.instance(&plan.device, &plan.lanes)),
            Some(Made::Audio(audio)) => Sound::Cache(audio.clone()),
            Some(Made::Notes(_)) | None => Sound::Silent,
        };
        Instrument {
            sound,
            feed: Feed::new(plan.notes.clone()),
            signature: plan.signature,
            scratch: vec![[0.0; 2]; max_block.max(1)],
            frame: 0,
        }
    }

    pub fn seek(&mut self, at: i64) {
        self.frame = at;
        self.feed.seek(at);
    }

    /// Ends the notes held, as at a stop; the sound rings out through the
    /// code's own release.
    pub fn release(&mut self) {
        self.feed.release();
    }

    /// Plays a note now, outside the timeline: `pitch` as a MIDI number,
    /// `velocity` 0 to 1, let go after `frames` frames. Only code plays
    /// one: an offline instrument's sound is its cache.
    pub fn preview(&mut self, pitch: f64, velocity: f64, frames: usize) {
        if matches!(self.sound, Sound::Live(_)) {
            self.feed.preview(pitch, velocity, frames);
        }
    }

    pub fn let_go_previews(&mut self) {
        self.feed.let_go_previews();
    }

    pub fn let_go_preview(&mut self, pitch: f64) {
        self.feed.let_go_preview(pitch);
    }

    pub fn previewing(&self) -> bool {
        self.feed.previewing()
    }

    /// Whether code plays the notes, which a preview needs.
    pub fn live(&self) -> bool {
        matches!(self.sound, Sound::Live(_))
    }

    /// Adds the instrument's sound for timeline frames from `start` to
    /// `out`. While `rolling` the notes play from the timeline; otherwise
    /// only previews and what rings out sound. Does not allocate.
    pub fn render(&mut self, out: &mut [Frame], start: i64, total: i64, rolling: bool) {
        let n = out.len();
        if n == 0 {
            return;
        }
        self.frame = start;
        let clock = Clock {
            frame: start,
            step: i64::from(rolling),
            total,
        };
        match &mut self.sound {
            Sound::Live(running) => {
                let events = self.feed.block(start, n, rolling, total);
                let x = &mut self.scratch[..n];
                x.fill([0.0; 2]);
                running.play(x, clock, events);
                for (o, x) in out.iter_mut().zip(x.iter()) {
                    o[0] += x[0];
                    o[1] += x[1];
                }
            }
            Sound::Cache(audio) => {
                if !rolling {
                    return;
                }
                for (i, o) in out.iter_mut().enumerate() {
                    let at = start + i as i64;
                    if at >= total {
                        break;
                    }
                    if let Some(f) = usize::try_from(at).ok().and_then(|at| audio.get(at)) {
                        o[0] += f64::from(f[0]);
                        o[1] += f64::from(f[1]);
                    }
                }
            }
            Sound::Silent => {}
        }
    }

    /// Adds what rings out, the transport standing where it last was, for
    /// the instrument of a renderer being taken over from.
    pub fn ring(&mut self, out: &mut [Frame]) {
        let frame = self.frame;
        self.render(out, frame, i64::MAX, false);
    }

    /// Continues from `old`, an instrument of the same signature: the
    /// code's state carries on here. With `same_place` the old one played
    /// the same notes from the same frame and the notes carry on too;
    /// otherwise what it held ends and these notes start where this one
    /// stands. Does not allocate.
    pub fn take_over(&mut self, old: &mut Instrument, same_place: bool) {
        if self.signature != old.signature {
            return;
        }
        if let (Sound::Live(new), Sound::Live(was)) = (&mut self.sound, &mut old.sound) {
            new.take_over(was.as_mut());
        }
        if same_place {
            self.feed.take_over(&old.feed);
        } else {
            self.feed.end_from(&old.feed);
        }
        self.frame = old.frame;
    }

    /// Takes the notes `old` is previewing, when the code is the same, so
    /// that a note held through an edit is let go here.
    pub fn take_previews(&mut self, old: &mut Instrument) {
        if self.signature == old.signature {
            self.feed.previews.clear();
            self.feed.previews.extend_from_slice(&old.feed.previews);
            old.feed.previews.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_rational::BigRational;

    fn note(pitch: i64, start: i64, end: i64) -> NoteOn {
        NoteOn {
            pitch,
            velocity: 100,
            start,
            end,
            at: BigRational::from_integer(0.into()),
            beats: BigRational::from_integer(1.into()),
        }
    }

    fn ev(at: usize, key: u8, velocity: u8) -> NoteEvent {
        NoteEvent { at, key, velocity }
    }

    #[test]
    fn notes_start_and_end_at_their_frames_whatever_the_blocks() {
        let notes = Arc::new(vec![note(60, 10, 70), note(64, 70, 90), note(60, 70, 200)]);
        let all = |block: usize| {
            let mut f = Feed::new(notes.clone());
            let mut out = Vec::new();
            let mut start = 0;
            while start < 160 {
                for e in f.block(start, block, true, 150) {
                    out.push((start + e.at as i64, e.key, e.velocity));
                }
                start += block as i64;
            }
            out
        };
        let want = vec![(10, 60, 100), (70, 60, 0), (70, 60, 100), (70, 64, 100), (90, 64, 0), (150, 60, 0)];
        for block in [1, 7, 64, 160] {
            let mut got = all(block);
            // Within a frame, an end comes first; starts in any order.
            got.sort_by_key(|(at, key, v)| (*at, *v != 0, *key));
            assert_eq!(got, want, "blocks of {block}");
        }
    }

    #[test]
    fn a_seek_ends_what_is_held_and_starts_what_sounds_there() {
        let mut f = Feed::new(Arc::new(vec![note(60, 0, 100), note(62, 20, 40), note(64, 50, 80)]));
        assert_eq!(f.block(0, 30, true, 1000), &[ev(0, 60, 100), ev(20, 62, 100)]);
        f.seek(60);
        // 60 and 62 end, then 60 and 64, sounding at 60, start again.
        assert_eq!(f.block(60, 10, true, 1000), &[ev(0, 60, 0), ev(0, 62, 0), ev(0, 60, 100), ev(0, 64, 100)]);
        f.release();
        assert_eq!(f.block(70, 10, false, 1000), &[ev(0, 60, 0), ev(0, 64, 0)]);
        assert!(!f.busy());
    }

    #[test]
    fn a_preview_sounds_standing_still_and_ends_after_its_frames() {
        let mut f = Feed::new(Arc::new(Vec::new()));
        f.preview(67.0, 1.0, 25);
        assert!(f.previewing());
        assert_eq!(f.block(500, 10, false, 100), &[ev(0, 67, 127)]);
        assert_eq!(f.block(500, 10, false, 100), &[]);
        assert_eq!(f.block(500, 10, false, 100), &[ev(5, 67, 0)]);
        assert!(!f.previewing());
        // Let go before a block played it: it starts and ends, in that order.
        f.preview(60.0, 0.5, 1000);
        f.let_go_previews();
        assert_eq!(f.block(0, 10, false, 100), &[ev(0, 60, 64), ev(0, 60, 0)]);
    }

    #[test]
    fn taking_over_ends_the_old_notes_before_starting_the_new() {
        let mut old = Feed::new(Arc::new(vec![note(60, 0, 100)]));
        old.block(0, 10, true, 1000);
        let mut new = Feed::new(Arc::new(vec![note(60, 0, 100), note(67, 0, 100)]));
        new.seek(10);
        new.end_from(&old);
        assert_eq!(new.block(10, 10, true, 1000), &[ev(0, 60, 0), ev(0, 60, 100), ev(0, 67, 100)]);
    }

    #[test]
    fn standing_still_no_note_of_the_timeline_starts() {
        let mut f = Feed::new(Arc::new(vec![note(60, 0, 10)]));
        assert_eq!(f.block(0, 64, false, 100), &[]);
        assert_eq!(f.block(0, 64, true, 100), &[ev(0, 60, 100), ev(10, 60, 0)]);
    }

    #[test]
    fn an_offline_instrument_plays_its_cache_by_the_timeline_while_rolling() {
        let cache: Arc<Vec<[f32; 2]>> = Arc::new((0..10).map(|i| [i as f32, 0.5]).collect());
        let device = aaw_model::Device {
            id: None,
            name: "x".into(),
            version: None,
            fields: Default::default(),
            bypass: false,
        };
        let plan = InstrumentPlan::new(Arc::new(Vec::new()), &device, BTreeMap::new(), Some(Made::Audio(cache)));
        let mut i = Instrument::new(&plan, 8);
        let mut out = vec![[1.0, 0.0]; 4];
        i.render(&mut out, 7, 9, true);
        assert_eq!(out, vec![[8.0, 0.5], [9.0, 0.5], [1.0, 0.0], [1.0, 0.0]], "added, and nothing at or past the end");
        let mut still = vec![[0.0; 2]; 4];
        i.render(&mut still, 0, 9, false);
        assert_eq!(still, vec![[0.0; 2]; 4]);
        i.preview(60.0, 1.0, 10);
        assert!(!i.previewing() && !i.live(), "an offline instrument previews nothing");
    }
}
