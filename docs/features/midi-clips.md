# MIDI tracks and note clips — proposed October 3, 2026

Status: proposed, not built. Backlog item 1. Product requirements are in
[concept.md](../concept.md#sound); decisions D60 and D61 record the reasons.
This is the handoff for the next build session. Field names below illustrate
the intended model; they are not accepted by the current schema.

## What

A MIDI track that can hold editable note clips with no instrument attached.
The person draws melodies, chords and rhythms, edits pitch, start, duration and
velocity, and copies a clip elsewhere to make an independent variation. The
agent reads and edits those same objects. Attaching, removing or replacing a
sampler changes the sound without changing the notes. The instrument boundary
also accommodates software instruments built later.

The first build includes the Rust model, host commands, playback through the
existing sampler, the Mac editor, and basic standard MIDI file import/export.
It does not build a synthesizer, plugin hosting, MIDI keyboard recording, MIDI
effects, controller/expression editing, MPE, a chord language or a theory engine.
Those are separate backlog items. Explicit enharmonic spelling is deferred.

## Why

The person's base Ableton workflow depends on musical material being separate
from the instrument, freely timed and independently editable after a copy.
The agent needs exactly that freedom, with readable data and precise commands.

Today an event requires a pad on the receiving track; a pitched event requires
that pad's sample to have a root note. Duplicating a clip keeps its pattern
reference, and Own Copy is a separate action. Notes therefore cannot be
programmed on an instrumentless track as required, and ordinary duplication
does not have the required independence. MIDI file interchange is not built.

This comes before packaging and the agent panel because it establishes the
musical objects those clients and future instruments will use. It can be built
with the existing host, CLI and app; neither a new project extension nor an
embedded agent is a dependency.

## Design

### Musical data and persistence

Use integer MIDI pitches 0–127 as the only stored pitch value for these notes.
Velocity is 1–127, duration is positive, and positions and durations use the
existing exact quarter-note beat representation. Preserve fractions and
off-grid decimals. The grid is an editing aid, not a storage constraint.

Each MIDI clip and note has a persistent ID. IDs survive saving, reopening,
moving and editing; independent copies receive new clip and note IDs. Display
names, if present, are separate from identity. The first-round agent view uses
numeric pitches. The piano roll may derive note labels using a documented
octave convention; there is no stored spelling, key or scale requirement.

An illustrative document fragment:

```yaml
tracks:
  - id: keys
    type: midi
    instrument: null
    clips:
      - id: phrase_a
        at: 16
        length_beats: 4
        notes:
          - {id: n1, pitch: 60, at: 0, duration: 1, velocity: 96}
          - {id: n2, pitch: 64, at: 0, duration: 1, velocity: 80}
          - {id: n3, pitch: 67, at: 0, duration: 1, velocity: 88}
          - {id: n4, pitch: 62, at: 2.025, duration: 0.5, velocity: 72}
```

The clip's position is in song beats; each note's position is relative to its
clip. Simultaneous notes form chords. Overlapping notes are valid, with voice
and note-off handling tested, including repeated pitches. Moving a clip leaves
its relative note positions alone. A single-note octave shift adds 12 to pitch;
an edit outside the supported pitch range is refused rather than silently
clamped. Off-grid timing changes are equally explicit.

The Rust model owns the data, the host owns the open revision, and the project
document saves it, currently in `song.yaml`. JSON inspection is a view of that
same state. A `.mid` file is an import/export artifact, not a second live source
of truth. This feature does not depend on the separate `.aaw` extension work.

### Instruments

An absent instrument is a valid state: notes can be created, saved, copied,
inspected and exported, and playback produces no instrument audio on that track.
The editor must show the full usable pitch range without a sample or root note.

Adapt the existing sampler behind a note-capable instrument boundary. The
instrument owns sample selection, root pitch, envelopes and velocity response;
the notes own musical pitch, timing and dynamics. Attaching, removing or
switching a sampler configuration preserves the notes exactly. Use two sampler
configurations to demonstrate replacement without building a synth in this item.

Keep existing sampler limits explicit: note-off starts release, and a sample
without a sustain loop can end before the requested note duration. Velocity
controls the instrument's response and is not a per-note dB value. Future synths
must be able to accept the same note data without adopting sample-pad fields.
Reuse the playback/render engine and keep device state out of the song's note
objects.

### Editing and the agent

The app provides track/clip creation, a piano roll, note creation and deletion,
pitch/start/duration/velocity edits, off-grid movement, clip placement and
copy/paste or duplication with independent content. Ordinary MIDI clip copies
must not require an Own Copy step. Internal sharing must not change that
behavior. Linked copies and relational variations are not required here.

The CLI exposes corresponding reads and edits over the existing host, including
instrument attachment/removal, bulk note creation and transposition. Inspection
returns numeric note fields and stable IDs, scoped to a clip or beat range;
the agent must not need to read the whole project for one note edit. A duplicate
command returns the new clip identity and enough information to address its
notes. A named batch can duplicate and vary a phrase as one undo step.

Edits from both clients use the existing validation, revision/precondition,
origin, change-log and undo mechanisms. A stale edit fails rather than changing
an unintended note. Save/reopen, undo/redo and app selection must agree on note
identity. Do not add a second Python song model or a separate editing path that
bypasses the host.

### Standard MIDI files

Provide a documented notes-only subset of Standard MIDI File import/export in
the CLI and app, including an instrumentless import and export of a selected
clip. Type 0 and type 1 note sequences are the initial target. Parse delta ticks
to absolute musical positions, pair note-on/off events (including note-on with
zero velocity), and export ordered messages from the editable notes.

A `.mid` dropped on the timeline creates MIDI note clips, including on a new
instrumentless track when necessary. It does not require assigning a sample
first. Preserve drum note numbers; sound assignment is the instrument's job.

Preserve supported pitches, starts, durations and velocities. Select sufficient
tick resolution and report timing rounding when exact export is impossible.
Import into an existing project keeps its tempo and reports relevant source
tempo information; it must not silently retime the whole project. Document
channel/track grouping, drum-channel treatment and clip-length handling.

Do not silently discard performance data that the first model cannot represent:
pedal/controllers, pitch bend, expression, tempo maps and other unsupported
cases must produce a specific report or refusal before committing an import.
An explicit notes-only conversion may be offered, with the losses reported.
Standard MIDI does not preserve AAW note IDs, instrument patches or independent
copy relationships; project save/reopen does. The implementation must state its
supported subset rather than claim general lossless MIDI round trips.

### Existing projects and build scope

Keep existing sample-pad patterns, audio clips and their sound working. Choose
and document schema versioning and compatibility before editing the model;
do not silently reinterpret old shared patterns as independent ones or rewrite
personal projects to test the migration. Any conversion must preserve their
musical behavior and have generated fixtures. Existing render reports and
legacy fingerprints must remain verifiable under the established contract.

The relevant code is `aaw-model/src/schema.rs`, `rules.rs` and `schedule.rs`;
`aaw-host/src/command.rs`, `session.rs` and `tree.rs`;
`aaw-engine/src/program.rs`; `aaw-ffi/src/view.rs` and `edits.rs`; and the Mac
`SongModel`, `PatternEditor` and `PatternLayout`. Reuse these layers rather than
creating a parallel sequencer. Published schema, `daw describe`, architecture
and the engine/app READMEs must be updated with implementation.

## Done when

1. In a scratch app project, create an instrumentless MIDI track and a clip,
   draw a chord and melody, save and reopen, and recover the same notes and IDs.
2. Edit pitch, duration and velocity individually. At 120 BPM, place notes at
   1.975 and 2.025 beats to demonstrate positions 12.5 ms ahead of and behind beat
   2. Triplets retain exact fractional positions. No operation silently snaps.
3. Copy/paste or duplicate the clip elsewhere, change the copy's timing, pitch
   and velocity, and verify the original's content remains identical. Verify
   independent copies and their IDs again after reopening and undo/redo.
4. Attach a sampler, hear the phrase, switch to another sampler configuration,
   and remove the instrument. Its notes remain identical throughout. Test chords
   and overlapping notes, duration/release and velocity response with generated
   audio. Compare playback and render through the existing engine checks.
5. The agent performs the same actions through commands, reads a single clip's
   numeric notes, and duplicates/transposes/varies it in a labeled batch. The
   app updates, undo restores it, and a stale expected revision is refused.
6. Export and reimport a generated notes-only MIDI clip; compare pitches,
   velocities, starts and durations within the declared tick precision. Test
   simultaneous notes, triplets, microtiming, both note-off encodings and the
   supported file types. Unsupported performance data is reported explicitly.
7. Existing generated legacy songs still load and render correctly. Test schema
   compatibility, identity, command operations and timing meaningfully. Run
   `cargo test` in `engine/`, then `uv run pytest -q`, and `./build.sh test` in
   `apps/mac/` after Swift changes. Use `AAW_DATA_DIR` for app verification.
8. A person tries the creation, note edits, independent copy and instrument
   replacement workflow by hand and ear. Record what was tried; anything not
   tried belongs under Verify rather than being claimed as heard.

Build this item on one branch and PR. On completion, rewrite this file as the
built reference, move the backlog item to completed with the date/PR/verification,
and renumber Next, following AGENTS.md.

## Open questions

These are implementation choices to settle at the start of the build, without
reopening the requirements above:

- The exact versioned schema: inline owned notes versus referenced content with
  independent-copy semantics; coexistence with legacy mixed tracks and patterns.
- Clip-edge behavior: note tails beyond a clip, a note dragged before its first
  beat, loop boundaries and clip resizing. Document and test the chosen behavior;
  no silent quantization or loss of a moved note.
- MIDI channel/track mapping and the exact unsupported-message policy, including
  ambiguous overlapping note-on/off pairs for the same pitch/channel.
