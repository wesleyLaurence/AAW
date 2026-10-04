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

pub const TOPICS: &[&str] = &["project", "sampler", "synth", "midi", "effects", "automation", "edit", "beats", "joins", "export"];

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
    ("projects", "PROJECT is a project's folder or the song.yaml in it. daw projects lists the projects open in the app or another host, the window in front first, with each one's title; --all adds the ones the app knows that are not open. daw move and daw copy save a project under another name, as Save As does in the app: a running host carries on there and still answers at the old path, and a command sent there is told where the project is now by project in its result and a notice on stderr."),
    ("playback", "daw play PROJECT --from BEAT plays through the default output. Samples sounding at the start position are picked up partway through; effects start empty there. A render always runs from the start of the song."),
    ("effects", "tracks[].effects, returns[].effects and master.effects are serial insert chains; see daw describe effects."),
    ("returns", "returns[] are reverb/delay buses fed by tracks[].sends; see daw describe effects."),
    ("automation", "tracks[].automation, returns[].automation and master.automation move gain, pan, send levels and effect parameters over time; see daw describe automation."),
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
    ("wavetables", "wave: wavetable reads one cycle from table: organ (drawbars), bright (every harmonic to the 64th), hollow (odd harmonics, between a triangle and a square), vowel (an ah: formants at the fifth, ninth and twentieth harmonics), fold (a sine through a wavefolder) and steps (a sine in eight steps), or the ID of a sample in the project whose file is one cycle: a WAV of up to 65536 frames, summed to mono, its DC removed and its level normalized, so a cycle drawn or cut from any sound becomes a wave. Every table is read bandlimited: harmonics above half the sample rate are left out at each pitch, so a table does not alias. A patch naming a sample loads into a song that has the sample. Multi-frame tables with a sweepable position are a Later line."),
    ("effects", "effects inside the patch is a chain of the song's effect kinds (daw describe effects) run on the sum of the voices before the track's inserts, so a patch carries its chorus, saturation or reverb and sounds finished on any track; daw synth audition hears it without --track-chain. A compressor here has no sidechain. daw effect add SONG tracks.T.instrument.synth KIND [FIELDS], daw effect remove, move and bypass work on the chain as on a track's, and daw set reaches a field as tracks.T.instrument.synth.effects.REF.FIELD. Lanes reach them as instrument.effects.REF.FIELD, REF an id or index."),
    ("filter", "One filter a voice, after the oscillators routed through it: mode lowpass, highpass, bandpass or notch, slope_db_per_octave 12 or 24, cutoff_hz, resonance_percent (0 is a Q of a half; 100 rings just short of self-oscillation), drive_db into a soft clip before the filter, and keytrack_percent, 100 moving the cutoff an octave an octave from middle C. enabled: false passes the oscillators through. The default is a lowpass open at 20 kHz, which changes nothing."),
    ("envelopes", "envelopes is a mapping; amp is always there and shapes each voice's level, and others (env2, env3) do nothing until the matrix uses them. attack_ms rises linearly to full; decay_ms falls exponentially to sustain_percent, reaching it at decay_ms; release_ms falls exponentially to silence after the note-off, reaching it at release_ms. A voice ends when its amp release does."),
    ("lfos", "lfos is a mapping of up to four, bipolar, -1 to 1. shape is sine, triangle, saw (falling), square or sample_hold (a new random value a cycle). rate_hz is cycles a second; rate_beats, when given, replaces it with one cycle in so many beats at the song's tempo, so 1/2 is an eighth note and 4 a bar. retrigger: true starts the cycle at each note from phase_percent; otherwise the LFO runs from the start of the song and every voice shares its phase, so a synced LFO lands on the bar."),
    ("modulation", "modulation lists {source, target, amount}. source is an envelope or LFO by its ID, velocity, note, random or macros.NAME; target is listed under targets with the unit of amount, which is how far the target moves at full modulation: semitones for a pitch, octaves for cutoff_hz and envelope times (1 doubles, -1 halves), dB for a level, points for a percentage, and the pan's own units. Envelopes, velocity, random and macros are unipolar, 0 to 1; note and LFOs are bipolar. An entry on an envelope's field is taken when the note starts and holds for the note. Automation and the matrix add: a lane moves the field's value and the matrix moves it from there. An entry naming a source or target the patch lacks is refused."),
    ("macros", "macros is a mapping of up to eight named knobs, 0 to 100, that do nothing but through the matrix: {source: macros.tone, target: filter.cutoff_hz, amount: 3} opens the filter three octaves as tone goes from 0 to 100. They are the knobs to automate (instrument.macros.tone) and to reach for first; daw check names a macro no entry uses."),
    ("voices", "voices is 1 to 16; 1 is monophonic. Past it the oldest releasing voice is stolen, else the oldest sounding, over a 5 ms fade. glide_ms slides each new note's pitch from the pitch of the last note started: portamento with one voice. velocity_percent is how much velocity moves the level: 100 is linear, as a sampler's, 0 none. A voice is a function of the patch, the note and the frames since it started, with its random phases and random value from seed, the track and the note's place in the track, so a render is the same bytes twice and playback from the start equals it. Playback from a locate chases the notes sounding there: each starts with its envelopes and free LFOs where time would have brought them and its filter empty."),
    ("automation", "Lanes on a MIDI track reach the synth as instrument.FIELD: instrument.filter.cutoff_hz, instrument.oscillators.a.level_db, instrument.oscillators.a.unison_detune_cents, instrument.macros.tone, instrument.envelopes.amp.release_ms, instrument.lfos.lfo1.rate_hz, and the patch's effects as instrument.effects.REF.FIELD. automatable lists every field a lane can move; frequencies and LFO rates interpolate in the log domain. A field the panel or the agent sets glides over 5 ms while the song plays; a change of wave, table, unison, filter mode, slope or routing, or of the patch's effects, swaps through a 10 ms dip."),
    ("commands", "daw synth add SONG TRACK; daw synth show SONG TRACK prints the patch as the song holds it, fields at their defaults left out; daw synth set SONG TRACK PATH VALUE [PATH VALUE ...] sets fields by their path in the patch as one undo step, a value being a number, a word or a JSON object, so oscillators.sub '{\"wave\": \"sine\", \"octave\": -1, \"filter\": false}' adds an oscillator and a null removes one; daw synth mod SONG TRACK SOURCE TARGET AMOUNT adds an entry or changes its amount, and --remove takes it out; daw synth audition SONG TRACK [--notes C2,C3] [--velocity 100] [--length-beats 2] [--track-chain] renders the notes one after another through the patch, and with --track-chain the track's effects, to a WAV under renders/auditions and replies with its path, peak, loudness and spectral centroid, so there is something to hear and daw listen can measure it. daw set SONG tracks.T.instrument.synth.PATH VALUE reaches any field too, and a labeled daw batch makes a designed sound one step."),
    ("recipes", "Each is a factory patch too, loaded with daw synth add SONG TRACK --patch NAME and read with daw patch show NAME. Sub bass: a sine, and a saw an octave up at -12 dB under a lowpass at 200 Hz; velocity to filter.cutoff_hz by 1. Pluck: a saw with unison 2, amp attack 1 decay 300 sustain 0 release 200, env2 attack 0 decay 250 sustain 0 on filter.cutoff_hz by 4 over a cutoff of 300 Hz with resonance 20, and a dotted-eighth delay mixed in low. Pad: a saw with unison 3 at 8 cents over a sub, amp attack 400 release 900, cutoff 900 opened by env2, lfo1 at 0.3 Hz on pitch by 0.08, a chorus and a reverb in the patch. Supersaw: one saw with unison 7 at 18 cents and full width, a chorus behind it. Lead: a square and a saw a fifth up at -9 dB, glide_ms 60 with one voice, lfo1 sine 5 Hz retriggered on pitch by 0.3 for vibrato, tube saturation. Organ: the organ wavetable with a sub and a slow chorus. Kick: a sine with env2 attack 0 decay 40 sustain 0 on pitch by 36, so the pitch falls three octaves onto the note, and the amp decay 300 sustain 0, plus a short noise burst with its own envelope, into soft saturation. Hat: noise with a highpass at 6 kHz, amp decay 60 sustain 0. Riser: noise and a saw through a lowpass whose cutoff an envelope opens over 4 beats, with resonance 40, into a long reverb. Every amount is in the target's unit; start from these and listen with daw synth audition."),
    ("limits", "LFO fields are not matrix targets; the triangle is not bandlimited; a wavetable is one cycle, not a sweep of frames; a sample's cycle is read at most 65536 frames long."),
];

const SAMPLER_DEVICE: &[(&str, &str)] = &[
    ("device", "The app's Sampler, dragged from the browser's Instruments onto a MIDI track, is the track's sampler instrument. Empty it is {sampler: {pads: {}, map: []}}, which plays nothing. A sample dropped on it loads one pad on every note at its pitch, as it is at middle C: {sampler: {pads: {NAME: {sample: S}}, map: [{notes: [0, 127], pad: NAME, pitched: true}]}}, the same as a sample dropped on the track's header makes. Its panel shows the file's waveform and sets the pad's mode, gain_db, pan, transpose, start_seconds, end_seconds, attack_ms, release_ms and reverse, which daw pad set SONG TRACK NAME sets too, and the sample's root_note, which daw set SONG samples.S.root_note NOTE sets; Measure in the panel sets it to the pitch daw samples analyze finds. A sampler of several pads, as daw instrument map makes, is listed in the panel and not drawn as the device."),
];

const MIDI: &[(&str, &str)] = &[
    ("tracks", "A track with type: midi has an instrument and note clips under clips, and no pads, pattern clips or audio clips. Its gain, pan, effects, sends and automation are any track's. daw track add SONG ID --type midi makes one with no instrument."),
    ("clips", "A note clip {id, at, length_beats, notes} owns its notes: at is the song beat it starts on, and each note's at is beats from the clip's start. A copy (daw clip duplicate) owns copies of the notes, so changing one clip changes nothing in another. Clips do not repeat; duplicate one to play it again."),
    ("notes", "A note is {id, pitch, at, duration, velocity}. pitch is a MIDI number, 0 to 127. A command or the document also takes a name, which is stored as its number: C4 is 60, middle C, so C3 is 48 and C2 36. velocity is 1 to 127, 100 unless given, and scales the level linearly. duration is more than 0. Chords are notes at the same at."),
    ("timing", "Positions are exact and nothing is snapped to a grid: write 1/3, 2/3 for triplets, and a decimal such as 1.975 for a note a little ahead of beat 2 (12.5 ms at 120 BPM) or 2.025 behind it."),
    ("ids", "A clip written without an id is given the next free clipN, unique in the song; a note, the next free nN, unique in its clip. IDs stay through edits, saving and reopening. A note is addressed as its clip's path with notes.ID, e.g. tracks.keys.clips.clip1.notes.n3, or by @N while a host runs. A command that makes clips or notes replies with their paths."),
    ("edges", "A note sounds from its start to the end of its duration or of its clip, whichever is first. A note that starts at or after its clip's end is kept and does not play, and lengthening the clip brings it back. A note may start before its clip, at a negative at, as a clip shortened from its left edge (daw clip trim --start) leaves the notes it passed: it is kept and does not play, even where it would last into the clip, and moving the start back brings it back. daw check and daw note list name notes outside their clip."),
    ("instrument", "instrument is null, and the notes play nothing and are kept, or {sampler: {pads, map}}, or {synth: {...}}, the Synth of daw describe synth. pads are the pads of daw describe sampler, by name. map lists {notes, pad, pitched}: notes is one note or [LOW, HIGH], inclusive, and no note may be in two entries. A pitched entry plays its pad repitched from its sample's root_note to the note, or from middle C (C4, 60) when the sample has none, as Ableton's Simpler does; an entry that is not pitched plays its pad as it is, whatever the note, as a drum rack does. A note no entry maps is silent, and daw check names it. Replacing or removing the instrument leaves the notes as they are."),
    ("drums", "General MIDI drum notes are the usual map: 36 (C2) kick, 38 (D2) snare, 42 (F#2) closed hat, 46 (A#2) open hat, 49 (C#3) crash."),
    ("voices", "Each note plays its own voice, overlapping notes of the same pitch included. A gate pad releases at the note-off over its release_ms; a one_shot pad plays its sample through. A voice never outlasts its sample: there is no sustain loop. Choke groups work as on any track."),
    ("commands", "daw clip add SONG TRACK --length-beats 4 [--at 16]; daw note add SONG CLIP --pitch C4 --duration 1 [--at 0 --velocity 96], or --notes '[{...}, ...]' for many; daw note set SONG NOTE --velocity 80; daw note move SONG NOTE... --by -1/48; daw note transpose SONG NOTE... --by 12; daw note remove SONG NOTE...; a clip given to move, transpose or remove stands for all its notes. daw clip duplicate, move, resize, trim and remove place clips; daw clip trim SONG CLIP --start BEAT --end BEAT moves either edge to a song beat and leaves the notes where they are in the song. daw note list SONG CLIP|TRACK [--from BEAT --to BEAT] reads notes with their names and song beats. daw instrument set SONG TRACK JSON attaches or replaces the instrument, daw instrument remove takes it off, daw instrument map SONG TRACK NOTES PAD [--pitched] adds a map entry, and daw pad add/set/remove edit the sampler's pads. A labeled daw batch makes a phrase and its variations one undo step."),
    ("version", "A song with a MIDI track is saved with schema_version: 2, which an engine from before MIDI tracks refuses. A song without one is saved as version 1, as before."),
    ("files", "daw midi import SONG FILE [--track TRACK] [--at BEAT] makes a note clip of a Standard MIDI file of one part, type 0 or 1, whose notes are in one file track and on one channel; a file of more parts is refused with them named. The clip goes on --track, a MIDI track, or else on a new MIDI track named after the file, at --at (0 unless given). It starts at the file's beat 0 and lasts to its last note's end in whole bars, and the song grows to hold it. Positions are exact. The file's tempo is not taken: the notes keep their beats, and the reply's file_tempo is for information. The reply's left_out counts what the song does not hold, by kind (sustain pedal, pitch bend, controllers, program changes, aftertouch and more), and adjusted counts notes with no note-off or no length. daw midi export SONG CLIP FILE writes the clip's notes that play as a type 0 file at 960 ticks a beat with the song's tempo, on channel 1; its reply counts notes outside the clip, notes shortened to its end, notes rounded to a tick, and a note inside a longer one of its pitch, whose lengths a MIDI file cannot keep apart."),
    ("limits", "No controllers, pitch bend or pedal, no recording, and clips do not loop. A MIDI file is one part; a file of several parts and its tempo are not read."),
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
    ("chorus", "A stereo chorus: each channel through a delay of delay_ms moved depth_ms either side by a sine at rate_hz, the right channel a quarter cycle behind the left, so the two sides drift apart. mix_percent (50 unless given) blends it under the dry signal; 100 is the wet signal alone, a vibrato. rate_hz, depth_ms, delay_ms and mix_percent can be automated. Adds no latency; the sweep is a function of the frames processed, so a render is the same bytes twice."),
    ("saturation", "The signal driven by drive_db into a curve: soft is tanh, odd harmonics that thicken as the drive rises; hard clips at full scale; tube is asymmetric, even harmonics too, with its DC removed. output_db trims the result and mix_percent (100 unless given) blends it under the dry signal, so a little tube at 30 percent is a warmth and a hard clip at 100 is a distortion. drive_db, output_db and mix_percent can be automated. No oversampling: a hard clip high up aliases a little."),
    ("mix", "delay, reverb, chorus and saturation take mix_percent: output = input * (1 - mix) + wet * mix. The default is 100, fully wet, for delay, reverb and saturation, and 50 for chorus; set a delay's or reverb's lower when used as a track insert."),
    ("returns", "returns[] are buses with id, gain_db, pan, mute and effects. tracks[].sends lists {to: RETURN, gain_db, pre_fader}. Post-fader sends (default) tap after the track's gain and pan; pre-fader after its inserts. Muted or solo-muted tracks send nothing. Returns are never solo-muted. A return compressor may sidechain a track; sidechains cannot name a return. Returns cannot send."),
    ("report", "Render reports list each effect with latency_frames; dynamics add max and mean gain reduction and the fraction of frames reduced over 1 dB. Returns appear under tracks with kind: return and their senders."),
    ("automation", "Effect parameters can change over time with automation lanes; see daw describe automation. Give an effect an id to address it by name."),
    ("synth", "A Synth patch carries a chain of these kinds of its own, tracks[].instrument.synth.effects, run before the track's inserts; see daw describe synth."),
    ("limits", "No clipper, phaser, groups, return-to-return sends or impulse-response samples yet."),
];

const AUTOMATION: &[(&str, &str)] = &[
    ("lanes", "tracks[].automation, returns[].automation and master.automation list lanes {param, points}. A lane overrides the static value for the whole song. One lane per parameter."),
    ("params", "Tracks: gain_db, pan, sends.RETURN.gain_db, effects.REF.FIELD, and on a MIDI track with a synth instrument.FIELD, such as instrument.filter.cutoff_hz, instrument.macros.tone or instrument.effects.REF.FIELD for the patch's own effects (daw describe synth). Returns: gain_db, pan, effects.REF.FIELD. Master: gain_db (replaces session.master_gain_db) and effects.REF.FIELD. REF is an effect id or zero-based index; eq fields are effects.REF.bands.N.FIELD. Automatable effect fields are listed under automatable."),
    ("points", "points are {at, value, curve, shape} in time order; at is in beats like any position and may equal the session length. Values use the parameter's own units and bounds."),
    ("curves", "curve shapes the segment after its point. linear (default) moves in the parameter's domain: dB, pan and percent linearly, frequencies and q in equal ratios per beat (log). hold keeps the value until the next point. shape bends a linear segment, from -1 to 1: above zero it starts slowly and finishes fast (at 0.5 progress goes as its square, at 1 as its fourth power), below zero it starts fast and finishes slowly, and 0 is straight. A sweep that should hold back and then open needs two points and a shape. Two points at the same at jump there; at most two may share a position."),
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
    ("clips", "An edit keeps parts of a finished song and joins them. A part is an audio clip on a track: tracks[].audio lists {sample, at, source_start_seconds, source_end_seconds, lead_ms, gain_db, fade_in_ms, fade_out_ms, fade_curve, source_bpm, stretch}. at is the beat that source_start_seconds of the file plays on; without source_end_seconds the clip plays to the end of the file. A clip plays its file at full level, a mono file on both sides."),
    ("session", "Set session.tempo to the song's measured tempo (daw describe beats), so a beat of the song is a beat of the session. Set session.sample_rate to the song's own rate when that is 44100 or 48000, so the song is not resampled, and session.master_gain_db to 0, which is -6 in a new song."),
    ("build", "daw track add SONG TRACK, then daw audio add SONG TRACK SAMPLE --at BEAT --source-start-seconds S: one clip from the song's downbeat at S seconds, the first beat to keep, to the last. Then daw audio cut SONG TRACK --from BEAT --to BEAT for each part to remove: it takes those beats out, moves what follows earlier and crossfades the join. Beats are the session's; daw timeline SONG --seconds TIME converts a time on the edit's timeline, and daw samples beats gives times in the song."),
    ("commands", "daw audio add, move CLIP --at BEAT --track TRACK, split CLIP --at BEAT, trim CLIP --start BEAT --end BEAT, crossfade CLIP and cut TRACK --from BEAT --to BEAT. CLIP is tracks.TRACK.audio.N, or the @N reference daw inspect lists while a host runs. A move takes a clip to another beat, another track or both, with its audio; to remove one, daw remove SONG CLIP. A split makes two clips that play exactly as the one did. A trim moves an edge to a beat and leaves the audio where it is."),
    ("lead", "lead_ms is how long before its beat a clip starts, so a cut sits in the quiet before a hit and the hit is whole; 5 is usual. A clip that ends where another on its track begins leaves from where that one starts, so their join is at one place before the beat whatever each clip's own lead."),
    ("fades", "fade_in_ms is over the clip's start and should be shorter than its lead. fade_out_ms follows where the clip leaves, so the next clip fades in under it: 12 out and 4 in suit a cut before a hit; use longer only under a sustained sound. fade_curve is equal_power, which keeps the level across two different parts of a song, or linear, which keeps it across audio that is the same. daw audio crossfade CLIP [--ms 12] [--in-ms 4] [--lead-ms 5] sets the join between a clip and the one before it."),
    ("counts", "Keep and remove whole bars so the count carries across a join: every part kept and every part removed is a multiple of four beats and starts on a downbeat."),
    ("ends", "Give the first clip a short fade_in_ms and the last a fade_out_ms. session.length_beats must reach the end of the last sound: daw timeline SONG --fit sets it there."),
    ("one_shots", "A sound added to the edit is its own sample on its own track, as an audio clip or a pad. To end a pad's sound on a beat, daw timeline SONG --end-at BEAT --pad TRACK.PAD gives the beat it starts on."),
    ("level", "A mastered song is at full scale and a render refuses to clip. With the song's track and the master at 0 dB, put a limiter on master.effects rather than turning the song down, so the song stays as loud as the original and only what is added on top is taken down; see daw describe effects. daw export --match SAMPLE reports how the file's loudness compares with the song's."),
    ("speed", "To make an edit shorter without changing its pitch, give each clip source_bpm, the song's measured tempo, and stretch: preserve_pitch, then raise session.tempo. Everything placed in beats stays where it is, so the joins hold; sounds without source_bpm keep their own length. daw timeline SONG --tempo-for SECONDS gives the tempo that makes the song that long. A few percent is the normal range; ask before going past about 8."),
    ("check", "Render, run daw joins on the render (daw describe joins), then daw export (daw describe export)."),
    ("pads", "A part can also be a pad with start_seconds and end_seconds, played by a pattern event, as edits were built before audio clips; daw joins checks those too. A pad's event plays at velocity 100 of 127 unless given 127, and a mono pad is 3 dB down."),
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

/// What `daw describe TOPIC` prints, or None for an unknown topic.
pub fn describe(topic: &str) -> Option<Json> {
    Some(match topic {
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
        "edit" => json!({"schema": {"audio_clip": model("AudioClip")}, "semantics": texts(EDIT)}),
        "beats" => json!({"semantics": texts(BEATS)}),
        "joins" => json!({"semantics": texts(JOINS)}),
        "export" => json!({"semantics": texts(EXPORT)}),
        _ => return None,
    })
}
