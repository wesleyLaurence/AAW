//! `daw generate` and `daw keys`: a sound, a loop or a song from a description
//! or a composition plan through the ElevenLabs API at the person's own key,
//! saved in the workspace's generated folder and imported into the project as
//! one undoable edit, a song with its beat map; composition plans; the
//! key in the Keychain, the plan's tier and credits, and the agent's daily
//! budget. The request, the files and the budget are `aaw_host::generate`;
//! this is the command shell around them.

use crate::{files, route, text, Cli, GenerateCmd, KeysCmd, Song};
use aaw_host::command::{Command, Origin};
use aaw_host::generate::{self, Client, MusicRequest, Saved, Settings, SoundRequest};
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
        GenerateCmd::Music {
            project,
            prompt,
            seconds,
            instrumental,
            plan,
            seed,
            model,
            id,
            estimate,
        } => {
            let plan = plan.as_deref().map(read_plan).transpose()?;
            let request = MusicRequest {
                prompt: prompt.clone(),
                plan,
                seconds: *seconds,
                instrumental: *instrumental,
                seed: *seed,
                model: model.clone().unwrap_or_else(|| generate::MUSIC_MODEL.to_string()),
            };
            music(cli, project, request, id.as_deref(), *estimate)
        }
        GenerateCmd::Plan { prompt, seconds, model, out } => plan(prompt, *seconds, model.as_deref().unwrap_or(generate::MUSIC_MODEL), out.as_deref()),
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
    let settings = hold.settle(&data_dir, spent, tier)?;
    let saved = saved?;

    let imported = import(cli, project, &song, &project_dir, &saved, &id, "sound")?;
    let settings = scan(&data_dir, settings, &mut notes)?;
    let mut out = json!({
        "sample_id": id,
        "path": imported.path,
        "seconds": (imported.seconds * 1000.0).round() / 1000.0,
        "sample_rate": saved.sample_rate,
        "channels": saved.channels,
        "format": generated.format.name(),
        "bpm": request.bpm,
        "bars": request.bars,
        "loop": request.looped,
        "text_sent": request.sent_text(),
        "recipe": imported.recipe,
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
    if let Some(revision) = imported.revision {
        out["revision"] = revision;
    }
    if !notes.is_empty() {
        out["notes"] = json!(notes);
    }
    Ok(out)
}

/// What the project's import made of a generated file.
struct Imported {
    path: String,
    recipe: String,
    seconds: f64,
    revision: Option<Json>,
}

/// The project's copy of a saved file, taken into the song as the sample
/// `id` by one edit, `Generate ID`. A WAV is copied as `samples import`
/// names a copy; a compressed file is decoded into the project by the same
/// import a person's file goes through, and taken into the song as a WAV is.
fn import(cli: &Cli, project: &Song, song: &Path, project_dir: &Path, saved: &Saved, id: &str, what: &str) -> Result<Imported> {
    let not_imported = |e: String| format!("The {what} was generated and saved as {} but not imported: {e}", saved.file.display());
    let mut seconds = saved.seconds;
    let is_wav = saved.file.extension().is_some_and(|e| e == "wav");
    let (entry, recipe) = if is_wav {
        let entry = generate::project_copy(saved, project_dir)?;
        let recipe = recipe_beside(entry["path"].as_str().unwrap_or_default());
        (entry, recipe)
    } else {
        let mut copied = python::run(
            "samples",
            &["import", &saved.file.to_string_lossy(), "--project", &song.to_string_lossy(), "--copy-only"],
        )
        .map_err(not_imported)?;
        let path = copied["path"].as_str().unwrap_or_default().to_string();
        let recipe_file = generate::project_recipe(saved, project_dir, &path)?;
        let recipe = format!("samples/{}", recipe_file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        if let Some(entry) = copied.as_object_mut() {
            // The decoder says the length where libsndfile could not.
            if let Some(decoded) = entry.remove("decoded").and_then(|d| d["duration"].as_f64()).filter(|_| seconds <= 0.0) {
                seconds = decoded;
            }
        }
        (copied, recipe)
    };
    let path = entry["path"].as_str().unwrap_or_default().to_string();
    let patch = json!({"samples": {id: entry}});
    let reply = route(cli, project, Command::Apply { patch, label: Some(format!("Generate {id}")) }, None).map_err(not_imported)?;
    Ok(Imported {
        path,
        recipe,
        seconds,
        revision: reply.get("revision").cloned(),
    })
}

/// The library's search reads its index, so what was generated is scanned in
/// now; the scan is incremental and registers the folder the first time.
fn scan(data_dir: &Path, settings: Settings, notes: &mut Vec<String>) -> Result<Settings> {
    let folder = generate::dir();
    match python::run("samples", &["scan", &folder.to_string_lossy()]) {
        Ok(_) if !settings.folder_registered => generate::update_settings(data_dir, |s| {
            s.folder_registered = true;
            Ok(s.clone())
        }),
        Ok(_) => Ok(settings),
        Err(e) => {
            notes.push(format!(
                "The generated folder {} was not scanned into the sample library ({e}); `daw samples folders add` that folder adds it",
                folder.display()
            ));
            Ok(settings)
        }
    }
}

/// The key, the plan's tier when it is not yet known, and the sound.
fn ask(base: &str, data_dir: &Path, request: &SoundRequest, tier: Option<String>, notes: &mut Vec<String>) -> Result<generate::Generated> {
    let (client, tier) = client_and_tier(base, data_dir, tier, notes, "the sound is asked for as MP3 at 128 kb/s")?;
    generate::sound(&client, request, tier.as_deref())
}

/// A client with the key, and the plan's tier, read once when not yet known.
fn client_and_tier(base: &str, data_dir: &Path, mut tier: Option<String>, notes: &mut Vec<String>, fallback: &str) -> Result<(Client, Option<String>)> {
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
            Err(e) => notes.push(format!("The plan's tier could not be read ({e}); {fallback}")),
        }
    }
    Ok((client, tier))
}

/// `daw generate music`: a song from a prompt or a plan, saved and imported
/// as a sound is, and its beat map measured so it can be cut on its bars.
fn music(cli: &Cli, project: &Song, request: MusicRequest, id: Option<&str>, estimate: bool) -> Result<Json> {
    let origin = cli.origin();
    let song = files(project);
    let p = aaw_model::load(&song, false).map_err(text)?;
    let project_dir = song.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
    request.check()?;
    if let Some(id) = id.filter(|id| p.samples.contains_key(*id)) {
        return Err(format!("Sample ID already exists: {id}"));
    }
    let taken = |candidate: &str| p.samples.contains_key(candidate);
    let data_dir = project::data_dir();
    let base = generate::base_url();
    if estimate {
        let settings = generate::load_settings(&data_dir);
        let mut out = request.estimate(&settings, origin);
        out["sample_id"] = json!(id.map_or_else(|| generate::default_id(request.prompt.as_deref().unwrap_or("music"), taken), str::to_string));
        out["key"] = json!(generate::key(&base).ok().flatten().is_some());
        return Ok(out);
    }
    let asked = request.seconds_asked();
    let (hold, held) = generate::hold(&data_dir, origin, asked).map_err(|e| match request.seconds_named() {
        None => format!("{e}. With no --seconds the model picks the length, so {:.0} s is held until it says; give --seconds to hold less", generate::MUSIC_MAX_SECONDS),
        Some(_) => e,
    })?;
    let mut notes: Vec<String> = Vec::new();
    let made = client_and_tier(&base, &data_dir, held.tier.clone(), &mut notes, "the song is asked for as MP3 at 48 kHz and 192 kb/s")
        .and_then(|(client, tier)| generate::music(&client, &request, tier.as_deref()));
    let made = match made {
        Ok(made) => made,
        Err(e) => {
            let _ = hold.settle(&data_dir, 0.0, None);
            return Err(e);
        }
    };
    notes.extend(made.generated.notes.iter().cloned());
    notes.extend(made.notes.iter().cloned());
    let id = match id {
        Some(id) => id.to_string(),
        None => generate::default_id(made.title().or(request.prompt.as_deref()).unwrap_or("music"), taken),
    };
    let folder = generate::dir();
    let saved = generate::save_music(&made, &folder, &generate::file_stem(&id), &request, origin);
    let spent = match &saved {
        Ok(saved) if saved.seconds > 0.0 => saved.seconds,
        _ => made.generated.pcm_seconds().unwrap_or(asked),
    };
    let tier = made.generated.tier.clone().filter(|t| held.tier.as_ref() != Some(t));
    let settings = hold.settle(&data_dir, spent, tier)?;
    let saved = saved?;
    let imported = import(cli, project, &song, &project_dir, &saved, &id, "song")?;
    let settings = scan(&data_dir, settings, &mut notes)?;

    // The plan the service used, or the one sent, in seconds; then in bars
    // once the beat map is measured.
    let plan = if made.plan.is_null() { request.plan.clone().unwrap_or(Json::Null) } else { made.plan.clone() };
    let mut sections = generate::plan_sections(&plan);
    let copy = project_dir.join(&imported.path);
    let copy = copy.canonicalize().unwrap_or(copy);
    let beat_map = match python::run("samples", &["beats", &copy.to_string_lossy(), "--meter", &p.session.time_signature, "--all"]) {
        Ok(report) => Some(beat_summary(&report, &mut sections)),
        Err(e) => {
            notes.push(format!("The beat map could not be measured ({e}); daw samples beats PATH measures it"));
            None
        }
    };
    let mut out = json!({
        "sample_id": id,
        "path": imported.path,
        "seconds": (imported.seconds * 1000.0).round() / 1000.0,
        "sample_rate": saved.sample_rate,
        "channels": saved.channels,
        "format": made.generated.format.name(),
        "model_id": request.model,
        "title": made.metadata["title"],
        "genres": made.metadata["genres"],
        "is_explicit": made.metadata["is_explicit"],
        "song_id": made.song_id,
        "sections": sections,
        "beat_map": beat_map,
        "recipe": imported.recipe,
        "generated_file": saved.file,
        "generated_recipe": saved.recipe_file,
        "request_id": made.generated.request_id,
        "cost_credits": made.generated.cost,
        "tier": settings.tier,
        "origin": origin.as_str(),
        "budget": settings.budget_json(),
        "placed": false,
        "next": "Nothing is placed. Cut it on its bars: daw audio add PROJECT TRACK ID --at BEAT --source-start-seconds S --source-end-seconds E, with S and E from beat_map.bars (bar N starts at bars[N-1]); --source-bpm with beat_map.tempo and --stretch preserve_pitch fits it to the song's tempo. daw samples beats PATH --near T lists the beats about a time",
    });
    if let Some(revision) = imported.revision {
        out["revision"] = revision;
    }
    if !notes.is_empty() {
        out["notes"] = json!(notes);
    }
    Ok(out)
}

/// What the reply says of a beat map: its tempo, downbeat and phrases, and
/// every bar's start in seconds; each section is given the bar nearest its
/// start.
fn beat_summary(report: &Json, sections: &mut Json) -> Json {
    let bars: Vec<f64> = report["beats"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["beat"].as_i64() == Some(1) && b["bar"].as_i64().is_some_and(|n| n >= 1))
        .filter_map(|b| b["seconds"].as_f64())
        .map(|t| (t * 1000.0).round() / 1000.0)
        .collect();
    if let Some(rows) = sections.as_array_mut() {
        for row in rows {
            let Some(start) = row["start_seconds"].as_f64() else { continue };
            let nearest = bars.iter().enumerate().min_by(|a, b| (a.1 - start).abs().total_cmp(&(b.1 - start).abs()));
            if let Some((i, at)) = nearest {
                row["bar"] = json!(i + 1);
                row["bar_seconds"] = json!(at);
                row["off_seconds"] = json!(((start - at) * 1000.0).round() / 1000.0);
            }
        }
    }
    json!({
        "tempo": report["tempo"],
        "meter": report["meter"],
        "beats_per_bar": report["beats_per_bar"],
        "first_downbeat_seconds": report["first_downbeat_seconds"],
        "downbeat_confidence": report["downbeat"]["confidence"],
        "beat_count": report["beat_count"],
        "bars": bars,
        "phrases": report["phrases"],
        "source": report["source"],
    })
}

/// `daw generate plan`: a plan from a prompt, at no cost, with its sections
/// and whether `daw generate music` would take it as it is.
fn plan(prompt: &str, seconds: Option<f64>, model: &str, out: Option<&Path>) -> Result<Json> {
    let base = generate::base_url();
    let key = generate::key(&base)?.ok_or_else(generate::no_key)?;
    let plan = generate::compose_plan(&Client::new(&base, key), prompt, seconds, model)?;
    if let Some(out) = out {
        let text = serde_json::to_string_pretty(&plan).expect("json");
        std::fs::write(out, text + "\n").map_err(|e| format!("{}: {e}", out.display()))?;
    }
    let mut reply = json!({
        "plan": plan,
        "model_id": model,
        "seconds": generate::plan_seconds(&plan),
        "sections": generate::plan_sections(&plan),
        "out": out,
        "cost": "none: a plan costs no credits and is not counted in the budget",
        "next": "Edit the chunks (each a [Section] name with its lyrics and {directions} in text, duration_ms, positive_styles and negative_styles), then daw generate music PROJECT --plan FILE",
    });
    if let Err(e) = generate::check_plan(&plan, model) {
        reply["problem"] = json!(e);
    }
    Ok(reply)
}

/// A plan from a file: the plan itself, or a `daw generate plan` reply.
fn read_plan(path: &Path) -> Result<Json> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let json: Json = serde_json::from_str(&text).map_err(|e| format!("{} is not JSON: {e}", path.display()))?;
    Ok(generate::plan_from(json))
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
