//! Generated audio against a stand-in for the ElevenLabs API on this machine:
//! a sound asked for, saved with its recipe, copied into a project and taken
//! into the song as an edit; the request as the service documents it; a
//! format the plan refuses asked for again one lower; a 429 retried once;
//! each error explained; the subscription read; the budget; and the key in
//! no file and no reply. A song from a plan as a multipart reply, its plan
//! and its channels read, and plans checked and asked for. No request leaves
//! the machine.

mod common;

use aaw_host::command::Origin;
use aaw_host::generate::{self, Client, Format, Key, MusicRequest, Settings, SoundRequest};
use aaw_host::session::Session;
use common::{edit, get, write_song};
use serde_json::{json, Value as Json};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};

const KEY: &str = "sk_test_never_printed_0123456789";

/// A request the stand-in saw.
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Json,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// What the stand-in answers next, in place of its usual reply.
#[derive(Clone, Debug)]
enum Scripted {
    /// A status with a JSON body and headers.
    Refuse(u16, &'static str, Vec<(&'static str, &'static str)>),
    /// Bytes as an MP3 reply.
    Mp3(Vec<u8>),
}

struct StandIn {
    url: String,
    tier: Arc<Mutex<String>>,
    seen: Arc<Mutex<Vec<Seen>>>,
    script: Arc<Mutex<VecDeque<Scripted>>>,
}

/// The boundary of the stand-in's multipart replies.
const BOUNDARY: &str = "aaw-test-boundary-7d1f";

/// Stereo 16-bit PCM at 48 kHz of `seconds`: a click on each beat at
/// `bpm`, louder on the first of each four, the left channel and the right
/// a little apart.
fn stereo_clicks(seconds: f64, bpm: f64) -> Vec<u8> {
    let rate = 48_000.0;
    let frames = (seconds * rate).round() as usize;
    let beat = 60.0 / bpm;
    (0..frames)
        .flat_map(|i| {
            let t = i as f64 / rate;
            let n = (t / beat).floor();
            let since = t - n * beat;
            let level = if n as i64 % 4 == 0 { 0.8 } else { 0.4 };
            let x = if since < 0.02 { level * (2.0 * std::f64::consts::PI * 1000.0 * since).sin() * (1.0 - since / 0.02) } else { 0.0 };
            let l = (x * 32_767.0) as i16;
            let r = (x * 0.9 * 32_767.0) as i16;
            [l.to_le_bytes(), r.to_le_bytes()].concat()
        })
        .collect()
}

/// A `/v1/music/detailed` reply: the JSON part, then the audio.
fn music_reply(json: &Json, audio: &[u8], audio_type: &str) -> Vec<u8> {
    let mut out = format!("--{BOUNDARY}\r\nContent-Type: application/json\r\n\r\n{json}\r\n--{BOUNDARY}\r\nContent-Type: {audio_type}\r\nContent-Disposition: attachment; filename=\"song.raw\"\r\n\r\n").into_bytes();
    out.extend_from_slice(audio);
    out.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    out
}

/// The plan the stand-in says it used: the one sent, or one chunk of the
/// length asked for.
fn plan_used(body: &Json) -> Json {
    match body.get("composition_plan") {
        Some(plan) => plan.clone(),
        None => {
            let ms = body["music_length_ms"].as_i64().unwrap_or(8_000);
            json!({"chunks": [{"text": "[Verse]\n{instrumental}", "duration_ms": ms, "positive_styles": ["soul"], "negative_styles": []}]})
        }
    }
}

/// A 440 Hz sine of `seconds`, as the headerless 16-bit PCM the service sends.
fn pcm(seconds: f64) -> Vec<u8> {
    let frames = (seconds * 44_100.0).round() as usize;
    (0..frames)
        .flat_map(|i| {
            let x = (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 44_100.0).sin() * 0.5;
            ((x * 32_767.0) as i16).to_le_bytes()
        })
        .collect()
}

impl StandIn {
    fn start(tier: &str) -> StandIn {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stand_in = StandIn {
            url,
            tier: Arc::new(Mutex::new(tier.to_string())),
            seen: Arc::new(Mutex::new(Vec::new())),
            script: Arc::new(Mutex::new(VecDeque::new())),
        };
        let (tier, seen, script) = (stand_in.tier.clone(), stand_in.seen.clone(), stand_in.script.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (tier, seen, script) = (tier.clone(), seen.clone(), script.clone());
                std::thread::spawn(move || serve(stream, &tier, &seen, &script));
            }
        });
        stand_in
    }

    fn client(&self) -> Client {
        Client::new(&self.url, Key::new(KEY).unwrap())
    }

    fn next(&self, s: Scripted) {
        self.script.lock().unwrap().push_back(s);
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn serve(stream: TcpStream, tier: &Mutex<String>, seen: &Mutex<Vec<Seen>>, script: &Mutex<VecDeque<Scripted>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let (path, query) = target.split_once('?').map_or((target.as_str(), ""), |(p, q)| (p, q));
    let mut headers = Vec::new();
    let mut length = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            }
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).unwrap();
    let request = Seen {
        method: method.clone(),
        path: path.to_string(),
        query: query.to_string(),
        headers: headers.clone(),
        body: serde_json::from_slice(&body).unwrap_or(Json::Null),
    };
    let authorized = request.header("xi-api-key") == Some(KEY);
    let scripted = script.lock().unwrap().pop_front();
    let (status, extra, bytes): (u16, Vec<(String, String)>, Vec<u8>) = match (authorized, scripted) {
        (false, _) => (401, vec![], br#"{"detail": {"status": "invalid_api_key", "message": "Invalid API key"}}"#.to_vec()),
        (true, Some(Scripted::Refuse(status, body, headers))) => (
            status,
            headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body.as_bytes().to_vec(),
        ),
        (true, Some(Scripted::Mp3(bytes))) => (200, vec![("request-id".into(), "req-mp3".into())], bytes),
        (true, None) if method == "GET" && path == "/v1/user/subscription" => (
            200,
            vec![],
            json!({"tier": *tier.lock().unwrap(), "character_count": 1200, "character_limit": 100000, "next_character_count_reset_unix": 1_761_955_200, "status": "active"})
                .to_string()
                .into_bytes(),
        ),
        (true, None) if method == "POST" && path == "/v1/music/detailed" => {
            let plan = plan_used(&request.body);
            let seconds = generate::plan_seconds(&plan);
            let pcm = query.contains("pcm_48000");
            let audio = if pcm { stereo_clicks(seconds, 120.0) } else { b"ID3-not-really-mp3".to_vec() };
            let meta = json!({"composition_plan": plan, "song_metadata": {"title": "Dusty Soul Ballad", "description": "A slow one", "genres": ["soul"], "languages": [], "is_explicit": false}});
            (
                200,
                vec![
                    ("request-id".into(), "req-music".into()),
                    ("song-id".into(), "song-abc123".into()),
                    ("content-type".into(), format!("multipart/mixed; boundary={BOUNDARY}")),
                ],
                music_reply(&meta, &audio, if pcm { "audio/pcm" } else { "audio/mpeg" }),
            )
        }
        (true, None) if method == "POST" && path == "/v1/music/plan" => {
            let ms = request.body["music_length_ms"].as_i64().unwrap_or(30_000);
            let plan = json!({"chunks": [
                {"text": "[Intro]", "duration_ms": ms / 3, "positive_styles": ["dusty soul", "Rhodes", "brushed drums", "82 bpm"], "negative_styles": ["EDM"], "context_adherence": "high"},
                {"text": "[Verse]\nWe kept the radio low\n{soft}", "duration_ms": ms - ms / 3, "positive_styles": ["warm vocal"], "negative_styles": []}
            ]});
            (200, vec![], plan.to_string().into_bytes())
        }
        (true, None) if method == "POST" && path == "/v1/sound-generation" => {
            let seconds = request.body["duration_seconds"].as_f64().unwrap_or(2.0);
            let n = seen.lock().unwrap().len() + 1;
            (
                200,
                vec![
                    ("request-id".into(), format!("req-{n}")),
                    ("character-cost".into(), format!("{}", (seconds * 40.0).round())),
                    ("content-type".into(), "audio/pcm".into()),
                ],
                pcm(seconds),
            )
        }
        _ => (404, vec![], b"{\"detail\": \"not here\"}".to_vec()),
    };
    seen.lock().unwrap().push(request);
    let mut out = stream;
    let mut head = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n", bytes.len());
    for (k, v) in extra {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = out.write_all(head.as_bytes());
    let _ = out.write_all(&bytes);
    let _ = out.flush();
}

fn request(text: &str, seconds: Option<f64>) -> SoundRequest {
    SoundRequest {
        text: text.into(),
        seconds,
        influence: None,
        looped: false,
        bpm: None,
        bars: None,
    }
}

/// Everything under a folder, as text, for the key to be looked for in.
fn all_text(dir: &Path) -> String {
    let mut out = String::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.push_str(&all_text(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            out.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    out
}

#[test]
fn a_sound_is_asked_for_saved_with_its_recipe_and_taken_into_the_song() {
    let stand_in = StandIn::start("pro");
    let client = stand_in.client();
    let dir = tempfile::tempdir().unwrap();
    let song = write_song(dir.path());
    let mut s = Session::open(&song, true).unwrap();

    let req = request("vinyl crackle, warm", None).with_bars(2.0, 120.0, 4.0).unwrap();
    assert_eq!(req.seconds, Some(4.0));
    let generated = generate::sound(&client, &req, Some("pro")).unwrap();
    assert_eq!(generated.format, Format::Pcm44100);
    assert_eq!(generated.request_id.as_deref(), Some("req-1"));
    assert_eq!(generated.cost, Some(160.0));
    assert_eq!(generated.bytes.len(), 4 * 44_100 * 2);

    // The request as the service documents it.
    let seen = stand_in.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!((seen[0].method.as_str(), seen[0].path.as_str()), ("POST", "/v1/sound-generation"));
    assert_eq!(seen[0].query, "output_format=pcm_44100");
    assert_eq!(seen[0].body["text"], json!("vinyl crackle, warm at 120 BPM"));
    assert_eq!(seen[0].body["duration_seconds"], json!(4.0));
    assert_eq!(seen[0].body["model_id"], json!("eleven_text_to_sound_v2"));
    assert!(seen[0].body.get("loop").is_none());
    assert!(seen[0].header("content-type").unwrap().starts_with("application/json"));

    // Saved as a WAV the engine reads, with the recipe beside it.
    let folder = dir.path().join("workspace/library/generated");
    let saved = generate::save(&generated, &folder, "2026-10-14-120000-crackle", &req, Origin::Agent).unwrap();
    assert_eq!(saved.file, folder.join("2026-10-14-120000-crackle.wav"));
    assert_eq!(saved.recipe_file, folder.join("2026-10-14-120000-crackle.generated.json"));
    assert_eq!((saved.seconds, saved.sample_rate, saved.channels), (4.0, 44_100, 1));
    let info = aaw_engine::sndfile::info(&saved.file).unwrap();
    assert_eq!((info.frames, info.rate, info.channels), (4 * 44_100, 44_100, 1));
    let recipe: Json = serde_json::from_str(&std::fs::read_to_string(&saved.recipe_file).unwrap()).unwrap();
    assert_eq!(recipe["service"], json!("elevenlabs"));
    assert_eq!(recipe["prompt"], json!("vinyl crackle, warm"));
    assert_eq!(recipe["text_sent"], json!("vinyl crackle, warm at 120 BPM"));
    assert_eq!(recipe["parameters"]["duration_seconds"], json!(4.0));
    assert_eq!(recipe["parameters"]["output_format"], json!("pcm_44100"));
    assert_eq!((&recipe["bpm"], &recipe["bars"]), (&json!(120.0), &json!(2.0)));
    assert_eq!(recipe["cost_credits"], json!(160.0));
    assert_eq!(recipe["tier"], json!("pro"));
    assert_eq!(recipe["origin"], json!("agent"));
    assert_eq!(recipe["request_id"], json!("req-1"));
    assert!(recipe["created"].as_str().unwrap().ends_with('Z'));

    // The project's copy, named by its hash, taken into the song as an edit.
    let entry = generate::project_copy(&saved, dir.path()).unwrap();
    let path = entry["path"].as_str().unwrap();
    assert!(path.starts_with("samples/") && path.ends_with("_2026-10-14-120000-crackle.wav"), "{path}");
    assert!(dir.path().join(path).is_file());
    assert_eq!(entry["source"], json!(saved.file));
    let r = edit(&mut s, json!({"op": "apply", "patch": {"samples": {"crackle": entry}}, "label": "Generate crackle"})).unwrap();
    assert_eq!(r["label"], json!("Generate crackle"));
    assert_eq!(get(&s, "samples.crackle.path"), json!(path));
    let listed = generate::list(dir.path(), Some(s.project()));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["sample_id"], json!("crackle"));
    assert_eq!(listed[0]["prompt"], json!("vinyl crackle, warm"));
    assert_eq!(listed[0]["file"], json!(path));
    assert_eq!(listed[0]["generated_file"], json!(saved.file));
    // The project's recipe is beside its copy.
    let stem = Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
    assert!(dir.path().join(format!("samples/{stem}.generated.json")).is_file());

    // The key is in no file and no reply.
    let everything = format!("{}{generated:?}{saved:?}{entry}{r}{client:?}", all_text(dir.path()));
    assert!(!everything.contains(KEY));
    assert!(format!("{client:?}").contains("Key(…)"));
}

#[test]
fn a_format_the_plan_refuses_is_asked_for_one_lower_and_the_tier_corrected() {
    let stand_in = StandIn::start("pro");
    stand_in.next(Scripted::Refuse(
        403,
        r#"{"detail": {"status": "invalid_output_format", "message": "output_format pcm_44100 requires the Pro tier"}}"#,
        vec![],
    ));
    stand_in.next(Scripted::Mp3(b"not-really-mp3".to_vec()));
    let generated = generate::sound(&stand_in.client(), &request("rain on a tent", Some(1.0)), Some("pro")).unwrap();
    assert_eq!(generated.format, Format::Mp3At192);
    assert_eq!(generated.tier.as_deref(), Some("creator"));
    assert_eq!(generated.notes.len(), 1);
    let formats: Vec<String> = stand_in.seen().iter().map(|s| s.query.clone()).collect();
    assert_eq!(formats, ["output_format=pcm_44100", "output_format=mp3_44100_192"]);
    // An MP3 is kept as it came, with its recipe.
    let dir = tempfile::tempdir().unwrap();
    let saved = generate::save(&generated, dir.path(), "rain", &request("rain on a tent", Some(1.0)), Origin::User).unwrap();
    assert_eq!(saved.file, dir.path().join("rain.mp3"));
    assert_eq!(std::fs::read(&saved.file).unwrap(), b"not-really-mp3");
    assert_eq!(saved.recipe["parameters"]["output_format"], json!("mp3_44100_192"));
    assert_eq!(saved.recipe["origin"], json!("user"));
}

#[test]
fn a_429_is_retried_once_after_the_wait_and_a_second_is_the_answer() {
    let stand_in = StandIn::start("pro");
    stand_in.next(Scripted::Refuse(429, r#"{"detail": {"status": "rate_limit_exceeded", "message": "Slow down"}}"#, vec![("retry-after", "0")]));
    let generated = generate::sound(&stand_in.client(), &request("a crowd", Some(1.0)), Some("pro")).unwrap();
    assert_eq!(generated.bytes.len(), 44_100 * 2);
    assert_eq!(stand_in.seen().len(), 2);

    let stand_in = StandIn::start("pro");
    for _ in 0..2 {
        stand_in.next(Scripted::Refuse(
            429,
            r#"{"detail": {"status": "concurrent_limit_exceeded", "message": "Too many at once"}}"#,
            vec![("retry-after", "0"), ("maximum-concurrent-requests", "2")],
        ));
    }
    let e = generate::sound(&stand_in.client(), &request("a crowd", Some(1.0)), Some("pro")).unwrap_err();
    assert!(e.starts_with("Too many requests to ElevenLabs (429 concurrent_limit_exceeded: Too many at once; the plan allows 2 at once)"), "{e}");
    assert_eq!(stand_in.seen().len(), 2);
}

#[test]
fn each_refusal_says_what_happened_and_what_to_do() {
    let stand_in = StandIn::start("creator");
    let say = |status: u16, body: &'static str| {
        stand_in.next(Scripted::Refuse(status, body, vec![]));
        generate::sound(&stand_in.client(), &request("a whoosh", Some(1.0)), Some("creator")).unwrap_err()
    };
    let e = say(401, r#"{"detail": {"status": "invalid_api_key", "message": "Invalid API key"}}"#);
    assert!(e.contains("wrong or expired") && e.contains("daw keys set elevenlabs"), "{e}");
    let e = say(402, r#"{"detail": {"status": "insufficient_credits", "message": "Not enough credits"}}"#);
    assert!(e.contains("No ElevenLabs credits are left") && e.contains("daw keys status"), "{e}");
    // A plan's refusal is not a format's: nothing is asked for again.
    let before = stand_in.seen().len();
    let e = say(403, r#"{"detail": {"status": "subscription_required", "message": "Upgrade"}}"#);
    assert!(e.contains("plan does not include this"), "{e}");
    assert_eq!(stand_in.seen().len(), before + 1);
    let e = say(422, r#"{"detail": [{"loc": ["body", "duration_seconds"], "msg": "ensure this value is less than or equal to 30", "type": "value_error"}]}"#);
    assert!(e.contains("refused a field") && e.contains("less than or equal to 30"), "{e}");
    let e = say(
        400,
        r#"{"detail": {"status": "bad_prompt", "message": "Prompts may not name artists", "suggested_prompt": "a dusty soul ballad with Rhodes"}}"#,
    );
    assert!(e.contains("named something it may not"), "{e}");
    assert!(e.contains("\"a dusty soul ballad with Rhodes\"") && e.contains("not to send unasked"), "{e}");
    // A wrong key is the service's 401, not a guess here.
    let wrong = Client::new(&stand_in.url, Key::new("sk_wrong").unwrap());
    let e = generate::sound(&wrong, &request("a whoosh", Some(1.0)), None).unwrap_err();
    assert!(e.contains("401 invalid_api_key"), "{e}");
}

#[test]
fn the_subscription_names_the_tier_and_the_credits() {
    let stand_in = StandIn::start("creator");
    let sub = generate::subscription(&stand_in.client()).unwrap();
    assert_eq!(sub.tier, "creator");
    assert_eq!((sub.credits_used, sub.credits_limit), (Some(1200.0), Some(100_000.0)));
    assert_eq!(sub.credits_reset.as_deref(), Some("2025-11-01T00:00:00Z"));
    assert_eq!(sub.json()["credits_left"], json!(98_800.0));
    assert_eq!(stand_in.seen()[0].header("xi-api-key"), Some(KEY));
    let wrong = Client::new(&stand_in.url, Key::new("sk_wrong").unwrap());
    assert!(generate::subscription(&wrong).unwrap_err().contains("wrong or expired"));
}

#[test]
fn a_service_that_cannot_be_reached_spends_nothing() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = Client::new(&url, Key::new(KEY).unwrap());
    let e = generate::sound(&client, &request("a whoosh", Some(1.0)), None).unwrap_err();
    assert!(e.starts_with("ElevenLabs could not be reached") && e.ends_with("nothing was spent"), "{e}");
}

#[test]
fn the_request_is_checked_before_anything_is_sent() {
    let stand_in = StandIn::start("pro");
    let client = stand_in.client();
    assert!(generate::sound(&client, &request("x", Some(31.0)), None).unwrap_err().contains("0.5 to 30"));
    assert!(generate::sound(&client, &request("x", Some(0.2)), None).unwrap_err().contains("0.5 to 30"));
    assert!(generate::sound(&client, &request("  ", None), None).unwrap_err().contains("empty"));
    let mut r = request("x", Some(2.0));
    r.influence = Some(1.5);
    assert!(generate::sound(&client, &r, None).unwrap_err().contains("--influence"));
    assert!(stand_in.seen().is_empty());
}

#[test]
fn settings_keep_the_tier_the_budget_and_the_spend_and_never_a_key() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    assert_eq!(generate::load_settings(&data), Settings::default());
    let mut s = generate::load_settings(&data);
    s.tier = Some("creator".into());
    s.budget_seconds = 30.0;
    s.spend(Origin::Agent, 12.5);
    s.spend(Origin::User, 4.0);
    s.folder_registered = true;
    generate::save_settings(&data, &s).unwrap();
    let again = generate::load_settings(&data);
    assert_eq!(again, s);
    assert_eq!(again.left(), 17.5);
    let text = std::fs::read_to_string(generate::settings_path(&data)).unwrap();
    assert!(!text.contains("key"), "{text}");
    // Another day starts the spend over.
    let mut stale: Json = serde_json::from_str(&text).unwrap();
    stale["day"] = json!("2001-01-01");
    std::fs::write(generate::settings_path(&data), stale.to_string()).unwrap();
    let rolled = generate::load_settings(&data);
    assert_eq!(rolled.spent_by(Origin::Agent), 0.0);
    assert_eq!(rolled.tier.as_deref(), Some("creator"));
    assert_eq!(rolled.budget_seconds, 30.0);
}

#[test]
fn an_estimate_sends_nothing() {
    let mut settings = Settings::default();
    settings.tier = Some("pro".into());
    settings.budget_seconds = 10.0;
    let e = request("a riser", None).estimate(&settings, Origin::Agent);
    assert_eq!((&e["seconds"], &e["seconds_assumed"], &e["credits"]), (&json!(30.0), &json!(true), &json!(1200.0)));
    assert_eq!(e["allowed"], json!(false));
    assert_eq!(e["format"], json!("pcm_44100"));
    let e = request("a riser", Some(3.0)).estimate(&settings, Origin::Agent);
    assert_eq!((&e["credits"], &e["allowed"], &e["sent"]), (&json!(120.0), &json!(true), &json!(false)));
}

#[test]
fn the_keychain_key_goes_to_the_service_and_nowhere_else() {
    assert!(generate::keychain_address(generate::DEFAULT_URL).is_ok());
    // Addresses the loopback check does not take as this machine are not the
    // service either, so no key is read for them and nothing is sent.
    for base in ["https://elsewhere.example", "http://127.1:9000", "http://0.0.0.0:9000", "http://localhost.:9000", "https://api.elevenlabs.io.example"] {
        let e = generate::key(base).unwrap_err();
        assert!(e.contains("neither ElevenLabs") && e.contains("nothing was sent"), "{base}: {e}");
    }
}

#[test]
fn a_redirect_is_not_followed_so_the_key_goes_nowhere_else() {
    let stand_in = StandIn::start("pro");
    let elsewhere = StandIn::start("pro");
    let location: &'static str = Box::leak(format!("{}/v1/user/subscription", elsewhere.url).into_boxed_str());
    stand_in.next(Scripted::Refuse(302, "", vec![("location", location)]));
    let e = generate::subscription(&stand_in.client()).unwrap_err();
    assert!(e.contains("302"), "{e}");
    stand_in.next(Scripted::Refuse(303, "", vec![("location", location)]));
    let e = generate::sound(&stand_in.client(), &request("a whoosh", Some(1.0)), Some("pro")).unwrap_err();
    assert!(e.contains("303"), "{e}");
    assert!(elsewhere.seen().is_empty());
}

#[test]
fn a_403_that_does_not_say_why_steps_down_once_and_leaves_the_tier() {
    let stand_in = StandIn::start("pro");
    stand_in.next(Scripted::Refuse(403, r#"{"detail": "Forbidden"}"#, vec![]));
    stand_in.next(Scripted::Mp3(b"not-really-mp3".to_vec()));
    let generated = generate::sound(&stand_in.client(), &request("a hum", Some(1.0)), Some("pro")).unwrap();
    assert_eq!(generated.format, Format::Mp3At192);
    assert_eq!(generated.tier.as_deref(), Some("pro"));
    assert!(generated.notes[0].contains("did not say why"), "{:?}", generated.notes);

    // A second is the answer: three requests are never made for it.
    let stand_in = StandIn::start("pro");
    for _ in 0..2 {
        stand_in.next(Scripted::Refuse(403, r#"{"detail": "Forbidden"}"#, vec![]));
    }
    let e = generate::sound(&stand_in.client(), &request("a hum", Some(1.0)), Some("pro")).unwrap_err();
    assert!(e.contains("plan does not include this"), "{e}");
    let formats: Vec<String> = stand_in.seen().iter().map(|s| s.query.clone()).collect();
    assert_eq!(formats, ["output_format=pcm_44100", "output_format=mp3_44100_192"]);
}

#[test]
fn a_hold_counts_against_requests_meanwhile_and_settles_to_what_came_back() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    generate::update_settings(&data, |s| {
        s.budget_seconds = 10.0;
        Ok(())
    })
    .unwrap();
    let (first, _) = generate::hold(&data, Origin::Agent, 6.0).unwrap();
    let e = generate::hold(&data, Origin::Agent, 6.0).unwrap_err();
    assert!(e.contains("6.0 s of 10 s"), "{e}");
    let settled = first.settle(&data, 3.0, Some("creator".into())).unwrap();
    assert_eq!(settled.spent_by(Origin::Agent), 3.0);
    assert_eq!(settled.tier.as_deref(), Some("creator"));
    // Nothing back gives the hold back.
    let (second, _) = generate::hold(&data, Origin::Agent, 6.0).unwrap();
    assert_eq!(generate::load_settings(&data).spent_by(Origin::Agent), 9.0);
    second.settle(&data, 0.0, None).unwrap();
    assert_eq!(generate::load_settings(&data).spent_by(Origin::Agent), 3.0);
    // A hold from a day since rolled over: only what came back counts, today.
    let (stale, _) = generate::hold(&data, Origin::User, 5.0).unwrap();
    let stale = generate::Hold { day: "2001-01-01".into(), ..stale };
    assert_eq!(stale.settle(&data, 2.0, None).unwrap().spent_by(Origin::User), 7.0);

    // Requests at once: each sees what the others hold.
    generate::update_settings(&data, |s| {
        s.budget_seconds = 13.0;
        Ok(())
    })
    .unwrap();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let data = data.clone();
            std::thread::spawn(move || generate::hold(&data, Origin::Agent, 2.0).is_ok())
        })
        .collect();
    let allowed = threads.into_iter().map(|t| t.join().unwrap()).filter(|ok| *ok).count();
    assert_eq!(allowed, 5);
    assert_eq!(generate::load_settings(&data).spent_by(Origin::Agent), 13.0);
}

fn plan(chunks: Json) -> Json {
    json!({"chunks": chunks})
}

fn chunk(name: &str, ms: i64) -> Json {
    json!({"text": format!("[{name}]\n{{instrumental}}"), "duration_ms": ms, "positive_styles": ["soul", "Rhodes"], "negative_styles": []})
}

fn music_request(plan: Option<Json>, prompt: Option<&str>) -> MusicRequest {
    MusicRequest {
        prompt: prompt.map(str::to_string),
        plan,
        seconds: None,
        instrumental: false,
        seed: None,
        model: generate::MUSIC_MODEL.into(),
    }
}

#[test]
fn a_song_from_a_plan_comes_back_as_multipart_and_is_saved_with_its_plan() {
    let stand_in = StandIn::start("pro");
    let sent = plan(json!([chunk("Intro", 4_000), chunk("Verse", 8_000)]));
    let mut req = music_request(Some(sent.clone()), None);
    req.seed = Some(7);
    assert_eq!(req.seconds_asked(), 12.0);
    let music = generate::music(&stand_in.client(), &req, Some("pro")).unwrap();

    // The request as the reference documents it.
    let seen = stand_in.seen();
    assert_eq!((seen[0].method.as_str(), seen[0].path.as_str()), ("POST", "/v1/music/detailed"));
    assert_eq!(seen[0].query, "output_format=pcm_48000");
    assert_eq!(
        seen[0].body,
        json!({"composition_plan": sent, "seed": 7, "model_id": "music_v2_5", "store_for_inpainting": true})
    );

    // The parts of the reply: the plan used, the title, the stored song and
    // the audio, without the closing delimiter.
    assert_eq!(music.generated.format, Format::Pcm48000);
    assert_eq!(music.song_id.as_deref(), Some("song-abc123"));
    assert_eq!(music.title(), Some("Dusty Soul Ballad"));
    assert_eq!(music.plan, sent);
    assert_eq!(music.generated.bytes.len(), 12 * 48_000 * 4);
    assert_eq!(music.generated.channels, 2);
    assert!(music.notes.is_empty(), "{:?}", music.notes);
    assert_eq!(music.generated.pcm_seconds(), Some(12.0));

    // Saved as a 48 kHz stereo WAV with a recipe that says how it was made.
    let dir = tempfile::tempdir().unwrap();
    let saved = generate::save_music(&music, dir.path(), "2026-10-24-120000-dusty-soul-ballad", &req, Origin::User).unwrap();
    let info = aaw_engine::sndfile::info(&saved.file).unwrap();
    assert_eq!((info.frames, info.rate, info.channels), (12 * 48_000, 48_000, 2));
    assert_eq!((saved.seconds, saved.sample_rate, saved.channels), (12.0, 48_000, 2));
    let recipe = &saved.recipe;
    assert_eq!(recipe["kind"], json!("music"));
    assert_eq!(recipe["endpoint"], json!("/v1/music/detailed"));
    assert_eq!(recipe["model_id"], json!("music_v2_5"));
    assert_eq!(recipe["plan"], sent);
    assert_eq!(recipe["plan_sent"], sent);
    assert_eq!(recipe["title"], json!("Dusty Soul Ballad"));
    assert_eq!(recipe["song_id"], json!("song-abc123"));
    assert_eq!(recipe["parameters"]["seed"], json!(7));
    assert_eq!(recipe["parameters"]["output_format"], json!("pcm_48000"));
    assert_eq!(recipe["parameters"]["store_for_inpainting"], json!(true));
    assert_eq!((&recipe["origin"], &recipe["channels"], &recipe["request_id"]), (&json!("user"), &json!(2), &json!("req-music")));

    // The plan's sections in order, each with where it starts.
    let sections = generate::plan_sections(&sent);
    assert_eq!(sections[1]["name"], json!("Verse"));
    assert_eq!((&sections[1]["start_seconds"], &sections[1]["seconds"]), (&json!(4.0), &json!(8.0)));

    // A plan refused as PCM is asked for as music's MP3, and kept as it came.
    let stand_in = StandIn::start("creator");
    let music = generate::music(&stand_in.client(), &music_request(None, Some("dusty soul, 82 bpm")), Some("creator")).unwrap();
    assert_eq!(stand_in.seen()[0].query, "output_format=mp3_48000_192");
    assert_eq!(stand_in.seen()[0].body, json!({"prompt": "dusty soul, 82 bpm", "model_id": "music_v2_5", "store_for_inpainting": true}));
    assert_eq!(music.generated.bytes, b"ID3-not-really-mp3");
    assert_eq!(Format::for_music_tier(Some("pro")), Format::Pcm48000);
    assert_eq!(Format::Pcm48000.lower(), Some(Format::Mp3At48k192));
    assert_eq!(Format::Mp3At48k192.lower(), Some(Format::Mp3At128));
}

#[test]
fn a_plan_is_asked_for_at_no_cost_and_checked() {
    let stand_in = StandIn::start("pro");
    let made = generate::compose_plan(&stand_in.client(), "dusty soul ballad", Some(30.0), "music_v2_5").unwrap();
    let seen = stand_in.seen();
    assert_eq!((seen[0].path.as_str(), &seen[0].body), ("/v1/music/plan", &json!({"prompt": "dusty soul ballad", "model_id": "music_v2_5", "music_length_ms": 30000})));
    assert_eq!(seen[0].query, "");
    assert_eq!(generate::plan_seconds(&made), 30.0);
    generate::check_plan(&made, "music_v2_5").unwrap();
    // A plan given back as the command's reply is its plan.
    assert_eq!(generate::plan_from(json!({"plan": made.clone(), "seconds": 30})), made);
    assert!(generate::compose_plan(&stand_in.client(), "x", Some(2.0), "music_v2_5").unwrap_err().contains("3 to 300"));
    assert!(generate::compose_plan(&stand_in.client(), "x", None, "music_v9").unwrap_err().contains("--model"));
    assert_eq!(stand_in.seen().len(), 1);
}

#[test]
fn a_plan_that_breaks_a_limit_is_refused_before_anything_is_sent() {
    let refused = |p: Json, model: &str| generate::check_plan(&p, model).unwrap_err();
    let e = refused(plan(json!((0..31).map(|i| chunk(&format!("Part {i}"), 3_000)).collect::<Vec<_>>())), "music_v2_5");
    assert!(e.contains("31 chunks") && e.contains("nothing was sent"), "{e}");
    assert!(refused(plan(json!([chunk("Intro", 2_000)])), "music_v2").contains("chunk 1 is 2000 ms"));
    assert!(refused(plan(json!([chunk("Intro", 121_000)])), "music_v2").contains("3000 to 120000"));
    let long = format!("[Verse]\n{}", "la ".repeat(80));
    let e = refused(plan(json!([{"text": long, "duration_ms": 10_000, "positive_styles": ["pop"]}])), "music_v2_5");
    assert!(e.contains("line of 239 characters"), "{e}");
    let many = format!("[Verse]\n{}", (0..31).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n"));
    assert!(refused(plan(json!([{"text": many, "duration_ms": 10_000, "positive_styles": ["pop"]}])), "music_v2_5").contains("31 lines"));
    // Thirty lines and a name are allowed.
    let thirty = format!("[Verse]\n{}", (0..30).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n"));
    generate::check_plan(&plan(json!([{"text": thirty, "duration_ms": 10_000, "positive_styles": ["pop"]}])), "music_v2_5").unwrap();
    assert!(refused(plan(json!([{"text": "[Verse]", "duration_ms": 10_000}])), "music_v2_5").contains("positive_styles"));
    let e = refused(plan(json!([{"text": "[Verse]", "duration_ms": 10_000, "positive_styles": [], "context_adherence": "total"}])), "music_v2_5");
    assert!(e.contains("context_adherence"), "{e}");
    assert!(refused(plan(json!([{"song_id": "s", "range": {"start_ms": 0, "end_ms": 20}}])), "music_v2_5").contains("at least 50 ms"));
    let e = refused(plan(json!((0..3).map(|i| chunk(&format!("Part {i}"), 120_000)).collect::<Vec<_>>())), "music_v2_5");
    assert!(e.contains("360.0 s") && e.contains("300 s"), "{e}");
    // A plan's shape is the model's.
    let v1 = json!({"positive_global_styles": ["soul"], "negative_global_styles": [], "sections": [
        {"section_name": "Verse", "positive_local_styles": [], "negative_local_styles": [], "duration_ms": 10_000, "lines": ["We kept the radio low"]}
    ]});
    generate::check_plan(&v1, "music_v1").unwrap();
    assert!(refused(v1.clone(), "music_v2_5").contains("--model music_v1"));
    assert!(refused(plan(json!([chunk("Intro", 4_000)])), "music_v1").contains("music_v1 takes sections"));
    // A chunk of a stored song counts its range.
    let kept = plan(json!([{"song_id": "song-abc123", "range": {"start_ms": 0, "end_ms": 30_000}}, chunk("Chorus", 30_000)]));
    generate::check_plan(&kept, "music_v2_5").unwrap();
    assert_eq!(generate::plan_seconds(&kept), 60.0);
    assert_eq!(generate::plan_sections(&kept)[0]["kept_from"], json!("song-abc123"));

    // The request's own fields.
    let check = |r: MusicRequest| r.check().unwrap_err();
    let mut r = music_request(Some(plan(json!([chunk("Intro", 4_000)]))), Some("soul"));
    assert!(check(r.clone()).contains("not both"));
    r.prompt = None;
    r.seconds = Some(30.0);
    assert!(check(r.clone()).contains("--seconds is for a prompt"));
    r.seconds = None;
    r.instrumental = true;
    assert!(check(r).contains("--instrumental is for a prompt"));
    let mut r = music_request(None, Some("soul"));
    r.seed = Some(1);
    assert!(check(r.clone()).contains("--seed is for a plan"));
    r.seed = None;
    r.seconds = Some(301.0);
    assert!(check(r.clone()).contains("3 to 300"));
    r.seconds = None;
    assert_eq!(r.seconds_asked(), 300.0);
    assert!(check(music_request(None, None)).contains("daw generate plan"));
}

#[test]
fn a_refused_prompt_or_plan_gives_its_suggestion_as_text() {
    let stand_in = StandIn::start("pro");
    stand_in.next(Scripted::Refuse(
        400,
        r#"{"detail": {"status": "bad_prompt", "message": "Prompts may not name artists", "data": {"prompt_suggestion": "a slow soul ballad with Rhodes"}}}"#,
        vec![],
    ));
    let e = generate::music(&stand_in.client(), &music_request(None, Some("like Marvin Gaye")), Some("pro")).unwrap_err();
    assert!(e.contains("The prompt named something it may not") && e.contains("\"a slow soul ballad with Rhodes\"") && e.contains("not to send unasked"), "{e}");
    stand_in.next(Scripted::Refuse(
        400,
        r#"{"detail": {"status": "bad_composition_plan", "message": "Styles name an artist", "data": {"composition_plan_suggestion": {"chunks": [{"text": "[Verse]", "duration_ms": 4000, "positive_styles": ["soul"]}]}}}}"#,
        vec![],
    ));
    let e = generate::music(&stand_in.client(), &music_request(Some(plan(json!([chunk("Intro", 4_000)]))), None), Some("pro")).unwrap_err();
    assert!(e.contains("The plan named something it may not") && e.contains("suggests this plan") && e.contains("positive_styles"), "{e}");
    // A bad plan is no format's refusal: one request each.
    assert_eq!(stand_in.seen().len(), 2);
}

#[test]
fn a_multipart_reply_is_split_on_its_boundary() {
    let audio = b"\x00\x01\r\n--not-it\r\n\x02\x03".to_vec();
    let body = music_reply(&json!({"song_metadata": {"title": "T"}}), &audio, "audio/pcm");
    let parts = generate::multipart(Some(&format!("multipart/mixed; boundary=\"{BOUNDARY}\"")), &body).unwrap();
    assert_eq!(parts.json.unwrap()["song_metadata"]["title"], json!("T"));
    assert_eq!(parts.audio, audio);
    assert_eq!((parts.audio_type.as_deref(), parts.filename.as_deref()), (Some("audio/pcm"), Some("song.raw")));
    // No boundary in the content type: the first line names it, and bare
    // line feeds end lines.
    let lf = format!("--b1\nContent-Type: application/json\n\n{{\"a\": 1}}\n--b1\nContent-Type: audio/mpeg\n\nMP3BYTES\n--b1--\n");
    let parts = generate::multipart(None, lf.as_bytes()).unwrap();
    assert_eq!(parts.json.unwrap(), json!({"a": 1}));
    assert_eq!(parts.audio, b"MP3BYTES");
    // A reply that is not multipart is the audio alone.
    let parts = generate::multipart(Some("audio/mpeg"), b"MP3").unwrap();
    assert!(parts.json.is_none());
    assert_eq!(parts.audio, b"MP3");
    assert!(generate::multipart(Some("multipart/mixed; boundary=zz"), b"--zz\r\nnothing after").is_err());
}

#[test]
fn pcm_channels_follow_the_length() {
    let rate = 48_000;
    assert_eq!(generate::pcm_channels(10 * 48_000 * 4, rate, Some(10.0)), (2, None));
    assert_eq!(generate::pcm_channels(10 * 48_000 * 2, rate, Some(10.0)), (1, None));
    let (channels, note) = generate::pcm_channels(10 * 48_000 * 4, rate, Some(30.0));
    assert_eq!(channels, 1);
    assert!(note.unwrap().contains("30.0 s was expected"));
    let (channels, note) = generate::pcm_channels(1000, rate, None);
    assert_eq!(channels, 2);
    assert!(note.unwrap().contains("stereo"));
}
