//! The Python side of `daw`: the sample library, sample analysis and
//! perception run in the `agent_daw` package, which this finds and runs.

use serde_json::Value as Json;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The Python that has the `agent_daw` package: AAW_PYTHON, or the environment
/// of the checkout this was built from.
pub fn interpreter() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("AAW_PYTHON") {
        return Ok(path.into());
    }
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let python = checkout.join(".venv/bin/python");
    if python.exists() {
        return Ok(python);
    }
    Err(format!(
        "This command runs in Python, which was not found at {}; run `uv sync` in the checkout or set AAW_PYTHON",
        python.display()
    ))
}

/// A Python `daw` command, such as `samples`, ready to be given its arguments.
pub fn command(name: &str) -> Result<Command, String> {
    let mut python = Command::new(interpreter()?);
    python.args(["-m", "agent_daw.cli", name]);
    Ok(python)
}

/// Runs a Python `daw` command and returns the JSON it prints, or its error.
pub fn run(name: &str, args: &[&str]) -> Result<Json, String> {
    let out = command(name)?
        .args(args)
        .output()
        .map_err(|e| format!("Could not start Python: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let message = serde_json::from_str::<Json>(stderr.trim())
            .ok()
            .and_then(|j| j.get("error").and_then(Json::as_str).map(str::to_string));
        return Err(message.unwrap_or_else(|| match stderr.trim() {
            "" => format!("daw {name} failed"),
            text => text.to_string(),
        }));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("daw {name} printed no JSON: {e}"))
}
