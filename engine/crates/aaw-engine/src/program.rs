//! A song compiled for playback: prepared sample buffers and every voice's
//! parameters, computed off the audio thread as the Python `Sampler` does.

use crate::schedule::{schedule, Trigger};
use crate::sndfile;
use aaw_dsp::resample::{repitch_ratio, resample_poly};
use aaw_model::rules::midi;
use aaw_model::{frame, Event, Pad, Project};
use std::collections::HashMap;
use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A pad's source after trim, reverse, downmix and repitch: interleaved frames.
#[derive(Debug)]
pub struct Prepared {
    pub channels: usize,
    pub data: Vec<f64>,
}

impl Prepared {
    pub fn frames(&self) -> usize {
        self.data.len() / self.channels
    }
}

/// One hit: a prepared buffer and the gains and envelope it plays with.
#[derive(Clone, Debug)]
pub struct Voice {
    pub start: i64,
    pub audio: Arc<Prepared>,
    /// `stereo_pan` gains for the pad: equal-power for mono, balance for stereo.
    pub pan: [f64; 2],
    pub gain: f64,
    pub velocity: f64,
    pub attack: usize,
    pub release: usize,
    pub length: usize,
}

impl Voice {
    /// The voice's output at its frame `i` for channel `c`, as `Voice.process`.
    #[inline]
    pub fn sample(&self, i: usize, c: usize) -> f64 {
        let p = if self.audio.channels == 1 {
            self.audio.data[i]
        } else {
            self.audio.data[i * 2 + c]
        };
        (p * self.pan[c] * self.gain * self.velocity) * self.envelope(i)
    }

    #[inline]
    fn envelope(&self, i: usize) -> f64 {
        let mut env = 1.0;
        if self.attack != 0 {
            env *= (i as f64 / self.attack as f64).min(1.0);
        }
        // Every natural sample ending fades. A gate releases at the note-off frame.
        let natural = self.release.min(self.length);
        if natural != 0 {
            let remaining = (self.length as f64 - 1.0 - i as f64) / natural.max(1) as f64;
            env *= remaining.max(0.0).min(1.0);
        }
        env
    }
}

/// `stereo_pan`'s gains for a pan position.
pub fn pan_gains(channels: usize, pan: f64) -> [f64; 2] {
    if channels == 1 {
        let angle = (pan + 1.0) * PI / 4.0;
        [angle.cos(), angle.sin()]
    } else {
        [(1.0 - pan).min(1.0), (1.0 + pan).min(1.0)]
    }
}

/// `10 ** (db / 20)`.
pub fn amplitude(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

#[derive(Debug)]
pub struct TrackProgram {
    pub id: String,
    /// Voices in trigger order.
    pub voices: Vec<Voice>,
    /// Balance gains of the track's stereo sum.
    pub pan: [f64; 2],
    pub gain: f64,
    /// Muted, or solo-muted.
    pub silent: bool,
}

#[derive(Debug)]
pub struct Program {
    pub rate: u32,
    /// The session length in frames.
    pub total: usize,
    /// Tracks in render order.
    pub tracks: Vec<TrackProgram>,
    pub master: f64,
    pub fade: usize,
    /// Features the engine does not process yet, which this program leaves out.
    pub omitted: Vec<String>,
}

impl Program {
    /// The end fade at a frame: `np.linspace(1, 0, fade)` over the last frames.
    #[inline]
    pub fn fader(&self, f: usize) -> f64 {
        let tail = self.total - self.fade;
        if f < tail {
            return 1.0;
        }
        let i = f - tail;
        if self.fade == 1 {
            1.0
        } else if i == self.fade - 1 {
            0.0
        } else {
            i as f64 * (-1.0 / (self.fade - 1) as f64) + 1.0
        }
    }

    /// The master gain and end fade at a frame.
    #[inline]
    pub fn envelope(&self, f: usize) -> f64 {
        self.fader(f) * self.master
    }
}

/// Loads and prepares pad audio, caching both, as `engine.Sampler`.
pub struct Sampler<'a> {
    project: &'a Project,
    directory: PathBuf,
    original: HashMap<String, Arc<sndfile::Audio>>,
    prepared: HashMap<(String, u64, Option<u64>, bool, bool, u64), Arc<Prepared>>,
}

impl<'a> Sampler<'a> {
    pub fn new(project: &'a Project, directory: &Path) -> Self {
        Sampler {
            project,
            directory: directory.to_path_buf(),
            original: HashMap::new(),
            prepared: HashMap::new(),
        }
    }

    fn original(&mut self, name: &str) -> Result<Arc<sndfile::Audio>, String> {
        if let Some(a) = self.original.get(name) {
            return Ok(a.clone());
        }
        let asset = &self.project.samples[name];
        let audio = sndfile::read(&self.directory.join(&asset.path))?;
        if !(audio.channels == 1 || audio.channels == 2)
            || audio.frames() == 0
            || !audio.data.iter().all(|x| x.is_finite())
        {
            return Err(format!("{name}: expected finite mono/stereo audio"));
        }
        let audio = Arc::new(audio);
        self.original.insert(name.to_string(), audio.clone());
        Ok(audio)
    }

    pub fn prepare(&mut self, pad: &Pad, event: &Event) -> Result<Arc<Prepared>, String> {
        let asset = &self.project.samples[&pad.sample];
        let x = self.original(&pad.sample)?;
        let mut semitones = pad.transpose + event.transpose;
        if let Some(note) = &event.note {
            let root = asset.root_note.as_deref().expect("validated root note");
            semitones += (midi(note)? - midi(root)?) as f64;
        }
        let mut speed = 2f64.powf(semitones / 12.0);
        if let Some(bpm) = pad.source_bpm {
            speed *= self.project.session.tempo / bpm;
        }
        let key = (
            pad.sample.clone(),
            pad.start_seconds.to_bits(),
            pad.end_seconds.map(f64::to_bits),
            pad.reverse,
            pad.mono,
            speed.to_bits(),
        );
        if let Some(p) = self.prepared.get(&key) {
            return Ok(p.clone());
        }
        let frames = x.frames();
        let source_rate = x.rate as f64;
        let start = (pad.start_seconds * source_rate).round_ties_even() as usize;
        let end = match pad.end_seconds {
            Some(e) => (e * source_rate).round_ties_even() as usize,
            None => frames,
        };
        if start >= frames || end > frames {
            return Err(format!("{}: trim outside sample", pad.sample));
        }
        let ch = x.channels;
        let mut rows: Vec<&[f64]> = (start..end.max(start)).map(|i| &x.data[i * ch..(i + 1) * ch]).collect();
        if pad.reverse {
            rows.reverse();
        }
        let (channels, mut data): (usize, Vec<f64>) = if pad.mono {
            let mean = |r: &[f64]| r.iter().fold(0.0, |s, v| s + v) / ch as f64;
            (1, rows.iter().map(|r| mean(r)).collect())
        } else {
            (ch, rows.concat())
        };
        if let Some((up, down)) = repitch_ratio(self.project.session.sample_rate as u32, x.rate, speed) {
            if !data.is_empty() {
                data = resample_poly(&data, channels, up, down);
            }
        }
        let prepared = Arc::new(Prepared { channels, data });
        self.prepared.insert(key, prepared.clone());
        Ok(prepared)
    }

    /// `Sampler.voice`: the voice a trigger plays.
    pub fn voice(&mut self, pad: &Pad, tr: &Trigger) -> Result<Voice, String> {
        let audio = self.prepare(pad, &tr.event)?;
        let rate = self.project.session.sample_rate as f64;
        let attack = (pad.attack_ms * rate / 1000.0).round_ties_even() as usize;
        let release = (pad.release_ms * rate / 1000.0).round_ties_even() as usize;
        let gate = tr.cutoff.map(|c| (c - tr.start).max(0) as usize);
        let frames = audio.frames();
        let length = match gate {
            Some(g) => frames.min(g + release),
            None => frames,
        };
        let velocity = (tr.event.velocity as f64 / 127.0 * tr.velocity_scale).min(1.0);
        Ok(Voice {
            start: tr.start,
            pan: pan_gains(audio.channels, pad.pan),
            gain: amplitude(pad.gain_db),
            velocity,
            attack,
            release,
            length,
            audio,
        })
    }
}

/// Features of a project that this engine does not process yet.
pub fn unsupported(p: &Project) -> Vec<String> {
    let mut out = Vec::new();
    let active = |effects: &[aaw_model::Effect]| effects.iter().any(|e| !e.bypass());
    if p.tracks.iter().any(|t| active(&t.effects)) {
        out.push("track effects".to_string());
    }
    if active(&p.master.effects) {
        out.push("master effects".to_string());
    }
    if !p.returns.is_empty() || p.tracks.iter().any(|t| !t.sends.is_empty()) {
        out.push("sends and returns".to_string());
    }
    let lanes = p.tracks.iter().any(|t| !t.automation.is_empty())
        || p.returns.iter().any(|r| !r.automation.is_empty())
        || !p.master.automation.is_empty();
    if lanes {
        out.push("automation".to_string());
    }
    out
}

/// Compiles a project whose samples live under `directory`. Features the engine
/// does not process are left out and listed in `Program::omitted`.
pub fn compile(p: &Project, directory: &Path) -> Result<Program, String> {
    let rate = p.session.sample_rate;
    let total = frame(&p.session.length_exact(), p.session.tempo, rate).max(0) as usize;
    let triggers = schedule(p);
    let mut sampler = Sampler::new(p, directory);
    let mut per_track: Vec<Vec<Voice>> = vec![Vec::new(); p.tracks.len()];
    for tr in &triggers {
        let pad = &p.tracks[tr.track].pads[&tr.pad];
        per_track[tr.track].push(sampler.voice(pad, tr)?);
    }
    let any_solo = p.tracks.iter().any(|t| t.solo);
    let index: HashMap<&str, usize> = p.tracks.iter().enumerate().map(|(i, t)| (t.id.as_str(), i)).collect();
    let tracks = p
        .render_order()
        .into_iter()
        .map(|t| {
            let voices = std::mem::take(&mut per_track[index[t.id.as_str()]]);
            TrackProgram {
                id: t.id.clone(),
                voices,
                pan: pan_gains(2, t.pan),
                gain: amplitude(t.gain_db),
                silent: t.mute || (any_solo && !t.solo),
            }
        })
        .collect();
    Ok(Program {
        rate: rate as u32,
        total,
        tracks,
        master: amplitude(p.session.master_gain_db),
        fade: total.min((p.session.end_fade_ms * rate as f64 / 1000.0).round_ties_even() as usize),
        omitted: unsupported(p),
    })
}
