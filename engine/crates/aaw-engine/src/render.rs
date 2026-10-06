//! The one processing path for offline export and real-time playback.
//!
//! A `Renderer` plays a program as one continuous stream: each track's voices
//! through its inserts, fader and sends, the returns, and the master chain,
//! with every path delayed to the slowest (see `program`). Voices are computed
//! from their own positions, devices carry their state from block to block and
//! automation is read at timeline frames, so the output does not depend on how
//! the stream is cut into blocks. `process` never allocates.
//!
//! The transport moves only the voices. A seek picks up the voices sounding at
//! the new position; effects keep their state, so a reverb rings on through a
//! locate or a stop, and starts empty only when the stream does. While the
//! transport is stopped the stream still runs, on silence, with the timeline
//! standing still.
//!
//! A renderer can take over from the one playing the previous revision of the
//! song: devices of the same kind continue from the old ones' state, levels
//! and knobs glide to their new values, and where a track's voices changed
//! the old ones ring out under the new.

use crate::program::{amplitude, pan_gains, ChainProgram, Program, Voice};
use aaw_dsp::device::Unit;
use aaw_dsp::dynamics::Reduction;
use aaw_dsp::envelope::{Glide, Param};
use aaw_dsp::synth::Synth;
use aaw_dsp::Clock;
use std::sync::Arc;

pub use aaw_dsp::Frame;

/// Length of the fades around an edit, a stop, a locate or a loop jump.
pub const FADE_SECONDS: f64 = 0.005;

const SILENCE: Frame = [0.0; 2];

#[derive(Clone, Copy, Debug)]
struct Active {
    voice: usize,
    cursor: usize,
    /// Frames since a seek picked this voice up mid-sample, while it fades in;
    /// `DONE` for voices that play at full level.
    ramp: usize,
}

const DONE: usize = usize::MAX;

/// A track's sounding voices.
pub(crate) struct Voices {
    /// The next voice not yet started.
    next: usize,
    /// Sounding voices in the order they started.
    active: Vec<Active>,
    /// Voices ringing out after a jump or a stop.
    ringing: Vec<Active>,
    ringing_left: usize,
}

impl Voices {
    pub(crate) fn new(voices: &[Voice], max_block: usize) -> Voices {
        // A voice stays listed from the block it starts in to the block it
        // ends in, so capacity covers overlaps widened by one block.
        let capacity = overlap(voices, max_block);
        Voices {
            next: 0,
            active: Vec::with_capacity(capacity),
            ringing: Vec::with_capacity(capacity),
            ringing_left: 0,
        }
    }

    /// Moves to timeline frame `at` and picks up the voices sounding there,
    /// fading them in when `ramp` is set.
    pub(crate) fn seek(&mut self, voices: &[Voice], at: i64, ramp: bool) {
        self.active.clear();
        self.next = 0;
        for (i, v) in voices.iter().enumerate() {
            if v.start >= at {
                break;
            }
            let elapsed = (at - v.start) as usize;
            if elapsed < v.length {
                self.active.push(Active {
                    voice: i,
                    cursor: elapsed,
                    ramp: if ramp { 0 } else { DONE },
                });
            }
            self.next = i + 1;
        }
    }

    /// Whether nothing sounds before timeline frame `end`: no voice is
    /// sounding and none starts by then.
    pub(crate) fn idle(&self, voices: &[Voice], end: i64) -> bool {
        self.active.is_empty() && voices.get(self.next).is_none_or(|v| v.start >= end)
    }

    /// Lets the sounding voices ring out over `fade` frames.
    fn release(&mut self, fade: usize) {
        std::mem::swap(&mut self.active, &mut self.ringing);
        self.active.clear();
        self.ringing_left = if self.ringing.is_empty() { 0 } else { fade };
    }

    /// Adds the voices for timeline frames from `start` to `out`, starting
    /// those that begin in the block. Nothing sounds past `total`.
    pub(crate) fn render(&mut self, voices: &[Voice], out: &mut [Frame], start: i64, total: i64, ramp: usize) {
        let end = (start + out.len() as i64).min(total);
        if end <= start {
            return;
        }
        while self.next < voices.len() && voices[self.next].start < end {
            self.active.push(Active {
                voice: self.next,
                cursor: 0,
                ramp: DONE,
            });
            self.next += 1;
        }
        for a in self.active.iter_mut() {
            let v = &voices[a.voice];
            let offset = start.max(v.start);
            let frames = ((end - offset) as usize).min(v.length - a.cursor);
            let first = (offset - start) as usize;
            let out = &mut out[first..first + frames];
            if a.ramp == DONE {
                for (k, o) in out.iter_mut().enumerate() {
                    let i = a.cursor + k;
                    o[0] += v.sample(i, 0);
                    o[1] += v.sample(i, 1);
                }
            } else {
                for (k, o) in out.iter_mut().enumerate() {
                    let i = a.cursor + k;
                    let g = ((a.ramp + k) as f64 / ramp as f64).min(1.0);
                    o[0] += v.sample(i, 0) * g;
                    o[1] += v.sample(i, 1) * g;
                }
                a.ramp = if a.ramp + frames >= ramp { DONE } else { a.ramp + frames };
            }
            a.cursor += frames;
        }
        self.active.retain(|a| a.cursor < voices[a.voice].length);
    }

    /// Adds the voices ringing out, fading over what is left of `fade` frames.
    fn ring(&mut self, voices: &[Voice], out: &mut [Frame], fade: usize, ramp: usize) {
        if self.ringing_left == 0 {
            return;
        }
        let n = out.len().min(self.ringing_left);
        for a in self.ringing.iter_mut() {
            let v = &voices[a.voice];
            let frames = n.min(v.length - a.cursor);
            for (k, o) in out[..frames].iter_mut().enumerate() {
                let i = a.cursor + k;
                let mut g = (self.ringing_left - k) as f64 / fade as f64;
                if a.ramp != DONE {
                    g *= ((a.ramp + k) as f64 / ramp as f64).min(1.0);
                }
                o[0] += v.sample(i, 0) * g;
                o[1] += v.sample(i, 1) * g;
            }
            a.cursor += frames;
            if a.ramp != DONE {
                a.ramp += frames;
            }
        }
        self.ringing_left -= n;
        if self.ringing_left == 0 {
            self.ringing.clear();
        } else {
            self.ringing.retain(|a| a.cursor < voices[a.voice].length);
        }
    }

    /// Continues exactly where `old` is, for the same voices.
    fn copy(&mut self, old: &Voices) {
        self.next = old.next;
        self.active.clear();
        self.active.extend_from_slice(&old.active);
        self.ringing.clear();
        self.ringing.extend_from_slice(&old.ringing);
        self.ringing_left = old.ringing_left;
    }
}

/// A fixed delay, which lines a path up with a slower one.
struct DelayLine {
    buffer: Vec<Frame>,
    at: usize,
}

impl DelayLine {
    fn new(frames: usize) -> DelayLine {
        DelayLine {
            buffer: vec![SILENCE; frames],
            at: 0,
        }
    }

    fn process(&mut self, x: &mut [Frame]) {
        if self.buffer.is_empty() {
            return;
        }
        for f in x.iter_mut() {
            std::mem::swap(f, &mut self.buffer[self.at]);
            self.at = (self.at + 1) % self.buffer.len();
        }
    }

    /// Takes `old`'s contents when it is as long.
    fn take_over(&mut self, old: &mut DelayLine) {
        if old.buffer.len() == self.buffer.len() {
            std::mem::swap(&mut self.buffer, &mut old.buffer);
            self.at = old.at;
        }
    }
}

struct Slot {
    unit: Unit,
    upstream: usize,
    /// The sidechain's source track and the delay that lines its audio up.
    key: Option<(usize, DelayLine)>,
    /// Whether a newer chain took this unit's state.
    taken: bool,
}

/// A channel's inserts in series.
struct Chain {
    slots: Vec<Slot>,
}

impl Chain {
    /// `reverbs` counts the renderer's reverbs so far, which spread their work.
    fn new(program: &ChainProgram, max_block: usize, glide: usize, reverbs: &mut usize) -> Chain {
        Chain {
            slots: program
                .devices
                .iter()
                .map(|d| Slot {
                    unit: {
                        let unit = Unit::new(&d.plan, max_block, glide, *reverbs);
                        *reverbs += usize::from(matches!(unit.device, aaw_dsp::device::Device::Reverb(_)));
                        unit
                    },
                    upstream: d.upstream,
                    key: d.key.as_ref().map(|k| (k.source, DelayLine::new(k.delay))),
                    taken: false,
                })
                .collect(),
        }
    }

    /// Processes a block in place. `clock` is the timeline of the chain's
    /// input; earlier devices' latency delays the audio reaching each device.
    /// `sources` are the tracks whose insert outputs key sidechains.
    fn process(&mut self, x: &mut [Frame], clock: Clock, sources: &[TrackState], scratch: &mut [Frame]) {
        for slot in &mut self.slots {
            let key = match &mut slot.key {
                Some((source, line)) => {
                    let key = &mut scratch[..x.len()];
                    key.copy_from_slice(&sources[*source].out[..x.len()]);
                    line.process(key);
                    Some(&*key)
                }
                None => None,
            };
            slot.unit.process(x, key, clock.earlier(slot.upstream));
        }
    }

    /// Each device continues from the first unused device of `old` with the
    /// same signature.
    fn take_over(&mut self, old: &mut Chain) {
        for slot in &mut self.slots {
            let Some(was) = old.slots.iter_mut().find(|o| !o.taken && o.unit.signature == slot.unit.signature) else {
                continue;
            };
            was.taken = true;
            slot.unit.take_over(&mut was.unit);
            if let (Some((_, line)), Some((_, old_line))) = (&mut slot.key, &mut was.key) {
                line.take_over(old_line);
            }
        }
    }
}

/// A fader's glides after a live edit.
struct Strip {
    /// The left and right gains of level and pan together.
    gains: [Glide; 2],
    /// 1 while heard, 0 while muted or solo-muted.
    heard: Glide,
}

impl Strip {
    fn new(glide: usize) -> Strip {
        Strip {
            gains: [Glide::new(glide), Glide::new(glide)],
            heard: Glide::new(glide),
        }
    }

    /// Continues from `old`, gliding where the levels or the muting changed.
    fn take_over(&mut self, old: &Strip, levels: bool, muting: bool) {
        self.gains[0].take_over(&old.gains[0], levels);
        self.gains[1].take_over(&old.gains[1], levels);
        self.heard.take_over(&old.heard, muting);
    }
}

struct TrackState {
    voices: Voices,
    /// A MIDI track's Synth, with its voices, and its patch's own effects,
    /// which the voices go through before the inserts.
    synth: Option<Synth>,
    patch_chain: Chain,
    /// The track of the renderer being taken over from whose voices ring out
    /// here, when this track's voices differ from them.
    inherits: Option<usize>,
    chain: Chain,
    /// The inserts' output for the current block, which keys sidechains.
    out: Vec<Frame>,
    align: DelayLine,
    strip: Strip,
    /// Each send's level, by return: from the pre-fader tap and from the
    /// post-fader one. A send uses one of them, and glides between them when
    /// it changes from one to the other.
    sends: Vec<[Glide; 2]>,
}

struct ReturnState {
    chain: Chain,
    bus: Vec<Frame>,
    align: DelayLine,
    strip: Strip,
}

/// What a device did, for the render report.
pub struct DeviceReport {
    pub kind: &'static str,
    pub bypass: bool,
    pub latency: usize,
    /// The names of every lane on the effect, constant ones included, sorted.
    pub automated: Vec<String>,
    /// Gain reduction over the session, for a compressor or limiter.
    pub reduction: Option<Reduction>,
}

/// How many notes previewed through Samplers sound at once, across the
/// tracks; past that the oldest stops.
const PREVIEWS: usize = 16;

/// A note previewed through a track's Sampler, outside the timeline.
struct Previewed {
    track: usize,
    voice: Voice,
    cursor: usize,
    /// Its place in the order previews arrived in.
    number: u64,
}

impl Previewed {
    fn sounding(&self) -> bool {
        self.cursor < self.voice.length
    }

    fn render(&mut self, out: &mut [Frame]) {
        let frames = out.len().min(self.voice.length - self.cursor);
        for (k, o) in out[..frames].iter_mut().enumerate() {
            let i = self.cursor + k;
            o[0] += self.voice.sample(i, 0);
            o[1] += self.voice.sample(i, 1);
        }
        self.cursor += frames;
    }
}

pub struct Renderer {
    program: Arc<Program>,
    /// The transport's position: the timeline frame of voices with no delay.
    cursor: usize,
    max_block: usize,
    /// Frames of a fade, and of the glide after a live edit.
    fade: usize,
    /// Frames over which voices picked up by the last seek fade in.
    ramp: usize,
    tracks: Vec<TrackState>,
    returns: Vec<ReturnState>,
    master_chain: Chain,
    master_gain: Glide,
    /// The dry sum, delayed to line up with the returns.
    dry_line: DelayLine,
    dry: Vec<Frame>,
    work: Vec<Frame>,
    post: Vec<Frame>,
    key: Vec<Frame>,
    values: [Vec<f64>; 5],
    /// Notes previewed through Samplers, in slots that are kept, finished or
    /// not, until a later preview takes their place.
    previews: Vec<Option<Previewed>>,
    previewed: u64,
}

/// A frame of one program at the same beat in another.
pub fn carry(frame: usize, from: &Program, to: &Program) -> usize {
    if from.tempo == to.tempo && from.rate == to.rate {
        return frame;
    }
    (frame as f64 * (to.rate as f64 / from.rate as f64) * (from.tempo / to.tempo)).round() as usize
}

/// Silences the frames of a chain's output at or past the session end: the
/// timeline is cut at the session length. Only while the transport rolls;
/// a standing stream carries tails and previewed notes whole.
fn gate(x: &mut [Frame], first: i64, step: i64, total: i64) {
    debug_assert_eq!(step, 1);
    let keep = (total - first).clamp(0, x.len() as i64) as usize;
    x[keep..].fill(SILENCE);
}

/// A strip's exact pan gains and level for a block.
fn levels(gain: &Param, pan: &Param, frame: i64, step: i64, left: &mut [f64], right: &mut [f64], level: &mut [f64]) {
    if pan.moves() {
        pan.fill(frame, step, left);
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            [*l, *r] = pan_gains(2, *l);
        }
    } else {
        let g = pan_gains(2, pan.value());
        left.fill(g[0]);
        right.fill(g[1]);
    }
    decibels(gain, frame, step, level);
}

/// A level's exact amplitude for a block.
fn decibels(gain: &Param, frame: i64, step: i64, level: &mut [f64]) {
    if gain.moves() {
        gain.fill(frame, step, level);
        for a in level.iter_mut() {
            *a = amplitude(*a);
        }
    } else {
        level.fill(amplitude(gain.value()));
    }
}

/// Applies level and pan to a block: `post = (x * pan) * level`, plus what is
/// left of a glide.
fn fade_strip(strip: &mut Strip, x: &[Frame], post: &mut [Frame], v: &mut [Vec<f64>; 5]) {
    let n = x.len();
    let [left, right, level, a, b] = v;
    let moving_left = strip.gains[0].offsets(left[0] * level[0], left[n - 1] * level[n - 1], &mut a[..n]);
    let moving_right = strip.gains[1].offsets(right[0] * level[0], right[n - 1] * level[n - 1], &mut b[..n]);
    for i in 0..n {
        let f = x[i];
        let mut p = [f[0] * left[i] * level[i], f[1] * right[i] * level[i]];
        if moving_left {
            p[0] += f[0] * a[i];
        }
        if moving_right {
            p[1] += f[1] * b[i];
        }
        post[i] = p;
    }
}

/// Silences a block that is not heard, or fades it while that changes.
fn mute(heard: &mut Glide, silent: bool, post: &mut [Frame], offsets: &mut [f64]) {
    let h = if silent { 0.0 } else { 1.0 };
    if heard.offsets(h, h, offsets) {
        for (p, o) in post.iter_mut().zip(offsets.iter()) {
            let g = h + o;
            *p = [p[0] * g, p[1] * g];
        }
    } else if silent {
        post.fill(SILENCE);
    }
}

impl Renderer {
    /// A renderer with its transport at `from`, with voices that started
    /// earlier and are still sounding picked up mid-sample, and every effect
    /// empty. Blocks may be up to `max_block` frames.
    pub fn new(program: Arc<Program>, from: usize, max_block: usize) -> Renderer {
        let fade = ((FADE_SECONDS * program.rate as f64).round() as usize).max(1);
        let mut reverbs = 0;
        let tracks = program
            .tracks
            .iter()
            .map(|t| TrackState {
                voices: Voices::new(&t.voices, max_block),
                synth: t.synth.as_ref().map(|s| Synth::new(s.patch.clone(), s.notes.clone(), max_block, fade)),
                patch_chain: match &t.synth {
                    Some(s) => Chain::new(&s.chain, max_block, fade, &mut reverbs),
                    None => Chain { slots: Vec::new() },
                },
                inherits: None,
                chain: Chain::new(&t.chain, max_block, fade, &mut reverbs),
                out: vec![SILENCE; max_block],
                align: DelayLine::new(t.align),
                strip: Strip::new(fade),
                sends: t.sends.iter().map(|_| [Glide::new(fade), Glide::new(fade)]).collect(),
            })
            .collect();
        let returns = program
            .returns
            .iter()
            .map(|r| ReturnState {
                chain: Chain::new(&r.chain, max_block, fade, &mut reverbs),
                bus: vec![SILENCE; max_block],
                align: DelayLine::new(r.align),
                strip: Strip::new(fade),
            })
            .collect::<Vec<_>>();
        let mut r = Renderer {
            cursor: 0,
            max_block,
            fade,
            ramp: 0,
            tracks,
            returns,
            master_chain: Chain::new(&program.master.chain, max_block, fade, &mut reverbs),
            master_gain: Glide::new(fade),
            dry_line: DelayLine::new(program.return_latency),
            dry: vec![SILENCE; max_block],
            work: vec![SILENCE; max_block],
            post: vec![SILENCE; max_block],
            key: vec![SILENCE; max_block],
            values: [0; 5].map(|_| vec![0.0; max_block]),
            previews: (0..PREVIEWS).map(|_| None).collect(),
            previewed: 0,
            program,
        };
        r.seek(from, false);
        r
    }

    /// Moves the transport to `to` and picks up the voices sounding there,
    /// fading them in when `ramp` is set. Voices starting at or after `to`
    /// play in full, and effects keep their state. Does not allocate.
    pub fn seek(&mut self, to: usize, ramp: bool) {
        self.cursor = to;
        self.ramp = if ramp { self.fade } else { 0 };
        for (t, state) in self.program.tracks.iter().zip(self.tracks.iter_mut()) {
            state.voices.seek(&t.voices, to as i64 - t.delay as i64, ramp);
            if let Some(s) = &mut state.synth {
                s.seek(to as i64 - t.delay as i64, ramp);
            }
        }
    }

    /// Plays a note now through the Synth of the track at `track` among the
    /// program's tracks, outside the timeline: `pitch` as a MIDI number,
    /// `velocity` 0 to 1, let go after `frames` frames. It runs through the
    /// track's chain and fader as its notes do, whether or not the transport
    /// rolls. Nothing happens for a track without a Synth. Does not allocate.
    pub fn preview(&mut self, track: usize, pitch: f64, velocity: f64, frames: usize) {
        if let Some(s) = self.tracks.get_mut(track).and_then(|t| t.synth.as_mut()) {
            s.preview(pitch, velocity, frames);
        }
    }

    /// Plays `voice`, a note of the Sampler of the track at `track` made by
    /// `program::preview_voice`, now and outside the timeline, as `preview`
    /// does through a Synth. Returns the voice whose slot it takes, a
    /// finished one or else the oldest, for the caller to free off the audio
    /// thread. Does not allocate.
    pub fn preview_voice(&mut self, track: usize, voice: Voice) -> Option<Voice> {
        self.previewed += 1;
        let slot = match self.previews.iter().position(|s| s.as_ref().is_none_or(|s| !s.sounding())) {
            Some(free) => free,
            None => (0..self.previews.len()).min_by_key(|&i| self.previews[i].as_ref().map_or(0, |s| s.number)).unwrap_or(0),
        };
        let new = Previewed {
            track,
            voice,
            cursor: 0,
            number: self.previewed,
        };
        self.previews[slot].replace(new).map(|old| old.voice)
    }

    /// Whether a previewed note is sounding on any track.
    pub fn previewing(&self) -> bool {
        self.tracks.iter().any(|t| t.synth.as_ref().is_some_and(|s| s.previewing()))
            || self.previews.iter().flatten().any(Previewed::sounding)
    }

    /// Lets the sounding voices ring out over a fade, as on a stop.
    pub fn release(&mut self) {
        for state in &mut self.tracks {
            state.voices.release(self.fade);
            if let Some(s) = &mut state.synth {
                s.release();
            }
        }
    }

    /// Moves the transport while playing: the old position rings out without
    /// starting new hits, and voices sounding at the new one fade in.
    pub fn jump(&mut self, to: usize) {
        self.release();
        self.seek(to, true);
    }

    /// The transport's position: the timeline frame of the voices starting now.
    pub fn position(&self) -> usize {
        self.cursor
    }

    /// The timeline frame at the output, which trails the transport by the
    /// program's latency.
    pub fn heard(&self) -> i64 {
        self.cursor as i64 - self.program.latency as i64
    }

    pub fn program(&self) -> &Arc<Program> {
        &self.program
    }

    pub fn max_block(&self) -> usize {
        self.max_block
    }

    /// The length of a fade in frames.
    pub fn fade(&self) -> usize {
        self.fade
    }

    /// Whether the session's last frame has left the output.
    pub fn finished(&self) -> bool {
        self.cursor >= self.program.total + self.program.latency
    }

    /// Renders the next `mix.len()` frames with the transport rolling, as an
    /// offline render does. See `process`.
    pub fn render(&mut self, mix: &mut [Frame], stem: impl FnMut(usize, &[Frame], i64)) {
        self.process(mix, true, None, stem);
    }

    /// Renders the next `mix.len()` frames of the stream into `mix`, whose
    /// first frame is timeline frame `heard()`.
    ///
    /// While `rolling`, the transport advances and voices play; otherwise the
    /// timeline stands still and only effects sound. `old` is the renderer
    /// this one took over from, while its voices ring out.
    ///
    /// `stem` receives each heard track's block, in render order, then each
    /// return's: after gain, pan, mute and solo and before the master gain and
    /// end fade, with the timeline frame of the block's first frame.
    pub fn process(
        &mut self,
        mix: &mut [Frame],
        rolling: bool,
        mut old: Option<&mut Renderer>,
        mut stem: impl FnMut(usize, &[Frame], i64),
    ) {
        let n = mix.len();
        assert!(n <= self.max_block, "block larger than max_block");
        if n == 0 {
            return;
        }
        let program = self.program.clone();
        let step = i64::from(rolling);
        let total = program.total as i64;
        let cursor = self.cursor as i64;
        let clock = |frame: i64| Clock { frame, step, total };
        // The timeline the faders, sends and return inputs share.
        let aligned = cursor - program.track_offset as i64;
        for r in &mut self.returns {
            r.bus[..n].fill(SILENCE);
        }
        self.dry[..n].fill(SILENCE);
        let mut stems = 0;

        for (ti, t) in program.tracks.iter().enumerate() {
            let (sources, rest) = self.tracks.split_at_mut(ti);
            let state = &mut rest[0];
            let out = &mut state.out[..n];
            out.fill(SILENCE);
            let start = cursor - t.delay as i64;
            if rolling {
                state.voices.render(&t.voices, out, start, total, self.ramp);
            }
            state.voices.ring(&t.voices, out, self.fade, self.ramp);
            if let Some(s) = &mut state.synth {
                s.render(out, start, total, rolling, self.ramp);
            }
            for p in self.previews.iter_mut().flatten().filter(|p| p.track == ti && p.sounding()) {
                p.render(out);
            }
            if let (Some(track), Some(old)) = (state.inherits, old.as_deref_mut()) {
                old.ring(track, out);
            }
            if !state.patch_chain.slots.is_empty() {
                state.patch_chain.process(out, clock(start), sources, &mut self.key);
            }
            if !state.chain.slots.is_empty() {
                state.chain.process(out, clock(start), sources, &mut self.key);
            }
            if rolling && !(state.chain.slots.is_empty() && state.patch_chain.slots.is_empty()) {
                gate(out, start - t.chain.latency as i64, step, total);
            }
            let x = &mut self.work[..n];
            x.copy_from_slice(out);
            state.align.process(x);

            let [left, right, level, ..] = &mut self.values;
            levels(&t.gain, &t.pan, aligned, step, &mut left[..n], &mut right[..n], &mut level[..n]);
            let post = &mut self.post[..n];
            fade_strip(&mut state.strip, &*x, post, &mut self.values);

            // Sends tap after the inserts (pre-fader) or after gain and pan
            // (post-fader). A muted or solo-muted track sends nothing.
            let [level, offsets, ..] = &mut self.values;
            let heard = if t.silent { 0.0 } else { 1.0 };
            for (ri, send) in t.sends.iter().enumerate() {
                if let Some(gain) = &send.level {
                    decibels(gain, aligned, step, &mut level[..n]);
                }
                let bus = &mut self.returns[ri].bus[..n];
                for (pre, glide) in [true, false].into_iter().zip(&mut state.sends[ri]) {
                    let tap: &[Frame] = if pre { &*x } else { &*post };
                    let sent = send.level.is_some() && send.pre_fader == pre;
                    let (first, last) = if sent { (heard * level[0], heard * level[n - 1]) } else { (0.0, 0.0) };
                    let moving = glide.offsets(first, last, &mut offsets[..n]);
                    if sent && !t.silent {
                        for i in 0..n {
                            bus[i][0] += tap[i][0] * level[i];
                            bus[i][1] += tap[i][1] * level[i];
                        }
                    }
                    if moving {
                        for i in 0..n {
                            bus[i][0] += tap[i][0] * offsets[i];
                            bus[i][1] += tap[i][1] * offsets[i];
                        }
                    }
                }
            }
            mute(&mut state.strip.heard, t.silent, post, &mut offsets[..n]);
            if t.in_mix {
                for (d, p) in self.dry[..n].iter_mut().zip(post.iter()) {
                    d[0] += p[0];
                    d[1] += p[1];
                }
                stem(stems, post, aligned);
                stems += 1;
            }
        }

        // The dry sum waits for the slowest return.
        self.dry_line.process(&mut self.dry[..n]);
        mix.copy_from_slice(&self.dry[..n]);
        // Returns are never solo-muted, so a soloed track keeps its reverb.
        for (ri, r) in program.returns.iter().enumerate() {
            let state = &mut self.returns[ri];
            let x = &mut state.bus[..n];
            if !state.chain.slots.is_empty() {
                state.chain.process(x, clock(aligned), &self.tracks, &mut self.key);
                if rolling {
                    gate(x, aligned - r.chain.latency as i64, step, total);
                }
            }
            let at = aligned - r.chain.latency as i64;
            let [left, right, level, ..] = &mut self.values;
            levels(&r.gain, &r.pan, at, step, &mut left[..n], &mut right[..n], &mut level[..n]);
            let post = &mut self.post[..n];
            fade_strip(&mut state.strip, x, post, &mut self.values);
            mute(&mut state.strip.heard, r.mute, post, &mut self.values[0][..n]);
            stem(stems, post, at);
            stems += 1;
            state.align.process(post);
            for (m, p) in mix.iter_mut().zip(post.iter()) {
                m[0] += p[0];
                m[1] += p[1];
            }
        }

        // Master gain, the master chain, then the end fade.
        let before = aligned - program.return_latency as i64;
        let heard = before - program.master.chain.latency as i64;
        let [level, offsets, ..] = &mut self.values;
        decibels(&program.master.gain, before, step, &mut level[..n]);
        if self.master_gain.offsets(level[0], level[n - 1], &mut offsets[..n]) {
            for (l, o) in level[..n].iter_mut().zip(offsets.iter()) {
                *l += o;
            }
        }
        // The end fade is the timeline's. While the transport stands still,
        // only tails and previewed notes sound, wherever it stands, so they
        // are heard whole: a stopped stream is cut by nothing but silence.
        let fader = |f: i64| {
            if !rolling || f < 0 {
                1.0
            } else if f >= total {
                0.0
            } else {
                program.fader(f as usize)
            }
        };
        if self.master_chain.slots.is_empty() {
            for (i, m) in mix.iter_mut().enumerate() {
                let env = fader(heard + i as i64 * step) * level[i];
                m[0] *= env;
                m[1] *= env;
            }
        } else {
            for (i, m) in mix.iter_mut().enumerate() {
                m[0] *= level[i];
                m[1] *= level[i];
            }
            self.master_chain.process(mix, clock(before), &self.tracks, &mut self.key);
            for (i, m) in mix.iter_mut().enumerate() {
                let f = fader(heard + i as i64 * step);
                m[0] *= f;
                m[1] *= f;
            }
        }
        if rolling {
            self.cursor += n;
        }
    }

    /// Adds a track's ringing voices to `out`, for the renderer taking over.
    fn ring(&mut self, track: usize, out: &mut [Frame]) {
        let voices = &self.program.tracks[track].voices;
        self.tracks[track].voices.ring(voices, out, self.fade, self.ramp);
        if let Some(s) = &mut self.tracks[track].synth {
            s.ring(out);
        }
    }

    /// Takes over from the renderer of a program with the same structure, at
    /// the same beat: every device continues from the old one's state, levels
    /// and knobs glide from where they were, and the audio in flight between
    /// channels moves over. A track whose voices are unchanged continues them;
    /// another picks its own up, and the old ones ring out from `old`, which
    /// must be passed to `process` until `end_take_over`. Returns whether any
    /// do. Does not allocate.
    pub fn take_over(&mut self, old: &mut Renderer) -> bool {
        debug_assert_eq!(self.program.structure, old.program.structure);
        let cursor = carry(old.cursor, &old.program, &self.program);
        self.cursor = cursor;
        self.ramp = self.fade;
        let same_time = cursor == old.cursor;
        let mut inherits = false;
        let program = self.program.clone();
        for (i, (state, was)) in self.tracks.iter_mut().zip(old.tracks.iter_mut()).enumerate() {
            let (t, o) = (&program.tracks[i], &old.program.tracks[i]);
            let same_notes = match (&t.synth, &o.synth) {
                (Some(a), Some(b)) => Arc::ptr_eq(&a.notes, &b.notes),
                (None, None) => true,
                _ => false,
            };
            if same_time && Arc::ptr_eq(&t.voices, &o.voices) && same_notes {
                state.voices.copy(&was.voices);
                if let (Some(s), Some(w)) = (&mut state.synth, &mut was.synth) {
                    s.take_over(w);
                }
                state.inherits = None;
            } else {
                state.voices.seek(&t.voices, cursor as i64 - t.delay as i64, true);
                if let Some(s) = &mut state.synth {
                    s.seek(cursor as i64 - t.delay as i64, true);
                }
                was.voices.release(old.fade);
                if let Some(w) = &mut was.synth {
                    w.release();
                    // A note being previewed carries on under the new notes.
                    if let Some(s) = &mut state.synth {
                        s.take_previews(w);
                    }
                }
                state.inherits = Some(i);
            }
            inherits |= state.inherits.is_some();
            state.patch_chain.take_over(&mut was.patch_chain);
            state.chain.take_over(&mut was.chain);
            state.align.take_over(&mut was.align);
            state.strip.take_over(&was.strip, t.gain != o.gain || t.pan != o.pan, t.silent != o.silent);
            for (r, (send, old_send)) in state.sends.iter_mut().zip(&was.sends).enumerate() {
                for (pre, (glide, old_glide)) in [true, false].into_iter().zip(send.iter_mut().zip(old_send)) {
                    let level = |s: &crate::program::SendProgram| s.level.clone().filter(|_| s.pre_fader == pre);
                    let changed = level(&t.sends[r]) != level(&o.sends[r]) || t.silent != o.silent;
                    glide.take_over(old_glide, changed);
                }
            }
        }
        if same_time && old.ramp != 0 {
            self.ramp = old.ramp;
        }
        for (i, (state, was)) in self.returns.iter_mut().zip(old.returns.iter_mut()).enumerate() {
            let (r, o) = (&program.returns[i], &old.program.returns[i]);
            state.chain.take_over(&mut was.chain);
            state.align.take_over(&mut was.align);
            state.strip.take_over(&was.strip, r.gain != o.gain || r.pan != o.pan, r.mute != o.mute);
        }
        self.master_chain.take_over(&mut old.master_chain);
        self.master_gain.take_over(&old.master_gain, program.master.gain != old.program.master.gain);
        self.dry_line.take_over(&mut old.dry_line);
        // Notes being previewed through Samplers carry on, on the same tracks.
        std::mem::swap(&mut self.previews, &mut old.previews);
        self.previewed = old.previewed;
        inherits
    }

    /// Stops ringing out the voices of the renderer taken over from.
    pub fn end_take_over(&mut self) {
        for state in &mut self.tracks {
            state.inherits = None;
        }
    }

    /// Replaces the renderer of a program with another structure, at the same
    /// beat, while the output is silent: channels with the same name keep the
    /// state of their devices that are still there, so tails ring on, and
    /// everything else starts afresh. Does not allocate.
    pub fn replace(&mut self, old: &mut Renderer) {
        let cursor = carry(old.cursor, &old.program, &self.program);
        self.seek(cursor, false);
        let program = self.program.clone();
        for (state, t) in self.tracks.iter_mut().zip(&program.tracks) {
            if let Some(i) = old.program.tracks.iter().position(|o| o.id == t.id) {
                state.patch_chain.take_over(&mut old.tracks[i].patch_chain);
                state.chain.take_over(&mut old.tracks[i].chain);
                // A note being previewed carries on under the new patch.
                if let (Some(s), Some(w)) = (&mut state.synth, &mut old.tracks[i].synth) {
                    s.take_previews(w);
                }
            }
        }
        for (state, r) in self.returns.iter_mut().zip(&program.returns) {
            if let Some(i) = old.program.returns.iter().position(|o| o.id == r.id) {
                state.chain.take_over(&mut old.returns[i].chain);
            }
        }
        self.master_chain.take_over(&mut old.master_chain);
        // Notes being previewed through Samplers carry on, on tracks that are
        // still there; the rest are freed with the old renderer.
        self.previewed = old.previewed;
        let mut free = 0;
        for slot in old.previews.iter_mut() {
            let Some(p) = slot.as_ref().filter(|p| p.sounding()) else { continue };
            let Some(track) = program.tracks.iter().position(|t| t.id == old.program.tracks[p.track].id) else { continue };
            if let Some(mut p) = slot.take() {
                p.track = track;
                self.previews[free] = Some(p);
                free += 1;
            }
        }
    }

    /// What each channel's effects did: tracks in render order, the patch
    /// effects of each track's Synth, returns, then the master.
    #[allow(clippy::type_complexity)]
    pub fn report(&self) -> (Vec<Vec<DeviceReport>>, Vec<Vec<DeviceReport>>, Vec<Vec<DeviceReport>>, Vec<DeviceReport>) {
        let chain = |program: &ChainProgram, chain: &Chain| {
            program
                .effects
                .iter()
                .map(|(kind, slot)| match slot {
                    None => DeviceReport {
                        kind,
                        bypass: true,
                        latency: 0,
                        automated: Vec::new(),
                        reduction: None,
                    },
                    Some(i) => DeviceReport {
                        kind,
                        bypass: false,
                        latency: chain.slots[*i].unit.latency,
                        automated: program.devices[*i].plan.lanes.keys().cloned().collect(),
                        reduction: chain.slots[*i].unit.reduction().copied(),
                    },
                })
                .collect()
        };
        (
            self.program.tracks.iter().zip(&self.tracks).map(|(p, s)| chain(&p.chain, &s.chain)).collect(),
            self.program
                .tracks
                .iter()
                .zip(&self.tracks)
                .map(|(p, s)| p.synth.as_ref().map_or_else(Vec::new, |synth| chain(&synth.chain, &s.patch_chain)))
                .collect(),
            self.program.returns.iter().zip(&self.returns).map(|(p, s)| chain(&p.chain, &s.chain)).collect(),
            chain(&self.program.master.chain, &self.master_chain),
        )
    }
}

/// The most voices listed at once when each is held `extra` frames past its end.
fn overlap(voices: &[Voice], extra: usize) -> usize {
    let mut edges: Vec<(i64, i32)> = Vec::with_capacity(voices.len() * 2);
    for v in voices {
        edges.push((v.start, 1));
        edges.push((v.start + (v.length + extra) as i64, -1));
    }
    edges.sort();
    let (mut now, mut most) = (0i32, 0i32);
    for (_, d) in edges {
        now += d;
        most = most.max(now);
    }
    most as usize
}
