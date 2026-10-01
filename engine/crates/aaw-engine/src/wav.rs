//! WAV files whose bytes depend only on the audio, and are those the first
//! (Python) engine wrote for it.
//!
//! libsndfile timestamps the PEAK chunk of float WAVs, so equal audio could hash
//! unequally. Float stems have the layout it writes with that chunk switched
//! off, zeroed PAD chunk included. The 24-bit mix follows the libsndfile that
//! Python's `soundfile` bundles; libsndfile builds differ in how they round, so
//! the conversion is done here.

use std::io::Write;
use std::path::Path;

fn chunk(name: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(name);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// The file for stereo frames already converted to float32, as `astype("<f4")`.
pub fn float_wav_bytes(frames: &[[f32; 2]], rate: u32) -> Vec<u8> {
    let channels: u16 = 2;
    let size = frames.len() as u32 * channels as u32 * 4;
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&3u16.to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&rate.to_le_bytes());
    fmt.extend_from_slice(&(rate * channels as u32 * 4).to_le_bytes());
    fmt.extend_from_slice(&(channels * 4).to_le_bytes());
    fmt.extend_from_slice(&32u16.to_le_bytes());
    let mut header = chunk(b"fmt ", &fmt);
    header.extend(chunk(b"fact", &(frames.len() as u32).to_le_bytes()));
    header.extend(chunk(b"PAD ", &vec![0u8; 8 + 8 * channels as usize]));
    let mut out = Vec::with_capacity(12 + header.len() + 8 + size as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(4 + header.len() as u32 + 8 + size).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend(header);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&size.to_le_bytes());
    for f in frames {
        out.extend_from_slice(&f[0].to_le_bytes());
        out.extend_from_slice(&f[1].to_le_bytes());
    }
    out
}

/// A 24-bit PCM stereo WAV of samples in (-1, 1), as `soundfile.write(...,
/// "PCM_24")` writes it: each sample is `lrint(x * 2**31) >> 8`.
pub fn pcm24_wav_bytes(frames: &[[f64; 2]], rate: u32) -> Vec<u8> {
    let channels: u16 = 2;
    let size = frames.len() as u32 * channels as u32 * 3;
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&rate.to_le_bytes());
    fmt.extend_from_slice(&(rate * channels as u32 * 3).to_le_bytes());
    fmt.extend_from_slice(&(channels * 3).to_le_bytes());
    fmt.extend_from_slice(&24u16.to_le_bytes());
    let header = chunk(b"fmt ", &fmt);
    let mut out = Vec::with_capacity(12 + header.len() + 8 + size as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(4 + header.len() as u32 + 8 + size).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend(header);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&size.to_le_bytes());
    for f in frames {
        for x in f {
            // C's conversion of the long to int wraps, as libsndfile's does.
            let value = ((x * 2147483648.0).round_ties_even() as i64 as i32) >> 8;
            out.extend_from_slice(&value.to_le_bytes()[..3]);
        }
    }
    out
}

/// Writes through a temporary file in the same directory, then renames it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut temp = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile_in(dir)
        .map_err(|e| e.to_string())?;
    temp.write_all(bytes).map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn float_stems_keep_the_bytes_of_earlier_renders() {
        // The hash of the libsndfile writer the first engine used: equal audio
        // keeps equal stem bytes across writes and engine versions.
        let frames: Vec<[f32; 2]> = (0..70001)
            .map(|i| [2 * i, 2 * i + 1].map(|n| (n as f64 / 70001.0 - 1.0) as f32))
            .collect();
        let bytes = float_wav_bytes(&frames, 48000);
        let hash: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hash, "c2554ec4d5acecc5494a73b6bcae7d474a4e6f1e3f0af74f31f4c7bd3e5281e0");
        assert!(!bytes.windows(4).any(|w| w == b"PEAK"));
    }
}
