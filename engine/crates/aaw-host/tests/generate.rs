//! Generated audio against a stand-in for the ElevenLabs API on this machine:
//! a sound asked for, saved with its recipe, copied into a project and taken
//! into the song as an edit; the request as the service documents it; a
//! format the plan refuses asked for again one lower; a 429 retried once;
//! each error explained; the subscription read; the budget; and the key in
//! no file and no reply. No request leaves the machine.

mod common;

use aaw_host::command::Origin;
use aaw_host::generate::{self, Client, Format, Key, Settings, SoundRequest};
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
