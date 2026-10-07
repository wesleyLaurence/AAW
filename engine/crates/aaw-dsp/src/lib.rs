//! Devices: the resampler, automation envelopes, the nine effects and the
//! Synth, as block processors with explicit state. Output never depends on
//! how the timeline is cut into blocks, and processing never allocates.

pub mod biquad;
pub mod chorus;
pub mod delay;
pub mod device;
pub mod dynamics;
pub mod envelope;
pub mod resample;
pub mod reverb;
pub mod saturation;
pub mod spectrum;
pub mod svf;
pub mod synth;
pub mod utility;
pub mod wavetable;

/// One stereo frame.
pub type Frame = [f64; 2];

/// Where a block is on the timeline.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    /// The timeline frame of the block's first frame. It may precede zero.
    pub frame: i64,
    /// Frames the timeline advances per frame: 1 while the transport rolls, 0
    /// while it is stopped and effects ring out.
    pub step: i64,
    /// The session length in frames; gain reduction is measured inside it.
    pub total: i64,
}

impl Clock {
    /// The clock `frames` earlier on the timeline, as audio delayed by that
    /// much latency is.
    pub fn earlier(self, frames: usize) -> Clock {
        Clock {
            frame: self.frame - frames as i64,
            ..self
        }
    }
}
