//! Recording: a stem or the mix tapped in the player equals the render's
//! stem frame for frame; a count-in holds the song and clicks, then rolls;
//! and, through BlackHole where the Mac has it, a hit played through the
//! output comes back through the input at its timeline frame by the anchor.

mod common;

use aaw_engine::capture::{feed, instant_ns, open_input, ChunkTime, Tap, TapSource};
use aaw_engine::player::{channel, Player};
use aaw_engine::program::{Program, StemKind};
use aaw_engine::realtime::{Settings, Transport};
use aaw_engine::render::{Frame, Renderer};
use common::{program, write_mix_song, write_sample};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// `n` frames from the player in 128-frame callbacks.
fn pull(player: &mut Player, n: usize) -> Vec<Frame> {
    let mut out = vec![[0.0; 2]; n];
    for chunk in out.chunks_mut(128) {
        player.render(chunk);
    }
    out
}

/// The render's stems by timeline frame, each with the master's gain and
/// end fade as the render writes them, and the mix.
fn rendered(p: &Arc<Program>) -> (Vec<Vec<Option<[f32; 2]>>>, Vec<Frame>) {
    let names = p.stems();
    let total = p.total as i64;
    let mut stems: Vec<Vec<Option<[f32; 2]>>> = vec![vec![None; p.total]; names.len()];
    let mut r = Renderer::new(p.clone(), 0, 4096);
    let mut mix = Vec::new();
    let mut block = vec![[0.0; 2]; 4096];
    while !r.finished() {
        let n = 4096.min(p.total + p.latency - r.position());
        r.render(&mut block[..n], |index, post, at| {
            for (k, f) in post.iter().enumerate() {
                let frame = at + k as i64;
                if frame < 0 || frame >= total {
                    continue;
                }
                let env = p.envelope(frame as usize);
                stems[index][frame as usize] = Some([(f[0] * env) as f32, (f[1] * env) as f32]);
            }
        });
        mix.extend_from_slice(&block[..n]);
    }
    (stems, mix)
}

#[test]
fn a_tapped_stem_and_the_mix_equal_the_render() {
    let dir = tempfile::tempdir().unwrap();
    let p = program(&write_mix_song(dir.path()), |_| {});
    let (stems, mix) = rendered(&p);
    let bass = p.stem_index(StemKind::Track, "bass").expect("bass is heard");
    let (mut control, mut player) = channel(p.clone());
    let (bass_feed, mut bass_capture) = feed(2, p.rate, 60.0);
    let (mix_feed, mut mix_capture) = feed(2, p.rate, 60.0);
    control
        .tap(Box::new(Tap {
            source: TapSource::Stem {
                kind: StemKind::Track,
                id: "bass".into(),
            },
            feed: bass_feed,
        }))
        .unwrap();
    control
        .tap(Box::new(Tap {
            source: TapSource::Master,
            feed: mix_feed,
        }))
        .unwrap();
    // Nothing is tapped while the transport stands still.
    pull(&mut player, 1024);
    let mut samples = Vec::new();
    assert!(bass_capture.drain(&mut samples).is_empty() && samples.is_empty());
    control.play(0).unwrap();
    let n = p.total + p.latency;
    pull(&mut player, n);
    control.untap().unwrap();
    pull(&mut player, 128);
    assert_eq!(control.collect_taps().len(), 2, "the taps come back to the host");

    let mut samples = Vec::new();
    let chunks = bass_capture.drain(&mut samples);
    assert!(!chunks.is_empty());
    let mut compared = 0;
    let mut offset = 0;
    for (time, frames) in &chunks {
        let ChunkTime::Frame(at) = time else { panic!("a tap knows its frame") };
        for k in 0..*frames {
            let frame = at + k as i64;
            let got = [samples[(offset + k) * 2], samples[(offset + k) * 2 + 1]];
            if frame >= 0 && (frame as usize) < p.total {
                let want = stems[bass][frame as usize].expect("rendered");
                assert_eq!(got, want, "bass stem at frame {frame}");
                compared += 1;
            }
        }
        offset += frames;
    }
    assert!(compared >= p.total - 4096, "{compared} of {} frames compared", p.total);
    assert_eq!(bass_capture.lost(), 0);

    // The mix, from the first heard frame.
    let mut samples = Vec::new();
    let chunks = mix_capture.drain(&mut samples);
    let mut offset = 0;
    let mut compared = 0;
    for (time, frames) in &chunks {
        let ChunkTime::Frame(at) = time else { panic!("a tap knows its frame") };
        for k in 0..*frames {
            let frame = at + k as i64;
            // The stream's frame `i` is heard at timeline frame `i - latency`.
            let i = frame + p.latency as i64;
            if (0..mix.len() as i64).contains(&i) {
                let want = mix[i as usize];
                let got = [samples[(offset + k) * 2], samples[(offset + k) * 2 + 1]];
                assert_eq!(got, [want[0] as f32, want[1] as f32], "mix at frame {frame}");
                compared += 1;
            }
        }
        offset += frames;
    }
    assert!(compared > 0);
}

#[test]
fn a_count_in_holds_the_song_and_clicks_then_rolls() {
    let dir = tempfile::tempdir().unwrap();
    let p = program(&write_mix_song(dir.path()), |_| {});
    let (mut control, mut player) = channel(p.clone());
    let bar = (p.rate as f64 * 60.0 / p.tempo * p.click_beats * p.beats_per_bar as f64).round() as usize;
    control.play_after(0, bar).unwrap();
    // The anchor: the first frame of this block plays at `ns`, and the
    // timeline is a bar before the start.
    let mut block = vec![[0.0; 2]; 128];
    player.render_at(&mut block, Some(1_000_000));
    assert_eq!(control.shared().anchor(), Some((1_000_000, -(bar as i64) - p.latency as i64)));
    let mut heard = block.clone();
    heard.extend(pull(&mut player, bar - 128));
    // The song stands at the start and the metronome counts.
    assert_eq!(control.position(), 0);
    assert!(control.playing());
    let peak = heard.iter().fold(0.0f64, |m, f| m.max(f[0].abs()));
    assert!(peak > 0.05, "the count-in clicks: peak {peak}");
    // One click a beat: a sound after at least a quarter beat of silence.
    let quiet = (p.rate as f64 * 60.0 / p.tempo / 4.0) as usize;
    let mut clicks = 0;
    let mut silent = quiet;
    for f in &heard {
        if f[0].abs() < 1e-4 {
            silent += 1;
        } else {
            if silent >= quiet {
                clicks += 1;
            }
            silent = 0;
        }
    }
    assert_eq!(clicks, p.beats_per_bar as usize, "clicks in a bar of {}", p.beats_per_bar);
    // Then the song rolls, as a plain render does, with no click.
    let n = 4 * bar;
    let rolled = pull(&mut player, n);
    let mut r = Renderer::new(p.clone(), 0, 4096);
    let mut plain = vec![[0.0; 2]; n];
    for chunk in plain.chunks_mut(4096) {
        r.render(chunk, |_, _, _| {});
    }
    // The strips settled during the count-in, so the first frames play in
    // full where a render from a stop fades them in over 5 ms; after that
    // the bars are the render's.
    let settled = 2 * (aaw_engine::player::FADE_SECONDS * p.rate as f64) as usize;
    let diff = rolled.iter().zip(&plain).skip(settled).map(|(a, b)| (a[0] - b[0]).abs().max((a[1] - b[1]).abs())).fold(0.0, f64::max);
    assert!(diff < 1e-9, "the take's bars equal the render: {diff}");
    let first = rolled.iter().zip(&plain).take(settled).map(|(a, b)| (a[0] - b[0]).abs()).fold(0.0, f64::max);
    assert!(first < 0.01, "the first frames differ by the fade alone: {first}");
    assert!(control.position() > 0);
    // A stop during a count-in ends it.
    control.play_after(0, bar).unwrap();
    pull(&mut player, 256);
    control.stop().unwrap();
    pull(&mut player, 256);
    assert!(!control.playing());
}

const DEVICE: &str = "BlackHole";

fn blackhole() -> Option<cpal::Device> {
    match aaw_engine::devices::find_output(DEVICE) {
        Ok(Some(d)) => Some(d),
        _ => {
            eprintln!("no {DEVICE} device: nothing is recorded");
            None
        }
    }
}

/// A song of silence with one noise burst at beat 2, at `rate`.
fn burst_song(dir: &Path, rate: u32) -> Arc<Program> {
    let burst: Vec<[f32; 2]> = common::noise(rate as usize / 10, 5).iter().map(|x| [(*x * 2.0) as f32; 2]).collect();
    write_sample(dir, "burst.wav", burst, rate);
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
    program(&path, |_| {})
}

/// Through BlackHole, a burst played at beat 2 comes back through the
/// input and, mapped by the anchor, lands within a millisecond of beat 2.
#[test]
fn a_hit_recorded_through_the_loopback_lands_on_its_beat() {
    if blackhole().is_none() {
        return;
    }
    let rate = 48000;
    let dir = tempfile::tempdir().unwrap();
    let song = burst_song(dir.path(), rate);
    let settings = Settings {
        output: Some(DEVICE.into()),
        buffer: 256,
        device_rate: None,
    };
    let mut transport = Transport::open(song.clone(), &settings).unwrap();
    assert_eq!(transport.device_rate, rate);
    std::thread::sleep(Duration::from_millis(300));
    let (stream, mut capture) = open_input(Some(DEVICE), &[1], rate, 8.0).unwrap();
    assert_eq!(stream.rate, rate);
    std::thread::sleep(Duration::from_millis(200));
    transport.control.play(0).unwrap();
    std::thread::sleep(Duration::from_millis(1800));
    transport.control.stop().unwrap();
    let anchor = transport.control.shared().anchor().expect("the output anchors");
    let mut samples = Vec::new();
    let chunks = capture.drain(&mut samples);
    assert!(stream.callbacks() > 0 && !chunks.is_empty());
    assert_eq!(capture.lost(), 0);
    transport.close();
    let (ChunkTime::Ns(first_ns), _) = chunks[0] else { panic!("an input knows its instant") };
    let (anchor_ns, anchor_frame) = anchor;
    let first_frame = anchor_frame as f64 + (first_ns as f64 - anchor_ns as f64) * rate as f64 / 1e9;
    let onset = samples.iter().position(|x| x.abs() > 0.05).expect("the burst came back");
    let landed = first_frame + onset as f64;
    let expected = rate as f64; // beat 2 at 120 BPM
    let error_ms = (landed - expected) / rate as f64 * 1000.0;
    eprintln!("the burst landed {error_ms:+.3} ms from its beat (first frame {first_frame:.1}, onset {onset})");
    assert!(error_ms.abs() < 1.0, "the burst landed {error_ms:+.2} ms from its beat");
    let _ = instant_ns;
}
