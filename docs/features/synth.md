# The Synth — proposed October 3, 2026

Status: proposed, not built. Backlog item 2, after the
[Sampler device](sampler-device.md), whose wide instrument panel it shares.
Four items, each a pull request: the engine and the commands, the patches, the
panel, and unison, wavetables and the patch's effects. D69 records what the
person settled on October 3, 2026.

## What

A polyphonic synthesizer the agent operates from the terminal and the person
from the device panel, with the same controls, on a MIDI track: oscillators, a
filter, envelopes, LFOs, a modulation matrix, macros and an effect chain of its
own, in the shape of a modern software synth. A sound is a text patch inside the
song, saved under a name to the person's library and loaded into any song, in
this session or another. Factory patches ship with it, so a track can start from
a sub bass, a pluck or a pad rather than from an unshaped saw.

It plays what the sampler plays: the notes of the track's note clips. It is for
basses, leads, chords, pads, plucks, drums designed from nothing and sound
effects. It is not a plugin host and not a generative model.

## Why

Agents asked for a sound the library lacked wrote scripts that synthesized a
sub or a glide into a WAV the DAW then could not see or change. The sampler
plays what exists; a synth makes what does not, in the song, where every value
is readable and every change is a command. The concept lists it under Sound.

The person's expectation is a Serum-class instrument: oscillators with unison,
a filter with drive, envelopes and LFOs assigned by a matrix, macros, and the
device's own effects, so a patch sounds finished by itself. The concept's
"subtractive synth" grows to that (D69).

## Design

### The sound is in the song

```yaml
tracks:
  - id: lead
    type: midi
    instrument:
      synth:
        patch: Soft Pad              # where it came from; nothing reads it
        voices: 8                    # 1 is monophonic
        glide_ms: 0
        velocity_percent: 60         # how much velocity moves the level
        seed: 1                      # random phases and the random source
        oscillators:
          a: {wave: saw, level_db: -6, unison: 5, unison_detune_cents: 14, unison_width_percent: 70}
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
        effects:
          - {type: chorus, rate_hz: 0.6, depth_ms: 4, mix_percent: 35}
          - {type: reverb, decay_seconds: 2.5, mix_percent: 20}
    clips: [...]
```

`instrument: {synth: ...}` is a second kind beside `sampler`. The mapping is the
whole sound; a project stays self-contained, as it does with samples copied in.
Every value is in real units and every field has a label, unit, range, default
and whether it is automatable or structural, printed by `daw describe synth` and
drawn by the panel, as effects are (D44). Unknown fields are rejected. A
`synth` with nothing but `oscillators: {a: {}}` is a valid patch: one saw with a
plain envelope, which is what an empty Synth dragged from the browser is.

**Oscillators** are a mapping of one to four, keyed by an ID the patch chooses
(`a`, `b`, `sub`, `noise` are conventions, not slots), drawn in the order
written. `wave` is `sine`, `triangle`, `saw`, `square`, `pulse` (with
`pulse_width`), `noise`, or `wavetable` from item 4. Each has `level_db`, `pan`,
`octave`, `semitones`, `detune_cents`, `phase` (`random` or a percentage),
`unison` (1 to 16 voices), `unison_detune_cents`, `unison_width_percent`, and
`filter`, true unless it is to bypass the filter, as a sub often does.

**The filter** is one per voice, a Simper state-variable filter, which
`aaw-dsp` has: `mode`, 12 or 24 dB an octave, `cutoff_hz`, `resonance_percent`
(0 is 0.5 Q and 100 self-oscillates short of instability), `drive_db` into a
soft clip before the filter, and `keytrack_percent`, 100 moving the cutoff an
octave an octave. `enabled: false` passes the oscillators through.

**Envelopes** are a mapping; `amp` is always there and shapes the level. Others
(`env2`, `env3`) do nothing until the matrix uses them. Attack rises in level
linearly over `attack_ms`; decay and release fall exponentially, `decay_ms`
being the time to reach the sustain level and `release_ms` the time to fall
60 dB after the note-off. `sustain_percent` is the held level.

**LFOs** are a mapping of up to four. `shape` is `sine`, `triangle`, `saw`,
`square` or `sample_hold`. `rate_hz` is free; `rate_beats` instead is one cycle
in so many beats at the song's tempo, so an LFO can be an eighth note, `1/2`.
`retrigger` starts the cycle at each note from `phase_percent`; without it the
LFO runs from the start of the song, so every voice shares its phase and a
synced LFO lands on the bar. An LFO is bipolar.

**Modulation** is a list of `{source, target, amount}`. Sources: any LFO, any
envelope, `velocity`, `note` (0 at middle C, 1 an octave up), `random` (one
value a note, from the seed) and any macro. Envelopes, velocity and macros are
unipolar, 0 to 1; `note` and LFOs are bipolar. `amount` is how far the target
moves at full modulation, in the unit describe gives that target: octaves for a
frequency, semitones for a pitch, dB for a level, points for a percentage, and
the pan's own units. Targets are the oscillators' `pitch` (a pseudo-field over
octave, semitones and detune), `level_db`, `pan`, `pulse_width` and
`unison_detune_cents`; the filter's `cutoff_hz`, `resonance_percent` and
`drive_db`; every envelope's and LFO's fields; `pitch` for all oscillators at
once; and the patch's effects' automatable fields. An entry whose source or
target is missing is an error that names it, as a lane's is.

**Macros** are a mapping of up to eight named knobs, 0 to 100. They do nothing
but through the matrix, so a patch decides what Tone or Movement means, and an
automation lane on `instrument.macros.tone` moves everything the patch wired to
it. They are the knobs the person reaches for first and the agent automates.

**Effects** inside the patch are the song's effect kinds in a chain of their
own, run on the sum of the voices before the track's inserts, so a patch
carries its chorus and its reverb and sounds the same on any track. They are
the `aaw-dsp` devices as they are, with `chorus` and `saturation` added in
item 4, both long wanted on tracks as well. Automation reaches them as
`instrument.effects.REF.FIELD`.

**Polyphony and glide.** `voices` is 1 to 16. Beyond it the oldest releasing
voice is stolen, else the oldest sounding, over a 5 ms fade. `glide_ms` slides
the pitch from the last note played; with one voice it is portamento, with more
it slides each new voice from the last note started.

**Determinism.** A voice is a function of the patch, the note and the frames
since its start; its random phases and its `random` value come from `seed`, the
track and the note's index. So a song played from the start equals its render,
as it does now, and the same patch played twice is the same audio. A render
runs the voices from each note's start. Playback from a locate chases the notes
sounding there: each starts at that frame with its envelopes and free LFOs at
the state time would have brought them to, and its filter empty, as effects
start empty at a locate today.

**Live edits** follow D44: a knob the panel or the agent moves glides over 5
ms, and voices sounding carry on; a structural change (an oscillator added or
removed, a wave, the voice count, a filter's slope, an effect) swaps through the
10 ms dip. Describe marks which fields are structural, and the panel sends those
when the drag ends.

**Automation** of a MIDI track gains the targets `instrument.FIELD` for every
field describe marks automatable: `instrument.filter.cutoff_hz`,
`instrument.oscillators.a.level_db`, `instrument.macros.tone`,
`instrument.envelopes.amp.release_ms`, `instrument.effects.verb.mix_percent`.
Frequencies interpolate in the log domain as effects' do. The matrix adds to the
automated value; neither replaces the other.

**Cost.** Eight voices of two oscillators in seven-voice unison with a sub, a
filter each and a chorus is a few hundred oscillator steps a frame, a fraction
of a core in Rust. Saw, square and pulse use PolyBLEP, so they are bandlimited
at a cost that does not depend on pitch; the state of every voice is allocated
at compile, never in the callback, and the renderer's `take_over` carries it
between programs.

### What the agent does

Everything goes through the host as a command with an origin, so it is heard,
undoable, in `daw changes`, and lit in the window. `daw set` reaches any field
by its path, and a labeled `daw batch` makes a designed sound one undo step;
these verbs are the shorter way and the one `daw describe synth` teaches.

```
daw synth add SONG lead [--patch "Soft Pad"]           # on a MIDI track, or a new one
daw synth show SONG lead                               # the patch as it is, modulation listed
daw synth set SONG lead filter.cutoff_hz 900 envelopes.amp.release_ms 600
daw synth set SONG lead oscillators.sub '{"wave": "sine", "octave": -1, "filter": false}'
daw synth mod SONG lead env2 filter.cutoff_hz 2        # add, or change the amount
daw synth mod SONG lead env2 filter.cutoff_hz --remove
daw synth audition SONG lead --notes C2,C3 --velocity 100 --length-beats 2
daw patch save SONG lead "Round Sub" --tags bass,sub --description "..."
daw patch list [QUERY] [--tag bass]
daw patch show "Round Sub"
daw patch load SONG lead "Round Sub"
```

`synth set` takes pairs, each a field path from the patch and a value or a JSON
object, and makes one undo step. `synth add` with no patch attaches the plain
saw. `synth audition` renders the given notes, or middle C, through the patch
and its effects, and with `--track-chain` through the track's inserts too, to a
WAV in `renders/` and replies with its path, peak, loudness and spectral
centroid, so `daw listen` and `daw compare` can work on it; every step leaves
something to hear (principle 7). `patch save` writes the file and sets `patch`
in the song; `patch load` replaces the whole `synth` mapping, the notes staying
as they are, in one undo step named for the patch.

`daw describe synth` prints the schema with each field's label, unit, range,
default and whether it is automatable or structural; the modulation sources and
every target with its unit; the semantics above; and short recipes, the way
`describe edit` teaches an edit: a sub is a sine with a saw an octave up under a
low filter; a pluck is a short filter envelope on a saw; a pad is slow attack,
detuned unison and chorus; a kick is a sine whose pitch an envelope drops two
octaves in 40 ms. The recipes are what makes the first session productive.

`daw check` warns of a matrix entry on a missing target, a macro nothing uses, a
synth whose amp envelope cannot reach silence before the song's end fade, and a
patch whose `voices` is below the notes its clips stack.

### Patches on disk

A patch is a YAML file, the `synth` mapping with a name around it:

```yaml
name: Round Sub
description: Sine and a quiet saw an octave up, under a filter velocity opens
tags: [bass, sub]
made_by: agent          # the origin of the save
saved: 2026-10-03
format: 1
synth: {...}            # exactly as the song holds it, without `patch`
```

They live in `~/Music/AAW/library/patches/`, the first folder of the workspace
the concept describes, made when the first patch is saved; `AAW_WORKSPACE`
points elsewhere, and tests use a scratch folder. A patch is named by its file,
`round-sub.yaml`; `patch save` over a name that exists asks, or `--replace`.
Factory patches are in `daw` itself, under `engine/patches/`, and `patch list`
shows them with `factory: true`; saving over one makes the person's copy, which
then shadows it. They are generic sounds, a dozen or so: Init, Sub Bass, Reese,
Supersaw, Pluck, Soft Pad, Bright Lead, Organ, Bell, Kick, Hat, Riser.

A patch saved from a song and loaded into another is the same audio, since
nothing outside the mapping shapes it; `rate_beats` follows the new song's tempo
as a delay's `time_beats` does.

### The browser and the panel

**Browser.** Instruments lists Synth beside Sampler. Synth opens to its patches,
Factory and Mine, searched by name and tag. Dragging Synth onto a MIDI track
attaches the plain saw; dragging a patch attaches a synth with it, or loads it
into the synth the track has, and dropped under the tracks either makes a new
MIDI track named after the patch. Double-click and + do what a drop on the
selected row would. These are `instrument.set`, `patch.load` and `track.add`
batches, as the browser's other drops are.

**The panel** takes the instrument's place at the head of a MIDI track's chain,
and is wider than an effect's. Its parts, left to right:

| Part | Shows | Fields |
|---|---|---|
| Header | The patch's name, ◂ ▸ through the patches of the list it came from, a menu with Save…, Save As… and the factory patches, and × | `patch` |
| Oscillators | One column each, a + for another, × on each; the wave drawn one cycle | every oscillator field |
| Filter | The response curve drawn from mode, cutoff, resonance and slope, dragged by its corner | every filter field |
| Envelopes | Tabs Amp, Env2…; the ADSR drawn, with its four handles dragged | attack, decay, sustain, release |
| LFOs | Tabs; the shape drawn over one cycle; Hz or beats | every LFO field |
| Matrix | A row a modulation entry: source, target, amount, ×; + adds a row | `modulation` |
| Macros | A knob each, named; + names another | `macros` |
| Effects | The patch's chain as the track's device panels draw a chain | `effects` |
| Keys | One octave of keys; a click plays the note through the synth | — |

The controls are the ones effects' panels have, drawn from describe, with the
wave, filter curve, envelope and LFO drawings the Synth alone needs, as the
Sampler's waveform is. A value the agent sets lights for a moment, as every
change does. A control with a matrix entry on it shows a ring for the depth, as
Serum shows its modulation; dragging an LFO's or an envelope's tab onto a
control adds a matrix row, and is the gesture the person knows.

The detail panel is 214 points high. A panel of this shape wants about 300, so
the detail panel's height becomes draggable, remembered per window, with 214
still the least. Where the window is narrow the panel scrolls sideways by part.

**Keys** need a note the schedule does not have. The host gains `note.preview`,
a note of a given pitch, velocity and length played now through a track's
instrument and chain, outside the timeline, the song's undo history and a
render; it is what the piano roll's "a note heard as it is drawn" (Later) and a
MIDI keyboard (Later) will also use. `daw synth audition --play` sends one.

### Items

1. **The engine and the commands.** The `synth` instrument with sine, triangle,
   saw, square, pulse and noise, the filter, envelopes, LFOs, the matrix, macros,
   polyphony and glide; `describe synth`; `daw synth add|show|set|mod|audition`;
   automation of instrument fields; `daw check` warnings. The panel shows the
   patch as a list of fields drawn from describe, as a new effect would be
   shown, so the person can turn every knob before item 3 draws it well.
2. **Patches.** The file, the workspace folder, the factory patches,
   `daw patch`, and the browser's Synth and its patches with their drops.
3. **The panel,** the drawings, the modulation gesture, the draggable panel
   height, `note.preview` and the keys.
4. **Unison, wavetables and the patch's effects.** Unison with its detune and
   width; `wavetable` with built-in tables and a single cycle read from a WAV in
   the project; `chorus` and `saturation` as effects anywhere; the patch's
   `effects` chain.

Items 2 and 3 can swap; 4 can follow either. Each item's pull request moves its
line and rewrites this file toward the present tense.

## Done when

- A MIDI track with a synth plays its note clips; a render equals playback from
  the start to the engine's tolerance, in every block size, and the same song
  rendered twice is the same bytes.
- Every field in the panel is one the agent reads and sets, and every value the
  agent sets shows in the panel while playing.
- A modulation entry, a macro and an automation lane on the filter are heard
  and verified against a rendered sweep.
- A patch saved from one project loads into another and renders the same audio
  in both at the same tempo.
- `synth audition` leaves a WAV `daw listen` measures.
- The factory patches are heard and one of each kind sounds like its name.
- Engine tests cover each oscillator's spectrum, the filter against the effect
  filter at its settings, envelope timings, LFO rates in Hz and beats, the
  matrix's units, voice stealing, glide, chased notes at a locate,
  block-partition invariance and allocation checking; host tests the commands,
  the patch files and the undo of a load; `./build.sh test` the panel's edits
  and drops, with the window seen in a picture plain, with a factory patch and
  with the panel drawn tall. Listening by hand is a line under Verify.
- `architecture.md`, the engine and Mac READMEs, `concept.md`'s Sound section
  and `decisions.md` are updated, and this file is rewritten as the reference.

## Settled

Chosen by the person on October 3, 2026 (D69), from the alternatives the
proposal offered:

- **After the Sampler device,** as backlog item 2, since the two share the wide
  instrument panel.
- **Oscillators are a mapping by ID,** up to four, as pads are, rather than
  Serum's fixed A, B, Sub and Noise slots: a smaller schema and stable command
  paths.
- **Patches live in `~/Music/AAW/library/patches/`,** the workspace folder the
  concept names, made when the first patch is saved. The workspace item (Next 5)
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

## Open questions

1. **A filter per oscillator route, two filters, or one.** One filter with a
   per-oscillator bypass is proposed; Serum has two with routing. To be decided
   when item 1 is picked up.
2. **Patch names with spaces.** Files are slugged, `round-sub.yaml`, and the
   name kept inside; whether `patch load` matches names case- and
   space-insensitively.
