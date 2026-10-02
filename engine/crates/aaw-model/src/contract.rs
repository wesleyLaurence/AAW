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

pub const TOPICS: &[&str] = &["project", "sampler", "effects", "automation", "edit", "beats", "joins", "export"];

static SCHEMA: LazyLock<Json> =
    LazyLock::new(|| serde_json::from_str(include_str!("schema.json")).expect("schema.json is JSON"));

const PROJECT: &[(&str, &str)] = &[
    ("time", "All at/duration/length_beats fields are quarter-note beats. at is zero-based. Use fraction strings for triplets. 4/4 only."),
    ("steps", "x = velocity 100, digits 1–9 = scaled velocities, dot = rest. Whitespace and | ignored. grid is beats per cell; 1/4 = sixteenth note."),
    ("swing", "0.5 straight, 0.75 maximum; delays odd step cells. Explicit events are unswung."),
    ("pitch", "sample.root_note includes octave, e.g. C2. event.note is target pitch. Repitch changes length. daw samples analyze measures pitch; import --root-note auto uses it; check warns when a declared root disagrees with the audio."),
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
    ("stretch", "pad.source_bpm is the tempo of the pad's sample; the pad then follows session.tempo. pad.stretch says how: repitch (default) plays it faster or slower and its pitch moves; preserve_pitch stretches it in time at its own pitch, and transpose and event.note still repitch. Stretching happens when the pad's audio is prepared, not while it plays. session.stretcher is signalsmith (built in) or rubberband (the installed rubberband program). check warns past about 8%."),
    ("limits", "No groups, synths or recording."),
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

const EDIT: &[(&str, &str)] = &[
    ("parts", "An edit keeps parts of a finished song and joins them. A part is a pad of the song's sample with start_seconds and end_seconds, played by one event of a pattern on one track. Give each part its own pad."),
    ("session", "Set session.tempo to the song's measured tempo (daw describe beats), so a beat of the song is a beat of the session. Set session.sample_rate to the song's own rate when that is 44100 or 48000, so the song is not resampled, and session.master_gain_db to 0, which is -6 in a new song."),
    ("cut", "Cut a few milliseconds before the beat, not on it, so the incoming hit is whole. With lead = 0.005 s, a part that starts on the song's beat at B seconds has start_seconds = B - lead."),
    ("place", "To put that beat on session beat N, the part's event is at N - lead * tempo / 60 beats. A decimal is exact enough: frames are rounded once from the beat."),
    ("join", "The leaving part has end_seconds = E - lead + fade and release_ms = fade * 1000, where E is the song time of the first beat it does not play and fade is the crossfade in seconds. The entering part starts lead before its first beat, as any part does, with attack_ms a little under lead * 1000, so its fade in is over when the beat arrives. A fade of 0.010 to 0.012 s out and 4 ms in suits a cut in the quiet before a hit; use longer only under a sustained sound."),
    ("counts", "Keep and remove whole bars so the count carries across a join: every part kept and every part removed is a multiple of four beats and starts on a downbeat."),
    ("ends", "Give the first part a short attack_ms and the last a release_ms. session.length_beats must reach the end of the last sound: daw timeline SONG --fit sets it there, and a pattern no longer than its last event lets it."),
    ("one_shots", "A sound added to the edit is its own sample and track. To end a sound on a beat, daw timeline SONG --end-at BEAT --pad TRACK.PAD gives the beat it starts on."),
    ("times", "daw timeline SONG --seconds TIME gives a time on the edit's timeline in beats, and --beats the other way. Times in the original song are another matter: they come from daw samples beats."),
    ("level", "A mastered song is at full scale and a render refuses to clip. Give each part's event velocity 127, since the default 100 plays it 2 dB down. With the song's track and the master at 0 dB, put a limiter on master.effects rather than turning the song down, so the song stays as loud as the original and only what is added on top is taken down; see daw describe effects. daw export --match SAMPLE reports how the file's loudness compares with the song's."),
    ("check", "Render, run daw joins on the render (daw describe joins), then daw export (daw describe export)."),
    ("speed", "To make an edit shorter without changing its pitch, give each part's pad source_bpm, the song's measured tempo, and stretch: preserve_pitch, then raise session.tempo. Everything placed in beats stays where it is, so the joins hold; sounds without source_bpm keep their own length. daw timeline SONG --tempo-for SECONDS gives the tempo that makes the song that long. A few percent is the normal range; ask before going past about 8."),
    ("limits", "Parts are pads until the song has audio clips."),
];

const BEATS: &[(&str, &str)] = &[
    ("command", "daw samples beats FILE measures a whole song: its tempo, every beat, the downbeats and where the arrangement changes. FILE is audio the engine reads; import an .m4a or .mp3 first and measure the copy in the project. It prints a summary, --all lists every beat, and the map is kept beside the file as NAME.beats.json."),
    ("near", "--near TIME lists the beats within --window seconds (3) of a time given as seconds or m:ss, each with its bar, beat, strength and offset_seconds, and the nearest beat, downbeat and phrase start. A timecode in a request is approximate: take the nearest downbeat, and prefer one that starts a phrase."),
    ("tempo", "tempo.steady means one grid fits the song and its beats are that grid's. drift_ms is how far runs of beats stray from it. ambiguous_with lists half or double the tempo where that fits the same onsets; --bpm settles it."),
    ("downbeat", "downbeat.candidates gives each of the four places a bar could start a confidence. When the chosen one is under about 0.8, say so and ask, or write a --click audition for the person. --downbeat TIME makes the beat nearest TIME a downbeat. --bpm and --downbeat are kept with the map until --refresh."),
    ("bars", "Bars count from 1 at the first downbeat; beats before it are bar 0. 4/4 is assumed."),
    ("phrases", "phrases are downbeats where the arrangement changes, with change from 0 to 1 against the song's largest. They are hints and can be a bar off."),
    ("click", "--click OUT.wav writes --seconds (20) of the song around --near, or from the first downbeat, with a click on each beat and a higher one on each downbeat. It is for a person to hear; it is not something you heard."),
    ("limits", "Estimates from audio. A song that changes tempo outright or is not in 4/4 is not handled."),
];

const JOINS: &[(&str, &str)] = &[
    ("command", "daw joins RENDER [--limit SECONDS] checks a full render of an edit: each join between two parts of a song on a track, and the file's length. RENDER is the render's folder, its report or renders/latest.json."),
    ("join", "A join is two hits in a row on a track that play one sample from different pads, each for a beat or more, the second starting as the first ends."),
    ("grid", "grid is from the sample's beat map. interval_error_ms is how far the beat slips across the join and should be under 1 ms. source_beats_skipped is how many beats of the song were removed and should be whole bars. enters_after_fade_in_ms below zero means the first beat starts inside the fade in. grid is null without a beat map: run daw samples beats on the sample."),
    ("measured", "measured asks the same of the track's stem: where the transients either side sit against session beats. Attacks differ by instrument, so up to about 3 ms can be a change of sound."),
    ("step", "step.ratio over 2 is a jump in the waveform where a part starts or stops: a possible click. A fade prevents it."),
    ("level", "level.change_db is the stem's level after the join against before it."),
    ("length", "length.seconds is the whole file. within_limit and over_by_seconds answer --limit. sound_ends_seconds shows trailing silence that a shorter session.length_beats would lose."),
    ("flags", "flags say in words what to look at, and flagged lists the joins that have any. No flags means the measurements passed, not that the join was heard."),
    ("excerpts", "Each join's excerpt is a few seconds of the mix around it, in the render's joins folder, for the person to hear. Say where they are and what was measured."),
];

const EXPORT: &[(&str, &str)] = &[
    ("command", "daw export PROJECT --to PATH writes the song's latest full render as PATH, rendering first if the song has changed. .wav is 24-bit, or 16 with --bits 16; .m4a is AAC and .mp3 is MP3, with --bitrate in kb/s. An existing file needs --replace."),
    ("level", "By default the file is the render as it is. --gain DB, --peak DBTP, --lufs LUFS or --match SAMPLE apply one gain to the whole file, held under --ceiling, a sample peak of -0.1 dBFS unless given. The export does not limit: that is a master limiter in the song."),
    ("result", "level reports the policy, gain_db, held_back_db and the loudness, peak and true peak of the file and of the render. warnings say when the ceiling held a gain back, and when a compressed file's true peak is above -1 dBTP."),
    ("record", "PATH.json beside the file records its hash, format, level and the render it came from."),
    ("where", "Deliverables go in a folder the person keeps, such as the project's exports folder. renders is a cache."),
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
        "edit" => json!({"semantics": texts(EDIT)}),
        "beats" => json!({"semantics": texts(BEATS)}),
        "joins" => json!({"semantics": texts(JOINS)}),
        "export" => json!({"semantics": texts(EXPORT)}),
        _ => return None,
    })
}
