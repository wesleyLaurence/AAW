//! The output opened on a device: a song at 44.1 kHz through a device run at
//! 48 kHz, resampled in the callback, and at 44.1 kHz unchanged. Needs a
//! virtual device nobody hears, BlackHole, whose input gives back what is
//! played into it; without one the tests say so and pass.

mod common;

use aaw_engine::devices;
use aaw_engine::program::Program;
use aaw_engine::realtime::{Settings, Transport};
use common::{program, write_sample};
use cpal::traits::{DeviceTrait, StreamTrait};
use std::f64::consts::PI;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const DEVICE: &str = "BlackHole";
/// One test at a time on the device: each sets its rate.
static DEVICE_IN_USE: Mutex<()> = Mutex::new(());

fn blackhole() -> Option<cpal::Device> {
    match devices::find_output(DEVICE) {
        Ok(Some(d)) => Some(d),
        _ => {
            eprintln!("no {DEVICE} device: the output is not opened");
            None
        }
    }
}

/// A song of one steady 1 kHz tone at -12 dBFS, made at `rate`.
fn tone_song(dir: &Path, rate: u32) -> Arc<Program> {
    let tone: Vec<[f32; 2]> = (0..4 * rate as usize)
        .map(|i| [(0.25 * (2.0 * PI * 1000.0 * i as f64 / rate as f64).sin()) as f32; 2])
        .collect();
    write_sample(dir, "tone.wav", tone, rate);
    let yaml = format!(
        r#"
session: {{tempo: 120, sample_rate: {rate}, length_beats: 8, master_gain_db: 0}}
samples:
  tone: {{path: tone.wav}}
patterns:
  hold: {{length_beats: 8, events: [{{at: 0, pad: t, duration: 8}}]}}
tracks:
- id: tone
  pads: {{t: {{sample: tone, mode: gate, attack_ms: 1, release_ms: 20}}}}
  clips: [{{pattern: hold}}]
"#
    );
    let path = dir.join("song.yaml");
    std::fs::write(&path, yaml).unwrap();
    program(&path, |_| {})
}

/// Records the device's input at `rate` while `run` runs, as interleaved
/// stereo frames.
fn recorded(device: &cpal::Device, rate: u32, run: impl FnOnce()) -> Vec<f32> {
    let taken = Arc::new(Mutex::new(Vec::<f32>::new()));
    let sink = taken.clone();
    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: rate,
        buffer_size: cpal::BufferSize::Default,
    };
    let stream = device
        .build_input_stream::<f32, _, _>(config, move |data: &[f32], _| sink.lock().unwrap().extend_from_slice(data), |e| panic!("{e}"), None)
        .unwrap();
    stream.play().unwrap();
    run();
    drop(stream);
    let out = taken.lock().unwrap().clone();
    out
}

/// The tone's frequency by its zero crossings and its RMS over `frames`
/// of the left channel.
fn measure(interleaved: &[f32], rate: u32, from: usize, frames: usize) -> (f64, f64) {
    let left: Vec<f64> = interleaved.iter().step_by(2).skip(from).take(frames).map(|x| *x as f64).collect();
    assert!(left.len() == frames, "only {} frames recorded", left.len() + from);
    let crossings = left.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    let hz = crossings as f64 * rate as f64 / frames as f64;
    let rms = (left.iter().map(|x| x * x).sum::<f64>() / frames as f64).sqrt();
    (hz, rms)
}

/// Plays the tone song made at `song_rate` into the device for a second
/// and a half: the rate the device was opened at, whether the song was
/// resampled, and what the device gave back.
fn plays(dir: &Path, song_rate: u32, device_rate: Option<u32>) -> (u32, bool, Vec<f32>) {
    let device = blackhole().expect("checked");
    let song = tone_song(dir, song_rate);
    let settings = Settings {
        output: Some(DEVICE.into()),
        buffer: 256,
        device_rate,
    };
    let mut transport = Transport::open(song, &settings).unwrap();
    let opened = transport.device_rate;
    let resampled = transport.resampled();
    // The device settles at the rate the output set before its input opens.
    std::thread::sleep(Duration::from_millis(300));
    let audio = recorded(&device, opened, || {
        transport.control.play(0).unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        transport.control.stop().unwrap();
        std::thread::sleep(Duration::from_millis(100));
    });
    transport.close();
    std::thread::sleep(Duration::from_millis(300));
    (opened, resampled, audio)
}

/// The same song at 44.1 kHz played at its own rate, which the device
/// offers, and through the device opened at 48 kHz, as one that offers only
/// 48 kHz would be: the tone comes back at its pitch and level either way.
/// The device's own output volume scales both alike, so the two are held
/// to each other rather than to the render.
#[test]
fn a_song_at_44_1_khz_plays_through_a_device_at_48_khz_as_at_its_own_rate() {
    if blackhole().is_none() {
        return;
    }
    let _turn = DEVICE_IN_USE.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (opened, resampled, direct) = plays(dir.path(), 44100, None);
    assert_eq!(opened, 44100);
    assert!(!resampled);
    // Half a second in, the gate is open and the tone steady.
    let (hz, level) = measure(&direct, 44100, 22050, 22050);
    assert!((hz - 1000.0).abs() < 3.0, "{hz} Hz at the song's rate");
    assert!(level > 0.01, "nothing came back from the device: {level}");

    let (opened, resampled, through) = plays(dir.path(), 44100, Some(48000));
    assert_eq!(opened, 48000);
    assert!(resampled);
    let (hz, rms) = measure(&through, 48000, 24000, 24000);
    assert!((hz - 1000.0).abs() < 3.0, "{hz} Hz through 48 kHz");
    assert!((rms - level).abs() < level * 0.03, "{rms} resampled against {level} direct");

    // A song at 48 kHz on the same device opens at 48 kHz unchanged.
    let song = tone_song(dir.path(), 48000);
    let t = Transport::open(
        song,
        &Settings {
            output: Some(DEVICE.into()),
            buffer: 256,
            device_rate: None,
        },
    )
    .unwrap();
    assert_eq!((t.device_rate, t.resampled()), (48000, false));
    t.close();
}

#[test]
fn the_devices_are_listed_with_their_rates() {
    let list = devices::list().unwrap();
    if blackhole().is_none() {
        return;
    }
    let d = list.iter().find(|d| d.name.contains(DEVICE)).expect("listed");
    assert!(d.output_channels >= 2 && d.input_channels >= 2);
    assert!(d.sample_rates.contains(&44100) && d.sample_rates.contains(&48000));
    assert!(d.current_rate.is_some());
    assert!(list.iter().filter(|d| d.default_output).count() <= 1);
    assert_eq!(devices::output_name("blackhole").unwrap().as_deref(), Some(d.name.as_str()));
    assert!(devices::output_name("no such device").unwrap().is_none());
}
