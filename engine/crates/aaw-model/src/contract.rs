//! The authoring contract `daw describe` prints: what a song's fields mean,
//! a line for each field generated from the JSON Schema, and with `--schema`
//! the JSON Schema itself.
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

pub const TOPICS: &[&str] = &["start", "project", "sampler", "synth", "midi", "effects", "automation", "edit", "check", "samples", "beats", "joins", "export", "listen", "reference"];

static SCHEMA: LazyLock<Json> =
    LazyLock::new(|| serde_json::from_str(include_str!("schema.json")).expect("schema.json is JSON"));

const PROJECT: &[(&str, &str)] = &[
    ("time", "All at/duration/length_beats fields are quarter-note beats, whatever the time signature. at is zero-based. Use fraction strings for triplets. session.time_signature is N/D, 1 to 32 beats over 1, 2, 4, 8 or 16, 4/4 unless set: a bar of 3/4 is 3 beats, of 6/8 3 and of 7/8 3.5. It is one for the whole song and sets the bars, the grid, the metronome's count and the beats of an exported MIDI file; daw set PROJECT session.time_signature 3/4 changes it."),
    ("steps", "x = velocity 100, digits 1–9 = scaled velocities, dot = rest. Whitespace and | ignored. grid is beats per cell; 1/4 = sixteenth note."),
    ("swing", "0.5 straight, 0.75 maximum; delays odd step cells. Explicit events are unswung."),
    ("pitch", "sample.root_note includes octave, e.g. C2. event.note is target pitch. Repitch changes length. daw samples analyze measures pitch; import --root-note auto uses it; check warns when a declared root disagrees with the audio."),
    ("samples", "sample.path is a file in the project that libsndfile reads: mono or stereo WAV, AIFF or FLAC. daw samples import copies such a file in, and decodes .m4a and .mp3 once to 32-bit float WAV, with the original's hash as source_sha256; a decoded file can peak a little above full scale. Copy-protected files cannot be decoded. check warns of a sample the engine cannot read."),
    ("gate", "Gate mode requires event.duration. Voice releases at note-off; it never sustains beyond sample length."),
    ("choke", "Pads sharing a choke_group within a track release on the next hit in that group."),
    ("mix", "gain_db is dB. pan is -1 left to +1 right. Mono pads use equal-power pan. Stereo pads/tracks use balance. mute wins over solo."),
    ("render", "Finite session length, tails truncated with end fade. PCM24 stereo mix and aligned FLOAT stems. Clipping fails export."),
    ("editing", "Edit with commands: daw set PATH VALUE, clip move, effect add and the others daw --help lists. While a host runs for the song (the Mac app, daw host or daw play) each command is heard, shown and undoable. daw apply PATCH --expect SHA replaces fields from a JSON merge patch: objects merge, arrays replace, null deletes; SHA is inspect's project_sha256. Writers without a host take a project lock."),
    ("map", "daw map PROJECT prints the song as a grid of tracks by bars, a line of map each. A letter is a clip, and the same letter is the same music: a note clip's notes and length, a pattern clip's pattern, an audio clip's part of its file. # is two or more clips of a track sounding at once, : a clip with nothing starting in the cell, as a held chord, and . nothing. Rows above give the bars and the sections; a legend says what each letter is and where it is, and a line a track its instrument, level, range and notes per bar. --per beat or --per 2 (bars) sizes a cell, --from and --to keep beats, --track keeps tracks, --lanes adds a row a lane with ~ where it moves. clips gives each letter's clips by reference, in the order they play."),
    ("markers", "markers[] are notes the person leaves at beats while listening: M in the app drops one where the song is playing, with a few words if they want, too busy or love this. They play nothing and are in no render. daw marker list PROJECT reads them in time order: each with its id, at, bar and beat, the bar it is in and the beat of that bar, counted from 1, section, the section over it, its text, and playing, the clips sounding there, a track each, by reference. Read them when the person says here, this bar or my markers, and when an edit's reply carries new_markers, which a running host adds once for the markers the person left since your last command. A marker dropped while the song played is where the playhead was when the key went down, a moment after what was heard: read the bar it is in and what led into it, and ask when it could mean either. The markers are the person's: answer one by changing the music or by saying why not, and remove it with daw marker remove PROJECT ID in the same batch as the edit that answers it ({\"op\": \"marker.remove\", \"markers\": [\"m1\"]}), so that one undo brings both back; leave one that asks for nothing, such as love this, and keep what it marks. daw marker add PROJECT AT --text TEXT leaves one of your own, to point the person at a bar; without AT it lands where a running host is playing. daw marker move PROJECT ID AT and daw marker text PROJECT ID TEXT change one. daw map shows them as a row, ! in a marker's cell, with a line each; range insert and delete move them with their beats, and a marker in deleted beats goes with them."),
    ("projects", "PROJECT is a project's folder or the song.yaml in it. daw projects lists the projects open in the app or another host, the window in front first, with each one's title; --all adds the ones the app knows that are not open. daw move and daw copy save a project under another name, as Save As does in the app: a running host carries on there and still answers at the old path, and a command sent there is told where the project is now by project in its result and a notice on stderr."),
    ("playback", "daw play PROJECT --from BEAT plays through the default output. Samples sounding at the start position are picked up partway through; effects start empty there. A render always runs from the start of the song."),
    ("effects", "tracks[].effects, groups[].effects, returns[].effects and master.effects are serial insert chains; see daw describe effects."),
    ("returns", "returns[] are reverb/delay buses fed by tracks[].sends and groups[].sends; see daw describe effects."),
    ("groups", "groups[] are buses between tracks and the master, as a drum bus: a track with group: G is summed into G instead of the master, through G's effects, gain_db, pan, mute, solo and sends. A group's tracks are next to each other in tracks[]; daw group add ID --tracks a,b makes one and moves them together. Nothing nests: a group is not in a group. See daw describe effects."),
    ("automation", "tracks[].automation, groups[].automation, returns[].automation and master.automation move gain, pan, send levels and effect parameters over time; see daw describe automation."),
    ("midi", "A track with type: midi holds note clips and an instrument; see daw describe midi."),
    ("audio", "tracks[].audio lists audio clips: parts of a sample file placed on the track's timeline, for edits of finished songs; see daw describe edit."),
    ("stretch", "pad.source_bpm is the tempo of the pad's sample; the pad then follows session.tempo. An audio clip has the same two fields. pad.stretch says how: repitch (default) plays it faster or slower and its pitch moves; preserve_pitch stretches it in time at its own pitch, and transpose and event.note still repitch. Stretching happens when the pad's audio is prepared, not while it plays. session.stretcher is signalsmith (built in) or rubberband (the installed rubberband program). check warns past about 8%."),
    ("synth", "A MIDI track's instrument may be the Synth, {synth: {...}}, a polyphonic synthesizer whose whole sound is that mapping; see daw describe synth."),
    ("limits", "No groups or recording."),
];

const SYNTH: &[(&str, &str)] = &[
    ("device", "The Synth is a MIDI track's instrument, {synth: {...}}: a polyphonic synthesizer whose whole sound is the mapping, so a project is self-contained. It plays the track's note clips as a sampler does. daw synth add SONG TRACK attaches the plain saw, {synth: {oscillators: {a: {}}}}, to a MIDI track, or makes a new MIDI track with it; daw synth add SONG TRACK --patch NAME attaches a patch instead, and daw instrument set SONG TRACK JSON attaches a whole mapping. Every field has a label, unit, range and default, listed under fields, and the panel in the app draws the same fields."),
    ("patches", "A patch is the synth mapping as a YAML file, with name, description, tags, saved_by and saved_at around it, so a sound designed in one song is loaded into another and is the same sound at the same tempo. daw patch list [WORDS] lists them, the factory patches built into daw (Init, Sub Bass, Reese, Supersaw, Pluck, Soft Pad, Bright Lead, Organ, Bell, Kick, Hat, Riser) and the saved ones in the workspace library, ~/Music/AAW/library/patches/ or under AAW_WORKSPACE, each a file named by the slug of its name (soft-pad.yaml); WORDS keep those with every word in the name or a tag. daw patch show NAME prints one. daw patch save SONG TRACK NAME [--description TEXT] [--tags a,b] [--replace] writes a track's synth there, making the folder when it is the first, and names the song's patch after it; a name already saved is written over only with --replace, and a saved patch of a factory name shadows the factory one. daw patch load SONG TRACK NAME puts the patch's mapping in place of the track's whole synth, in one undo step, and the notes stay; NAME is a patch's name or a .yaml file. The patch field in the song is where the sound came from, set by load and save, and nothing reads it."),
    ("oscillators", "oscillators is a mapping of one to four, keyed by an ID the patch chooses (a, b, sub and noise are conventions), summed in the order written. wave is sine, triangle, saw, square, pulse (with pulse_width, the percent of the cycle that is high), noise or wavetable. level_db, pan, octave, semitones and detune_cents tune and place it. phase is the percent of its cycle a note starts at; left out, each note starts at a random place from seed, which thickens stacked notes. filter: false keeps it out of the filter, as a sub often is. Saw, square and pulse are bandlimited (PolyBLEP); the triangle is plain."),
    ("unison", "unison (1 to 16) plays that many copies of the oscillator at once, spread evenly from -unison_detune_cents to +unison_detune_cents around the pitch and from left to right across unison_width_percent of the stereo field either side of pan, each copy starting at its own random phase (or all at phase when given), and the sum held at the level of one: a supersaw is one saw with unison 7 and unison_detune_cents 18. unison_detune_cents is a matrix target in cents and a lane target; unison itself is structural, so a change of it swaps through the dip."),
    ("width", "width_percent places each voice across the stereo field, either side of where its oscillators' pan and unison put it: 0 (the default) leaves every note where its oscillators are, 100 reaches the edges. width_mode says where a note lands within the width: alternate (the default) puts each note of the track on the other side from the one before it, so a line swings between the sides and a chord's notes fall on both; pitch places a note by its pitch, two octaves either side of middle C reaching the edges, as a piano's keys lie; random draws a place for each note from seed. A note keeps its place for its whole length, under the oscillators' own pan and unison width. width_percent is a lane target and a matrix target in points, so a macro or an LFO can open the field; width_mode is read when a note starts."),
    ("wavetables", "wave: wavetable reads one cycle from table: organ (drawbars), bright (every harmonic to the 64th), hollow (odd harmonics, between a triangle and a square), vowel (an ah: formants at the fifth, ninth and twentieth harmonics), fold (a sine through a wavefolder) and steps (a sine in eight steps), or the ID of a sample in the project whose file is one cycle: a WAV of up to 65536 frames, summed to mono, its DC removed and its level normalized, so a cycle drawn or cut from any sound becomes a wave. Every table is read bandlimited: harmonics above half the sample rate are left out at each pitch, so a table does not alias. A patch naming a sample loads into a song that has the sample. Multi-frame tables with a sweepable position are a Later line."),
    ("effects", "effects inside the patch is a chain of the song's effect kinds (daw describe effects) run on the sum of the voices before the track's inserts, so a patch carries its chorus, saturation or reverb and sounds finished on any track; daw synth audition hears it without --track-chain. A compressor here has no sidechain. daw effect add SONG tracks.T.instrument.synth KIND [FIELDS], daw effect remove, move and bypass work on the chain as on a track's, and daw set reaches a field as tracks.T.instrument.synth.effects.REF.FIELD. Lanes reach them as instrument.effects.REF.FIELD, REF an id or index."),
    ("filter", "One filter a voice, after the oscillators routed through it: mode lowpass, highpass, bandpass or notch, slope_db_per_octave 12 or 24, cutoff_hz, resonance_percent (0 is a Q of a half; 100 rings just short of self-oscillation), drive_db into a soft clip before the filter, and keytrack_percent, 100 moving the cutoff an octave an octave from middle C. enabled: false passes the oscillators through. The default is a lowpass open at 20 kHz, which changes nothing."),
    ("envelopes", "envelopes is a mapping; amp is always there and shapes each voice's level, and others (env2, env3) do nothing until the matrix uses them. attack_ms rises linearly to full; decay_ms falls exponentially to sustain_percent, reaching it at decay_ms; release_ms falls exponentially to silence after the note-off, reaching it at release_ms. A voice ends when its amp release does."),
    ("lfos", "lfos is a mapping of up to four, bipolar, -1 to 1. shape is sine, triangle, saw (falling), square or sample_hold (a new random value a cycle). rate_hz is cycles a second; rate_beats, when given, replaces it with one cycle in so many beats at the song's tempo, so 1/2 is an eighth note and 4 a bar. retrigger: true starts the cycle at each note from phase_percent; otherwise the LFO runs from the start of the song and every voice shares its phase, so a synced LFO lands on the bar."),
    ("modulation", "modulation lists {source, target, amount}. source is an envelope or LFO by its ID, velocity, note, random or macros.NAME; target is listed under targets with the unit of amount, which is how far the target moves at full modulation: semitones for a pitch, octaves for cutoff_hz and envelope times (1 doubles, -1 halves), dB for a level, points for a percentage, and the pan's own units. Envelopes, velocity, random and macros are unipolar, 0 to 1; note and LFOs are bipolar. An entry on an envelope's field is taken when the note starts and holds for the note. Automation and the matrix add: a lane moves the field's value and the matrix moves it from there. An entry naming a source or target the patch lacks is refused."),
    ("macros", "macros is a mapping of up to eight named knobs, 0 to 100, that do nothing but through the matrix: {source: macros.tone, target: filter.cutoff_hz, amount: 3} opens the filter three octaves as tone goes from 0 to 100. They are the knobs to automate (instrument.macros.tone) and to reach for first; daw check names a macro no entry uses."),
    ("voices", "voices is 1 to 16; 1 is monophonic. Past it the oldest releasing voice is stolen, else the oldest sounding, over a 5 ms fade. glide_ms slides each new note's pitch from the pitch of the last note started: portamento with one voice. velocity_percent is how much velocity moves the level: 100 is linear, as a sampler's, 0 none. A voice is a function of the patch, the note and the frames since it started, with its random phases and random value from seed, the track and the note's place in the track, so a render is the same bytes twice and playback from the start equals it. Playback from a locate chases the notes sounding there: each starts with its envelopes and free LFOs where time would have brought them and its filter empty."),
    ("automation", "Lanes on a MIDI track reach the synth as instrument.FIELD: instrument.filter.cutoff_hz, instrument.oscillators.a.level_db, instrument.oscillators.a.unison_detune_cents, instrument.width_percent, instrument.macros.tone, instrument.envelopes.amp.release_ms, instrument.lfos.lfo1.rate_hz, and the patch's effects as instrument.effects.REF.FIELD. automatable lists every field a lane can move; frequencies and LFO rates interpolate in the log domain. A field the panel or the agent sets glides over 5 ms while the song plays; a change of wave, table, unison, filter mode, slope or routing, or of the patch's effects, swaps through a 10 ms dip."),
    ("commands", "daw synth add SONG TRACK; daw synth show SONG TRACK prints the patch as the song holds it, fields at their defaults left out; daw synth set SONG TRACK PATH VALUE [PATH VALUE ...] sets fields by their path in the patch as one undo step, a value being a number, a word or a JSON object, so oscillators.sub '{\"wave\": \"sine\", \"octave\": -1, \"filter\": false}' adds an oscillator and a null removes one; daw synth mod SONG TRACK SOURCE TARGET AMOUNT adds an entry or changes its amount, and --remove takes it out; daw synth audition SONG TRACK [--notes C2,C3] [--velocity 100] [--length-beats 2] [--track-chain] renders the notes one after another through the patch, and with --track-chain the track's effects, to a WAV under renders/auditions and replies with its path, peak, loudness and spectral centroid, so there is something to hear and daw listen can measure it. daw set SONG tracks.T.instrument.synth.PATH VALUE reaches any field too, and a labeled daw batch makes a designed sound one step."),
    ("recipes", "Each is a factory patch too, loaded with daw synth add SONG TRACK --patch NAME and read with daw patch show NAME. Sub bass: a sine, and a saw an octave up at -12 dB under a lowpass at 200 Hz; velocity to filter.cutoff_hz by 1. Pluck: a saw with unison 2, amp attack 1 decay 300 sustain 0 release 200, env2 attack 0 decay 250 sustain 0 on filter.cutoff_hz by 4 over a cutoff of 300 Hz with resonance 20, and a dotted-eighth delay mixed in low. Pad: a saw with unison 3 at 8 cents over a sub, amp attack 400 release 900, cutoff 900 opened by env2, lfo1 at 0.3 Hz on pitch by 0.08, a chorus and a reverb in the patch. Supersaw: one saw with unison 7 at 18 cents and full width, a chorus behind it. Lead: a square and a saw a fifth up at -9 dB, glide_ms 60 with one voice, lfo1 sine 5 Hz retriggered on pitch by 0.3 for vibrato, tube saturation. Organ: the organ wavetable with a sub and a slow chorus. Kick: a sine with env2 attack 0 decay 40 sustain 0 on pitch by 36, so the pitch falls three octaves onto the note, and the amp decay 300 sustain 0, plus a short noise burst with its own envelope, into soft saturation. Hat: noise with a highpass at 6 kHz, amp decay 60 sustain 0. Riser: noise and a saw through a lowpass whose cutoff an envelope opens over 4 beats, with resonance 40, into a long reverb. Every amount is in the target's unit; start from these and listen with daw synth audition."),
    ("limits", "LFO fields are not matrix targets; the triangle is not bandlimited; a wavetable is one cycle, not a sweep of frames; a sample's cycle is read at most 65536 frames long."),
];

const SAMPLER_DEVICE: &[(&str, &str)] = &[
    ("device", "The app's Sampler, dragged from the browser's Instruments onto a MIDI track, is the track's sampler instrument. Empty it is {sampler: {pads: {}, map: []}}, which plays nothing. A sample dropped on it loads one pad on every note at its pitch, as it is at middle C: {sampler: {pads: {NAME: {sample: S}}, map: [{notes: [0, 127], pad: NAME, pitched: true}]}}, the same as a sample dropped on the track's header or under the tracks makes; a sample the browser measured a pitch of starts with mode gate. Its panel shows the file's waveform and sets the pad's mode, gain_db, pan, transpose, start_seconds, end_seconds, attack_ms, release_ms and reverse, which daw pad set SONG TRACK NAME sets too, and the sample's root_note, which daw set SONG samples.S.root_note NOTE sets; Measure in the panel sets it to the pitch daw samples analyze finds. A sampler of several pads, as daw instrument map makes, is listed in the panel and not drawn as the device."),
];

const MIDI: &[(&str, &str)] = &[
    ("tracks", "A track with type: midi has an instrument and note clips under clips, and no pads, pattern clips or audio clips. Its gain, pan, effects, sends and automation are any track's. daw track add SONG ID --type midi makes one with no instrument."),
    ("clips", "A note clip {id, at, length_beats, loop_beats, notes} owns its notes: at is the song beat it starts on, and each note's at is beats from the clip's start. A copy (daw clip duplicate) owns copies of the notes, so changing one clip changes nothing in another."),
    ("loops", "With loop_beats, the clip's first loop_beats play again and again from its start until length_beats, as an Ableton clip with Loop on: daw clip loop SONG CLIP 8 loops the first 8 beats, daw clip resize SONG CLIP 64 plays them eight times, and daw clip loop SONG CLIP off plays the clip once again. The last repetition is cut off at the clip's end; a note held across the loop's end is cut there, and one at or after it is kept and does not play, which daw check and daw note list say. Audio clips loop by the same field (daw describe edit); the map shows a looped clip as one letter."),
    ("notes", "A note is {id, pitch, at, duration, velocity}. pitch is a MIDI number, 0 to 127. A command or the document also takes a name, which is stored as its number: C4 is 60, middle C, so C3 is 48 and C2 36. velocity is 1 to 127, 100 unless given, and scales the level linearly. duration is more than 0. Chords are notes at the same at."),
    ("timing", "Positions are exact and nothing is snapped to a grid: write 1/3, 2/3 for triplets, and a decimal such as 1.975 for a note a little ahead of beat 2 (12.5 ms at 120 BPM) or 2.025 behind it."),
    ("ids", "A clip written without an id is given the next free clipN, unique in the song; a note, the next free nN, unique in its clip. IDs stay through edits, saving and reopening. A note is addressed as its clip's path with notes.ID, e.g. tracks.keys.clips.clip1.notes.n3, or by @N while a host runs. A command that makes clips or notes replies with their paths."),
    ("edges", "A note sounds from its start to the end of its duration or of its clip, whichever is first. A note that starts at or after its clip's end, or its loop's, is kept and does not play, and lengthening the clip or the loop brings it back. A note may start before its clip, at a negative at, as a clip shortened from its left edge (daw clip trim --start) leaves the notes it passed: it is kept and does not play, even where it would last into the clip, and moving the start back brings it back. daw check and daw note list name notes outside their clip."),
    ("instrument", "instrument is null, and the notes play nothing and are kept, or {sampler: {pads, map}}, or {synth: {...}}, the Synth of daw describe synth. pads are the pads of daw describe sampler, by name. map lists {notes, pad, pitched}: notes is one note or [LOW, HIGH], inclusive, and no note may be in two entries. A pitched entry plays its pad repitched from its sample's root_note to the note, or from middle C (C4, 60) when the sample has none, as Ableton's Simpler does; an entry that is not pitched plays its pad as it is, whatever the note, as a drum rack does. A note no entry maps is silent, and daw check names it. Replacing or removing the instrument leaves the notes as they are."),
    ("drums", "General MIDI drum notes are the usual map: 36 (C2) kick, 38 (D2) snare, 42 (F#2) closed hat, 46 (A#2) open hat, 49 (C#3) crash."),
    ("voices", "Each note plays its own voice, overlapping notes of the same pitch included. A gate pad releases at the note-off over its release_ms; a one_shot pad plays its sample through. A voice never outlasts its sample: there is no sustain loop. Choke groups work as on any track."),
    ("commands", "daw clip add SONG TRACK --length-beats 4 [--at 16]; daw note add SONG CLIP --pitch C4 --duration 1 [--at 0 --velocity 96], or --notes '[{...}, ...]' for many; daw note set SONG NOTE --velocity 80; daw note move SONG NOTE... --by -1/48; daw note transpose SONG NOTE... --by 12; daw note remove SONG NOTE...; a clip given to move, transpose or remove stands for all its notes. daw clip duplicate, move, resize, trim, loop, join and remove place clips; daw clip trim SONG CLIP --start BEAT --end BEAT moves either edge to a song beat and leaves the notes where they are in the song; daw clip loop SONG CLIP BEATS|off sets how many of its first beats repeat. daw note list SONG CLIP|TRACK [--from BEAT --to BEAT] reads notes with their names and song beats. daw instrument set SONG TRACK JSON attaches or replaces the instrument, daw instrument remove takes it off, daw instrument map SONG TRACK NOTES PAD [--pitched] adds a map entry, and daw pad add/set/remove edit the sampler's pads. A labeled daw batch makes a phrase and its variations one undo step."),
    ("version", "A song with a MIDI track is saved with schema_version: 2, which an engine from before MIDI tracks refuses; one without is saved as version 1."),
    ("files", "daw midi import SONG FILE [--track TRACK] [--at BEAT] makes a note clip of a Standard MIDI file of one part, type 0 or 1, whose notes are in one file track and on one channel; a file of more parts is refused with them named. The clip goes on --track, a MIDI track, or else on a new MIDI track named after the file, at --at (0 unless given). It starts at the file's beat 0 and lasts to its last note's end in whole bars, and the song grows to hold it. Positions are exact. The file's tempo is not taken: the notes keep their beats, and the reply's file_tempo is for information. The reply's left_out counts what the song does not hold, by kind (sustain pedal, pitch bend, controllers, program changes, aftertouch and more), and adjusted counts notes with no note-off or no length. daw midi export SONG CLIP FILE writes the clip's notes that play as a type 0 file at 960 ticks a beat with the song's tempo, on channel 1; its reply counts notes outside the clip, notes shortened to its end, notes rounded to a tick, and a note inside a longer one of its pitch, whose lengths a MIDI file cannot keep apart."),
    ("limits", "No controllers, pitch bend or pedal, and no recording. A loop starts at its clip's start, and a looped clip is cut by daw range only at a wrap. A MIDI file is one part; a file of several parts and its tempo are not read."),
];

const EFFECT: &[(&str, &str)] = &[
    ("order", "Effects run top to bottom. Track inserts come before track gain and pan. Return effects process the sum of its sends, before the return's gain and pan. master.effects follow master_gain_db and precede the end fade."),
    ("filter", "Butterworth highpass or lowpass. slope_db_per_octave 12/24/36/48 is the order times 6 dB. No resonance control."),
    ("eq", "A parametric equalizer: up to 16 RBJ biquad bands in series. bell raises or lowers gain_db at freq_hz with q as its width; low_shelf and high_shelf use q as the shelf's slope (0.71 is maximally flat); highpass and lowpass cut past freq_hz at slope_db_per_octave (12, 24, 36 or 48, Butterworth), with q the resonance at the corner (0.71 is flat, higher lifts it) and gain_db ignored. gain_db is 0 unless given; slope_db_per_octave counts only on a pass. The app draws the bands as one curve over the playing spectrum."),
    ("compressor", "Stereo-linked sample-peak detector, soft knee of knee_db centered on threshold_db. attack_ms smooths onset; release_ms is the time for reduction to fall by a factor of e. makeup_db is gain added after reduction; threshold_db and makeup_db can be automated."),
    ("sidechain", "compressor.sidechain names another track. Its key is that track after its own inserts, before its gain, pan, mute and solo, so a muted kick still ducks the bass. Cycles and self-sidechains are rejected. A group's or a return's compressor may be keyed by a track; a sidechain cannot name a group or a return. Master compressors cannot use a sidechain."),
    ("limiter", "Look-ahead brickwall: no output sample exceeds ceiling_db. lookahead_ms is compensated latency. Without true_peak it holds the samples alone, and the level between them, which a converter or a lossy encoder makes of them, passes the ceiling: a render's estimated_true_peak_dbtp reads over ceiling_db, by a few tenths of a dB on most mixes and by up to 3 dB where the top is loud. true_peak: true holds that estimate too: the limiter reads the level between the samples as the render's report, daw listen and daw export measure it, four times oversampled, and turns down for it, so estimated_true_peak_dbtp comes out at or under ceiling_db and ceiling_db: -1 is -1 dBTP, with no margin left by guess. It aims 0.01 dB under the ceiling, since a gain that moves across the samples a point is read from leaves the point a little off: on noise far over the ceiling by up to 0.001 dB with the look-ahead of 3 ms and 0.004 with 1 ms, which the 0.01 covers, and by up to 0.03 with the shortest, 0.5 ms, which it does not. It takes off a little more than without, what was over between the samples, and is 19 frames later, compensated as the look-ahead is; a signal under the ceiling still passes bit-identical. Set it on the master's last limiter, with ceiling_db the true peak wanted: -1 for a file that is to be encoded or streamed. The end fade follows the master's effects and only lowers. A section preview is cut out of the song, and a cut in the middle of a sound is a peak of its own between the samples that no limiter before it holds; so is the end of a song with end_fade_ms 0 that stops while it sounds. The estimate is not a certified meter."),
    ("clipper", "The signal driven by drive_db into a ceiling: no output sample exceeds ceiling_db, which goes from -60 to 24, since inside a chain a signal passes full scale freely and a clipper is set against the level that reaches it, not against 0 dBFS. Each channel and each sample is cut by its own level, with no look-ahead and no release, so nothing pumps: what is over the ceiling is cut off and comes back as harmonics, where a limiter turns the level down around the peak. A drum's first milliseconds lose a few dB with little sound of it; a held tone or a bass distorts. knee_db is how far under the ceiling the curve starts to bend: 0 is a hard clip, and with a knee the signal is as it came up to ceiling_db - knee_db and nears the ceiling from there (tanh), softer, and under the ceiling where a hard clip would reach it. oversample (4 unless given; 1, 2 or 4) runs the curve at that many times the song's rate, so that less of the harmonics over half the rate folds back under it: a 1 kHz tone 6 dB over the ceiling folds back 47 dB under the signal at 1 and 59 at 4, a 6 kHz tone 3 dB over it 29 dB under at 1, 43 at 2 and 50 at 4. Oversampled, it also cuts crests that pass the ceiling between the samples, and is 16 frames late, compensated as the limiter's look-ahead is; at 1 it adds no latency. Under the knee the signal passes bit-identical. ceiling_db, drive_db and knee_db can be automated. The render's report and daw listen say what it took off as they do for a limiter: max_gain_reduction_db is the most it cut off one sample, and fraction_over_1db_reduction the share of samples it cut by more than 1 dB. Put one on a drum group, with the ceiling a few dB under the peak_dbfs daw listen gives that stem, or on the master before the limiter, with the ceiling a few dB under what reaches it, which is the limiter's ceiling_db plus its max_gain_reduction_db, so the limiter after it has less to turn down. As the last device of the master its ceiling must be under 0, since a render refuses a sample at full scale. Estimated true peak can still exceed the ceiling; a limiter with true_peak after it holds that."),
    ("bypass", "bypass: true keeps an effect in the document without processing, for A/B renders with daw compare."),
    ("stems", "Stems are post-insert, post-track and post-master gain and fade, before master effects. Track stems are dry; each group and each return has its own stem. A grouped track's stem is its sound before its group, so the stems of the ungrouped tracks, the groups and the returns sum to the mix. Without master effects or groups every stem sums to the mix; report.stems_sum_to_mix says which."),
    ("previews", "render --track TRACK renders that track plus its sidechain sources and omits returns and master effects, matching its stem. render --track GROUP renders the group with its tracks, the group's stem. render --track RETURN renders the return with its senders, output wet only. Section previews include groups, returns and the master chain."),
    ("delay", "Tempo-synced feedback delay. time_beats (fractions allowed, 1 ms to 10 s at the session tempo) is the echo spacing; feedback_percent is each repeat's level relative to the previous one. Optional lowcut_hz/highcut_hz are 12 dB/octave filters inside the feedback loop, so every repeat darkens further. ping_pong sums the input to mono, starts on the left and alternates channels."),
    ("reverb", "Convolution with a seeded synthetic impulse response: same parameters and seed, same tail. decay_seconds is the RT60 up to damping_hz; above it RT60 falls in proportion to 1/f. predelay_ms delays the tail; lowcut_hz is a 12 dB/octave highpass on the tail; width_percent 0 is mono, 100 fully decorrelated. Input is summed to mono. Energy-normalized: white noise in gives wet RMS equal to the input RMS. Adds no latency. Tails past the session end are cut by the end fade."),
    ("chorus", "A stereo chorus: each channel through a delay of delay_ms moved depth_ms either side by a sine at rate_hz, the right channel a quarter cycle behind the left, so the two sides drift apart. mix_percent (50 unless given) blends it under the dry signal; 100 is the wet signal alone, a vibrato. rate_hz, depth_ms, delay_ms and mix_percent can be automated. Adds no latency; the sweep is a function of the frames processed, so a render is the same bytes twice."),
    ("saturation", "The signal driven by drive_db into a curve: soft is tanh, odd harmonics that thicken as the drive rises; hard clips at full scale; tube is asymmetric, even harmonics too, with its DC removed. output_db trims the result and mix_percent (100 unless given) blends it under the dry signal, so a little tube at 30 percent is a warmth and a hard clip at 100 is a distortion. drive_db, output_db and mix_percent can be automated. No oversampling: a hard clip high up aliases a little. For a ceiling in dB that is held, a knee and oversampling, use a clipper."),
    ("utility", "The channel moves that need no other device, in this order: invert flips the polarity of the left, the right or both channels; mono sums the channels to their average; mono_below_hz, when given, sums only what lies below it, through a Linkwitz-Riley crossover of 24 dB per octave whose two halves meet flat, so a bass stays in the middle while the rest keeps its width; width_percent scales the side signal, 0 mono, 100 as it came, up to 400; gain_db; and pan, a balance as a track's. gain_db, pan and width_percent can be automated, so a utility is also a second fader or a width lane anywhere in a chain. At its defaults it passes the signal through bit-identical."),
    ("analyzer", "An effect that changes nothing: its output is its input, bit for bit, with no latency, so a render with one is the render without it. In the app it shows the sound passing through it, where it sits in the chain: a strip in the device panel with the levels and the spectrum, and a window of its own with each channel's peak, RMS and true peak, the momentary, short-term and integrated loudness and the loudness range (BS.1770-4, as daw listen measures a render) with the short-term loudness over the last minutes, the spectrum with its peaks held, the stereo field as a vectorscope with the correlation and the balance, and the last twenty seconds scrolling as a waveform of each channel and as a spectrogram. Put one on the master after the limiter to watch the mix, or on a drum group to watch the kit. Bypassed, it measures nothing. Not inside a Synth's patch. It has no fields beyond id and bypass, and nothing to automate."),
    ("mix", "delay, reverb, chorus and saturation take mix_percent: output = input * (1 - mix) + wet * mix. The default is 100, fully wet, for delay, reverb and saturation, and 50 for chorus; set a delay's or reverb's lower when used as a track insert."),
    ("returns", "returns[] are buses with id, gain_db, pan, mute and effects. tracks[].sends and groups[].sends list {to: RETURN, gain_db, pre_fader}. Post-fader sends (default) tap after the track's gain and pan; pre-fader after its inserts. Muted or solo-muted tracks send nothing. Returns are never solo-muted. A return compressor may sidechain a track; sidechains cannot name a return. Returns cannot send."),
    ("groups", "groups[] are buses between the tracks and the master, with id, gain_db, pan, mute, solo, effects, sends and automation. A track with group: G goes into G instead of the master sum: G's effects run on the sum of its tracks, then its gain and pan, then to the master and G's sends. A muted group silences its tracks and their sends; a soloed group is heard with its tracks, and a soloed track is heard through its group. A group's latency is waited for by every other track, so the mix stays aligned. The tracks of a group are next to each other in tracks[]."),
    ("report", "Render reports list each effect with latency_frames; a compressor, a limiter and a clipper add max and mean gain reduction and the fraction of frames reduced over 1 dB, over the song and under sections for each section, and an effect with an id names it. Groups appear under tracks with kind: group and their tracks, returns with kind: return and their senders; a grouped track names its group."),
    ("automation", "Effect parameters can change over time with automation lanes; see daw describe automation. Give an effect an id to address it by name."),
    ("synth", "A Synth patch carries a chain of these kinds of its own, tracks[].instrument.synth.effects, run before the track's inserts; see daw describe synth."),
    ("racks", "A rack is a chain of effects as a YAML file, with name, description, tags, saved_by and saved_at around its effects list, so a chain built on one track is added to a track in any song. daw rack save SONG OWNER NAME [--description TEXT] [--tags a,b] [--replace] writes a chain there, OWNER being tracks.T, groups.G, returns.R, master or tracks.T.instrument.synth, as effect add takes it; the song does not change. Racks live in the workspace library, ~/Music/AAW/library/racks/ or under AAW_WORKSPACE, each a file named by the slug of its name (drum-glue.yaml); there are no factory racks. daw rack list [WORDS] lists them, WORDS keeping those with every word in the name, a tag or an effect's kind, and daw rack show NAME prints one. daw rack load SONG OWNER NAME [--index N] adds the rack's effects to the chain at N or its end, in one undo step; an effect whose id the chain has is given a number (glue-2), and a compressor's sidechain is kept only on a track, group or return when it names another track of the song, and dropped otherwise, said under also. In the app a rack is dragged from the browser's Audio Effects onto a header or the device panel, and Save Rack… in the device panel saves a chain."),
    ("limits", "No phaser, groups in groups, return-to-return sends or impulse-response samples yet."),
];

const AUTOMATION: &[(&str, &str)] = &[
    ("lanes", "tracks[].automation, groups[].automation, returns[].automation and master.automation list lanes {param, points}. A lane overrides the static value for the whole song. One lane per parameter."),
    ("params", "Tracks: gain_db, pan, sends.RETURN.gain_db, effects.REF.FIELD, and on a MIDI track with a synth instrument.FIELD, such as instrument.filter.cutoff_hz, instrument.macros.tone or instrument.effects.REF.FIELD for the patch's own effects (daw describe synth). Groups: gain_db, pan, sends.RETURN.gain_db, effects.REF.FIELD. Returns: gain_db, pan, effects.REF.FIELD. Master: gain_db (replaces session.master_gain_db) and effects.REF.FIELD. REF is an effect id or zero-based index; eq fields are effects.REF.bands.N.FIELD. Automatable effect fields are listed under automatable."),
    ("points", "points are {at, value, curve, shape} in time order; at is in beats like any position and may equal the session length. Values use the parameter's own units and bounds."),
    ("curves", "curve shapes the segment after its point. linear (default) moves in the parameter's domain: dB, pan and percent linearly, frequencies and q in equal ratios per beat (log). hold keeps the value until the next point. shape bends a linear segment, from -1 to 1: above zero it starts slowly and finishes fast (at 0.5 progress goes as its square, at 1 as its fourth power), below zero it starts fast and finishes slowly, and 0 is straight. A sweep that should hold back and then open needs two points and a shape. Two points at the same at jump there; at most two may share a position."),
    ("outside", "Before the first point the lane holds the first value; after the last it holds the last value. A lane whose points all share one value renders exactly as that static value."),
    ("timing", "Values are evaluated on the timeline at each audio frame and move with latency compensation, so a change at beat 16 lands on beat 16. gain, pan, send, compressor, delay and reverb values change every frame. Filter and eq coefficients update every 64 frames of the song timeline."),
    ("clicks", "Nothing is smoothed. A hold step or jump on gain_db, pan or a send changes level within one frame and can click on sustained material; ramp over a few milliseconds (e.g. 1/64 beat) instead. Filter and eq sections keep their state across a coefficient jump, so a step changes tone without a burst; a highpass dropping far in one step leaves a decaying low thump that a short ramp softens."),
    ("fades", "gain_db moves linearly in dB, so a fade to -96 dB is already at -48 dB halfway and most of it is inaudible; stop at -40 to -60 dB and let the end fade finish."),
    ("sidechain", "Sidechain keys are taken after the source's inserts and before its fader, so gain_db, pan and send automation on a key track never change ducking; its effect automation does."),
    ("stems", "Track and return stems include their gain, pan and effect automation and master gain automation, like the static values."),
    ("report", "Render reports list each channel's lane params under tracks.ID.automation, master lanes under master_automation, and automated effect fields under the effect's automated key."),
    ("check", "daw check warns about lanes on bypassed effects (lane-on-bypassed-effect); see daw describe check."),
];

const CHECK: &[(&str, &str)] = &[
    ("command", "daw check PROJECT prints daw inspect's summary, each sample's root_note against the pitch measured from its audio, and warnings: what a render will play that was probably not meant. It reads the song and is cheap: run it after writing notes or placing clips, before rendering."),
    ("warnings", "warnings is a list of objects: code, listed under codes; level, warning or info for what is often deliberate; message, in words; paths, what it is about as commands name it (tracks.drums.clips.1, tracks.keys.clips.chords, samples.kick); and at, the song beat, when it has a place in time. An empty list is a clean song."),
    ("duplicate", "daw clip duplicate SONG CLIP puts the copy right after the original or, when copies already follow it there, after the last of them, so duplicating again lays the copies in a row; --times N makes N copies in a row as one step, and --at BEAT puts them from that beat."),
    ("limits", "Checks read the song, not the audio: two tracks masking each other is for daw listen. A note is counted at its written pitch; an instrument tuned an octave away, or filtered thin, is not known to register-crowded."),
];

const EDIT: &[(&str, &str)] = &[
    ("clips", "An edit keeps parts of a finished song and joins them. A part is an audio clip on a track: tracks[].audio lists {sample, at, source_start_seconds, source_end_seconds, lead_ms, gain_db, fade_in_ms, fade_out_ms, fade_curve, source_bpm, stretch, loop_beats, length_beats}. at is the beat that source_start_seconds of the file plays on; without source_end_seconds the clip plays to the end of the file. A clip plays its file at full level, a mono file on both sides."),
    ("loops", "A clip with loop_beats plays its first loop_beats again and again from its start until length_beats, its own length on the timeline: a two-bar drum loop made sixteen bars long is one clip. daw clip loop SONG CLIP 8 loops the first 8 beats and keeps the clip's length; daw clip resize SONG CLIP 64, or daw audio trim --end, sets how long it plays; daw clip loop SONG CLIP off plays the file once again, as before. Each repetition is a copy of the clip, with its lead_ms, fade_in_ms and fade_out_ms, and leaves at the wrap from where the next starts, as two clips that meet do, so the wrap is a crossfade the clip's own fades set; the last repetition is cut off where the clip ends. The loop is loop_beats of song time: with source_bpm it is that many beats of the file at its own tempo and follows session.tempo; without, it is a fixed length in seconds. A loop of a part that is not a whole number of beats of the file drifts a little each wrap: cut loops on the file's beats, which daw samples beats gives, and give the clip source_bpm. A looped clip's start cannot be trimmed and it is split, by audio split or daw range, only at a wrap; turn the loop off first."),
    ("session", "Set session.tempo to the song's measured tempo (daw describe beats), so a beat of the song is a beat of the session. Set session.sample_rate to the song's own rate when that is 44100 or 48000, so the song is not resampled, and session.master_gain_db to 0, which is -6 in a new song."),
    ("build", "daw track add SONG TRACK, then daw audio add SONG TRACK SAMPLE --at BEAT --source-start-seconds S: one clip from the song's downbeat at S seconds, the first beat to keep, to the last. Then daw audio cut SONG TRACK --from BEAT --to BEAT for each part to remove: it takes those beats out, moves what follows earlier and crossfades the join. Beats are the session's; daw timeline SONG --seconds TIME converts a time on the edit's timeline, and daw samples beats gives times in the song."),
    ("commands", "daw audio add, move CLIP --at BEAT --track TRACK, split CLIP --at BEAT, trim CLIP --start BEAT --end BEAT, crossfade CLIP and cut TRACK --from BEAT --to BEAT. CLIP is tracks.TRACK.audio.N, or the @N reference daw inspect lists while a host runs. A move takes a clip to another beat, another track or both, with its audio; to remove one, daw remove SONG CLIP. A split makes two clips that play exactly as the one did. A trim moves an edge to a beat and leaves the audio where it is. daw clip join SONG CLIP CLIP... makes one clip of audio clips of one track that meet and are the same file played on from where the one before leaves, as audio split left them, so one clip plays what they played; a join made by audio cut, with a crossfade, and clips of two files are refused with the reason. Pattern clips of one pattern that meet join too, their repeats summed."),
    ("lead", "lead_ms is how long before its beat a clip starts, so a cut sits in the quiet before a hit and the hit is whole; 5 is usual. A clip that ends where another on its track begins leaves from where that one starts, so their join is at one place before the beat whatever each clip's own lead."),
    ("fades", "fade_in_ms is over the clip's start and should be shorter than its lead. fade_out_ms follows where the clip leaves, so the next clip fades in under it: 12 out and 4 in suit a cut before a hit; use longer only under a sustained sound. fade_curve is equal_power, which keeps the level across two different parts of a song, or linear, which keeps it across audio that is the same. daw audio crossfade CLIP [--ms 12] [--in-ms 4] [--lead-ms 5] sets the join between a clip and the one before it."),
    ("counts", "Keep and remove whole bars so the count carries across a join: every part kept and every part removed is a multiple of four beats and starts on a downbeat."),
    ("ends", "Give the first clip a short fade_in_ms and the last a fade_out_ms. session.length_beats must reach the end of the last sound: daw timeline SONG --fit sets it there."),
    ("one_shots", "A sound added to the edit is its own sample on its own track, as an audio clip or a pad. To end a pad's sound on a beat, daw timeline SONG --end-at BEAT --pad TRACK.PAD gives the beat it starts on."),
    ("level", "A mastered song is at full scale and a render refuses to clip. With the song's track and the master at 0 dB, put a limiter on master.effects rather than turning the song down, so the song stays as loud as the original and only what is added on top is taken down; see daw describe effects. daw export --match SAMPLE reports how the file's loudness compares with the song's."),
    ("speed", "To make an edit shorter without changing its pitch, give each clip source_bpm, the song's measured tempo, and stretch: preserve_pitch, then raise session.tempo. Everything placed in beats stays where it is, so the joins hold; sounds without source_bpm keep their own length. daw timeline SONG --tempo-for SECONDS gives the tempo that makes the song that long. A few percent is the normal range; ask before going past about 8."),
    ("check", "Render, run daw joins on the render (daw describe joins), then daw export (daw describe export)."),
    ("pads", "A part can also be a pad with start_seconds and end_seconds, played by a pattern event, as edits were built before audio clips; daw joins checks those too. A pad's event plays at velocity 100 of 127 unless given 127, and a mono pad is 3 dB down."),
    ("ranges", "daw range copy SONG START LENGTH --to AT, insert SONG AT LENGTH, delete SONG START LENGTH and clear SONG START LENGTH edit a range of beats across every track at once, with the pattern clips, note clips, audio clips, automation and sections in it: a verse repeated, four bars put in before the drop, a breakdown. START, LENGTH and AT are beats; a bar is four. copy puts the range over what is at AT, and with --insert opens time for it; insert opens empty beats and the song grows; delete removes the beats and closes the gap, so the song shrinks; clear empties the beats and moves nothing. --track T, repeatable, limits a command to some tracks, whose lanes go with them; the sections, the returns' and master's lanes and the song's length change only when every track does. Each command is one undo step, and its reply lists what it made (paths), split, moved and removed."),
    ("edges", "A clip that crosses a range's edge is cut there, so the range holds exactly what plays in it. A note clip becomes two, a note across the cut held to it on the left and the rest starting the right clip; a copy owns its notes under IDs of its own. An audio clip is split as audio split splits it, unfaded where the halves still meet; a half moved or copied away from the other fades 4 ms in or 12 ms out at the cut, since it joins other audio there. A pattern clip is cut only between repeats, and a looped clip only at a wrap, each half keeping the loop: inside one the command is refused, naming the clip; move the range to a repeat's edge, split the pattern or turn the loop off. A lane gains a point at each edge with the value it had there, so the curve outside the range is unchanged; a cleared range runs straight between its edges, inserted beats hold the value. A copy starts clean: tails of what came before the range are not part of it. After delete, the two clips that meet where the gap closed become one again when they are one music, so insert then delete gives the song back."),
    ("sections", "daw section duplicate SONG SECTION [--to AT] [--id ID] puts the section and what is under it again, right after it or at AT, pushing what follows later, under a free name or ID. daw section move SONG SECTION AT --with-content takes its beats with the label, over what is at AT; daw section remove SONG SECTION --with-content deletes its beats as range delete does. Without --with-content, move and remove touch the label alone."),
];

const SAMPLES: &[(&str, &str)] = &[
    ("command", "daw samples search QUERY lists the library's samples whose path has every word of QUERY, by name, 20 unless --limit; --category, --type, --key and --bpm narrow it by what a file's name says, which is a hint. A row's path or id is what daw samples import, analyze, inspect, audition and like take. The library is the folders added with daw samples folders add DIR; the files in them are never changed. These measure; they do not hear, so do not say you listened."),
    ("measured", "A row's measured is what the sample's audio does, or null for a sample not measured yet: pitched, note, cents and confidence, kind (one_shot, loop or uncertain) and bpm, the seven numbers of sound, category, words and words_among. daw samples analyze SAMPLE measures one sample and prints all of it, the seven under sound; daw samples analyze --all measures every sample of the library not measured yet, several at a time, a few minutes for some thousands of samples, saying how far it is on stderr, and a run that is stopped goes on from there. What is measured is kept until the file or the measuring changes. A search that sorts or filters by a measurement lists measured samples only: it says on stderr how many it left out, and is refused when it would leave out every match; --measure N measures the first N of them first."),
    ("sound", "Seven numbers, each of the sound from its first sample within 50 dB of the peak to its last. centroid_hz is where the middle of its power lies, from 20 Hz to 20 kHz: a kick near 100, a snare near 2500, a hat near 8000. low_fraction is the share of its power under 120 Hz, from 0 to 1. attack_ms is the time the sound takes from first reaching a tenth of its level to first reaching nine tenths: a drum under 3, a pad hundreds. decay_ms is the time from there to the last moment within 20 dB of the level: how long it rings, and its length from there for a sound that is cut before it falls that far. The level is the peak, or for noise, whose highest sample is a chance, 3 dB over the RMS of its loudest 10 ms. noisiness runs from 0 for a tone to near 1 for noise: a stretch of 40 ms is held against the stretch from 2 to 40 ms after it, and what it shares with the one most like it is taken from 1, so an 808 reads near 0.02, a bass under 0.1, a kick 0.4, a snare 0.7 and a clap or a hat 0.8, whatever band they fill. loudness_lufs is its loudest 400 ms as daw listen weighs loudness, a shorter sound ending in silence: the difference between two samples' is the gain that matches them. punch_db is what daw listen gives a hit: the peak of the first 30 ms against the RMS of the 200 ms after them, silence where the sound has ended, never past 60. A loop has many hits, so its attack_ms, decay_ms and punch_db are null."),
    ("words", "words gives centroid_hz, low_fraction, attack_ms, decay_ms and noisiness a word each: dark, warm or bright; thin, full or boomy; sharp, soft or slow; short, medium or long; tonal, mixed or noisy. A word says where the number lies among samples of the same category, named by words_among, so a long kick rings for 150 ms and a long 808 for two and a half seconds. lines lists the two lines between each field's three words, under any for every sample and under a category for the fields where its samples spread enough to tell apart: a number under the first line takes the first word, one at or over the second the third. A sample of a category with no lines of its own, or of none, is read against any. The lines are fixed: the thirds of the 1,318 samples of one library that are not loops, on October 9, 2026, rounded."),
    ("category", "category is read from a file's name and is other for a name that says nothing. measured.category is what a single sound measures as where that is clear, hat or kick, and null otherwise: a short noisy sound with nothing low is a hat, a short low one with a hard start a kick. --category C finds the samples named C and those named for nothing that measure as C, and such a sample's words are C's."),
    ("search", "--sort FIELD puts the most first: duration the longest, centroid the brightest, low the most low end, attack the slowest, decay the longest ringing, noisiness the noisiest, loudness the loudest and punch the hardest hit; --reverse turns the order around, by name too. A word as a flag keeps the samples it is true of, --short --bright, and --min-FIELD X and --max-FIELD X those whose number is between, with the same names as --sort: daw samples search kick --sort punch --limit 5; daw samples search snare --short --bright; daw samples search --category hat --max-decay 60 --sort loudness. --pitched, --unpitched, --note-range C1-B1, --measured-type and --measured-bpm filter by pitch, kind and tempo as measured."),
    ("like", "daw samples like SAMPLE lists the measured samples nearest a chosen one in sound, nearest first, 10 unless --limit. SAMPLE is an id, a path, or a file in a project, which is measured there and then. It is compared with the samples of its category, by its name or else as it measures, and with every sample when it has none, which among says as any; --category C chooses, and --category any is every sample. A loop is compared with loops and a single sound with single sounds. distance is in steps over the numbers both have, the root of the mean of their squares: a step is half an octave of centroid_hz, 0.15 of low_fraction, a doubling of attack_ms plus 1, a doubling of decay_ms plus 10, 0.15 of noisiness and 6 dB of punch_db. Loudness is left out, since a gain changes it, and so is pitch: transpose the pad. Under 1 is close; compared counts the samples it was held against and not_measured those left out."),
    ("limits", "Measurements of the first two minutes of a file; the centroid, the low end and noisiness are of the channels' sum. They do not say what a sound is: two samples a step apart can be a snare and a rim. A click before a slow swell has one attack, the click's or the swell's, and a sound with an echo a long decay. A fast sweep is not like itself a moment later, so a short kick that is mostly its sweep reads as noisy, and noise held to a narrow band, or a rumble under 100 Hz, as partly a tone. A loop's numbers are of the whole loop, not of its hits. The words' lines came from one library of bought sample packs and may sit elsewhere for another. Nothing is heard: audition a choice for the person with daw samples audition, or place it and let them listen."),
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

const LISTEN: &[(&str, &str)] = &[
    ("command", "daw listen RENDER measures a saved render. RENDER is the render's folder, its report.json, its mix.wav or renders/latest.json; any other audio file is measured too, without sections or stems. It prints a report and writes it under analysis/ in the render's folder with overview.png, the energy over a spectrogram, and spectrum.png, the mix's third octaves over its stems'; --no-images leaves the pictures out. daw compare BEFORE AFTER measures two renders and reports after minus before, as they are and with the after matched to the before's loudness. These measure; they do not hear, so do not say you listened."),
    ("measurements", "mix, each entry of sections and each stem under tracks, whole and by section, hold integrated_lufs (BS.1770-4, gated), peak_dbfs, estimated_true_peak_dbtp, rms_dbfs, crest_db, band_dbfs and band_fraction in seven bands (sub 20-60 Hz, low 60-250, low_mid 250-500, mid 500-2000, high_mid 2000-4000, high 4000-8000, air 8000-20000), stereo_correlation and side_energy_fraction, and the finer spectrum_db and tilt_db_per_octave. mix and each entry of sections also hold loudness_range_lu, max_short_term_lufs and max_momentary_lufs. null is undefined, such as the loudness of silence. timeline has the energy of each beat and the short-term loudness."),
    ("spectrum", "spectrum_db is the power in each of 31 third octaves, dBFS, at the standard centers from 20 Hz to 20 kHz, listed once as methods.third_octave_hz: where band_dbfs says a stem is heavy from 250 to 500 Hz, spectrum_db says whether that is a bump at 315 Hz or the whole octave. A band is null where the audio is too short to tell it apart. tilt_db_per_octave is the slope of a line through the bands from 50 Hz to 10 kHz: 0 for pink noise, which has the same power in every octave, +3 for white, lower for a darker sound; it does not move with the level, and is null for a sound that fills under half of those bands, such as a kick or a hat. The whole render's are printed, of the mix and of each stem; a section's are in the written report and printed by --section, and its tilt is printed. spectrum.png draws the mix's curve over the stems', the eight that come nearest the mix in any band each in a color, for seeing which stem makes a bump."),
    ("resonances", "resonances, of each stem and not of the mix, are narrow peaks that stay where they are while the notes move: a ring, a filter's peak, a sample's own partial. Each has freq_hz and note, the nearest pitch; prominence_db, how far it stands over the median level of the two octaves about it; q, its frequency over its width; present_fraction, the share of the bars where the stem sounds that it is found in; and could_be_note. Up to five are printed, those the notes do not explain first and then the most prominent, and resonances_omitted counts the rest. freq_hz, prominence_db and q are the three numbers an eq's bell takes: a bell at freq_hz with gain_db of minus the prominence and that q lowers the peak, and daw compare then shows by how much. prominence_db reads a little under the gain of a bell that made the peak, about 6 for a bell of 9 at a q of 8. q is measured 3 dB down, or halfway down a peak under 6 dB, and a peak narrower than a twelfth of an octave reads as about 16, however narrow it is. resonances is null for a stem that sounds in fewer than four bars: too few to tell what stays from what passes."),
    ("could_be_note", "could_be_note is read from the song, not the audio. true: the peak may be what the track plays, because nothing it plays moves in pitch, as a drum's hits or one held note, so nothing tells a ring from the sound's own partials; or because in half the bars a note it plays has one of its first sixteen partials within half a semitone of the peak. false: the track's notes move and none of them explains a peak that stays, which is what a resonance is. null: the song does not say, since the track plays audio clips or nothing. A group is read from its tracks and a return from the tracks that feed it. A true peak can still be a ring worth cutting; a false one is the stronger evidence."),
    ("section", "daw listen RENDER --section ID prints one section in place of the report: mix, what daw listen measures of the mix in the section with its spectrum_db, and tracks, each stem's measurements there with its spectrum_db, its resonances over the section's bars and its hits in it. third_octave_hz lists the bands, translation has what the mix loses there in mono and on a small speaker, and effects what each compressor, limiter and clipper took off in the section."),
    ("stems", "tracks holds a stem for each track, group and return, as the render wrote them: a track's sound after its effects and fader and before the master's effects, a grouped track's before its group. musical_context is read from the song: how many hits each track schedules, whole and by section; hits says how hard they land."),
    ("overlap", "overlap says where two stems put their energy in the same band at the same moments: a kick and a bass both loud under 90 Hz on the same sixteenths. pairs lists up to five pairs, the most contested of the time first, and pairs_omitted counts the rest. A render of one stem, or a file, has overlap null. Every pair of stems is read but a group against its own tracks and a return against what feeds it, whose sound is theirs; a group's pair is left out where one of its tracks' pairs names the same bands."),
    ("contested", "A cell is a band over a sixteenth note, or the next note up that lasts 100 ms (cell_beats): third octaves from 89 Hz to 20 kHz, and 20-45 and 45-89 Hz below. Two stems contest a cell when the weaker is within 6 dB of the stronger and within 9 dB of the stems' sum there, so the pair is what is heard. A cell does not count when the sum there is 40 dB under its band's loudest or 70 dB under the loudest anywhere, or is the skirt of a louder band: 12 dB under the band beside it, and 4 dB more for each band further. A stem sounds in a cell when it is within 9 dB of the sum there."),
    ("pair", "a and b are the stems. band_hz is the range of bands the pair contests most, and other_bands_hz any other ranges. fraction is the contested moments over those where either stem sounds in the range; fraction_of_a and fraction_of_b are over the moments where that stem sounds, so a kick under a held bass reads fraction 0.25 and fraction_of_a 1: contested every time it hits. contested_beats is for how long in all. level_dbfs is the level of the stems' sum in the contested cells, and level_difference_db is a minus b there. keyed is true when one has a compressor keyed by the other: the duck is already in the audio and lowers the fraction. worst is up to three sections, or runs of bars in a song without sections, where the fraction is highest, with at_beat and length_beats. A range is named when its fraction is 0.25 or more, or 0.5 of one stem's moments, for 4 beats or more."),
    ("one_pair", "daw listen RENDER --overlap A B prints one pair in place of the report: ranges, every range the pair contests, each with its fraction in every section; bands_hz, the 26 bands; whole, each band's a_dbfs and b_dbfs, the stems' levels over the render, with its fraction, fraction_of_a, fraction_of_b and level_difference_db; and sections, each with its fraction in every band, null where neither stem sounds. related is true for a pair the list leaves out, such as a group and its track. It writes overlap-A-B.png: bands up, beats across, each cell in the color of the stem that is louder there, and dark where the two contest it."),
    ("translation", "translation says what the mix loses somewhere else, as arithmetic on the render: mono, summed to one channel as a mono button does it, and small_speaker, through the band a phone's speaker plays. Every loss is in decibels at or under zero, and -60 stands for 60 dB or more: gone. observations are the largest in words: a band of the mix that loses more than 3 dB in mono, with the stem that loses it there; a stem that loses more than 3 dB in mono, with where; and a stem that loses more than 20 dB on the small speaker. A stem more than 40 dB under the mix is not named, nor a group whose track is; at most five stems are named for each of the two, and observations_omitted counts the rest. daw listen RENDER --write-translation also writes mono.wav and small-speaker.wav beside the report, with their paths under translation_audio, for the person to hear what was measured: you did not hear them. small_speaker_gain_db is how far the second was lowered so that its peaks fit, 0 when it was not."),
    ("mono", "translation.mono has loss_db, the power of the mid, (L+R)/2, against the mean power of the two channels: 0 when the channels are the same, -3 when they have nothing in common, which is as far as width alone goes, and further only as they near opposite polarity. Over all frequencies it is 10*log10(1 - side_energy_fraction). band_loss_db is the same in each of the seven bands, which says where: a pad that is wide up high and one whose low end cancels can have the same loss_db, and only the second is a fault. sections has both for each section of the mix and tracks for each stem. A band more than 60 dB under the sound's loudest is null: there is nothing there to lose. mono is null for a file of one channel. A utility with mono_below_hz on the stem that loses its low end is the usual fix, and takes those bands' loss to within a decibel of 0."),
    ("small_speaker", "translation.small_speaker is the mix and each stem through a highpass at 200 Hz and a lowpass at 8 kHz, 24 dB an octave each (band_hz, slope_db_per_octave): a stand-in for a phone's speaker, not a model of one. loss_lu is the filtered mix's integrated loudness minus the mix's, and loss_db the power it keeps; sections has both for each section. tracks has each stem's loss_db, its filtered power against its own, and share_change_db, its loss minus the mix's: under zero the stem falls back on the small speaker, over zero it comes forward. A sine bass at 50 Hz loses 48 dB and is not there; driven into saturation, which gives it harmonics over 200 Hz, it loses under 20. Energy over 200 Hz says a bass's harmonics are there, not how loud its note seems."),
    ("loudness_range", "mix and each entry of sections hold three figures that stems do not: how far the loudness ranges and where it is loudest. loudness_range_lu is the range of EBU Tech 3342: the loudness of the last 3 s is read every 100 ms, the readings under -70 LUFS and those more than 20 LU under the mean of the rest are left out, and the range runs from the 10th to the 95th percentile of what is left. It is 0 for a sound at one level and 10 for one whose halves are 10 LU apart. max_short_term_lufs is the loudest 3 s and max_momentary_lufs the loudest 400 ms, which a master is held to beside integrated_lufs. They are the app's analyzer run over the saved audio, so they are what its window shows while the song plays. A figure is null under 3 s of audio, or 400 ms for the momentary; the range of a section takes a little over 3 s."),
    ("hits", "hits, of each track, says how hard its hits land. A hit is what the song schedules on the track, a pad's trigger or a note, not an onset found in the audio, and hits that start within 50 ms of one another are one: a chord, a flam. Its punch is the peak of its first 30 ms over the RMS of the 200 ms after them, or of what there is before the next hit, in decibels. punch_db is the median over the track's hits at its stem, which is after the track's effects: about 10 for a kick with a body, 60, where it stops, for a click with nothing after it, about 3 for a held tone, and under 0 for a sound that swells or is ducked as it starts. punch_in_mix_db is the same frames read in the mix, with everything else that plays then: far under punch_db, the hit is there and something covers it. punch_in_group_db, of a grouped track, is the same at its group's stem, after the group's effects, which the track's own stem is before. count is the hits that made a sound. sections has the four for each section a hit starts in, and pads, on a track that plays more than one pad, for the hits each pad starts in, with whatever else the track plays then. A compressor's attack moves punch_db: a faster one takes the front off and lowers it, a slower one lets the front through and raises it. hits is null for a group, a return and a track of audio clips, where the song schedules nothing."),
    ("effects", "effects lists every compressor, limiter and clipper of the render, the master's under master and the rest under tracks by track, group or return, in the chain's order with a Synth patch's first. Each has path, where the device is in the song as daw effect and daw set take it; type; id if it has one; keyed_by, the track a compressor's sidechain names; and what it took off, as the engine measured while it rendered: max_gain_reduction_db, mean_gain_reduction_db and fraction_over_1db_reduction, the share of the time it took more than 1 dB, over the song and under sections in each section. So a limiter that works through the drop and rests in the verse reads as one, where the whole song's numbers say only that it took 6 dB somewhere. A bypassed device has bypass true and no numbers. A limiter reaches ahead by its look-ahead, so a peak just past a section's start shows in the max of the section before it. A clipper's numbers are of the samples it cut, each by its own level: its max is the most it took off one sample, and its fraction the share of samples it cut by more than 1 dB, so a clipper that takes 3 dB off a kick's first milliseconds reads a max of 3 and a fraction near 0. effects is null for a file that is not a render, and a render an earlier engine made has no sections in it: render the song again."),
    ("compare", "daw compare BEFORE AFTER has overlap.pairs: each pair either render's list names, over the bands it was named for, with before, after and delta, the fraction's change, keyed_before and keyed_after, and by section where the two renders line up. A cut in one of the two, a keyed compressor or a part moved off the other's hits shows as the fraction falling. The mix and each stem also have spectrum_db under actual_delta and loudness_matched_delta, the change in each third octave, and the change in tilt_db_per_octave; a section has the tilt's change alone. Each stem has resonances: every one either render lists, with before_db and after_db, its height in each, measured there whether or not that render lists it (listed_before, listed_after), and delta_db, the largest change first. A bell cut at a reported resonance shows as its delta_db falling by about the cut. translation has each loss with before, after and delta: mono's loss_db and band_loss_db and small_speaker's loss_lu and loss_db for the mix, the loss_db and loss_lu of each section where the two renders line up, and each stem both renders have, with its share_change_db, the stem whose loss changed most first. The mix and each section have the change in loudness_range_lu, max_short_term_lufs and max_momentary_lufs under actual_delta, and the two maxima under loudness_matched_delta: a gain moves them and leaves the range. Each track has hits, with count, punch_db, punch_in_mix_db and punch_in_group_db before and after, by section where the renders line up and by pad: a slower attack shows as punch_db rising, and a duck on the bass as the kick's punch_in_mix_db rising. effects has devices, each compressor, limiter and clipper both renders have, matched by its id, or by its place among its channel's devices of its type when it has none, with each reduction before and after, whole and by section, the largest change first; and added and removed, the paths of those only one render has."),
    ("reading", "Overlap is energy at close levels, not masking as an ear does it, and not a fault by itself: a kick and its click layered on purpose contest every hit. It does not say which of the two should give way, which is a choice of arrangement, an equalizer's cut, a keyed compressor or a level. A stem far under another in a band is not in the list, since it is covered and not contested: read each stem's band_dbfs for that. A resonance is a measurement too, and not a fault: a peak can be the character of the sound. A loss in translation is one as well: a sub that a phone cannot play may be meant, and width costs up to 3 dB in mono by its nature. So are punch and reduction: a kick that sits under a pad may be meant to, and how much a limiter takes is a choice. Say what was found before changing the mix."),
    ("limits", "The thresholds were chosen on generated audio and not with anyone listening. A grouped track is read before its group's effects and fader. Resonances wider than a q of about 4 are not found, since they are measured against the two octaves about them: read spectrum_db for a broad bump. A part whose partials stand apart, a melody in the middle octaves, lists its notes, with could_be_note true, and a resonance among them can be left out of the five. The small speaker's corners are round numbers and not measured from a phone, and mono's loss is read in the seven bands, not in third octaves. A hit inside an audio clip is not found, a punch's 30 and 200 ms were chosen without anyone listening, and punch_in_mix_db reads everything that plays at the hit, so it says most beside punch_db and beside other tracks'. No keys, notes or taste are judged. Each call reads and measures the audio again; a three-minute song of twelve stems takes most of a minute."),
];

const REFERENCE: &[(&str, &str)] = &[
    ("command", "daw reference add FILE --name NAME analyzes a song the person points to as what good sounds like, .m4a and .mp3 too, and keeps its measurements in the workspace library, library/references/NAME/; the file's path and hash are kept and its audio is not copied, so nothing of it leaves the Mac. daw reference list names the saved ones, daw reference show NAME prints one, daw reference remove NAME forgets one. daw compare RENDER --reference NAME then compares a render, or any audio file, with it: the whole of each, and section by section. Which song is a reference is the person's choice: add the one they name."),
    ("sections", "A reference's sections are measured from its beat map: each phrase from its start to the next, named bar-N for the bar it starts on, with at and length_beats in the reference's own beats, 0 on its first downbeat. --bpm, --downbeat and --meter on add correct the map as daw samples beats takes them. daw reference sections NAME '[{\"id\": \"verse\", \"at\": 32}, {\"id\": \"drop\", \"seconds\": \"1:02\"}]' names them by hand, each from its start, in beats or as seconds or m:ss, to the next one's start and the last to the end of the sound; it reads the person's file again, which must be where it was. --measured goes back to the phrases. A reference with no beat that can be found has no measured sections and is compared whole, or by sections given in seconds."),
    ("section", "Each section holds start_seconds and end_seconds, audio with what daw listen measures of it but the true peak, relative_lu, its integrated loudness minus the whole file's, and label: high, mid or low by where its loudness sits between the reference's quietest and loudest sections, in thirds at least 1 LU wide."),
    ("matching", "matching.sections says how the song's sections were paired with the reference's. id: the two share section IDs, and each shared ID is a pair; give the reference's sections the song's IDs to choose the pairs. loudness: no ID is shared, so each side's sections are put in order of integrated loudness and the song's loudest is compared with the reference's loudest, its quietest with the quietest and those between spread evenly. none: a side has no sections, and only whole is compared. unmatched_sections lists what was left out on each side."),
    ("fields", "Every delta is mix minus reference. whole and each entry of sections hold mix, reference and delta. integrated_lufs, crest_db, stereo_correlation, side_energy_fraction, tilt_db_per_octave and band_fraction are daw listen's; whole also has estimated_true_peak_dbtp, and a section relative_lu and the reference section's label. delta.band_fraction is the change in each band's share of the power."),
    ("band_db", "delta.band_db is the mix's tonal balance against the reference's: each band's level, mix minus reference, with the median of the seven differences taken out. A mix that is louder or quieter all over reads as 0 in every band; a mix whose sub alone is 6 dB up reads sub 6 and the rest 0, which is the gain an equalizer or a fader would take. It is null where a side holds under -70 dB of its power in the band."),
    ("spectrum_db", "whole.delta.spectrum_db is the same balance in 31 third octaves, listed as methods.third_octave_hz, with the median of their differences taken out: where band_db says the low mids are 3 dB up, it says whether that is a bump at 315 Hz or the octave from 250 to 500. It is of the whole files only, since a section's third octaves follow the notes it plays and a mix in another key differs band by band for that alone; read a single low band's difference with that in mind. delta.tilt_db_per_octave is the mix's tilt minus the reference's, whole and by section: above zero the mix is brighter. A reference added before third octaves were measured has neither: add it again with daw reference add FILE --name NAME --replace."),
    ("contour", "relative_lu is how far a section sits above or below its own file's loudness, so delta.relative_lu says whether the drop lifts as far over the song as the reference's does. contour.mix_span_lu and reference_span_lu are how far apart each side's quietest and loudest sections are."),
    ("observations", "observations are the differences past a threshold, in words, the largest first: 1 LU of loudness, 1 dB of true peak, 3 dB of a band, 0.5 dB an octave of tilt, 2 dB of crest, 5 points of side energy, 1.5 LU of a section's relative loudness and 2 LU of span. They say which way and by how much, over the whole mix and by section. Over the whole mix the balance is said in third octaves, each run of bands past 3 dB one way as one range with its mean and its largest band; by section it is said in the seven bands. They are measured, not judged: a mix can differ from its reference on purpose, so say what was found and let the person choose what to follow."),
    ("limits", "A reference has no stems, so tracks are not compared. Keys, notes and arrangement are not compared, and sections are compared whole, not bar by bar. The bands are daw listen's. The thresholds were not tuned on real songs. The report is written in the render's analysis folder, or beside a file that is not a render, as reference-NAME.json; no picture is made."),
];

const JOINS: &[(&str, &str)] = &[
    ("command", "daw joins RENDER [--limit SECONDS] checks a full render of an edit: each join between two parts of a song on a track, and the file's length. RENDER is the render's folder, its report or renders/latest.json."),
    ("join", "A join is two audio clips in a row on a track, or two hits of different pads, that play different parts of one sample, each for a beat or more, the second starting as the first ends. parts names them, as audio.N or the pad."),
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
    ("level", "By default the file is the render as it is. --gain DB, --peak DBTP, --lufs LUFS or --match SAMPLE apply one gain to the whole file, held under --ceiling, a sample peak of -0.1 dBFS unless given. The export does not limit: that is a master limiter in the song, which with true_peak: true holds the true peak at its ceiling_db, so that no gain is needed to leave room (daw describe effects)."),
    ("result", "level reports the policy, gain_db, held_back_db and the loudness, peak and true peak of the file and of the render. warnings say when the ceiling held a gain back, and when a compressed file's true peak is above -1 dBTP."),
    ("record", "PATH.json beside the file records its hash, format, level and the render it came from."),
    ("where", "Deliverables go in a folder the person keeps, such as the project's exports folder. renders is a cache."),
    ("app", "In the Mac app, File > Export Audio... (shift-cmd-R) runs this same export from a save panel that opens in the project's exports folder, with the format (WAV 24- or 16-bit, AAC, MP3), the level (as rendered, --peak, --lufs or --gain) and the ceiling beside the name, and shows the measurements and warnings in a banner. The song does not change."),
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

/// Each part of the Synth's fields: label, unit, range, default, choices,
/// and whether a lane can move the field or a change swaps through a dip.
fn synth_fields() -> Json {
    use crate::describe::{Initial, Kind, SYNTH_PARTS};
    let initial = |i: Initial| match i {
        Initial::Number(x) => json!(x),
        Initial::Text(s) => json!(s),
        Initial::Flag(b) => json!(b),
        Initial::Required => json!("required"),
        Initial::Absent => Json::Null,
    };
    Json::Object(
        SYNTH_PARTS
            .iter()
            .map(|(part, fields)| {
                let rows: Vec<Json> = fields
                    .iter()
                    .map(|f| {
                        let mut row = Map::new();
                        row.insert("name".into(), json!(f.name));
                        row.insert("label".into(), json!(f.label));
                        row.insert(
                            "kind".into(),
                            json!(match f.kind {
                                Kind::Number => "number",
                                Kind::Integer => "integer",
                                Kind::Choice => "choice",
                                Kind::Flag => "flag",
                                Kind::Beats => "beats",
                                Kind::Track => "track",
                            }),
                        );
                        if !f.unit.is_empty() {
                            row.insert("unit".into(), json!(f.unit));
                        }
                        if f.choices.is_empty() && matches!(f.kind, Kind::Number | Kind::Integer | Kind::Beats) {
                            row.insert("min".into(), json!(f.min));
                            row.insert("max".into(), json!(f.max));
                        }
                        if !f.choices.is_empty() {
                            row.insert("choices".into(), json!(f.choices));
                        }
                        row.insert("default".into(), initial(f.default));
                        if f.default == Initial::Absent {
                            row.insert("absent".into(), json!(if f.name == "phase" { "random" } else { "off" }));
                        }
                        row.insert("automatable".into(), json!(f.kind == Kind::Number && !f.structural));
                        row.insert("structural".into(), json!(f.structural));
                        Json::Object(row)
                    })
                    .collect();
                (part.to_string(), Json::Array(rows))
            })
            .chain(std::iter::once((
                "macros".to_string(),
                json!([{"name": "NAME", "label": "a macro", "kind": "number", "min": 0, "max": 100, "default": 0, "automatable": true, "structural": false}]),
            )))
            .collect(),
    )
}

/// The Synth's automatable fields by part, with their interpolation domains.
fn synth_automatable() -> Json {
    use crate::describe::{Kind, SYNTH_PARTS};
    Json::Object(
        SYNTH_PARTS
            .iter()
            .map(|(part, fields)| {
                let own: Vec<(&str, Domain)> = fields
                    .iter()
                    .filter(|f| f.kind == Kind::Number && !f.structural)
                    .map(|f| (f.name, if f.log { Domain::Log } else { Domain::Linear }))
                    .collect();
                (part.to_string(), domains(&own))
            })
            .chain(std::iter::once(("macros".to_string(), json!({"NAME": "linear"}))))
            .collect(),
    )
}

fn domains(params: &[(&str, Domain)]) -> Json {
    let name = |d: &Domain| if *d == Domain::Log { "log" } else { "linear" };
    Json::Object(params.iter().map(|(field, d)| (field.to_string(), json!(name(d)))).collect())
}

/// The schema of the whole song.
pub fn schema() -> &'static Json {
    &SCHEMA
}

/// What each topic is about, as `daw describe` lists them.
const ABOUT: &[(&str, &str)] = &[
    ("start", "The commands that make a first song, from init to listen. Read this first."),
    ("project", "The song: session, samples, patterns, tracks, groups, returns, sections and the person's markers; editing, rendering and playback."),
    ("sampler", "Pads, samples and pattern events: pitch, gate, choke, stretch, and the app's Sampler."),
    ("synth", "The Synth: oscillators, filter, envelopes, LFOs, the modulation matrix, macros, patches and recipes."),
    ("midi", "MIDI tracks: note clips, notes, instruments, drum maps and MIDI files."),
    ("effects", "Insert effects and what each takes, sends, returns, groups and racks."),
    ("automation", "Lanes and points: what a lane can move and how values move between points."),
    ("edit", "Editing a finished song from audio clips: cuts, joins, crossfades, speed; and ranges of beats across the tracks: copy, insert, delete, clear, a section with what is under it."),
    ("check", "daw check: warnings about the song, each with a code: stacked clips, notes struck twice, crowded low registers."),
    ("samples", "daw samples search, analyze and like: the library's sounds found by name and by what they measure as, brightness, low end, attack, decay, noisiness, loudness and punch, with a word for each, and the samples nearest a chosen one."),
    ("beats", "daw samples beats: a song's tempo, beats, downbeats and phrases."),
    ("joins", "daw joins: checking a render's joins and length."),
    ("export", "daw export: a named WAV, AAC or MP3 file at a stated level."),
    ("listen", "daw listen and daw compare: a render measured, whole, by section and by stem, in third octaves, with each stem's resonances, where two stems overlap, what the mix loses in mono and on a small speaker, how hard each track's hits land and what each compressor, limiter and clipper took off."),
    ("reference", "daw reference and daw compare --reference: a song kept as what good sounds like, and a mix measured against it section by section."),
];

/// The topic that describes each model, for a field list that meets a
/// model another topic lists.
fn home(model: &str) -> &'static str {
    match model {
        "Pad" | "Sample" | "Event" => "sampler",
        "AudioClip" => "edit",
        "MidiTrack" | "NoteClip" | "Note" | "Instrument" | "Sampler" | "NoteMap" => "midi",
        "Synth" | "Oscillator" | "SynthFilter" | "SynthEnvelope" | "Lfo" | "Modulation" => "synth",
        "Lane" | "Point" => "automation",
        "Send" | "Group" | "Return" | "EqBand" => "effects",
        m if EFFECT_TYPES.iter().any(|k| title(k) == m) => "effects",
        _ => "project",
    }
}

fn ref_name(node: &Json) -> Option<&str> {
    node.get("$ref")?.as_str()?.strip_prefix("#/$defs/")
}

/// A number's range in a few characters: `-96..24`, `>0..2`, `>=1`.
fn range(node: &Json) -> String {
    let low = match (node.get("minimum"), node.get("exclusiveMinimum")) {
        (Some(m), _) => Some(m.to_string()),
        (None, Some(m)) => Some(format!(">{m}")),
        _ => None,
    };
    let high = node.get("maximum").or_else(|| node.get("exclusiveMaximum")).map(Json::to_string);
    match (low, high) {
        (Some(l), Some(h)) => format!(" {l}..{h}"),
        (Some(l), None) if l.starts_with('>') => format!(" {l}"),
        (Some(l), None) => format!(" >={l}"),
        (None, Some(h)) => format!(" <={h}"),
        (None, None) => String::new(),
    }
}

/// What a value of a field may be, in a few words.
fn kind(node: &Json) -> String {
    if let Some(model) = ref_name(node) {
        return format!("{model} (daw describe {})", home(model));
    }
    if node.get("discriminator").is_some() {
        return "effect (daw describe effects)".into();
    }
    if let Some(alternatives) = node.get("anyOf").and_then(Json::as_array) {
        let null = alternatives.iter().any(|a| a["type"] == "null");
        let rest: Vec<&Json> = alternatives.iter().filter(|a| a["type"] != "null").collect();
        let types: Vec<&str> = rest.iter().filter_map(|a| a["type"].as_str()).collect();
        let mut text = if types == ["integer", "number", "string"] && rest.len() == 3 {
            // A position or length: a number, or a fraction such as "1/3".
            "beats".to_string()
        } else {
            rest.iter().map(|a| kind(a)).collect::<Vec<_>>().join(" or ")
        };
        if null {
            text.push_str(" or null");
        }
        return text;
    }
    if let Some(choices) = node.get("enum").and_then(Json::as_array) {
        return choices.iter().map(plain).collect::<Vec<_>>().join("|");
    }
    if let Some(only) = node.get("const") {
        return plain(only);
    }
    match node["type"].as_str() {
        Some(t @ ("number" | "integer")) => format!("{t}{}", range(node)),
        Some("boolean") => "true|false".into(),
        Some("string") => match node["pattern"].as_str() {
            Some(p) if p == crate::schema::ID => "id".into(),
            Some(p) if p == crate::meter::PATTERN => "N/D such as 4/4, 3/4 or 6/8".into(),
            Some(_) => "sha256".into(),
            None => "text".into(),
        },
        Some("array") => {
            let mut text = format!("list of {}", kind(&node["items"]));
            if let Some(n) = node.get("maxItems") {
                text.push_str(&format!(", at most {n}"));
            }
            text
        }
        Some("object") => format!("map of names to {}", kind(&node["additionalProperties"])),
        _ => "any".into(),
    }
}

/// A JSON value as text: a string without its quotes.
fn plain(v: &Json) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

/// One field on one line: what it may be, its default, and whether it is
/// required. `null` defaults are said, since null usually means "off".
pub fn field_line(node: &Json, required: bool) -> String {
    let mut line = kind(node);
    if let Some(d) = node.get("default") {
        line.push_str(&format!(", default {}", plain(d)));
    }
    if required {
        line.push_str(", required");
    }
    line
}

/// A model's own fields, each with its line, in the schema's order.
pub fn model_fields(model: &str) -> Vec<(String, String)> {
    let own = &SCHEMA["$defs"][model];
    let required = |name: &str| own["required"].as_array().is_some_and(|r| r.iter().any(|x| x == name));
    own["properties"]
        .as_object()
        .map(|props| props.iter().map(|(name, node)| (name.clone(), field_line(node, required(name)))).collect())
        .unwrap_or_default()
}

/// The fields of a topic's models by their path in the song, each on one
/// line. Models the topic describes are listed field by field; any other
/// is named with the topic that describes it.
struct Listing<'a> {
    expand: &'a [&'a str],
    lines: Map<String, Json>,
    open: Vec<String>,
}

impl Listing<'_> {
    fn model(&mut self, prefix: &str, model: &Json) {
        let required = |name: &str| model["required"].as_array().is_some_and(|r| r.iter().any(|x| x == name));
        for (name, node) in model["properties"].as_object().into_iter().flatten() {
            let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}.{name}") };
            self.field(&path, node, required(name));
        }
    }

    /// The models a node refers to, and whether it may be null.
    fn refs(node: &Json) -> (Vec<&str>, bool) {
        if let Some(model) = ref_name(node) {
            return (vec![model], false);
        }
        let alternatives = node.get("anyOf").and_then(Json::as_array);
        let refs: Vec<&str> = alternatives.into_iter().flatten().filter_map(ref_name).collect();
        let null = alternatives.into_iter().flatten().any(|a| a["type"] == "null");
        (refs, null)
    }

    fn listed(&self, model: &str) -> bool {
        self.expand.contains(&model) && !self.open.iter().any(|m| m == model)
    }

    fn descend(&mut self, path: &str, model: &str) {
        self.open.push(model.to_string());
        self.model(path, &SCHEMA["$defs"][model]);
        self.open.pop();
    }

    fn line(&mut self, path: &str, text: String) {
        match self.lines.get_mut(path) {
            Some(Json::String(have)) => {
                have.push_str("; ");
                have.push_str(&text);
            }
            _ => {
                self.lines.insert(path.to_string(), json!(text));
            }
        }
    }

    fn field(&mut self, path: &str, node: &Json, required: bool) {
        let (refs, null) = Self::refs(node);
        if !refs.is_empty() {
            for model in refs {
                if self.listed(model) {
                    if null {
                        self.line(path, format!("null or the fields below, default {}", plain(&node["default"])));
                    }
                    self.descend(path, model);
                } else {
                    self.line(path, field_line(node, required));
                }
            }
            return;
        }
        let items = &node["items"];
        let (item_refs, _) = Self::refs(items);
        if node["type"] == "array" && !item_refs.is_empty() {
            let each = format!("{path}[]");
            for model in item_refs {
                if self.listed(model) {
                    self.descend(&each, model);
                } else {
                    self.line(&each, kind(&json!({"$ref": format!("#/$defs/{model}")})));
                }
            }
            return;
        }
        if let Some(model) = node.get("additionalProperties").and_then(ref_name) {
            if self.listed(model) {
                self.descend(&format!("{path}.ID"), model);
                return;
            }
        }
        self.line(path, field_line(node, required));
    }
}

/// The field lines of a topic: each root model at its path in the song,
/// listing the `expand` models field by field.
fn listing(roots: &[(&str, &str)], expand: &[&str]) -> Json {
    let mut listing = Listing {
        expand,
        lines: Map::new(),
        open: Vec::new(),
    };
    for (path, model) in roots {
        if *model == "Project" {
            listing.model(path, &SCHEMA);
        } else {
            listing.descend(path, model);
        }
    }
    Json::Object(listing.lines)
}

/// The fields of a topic, or None for a topic of commands alone.
fn fields(topic: &str) -> Option<Json> {
    Some(match topic {
        "project" => listing(
            &[("", "Project")],
            &["Session", "Sample", "Pattern", "Event", "Track", "Clip", "Section", "Marker", "Master", "Group", "Return", "Send"],
        ),
        "sampler" => listing(
            &[("samples.ID", "Sample"), ("tracks[].pads.ID", "Pad"), ("patterns.ID.events[]", "Event")],
            &["Sample", "Pad", "Event"],
        ),
        // By their paths in the patch, as daw synth set, lanes and the matrix
        // take them; the patch is tracks.T.instrument.synth.
        "synth" => listing(
            &[("", "Synth")],
            &["Synth", "Oscillator", "SynthFilter", "SynthEnvelope", "Lfo", "Modulation"],
        ),
        "midi" => listing(&[("tracks[]", "MidiTrack")], &["MidiTrack", "NoteClip", "Note", "Instrument", "Sampler", "NoteMap"]),
        "effects" => {
            let roots: Vec<(&str, String)> = EFFECT_TYPES.iter().map(|k| (*k, title(k))).collect();
            let mut roots: Vec<(&str, &str)> = roots.iter().map(|(k, m)| (*k, m.as_str())).collect();
            roots.extend([("tracks[].sends[]", "Send"), ("groups[]", "Group"), ("returns[]", "Return")]);
            let mut expand: Vec<String> = EFFECT_TYPES.iter().map(|k| title(k)).collect();
            expand.extend(["EqBand".into(), "Send".into(), "Group".into(), "Return".into()]);
            listing(&roots, &expand.iter().map(String::as_str).collect::<Vec<_>>())
        }
        "automation" => listing(&[("tracks[].automation[]", "Lane")], &["Lane", "Point"]),
        "edit" => listing(&[("tracks[].audio[]", "AudioClip")], &["AudioClip"]),
        _ => return None,
    })
}

/// `daw describe` with no topic: the topics, a line each.
pub fn topics() -> Json {
    json!({
        "topics": texts(ABOUT),
        "usage": "daw describe TOPIC prints what the topic's fields mean and, for each field, its path, what it takes and its default; --schema prints the JSON Schema as well. A command's --help lists the fields it takes.",
    })
}

/// The first-song recipe: `daw describe start`.
const START: &[(&str, &str)] = &[
    ("daw init song --tempo 96 --bars 8", "Makes song/song.yaml: eight bars of 4/4 at 96 BPM (--time-signature 3/4 for a waltz). song is the PROJECT every other command takes."),
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
];

/// What `daw describe start` prints.
fn start() -> Json {
    let steps: Vec<Json> = START.iter().map(|(run, does)| json!({"run": run, "does": does})).collect();
    let chord = |at: i64, names: [&str; 3]| -> Vec<Json> {
        names.iter().map(|n| json!({"pitch": n, "at": at, "duration": 8})).collect()
    };
    let notes: Vec<Json> = [(0, ["C4", "D#4", "G4"]), (8, ["A#3", "D4", "F4"]), (16, ["G#3", "C4", "D#4"]), (24, ["A#3", "D4", "G4"])]
        .into_iter()
        .flat_map(|(at, names)| chord(at, names))
        .collect();
    json!({
        "steps": steps,
        "batch": {
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
        },
        "semantics": texts(&[
            ("placeholders", "KICK_PATH and the other paths come from daw samples search. Everything else runs as written in an empty folder."),
            ("batch", "A batch file lists commands as objects: op is the command (set, track.add, clip.add, note.add, synth.add, effect.add and the others), and its other keys are the command's arguments and the fields of what it adds. A label names the step in the change log and for undo."),
            ("pace", "With the app open (daw status says host: true), run each step as soon as it is decided, as they are listed here, so the person watches each sound, part and clip land on the timeline and in the activity as it is made and can say what they think before the next; do not plan the whole song and then run everything at once. A batch is one musical edit, as the chords here, not the whole song. A host whose agent sent nothing for five minutes says so in the next edit's reply, as hint."),
            ("next", "daw play song plays it, or open the folder in the app. daw map song shows where each track plays, bar by bar, daw inspect song summarizes it, daw get song PATH reads any part, and daw set song PATH VALUE changes any field. daw --help lists the commands, a command's --help what it takes, and daw describe the topics."),
        ]),
    })
}

/// What `daw describe TOPIC` prints: what the topic's fields mean and a line
/// for each field, or None for an unknown topic.
pub fn describe(topic: &str) -> Option<Json> {
    if topic == "start" {
        return Some(start());
    }
    let mut full = describe_with_schema(topic)?;
    let full = full.as_object_mut().expect("a topic is an object");
    let mut out = Map::new();
    out.insert("semantics".into(), full.remove("semantics").expect("a topic has semantics"));
    if let Some(lines) = fields(topic) {
        out.insert("fields".into(), lines);
    }
    // What the schema cannot say: the matrix, what lanes can move, what
    // check's codes mean and the lines between a sample's words.
    for key in ["modulation", "automatable", "codes", "lines"] {
        if let Some(v) = full.remove(key) {
            out.insert(key.into(), v);
        }
    }
    Some(Json::Object(out))
}

/// What `daw describe TOPIC --schema` prints: the JSON Schema of the topic's
/// models with what their fields mean, or None for an unknown topic.
pub fn describe_with_schema(topic: &str) -> Option<Json> {
    Some(match topic {
        "start" => start(),
        "project" => json!({"schema": schema(), "semantics": texts(PROJECT)}),
        "sampler" => json!({
            "schema": {"pad": model("Pad"), "event": model("Event"), "sample": model("Sample")},
            "semantics": texts(&[PROJECT, SAMPLER_DEVICE].concat()),
        }),
        "synth" => json!({
            "schema": {"synth": model("Synth")},
            "fields": synth_fields(),
            "modulation": {
                "sources": texts(crate::rules::MOD_SOURCES),
                "targets": texts(crate::rules::MOD_TARGETS),
            },
            "automatable": {
                "paths": "instrument.FIELD on the MIDI track, FIELD being a path in the patch: a field of the synth, oscillators.ID.FIELD, filter.FIELD, envelopes.ID.FIELD, lfos.ID.FIELD, macros.NAME or effects.REF.FIELD (effects.REF.bands.N.FIELD for an equalizer), with the effect fields of daw describe automation",
                "fields": synth_automatable(),
            },
            "wavetables": crate::schema::WAVETABLES,
            "semantics": texts(SYNTH),
        }),
        "midi" => json!({
            "schema": {"track": model("MidiTrack"), "clip": model("NoteClip"), "instrument": model("Instrument")},
            "semantics": texts(MIDI),
        }),
        "effects" => json!({
            "schema": Json::Object(EFFECT_TYPES.iter().map(|k| (k.to_string(), model(&title(k)))).collect()),
            "routing": {"send": model("Send"), "group": model("Group"), "return": model("Return")},
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
        "edit" => json!({"schema": {"audio_clip": model("AudioClip")}, "semantics": texts(EDIT)}),
        "check" => json!({"semantics": texts(CHECK), "codes": texts(crate::check::CODES)}),
        "samples" => {
            let said: Json = serde_json::from_str(include_str!("../../../../src/agent_daw/sample_words.json"))
                .expect("sample_words.json is JSON");
            json!({"semantics": texts(SAMPLES), "lines": said["lines"]})
        }
        "beats" => json!({"semantics": texts(BEATS)}),
        "joins" => json!({"semantics": texts(JOINS)}),
        "export" => json!({"semantics": texts(EXPORT)}),
        "listen" => json!({"semantics": texts(LISTEN)}),
        "reference" => json!({"semantics": texts(REFERENCE)}),
        _ => return None,
    })
}
