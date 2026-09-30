//! Offline export: the mix, stems, snapshot and `report.json` a render leaves, in
//! the Python engine's formats so `daw listen` and `daw compare` read them.

use crate::program::{compile, unsupported, Program};
use crate::render::{Frame, Renderer};
use crate::{sndfile, wav};
use aaw_dsp::resample::{pairwise_sum, resample_poly};
use aaw_model::pyfmt::{json_dumps, json_dumps_indent};
use aaw_model::value::{dict, Value};
use aaw_model::{frame, project_hash, Project};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

pub struct RenderOptions {
    pub output: Option<PathBuf>,
    pub track: Option<String>,
    pub section: Option<String>,
    pub block_size: usize,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            output: None,
            track: None,
            section: None,
            block_size: 4096,
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}

/// `engine.metrics`: level and safety measurements of a stereo render.
pub fn metrics(x: &[Frame], rate: u32) -> Value {
    let flat: Vec<f64> = x.iter().flat_map(|f| [f[0], f[1]]).collect();
    let n = flat.len();
    let peak = flat.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let squares: Vec<f64> = flat.iter().map(|v| v * v).collect();
    let rms = if n > 0 { (pairwise_sum(&squares) / n as f64).sqrt() } else { 0.0 };
    // Oversampled peak is an estimate, not a certified true-peak meter.
    let true_peak = if n > 0 {
        resample_poly(&flat, 2, 4, 1).iter().fold(0.0f64, |m, v| m.max(v.abs()))
    } else {
        0.0
    };
    let dc = if n > 0 { pairwise_sum(&flat) / n as f64 } else { f64::NAN };
    dict(vec![
        ("frames", Value::int(x.len() as i64)),
        ("duration_seconds", Value::Float(x.len() as f64 / rate as f64)),
        ("sample_rate", Value::int(rate as i64)),
        ("channels", Value::int(2)),
        ("peak_dbfs", Value::Float(db(peak))),
        ("estimated_true_peak_dbtp", Value::Float(db(true_peak))),
        ("rms_dbfs", Value::Float(db(rms))),
        ("over_range_samples", Value::int(flat.iter().filter(|v| v.abs() >= 1.0).count() as i64)),
        ("dc_offset", Value::Float(dc)),
        ("finite", Value::Bool(flat.iter().all(|v| v.is_finite()))),
    ])
}

/// The engine's identity for render IDs: a hash of the running executable.
fn engine_hash() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bytes = std::fs::read(exe).map_err(|e| e.to_string())?;
    Ok(hex(&Sha256::digest(&bytes)))
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    aaw_model::atomic_write(path, text).map_err(|e| e.to_string())
}

/// Renders a project the way `daw render` does: the whole song, one track or
/// return as its stem, or one section.
pub fn render(path: &Path, opts: &RenderOptions) -> Result<Value, String> {
    if opts.block_size == 0 {
        return Err("block_size must be positive".into());
    }
    let before = Instant::now();
    let p: Project = aaw_model::load(path, true).map_err(|e| e.to_string())?;
    let missing = unsupported(&p);
    if !missing.is_empty() {
        return Err(format!(
            "The Rust engine does not render {} yet; use `uv run daw render`",
            missing.join(", ")
        ));
    }
    let rate = p.session.sample_rate;
    let tempo = p.session.tempo;
    if let Some(t) = &opts.track {
        if p.track(t).is_none() {
            return Err(format!("Unknown track or return: {t}"));
        }
    }
    let region = match &opts.section {
        None => None,
        Some(name) => {
            let matches: Vec<_> = p.sections.iter().filter(|s| &s.id == name).collect();
            if matches.len() != 1 {
                return Err(format!("Unknown or ambiguous section: {name}"));
            }
            let s = matches[0];
            Some((
                frame(&s.at_exact(), tempo, rate),
                frame(&(s.at_exact() + s.length_exact()), tempo, rate),
            ))
        }
    };
    let total = frame(&p.session.length_exact(), tempo, rate);
    if total < 1 {
        return Err("Session length must be at least one audio frame".into());
    }
    if region.is_some_and(|(a, b)| b <= a) {
        return Err("Section length must be at least one audio frame".into());
    }
    if total > rate * 60 * 15 {
        return Err("MVP render limit is 15 minutes".into());
    }
    let directory = path.parent().unwrap_or(Path::new("."));
    let mut program: Program = compile(&p, directory)?;
    // A track preview is that track's stem.
    if let Some(t) = &opts.track {
        program.tracks.retain(|x| &x.id == t);
    }
    let program = Arc::new(program);
    let (first, last) = region.map_or((0, total as usize), |(a, b)| (a as usize, b as usize));
    let mut renderer = Renderer::new(program.clone(), 0, opts.block_size);
    let mut mix: Vec<Frame> = Vec::with_capacity(last - first);
    let mut stems: Vec<Vec<[f32; 2]>> = vec![Vec::with_capacity(last - first); program.tracks.len()];
    let mut peaks = vec![0.0f64; program.tracks.len()];
    let mut block = vec![[0.0; 2]; opts.block_size];
    while renderer.position() < total as usize {
        let at = renderer.position();
        let n = opts.block_size.min(total as usize - at);
        let lo = first.max(at).min(at + n) - at;
        let hi = last.min(at + n).max(at) - at;
        renderer.process(&mut block[..n], |ti, post| {
            for (k, f) in post[lo..hi].iter().enumerate() {
                let env = program.envelope(at + lo + k);
                let x = [f[0] * env, f[1] * env];
                peaks[ti] = peaks[ti].max(x[0].abs()).max(x[1].abs());
                stems[ti].push([x[0] as f32, x[1] as f32]);
            }
        });
        mix.extend_from_slice(&block[lo..hi]);
    }
    let report = metrics(&mix, rate as u32);
    let finite = matches!(report.get("finite"), Some(Value::Bool(true)));
    let over = matches!(report.get("over_range_samples"), Some(Value::Int(n)) if *n != 0.into());
    if !finite || over {
        let Some(Value::Float(peak)) = report.get("peak_dbfs") else { unreachable!() };
        return Err(format!(
            "Unsafe PCM export: peak {peak:.2} dBFS; lower master_gain_db or add a master limiter"
        ));
    }
    let fingerprint = project_hash(&p);
    let engine = engine_hash()?;
    let mut sample_hashes = Vec::new();
    for (name, s) in &p.samples {
        let digest = aaw_model::digest(&directory.join(&s.path)).map_err(|e| e.to_string())?;
        sample_hashes.push((name.as_str(), Value::str(&digest)));
    }
    let sample_hashes = dict(sample_hashes);
    let dependencies = dict(vec![
        ("aaw-engine", Value::str(env!("CARGO_PKG_VERSION"))),
        ("libsndfile", Value::str(&sndfile::version())),
    ]);
    let opt_str = |s: &Option<String>| s.as_deref().map_or(Value::None, Value::str);
    let render_id = hex(&Sha256::digest(
        json_dumps(&dict(vec![
            ("project", Value::str(&fingerprint)),
            ("engine", Value::str(&engine)),
            ("samples", sample_hashes.clone()),
            ("dependencies", dependencies.clone()),
            ("track", opt_str(&opts.track)),
            ("section", opt_str(&opts.section)),
        ]))
        .as_bytes(),
    ));
    let output = opts
        .output
        .clone()
        .unwrap_or_else(|| directory.join("renders").join(&render_id[..12]));
    if output.exists() && std::fs::read_dir(&output).map_err(|e| e.to_string())?.next().is_some() {
        let report_path = output.join("report.json");
        let same = std::fs::read_to_string(&report_path)
            .ok()
            .and_then(|t| aaw_model::json_value(&t).ok())
            .and_then(|v| v.get("render_id").cloned())
            .is_some_and(|v| matches!(v, Value::Str(s) if s == render_id));
        if !same {
            return Err(
                "Output directory contains a different render; choose a fresh --output directory".into(),
            );
        }
    }
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let mix_path = output.join("mix.wav");
    wav::write_atomic(&mix_path, &wav::pcm24_wav_bytes(&mix, rate as u32))?;
    let mut track_reports = Vec::new();
    // Stems are reported in document order.
    for t in &p.tracks {
        let Some(ti) = program.tracks.iter().position(|x| x.id == t.id) else {
            continue;
        };
        let stem_path = output.join("stems").join(format!("{}.wav", t.id));
        let bytes = wav::float_wav_bytes(&stems[ti], rate as u32);
        wav::write_atomic(&stem_path, &bytes)?;
        let events = program.tracks[ti].voices.len();
        track_reports.push((
            t.id.as_str(),
            dict(vec![
                ("kind", Value::str("track")),
                ("audio_sha256", Value::str(&hex(&Sha256::digest(&bytes)))),
                ("peak_dbfs", Value::Float(db(peaks[ti]))),
                ("events", Value::int(events as i64)),
                ("effects", Value::List(vec![])),
            ]),
        ));
    }
    let region_value = match region {
        Some((a, b)) => Value::List(vec![Value::int(a), Value::int(b)]),
        None => Value::None,
    };
    let project_file = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mix_digest = aaw_model::digest(&mix_path).map_err(|e| e.to_string())?;
    let manifest = dict(vec![
        ("engine_version", Value::str(env!("CARGO_PKG_VERSION"))),
        ("engine", Value::str("rust")),
        ("platform", Value::str(&format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH))),
        ("dependencies", dependencies),
        ("project_sha256", Value::str(&fingerprint)),
        ("sample_sha256", sample_hashes),
        ("engine_sha256", Value::str(&engine)),
        ("render_id", Value::str(&render_id)),
        (
            "target",
            dict(vec![
                ("track", opt_str(&opts.track)),
                ("section", opt_str(&opts.section)),
                ("source_frames", region_value),
            ]),
        ),
        ("project_file", Value::str(&project_file.to_string_lossy())),
        ("audio_sha256", Value::str(&mix_digest)),
        ("render_seconds", Value::Float(before.elapsed().as_secs_f64())),
        ("mix", report),
        ("tracks", dict(track_reports)),
        ("sections", Value::List(p.sections.iter().map(|s| s.dump(false)).collect())),
        ("tail_policy", Value::str("truncate at session length with explicit end fade")),
        ("master_effects", Value::List(vec![])),
        ("master_automation", Value::List(vec![])),
        ("stems_sum_to_mix", Value::Bool(true)),
        ("stem_policy", Value::str("post-insert, post-track and post-master gain and fade, before master effects; track stems are dry, each return has its own stem; float WAV, same start and length")),
        ("send_policy", Value::str("post-fader sends tap after track gain and pan, pre-fader after inserts; muted or solo-muted tracks send nothing; returns are never solo-muted")),
        ("sidechain_policy", Value::str("key is the source track after its inserts, before its gain, pan, mute and solo")),
        ("automation_policy", Value::str("lanes override static values for the whole song, holding the first value before the first point and the last after the last point; a lane whose points share one value renders exactly as that static value; gain, pan and send levels change per frame, compressor, delay and reverb parameters per frame, filter and eq coefficients every 64 frames; no smoothing")),
        ("pitch_policy", Value::str("bandlimited repitch; pitch changes duration; source_bpm also repitches")),
    ]);
    write_text(&output.join("report.json"), &(json_dumps_indent(&manifest) + "\n"))?;
    aaw_model::save(&p, &output.join("song.snapshot.yaml")).map_err(|e| e.to_string())?;
    let latest = if opts.track.is_none() && opts.section.is_none() {
        "latest.json"
    } else {
        "latest-preview.json"
    };
    let resolved = std::fs::canonicalize(&output).map_err(|e| e.to_string())?;
    write_text(
        &directory.join("renders").join(latest),
        &(json_dumps_indent(&dict(vec![
            ("directory", Value::str(&resolved.to_string_lossy())),
            ("mix", Value::str(&resolved.join("mix.wav").to_string_lossy())),
            ("project_sha256", Value::str(&fingerprint)),
        ])) + "\n"),
    )?;
    let Value::Dict(fields) = manifest else { unreachable!() };
    let mut result = aaw_model::value::Dict::new();
    result.insert(aaw_model::value::Key::str("directory"), Value::str(&resolved.to_string_lossy()));
    result.insert(
        aaw_model::value::Key::str("mix_path"),
        Value::str(&resolved.join("mix.wav").to_string_lossy()),
    );
    result.extend(fields);
    Ok(Value::Dict(result))
}
