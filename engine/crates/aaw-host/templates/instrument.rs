//! NAME: the scaffold of a real-time instrument, to edit.
//!
//! A real-time instrument runs on the audio thread as WebAssembly. This
//! file is its source; `daw plugin build SLUG` compiles it to device.wasm
//! with rustc (once: rustup target add wasm32-unknown-unknown). The
//! standard library is there, so f64's sin, exp and powf are too.
//!
//! The engine calls `Device::process` with at most 128 frames at a time:
//! `notes` are the notes starting or ending within them, in order, each at
//! its frame among them, a velocity of 0 ending the note; `left` and
//! `right` hold silence, to be replaced by the sound; `fields` the values
//! in the manifest's order, a number as itself, a switch as 1.0 or 0.0, a
//! choice as the index of its choice. A lane changes a field between
//! calls, so glide what would click. `frame` is the timeline frame of the
//! first frame; `rolling` is false while the transport stands still, when
//! notes previewed from the piano roll still come and tails ring out. Make
//! what the sound needs in `Device::new`, which runs before any audio:
//! `process` must not allocate, wait or take long, or the engine stops the
//! instrument and it falls silent.

/// The fields, by their place in device.yaml.
const GAIN_DB: usize = 0;
const WAVE: usize = 1;
const ATTACK_MS: usize = 2;
const RELEASE_MS: usize = 3;
const TONE_HZ: usize = 4;
const FIELDS: usize = 5;

/// How many notes sound at once; past that the quietest is taken.
const VOICES: usize = 16;

#[derive(Clone, Copy)]
struct Voice {
    key: u8,
    /// Whether the key is still down.
    held: bool,
    velocity: f64,
    /// The envelope, 0 to 1.
    level: f64,
    phase: f64,
    /// The phase's step a frame.
    step: f64,
}

struct Device {
    rate: f64,
    voices: [Option<Voice>; VOICES],
    /// The gain as it glides, linear.
    gain: f64,
    /// The tone's lowpass.
    lowpass: f64,
}

impl Device {
    fn new(sample_rate: f64, _tempo: f64) -> Device {
        Device {
            rate: sample_rate,
            voices: [None; VOICES],
            gain: f64::NAN,
            lowpass: 0.0,
        }
    }

    /// The frames by which the sound trails its notes.
    fn latency(&self) -> usize {
        0
    }

    /// A note starts, or with a velocity of 0 ends.
    fn note(&mut self, key: u8, velocity: u8) {
        if velocity == 0 {
            for v in self.voices.iter_mut().flatten() {
                if v.key == key {
                    v.held = false;
                }
            }
            return;
        }
        let voice = Voice {
            key,
            held: true,
            velocity: f64::from(velocity) / 127.0,
            level: 0.0,
            phase: 0.0,
            step: 440.0 * 2f64.powf((f64::from(key) - 69.0) / 12.0) / self.rate,
        };
        // A free voice, else the quietest.
        let slot = self.voices.iter().position(Option::is_none).unwrap_or_else(|| {
            (0..VOICES).min_by(|&a, &b| {
                let level = |i: usize| self.voices[i].map_or(0.0, |v| v.level);
                level(a).total_cmp(&level(b))
            }).unwrap_or(0)
        });
        self.voices[slot] = Some(voice);
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32], notes: &[Note], fields: &[f64; FIELDS], _frame: i64, _rolling: bool) {
        let target = 10f64.powf(fields[GAIN_DB] / 20.0);
        if self.gain.is_nan() {
            self.gain = target;
        }
        // The gain glides to its value over about 5 ms.
        let glide = 1.0 - (-1.0 / (0.005 * self.rate)).exp();
        let rise = 1.0 / (fields[ATTACK_MS] / 1000.0 * self.rate).max(1.0);
        let fall = (-5.0 / (fields[RELEASE_MS] / 1000.0 * self.rate).max(1.0)).exp();
        let pole = (-2.0 * std::f64::consts::PI * fields[TONE_HZ].min(0.49 * self.rate) / self.rate).exp();
        let wave = fields[WAVE];
        let mut next = 0;
        for i in 0..left.len() {
            while next < notes.len() && notes[next].at <= i {
                self.note(notes[next].key, notes[next].velocity);
                next += 1;
            }
            self.gain += (target - self.gain) * glide;
            let mut sum = 0.0;
            for slot in self.voices.iter_mut() {
                let Some(v) = slot else { continue };
                v.level = if v.held { (v.level + rise).min(1.0) } else { v.level * fall };
                if !v.held && v.level < 1e-4 {
                    *slot = None;
                    continue;
                }
                let x = match wave as i32 {
                    0 => (2.0 * std::f64::consts::PI * v.phase).sin(),
                    2 => if v.phase < 0.5 { 1.0 } else { -1.0 },
                    _ => 2.0 * v.phase - 1.0,
                };
                sum += x * v.level * v.velocity;
                v.phase = (v.phase + v.step).fract();
            }
            self.lowpass = (1.0 - pole) * sum + pole * self.lowpass;
            let y = (self.lowpass * self.gain) as f32;
            left[i] = y;
            right[i] = y;
        }
        // Notes given past the frames, which the engine does not do.
        for n in &notes[next..] {
            self.note(n.key, n.velocity);
        }
    }
}

// ---------------------------------------------------------------------------
// The contract, device ABI 1 with an instrument's aaw_note: what the engine
// calls. Leave it as it is.

#[derive(Clone, Copy)]
struct Note {
    /// The frame among those of the next call.
    at: usize,
    key: u8,
    velocity: u8,
}

const FRAMES: usize = 128;
const MAX_NOTES: usize = 1024;
static mut LEFT: [f32; FRAMES] = [0.0; FRAMES];
static mut RIGHT: [f32; FRAMES] = [0.0; FRAMES];
static mut VALUES: [f64; FIELDS] = [0.0; FIELDS];
static mut NOTES: [Note; MAX_NOTES] = [Note { at: 0, key: 0, velocity: 0 }; MAX_NOTES];
static mut COUNT: usize = 0;
static mut DEVICE: Option<Device> = None;

#[unsafe(no_mangle)]
pub extern "C" fn aaw_abi() -> i32 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn aaw_init(sample_rate: f64, tempo: f64) {
    unsafe { *&raw mut DEVICE = Some(Device::new(sample_rate, tempo)) }
}

#[unsafe(no_mangle)]
pub extern "C" fn aaw_latency() -> i32 {
    unsafe { (*&raw const DEVICE).as_ref().map_or(0, |d| d.latency() as i32) }
}

#[unsafe(no_mangle)]
pub extern "C" fn aaw_fields() -> *mut f64 {
    &raw mut VALUES as *mut f64
}

#[unsafe(no_mangle)]
pub extern "C" fn aaw_left() -> *mut f32 {
    &raw mut LEFT as *mut f32
}

#[unsafe(no_mangle)]
pub extern "C" fn aaw_right() -> *mut f32 {
    &raw mut RIGHT as *mut f32
}

#[unsafe(no_mangle)]
pub extern "C" fn aaw_note(at: i32, key: i32, velocity: i32) {
    // The engine calls one function at a time, so these are not shared.
    let (notes, count) = unsafe { (&mut *&raw mut NOTES, &mut *&raw mut COUNT) };
    if *count < MAX_NOTES {
        notes[*count] = Note {
            at: at.max(0) as usize,
            key: key.clamp(0, 127) as u8,
            velocity: velocity.clamp(0, 127) as u8,
        };
        *count += 1;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aaw_process(frames: i32, frame: i64, rolling: i32) {
    let n = (frames.max(0) as usize).min(FRAMES);
    let (device, left, right, values, notes, count) =
        unsafe { (&mut *&raw mut DEVICE, &mut *&raw mut LEFT, &mut *&raw mut RIGHT, &*&raw const VALUES, &*&raw const NOTES, &mut *&raw mut COUNT) };
    if let Some(device) = device {
        device.process(&mut left[..n], &mut right[..n], &notes[..*count], values, frame, rolling != 0);
    }
    *count = 0;
}
