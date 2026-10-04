# The Synth — engine and commands implemented October 3, 2026

Status: the first of four items is built. The engine, the patch in the song,
`daw synth` and `daw describe synth`, automation of the patch's fields,
`daw check` warnings and a panel of plain fields in the app are built and
described here in the present tense. Patches as files, the panel's drawings
and unison, wavetables and the patch's effects are the three items that
remain, described under [What remains](#what-remains). D69 records what the
person settled on October 3, 2026, and D71 what was decided in building the
first item.

## What

A polyphonic synthesizer the agent operates from the terminal and the person
from the device panel, with the same controls, on a MIDI track: oscillators, a
filter, envelopes, LFOs, a modulation matrix and macros, in the shape of a
modern software synth. The sound is a text patch inside the song, so a project
is self-contained. It plays what the sampler plays: the notes of the track's
note clips. It is for basses, leads, chords, pads, plucks, drums designed from
nothing and sound effects. It is not a plugin host and not a generative model.

## Why

Agents asked for a sound the library lacked wrote scripts that synthesized a
sub or a glide into a WAV the DAW then could not see or change. The sampler
plays what exists; a synth makes what does not, in the song, where every value
is readable and every change is a command. The person's expectation is a
Serum-class instrument (D69).

## The sound is in the song

```yaml
tracks:
  - id: lead
    type: midi
    instrument:
      synth:
        patch: Soft Pad              # where it came from; nothing reads it
        voices: 8                    # 1 is monophonic
        glide_ms: 0
        velocity_percent: 100        # how much velocity moves the level
        seed: 1                      # random phases and the random source
        oscillators:
          a: {wave: saw, level_db: -6}
          b: {wave: saw, level_db: -9, semitones: 7, detune_cents: -5}
          sub: {wave: sine, level_db: -12, octave: -1, filter: false}
        filter:
          mode: lowpass              # lowpass, highpass, bandpass, notch
          slope_db_per_octave: 24    # 12 or 24
          cutoff_hz: 900
          resonance_percent: 15
          drive_db: 3
          keytrack_percent: 50
        envelopes:
          amp: {attack_ms: 400, decay_ms: 300, sustain_percent: 80, release_ms: 900}
          env2: {attack_ms: 1200, decay_ms: 0, sustain_percent: 100, release_ms: 600}
        lfos:
          lfo1: {shape: sine, rate_hz: 0.3}
          lfo2: {shape: triangle, rate_beats: 2, phase_percent: 0, retrigger: false}
        modulation:
          - {source: env2, target: filter.cutoff_hz, amount: 2}         # octaves
          - {source: lfo1, target: oscillators.b.pitch, amount: 0.08}   # semitones
          - {source: macros.tone, target: filter.cutoff_hz, amount: 3}
          - {source: velocity, target: filter.cutoff_hz, amount: 1}
        macros:
          tone: 40
    clips: [...]
```

`instrument: {synth: ...}` is the second kind of instrument beside `sampler`;
an instrument is one or the other. Every value is in real units, and every
field has a label, unit, range, default and whether it is automatable or
structural, printed by `daw describe synth` and drawn by the panel, as effects
are (D44). Unknown fields are rejected. `{synth: {oscillators: {a: {}}}}` is a
valid patch: one saw with the plain envelope, which is what `daw synth add`
attaches. A song with a synth is saved as schema version 2, as any song with a
MIDI track is.

**Oscillators** are a mapping of one to four, keyed by an ID the patch chooses
(`a`, `b`, `sub`, `noise` are conventions, not slots), summed in the order
written. `wave` is `sine`, `triangle`, `saw`, `square`, `pulse` (with
`pulse_width`, the percent of the cycle that is high; the wave is kept
centered) or `noise`. Each has `level_db`, `pan`, `octave`, `semitones`,
`detune_cents`, `phase` (a percent of the cycle, or left out for a random
place each note, from `seed`) and `filter`, true unless it is to bypass the
filter, as a sub often does. Saw, square and pulse are bandlimited with
PolyBLEP, at a cost that does not depend on pitch; the triangle is plain.

**The filter** is one a voice, a Simper state-variable filter: `mode`, 12 or
24 dB an octave, `cutoff_hz`, `resonance_percent` (0 is a Q of a half, 100
rings just short of self-oscillation), `drive_db` into a soft clip before the
filter, and `keytrack_percent`, 100 moving the cutoff an octave an octave from
middle C. `enabled: false` passes the oscillators through. The default is a
lowpass open at 20 kHz, which changes nothing.

**Envelopes** are a mapping; `amp` is always there and shapes the level, put
in when a patch leaves it out. Others (`env2`, `env3`) do nothing until the
matrix uses them. Attack rises linearly to full over `attack_ms`; decay and
release fall exponentially, reaching the sustain level at `decay_ms` and
silence at `release_ms` after the note-off, 60 dB down and then exactly
nothing, so a voice ends without a step. A voice ends when its amp release
does.

**LFOs** are a mapping of up to four, bipolar. `shape` is `sine`, `triangle`,
`saw` (falling), `square` or `sample_hold`, a new random value a cycle.
`rate_hz` is free; `rate_beats`, when given, replaces it with one cycle in so
many beats at the song's tempo, so an LFO can be an eighth note, `1/2`.
`retrigger` starts the cycle at each note from `phase_percent`; without it the
LFO runs from the start of the song, every voice shares its phase, and a
synced LFO lands on the bar.

**Modulation** is a list of `{source, target, amount}`. Sources: any envelope
or LFO by its ID, `velocity`, `note` (0 at middle C, 1 an octave up), `random`
(one value a note, from the seed) and `macros.NAME`. Envelopes, velocity,
random and macros are unipolar, 0 to 1; `note` and LFOs are bipolar. `amount`
is how far the target moves at full modulation, in the unit `describe` gives
that target: semitones for a pitch, octaves for the cutoff and for an
envelope's times (1 doubles, -1 halves), dB for a level, points for a
percentage, and the pan's own units. Targets are `pitch` for all oscillators
at once; an oscillator's `pitch`, `level_db`, `pan` and `pulse_width`; the
filter's `cutoff_hz`, `resonance_percent` and `drive_db`; and an envelope's
four fields, taken when the note starts and held for the note. An entry whose
source or target the patch lacks is refused, naming it; an envelope or LFO may
not be named `velocity`, `note` or `random`, and an envelope and an LFO may not
share a name, since the matrix names them by ID.

**Macros** are a mapping of up to eight named knobs, 0 to 100. They do nothing
but through the matrix, so a patch decides what Tone or Movement means, and an
automation lane on `instrument.macros.tone` moves everything the patch wired to
it. `daw check` names a macro nothing uses.

**Polyphony and glide.** `voices` is 1 to 16. Beyond it the oldest releasing
voice is stolen, else the oldest sounding, over a 5 ms fade. `glide_ms` slides
each new note's pitch from the pitch of the last note started: with one voice
it is portamento. The first note of a song has nothing to glide from.
`velocity_percent` is how much velocity moves the level: 100 is linear, as a
sampler's, 0 none.

**Determinism.** A voice is a function of the patch, the note and the frames
since its start. Its oscillators and filter run every frame; every control
value (the envelopes, the LFOs, the matrix, the filter's tuning, the pitch
with its glide) is worked out every 16 frames of the voice's own time, so the
output does not depend on how the stream is cut into blocks, a song played
from the start equals its render, and the same song rendered twice is the
same bytes. Random phases and the `random` value come from `seed`, the
track's ID and the note's place among the track's notes. Playback from a
locate chases the notes sounding there: each starts at that frame with its
envelopes and free LFOs where time would have brought them and its filter
empty, as effects start empty at a locate today.

**Live edits** follow D44. A knob the panel or the agent moves glides over
5 ms: the new patch's values blend from the old patch's, frequencies in the
log domain, while every voice carries on. A change of wave, of the filter's
mode, slope or enabling, of an oscillator's filter routing, of the parts a
patch has or of its matrix swaps through the 10 ms dip, since it would jump
the waveform. Describe marks these fields structural, and the panel sends them
when a change ends. The notes stay shared between the programs a patch edit
makes, so the voices are the same objects across the edit.

**Automation** of a MIDI track gains the targets `instrument.FIELD` for every
field describe marks automatable: `instrument.filter.cutoff_hz`,
`instrument.oscillators.a.level_db`, `instrument.macros.tone`,
`instrument.envelopes.amp.release_ms`, `instrument.lfos.lfo1.rate_hz`.
Frequencies and LFO rates interpolate in the log domain as effects' do; a
lane is read at the song's frame at each control tick. The matrix adds to the
automated value; neither replaces the other. A lane on a field the instrument
no longer has goes with the instrument when it is replaced or removed, and
the reply says so; a part removed while a lane names it is refused.

**Cost.** Eight voices of three oscillators with a filter each is a few
hundred oscillator steps a frame, a small fraction of a core. The state of
every voice, 16 of them and 16 more ringing out, is allocated when a renderer
is built, never in the callback; the renderer's take-over carries it between
programs.

## What the agent does

Everything goes through the host as a command with an origin, so it is heard,
undoable, in `daw changes`, and lit in the window. `daw set` reaches any field
by its path, and a labeled `daw batch` makes a designed sound one undo step;
these verbs are the shorter way and the one `daw describe synth` teaches.

```
daw synth add SONG lead                                # on a MIDI track, or a new one
daw synth show SONG lead                               # the patch as the song holds it
daw synth set SONG lead filter.cutoff_hz 900 envelopes.amp.release_ms 600
daw synth set SONG lead oscillators.sub '{"wave": "sine", "octave": -1, "filter": false}'
daw synth set SONG lead oscillators.sub null           # a part removed
daw synth mod SONG lead env2 filter.cutoff_hz 2        # add, or change the amount
daw synth mod SONG lead env2 filter.cutoff_hz --remove
daw synth audition SONG lead --notes C2,C3 --velocity 100 --length-beats 2 [--track-chain]
```

`synth set` takes pairs, each a field path from the patch and a value or a
JSON object, and makes one undo step; `synth add` attaches the plain saw, or
makes a new MIDI track with it when no track has the name. `synth audition`
renders the given notes one after another, or middle C, through the patch,
and with `--track-chain` through the track's inserts too, to
`renders/auditions/TRACK-HASH.wav`, named for what was heard so the same
audition writes the same file, and replies with its path, peak, loudness in
LUFS and spectral centroid, so `daw listen` can work on it; every step leaves
something to hear (principle 7). A patch louder than full scale is written
scaled to fit and the reply says by how much. `synth show` is `daw get` of
the patch, with fields at their defaults left out; `daw get` of a field, and
the panel, read the defaults.

`daw describe synth` prints the schema; every part's fields with label, unit,
range, default, choices and whether a lane can move the field or a change
swaps through a dip; the modulation sources and every target with its unit;
the semantics above; and recipes for a sub, a pluck, a pad, a lead, a kick, a
hat and a riser, the way `describe edit` teaches an edit.

`daw check` warns of a macro no entry uses, of more notes stacked than the
synth has voices, and of a last note whose amp release ends past the song's
end, which the end fade cuts.

## The panel

The Synth takes the instrument's place at the head of a MIDI track's chain, as
the Sampler does, drawn from describe as an effect's panel is: a column for the
Synth's own fields, one for each oscillator, the filter, each envelope, each
LFO, the macros, and the matrix as rows of source, target and amount with ×
to remove one. Each control is the one effects' panels have, with the lane
mark that adds or removes a lane on the field; a wave or a mode is sent when
chosen and a knob is heard as it is dragged. The agent adds parts and matrix
entries with `daw synth set` and `daw synth mod`; the panel does not yet, and
draws no wave, curve or envelope. That is the panel item.

## What remains

2. **Patches.** A patch as a YAML file, the `synth` mapping with a name,
   description, tags, who saved it and when around it, in
   `~/Music/AAW/library/patches/`, the first folder of the workspace, made when
   the first is saved; `AAW_WORKSPACE` points elsewhere. A patch is named by
   its file, `round-sub.yaml`; `patch save` over a name that exists asks, or
   `--replace`. Factory patches in `daw` itself under `engine/patches/`, which
   `patch list` shows with `factory: true` and a saved copy shadows: a dozen
   generic sounds, Init, Sub Bass, Reese, Supersaw, Pluck, Soft Pad, Bright
   Lead, Organ, Bell, Kick, Hat, Riser. `daw patch save|list|show|load` and
   `daw synth add --patch`; `patch load` replaces the whole `synth` mapping,
   the notes staying as they are, in one undo step named for the patch. The
   browser's Instruments lists Synth beside Sampler, opening to its patches,
   Factory and Mine, searched by name and tag; dragging Synth onto a MIDI track
   attaches the plain saw, dragging a patch attaches a synth with it or loads
   it into the synth the track has, and a drop under the tracks makes a new
   MIDI track named after the patch. A patch saved from one song and loaded
   into another is the same audio at the same tempo, since nothing outside the
   mapping shapes it.
3. **The panel.** A header with the patch's name, ◂ ▸ through the patches of
   the list it came from, a menu with Save…, Save As… and the factory patches;
   the wave drawn one cycle; the filter's response curve dragged by its corner;
   the ADSR drawn with its four handles; the LFO's shape over one cycle; a ring
   on a control for the depth of a matrix entry on it, and an envelope's or
   LFO's tab dragged onto a control to add one; + and × for the parts; the
   detail panel's height draggable, 214 points the least; `note.preview` in the
   host, a note of a pitch, velocity and length played now through a track's
   instrument and chain, outside the timeline and the undo history, and one
   octave of keys in the panel that sends it. `daw synth audition --play`
   sends one too.
4. **Unison, wavetables and the patch's effects.** `unison` (1 to 16 voices),
   `unison_detune_cents` and `unison_width_percent` on an oscillator, with
   `unison_detune_cents` a matrix target; `wave: wavetable` with built-in
   tables and a single cycle read from a WAV in the project; `chorus` and
   `saturation` as effect kinds anywhere; and `effects` inside the patch, the
   song's effect kinds in a chain of their own run on the sum of the voices
   before the track's inserts, automated as `instrument.effects.REF.FIELD`.

Items 2 and 3 can swap; 4 can follow either. Each item's pull request moves its
line and rewrites this file toward the present tense.

## Done when

Of the first item:

- A MIDI track with a synth plays its note clips; a render equals playback from
  the start to the engine's tolerance, in every block size, and the same song
  rendered twice is the same bytes. Done: `crates/aaw-engine/tests/synth.rs`.
- Every field in the panel is one the agent reads and sets, and every value
  the agent sets shows in the panel. Done: `crates/aaw-ffi/tests/song.rs`.
- A modulation entry, a macro and an automation lane on the filter are heard
  and verified against a rendered sweep. Done in the engine's and the DSP's
  tests; not heard by a person.
- `synth audition` leaves a WAV `daw listen` measures. Done: `tests/test_midi.py`.
- Engine tests cover each oscillator's spectrum, the filter against the effect
  filter at its settings, envelope timings, LFO rates in Hz and beats, the
  matrix's units, voice stealing, glide, chased notes at a locate,
  block-partition invariance and allocation checking. Done in part: a sine's
  pitch and level, the envelope's attack and release, a lane's sweep, glide,
  stealing, chasing, invariance and allocation in `crates/aaw-dsp/src/synth.rs`
  and the engine's tests; each oscillator's spectrum, the filter against the
  effect filter and the LFO's rate in beats are not measured by a test.
- Host tests the commands and the undo of a set: done, `crates/aaw-host/tests/synth.rs`.
- `./build.sh test` the panel's edits: the Swift package builds and its tests
  pass with the panel; the panel was not seen in a picture.
- `architecture.md`, the engine and Mac READMEs, `concept.md`'s Sound section
  and `decisions.md` are updated, and this file is rewritten as the reference.

Of the whole: a patch saved from one project loads into another and renders
the same audio in both at the same tempo; the factory patches are heard and
one of each kind sounds like its name; the panel's drawings, drops and tall
panel are seen in pictures. Listening by hand is a line under Verify.

## Settled

Chosen by the person on October 3, 2026 (D69), from the alternatives the
proposal offered:

- **After the Sampler device,** as backlog item 2, since the two share the wide
  instrument panel.
- **Oscillators are a mapping by ID,** up to four, as pads are, rather than
  Serum's fixed A, B, Sub and Noise slots: a smaller schema and stable command
  paths.
- **Patches live in `~/Music/AAW/library/patches/`,** the workspace folder the
  concept names, made when the first patch is saved. The workspace item
  inherits one folder that is already there. Not the hidden app data folder,
  not a registered folder as sample Folders are.
- **Effects are in the patch,** so a patch carries its chorus and reverb and
  sounds finished on any track.
- **Wavetables as staged:** built-in tables and a single cycle read from a WAV,
  in item 4. Serum-style multi-frame tables with a sweepable position are a
  Later line if wanted.
- **The detail panel's height is draggable,** 214 points the least, rather than
  a window of the synth's own.
- **No MIDI keyboard yet.** `note.preview` and the panel's keys are the hook; a
  keyboard stays its own Later line.

Decided in building the first item, October 3, 2026 (D71):

- **One filter a voice, with a per-oscillator bypass,** rather than two with
  routing; a second filter is a Later line.
- **Control values every 16 frames of a voice's time,** oscillators and the
  filter every frame.
- **An entry on an envelope's field is taken when the note starts;** an LFO's
  fields are not targets.
- **The amp release reaches exactly nothing** at `release_ms`, 60 dB down and
  then zero, so a voice ends without a step and the song is quiet after it.

## Limits

- No unison, wavetables or effects inside the patch until item 4, and no
  patches as files until item 2; `patch` in the song is a name nothing reads.
- The triangle is not bandlimited; it aliases a little at the top of the keys.
- An LFO's fields are not matrix targets. A free LFO's phase at a locate is
  worked out from its static rate, so under a lane on its rate the phase after
  a locate differs from the render's; from the start of the song they agree.
- Playback from a locate starts each chased voice's oscillators at their first
  phase and its filter empty, so from a locate the waveform differs from the
  render's where a note was already sounding; the envelopes and free LFOs do
  not.
- `synth show` leaves out fields at their defaults, as `daw get` does;
  `describe synth` lists the defaults.
- The panel draws plain controls: no wave, curve, envelope or keys, and no +
  for a part or an entry, until item 3.
