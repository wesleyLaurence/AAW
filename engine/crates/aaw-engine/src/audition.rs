//! `daw synth audition`: a few notes played through a track's Synth, and
//! with its chain its effects, rendered to a WAV the person can hear and
//! `daw listen` can measure, with the measurements an agent reads first.

use crate::measure::{loudness_lufs, spectral_centroid_hz};
use crate::offline::metrics;
use crate::program::compile;
use crate::render::{Frame, Renderer};
use crate::wav;
use aaw_model::rules::note_name;
use aaw_model::value::{dict, Value};
use aaw_model::{Beat, Note, NoteClip, Project};
use num_rational::BigRational;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct AuditionOptions {
    pub track: String,
    /// MIDI numbers, played one after another; middle C when empty.
    pub notes: Vec<i64>,
    pub velocity: i64,
    pub length_beats: f64,
    /// Whether the track's effects are heard too.
    pub track_chain: bool,
    /// Where to write the file; under the project's `renders/auditions`
    /// when None.
    pub output: Option<PathBuf>,
}

/// The song an audition renders: the track alone, with no sends, lanes or
/// master effects, one clip of the notes in a row, and the song as long as
/// they last plus the release and a beat.
fn song(p: &Project, opts: &AuditionOptions) -> Result<Project, String> {
    let t = p.track(&opts.track).ok_or_else(|| format!("Unknown track: {}", opts.track))?;
    let midi = t.midi.as_ref().ok_or_else(|| format!("{} is not a MIDI track", t.id))?;
    let synth = midi.synth().ok_or_else(|| format!("{} has no synth", t.id))?;
    if !(1..=127).contains(&opts.velocity) {
        return Err("velocity is 1 to 127".into());
    }
    if !(opts.length_beats > 0.0 && opts.length_beats <= 64.0) {
        return Err("length_beats is more than 0 and at most 64".into());
    }
    let notes: Vec<i64> = if opts.notes.is_empty() { vec![60] } else { opts.notes.clone() };
    let length = BigRational::from_float(opts.length_beats).ok_or("length_beats is not a number")?;
    let mut clip = NoteClip {
        id: "audition".into(),
        at: Beat::int(0),
        length_beats: Beat::Str(aaw_model::fraction_str(&(&length * BigRational::from_integer(notes.len().into())))),
        loop_beats: None,
        notes: Vec::new(),
    };
    for (i, pitch) in notes.iter().enumerate() {
        if !(0..=127).contains(pitch) {
            return Err(format!("note {pitch} is outside 0 to 127"));
        }
        clip.notes.push(Note {
            id: format!("n{}", i + 1),
            pitch: *pitch,
            at: Beat::Str(aaw_model::fraction_str(&(&length * BigRational::from_integer(i.into())))),
            duration: Beat::Str(aaw_model::fraction_str(&length)),
            velocity: opts.velocity,
        });
    }
    let release = synth.envelopes.get("amp").map_or(0.0, |e| e.release_ms);
    let tail = release / 1000.0 * p.session.tempo / 60.0 + 1.0;
    let total = opts.length_beats * notes.len() as f64 + tail;
    let mut track = t.clone();
    track.midi = Some(aaw_model::Midi {
        instrument: midi.instrument.clone(),
        clips: vec![clip],
    });
    track.sends.clear();
    track.automation.clear();
    track.mute = false;
    track.solo = false;
    // The audition is the track alone, outside any group.
    track.group = None;
    if !opts.track_chain {
        track.effects.clear();
    }
    let mut song = Project {
        session: p.session.clone(),
        samples: p.samples.clone(),
        patterns: Default::default(),
        tracks: vec![track],
        groups: Vec::new(),
        returns: Vec::new(),
        sections: Vec::new(),
        master: Default::default(),
    };
    song.session.length_beats = Beat::Float((total * 1000.0).ceil() / 1000.0);
    song.session.master_gain_db = 0.0;
    // Validation checks the song as a whole, and fills in what it refuses.
    Project::validate(&song.dump(false)).map_err(|e| e.to_string())
}

/// Renders the audition and writes the file; the reply says where it is
/// and what it measures.
pub fn audition(path: &Path, opts: &AuditionOptions) -> Result<Value, String> {
    let p = aaw_model::load(path, true).map_err(|e| e.to_string())?;
    let directory = path.parent().unwrap_or(Path::new("."));
    let song = song(&p, opts)?;
    let rate = song.session.sample_rate;
    let program = Arc::new(compile(&song, directory)?);
    let total = program.total + program.latency;
    let mut renderer = Renderer::new(program.clone(), 0, 4096);
    let mut mix: Vec<Frame> = Vec::with_capacity(total);
    let mut block = vec![[0.0; 2]; 4096];
    while !renderer.finished() {
        let n = 4096.min(total - renderer.position());
        let heard = renderer.heard();
        renderer.render(&mut block[..n], |_, _, _| {});
        let lo = (-heard).clamp(0, n as i64) as usize;
        let hi = (program.total as i64 - heard).clamp(0, n as i64) as usize;
        mix.extend_from_slice(&block[lo..hi]);
    }
    let peak = mix.iter().fold(0.0f64, |m, f| m.max(f[0].abs()).max(f[1].abs()));
    let notes: Vec<i64> = if opts.notes.is_empty() { vec![60] } else { opts.notes.clone() };
    let output = match &opts.output {
        Some(o) => o.clone(),
        None => {
            // Named for what was heard, so the same audition writes the same file.
            let synth = p.track(&opts.track).and_then(|t| t.midi.as_ref()).and_then(|m| m.synth()).map(|s| s.dump(false));
            let key = format!("{:?}|{:?}|{}|{}|{}", synth, notes, opts.velocity, opts.length_beats, opts.track_chain);
            let digest: String = Sha256::digest(key.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
            directory.join("renders").join("auditions").join(format!("{}-{}.wav", opts.track, &digest[..8]))
        }
    };
    // A loud patch is heard as it is, scaled to fit the file, and the reply says so.
    let scale = if peak >= 1.0 { 0.999 / peak } else { 1.0 };
    let written: Vec<Frame> = mix.iter().map(|f| [f[0] * scale, f[1] * scale]).collect();
    wav::write_atomic(&output, &wav::pcm24_wav_bytes(&written, rate as u32))?;
    let resolved = std::fs::canonicalize(&output).unwrap_or(output);
    let measured = metrics(&mix, rate as u32);
    let db = |x: f64| 20.0 * x.max(1e-12).log10();
    let float = |x: Option<f64>| x.map_or(Value::None, Value::Float);
    Ok(dict(vec![
        ("path", Value::str(&resolved.to_string_lossy())),
        ("track", Value::str(&opts.track)),
        (
            "notes",
            Value::List(notes.iter().map(|n| Value::str(&note_name(*n).unwrap_or_default())).collect()),
        ),
        ("velocity", Value::int(opts.velocity)),
        ("length_beats", Value::Float(opts.length_beats)),
        ("track_chain", Value::Bool(opts.track_chain)),
        ("seconds", Value::Float(mix.len() as f64 / rate as f64)),
        ("peak_dbfs", Value::Float(db(peak))),
        ("estimated_true_peak_dbtp", measured.get("estimated_true_peak_dbtp").cloned().unwrap_or(Value::None)),
        ("rms_dbfs", measured.get("rms_dbfs").cloned().unwrap_or(Value::None)),
        ("loudness_lufs", float(loudness_lufs(&mix, rate as u32))),
        ("spectral_centroid_hz", float(spectral_centroid_hz(&mix, rate as u32))),
        ("scaled_db", Value::Float(db(scale))),
        ("frames", Value::int(mix.len() as i64)),
    ]))
}
