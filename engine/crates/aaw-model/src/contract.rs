//! The authoring contract `daw describe` prints: the JSON Schema of a song and
//! what its fields mean.
//!
//! `schema.json` is the schema of the whole document, in the form pydantic
//! generated it from the Python model. A test holds it to what validation
//! accepts and to the defaults the model fills in, so a change to a field must
//! change both.

use crate::rules::{effect_params, Domain};
use crate::EFFECT_TYPES;
use serde_json::{json, Map, Value as Json};
use std::collections::BTreeSet;
use std::sync::LazyLock;

pub const TOPICS: &[&str] = &["project", "sampler", "effects", "automation"];

static SCHEMA: LazyLock<Json> =
    LazyLock::new(|| serde_json::from_str(include_str!("schema.json")).expect("schema.json is JSON"));

const PROJECT: &[(&str, &str)] = &[
    ("time", "All at/duration/length_beats fields are quarter-note beats. at is zero-based. Use fraction strings for triplets. 4/4 only."),
    ("steps", "x = velocity 100, digits 1–9 = scaled velocities, dot = rest. Whitespace and | ignored. grid is beats per cell; 1/4 = sixteenth note."),
    ("swing", "0.5 straight, 0.75 maximum; delays odd step cells. Explicit events are unswung."),
    ("pitch", "sample.root_note includes octave, e.g. C2. event.note is target pitch. Repitch changes length. No pitch-preserving stretch. daw samples analyze measures pitch; import --root-note auto uses it; check warns when a declared root disagrees with the audio."),
    ("samples", "sample.path is a file in the project that libsndfile reads: mono or stereo WAV, AIFF or FLAC. daw samples import copies such a file in, and decodes .m4a and .mp3 once to 32-bit float WAV, with the original's hash as source_sha256; a decoded file can peak a little above full scale. Copy-protected files cannot be decoded. check warns of a sample the engine cannot read."),
    ("gate", "Gate mode requires event.duration. Voice releases at note-off; it never sustains beyond sample length."),
    ("choke", "Pads sharing a choke_group within a track release on the next hit in that group."),
    ("mix", "gain_db is dB. pan is -1 left to +1 right. Mono pads use equal-power pan. Stereo pads/tracks use balance. mute wins over solo."),
    ("render", "Finite session length, tails truncated with end fade. PCM24 stereo mix and aligned FLOAT stems. Clipping fails export."),
    ("editing", "Edit with commands: daw set PATH VALUE, clip move, effect add and the others daw --help lists. While a host runs for the song (the Mac app, daw host or daw play) each command is heard, shown and undoable. daw apply PATCH --expect SHA replaces fields from a JSON merge patch: objects merge, arrays replace, null deletes; SHA is inspect's project_sha256. Writers without a host take a project lock."),
    ("playback", "daw play PROJECT --from BEAT plays through the default output. Samples sounding at the start position are picked up partway through; effects start empty there. A render always runs from the start of the song."),
    ("effects", "tracks[].effects, returns[].effects and master.effects are serial insert chains; see daw describe effects."),
    ("returns", "returns[] are reverb/delay buses fed by tracks[].sends; see daw describe effects."),
    ("automation", "tracks[].automation, returns[].automation and master.automation move gain, pan, send levels and effect parameters over time; see daw describe automation."),
    ("limits", "No groups, synths, time stretching or recording."),
];

const EFFECT: &[(&str, &str)] = &[
    ("order", "Effects run top to bottom. Track inserts come before track gain and pan. Return effects process the sum of its sends, before the return's gain and pan. master.effects follow master_gain_db and precede the end fade."),
    ("filter", "Butterworth highpass or lowpass. slope_db_per_octave 12/24/36/48 is the order times 6 dB. No resonance control."),
    ("eq", "RBJ biquad bands in series. bell uses q as bandwidth; shelves use q as shelf slope (0.71 is maximally flat). gain_db at freq_hz."),
    ("compressor", "Stereo-linked sample-peak detector, soft knee of knee_db centered on threshold_db. attack_ms smooths onset; release_ms is the time for reduction to fall by a factor of e. makeup_db is gain added after reduction; threshold_db and makeup_db can be automated."),
    ("sidechain", "compressor.sidechain names another track. Its key is that track after its own inserts, before its gain, pan, mute and solo, so a muted kick still ducks the bass. Cycles and self-sidechains are rejected. Master compressors cannot use a sidechain."),
    ("limiter", "Look-ahead brickwall on sample peaks: no output sample exceeds ceiling_db. lookahead_ms is compensated latency. Estimated true peak can still exceed the ceiling slightly; leave margin below 0 dBFS."),
    ("bypass", "bypass: true keeps an effect in the document without processing, for A/B renders with daw compare."),
    ("stems", "Stems are post-insert, post-track and post-master gain and fade, before master effects. Track stems are dry; each return has its own stem. Without master effects track and return stems sum to the mix; report.stems_sum_to_mix says which."),
    ("previews", "render --track TRACK renders that track plus its sidechain sources and omits returns and master effects, matching its stem. render --track RETURN renders the return with its senders, output wet only. Section previews include returns and the master chain."),
    ("delay", "Tempo-synced feedback delay. time_beats (fractions allowed, 1 ms to 10 s at the session tempo) is the echo spacing; feedback_percent is each repeat's level relative to the previous one. Optional lowcut_hz/highcut_hz are 12 dB/octave filters inside the feedback loop, so every repeat darkens further. ping_pong sums the input to mono, starts on the left and alternates channels."),
    ("reverb", "Convolution with a seeded synthetic impulse response: same parameters and seed, same tail. decay_seconds is the RT60 up to damping_hz; above it RT60 falls in proportion to 1/f. predelay_ms delays the tail; lowcut_hz is a 12 dB/octave highpass on the tail; width_percent 0 is mono, 100 fully decorrelated. Input is summed to mono. Energy-normalized: white noise in gives wet RMS equal to the input RMS. Adds no latency. Tails past the session end are cut by the end fade."),
    ("mix", "delay and reverb take mix_percent: output = input * (1 - mix) + wet * mix. The default 100 is fully wet, for returns; set it lower when used as a track insert."),
    ("returns", "returns[] are buses with id, gain_db, pan, mute and effects. tracks[].sends lists {to: RETURN, gain_db, pre_fader}. Post-fader sends (default) tap after the track's gain and pan; pre-fader after its inserts. Muted or solo-muted tracks send nothing. Returns are never solo-muted. A return compressor may sidechain a track; sidechains cannot name a return. Returns cannot send."),
    ("report", "Render reports list each effect with latency_frames; dynamics add max and mean gain reduction and the fraction of frames reduced over 1 dB. Returns appear under tracks with kind: return and their senders."),
    ("automation", "Effect parameters can change over time with automation lanes; see daw describe automation. Give an effect an id to address it by name."),
    ("limits", "No saturation, modulation effects, groups, return-to-return sends or impulse-response samples yet."),
];

const AUTOMATION: &[(&str, &str)] = &[
    ("lanes", "tracks[].automation, returns[].automation and master.automation list lanes {param, points}. A lane overrides the static value for the whole song. One lane per parameter."),
    ("params", "Tracks: gain_db, pan, sends.RETURN.gain_db, effects.REF.FIELD. Returns: gain_db, pan, effects.REF.FIELD. Master: gain_db (replaces session.master_gain_db) and effects.REF.FIELD. REF is an effect id or zero-based index; eq fields are effects.REF.bands.N.FIELD. Automatable effect fields are listed under automatable."),
    ("points", "points are {at, value, curve} in time order; at is in beats like any position and may equal the session length. Values use the parameter's own units and bounds."),
    ("curves", "curve shapes the segment after its point. linear (default) moves in the parameter's domain: dB, pan and percent linearly, frequencies and q in equal ratios per beat (log). hold keeps the value until the next point. Two points at the same at jump there; at most two may share a position."),
    ("outside", "Before the first point the lane holds the first value; after the last it holds the last value. A lane whose points all share one value renders exactly as that static value."),
    ("timing", "Values are evaluated on the timeline at each audio frame and move with latency compensation, so a change at beat 16 lands on beat 16. gain, pan, send, compressor, delay and reverb values change every frame. Filter and eq coefficients update every 64 frames of the song timeline."),
    ("clicks", "Nothing is smoothed. A hold step or jump on gain_db, pan or a send changes level within one frame and can click on sustained material; ramp over a few milliseconds (e.g. 1/64 beat) instead. Filter and eq sections keep their state across a coefficient jump, so a step changes tone without a burst; a highpass dropping far in one step leaves a decaying low thump that a short ramp softens."),
    ("fades", "gain_db moves linearly in dB, so a fade to -96 dB is already at -48 dB halfway and most of it is inaudible; stop at -40 to -60 dB and let the end fade finish."),
    ("sidechain", "Sidechain keys are taken after the source's inserts and before its fader, so gain_db, pan and send automation on a key track never change ducking; its effect automation does."),
    ("stems", "Track and return stems include their gain, pan and effect automation and master gain automation, like the static values."),
    ("report", "Render reports list each channel's lane params under tracks.ID.automation, master lanes under master_automation, and automated effect fields under the effect's automated key."),
    ("check", "daw check warns about lanes on bypassed effects."),
];

fn texts(items: &[(&str, &str)]) -> Json {
    Json::Object(items.iter().map(|(k, v)| (k.to_string(), json!(v))).collect())
}

/// The models a schema refers to, directly or through those.
fn references(schema: &Json, node: &Json, found: &mut BTreeSet<String>) {
    match node {
        Json::Object(map) => {
            let name = map.get("$ref").and_then(Json::as_str).and_then(|r| r.strip_prefix("#/$defs/"));
            if let Some(name) = name.filter(|n| found.insert(n.to_string())) {
                references(schema, &schema["$defs"][name], found);
            }
            map.values().for_each(|v| references(schema, v, found));
        }
        Json::Array(items) => items.iter().for_each(|v| references(schema, v, found)),
        _ => {}
    }
}

/// One model's schema on its own, with the models it refers to under `$defs`.
pub fn model(name: &str) -> Json {
    let schema = &*SCHEMA;
    let own = &schema["$defs"][name];
    let mut needed = BTreeSet::new();
    references(schema, own, &mut needed);
    let mut out = Map::new();
    if !needed.is_empty() {
        let defs: Map<String, Json> = needed.iter().map(|n| (n.clone(), schema["$defs"][n].clone())).collect();
        out.insert("$defs".into(), Json::Object(defs));
    }
    out.extend(own.as_object().expect("a model is an object").clone());
    Json::Object(out)
}

fn title(kind: &str) -> String {
    let mut name = kind.to_string();
    name[..1].make_ascii_uppercase();
    name
}

fn domains(params: &[(&str, Domain)]) -> Json {
    let name = |d: &Domain| if *d == Domain::Log { "log" } else { "linear" };
    Json::Object(params.iter().map(|(field, d)| (field.to_string(), json!(name(d)))).collect())
}

/// The schema of the whole song.
pub fn schema() -> &'static Json {
    &SCHEMA
}

/// What `daw describe TOPIC` prints, or None for an unknown topic.
pub fn describe(topic: &str) -> Option<Json> {
    Some(match topic {
        "project" => json!({"schema": schema(), "semantics": texts(PROJECT)}),
        "sampler" => json!({
            "schema": {"pad": model("Pad"), "event": model("Event"), "sample": model("Sample")},
            "semantics": texts(PROJECT),
        }),
        "effects" => json!({
            "schema": Json::Object(EFFECT_TYPES.iter().map(|k| (k.to_string(), model(&title(k)))).collect()),
            "routing": {"send": model("Send"), "return": model("Return")},
            "semantics": texts(EFFECT),
        }),
        "automation" => json!({
            "schema": {"lane": model("Lane")},
            "automatable": {
                "channel": {"gain_db": "linear", "pan": "linear"},
                "effects": Json::Object(
                    EFFECT_TYPES
                        .iter()
                        .filter(|k| !effect_params(k).is_empty())
                        .map(|k| (k.to_string(), domains(effect_params(k))))
                        .collect(),
                ),
            },
            "semantics": texts(AUTOMATION),
        }),
        _ => return None,
    })
}
