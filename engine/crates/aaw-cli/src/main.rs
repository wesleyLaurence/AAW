//! The `daw` binary: JSON in and out over the Rust model and engine.
//!
//! Results print to stdout as JSON. Errors print `{"error", "command"}` to stderr
//! with exit status 1, as the Python CLI does.

use aaw_model::validate::ValidationError;
use aaw_model::{ModelError, Project};
use clap::{Parser, Subcommand};
use serde_json::{json, Value as Json};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Command::Fmt { .. } => "fmt",
            Command::Apply { .. } => "apply",
            Command::Model { .. } => "model",
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

fn run(command: &Command) -> Result<Json> {
    match command {
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
