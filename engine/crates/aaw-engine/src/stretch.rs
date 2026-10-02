//! Time stretching that keeps pitch: a pad's audio made longer or shorter by a
//! ratio, for a song played at another tempo than its own.
//!
//! It runs where repitching does, when a pad's audio is prepared, so nothing
//! stretches while audio plays and playback equals the render. Signalsmith
//! Stretch is built in. Rubber Band is run as the installed `rubberband`
//! program, which keeps its license out of this binary.

use crate::sndfile;
use aaw_model::Stretcher;
use std::process::Command;

/// The version of the built-in stretcher, for render reports.
pub const SIGNALSMITH: &str = "signalsmith-stretch 0.1.3";

/// The audio either side of a region that is stretched with it and then cut
/// off, so the stretcher's start and end are not in what is heard.
pub const MARGIN_SECONDS: f64 = 0.2;

/// Interleaved audio played `ratio` times as fast at its own pitch:
/// `frames / ratio` frames, rounded.
pub fn stretch(data: &[f64], channels: usize, rate: u32, ratio: f64, with: Stretcher) -> Result<Vec<f64>, String> {
    let frames = data.len() / channels.max(1);
    let wanted = (frames as f64 / ratio).round() as usize;
    if frames == 0 || wanted == 0 {
        return Ok(Vec::new());
    }
    let mut out = match with {
        Stretcher::Signalsmith => signalsmith(data, channels, rate, wanted)?,
        Stretcher::Rubberband => rubberband(data, channels, rate, ratio)?,
    };
    out.resize(wanted * channels, 0.0);
    Ok(out)
}

fn signalsmith(data: &[f64], channels: usize, rate: u32, wanted: usize) -> Result<Vec<f64>, String> {
    let input: Vec<f32> = data.iter().map(|&x| x as f32).collect();
    let mut output = vec![0f32; wanted * channels];
    let mut stretcher = signalsmith_stretch::Stretch::preset_default(channels as u32, rate);
    if !stretcher.exact(&input, &mut output) {
        return Err(format!(
            "{:.0} ms is too short to stretch; use stretch: repitch",
            1000.0 * (data.len() / channels) as f64 / rate as f64
        ));
    }
    Ok(output.into_iter().map(f64::from).collect())
}

/// A 32-bit float WAV of interleaved audio, for the program to read.
fn wav(data: &[f64], channels: usize, rate: u32) -> Vec<u8> {
    let size = (data.len() * 4) as u32;
    let mut out = Vec::with_capacity(44 + data.len() * 4);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + size).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&(channels as u16).to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * channels as u32 * 4).to_le_bytes());
    out.extend_from_slice(&((channels * 4) as u16).to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&size.to_le_bytes());
    for &x in data {
        out.extend_from_slice(&(x as f32).to_le_bytes());
    }
    out
}

fn rubberband(data: &[f64], channels: usize, rate: u32, ratio: f64) -> Result<Vec<f64>, String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (source, target) = (dir.path().join("in.wav"), dir.path().join("out.wav"));
    std::fs::write(&source, wav(data, channels, rate)).map_err(|e| e.to_string())?;
    let out = Command::new("rubberband")
        .args(["--quiet", "--fine", "--tempo", &format!("{ratio:.17}")])
        .arg(&source)
        .arg(&target)
        .output()
        .map_err(|_| MISSING.to_string())?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        return Err(format!("rubberband failed: {}", said.lines().last().unwrap_or("no message")));
    }
    let audio = sndfile::read(&target)?;
    if audio.channels != channels {
        return Err(format!("rubberband gave {} channels for {channels}", audio.channels));
    }
    Ok(audio.data)
}

const MISSING: &str = "session.stretcher is rubberband, and the rubberband program was not found; install it (brew install rubberband) or set session.stretcher to signalsmith";

/// The installed Rubber Band's version, such as `rubberband 4.0.0`, for render
/// reports; an error when the program is not there.
pub fn rubberband_version() -> Result<String, String> {
    let out = Command::new("rubberband").arg("--version").output().map_err(|_| MISSING.to_string())?;
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    Ok(format!("rubberband {}", said.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    const RATE: u32 = 44100;

    /// A stereo tone with a click every half second.
    fn song(seconds: f64) -> Vec<f64> {
        let frames = (seconds * RATE as f64) as usize;
        let mut data = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let tone = 0.3 * (TAU * 440.0 * i as f64 / RATE as f64).sin();
            let click = if i % (RATE as usize / 2) < 40 { 0.5 } else { 0.0 };
            data.extend([tone + click, 0.5 * (tone + click)]);
        }
        data
    }

    /// Zero crossings a second of the left channel: twice a tone's frequency.
    fn crossings(data: &[f64]) -> f64 {
        let left: Vec<f64> = data.iter().step_by(2).copied().collect();
        let count = left.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        count as f64 / (left.len() as f64 / RATE as f64)
    }

    fn check(with: Stretcher) {
        let data = song(4.0);
        for ratio in [1.04, 0.96] {
            let out = stretch(&data, 2, RATE, ratio, with).unwrap();
            assert_eq!(out.len() / 2, (4.0 * RATE as f64 / ratio).round() as usize);
            // The pitch is the tone's, where repitching would move it by the
            // ratio, to 458 or 422 Hz. The clicks add a few crossings.
            let tone = crossings(&out);
            assert!((tone - 440.0).abs() < 6.0, "{with:?} at {ratio}: {tone} Hz");
            // The second channel is still half the first.
            let (l, r): (f64, f64) = out.chunks(2).fold((0.0, 0.0), |(l, r), f| (l + f[0] * f[0], r + f[1] * f[1]));
            assert!((r / l - 0.25).abs() < 0.02, "{with:?}: {}", r / l);
            assert_eq!(stretch(&data, 2, RATE, ratio, with).unwrap(), out, "the same audio stretches the same");
        }
    }

    #[test]
    fn signalsmith_keeps_pitch_and_gives_the_length_asked() {
        check(Stretcher::Signalsmith);
        let short = song(0.02);
        assert!(stretch(&short, 2, RATE, 1.04, Stretcher::Signalsmith).unwrap_err().contains("too short"));
        assert!(stretch(&[], 2, RATE, 1.04, Stretcher::Signalsmith).unwrap().is_empty());
    }

    #[test]
    fn rubberband_does_the_same_where_it_is_installed() {
        if rubberband_version().is_ok() {
            check(Stretcher::Rubberband);
        }
    }
}
