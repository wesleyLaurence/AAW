//! `daw generate` and `daw keys`: a sound or a loop from a description through
//! the ElevenLabs API at the person's own key, saved in the workspace's
//! generated folder and imported into the project as one undoable edit; the
//! key in the Keychain, the plan's tier and credits, and the agent's daily
//! budget. The request, the files and the budget are `aaw_host::generate`;
//! this is the command shell around them.

use crate::{files, route, text, Cli, GenerateCmd, KeysCmd, Song};
use aaw_host::command::{Command, Origin};
use aaw_host::generate::{self, Client, Format, SoundRequest};
use aaw_host::{keys, project, python};
use aaw_model::Meter;
use serde_json::{json, Value as Json};
use std::io::{BufRead, IsTerminal};
use std::path::Path;

type Result<T> = std::result::Result<T, String>;

pub fn run(cli: &Cli, cmd: &GenerateCmd) -> Result<Json> {
    match cmd {
        GenerateCmd::Sound {
            project,
            text,
            seconds,
            bars,
            looped,
            influence,
            id,
            estimate,
        } => sound(cli, project, text, *seconds, *bars, *looped, *influence, id.as_deref(), *estimate),
        GenerateCmd::List { project } => {
            let song = files(project);
            let p = aaw_model::load(&song, false).ok();
            let dir = song.parent().unwrap_or(Path::new("."));
            Ok(json!({
                "project": project::shown(&song),
                "generated": generate::list(dir, p.as_ref()),
                "folder": generate::dir(),
            }))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sound(
    cli: &Cli,
    project: &Song,
    description: &str,
    seconds: Option<f64>,
    bars: Option<f64>,
    looped: bool,
    influence: Option<f64>,
    id: Option<&str>,
    estimate: bool,
) -> Result<Json> {
    let origin = cli.origin();
    let song = files(project);
    let p = aaw_model::load(&song, false).map_err(text)?;
    let project_dir = song.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
    let mut request = SoundRequest {
        text: description.to_string(),
        seconds,
        influence,
        looped,
        bpm: None,
        bars: None,
    };
    if let Some(bars) = bars {
        if seconds.is_some() {
            return Err("Give --bars or --seconds, not both".into());
        }
        let meter = Meter::parse(&p.session.time_signature)?;
        request = request.with_bars(bars, p.session.tempo, meter.bar_f64())?;
    }
    request.check()?;
    let id = match id {
        Some(id) if p.samples.contains_key(id) => return Err(format!("Sample ID already exists: {id}")),
        Some(id) => id.to_string(),
        None => generate::default_id(&request.text, |candidate| p.samples.contains_key(candidate)),
    };
    let data_dir = project::data_dir();
    let base = generate::base_url();
    if estimate {
        let settings = generate::load_settings(&data_dir);
        let mut out = request.estimate(&settings, origin);
        out["sample_id"] = json!(id);
        out["key"] = json!(generate::key(&base).ok().flatten().is_some());
        return Ok(out);
    }
    // The seconds asked for are held before anything is sent, and settled
    // once the audio says how long it is: nothing when nothing came back.
    let asked = request.seconds_asked();
    let (hold, held) = generate::hold(&data_dir, origin, asked)?;
    let mut notes: Vec<String> = Vec::new();
    let generated = match ask(&base, &data_dir, &request, held.tier.clone(), &mut notes) {
        Ok(generated) => generated,
        Err(e) => {
            // The request's refusal is what the caller needs; a hold that
            // could not be given back stays counted.
            let _ = hold.settle(&data_dir, 0.0, None);
            return Err(e);
        }
    };
    notes.extend(generated.notes.iter().cloned());
    let folder = generate::dir();
    let saved = generate::save(&generated, &folder, &generate::file_stem(&id), &request, origin);
    // What came back is spent, whether or not it was saved or imported.
    let spent = match &saved {
        Ok(saved) if saved.seconds > 0.0 => saved.seconds,
        _ => generated.pcm_seconds().unwrap_or(asked),
    };
    let tier = generated.tier.clone().filter(|t| held.tier.as_ref() != Some(t));
    let mut settings = hold.settle(&data_dir, spent, tier)?;
    let saved = saved?;

    let not_imported = |e: String| format!("The sound was generated and saved as {} but not imported: {e}", saved.file.display());
    let mut seconds = saved.seconds;
    let (entry, recipe) = match generated.format {
        Format::Pcm44100 => {
            let entry = generate::project_copy(&saved, &project_dir)?;
            let recipe = recipe_beside(entry["path"].as_str().unwrap_or_default());
            (entry, recipe)
        }
        Format::Mp3At192 | Format::Mp3At128 => {
            // A compressed file is decoded into the project by the same
            // import a person's file goes through, and taken into the song
            // below as a PCM sound is.
            let mut copied = python::run(
                "samples",
                &["import", &saved.file.to_string_lossy(), "--project", &song.to_string_lossy(), "--copy-only"],
            )
            .map_err(not_imported)?;
            let path = copied["path"].as_str().unwrap_or_default().to_string();
            let recipe_file = generate::project_recipe(&saved, &project_dir, &path)?;
            let recipe = format!("samples/{}", recipe_file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            if let Some(entry) = copied.as_object_mut() {
                // The decoder says the length where libsndfile could not.
                if let Some(decoded) = entry.remove("decoded").and_then(|d| d["duration"].as_f64()).filter(|_| seconds <= 0.0) {
                    seconds = decoded;
                }
            }
            (copied, recipe)
        }
    };
    let sample_path = entry["path"].as_str().unwrap_or_default().to_string();
    let patch = json!({"samples": {id.clone(): entry}});
    let reply = route(cli, project, Command::Apply { patch, label: Some(format!("Generate {id}")) }, None).map_err(not_imported)?;
    let revision = reply.get("revision").cloned();
    // The library's search reads its index, so the new sound is scanned in
    // now; the scan is incremental and registers the folder the first time.
    match python::run("samples", &["scan", &folder.to_string_lossy()]) {
        Ok(_) if !settings.folder_registered => {
            settings = generate::update_settings(&data_dir, |s| {
                s.folder_registered = true;
                Ok(s.clone())
            })?;
        }
        Ok(_) => {}
        Err(e) => notes.push(format!(
            "The generated folder {} was not scanned into the sample library ({e}); `daw samples folders add` that folder adds it",
            folder.display()
        )),
    }
    let mut out = json!({
        "sample_id": id,
        "path": sample_path,
        "seconds": (seconds * 1000.0).round() / 1000.0,
        "sample_rate": saved.sample_rate,
        "channels": saved.channels,
        "format": generated.format.name(),
        "bpm": request.bpm,
        "bars": request.bars,
        "loop": request.looped,
        "text_sent": request.sent_text(),
        "recipe": recipe,
        "generated_file": saved.file,
        "generated_recipe": saved.recipe_file,
        "request_id": generated.request_id,
        "cost_credits": generated.cost,
        "tier": settings.tier,
        "origin": origin.as_str(),
        "budget": settings.budget_json(),
        "placed": false,
        "next": "Nothing is placed: daw pad add PROJECT TRACK PAD --sample ID plays it from a pad, daw audio add puts it on a track as an audio clip, and daw samples analyze measures it",
    });
    if let Some(revision) = revision {
        out["revision"] = revision;
    }
    if !notes.is_empty() {
        out["notes"] = json!(notes);
    }
    Ok(out)
}

/// The key, the plan's tier when it is not yet known, and the sound.
fn ask(base: &str, data_dir: &Path, request: &SoundRequest, mut tier: Option<String>, notes: &mut Vec<String>) -> Result<generate::Generated> {
    let key = generate::key(base)?.ok_or_else(generate::no_key)?;
    let client = Client::new(base, key);
    if tier.is_none() {
        match generate::subscription(&client) {
            Ok(sub) => {
                tier = Some(sub.tier.clone());
                generate::update_settings(data_dir, |s| {
                    s.tier = Some(sub.tier);
                    Ok(())
                })?;
            }
            Err(e) => notes.push(format!("The plan's tier could not be read ({e}); the sound is asked for as MP3 at 128 kb/s")),
        }
    }
    generate::sound(&client, request, tier.as_deref())
}

/// The recipe a project copy has beside it: `HASH12_stem.generated.json`.
fn recipe_beside(sample_path: &str) -> String {
    let path = Path::new(sample_path);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    format!("samples/{stem}{}", generate::RECIPE_SUFFIX)
}

pub fn keys(cmd: &KeysCmd) -> Result<Json> {
    let data_dir = project::data_dir();
    let base = generate::base_url();
    match cmd {
        KeysCmd::Set { service } => {
            keys::account(service)?;
            if generate::is_loopback(&base) {
                return Err(
                    "AAW_GENERATE_URL names a stand-in on this machine, whose key is AAW_TEST_GENERATE_KEY; the Keychain is not written".into(),
                );
            }
            generate::keychain_address(&base)?;
            let key = read_key()?;
            keys::write(service, &key)?;
            let mut out = json!({
                "service": service,
                "stored": true,
                "where": format!("the macOS Keychain, service {} account {service}", keys::KEYCHAIN_SERVICE),
            });
            // A wrong key is said at once.
            let client = Client::new(&base, generate::Key::new(&key).expect("checked above"));
            match generate::subscription(&client) {
                Ok(sub) => {
                    generate::update_settings(&data_dir, |s| {
                        s.tier = Some(sub.tier.clone());
                        Ok(())
                    })?;
                    out["subscription"] = sub.json();
                }
                Err(e) => out["problem"] = json!(e),
            }
            Ok(out)
        }
        KeysCmd::Status => {
            let mut settings = generate::load_settings(&data_dir);
            let mut out = json!({
                "service": generate::SERVICE,
                "budget": settings.budget_json(),
                "generated_folder": generate::dir(),
                "settings": generate::settings_path(&data_dir),
            });
            match generate::key(&base) {
                Ok(Some(key)) => {
                    out["key"] = json!(true);
                    match generate::subscription(&Client::new(&base, key)) {
                        Ok(sub) => {
                            if settings.tier.as_deref() != Some(sub.tier.as_str()) {
                                settings = generate::update_settings(&data_dir, |s| {
                                    s.tier = Some(sub.tier.clone());
                                    Ok(s.clone())
                                })?;
                            }
                            out["subscription"] = sub.json();
                        }
                        Err(e) => out["problem"] = json!(e),
                    }
                }
                Ok(None) => {
                    out["key"] = json!(false);
                    out["hint"] = json!(generate::no_key());
                }
                Err(e) => {
                    out["key"] = Json::Null;
                    out["problem"] = json!(e);
                }
            }
            out["tier"] = json!(settings.tier);
            Ok(out)
        }
        KeysCmd::Remove { service } => {
            let removed = keys::remove(service)?;
            generate::update_settings(&data_dir, |s| {
                s.tier = None;
                Ok(())
            })?;
            Ok(json!({"service": service, "removed": removed}))
        }
        KeysCmd::Budget { agent_seconds } => {
            let mut settings = generate::load_settings(&data_dir);
            if let Some(seconds) = agent_seconds {
                if !(0.0..=86_400.0).contains(seconds) {
                    return Err("--agent-seconds must be 0 to 86400".into());
                }
                settings = generate::update_settings(&data_dir, |s| {
                    s.budget_seconds = *seconds;
                    Ok(s.clone())
                })?;
            }
            Ok(json!({"budget": settings.budget_json(), "settings": generate::settings_path(&data_dir)}))
        }
    }
}

/// The key typed on the terminal, without being shown, or the first line of
/// a pipe. It is never an argument, where every process could read it.
fn read_key() -> Result<String> {
    let stdin = std::io::stdin();
    let key = if stdin.is_terminal() {
        rpassword::prompt_password("ElevenLabs API key (not shown): ").map_err(|e| format!("Could not read the key: {e}"))?
    } else {
        let mut line = String::new();
        stdin.lock().read_line(&mut line).map_err(|e| format!("Could not read the key: {e}"))?;
        line
    };
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("No key was given".into());
    }
    Ok(key)
}

impl Cli {
    pub(crate) fn origin(&self) -> Origin {
        match self.origin {
            crate::Who::Agent => Origin::Agent,
            crate::Who::User => Origin::User,
        }
    }
}
