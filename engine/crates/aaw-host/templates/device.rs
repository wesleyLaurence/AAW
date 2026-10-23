//! NAME: the scaffold of a real-time device, to edit.
//!
//! A real-time device runs on the audio thread as WebAssembly. This file is
//! its source; `daw plugin build SLUG` compiles it to device.wasm with
//! rustc (once: rustup target add wasm32-unknown-unknown). The standard
//! library is there, so f64's tanh, exp and powf are too.
//!
//! The engine calls `Device::process` with at most 128 frames at a time:
//! `left` and `right` hold the input, to be replaced by the output, and
//! `fields` the values in the manifest's order, a number as itself, a
//! switch as 1.0 or 0.0, a choice as the index of its choice. A lane changes
//! a field between calls, so glide what would click. `frame` is the
//! timeline frame of the first frame; `rolling` is false while the
//! transport stands still and a tail rings out. Make what the sound needs
//! in `Device::new`, which runs before any audio: `process` must not
//! allocate, wait or take long, or the engine stops the device and passes
//! its input through.

/// The fields, by their place in device.yaml.
const GAIN_DB: usize = 0;
const TONE_HZ: usize = 1;
const WARMTH_PERCENT: usize = 2;
const MODE: usize = 3;
const INVERT: usize = 4;
const FIELDS: usize = 5;

struct Device {
    rate: f64,
    /// The gain as it glides, linear.
    gain: f64,
    /// The tone's lowpass, a pole a channel.
    lowpass: [f64; 2],
}

impl Device {
    fn new(sample_rate: f64, _tempo: f64) -> Device {
        Device {
            rate: sample_rate,
            gain: f64::NAN,
            lowpass: [0.0; 2],
        }
    }

    /// The frames by which the device delays its input.
    fn latency(&self) -> usize {
        0
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32], fields: &[f64; FIELDS], _frame: i64, _rolling: bool) {
        let target = 10f64.powf(fields[GAIN_DB] / 20.0);
        if self.gain.is_nan() {
            self.gain = target;
        }
        // The gain glides to its value over about 5 ms.
        let glide = 1.0 - (-1.0 / (0.005 * self.rate)).exp();
        // At the top of its range the tone is open; below, a one-pole lowpass.
        let tone = fields[TONE_HZ];
        let open = tone >= 20000.0 || tone >= 0.49 * self.rate;
        let pole = (-2.0 * std::f64::consts::PI * tone / self.rate).exp();
        let warmth = fields[WARMTH_PERCENT] / 100.0;
        let drive = 1.0 + 3.0 * warmth;
        let mono = fields[MODE] == 1.0;
        let sign = if fields[INVERT] != 0.0 { -1.0 } else { 1.0 };
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.gain += (target - self.gain) * glide;
            let mut x = [f64::from(*l), f64::from(*r)];
            if mono {
                let m = 0.5 * (x[0] + x[1]);
                x = [m, m];
            }
            for (c, v) in x.iter_mut().enumerate() {
                let mut y = *v * self.gain;
                if !open {
                    self.lowpass[c] = (1.0 - pole) * y + pole * self.lowpass[c];
                    y = self.lowpass[c];
                }
                if warmth > 0.0 {
                    y = (1.0 - warmth) * y + warmth * (y * drive).tanh() / drive.tanh();
                }
                *v = sign * y;
            }
            (*l, *r) = (x[0] as f32, x[1] as f32);
        }
    }
}

// ---------------------------------------------------------------------------
// The contract, device ABI 1: what the engine calls. Leave it as it is.

const FRAMES: usize = 128;
static mut LEFT: [f32; FRAMES] = [0.0; FRAMES];
static mut RIGHT: [f32; FRAMES] = [0.0; FRAMES];
static mut VALUES: [f64; FIELDS] = [0.0; FIELDS];
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
pub extern "C" fn aaw_process(frames: i32, frame: i64, rolling: i32) {
    let n = (frames.max(0) as usize).min(FRAMES);
    // The engine calls one function at a time, so these are not shared.
    let (device, left, right, values) = unsafe { (&mut *&raw mut DEVICE, &mut *&raw mut LEFT, &mut *&raw mut RIGHT, &*&raw const VALUES) };
    if let Some(device) = device {
        device.process(&mut left[..n], &mut right[..n], values, frame, rolling != 0);
    }
}
