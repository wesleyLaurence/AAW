# The Synth — engine and commands implemented October 3, 2026; patches and the panel October 4, 2026

Status: three of four items are built. The engine, the patch in the song,
`daw synth` and `daw describe synth`, automation of the patch's fields,
`daw check` warnings, patches as files with the factory patches, `daw patch`
and the browser's Synth, and the panel with its drawings, its Save and Load,
its keys and `note.preview` are built and described here in the present
tense. Unison, wavetables and the patch's effects are the item that remains,
described under [What remains](#what-remains). D69 records what the person
settled on October 3, 2026, D71 what was decided in building the first item,
D72 what was decided in building patches and D73 what was decided in
building the panel.

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

## Patches

A patch is the `synth` mapping as a YAML file outside any song, so that a
sound designed in one project is loaded into another and is the same sound at
the same tempo, since nothing outside the mapping shapes it. Around the
mapping are a name, a description, tags, who saved it and when:

```yaml
name: Round Sub
description: A sine with a saw an octave up under a low filter.
tags: [bass, sub]
saved_by: agent
saved_at: '2026-10-04T03:16:50Z'
synth:
  voices: 1
  oscillators:
    a: {wave: sine, level_db: -6, filter: false}
    b: {level_db: -18, octave: 1}
  filter: {cutoff_hz: 200, slope_db_per_octave: 24}
```

The mapping is written as the song saves it, fields at their defaults left
out, and validated as the song validates a synth, so a file with a wrong wave
or an entry naming a part the patch lacks is refused with the field named.

**Where they live.** Saved patches are files in the workspace's library,
`~/Music/AAW/library/patches/`, or `library/patches/` under `AAW_WORKSPACE`:
the first folder of the workspace the concept names, made when the first
patch is saved. A patch is named by its file: the file's stem is the slug of
the name, `round-sub.yaml` for Round Sub, so a name may have spaces and
capitals and the file has neither (D72). A `.yaml` file anywhere loads by its
path too. Twelve factory patches are built into `daw` from
`engine/patches/`: Init, Sub Bass, Reese, Supersaw, Pluck, Soft Pad, Bright
Lead, Organ, Bell, Kick, Hat and Riser, generic sounds that start from the
recipes `daw describe synth` teaches. A saved patch whose slug is a factory
patch's shadows it.

**Commands.**

```
daw patch list [WORDS]                                  # factory first, then mine by name
daw patch show NAME                                     # the file: name, tags, who, when, synth
daw patch save SONG lead "Round Sub" --description "…" --tags bass,sub [--replace]
daw patch load SONG lead round-sub                      # by name, by slug, or a .yaml path
daw synth add SONG lead --patch "Soft Pad"              # attach, or a new track with it
```

`patch list` lists every patch with its name, slug, description, tags,
whether it is factory and its file; `WORDS` keep those with every word in the
name or a tag. A file in the folder that is not a patch is named under
`problems`, not fatal. `patch save` writes a track's synth under the name, as
the file above, with `saved_by` the command's origin, user or agent; a name
already saved is written over only with `--replace`, which keeps the file's
description and tags unless new ones are given, and a factory name is
shadowed without asking. It also sets the song's `patch` field to the name,
so the song says where its sound came from; that is the one undo step it
makes, and when the song already says so nothing changes. `patch load` puts
the patch's mapping, led by `patch: NAME`, in place of the track's whole
instrument, in one undo step named for the patch; the notes stay, and a lane
on a field the new patch lacks goes, as it does when an instrument is
replaced. A MIDI track with no instrument takes a patch too; a track of
patterns is refused. `synth add --patch` attaches a patch instead of the
plain saw, or makes a new MIDI track with it. The replies of `load` and
`synth add` say `patch` and `factory`; `save` says the file.

**The browser.** Instruments lists Sampler and Synth, and under Synth its
patches, Factory and Mine, searched by name and tag with the field at the
top. Dragging Synth onto a MIDI track's header or device panel attaches the
plain saw in place of the instrument it had; dragging a patch loads it into
the track's Synth, or attaches a Synth with it; a drop under the tracks makes
a new MIDI track, `synth-1` for the plain saw and named after the patch
otherwise, `soft-pad`, then `soft-pad-2`. Double-click and + do the same to
the selected row. Each is one undo step, labeled as the commands label
theirs. The list is read when the browser opens on Instruments and when the
window comes back to the front, so a patch saved from the terminal appears.

## The panel

The Synth takes the instrument's place at the head of a MIDI track's chain, as
the Sampler does, drawn from describe as an effect's panel is: a column for the
Synth's own fields, one for each oscillator, the filter, each envelope, each
LFO, the macros, and the matrix. Each control is the one effects' panels have,
with the lane mark that adds or removes a lane on the field; a wave or a mode
is sent when chosen and a knob is heard as it is dragged.

**The header** shows the patch's name, or Synth when the sound came from
none, with ◂ ▸ through the patches the browser lists, Factory then Mine, and
a menu: Save…, Save As…, and the patches to load. Save… writes over the patch
the sound came from when that is one of the person's own, keeping its
description and tags; otherwise, as Save As… does, it asks for a name, a
description and tags in a sheet, and refuses a name of the person's already
saved unless asked to write over it. Both send `patch.save`; loading sends
`patch.load`, and × takes the Synth off.

**The drawings**, at the head of each column: an oscillator's wave over one
cycle, with its pulse width; the filter's response from 10 Hz to 20 kHz,
-36 to +36 dB, with a corner at the cutoff that a drag moves across for the
cutoff and up and down for the resonance, both in one undo step and heard as
they move, a press elsewhere on the curve taking the cutoff alone; an
envelope with a handle at the end of its attack, of its decay, which also
sets the sustain, and of its release, each time taking up to a quarter of the
width in equal ratios of (1 + ms); an LFO's shape over one cycle from its
phase. Under a control a matrix entry moves, a line reaches from its value to
where full modulation takes it, one line an entry. Each envelope's and LFO's
title, each macro's name, and velocity, note and random in the Matrix column
are tabs: dropped on a control, they add an entry from that source to the
control's target, an oscillator's semitones standing for its pitch, with an
amount enough to hear (an octave, a semitone, 6 dB, 25 points or half the
pan). An entry's amount is a bar in the matrix, and × takes it out. + in the
Synth column adds an oscillator (`a` to `d`, the plain saw), an envelope
(`env2` on, a decay to nothing), an LFO (`lfo1` on) or a macro (`macro1` on,
at 0), and × on a column or a macro takes it off; the last oscillator, `amp`
and a part the matrix names are refused with the reason.

**The keys.** The Synth column ends in an octave of keys, C3 to C4 until the
octave is stepped, that play a note now through the track: a beat long,
softly at the top of a key and hard at the bottom, and a drag onto the next
key plays that one. They send `note.preview`:

```
{"op": "note.preview", "track": "lead", "pitch": "C4", "velocity": 100, "length_beats": 1}
daw synth audition SONG lead --notes C2,G2 --play     # the same notes, through the running host
```

`note.preview` is a transport command, as `metronome` is: it plays a note of
a pitch, velocity and length through a MIDI track's Synth and the track's
chain, from where the stream stands, whether or not the song plays, outside
the timeline and the undo history, and nothing of it is rendered. A note
held while a knob is dragged carries on under the new patch, and through a
change of wave. It needs a host, which opens the output if nothing has played
yet; it refuses a track with no synth, and plays nothing through a Sampler.
While the transport stands still the stream is no longer cut at the song's
end: a tail or a previewed note is heard whole wherever the transport is.
`--play` sends the audition's notes one after another and needs a running
host; the file is written as well.

**The detail panel's height** is dragged at its top edge, 214 points the
least and kept between projects, so the Synth has room; at the least height
the panel's columns scroll.

## What remains

4. **Unison, wavetables and the patch's effects.** `unison` (1 to 16 voices),
   `unison_detune_cents` and `unison_width_percent` on an oscillator, with
   `unison_detune_cents` a matrix target; `wave: wavetable` with built-in
   tables and a single cycle read from a WAV in the project; `chorus` and
   `saturation` as effect kinds anywhere; and `effects` inside the patch, the
   song's effect kinds in a chain of their own run on the sum of the voices
   before the track's inserts, automated as `instrument.effects.REF.FIELD`.
   The factory patches gain unison and effects where they want them.

Its pull request moves its line and rewrites this file toward the present
tense.

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

Of the patches item:

- A patch saved from one project loads into another and renders the same
  audio in both at the same tempo. Done: `crates/aaw-host/tests/patches.rs`
  holds the two auditions byte for byte, and `tests/test_midi.py` does the
  same through `daw`.
- Every factory patch reads, validates, renders on a few notes and sits
  under full scale. Done in the same test; whether each sounds like its name
  is not, and is a line under Verify.
- The browser's Synth and its patches make tracks and load into them: done
  in `crates/aaw-ffi/tests/song.rs` and the Mac tests, through the model,
  and the list was seen in a scripted picture; nothing was dragged by hand.

Of the panel item:

- A previewed note sounds while the transport stands, through the track's
  chain, carries across a knob's take-over and a wave's swap, is heard past
  the song's end, and is in no render. Done: `crates/aaw-dsp/src/synth.rs`
  and `crates/aaw-engine/tests/synth.rs`; the host refuses a bad note before
  the output opens, `crates/aaw-host/tests/host.rs`; `--play` without a host
  says so, `tests/test_midi.py`.
- The panel's edits reach the song as the commands do: several fields in one
  step, parts added under the next free name and removed, entries added by
  a drop and their amounts set, a patch saved and listed under Mine. Done:
  `crates/aaw-ffi/tests/song.rs`.
- The filter's corner, the envelope's handles, the keys and an entry's
  reach are where they are drawn and read back what they were dragged to.
  Done: `apps/mac/Tests/AAWAppTests/SynthLayoutTests.swift`.
- The header, the drawings, the keys, the reach lines and the panel dragged
  taller were seen in scripted pictures; nothing was dragged by hand, no key
  was pressed and nothing was heard.

Of the whole: the factory patches are heard and one of each kind sounds like
its name. Listening by hand is a line under Verify.

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

Decided in building patches, October 4, 2026 (D72):

- **A patch's file is the slug of its name,** so Soft Pad is `soft-pad.yaml`
  and a name may have spaces; the name in the file is the patch's.
- **`patch save` names the song's patch** after the patch it wrote, as one
  undo step, so the song says where its sound came from and the panel's
  header shows it; `patch load` sets it too. Nothing reads the field.
- **A patch file is validated as the song validates a synth,** so one
  refusal names a bad field whether it is in a song or a file.
- **The factory patches are written for the Synth as it is:** four detuned
  saws stand in for unison, and no effects are in them, until item 4.

Decided in building the panel, October 4, 2026 (D73):

- **`note.preview` plays a Synth only,** from where the stream stands, and
  a standing stream is cut by nothing: the end fade and the gate at the
  song's end apply only while the transport rolls.
- **A key's note is a beat long** and its velocity is how far down the key
  it is pressed; there is no note-off.
- **◂ ▸ step through the browser's list,** Factory then Mine, and Save…
  writes over a patch of the person's own only.

## Limits

- No unison, wavetables or effects inside the patch until item 4.
- `note.preview` plays nothing through a Sampler: a Sampler's note needs
  audio prepared for its pitch, which the compiled song has only for the
  notes it plays. A Later line.
- A key plays a note of one beat; it cannot be held or let go early, and
  the panel's keys are the only keyboard.
- A previewed note is kept across a knob's glide and a wave's swap, but
  not across a change of the song's tempo or sample rate, which opens
  another stream.
- `patch` in the song is where the sound came from, set by `patch load`,
  `patch save` and `synth add --patch`; nothing reads it, and a changed
  sound keeps the name until it is saved under another.
- The browser reads the patches when it opens on Instruments and when the
  window comes to the front, not while a terminal saves one in front of it.
- A patch names no tempo, so a synced LFO or a riser's envelope is in beats
  or milliseconds as the patch wrote it, whatever the song's tempo.
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
- The panel does not rename a part or a macro, and a matrix entry's source
  and target are set when it is made: `daw synth set` and `daw synth mod`
  do the rest.
