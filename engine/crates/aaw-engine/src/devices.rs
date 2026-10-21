//! The Mac's audio devices: what each offers, which is the default, and a
//! count that moves when the default changes or a device comes or goes, so a
//! host follows the system's choice as every Mac app does.

use aaw_model::value::{dict, Value};
use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::OnceLock;

/// What the Mac says of one device.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    pub name: String,
    pub input_channels: u16,
    pub output_channels: u16,
    /// The rates it offers for output, or for input when it has no output.
    pub sample_rates: Vec<u32>,
    /// The rate it runs at now, which the Mac's Audio MIDI Setup changes.
    pub current_rate: Option<u32>,
    /// The buffers it takes, in frames.
    pub buffer_frames: Option<(u32, u32)>,
    pub default_input: bool,
    pub default_output: bool,
}

impl Info {
    pub fn to_value(&self) -> Value {
        dict(vec![
            ("name", Value::str(&self.name)),
            ("input_channels", Value::int(self.input_channels as i64)),
            ("output_channels", Value::int(self.output_channels as i64)),
            ("sample_rates", Value::List(self.sample_rates.iter().map(|r| Value::int(*r as i64)).collect())),
            ("current_rate", self.current_rate.map_or(Value::None, |r| Value::int(r as i64))),
            (
                "buffer_frames",
                self.buffer_frames
                    .map_or(Value::None, |(min, max)| Value::List(vec![Value::int(min as i64), Value::int(max as i64)])),
            ),
            ("default_input", Value::Bool(self.default_input)),
            ("default_output", Value::Bool(self.default_output)),
        ])
    }
}

fn name_of(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_string())
}

/// Every device the Mac has, in the order it lists them.
pub fn list() -> Result<Vec<Info>, String> {
    let host = cpal::default_host();
    let default_input = host.default_input_device().and_then(|d| name_of(&d));
    let default_output = host.default_output_device().and_then(|d| name_of(&d));
    let mut out = Vec::new();
    for device in host.devices().map_err(|e| e.to_string())? {
        let Some(name) = name_of(&device) else { continue };
        let outputs: Vec<cpal::SupportedStreamConfigRange> = device.supported_output_configs().map(|c| c.collect()).unwrap_or_default();
        let inputs: Vec<cpal::SupportedStreamConfigRange> = device.supported_input_configs().map(|c| c.collect()).unwrap_or_default();
        let channels = |configs: &[cpal::SupportedStreamConfigRange]| configs.iter().map(|c| c.channels()).max().unwrap_or(0);
        let rates = |configs: &[cpal::SupportedStreamConfigRange]| {
            let mut rates: Vec<u32> = configs.iter().flat_map(|c| [c.min_sample_rate(), c.max_sample_rate()]).collect();
            rates.sort_unstable();
            rates.dedup();
            rates
        };
        let (output_channels, input_channels) = (channels(&outputs), channels(&inputs));
        let (ranged, current) = if output_channels > 0 {
            (rates(&outputs), device.default_output_config().ok().map(|c| c.sample_rate()))
        } else {
            (rates(&inputs), device.default_input_config().ok().map(|c| c.sample_rate()))
        };
        let buffer = outputs
            .iter()
            .chain(inputs.iter())
            .find_map(|c| match c.buffer_size() {
                cpal::SupportedBufferSize::Range { min, max } => Some((*min, *max)),
                cpal::SupportedBufferSize::Unknown => None,
            });
        out.push(Info {
            default_input: input_channels > 0 && default_input.as_deref() == Some(name.as_str()),
            default_output: output_channels > 0 && default_output.as_deref() == Some(name.as_str()),
            name,
            input_channels,
            output_channels,
            sample_rates: ranged,
            current_rate: current,
            buffer_frames: buffer,
        });
    }
    Ok(out)
}

/// The output device whose name contains `name`, ignoring case: the first
/// listed, as a scripted run names a virtual device by part of its name.
pub fn find_output(name: &str) -> Result<Option<cpal::Device>, String> {
    let wanted = name.trim().to_lowercase();
    if wanted.is_empty() {
        return Ok(None);
    }
    let host = cpal::default_host();
    Ok(host
        .output_devices()
        .map_err(|e| e.to_string())?
        .find(|d| name_of(d).is_some_and(|n| n.to_lowercase().contains(&wanted))))
}

/// The full name of the output device whose name contains `name`.
pub fn output_name(name: &str) -> Result<Option<String>, String> {
    Ok(find_output(name)?.and_then(|d| name_of(&d)))
}

/// The name of the system's default output device.
pub fn default_output_name() -> Option<String> {
    cpal::default_host().default_output_device().and_then(|d| name_of(&d))
}

static GENERATION: AtomicU64 = AtomicU64::new(0);
static WATCHING: OnceLock<bool> = OnceLock::new();

/// How many times the default devices or the list of devices has changed
/// since `watch` began; the same number means nothing has.
pub fn generation() -> u64 {
    GENERATION.load(Relaxed)
}

/// Begins counting changes of the default input and output devices and of
/// the devices present, once for the process. Harmless to call again.
pub fn watch() {
    WATCHING.get_or_init(|| {
        #[cfg(target_os = "macos")]
        {
            use objc2_core_audio::{
                kAudioHardwarePropertyDefaultInputDevice, kAudioHardwarePropertyDefaultOutputDevice,
                kAudioHardwarePropertyDevices, kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
                kAudioObjectSystemObject, AudioObjectAddPropertyListener, AudioObjectID, AudioObjectPropertyAddress,
            };
            use std::ptr::NonNull;
            unsafe extern "C-unwind" fn changed(
                _object: AudioObjectID,
                _count: u32,
                _addresses: NonNull<AudioObjectPropertyAddress>,
                _client: *mut std::ffi::c_void,
            ) -> i32 {
                GENERATION.fetch_add(1, Relaxed);
                0
            }
            for selector in [kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyDefaultInputDevice, kAudioHardwarePropertyDevices] {
                let address = AudioObjectPropertyAddress {
                    mSelector: selector,
                    mScope: kAudioObjectPropertyScopeGlobal,
                    mElement: kAudioObjectPropertyElementMain,
                };
                // The address is copied by CoreAudio; the listener keeps nothing.
                let _ = unsafe {
                    AudioObjectAddPropertyListener(kAudioObjectSystemObject as AudioObjectID, NonNull::from(&address), Some(changed), std::ptr::null_mut())
                };
            }
        }
        true
    });
}
