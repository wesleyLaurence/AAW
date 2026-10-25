//! Recipes: the commands for a job, in order, with what each reply says.
//! `daw describe recipes` lists them and `daw describe recipes.NAME` prints
//! one; `start`, the first, is also `daw describe start`. A test runs each
//! one as written on a song it makes from generated audio, so a command
//! that changes breaks the test and not the recipe.

use serde_json::{json, Map, Value as Json};

/// A recipe: its name, what it is for in a line, the song it starts from,
/// its steps, a command and what it does each, a batch file the steps run
/// if it has one and what else it says.
struct Recipe {
    name: &'static str,
    about: &'static str,
    given: &'static str,
    steps: &'static [(&'static str, &'static str)],
    batch: Option<fn() -> Json>,
    semantics: &'static [(&'static str, &'static str)],
}

const RECIPES: &[Recipe] = &[
    Recipe {
        name: "start",
        about: "A first song, from init to listen. Read this first.",
        given: "An empty folder, and a sample library with a kick, a snare and a hat in it.",
        steps: &[
            ("daw init song --tempo 96", "Makes song/song.yaml at 96 BPM in 4/4 (--time-signature 3/4 for a waltz), as long as what is put in it: the end follows the last clip, and song/song.aaw, the file that opens the project in the app. song is the PROJECT every other command takes. SONG.md beside it is for what the song is and where it is heading (daw describe project, notes)."),
            ("daw samples search kick", "Finds sounds in the sample library by name, here kicks; search for a snare and a hat too. A result's path is what import takes. An empty library is filled with daw samples folders add DIR. daw describe samples says how to choose between them by what they measure as."),
            ("daw samples import KICK_PATH --project song --id kick", "Copies the file into the project and names it kick in the song. Import the snare and the hat the same way, as snare and hat."),
            ("daw track add song drums", "A track of pads, played by patterns."),
            ("daw pad add song drums kick --sample kick", "A pad plays a sample. Add the snare and hat pads the same way; daw describe sampler lists what a pad takes."),
            ("daw pattern add song beat --length-beats 4", "A one-bar pattern. Its grid is 1/4 of a beat, so a row has sixteen steps of a sixteenth note."),
            ("daw pattern steps song beat kick \"x... .... x.x. ....\"", "A pad's row: x hits, a dot rests. Then snare \".... x... .... x...\" and hat \"x.x. x.x. x.x. x.x.\"."),
            ("daw clip add song drums beat --repeats 8", "Plays the pattern eight times in a row from beat 0."),
            ("daw synth add song bass --patch \"Sub Bass\"", "A new MIDI track playing a factory patch; daw patch list names them all."),
            ("daw clip add song bass --id bassline --length-beats 32", "A note clip of 32 beats at beat 0. Its notes are placed from its start."),
            (
                "daw note add song tracks.bass.clips.bassline --notes '[{\"pitch\": \"C2\", \"duration\": 3.5}, {\"pitch\": \"A#1\", \"at\": 8, \"duration\": 3.5}, {\"pitch\": \"C2\", \"at\": 16, \"duration\": 3.5}, {\"pitch\": \"A#1\", \"at\": 24, \"duration\": 3.5}]'",
                "Notes by name or MIDI number, C4 being middle C; at and duration in beats.",
            ),
            ("daw batch song chords.json", "Runs the commands of the file below as one undo step: a pad track with chords."),
            ("daw effect add song master --type limiter", "A limiter on the master holds the render under full scale."),
            ("daw render song", "Renders the mix and each track's stem under song/renders. The reply has the mix's peak and loudness."),
            ("daw listen song/renders/latest.json", "Measures the render: loudness, spectrum, each track's resonances, where two tracks overlap, what the mix loses in mono and on a small speaker, how hard each track's hits land, what each compressor, limiter and clipper took off, and pictures to look at (daw describe listen). It measures; it does not hear for you."),
        ],
        batch: Some(chords),
        semantics: &[
            ("placeholders", "KICK_PATH and the other paths come from daw samples search. Everything else runs as written in an empty folder."),
            ("batch", "A batch file lists commands as objects: op is the command (set, track.add, clip.add, note.add, synth.add, effect.add and the others), and its other keys are the command's arguments and the fields of what it adds; daw describe batch lists every op with its keys. A label names the step in the change log and for undo."),
            ("pace", "With the app open (daw status says host: true), run each step as soon as it is decided, as they are listed here, so the person watches each sound, part and clip land on the timeline and in the activity as it is made and can say what they think before the next; do not plan the whole song and then run everything at once. A batch is one musical edit, as the chords here, not the whole song. A host whose agent sent nothing for five minutes says so in the next edit's reply, as hint."),
            ("next", "daw play song plays it, or open the folder in the app. daw map song shows where each track plays, bar by bar, daw inspect song summarizes it, daw get song PATH reads any part, and daw set song PATH VALUE changes any field. daw --help lists the commands, a command's --help what it takes, daw describe the topics and daw describe recipes the other recipes."),
        ],
    },
    Recipe {
        name: "drum-bus",
        about: "Drum tracks grouped, a compressor and a level on the group, a send to a reverb return, and what the compressor took off.",
        given: "A song at song with three tracks of pads, kick, snare and hat, each playing a pattern of its own pad.",
        steps: &[
            ("daw group add song drums --tracks kick,snare,hat", "A group of the three: they play through its chain, gain and pan, and move together to where kick is. Each track names it as its group."),
            ("daw effect add song groups.drums --type compressor --threshold-db -18 --ratio 4 --attack-ms 10 --release-ms 120", "A compressor on the group, which every drum goes through together; an attack of 10 ms lets the front of each hit through. daw describe effects.compressor says what each field takes."),
            ("daw set song groups.drums.gain_db -2", "The group's own level, after its chain."),
            ("daw return add song room", "A return, a channel that only sends feed."),
            ("daw effect add song returns.room --type reverb", "A reverb on it, all wet as a return's reverb is (mix_percent 100 unless given)."),
            ("daw send set song drums room --gain-db -12", "The group sends to the return, after its fader and 12 dB down; --pre-fader sends before it. A track sends the same way."),
            ("daw render song", "The mix, and a stem for each track, the group and the return."),
            ("daw listen song/renders/latest.json", "effects.tracks.drums has what the compressor took off: max_gain_reduction_db, mean_gain_reduction_db and the share of the time over 1 dB. tracks.kick.hits has punch_db at the kick's own stem and punch_in_group_db after the group's compressor, so a faster attack shows as a lower punch in the group. The group and the return are measured as stems as the tracks are."),
        ],
        batch: None,
        semantics: &[("names", "kick, snare, hat, drums and room are this song's; put your song's tracks and names in their place. Groups can hold groups: --groups puts groups in a new one.")],
    },
    Recipe {
        name: "phrase",
        about: "A note clip, a variation of it after it, the two copied on, and a one-bar part looped under them.",
        given: "An empty song at song, as daw init song --tempo 120 makes it.",
        steps: &[
            ("daw synth add song lead --patch Pluck", "A MIDI track playing a factory patch."),
            (
                "daw clip add song lead --id call --length-beats 8 --notes '[{\"pitch\": \"E4\", \"duration\": 1}, {\"pitch\": \"G4\", \"at\": 1, \"duration\": 1}, {\"pitch\": \"A4\", \"at\": 2, \"duration\": 2}, {\"pitch\": \"G4\", \"at\": 4, \"duration\": 1}, {\"pitch\": \"E4\", \"at\": 5, \"duration\": 3}]'",
                "A two-bar phrase at beat 0. Its notes are named n1 to n5 in the order given.",
            ),
            ("daw clip duplicate song tracks.lead.clips.call --id answer", "A copy right after it, at beat 8, with notes of its own: changing them leaves the first as it is."),
            ("daw note set song tracks.lead.clips.answer.notes.n5 --pitch D4", "The variation: the copy ends a step lower."),
            ("daw note transpose song tracks.lead.clips.answer.notes.n3 --by 2", "And its long note a tone higher; a clip in place of a note moves all of its notes."),
            ("daw range copy song 0 16 --to 16 --track lead", "Both clips again from beat 16. The reply lists the copies by path; each owns its notes, so a change to one is a change to that bar alone."),
            ("daw synth add song bass --patch \"Sub Bass\"", "A second MIDI track."),
            (
                "daw clip add song bass --id root --length-beats 4 --notes '[{\"pitch\": \"A1\", \"duration\": 0.5}, {\"pitch\": \"A1\", \"at\": 1.5, \"duration\": 0.5}, {\"pitch\": \"E2\", \"at\": 3, \"duration\": 0.5}]'",
                "A one-bar part.",
            ),
            ("daw clip loop song tracks.bass.clips.root 4", "Its first four beats play again and again until the clip ends."),
            ("daw clip resize song tracks.bass.clips.root 32", "The clip eight bars long, so the bar plays eight times: one clip to change, where copies are clips to change one by one."),
            ("daw map song", "The song by bars: the same letter is the same music, so the phrase reads AABBAABB on lead, the variation B, and the bass one letter that loops every 4 beats."),
        ],
        batch: None,
        semantics: &[("copies", "A duplicate or a range copy is music of its own that can change; a loop is the same bar played again. daw clip duplicate --times N lays N copies in a row, and daw clip join makes one clip of clips that meet.")],
    },
    Recipe {
        name: "mix-pass",
        about: "check, a render, what listen reads in two stems, an eq bell from a resonance, a render again and compare.",
        given: "The song daw describe start makes, at song.",
        steps: &[
            ("daw check song", "warnings lists what the song itself says may be wrong, each with a code (daw describe check): notes struck twice, clips stacked, releases the end cuts off. Fix or keep each before measuring."),
            ("daw render song", "The mix and the stems. Keep the reply's directory, BEFORE: a render is kept in it under its own name, among the newest 3 full renders, and song/renders/latest.json points to the newest."),
            ("daw listen song/renders/latest.json", "overlap.pairs names the stems that crowd the same band at the same moments, the most first: STEM_A and STEM_B are the first pair's a and b. tracks.TRACK.audio.resonances lists each stem's narrow peaks: freq_hz, prominence_db and q, the three numbers a bell takes, and could_be_note, true when the notes may explain it."),
            ("daw listen song/renders/latest.json --overlap STEM_A STEM_B", "The two stems band by band and section by section, with the beats where they contest each band."),
            ("daw effect add song tracks.TRACK --type eq --bands '[{\"shape\": \"bell\", \"freq_hz\": PEAK_HZ, \"gain_db\": PEAK_GAIN, \"q\": PEAK_Q}]'", "A bell on TRACK at PEAK_HZ, a resonance's freq_hz, with PEAK_GAIN its prominence_db made negative, no lower than -24, and PEAK_Q its q, no higher than 18, lowers that peak."),
            ("daw render song", "The mix again, with the bell."),
            ("daw compare BEFORE song/renders/latest.json", "What changed, after minus before, loudness-matched so that a level is not read as a change of sound: the mix's and each stem's bands and loudness. tracks.TRACK shows what the bell took from that stem."),
        ],
        batch: None,
        semantics: &[("placeholders", "BEFORE, STEM_A, STEM_B, TRACK, PEAK_HZ, PEAK_GAIN and PEAK_Q come from the replies, as each step says. Which pair and which peak to answer is the song's call; the recipe is how, not what. A resonance the notes explain may be the sound itself.")],
    },
    Recipe {
        name: "markers",
        about: "The person's markers read with what plays at each, one answered by an edit that removes it in the same batch, and one kept.",
        given: "The song daw describe start makes, at song, with the person's markers m1 at beat 16, bass too loud here, and m2 at beat 24, love this.",
        steps: &[
            ("daw marker list song", "The markers in time order, each with its bar and beat, the section over it, its text and the clips playing there."),
            ("daw map song --from 12 --to 20", "The bars around m1: a marker dropped while the song played is a moment after what was heard, so read what leads into it as well."),
            ("daw batch song answer.json", "The file below: the answer to m1, the bass 4 dB down from bar 5, and m1 removed, as one step, so that one undo brings both back. The level moves over a sixteenth of a beat, since a jump in gain clicks."),
            ("daw marker list song", "m2 is left: love this asks for nothing, so it stays with what it marks."),
        ],
        batch: Some(answer),
        semantics: &[("answering", "A marker is answered by changing the music or by saying why not. With the app open, an edit's reply carries new_markers, the markers the person left since the last command (daw describe project.markers).")],
    },
    Recipe {
        name: "finished-edit",
        about: "A finished song imported, its beats found, bars cut out of an audio clip, the join crossfaded and checked with joins, and an export.",
        given: "A finished song as a WAV file at SONG_PATH, in 4/4 at a steady tempo, and an empty song at song.",
        steps: &[
            ("daw samples import SONG_PATH --project song --id tune", "A copy of the file in the project, as the sample tune. path in the reply is SAMPLE_PATH, within song."),
            ("daw samples beats song/SAMPLE_PATH", "The song's beats, measured from the copy, whose map is kept beside it: tempo.bpm is TEMPO, first_downbeat_seconds is DOWNBEAT and sample_rate is RATE. A downbeat whose confidence is under about 0.8 is worth asking about (daw describe beats)."),
            ("daw set song session.tempo TEMPO", "A beat of the session is a beat of the song."),
            ("daw set song session.sample_rate RATE", "The song is not resampled, when RATE is 44100 or 48000."),
            ("daw set song session.master_gain_db 0", "The master at 0 dB, so the song plays at its own level; a new song's is -6."),
            ("daw track add song tune", "A track for the song's audio."),
            ("daw audio add song tune tune --at 0 --source-start-seconds DOWNBEAT", "One clip of the whole song from its first downbeat at beat 0, so bar 1 of the edit is bar 1 of the song."),
            ("daw audio cut song tune --from 16 --to 32", "Bars 5 to 8 out: what follows moves 16 beats earlier and the join is crossfaded. Keep and cut whole bars, so the count carries across."),
            ("daw audio crossfade song tracks.tune.audio.1 --ms 12 --in-ms 4", "The join set by hand: 12 ms out under 4 ms in suits a cut before a hit; longer under a held sound."),
            ("daw set song tracks.tune.audio.1.fade_out_ms 2000", "The last clip fades out over two seconds."),
            ("daw effect add song master --type limiter", "A limiter on the master, so the song stays as loud as it was and nothing clips."),
            ("daw render song", "The edit rendered."),
            ("daw joins song/renders/latest.json", "Each join: where it is in the edit and in the song, its fade, how far the beats on either side are from the grid, and a short excerpt of each to hear; and the edit's length."),
            ("daw export song --to edit.wav", "The render as a WAV file, at the level it was rendered; --match tune reaches the original's loudness instead."),
        ],
        batch: None,
        semantics: &[("placeholders", "SONG_PATH is the person's file, which is copied and never changed. SAMPLE_PATH, TEMPO, DOWNBEAT and RATE come from the replies, as each step says. daw describe edit says the rest: a clip's lead, the fades, sounds added on top, an edit made faster.")],
    },
];

/// start's batch file: a pad track with chords.
fn chords() -> Json {
    let chord = |at: i64, names: [&str; 3]| -> Vec<Json> { names.iter().map(|n| json!({"pitch": n, "at": at, "duration": 8})).collect() };
    let notes: Vec<Json> = [(0, ["C4", "D#4", "G4"]), (8, ["A#3", "D4", "F4"]), (16, ["G#3", "C4", "D#4"]), (24, ["A#3", "D4", "G4"])]
        .into_iter()
        .flat_map(|(at, names)| chord(at, names))
        .collect();
    json!({
        "file": "chords.json",
        "content": {
            "label": "Add chords",
            "commands": [
                {"op": "synth.add", "track": "keys", "patch": "Soft Pad"},
                {"op": "clip.add", "track": "keys", "id": "chords", "length_beats": 32},
                {"op": "note.add", "clip": "tracks.keys.clips.chords", "notes": notes},
                {"op": "set", "path": "tracks.keys.gain_db", "value": -6},
            ],
        },
    })
}

/// markers' batch file: m1 answered and removed in one step.
fn answer() -> Json {
    json!({
        "file": "answer.json",
        "content": {
            "label": "Bass down from bar 5, as m1 asked",
            "commands": [
                {"op": "lane.set", "owner": "tracks.bass", "param": "gain_db", "points": [{"at": 15.9375, "value": 0}, {"at": 16, "value": -4}]},
                {"op": "marker.remove", "markers": ["m1"]},
            ],
        },
    })
}

/// What `daw describe recipes` prints: each recipe's line, and how a
/// recipe is read.
pub fn listing() -> Json {
    let recipes: Map<String, Json> = RECIPES.iter().map(|r| (r.name.to_string(), json!(r.about))).collect();
    json!({
        "recipes": recipes,
        "semantics": {
            "recipes": "daw describe recipes.NAME prints one: given, the song it starts from; steps, each a command to run as written and what it does and what its reply says; and batch, a file to write before the step that runs it. A word of two or more capitals in a command, such as KICK_PATH, comes from an earlier reply or from the given, as its step says. A test runs every recipe as written, so the commands and replies are as they are now.",
            "mechanics": "A recipe is which commands, in what order, and what each reply says; it holds no taste. The values it sets are examples: what to set is the song's and the person's call. A skill names a recipe for a step rather than repeating its commands.",
        },
    })
}

/// The recipes' names, in order.
pub fn names() -> Vec<&'static str> {
    RECIPES.iter().map(|r| r.name).collect()
}

/// One recipe, as `daw describe recipes.NAME` prints it, or None.
pub fn recipe(name: &str) -> Option<Json> {
    let r = RECIPES.iter().find(|r| r.name == name)?;
    let steps: Vec<Json> = r.steps.iter().map(|(run, does)| json!({"run": run, "does": does})).collect();
    let mut out = json!({"given": r.given, "steps": steps});
    if let Some(batch) = r.batch {
        out["batch"] = batch();
    }
    out["semantics"] = Json::Object(r.semantics.iter().map(|(k, v)| (k.to_string(), json!(v))).collect());
    Some(out)
}
