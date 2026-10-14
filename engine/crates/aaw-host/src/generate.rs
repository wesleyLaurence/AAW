//! Generated audio: a sound or a loop made from a description by the
//! ElevenLabs API, at the person's own key, saved in the workspace's generated
//! folder (`~/Music/AAW/library/generated/`, or under `AAW_WORKSPACE`) with
//! its recipe beside it, and copied into a project as a sample. The service
//! is a source of material, never the product: the command places nothing.
//!
//! The key is read by `keys` for the one request and never printed, logged
//! or written to a file; `Key` has no `Display` and a redacted `Debug`. The
//! settings file in the data folder, `generate.json`, holds the plan's tier,
//! the agent's daily budget in seconds of audio and what each origin spent
//! today, and never the key. `AAW_GENERATE_URL` points the client at a
//! stand-in server for tests; only when it names a loopback address is the
//! key taken from `AAW_TEST_GENERATE_KEY` instead of the Keychain.

use crate::command::Origin;
use crate::patches::{slug, timestamp};
use serde_json::{json, Map, Value as Json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

type Result<T> = std::result::Result<T, String>;

pub const SERVICE: &str = "elevenlabs";
pub const DEFAULT_URL: &str = "https://api.elevenlabs.io";
pub const SOUND_MODEL: &str = "eleven_text_to_sound_v2";
/// What a second of a sound costs when a length is given, by the documentation.
pub const SOUND_CREDITS_PER_SECOND: f64 = 40.0;
pub const SOUND_MIN_SECONDS: f64 = 0.5;
pub const SOUND_MAX_SECONDS: f64 = 30.0;
/// The seconds the agent may generate in a day without asking, unless set.
pub const DEFAULT_BUDGET_SECONDS: f64 = 120.0;
pub const SETTINGS_FILE: &str = "generate.json";
/// The suffix of a recipe, beside its audio.
pub const RECIPE_SUFFIX: &str = ".generated.json";
/// The most a reply may be: a minute of 48 kHz stereo PCM is 11.5 MB.
const REPLY_LIMIT: u64 = 256 * 1024 * 1024;

/// The service's key. It prints as nothing.
#[derive(Clone)]
pub struct Key(String);

impl Key {
    pub fn new(text: &str) -> Option<Key> {
        let text = text.trim();
        (!text.is_empty()).then(|| Key(text.to_string()))
    }

    fn header(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(…)")
    }
}

/// The service's address: `AAW_GENERATE_URL`, or the real one.
pub fn base_url() -> String {
    std::env::var("AAW_GENERATE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .unwrap_or_else(|| DEFAULT_URL.to_string())
}

/// Whether an address names this machine: `127.0.0.1`, `localhost` or `::1`.
pub fn is_loopback(url: &str) -> bool {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?']).next().unwrap_or("");
    let host = host.rsplit_once(':').map_or(host, |(h, port)| if port.chars().all(|c| c.is_ascii_digit()) { h } else { host });
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host == "localhost" || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// The key for `base`: from `AAW_TEST_GENERATE_KEY` when the address is a
/// loopback stand-in, and from the Keychain otherwise. None when none is set.
pub fn key(base: &str) -> Result<Option<Key>> {
    if is_loopback(base) {
        return Ok(std::env::var("AAW_TEST_GENERATE_KEY").ok().and_then(|k| Key::new(&k)));
    }
    crate::keys::read()
}

/// What the person is told when there is no key.
pub fn no_key() -> String {
    format!("No ElevenLabs key is set: enter one with `daw keys set {SERVICE}` (it is kept in the macOS Keychain and never shown); generating needs a paid plan for music and credits for sounds")
}

// ---------------------------------------------------------------- settings

/// `generate.json` in the data folder: the tier, the budget, today's spend by
/// origin, and whether the generated folder is in the sample library.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub tier: Option<String>,
    pub budget_seconds: f64,
    /// The UTC day the spend is of, `2026-10-14`.
    pub day: String,
    /// Seconds of audio generated today, by origin (`agent`, `user`).
    pub spent: Map<String, Json>,
    pub folder_registered: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            tier: None,
            budget_seconds: DEFAULT_BUDGET_SECONDS,
            day: today(),
            spent: Map::new(),
            folder_registered: false,
        }
    }
}

/// Today's date in UTC, `2026-10-14`, from the same clock as a recipe's date.
pub fn today() -> String {
    timestamp().chars().take(10).collect()
}

pub fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SETTINGS_FILE)
}

pub fn load_settings(data_dir: &Path) -> Settings {
    let Ok(text) = std::fs::read_to_string(settings_path(data_dir)) else {
        return Settings::default();
    };
    let Ok(j) = serde_json::from_str::<Json>(&text) else {
        return Settings::default();
    };
    let mut s = Settings {
        tier: j["tier"].as_str().map(str::to_string),
        budget_seconds: j["budget_seconds"].as_f64().unwrap_or(DEFAULT_BUDGET_SECONDS),
        day: j["day"].as_str().unwrap_or("").to_string(),
        spent: j["spent"].as_object().cloned().unwrap_or_default(),
        folder_registered: j["folder_registered"].as_bool().unwrap_or(false),
    };
    s.roll_day();
    s
}

pub fn save_settings(data_dir: &Path, s: &Settings) -> Result<()> {
    std::fs::create_dir_all(data_dir).map_err(|e| format!("{}: {e}", data_dir.display()))?;
    let path = settings_path(data_dir);
    let text = serde_json::to_string_pretty(&s.json()).expect("json");
    write_file(&path, text.as_bytes())
}

impl Settings {
    /// A new day starts the spend over.
    fn roll_day(&mut self) {
        let now = today();
        if self.day != now {
            self.day = now;
            self.spent = Map::new();
        }
    }

    pub fn spent_by(&self, origin: Origin) -> f64 {
        self.spent.get(origin.as_str()).and_then(Json::as_f64).unwrap_or(0.0)
    }

    /// What the agent may still generate today.
    pub fn left(&self) -> f64 {
        (self.budget_seconds - self.spent_by(Origin::Agent)).max(0.0)
    }

    pub fn spend(&mut self, origin: Origin, seconds: f64) {
        self.roll_day();
        let total = self.spent_by(origin) + seconds;
        self.spent.insert(origin.as_str().into(), json!(round3(total)));
    }

    /// Refuses the agent what its budget does not cover; the person is not
    /// counted. Nothing is sent after a refusal.
    pub fn allow(&self, origin: Origin, seconds: f64) -> Result<()> {
        if origin != Origin::Agent {
            return Ok(());
        }
        let spent = self.spent_by(Origin::Agent);
        if spent + seconds <= self.budget_seconds + 1e-9 {
            return Ok(());
        }
        Err(if self.budget_seconds <= 0.0 {
            "The agent's daily budget for generated audio is 0 s, so each request is the person's to make: ask them, and they run it with --origin user or raise the budget with `daw keys budget --agent-seconds N`".to_string()
        } else {
            format!(
                "The agent's daily budget for generated audio is spent: {:.1} s of {:.0} s today, and this request asks for {:.1} s. Tell the person; they raise it with `daw keys budget --agent-seconds N` or make the request themselves with --origin user",
                spent, self.budget_seconds, seconds
            )
        })
    }

    /// The budget as a reply shows it.
    pub fn budget_json(&self) -> Json {
        json!({
            "agent_seconds_per_day": self.budget_seconds,
            "spent_today": {"agent": round3(self.spent_by(Origin::Agent)), "user": round3(self.spent_by(Origin::User))},
            "left_today": round3(self.left()),
            "day": self.day,
        })
    }

    fn json(&self) -> Json {
        json!({
            "tier": self.tier,
            "budget_seconds": self.budget_seconds,
            "day": self.day,
            "spent": self.spent,
            "folder_registered": self.folder_registered,
        })
    }
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

// ---------------------------------------------------------------- formats

/// What the audio is asked for as. The plan decides what is allowed: PCM at
/// 44.1 kHz needs Pro, MP3 at 192 kb/s needs Creator, and MP3 at 128 kb/s is
/// everyone's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pcm44100,
    Mp3At192,
    Mp3At128,
}

impl Format {
    /// The `output_format` query parameter.
    pub fn name(self) -> &'static str {
        match self {
            Format::Pcm44100 => "pcm_44100",
            Format::Mp3At192 => "mp3_44100_192",
            Format::Mp3At128 => "mp3_44100_128",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Pcm44100 => "wav",
            Format::Mp3At192 | Format::Mp3At128 => "mp3",
        }
    }

    pub fn sample_rate(self) -> u32 {
        44_100
    }

    /// The next format down, when the plan refused this one.
    pub fn lower(self) -> Option<Format> {
        match self {
            Format::Pcm44100 => Some(Format::Mp3At192),
            Format::Mp3At192 => Some(Format::Mp3At128),
            Format::Mp3At128 => None,
        }
    }

    /// The best format a tier allows; MP3 at 128 when the tier is unknown.
    pub fn for_tier(tier: Option<&str>) -> Format {
        match tier.map(|t| t.to_ascii_lowercase()).as_deref() {
            Some("pro" | "scale" | "business" | "enterprise") => Format::Pcm44100,
            Some("creator") => Format::Mp3At192,
            _ => Format::Mp3At128,
        }
    }

    /// The tier a refusal of this format says the plan is at most.
    fn tier_below(self) -> &'static str {
        match self {
            Format::Pcm44100 => "creator",
            Format::Mp3At192 | Format::Mp3At128 => "starter",
        }
    }

    pub fn from_name(name: &str) -> Option<Format> {
        [Format::Pcm44100, Format::Mp3At192, Format::Mp3At128].into_iter().find(|f| f.name() == name)
    }
}

// ---------------------------------------------------------------- the client

/// One reply of the service: its status, headers and body.
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn json(&self) -> Json {
        serde_json::from_slice(&self.body).unwrap_or(Json::Null)
    }

    /// The error a failed reply carries: its code, its message and, for a
    /// refused prompt, the prompt the service suggests. The shapes seen in
    /// the service's documentation are `{detail: {status, message}}`,
    /// `{detail: "text"}`, `{detail: [{msg}]}` and `{code, message}`.
    fn failure(&self) -> Failure {
        let j = self.json();
        let detail = &j["detail"];
        let text = |v: &Json| v.as_str().map(str::to_string);
        let code = text(&detail["status"])
            .or_else(|| text(&detail["code"]))
            .or_else(|| text(&j["code"]))
            .or_else(|| text(&j["type"]))
            .or_else(|| text(&detail["type"]));
        let message = text(&detail["message"])
            .or_else(|| text(detail))
            .or_else(|| detail.as_array().and_then(|a| a.first()).and_then(|d| text(&d["msg"])))
            .or_else(|| text(&j["message"]))
            .or_else(|| text(&j["error"]))
            .unwrap_or_else(|| match String::from_utf8_lossy(&self.body).trim() {
                "" => format!("status {}", self.status),
                body => body.chars().take(300).collect(),
            });
        let suggestion = text(&detail["suggested_prompt"])
            .or_else(|| text(&detail["data"]["suggested_prompt"]))
            .or_else(|| text(&j["suggested_prompt"]));
        Failure {
            status: self.status,
            code,
            message,
            suggestion,
            retry_after: self.header("retry-after").and_then(|s| s.trim().parse::<f64>().ok()),
            at_most: self.header("maximum-concurrent-requests").map(str::to_string),
        }
    }
}

/// A reply the service refused, read for what to tell the person.
#[derive(Clone, Debug)]
pub struct Failure {
    pub status: u16,
    pub code: Option<String>,
    pub message: String,
    pub suggestion: Option<String>,
    pub retry_after: Option<f64>,
    pub at_most: Option<String>,
}

impl Failure {
    fn is_bad_prompt(&self) -> bool {
        self.code.as_deref() == Some("bad_prompt") || self.message.to_ascii_lowercase().contains("bad_prompt")
    }

    /// Whether a refusal is of the format asked for, as the plan's tier
    /// decides it.
    fn is_format(&self, format: Format) -> bool {
        let m = self.message.to_ascii_lowercase();
        let plan = matches!(
            self.code.as_deref(),
            Some("invalid_api_key" | "missing_permissions" | "subscription_required" | "quota_exceeded" | "insufficient_credits" | "bad_prompt")
        );
        !plan && (self.status == 403 || m.contains("output_format") || m.contains(format.name()))
    }

    /// What happened and what to do, as the table in the feature's file says.
    pub fn explain(&self) -> String {
        let code = self.code.as_deref().unwrap_or("");
        let said = if code.is_empty() {
            format!("{}: {}", self.status, self.message)
        } else {
            format!("{} {}: {}", self.status, code, self.message)
        };
        if self.is_bad_prompt() {
            let suggested = match &self.suggestion {
                Some(s) => format!(" The service suggests this prompt instead, which is text to offer the person and not to send unasked: {s:?}."),
                None => String::new(),
            };
            return format!("The prompt named something it may not, such as an artist, a song or a label ({said}).{suggested}");
        }
        match self.status {
            401 => format!("The ElevenLabs key is wrong or expired ({said}). Enter it again with `daw keys set {SERVICE}`"),
            402 => format!("No ElevenLabs credits are left ({said}); `daw keys status` says when they reset. Nothing more is generated until then"),
            403 => format!("The ElevenLabs plan does not include this, or the key is limited and not allowed this endpoint ({said})"),
            429 => {
                let at_most = self.at_most.as_deref().map_or(String::new(), |n| format!("; the plan allows {n} at once"));
                format!("Too many requests to ElevenLabs ({said}{at_most}). One retry was made after the wait it gave; wait and ask again")
            }
            422 => format!("ElevenLabs refused a field of the request ({said})"),
            _ => format!("ElevenLabs refused the request ({said})"),
        }
    }
}

/// The client: the address, the key and a blocking HTTP agent.
pub struct Client {
    base: String,
    key: Key,
    agent: ureq::Agent,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("base", &self.base).field("key", &self.key).finish_non_exhaustive()
    }
}

impl Client {
    pub fn new(base: &str, key: Key) -> Client {
        let mut config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_global(Some(Duration::from_secs(300)))
            .user_agent("aaw-daw");
        if is_loopback(base) {
            // A stand-in on this machine is reached directly, whatever the
            // environment says about proxies.
            config = config.proxy(None);
        }
        Client {
            base: base.trim_end_matches('/').to_string(),
            key,
            agent: config.build().new_agent(),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn read(&self, result: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Reply> {
        let mut response = result.map_err(|e| unreachable_service(&e))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response
            .body_mut()
            .with_config()
            .limit(REPLY_LIMIT)
            .read_to_vec()
            .map_err(|e| format!("ElevenLabs's reply could not be read ({e})"))?;
        Ok(Reply { status, headers, body })
    }

    fn get(&self, path: &str) -> Result<Reply> {
        let url = format!("{}{path}", self.base);
        self.read(self.agent.get(&url).header("xi-api-key", self.key.header()).call())
    }

    fn post(&self, path: &str, query: &[(&str, &str)], body: &Json) -> Result<Reply> {
        let url = format!("{}{path}", self.base);
        let mut request = self.agent.post(&url).header("xi-api-key", self.key.header()).header("accept", "*/*");
        for (k, v) in query {
            request = request.query(*k, *v);
        }
        self.read(request.send_json(body))
    }
}

/// A request that never reached the service, or whose answer never came.
fn unreachable_service(e: &ureq::Error) -> String {
    format!("ElevenLabs could not be reached ({e}); nothing was sent and nothing was spent")
}

// ---------------------------------------------------------------- the account

/// What `/v1/user/subscription` says of the key's plan.
#[derive(Clone, Debug, PartialEq)]
pub struct Subscription {
    pub tier: String,
    pub credits_used: Option<f64>,
    pub credits_limit: Option<f64>,
    /// When the credits reset, as `2026-11-01T00:00:00Z`.
    pub credits_reset: Option<String>,
    pub status: Option<String>,
}

impl Subscription {
    pub fn json(&self) -> Json {
        json!({
            "tier": self.tier,
            "credits_used": self.credits_used,
            "credits_limit": self.credits_limit,
            "credits_left": match (self.credits_limit, self.credits_used) {
                (Some(limit), Some(used)) => json!((limit - used).max(0.0)),
                _ => Json::Null,
            },
            "credits_reset": self.credits_reset,
            "status": self.status,
        })
    }
}

/// Asks the service what the key's plan is.
pub fn subscription(client: &Client) -> Result<Subscription> {
    let reply = client.get("/v1/user/subscription")?;
    if reply.status >= 400 {
        return Err(reply.failure().explain());
    }
    let j = reply.json();
    let tier = j["tier"].as_str().ok_or_else(|| "ElevenLabs's subscription reply named no tier".to_string())?;
    Ok(Subscription {
        tier: tier.to_string(),
        credits_used: j["character_count"].as_f64(),
        credits_limit: j["character_limit"].as_f64(),
        credits_reset: j["next_character_count_reset_unix"].as_i64().map(iso_date),
        status: j["status"].as_str().map(str::to_string),
    })
}

/// A Unix time as `2026-11-01T00:00:00Z`.
pub fn iso_date(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

// ---------------------------------------------------------------- sounds

/// A sound or a loop to ask for.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundRequest {
    /// The description as given.
    pub text: String,
    /// The length, 0.5 to 30; the model picks one when it is left out.
    pub seconds: Option<f64>,
    /// 0 to 1, 0.3 by the service's default; higher follows the text more literally.
    pub influence: Option<f64>,
    /// A sound made to repeat, with no audible start or end.
    pub looped: bool,
    /// With `--bars`: the song's tempo, written into the text and the sample.
    pub bpm: Option<f64>,
    pub bars: Option<f64>,
}

impl SoundRequest {
    /// The text sent: the description, with the tempo when a length in bars
    /// asked for one.
    pub fn sent_text(&self) -> String {
        match self.bpm {
            Some(bpm) if !self.text.to_ascii_lowercase().contains("bpm") => format!("{} at {} BPM", self.text.trim(), tidy(bpm)),
            _ => self.text.trim().to_string(),
        }
    }

    /// The seconds a loop of `bars` is at `tempo` in the meter, and the tempo.
    pub fn with_bars(mut self, bars: f64, tempo: f64, beats_per_bar: f64) -> Result<SoundRequest> {
        if !(bars > 0.0) || !bars.is_finite() {
            return Err("--bars must be more than 0".into());
        }
        let seconds = bars * beats_per_bar * 60.0 / tempo;
        if seconds > SOUND_MAX_SECONDS {
            return Err(format!(
                "{} bars at {} BPM is {:.1} s, and a sound can be at most {SOUND_MAX_SECONDS:.0} s; nothing was sent. Ask for fewer bars, or for a song with daw generate music once it is built",
                tidy(bars),
                tidy(tempo),
                seconds
            ));
        }
        self.seconds = Some((seconds * 1000.0).round() / 1000.0);
        self.bpm = Some(tempo);
        self.bars = Some(bars);
        Ok(self)
    }

    /// Checks the fields before anything is sent.
    pub fn check(&self) -> Result<()> {
        if self.text.trim().is_empty() {
            return Err("The description is empty".into());
        }
        if let Some(s) = self.seconds {
            if !(SOUND_MIN_SECONDS..=SOUND_MAX_SECONDS).contains(&s) {
                return Err(format!("--seconds must be {SOUND_MIN_SECONDS} to {SOUND_MAX_SECONDS:.0}; nothing was sent"));
            }
        }
        if let Some(i) = self.influence {
            if !(0.0..=1.0).contains(&i) {
                return Err("--influence must be 0 to 1; nothing was sent".into());
            }
        }
        Ok(())
    }

    /// The seconds the budget is held to before the request: the length
    /// asked for, or the most a sound can be when the model picks one.
    pub fn seconds_asked(&self) -> f64 {
        self.seconds.unwrap_or(SOUND_MAX_SECONDS)
    }

    /// The credits by the documented rate.
    pub fn credits_estimated(&self) -> f64 {
        (self.seconds_asked() * SOUND_CREDITS_PER_SECOND).round()
    }

    /// The body of `POST /v1/sound-generation`.
    pub fn body(&self) -> Json {
        let mut m = Map::new();
        m.insert("text".into(), json!(self.sent_text()));
        m.insert("model_id".into(), json!(SOUND_MODEL));
        if let Some(s) = self.seconds {
            m.insert("duration_seconds".into(), json!(s));
        }
        if let Some(i) = self.influence {
            m.insert("prompt_influence".into(), json!(i));
        }
        if self.looped {
            m.insert("loop".into(), json!(true));
        }
        Json::Object(m)
    }

    /// `--estimate`: what the request would cost, with nothing sent.
    pub fn estimate(&self, settings: &Settings, origin: Origin) -> Json {
        let seconds = self.seconds_asked();
        json!({
            "kind": "sound",
            "text": self.sent_text(),
            "seconds": seconds,
            "seconds_assumed": self.seconds.is_none(),
            "credits": self.credits_estimated(),
            "credits_per_second": SOUND_CREDITS_PER_SECOND,
            "format": Format::for_tier(settings.tier.as_deref()).name(),
            "tier": settings.tier,
            "budget": settings.budget_json(),
            "allowed": settings.allow(origin, seconds).is_ok(),
            "origin": origin.as_str(),
            "sent": false,
        })
    }
}

fn tidy(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{}", x.round() as i64)
    } else {
        format!("{x}")
    }
}

/// What came back: the audio and what the reply said of it.
#[derive(Clone, Debug)]
pub struct Generated {
    pub bytes: Vec<u8>,
    pub format: Format,
    pub request_id: Option<String>,
    /// The credits the service says it cost, from `character-cost` or
    /// `x-character-count`.
    pub cost: Option<f64>,
    /// The tier the format was allowed at, corrected when a format was refused.
    pub tier: Option<String>,
    /// Any note worth passing on: a format the plan refused.
    pub notes: Vec<String>,
}

/// Asks for a sound, in the best format the tier allows. A 429 is retried
/// once after the wait the reply gives; a format the plan refuses is asked for
/// again one format lower, once, and the tier is corrected. Nothing else is
/// retried, so a request is paid for at most once.
pub fn sound(client: &Client, request: &SoundRequest, tier: Option<&str>) -> Result<Generated> {
    request.check()?;
    let body = request.body();
    let mut format = Format::for_tier(tier);
    let mut tier = tier.map(str::to_string);
    let mut notes = Vec::new();
    let mut retried_429 = false;
    loop {
        let reply = client.post("/v1/sound-generation", &[("output_format", format.name())], &body)?;
        if reply.status < 400 {
            let header = |name: &str| reply.header(name).map(str::to_string);
            let request_id = header("request-id").or_else(|| header("x-request-id"));
            let cost = header("character-cost").or_else(|| header("x-character-count")).and_then(|s| s.trim().parse().ok());
            return Ok(Generated {
                bytes: reply.body,
                format,
                request_id,
                cost,
                tier,
                notes,
            });
        }
        let failure = reply.failure();
        if failure.status == 429 && !retried_429 {
            retried_429 = true;
            let wait = failure.retry_after.unwrap_or(1.0).clamp(0.0, 30.0);
            std::thread::sleep(Duration::from_secs_f64(wait));
            continue;
        }
        if let (true, Some(lower)) = (failure.is_format(format) && !failure.is_bad_prompt(), format.lower()) {
            notes.push(format!(
                "The plan refused {} ({}); asked again as {} and the tier noted as {}",
                format.name(),
                failure.message,
                lower.name(),
                format.tier_below()
            ));
            tier = Some(format.tier_below().to_string());
            format = lower;
            continue;
        }
        return Err(failure.explain());
    }
}

// ---------------------------------------------------------------- the files

/// A generated sound on disk: its audio and its recipe.
#[derive(Clone, Debug)]
pub struct Saved {
    pub file: PathBuf,
    pub recipe_file: PathBuf,
    pub recipe: Json,
    pub seconds: f64,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Where generated sounds go: `library/generated` in the workspace.
pub fn dir() -> PathBuf {
    crate::patches::workspace().join("library").join("generated")
}

/// A file's stem for a sound named `id` now: `2026-10-14-153012-crackle`.
pub fn file_stem(id: &str) -> String {
    let stamp = timestamp();
    let (date, time) = stamp.split_once('T').unwrap_or((&stamp, ""));
    let time: String = time.chars().filter(|c| c.is_ascii_digit()).take(6).collect();
    format!("{date}-{time}-{}", slug(id))
}

/// Writes what came back into `folder` as `stem.wav` or `stem.mp3`, PCM given
/// a WAV header first, with the recipe beside it as `stem.generated.json`.
pub fn save(generated: &Generated, folder: &Path, stem: &str, request: &SoundRequest, origin: Origin) -> Result<Saved> {
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let file = folder.join(format!("{stem}.{}", generated.format.extension()));
    let channels: u16 = 1;
    let (bytes, seconds, sample_rate) = match generated.format {
        Format::Pcm44100 => {
            let rate = generated.format.sample_rate();
            let frames = generated.bytes.len() / (2 * channels as usize);
            (pcm16_wav(&generated.bytes, rate, channels), frames as f64 / f64::from(rate), rate)
        }
        _ => (generated.bytes.clone(), 0.0, generated.format.sample_rate()),
    };
    write_file(&file, &bytes)?;
    let (seconds, sample_rate, channels) = match aaw_engine::sndfile::info(&file) {
        Ok(info) => (info.seconds(), info.rate, info.channels as u16),
        Err(_) => (seconds, sample_rate, channels),
    };
    let recipe = json!({
        "service": SERVICE,
        "endpoint": "/v1/sound-generation",
        "kind": "sound",
        "model_id": SOUND_MODEL,
        "prompt": request.text,
        "text_sent": request.sent_text(),
        "parameters": {
            "duration_seconds": request.seconds,
            "prompt_influence": request.influence,
            "loop": request.looped,
            "output_format": generated.format.name(),
        },
        "bars": request.bars,
        "bpm": request.bpm,
        "song_id": Json::Null,
        "request_id": generated.request_id,
        "cost_credits": generated.cost,
        "tier": generated.tier,
        "seconds": round3(seconds),
        "sample_rate": sample_rate,
        "channels": channels,
        "file": file.file_name().map(|n| n.to_string_lossy().into_owned()),
        "created": timestamp(),
        "origin": origin.as_str(),
    });
    let recipe_file = folder.join(format!("{stem}{RECIPE_SUFFIX}"));
    write_file(&recipe_file, serde_json::to_string_pretty(&recipe).expect("json").as_bytes())?;
    Ok(Saved {
        file,
        recipe_file,
        recipe,
        seconds,
        sample_rate,
        channels,
    })
}

/// Headerless 16-bit little-endian PCM as a WAV file.
pub fn pcm16_wav(pcm: &[u8], rate: u32, channels: u16) -> Vec<u8> {
    let data_len = (pcm.len() - pcm.len() % 2) as u32;
    let block = channels * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&pcm[..data_len as usize]);
    out
}

/// The project's copy of a WAV, as `samples import` makes one: under
/// `samples/`, named by the first twelve characters of its hash, with the
/// recipe copied beside it. The entry for the song's `samples` comes back.
pub fn project_copy(saved: &Saved, project_dir: &Path) -> Result<Json> {
    let bytes = std::fs::read(&saved.file).map_err(|e| format!("{}: {e}", saved.file.display()))?;
    let checksum = hex(&Sha256::digest(&bytes));
    let name = saved.file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let target = project_dir.join("samples").join(format!("{}_{name}", &checksum[..12]));
    if !target.exists() {
        write_file(&target, &bytes)?;
    }
    let recipe_name = saved.recipe_file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let recipe_target = project_dir.join("samples").join(format!("{}_{recipe_name}", &checksum[..12]));
    let mut recipe = saved.recipe.clone();
    recipe["file"] = json!(target.file_name().map(|n| n.to_string_lossy().into_owned()));
    recipe["generated_file"] = json!(saved.file);
    write_file(&recipe_target, serde_json::to_string_pretty(&recipe).expect("json").as_bytes())?;
    Ok(json!({
        "path": format!("samples/{}", target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
        "sha256": checksum,
        "source": saved.file,
        "root_note": Json::Null,
    }))
}

/// The recipe beside a project's copy of a compressed file, decoded by
/// `samples import` as `samples/HASH12_stem.wav`: written as
/// `samples/HASH12_stem.generated.json`.
pub fn project_recipe(saved: &Saved, project_dir: &Path, imported_path: &str) -> Result<PathBuf> {
    let imported = Path::new(imported_path);
    let stem = imported.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let recipe_target = project_dir.join("samples").join(format!("{stem}{RECIPE_SUFFIX}"));
    let mut recipe = saved.recipe.clone();
    recipe["file"] = json!(imported.file_name().map(|n| n.to_string_lossy().into_owned()));
    recipe["generated_file"] = json!(saved.file);
    write_file(&recipe_target, serde_json::to_string_pretty(&recipe).expect("json").as_bytes())?;
    Ok(recipe_target)
}

/// `daw generate list`: the recipes in a project's `samples/`, newest first,
/// each with the sample that plays it when the song still has one.
pub fn list(project_dir: &Path, song: Option<&aaw_model::Project>) -> Vec<Json> {
    let dir = project_dir.join("samples");
    let mut rows: Vec<Json> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(RECIPE_SUFFIX))
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path()).ok()?;
            let recipe: Json = serde_json::from_str(&text).ok()?;
            let file = recipe["file"].as_str().map(|f| format!("samples/{f}"));
            let sample_id = song.and_then(|p| {
                let file = file.as_deref()?;
                p.samples.iter().find(|(_, s)| s.path == file).map(|(id, _)| id.clone())
            });
            Some(json!({
                "recipe": format!("samples/{}", e.file_name().to_string_lossy()),
                "file": file,
                "sample_id": sample_id,
                "prompt": recipe["prompt"],
                "kind": recipe["kind"],
                "seconds": recipe["seconds"],
                "bpm": recipe["bpm"],
                "bars": recipe["bars"],
                "created": recipe["created"],
                "origin": recipe["origin"],
                "cost_credits": recipe["cost_credits"],
                "tier": recipe["tier"],
                "parameters": recipe["parameters"],
                "generated_file": recipe["generated_file"],
            }))
        })
        .collect();
    rows.sort_by(|a, b| b["created"].as_str().unwrap_or("").cmp(a["created"].as_str().unwrap_or("")));
    rows
}

/// A sample ID for a description: the first words of it as a slug, given a
/// number when the song has the name.
pub fn default_id(text: &str, taken: impl Fn(&str) -> bool) -> String {
    let words: Vec<&str> = text.split_whitespace().take(3).collect();
    let mut base = slug(&words.join(" "));
    if base.is_empty() {
        base = "generated".into();
    }
    if !taken(&base) {
        return base;
    }
    (2..).map(|n| format!("{base}-{n}")).find(|id| !taken(id)).expect("a free name")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Writes a file whole, under its folder, through a temporary name.
fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let temp = dir.join(format!(".{}.part", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    std::fs::write(&temp, bytes).map_err(|e| format!("{}: {e}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_addresses_are_told_from_the_service() {
        assert!(is_loopback("http://127.0.0.1:8080"));
        assert!(is_loopback("http://localhost:1234/"));
        assert!(is_loopback("http://[::1]:9/x"));
        assert!(!is_loopback("https://api.elevenlabs.io"));
        assert!(!is_loopback("https://127.0.0.1.example.com"));
    }

    #[test]
    fn a_key_prints_as_nothing() {
        let k = Key::new(" sk_secret ").unwrap();
        assert_eq!(format!("{k:?}"), "Key(…)");
        assert_eq!(k.header(), "sk_secret");
        assert!(Key::new("  ").is_none());
    }

    #[test]
    fn formats_follow_the_tier_and_fall_back_in_order() {
        assert_eq!(Format::for_tier(Some("pro")), Format::Pcm44100);
        assert_eq!(Format::for_tier(Some("Creator")), Format::Mp3At192);
        assert_eq!(Format::for_tier(Some("free")), Format::Mp3At128);
        assert_eq!(Format::for_tier(None), Format::Mp3At128);
        assert_eq!(Format::Pcm44100.lower(), Some(Format::Mp3At192));
        assert_eq!(Format::Mp3At128.lower(), None);
    }

    #[test]
    fn bars_become_seconds_at_the_tempo_and_too_many_are_refused() {
        let r = SoundRequest {
            text: "drum loop".into(),
            seconds: None,
            influence: None,
            looped: true,
            bpm: None,
            bars: None,
        };
        let two = r.clone().with_bars(2.0, 120.0, 4.0).unwrap();
        assert_eq!(two.seconds, Some(4.0));
        assert_eq!(two.sent_text(), "drum loop at 120 BPM");
        assert_eq!(two.body()["duration_seconds"], json!(4.0));
        assert_eq!(two.body()["loop"], json!(true));
        let waltz = r.clone().with_bars(2.0, 90.0, 3.0).unwrap();
        assert_eq!(waltz.seconds, Some(4.0));
        let e = r.with_bars(16.0, 120.0, 4.0).unwrap_err();
        assert!(e.contains("32.0 s") && e.contains("at most 30"), "{e}");
    }

    #[test]
    fn the_budget_counts_the_agent_and_not_the_person() {
        let mut s = Settings::default();
        s.budget_seconds = 10.0;
        assert!(s.allow(Origin::Agent, 10.0).is_ok());
        s.spend(Origin::Agent, 6.0);
        assert!(s.allow(Origin::Agent, 4.0).is_ok());
        let e = s.allow(Origin::Agent, 4.5).unwrap_err();
        assert!(e.contains("6.0 s of 10 s") && e.contains("--agent-seconds"), "{e}");
        assert!(s.allow(Origin::User, 400.0).is_ok());
        s.spend(Origin::User, 20.0);
        assert_eq!(s.left(), 4.0);
        assert_eq!(s.budget_json()["spent_today"], json!({"agent": 6.0, "user": 20.0}));
        s.budget_seconds = 0.0;
        assert!(s.allow(Origin::Agent, 1.0).unwrap_err().contains("0 s"));
    }

    #[test]
    fn a_pcm_reply_is_a_wav_file() {
        let wav = pcm16_wav(&[0, 0, 0xff, 0x7f, 1], 44_100, 1);
        assert_eq!(wav.len(), 48);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 4);
    }

    #[test]
    fn failures_are_explained() {
        let f = Reply {
            status: 401,
            headers: vec![],
            body: br#"{"detail": {"status": "invalid_api_key", "message": "Invalid API key"}}"#.to_vec(),
        }
        .failure();
        assert!(f.explain().starts_with("The ElevenLabs key is wrong or expired (401 invalid_api_key: Invalid API key)"));
        let f = Reply {
            status: 400,
            headers: vec![],
            body: br#"{"detail": {"status": "bad_prompt", "message": "No artists", "suggested_prompt": "a soul ballad"}}"#.to_vec(),
        }
        .failure();
        assert!(f.is_bad_prompt());
        assert!(f.explain().contains("\"a soul ballad\""));
        let f = Reply {
            status: 429,
            headers: vec![("retry-after".into(), "2".into()), ("maximum-concurrent-requests".into(), "2".into())],
            body: b"slow down".to_vec(),
        }
        .failure();
        assert_eq!(f.retry_after, Some(2.0));
        assert!(f.explain().contains("allows 2 at once"));
    }

    #[test]
    fn dates_are_iso() {
        assert_eq!(iso_date(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_date(1_761_955_200), "2025-11-01T00:00:00Z");
    }

    #[test]
    fn ids_come_from_the_words_and_stay_unique() {
        assert_eq!(default_id("Vinyl crackle, warm and slow", |_| false), "vinyl-crackle-warm");
        assert_eq!(default_id("Vinyl crackle", |id| id == "vinyl-crackle"), "vinyl-crackle-2");
        assert_eq!(default_id("***", |_| false), "generated");
    }
}
