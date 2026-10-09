//! Offline export: the mix, stems, snapshot and `report.json` a render leaves, in
//! the formats renders have always had, which `daw listen` and `daw compare` read.

use crate::program::{amplitude, compile_scoped, Cache, Scope, StemKind};
use crate::render::{DeviceReport, Frame, Renderer};
use aaw_dsp::dynamics::Reduction;
use crate::{sndfile, stretch, wav};
use aaw_dsp::resample::{pairwise_sum, Resampler};
use aaw_model::pyfmt::{json_dumps, json_dumps_indent};
use aaw_model::value::{dict, Value};
use aaw_model::{frame, project_hash, Effect, Project, Stretch, Stretcher};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
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

/// Level and safety measurements of a stereo render.
pub fn metrics(x: &[Frame], rate: u32) -> Value {
    let flat: Vec<f64> = x.iter().flat_map(|f| [f[0], f[1]]).collect();
    let n = flat.len();
    let peak = flat.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let squares: Vec<f64> = flat.iter().map(|v| v * v).collect();
    let rms = if n > 0 { (pairwise_sum(&squares) / n as f64).sqrt() } else { 0.0 };
    // Oversampled peak is an estimate, not a certified true-peak meter. The
    // channels are resampled apart, as `resample_poly` does, and side by side.
    let true_peak = if n > 0 {
        let resampler = Resampler::new(4, 1);
        let channel = |c: usize| {
            let column: Vec<f64> = x.iter().map(|f| f[c]).collect();
            resampler.apply(&column).iter().fold(0.0f64, |m, v| m.max(v.abs()))
        };
        std::thread::scope(|scope| {
            let left = scope.spawn(|| channel(0));
            channel(1).max(left.join().expect("the peak estimate panicked"))
        })
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

/// A compressor's, limiter's or clipper's reduction over some frames, as the report
/// names it.
fn reduction(r: &Reduction) -> Vec<(&'static str, Value)> {
    vec![
        ("max_gain_reduction_db", Value::Float(r.max)),
        ("mean_gain_reduction_db", Value::Float(r.mean())),
        ("fraction_over_1db_reduction", Value::Float(r.fraction_over_1db())),
    ]
}

/// A device's entry in the report, as `Device.report`: its ID if the song
/// gives it one, and a compressor's, limiter's or clipper's reduction over the song and
/// over each of its `sections` that holds a frame.
fn device_report(d: &DeviceReport, effect: &Effect, sections: &[&str]) -> Value {
    let mut out = vec![("type", Value::str(d.kind))];
    if let Some(id) = effect.id() {
        out.push(("id", Value::str(id)));
    }
    if d.bypass {
        out.push(("bypass", Value::Bool(true)));
        return dict(out);
    }
    out.push(("latency_frames", Value::int(d.latency as i64)));
    if !d.automated.is_empty() {
        out.push(("automated", Value::List(d.automated.iter().map(|a| Value::str(a)).collect())));
    }
    if let Some(r) = &d.reduction {
        out.extend(reduction(r));
        let by_section: Vec<(&str, Value)> =
            sections.iter().zip(&d.spans).filter(|(_, r)| r.frames > 0).map(|(id, r)| (*id, dict(reduction(r)))).collect();
        if !by_section.is_empty() {
            out.push(("sections", dict(by_section)));
        }
    }
    dict(out)
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    aaw_model::atomic_write(path, text).map_err(|e| e.to_string())
}

/// The error for a mix that would clip: its peak, the loudest stems with
/// theirs, and what to change, as paths and a command.
fn clipped(p: &Project, peak: f64, names: &[(&str, StemKind)], peaks: &[f64]) -> String {
    let mut loudest: Vec<(&(&str, StemKind), f64)> = names.iter().zip(peaks.iter().copied()).collect();
    loudest.sort_by(|a, b| b.1.total_cmp(&a.1));
    loudest.truncate(3);
    let stems: Vec<String> = loudest.iter().map(|((name, _), peak)| format!("{name} {:+.1}", db(*peak))).collect();
    let mut fixes = Vec::new();
    if let Some(((name, kind), _)) = loudest.first() {
        fixes.push(format!("`{}.{name}.gain_db`", kind.list()));
    }
    fixes.push("`session.master_gain_db`".to_string());
    // A limiter, or a clipper with its ceiling under full scale, holds the
    // mix under it, so what clips comes after it.
    let held = p.master.effects.iter().rev().find(|e| match e {
        Effect::Limiter(l) => !l.bypass,
        Effect::Clipper(c) => !c.bypass && c.ceiling_db < 0.0,
        _ => false,
    });
    let fix = if let Some(held) = held {
        format!("lower {}, or what follows the master {}", fixes.join(" or "), held.kind())
    } else {
        format!("lower {}, or add a limiter: daw effect add PROJECT master --type limiter", fixes.join(" or "))
    };
    let stems = if stems.is_empty() { String::new() } else { format!("; loudest stems {}", stems.join(", ")) };
    format!("Unsafe PCM export: the mix peaks at {:+.2} dBFS{stems}; {fix}", db(peak))
}

/// Renders a project the way `daw render` does: the whole song, one track,
/// group or return as its stem, or one section.
pub fn render(path: &Path, opts: &RenderOptions) -> Result<Value, String> {
    if opts.block_size == 0 {
        return Err("block_size must be positive".into());
    }
    let before = Instant::now();
    let p: Project = aaw_model::load(path, true).map_err(|e| e.to_string())?;
    let rate = p.session.sample_rate;
    let tempo = p.session.tempo;
    if let Some(t) = &opts.track {
        if p.track(t).is_none() && p.group(t).is_none() && !p.returns.iter().any(|r| &r.id == t) {
            return Err(format!("Unknown track, group or return: {t}"));
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
    // A track, group or return preview is its stem: it needs the tracks that
    // key or feed it, and omits the master chain.
    let scope = opts.track.as_deref().map_or(Scope::Song, Scope::Channel);
    let program = Arc::new(compile_scoped(&p, directory, &mut Cache::default(), scope)?);
    let (first, last) = region.map_or((0, total), |(a, b)| (a, b));
    let frames = (last - first).max(0) as usize;
    let names = program.stems();
    let mut renderer = Renderer::new(program.clone(), 0, opts.block_size);
    // Each compressor, limiter and clipper says what it took off in each section.
    let spans: Vec<(i64, i64)> = p
        .sections
        .iter()
        .map(|s| (frame(&s.at_exact(), tempo, rate), frame(&(s.at_exact() + s.length_exact()), tempo, rate)))
        .collect();
    renderer.measure_spans(&spans);
    let section_ids: Vec<&str> = p.sections.iter().map(|s| s.id.as_str()).collect();
    let mut mix: Vec<Frame> = Vec::with_capacity(frames);
    let mut stems: Vec<Vec<[f32; 2]>> = vec![Vec::with_capacity(frames); names.len()];
    let mut peaks = vec![0.0f64; names.len()];
    let mut block = vec![[0.0; 2]; opts.block_size];
    // The master gain and end fade, which stems carry as the mix does, for the
    // frames of one block. Most stems of a block share its frames.
    let mut envelope: (i64, Vec<f64>) = (0, Vec::with_capacity(opts.block_size));
    let level = (!program.master.gain.moves()).then(|| amplitude(program.master.gain.value()));
    let fill_envelope = |at: i64, n: usize, envelope: &mut (i64, Vec<f64>)| {
        if envelope.0 == at && envelope.1.len() == n {
            return;
        }
        envelope.0 = at;
        envelope.1.clear();
        envelope.1.extend((at..at + n as i64).map(|f| {
            if f < 0 || f >= total {
                return 0.0;
            }
            program.fader(f as usize) * level.unwrap_or_else(|| amplitude(program.master.gain.at(f)))
        }));
    };
    // The output trails the transport by the program's latency; rendering that
    // much further brings out the session's last frames.
    while !renderer.finished() {
        let heard = renderer.heard();
        let n = opts.block_size.min(program.total + program.latency - renderer.position());
        renderer.render(&mut block[..n], |index, post, at| {
            fill_envelope(at, post.len(), &mut envelope);
            // The block's frames inside the region.
            let lo = (first - at).clamp(0, post.len() as i64) as usize;
            let hi = (last - at).clamp(0, post.len() as i64) as usize;
            let mut peak = peaks[index];
            stems[index].extend(post[lo..hi].iter().zip(&envelope.1[lo..hi]).map(|(f, env)| {
                let x = [f[0] * env, f[1] * env];
                peak = peak.max(x[0].abs()).max(x[1].abs());
                [x[0] as f32, x[1] as f32]
            }));
            peaks[index] = peak;
        });
        let lo = (first - heard).clamp(0, n as i64) as usize;
        let hi = (last - heard).clamp(0, n as i64) as usize;
        mix.extend_from_slice(&block[lo..hi]);
    }
    // An unsafe mix writes nothing. Its other measurements wait until the
    // files are written, beside them.
    if !mix.iter().all(|f| f[0].is_finite() && f[1].is_finite()) {
        return Err("Unsafe PCM export: the mix has samples that are not numbers".into());
    }
    let peak = mix.iter().fold(0.0f64, |m, f| m.max(f[0].abs()).max(f[1].abs()));
    if peak >= 1.0 {
        return Err(clipped(&p, peak, &names, &peaks));
    }
    let fingerprint = project_hash(&p);
    let engine = engine_hash()?;
    let mut sample_hashes = Vec::new();
    for (name, s) in &p.samples {
        let digest = aaw_model::digest(&directory.join(&s.path)).map_err(|e| e.to_string())?;
        sample_hashes.push((name.as_str(), Value::str(&digest)));
    }
    let sample_hashes = dict(sample_hashes);
    let mut dependencies = vec![
        ("aaw-engine", Value::str(env!("CARGO_PKG_VERSION"))),
        ("libsndfile", Value::str(&sndfile::version())),
    ];
    // A song that stretches names what stretched it; one that does not keeps
    // the identity it had.
    let stretches = p.tracks.iter().flat_map(|t| t.sound_pads().values()).any(|pad| {
        pad.stretch == Stretch::PreservePitch && pad.source_bpm.is_some_and(|bpm| bpm != p.session.tempo)
    });
    if stretches {
        dependencies.push((
            "stretcher",
            Value::str(&match p.session.stretcher {
                Stretcher::Signalsmith => stretch::SIGNALSMITH.to_string(),
                Stretcher::Rubberband => stretch::rubberband_version()?,
            }),
        ));
    }
    let dependencies = dict(dependencies);
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
    // The mix's measurements, its file and the stems' files are made side by
    // side: a few stems at a time, since each is held whole while it is hashed
    // and written.
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4);
    let (report, written, hashes) = std::thread::scope(|scope| {
        let report = scope.spawn(|| metrics(&mix, rate as u32));
        let written = scope.spawn(|| wav::write_atomic(&mix_path, &wav::pcm24_wav_bytes(&mix, rate as u32)));
        let stems: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Relaxed);
                        let Some(frames) = stems.get(i) else { break };
                        let bytes = wav::float_wav_bytes(frames, rate as u32);
                        let path = output.join("stems").join(format!("{}.wav", names[i].0));
                        done.push((i, wav::write_atomic(&path, &bytes).map(|_| hex(&Sha256::digest(&bytes)))));
                    }
                    done
                })
            })
            .collect();
        let mut hashes: Vec<Result<String, String>> = vec![Err("The stem was not written".into()); names.len()];
        for worker in stems {
            for (i, hash) in worker.join().expect("a stem writer panicked") {
                hashes[i] = hash;
            }
        }
        (
            report.join().expect("the measurements panicked"),
            written.join().expect("the mix writer panicked"),
            hashes,
        )
    });
    written?;
    let reports = renderer.report();
    let effects = |chain: &[DeviceReport], of: &[Effect]| {
        Value::List(chain.iter().zip(of).map(|(d, effect)| device_report(d, effect, &section_ids)).collect())
    };
    let params = |lanes: &[String]| Value::List(lanes.iter().map(|l| Value::str(l)).collect());
    let mut track_reports = Vec::new();
    let mut stem = |name: &str, fields: Vec<(&str, Value)>| -> Result<(), String> {
        let index = names.iter().position(|(n, _)| *n == name).expect("a stem for each heard channel");
        let mut entry = vec![
            ("kind", Value::str(names[index].1.name())),
            ("audio_sha256", Value::str(&hashes[index].clone()?)),
            ("peak_dbfs", Value::Float(db(peaks[index]))),
        ];
        entry.extend(fields);
        track_reports.push((name.to_string(), dict(entry)));
        Ok(())
    };
    // Stems are reported in document order: tracks, groups, then returns.
    for t in &p.tracks {
        let Some(ti) = program.tracks.iter().position(|x| x.id == t.id && x.in_mix) else {
            continue;
        };
        let track = &program.tracks[ti];
        let events = track.synth.as_ref().map_or(track.voices.len(), |s| s.notes.len());
        let mut fields = vec![
            ("events", Value::int(events as i64)),
            ("effects", effects(&reports.tracks[ti], &t.effects)),
        ];
        if let Some(synth) = t.midi.as_ref().and_then(|m| m.synth()).filter(|_| !reports.patches[ti].is_empty()) {
            fields.push(("instrument_effects", effects(&reports.patches[ti], &synth.effects)));
        }
        if !track.automation.is_empty() {
            fields.push(("automation", params(&track.automation)));
        }
        if let Some(g) = track.group {
            fields.push(("group", Value::str(&program.groups[g].id)));
        }
        stem(&t.id, fields)?;
    }
    for (gi, g) in program.groups.iter().enumerate().filter(|(_, g)| g.in_mix) {
        let mut fields = vec![
            ("tracks", Value::List(g.members.iter().map(|s| Value::str(s)).collect())),
            ("effects", effects(&reports.groups[gi], p.group(&g.id).map_or(&[], |g| &g.effects))),
        ];
        if !g.automation.is_empty() {
            fields.push(("automation", params(&g.automation)));
        }
        stem(&g.id, fields)?;
    }
    for (ri, r) in program.returns.iter().enumerate() {
        let mut fields = vec![
            ("senders", Value::List(r.senders.iter().map(|s| Value::str(s)).collect())),
            ("effects", effects(&reports.returns[ri], p.returns.iter().find(|x| x.id == r.id).map_or(&[], |x| &x.effects))),
        ];
        if !r.automation.is_empty() {
            fields.push(("automation", params(&r.automation)));
        }
        stem(&r.id, fields)?;
    }
    let track_reports: Vec<(&str, Value)> = track_reports.iter().map(|(name, v)| (name.as_str(), v.clone())).collect();
    // Every stem sums to the mix without master effects, unless a grouped
    // track's stem would count twice beside its group's.
    let grouped = program.tracks.iter().any(|t| t.in_mix && t.group.is_some_and(|g| program.groups[g].in_mix));
    let stems_sum = program.master.chain.devices.is_empty() && !grouped;
    let master_effects = reports.master;
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
        ("master_effects", effects(&master_effects, &p.master.effects)),
        ("master_automation", params(&program.master.automation)),
        ("stems_sum_to_mix", Value::Bool(stems_sum)),
        ("stem_policy", Value::str("post-insert, post-track and post-master gain and fade, before master effects; track stems are dry, a grouped track's before its group, each group and each return has its own stem; the ungrouped tracks, the groups and the returns sum to the mix; float WAV, same start and length")),
        ("send_policy", Value::str("post-fader sends tap after track gain and pan, pre-fader after inserts; muted or solo-muted tracks and groups send nothing; a muted group silences its tracks; returns are never solo-muted")),
        ("sidechain_policy", Value::str("key is the source track after its inserts, before its gain, pan, mute and solo")),
        ("automation_policy", Value::str("lanes override static values for the whole song, holding the first value before the first point and the last after the last point; a lane whose points share one value renders exactly as that static value; gain, pan and send levels change per frame, compressor, delay and reverb parameters per frame, filter and eq coefficients every 64 frames; no smoothing")),
        (
            "pitch_policy",
            Value::str("bandlimited repitch; pitch changes duration; source_bpm repitches, or stretches in time at the same pitch where the pad's stretch is preserve_pitch"),
        ),
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
