//! A sound captured from an input with no track, the agent panel's Record
//! Audio: what `capture.start` and `capture.stop` refuse without a device,
//! and, through BlackHole where the Mac has it, a sound captured while the
//! song plays landing on the beat it was heard against.

mod common;

use aaw_host::client::Request;
use aaw_host::command::Origin;
use aaw_host::host::{self, Options, Running};
use common::{cmd, write_song};
use serde_json::{json, Value as Json};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

const DEVICE: &str = "BlackHole";

fn registry_dir() {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::Builder::new().prefix("aaw").tempdir_in("/tmp").unwrap();
        // SAFETY: set once, before any test reads it; every test calls this first.
        unsafe { std::env::set_var("AAW_HOST_DIR", dir.path()) };
        dir
    });
}

fn spawn(path: &Path, audio: host::Audio) -> Running {
    host::spawn(
        path,
        Options {
            audio,
            play: None,
            exit_on_stop: false,
            seconds: None,
            feed: false,
            prepare: true,
            pace: host::PACE,
        },
        |_| {},
    )
    .unwrap()
}

#[test]
fn a_capture_is_refused_without_its_input_or_with_its_file_taken() {
    registry_dir();
    let dir = tempfile::tempdir().unwrap();
    let path = write_song(dir.path());
    let running = spawn(&path, host::Audio::default());
    let ask = |j: Json| running.request(Request::new(cmd(j), Origin::User));

    assert_eq!(ask(json!({"op": "status"})).unwrap()["capture"], Json::Null);
    let e = ask(json!({"op": "capture.stop"})).unwrap_err();
    assert!(e.contains("Nothing is being captured"), "{e}");
    let e = ask(json!({"op": "capture.start", "file": "hum.wav", "input": "no-such-device-xyz"})).unwrap_err();
    assert!(e.contains("No audio input device named no-such-device-xyz"), "{e}");
    assert!(!dir.path().join("hum.wav").exists());
    // The file is checked before the device.
    std::fs::write(dir.path().join("taken.wav"), b"").unwrap();
    let e = ask(json!({"op": "capture.start", "file": "taken.wav", "input": "no-such-device-xyz"})).unwrap_err();
    assert!(e.contains("taken.wav exists"), "{e}");
    let e = ask(json!({"op": "capture.start", "file": "hum.wav", "input": "no-such-device-xyz", "channels": [0]})).unwrap_err();
    assert!(e.contains("counted from 1"), "{e}");
    let e = ask(json!({"op": "capture.start", "file": "", "input": "no-such-device-xyz"})).unwrap_err();
    assert!(e.contains("file names"), "{e}");
    assert_eq!(ask(json!({"op": "status"})).unwrap()["capture"], Json::Null);
    running.close().unwrap();
}

/// A song of silence with a noise burst at beat 2, at 120 BPM.
fn burst_song(dir: &Path, rate: u32) -> std::path::PathBuf {
    let mut seed = 5u64;
    let burst: Vec<[f32; 2]> = (0..rate as usize / 10)
        .map(|_| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let x = ((seed >> 11) as f64 / (1u64 << 53) as f64 * 1.6 - 0.8) as f32;
            [x; 2]
        })
        .collect();
    std::fs::write(dir.join("burst.wav"), aaw_engine::wav::float_wav_bytes(&burst, rate)).unwrap();
    let yaml = format!(
        r#"
session: {{tempo: 120, sample_rate: {rate}, length_beats: 8, master_gain_db: 0}}
samples:
  burst: {{path: burst.wav}}
patterns:
  one: {{length_beats: 4, events: [{{at: 2, pad: b}}]}}
tracks:
- id: hit
  pads: {{b: {{sample: burst}}}}
  clips: [{{pattern: one}}]
"#
    );
    let path = dir.join("song.yaml");
    std::fs::write(&path, yaml).unwrap();
    path
}

/// Through BlackHole as the output and the input: a sound captured while
/// the song plays has the burst where `at_beat` says beat 2 is, within a
/// millisecond; a discarded capture writes nothing.
#[test]
fn a_sound_captured_while_the_song_plays_knows_the_beat_it_began_at() {
    registry_dir();
    match aaw_engine::devices::find_input(DEVICE) {
        Ok(Some(_)) => {}
        _ => {
            eprintln!("no {DEVICE} device: nothing is captured");
            return;
        }
    }
    let rate = 48000;
    let dir = tempfile::tempdir().unwrap();
    let path = burst_song(dir.path(), rate);
    let audio = host::Audio {
        output: Some(DEVICE.into()),
        input: Some(DEVICE.into()),
        strict_output: true,
        ..host::Audio::default()
    };
    let running = spawn(&path, audio);
    let ask = |j: Json| running.request(Request::new(cmd(j), Origin::User));

    // Stopped: the sound has no beat.
    ask(json!({"op": "capture.start", "file": ".daw/attachments/quiet.wav"})).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    let status = ask(json!({"op": "status"})).unwrap();
    assert!(status["capture"]["seconds"].as_f64().unwrap() > 0.2, "{}", status["capture"]);
    assert_eq!(status["capture"]["at_beat"], Json::Null);
    let e = ask(json!({"op": "capture.start", "file": ".daw/attachments/other.wav"})).unwrap_err();
    assert!(e.contains("Already capturing"), "{e}");
    let reply = ask(json!({"op": "capture.stop"})).unwrap();
    let quiet = &reply["captured"];
    assert_eq!(quiet["file"], json!(".daw/attachments/quiet.wav"));
    assert_eq!(quiet["at_beat"], Json::Null);
    assert_eq!((quiet["channels"].clone(), quiet["sample_rate"].clone()), (json!(1), json!(rate)));
    assert!(dir.path().join(".daw/attachments/quiet.wav").exists());
    assert_eq!(reply["capture"], Json::Null);

    // Playing from the start: the burst at beat 2 lands a second after the
    // beat the first frame was heard against.
    ask(json!({"op": "play"})).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    ask(json!({"op": "capture.start", "file": "hum.wav"})).unwrap();
    std::thread::sleep(Duration::from_millis(1800));
    let reply = ask(json!({"op": "capture.stop"})).unwrap();
    ask(json!({"op": "stop"})).unwrap();
    let captured = &reply["captured"];
    let at_beat = captured["at_beat"].as_f64().expect("the beat the sound began at");
    assert!(at_beat > 0.0 && at_beat < 1.5, "{captured}");
    assert_eq!(captured["lost_frames"], json!(0));
    assert!(captured["seconds"].as_f64().unwrap() > 1.5, "{captured}");
    let audio = aaw_engine::sndfile::read(&dir.path().join("hum.wav")).unwrap();
    assert_eq!((audio.channels, audio.rate), (1, rate));
    let onset = audio.data.iter().position(|x| x.abs() > 0.05).expect("the burst came back");
    let landed = at_beat * 60.0 / 120.0 * rate as f64 + onset as f64;
    let error_ms = (landed - rate as f64) / rate as f64 * 1000.0;
    assert!(error_ms.abs() < 1.0, "the burst landed {error_ms:+.2} ms from beat 2 (pulled {} ms)", captured["pulled_ms"]);

    // Discarded: nothing written.
    ask(json!({"op": "capture.start", "file": "dropped.wav"})).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let reply = ask(json!({"op": "capture.stop", "discard": true})).unwrap();
    assert_eq!(reply["captured"], Json::Null);
    assert!(!dir.path().join("dropped.wav").exists());
    running.close().unwrap();
}
