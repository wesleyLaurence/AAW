//! The `daw` binary: JSON in and out over the Rust model and engine.
//!
//! Results print to stdout as JSON. Errors print `{"error", "command"}` to stderr
//! with exit status 1, as the Python CLI does.

use aaw_engine::offline::{render, RenderOptions};
use aaw_engine::program::compile;
use aaw_engine::realtime::{benchmark, play, PlayOptions};
use aaw_engine::schedule::schedule;
use aaw_model::validate::ValidationError;
use aaw_model::{frame, Beat, ModelError, Project};
use clap::{Parser, Subcommand};
use serde_json::{json, Value as Json};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// Debug builds abort when the audio callback allocates.
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

#[derive(Parser)]
#[command(name = "daw", about = "Agent DAW engine (Rust). Emits JSON.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Rewrite a project in canonical form.
    Fmt { project: PathBuf },
    /// Validate and atomically replace project fields from a JSON merge patch.
    Apply {
        project: PathBuf,
        patch: PathBuf,
        /// Project SHA256 from inspect; rejects stale edits.
        #[arg(long)]
        expect: String,
    },
    /// Validate documents with the Rust model and print each one's canonical
    /// YAML and fingerprints, or its errors. Reads only; samples are not checked.
    Model { paths: Vec<PathBuf> },
    /// Render the mix and stems, one track or return as its stem, or one section.
    Render {
        project: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        section: Option<String>,
    },
    /// List every scheduled hit: its start frame, track, pad and release frame.
    Schedule { project: PathBuf },
    /// Play the song through the default audio output.
    Play {
        project: PathBuf,
        /// Start position in beats; sounding samples are picked up mid-sample.
        #[arg(long, default_value = "0")]
        from: String,
        /// Stop after this many seconds.
        #[arg(long)]
        seconds: Option<f64>,
        /// Hardware buffer size in frames.
        #[arg(long, default_value_t = 128)]
        buffer: u32,
        /// Time the playback path without a device instead of playing.
        #[arg(long)]
        benchmark: bool,
    },
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Command::Fmt { .. } => "fmt",
            Command::Apply { .. } => "apply",
            Command::Model { .. } => "model",
            Command::Render { .. } => "render",
            Command::Schedule { .. } => "schedule",
            Command::Play { .. } => "play",
        }
    }
}

type Result<T> = std::result::Result<T, String>;

fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Holds the project's advisory lock, as the Python CLI's writers do.
fn lock(project: &Path) -> Result<File> {
    let dir = match project.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let file = File::options()
        .append(true)
        .create(true)
        .open(dir.join(".daw.lock"))
        .map_err(text)?;
    file.lock().map_err(text)?;
    Ok(file)
}

fn errors_json(e: &ValidationError) -> Json {
    Json::Array(
        e.errors
            .iter()
            .map(|x| json!({"loc": x.loc_text(), "type": x.kind, "msg": x.msg}))
            .collect(),
    )
}

fn model_entry(path: &Path) -> Json {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return json!({"path": path, "valid": false, "kind": "io", "error": e.to_string()}),
    };
    match aaw_model::parse(&text) {
        Ok(p) => json!({
            "path": path,
            "valid": true,
            "yaml": aaw_model::to_yaml(&p),
            "project_sha256": aaw_model::project_hash(&p),
            "fingerprints": aaw_model::fingerprints(&p),
        }),
        Err(ModelError::Validation(e)) => json!({
            "path": path,
            "valid": false,
            "kind": "validation",
            "error": e.to_string(),
            "errors": errors_json(&e),
        }),
        Err(e) => json!({"path": path, "valid": false, "kind": "yaml", "error": e.to_string()}),
    }
}

/// An engine result as JSON, keeping its key order.
fn value(v: aaw_model::value::Value) -> Json {
    use aaw_model::value::Value;
    match v {
        Value::None => Json::Null,
        Value::Bool(b) => Json::Bool(b),
        Value::Int(n) => serde_json::from_str(&n.to_string()).expect("integer"),
        Value::Float(f) => serde_json::Number::from_f64(f).map_or(Json::Null, Json::Number),
        Value::Str(s) => Json::String(s),
        Value::List(items) => Json::Array(items.into_iter().map(value).collect()),
        Value::Dict(d) => Json::Object(
            d.into_iter()
                .map(|(k, v)| (k.as_str().unwrap_or_default().to_string(), value(v)))
                .collect(),
        ),
        Value::Bytes(_) | Value::Other(_) => Json::Null,
    }
}

fn run(command: &Command) -> Result<Json> {
    match command {
        Command::Render {
            project,
            output,
            track,
            section,
        } => render(
            project,
            &RenderOptions {
                output: output.clone(),
                track: track.clone(),
                section: section.clone(),
                ..RenderOptions::default()
            },
        )
        .map(value),
        Command::Schedule { project } => {
            let p = aaw_model::load(project, false).map_err(text)?;
            let triggers: Vec<Json> = schedule(&p)
                .iter()
                .map(|t| json!({"start": t.start, "track": t.track_id, "pad": t.pad, "cutoff": t.cutoff}))
                .collect();
            Ok(Json::Array(triggers))
        }
        Command::Play {
            project,
            from,
            seconds,
            buffer,
            benchmark: bench,
        } => {
            let p = aaw_model::load(project, true).map_err(text)?;
            let from_beat = aaw_model::beat(&Beat::Str(from.clone()))?;
            let start = frame(&from_beat, p.session.tempo, p.session.sample_rate).max(0) as usize;
            let dir = project.parent().unwrap_or(Path::new("."));
            let program = Arc::new(compile(&p, dir)?);
            if start >= program.total {
                return Err(format!("--from {from} is at or after the session end"));
            }
            let opts = PlayOptions {
                from: start,
                seconds: *seconds,
                buffer: *buffer,
            };
            if *bench {
                return Ok(value(benchmark(program, &opts)));
            }
            if !program.omitted.is_empty() {
                eprintln!(
                    "{}",
                    json!({"warning": format!("playing without {}, which the Rust engine does not process yet", program.omitted.join(", "))})
                );
            }
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed)).map_err(text)?;
            play(program, &opts, stop).map(value)
        }
        Command::Model { paths } => Ok(Json::Array(paths.iter().map(|p| model_entry(p)).collect())),
        Command::Fmt { project } => {
            let _lock = lock(project)?;
            let p = aaw_model::load(project, true).map_err(text)?;
            aaw_model::save(&p, project).map_err(text)?;
            Ok(json!({"project": project, "formatted": true}))
        }
        Command::Apply {
            project,
            patch,
            expect,
        } => {
            let _lock = lock(project)?;
            let p: Project = aaw_model::load(project, true).map_err(text)?;
            if &aaw_model::project_hash(&p) != expect {
                return Err("Stale project revision; inspect and retry".into());
            }
            let patch = aaw_model::json_value(&std::fs::read_to_string(patch).map_err(text)?)
                .map_err(text)?;
            let updated = aaw_model::apply_patch(&p, &patch).map_err(text)?;
            // Verify referenced audio before replacing the authoritative document.
            let root = project.parent().unwrap_or(Path::new("."));
            for (name, asset) in &updated.samples {
                let path = root.join(&asset.path);
                if !path.is_file() {
                    return Err(format!("Missing asset {name}"));
                }
                if let Some(sha) = &asset.sha256 {
                    if &aaw_model::digest(&path).map_err(text)? != sha {
                        return Err(format!("Changed asset {name}"));
                    }
                }
            }
            aaw_model::save(&updated, project).map_err(text)?;
            Ok(json!({"project_sha256": aaw_model::project_hash(&updated)}))
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli.command) {
        Ok(result) => {
            println!("{}", serde_json::to_string_pretty(&result).expect("json"));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", json!({"error": error, "command": cli.command.name()}));
            ExitCode::FAILURE
        }
    }
}
