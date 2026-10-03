# MIDI tracks and note clips — proposed October 3, 2026

Status: item 1 built October 3, 2026 (#39), item 2 the same day (#40);
item 3 being built. They were backlog items 1, 2 and 3, one branch and pull request
each, in that order. Product requirements are in
[concept.md](../concept.md#sound); decisions D60 to D65 record
the reasons. What item 2 built in the app is in
[apps/mac/README.md](../../apps/mac/README.md). The schema and commands of item 1 are as
`daw describe midi` and [engine/README.md](../../engine/README.md) give them;
this file becomes their reference once item 3 is built.

## What

A MIDI track that can hold editable note clips with no instrument attached.
The person draws melodies, chords and rhythms, edits pitch, start, duration and
velocity, and copies a clip elsewhere to make an independent variation. The
agent reads and edits those same objects. Attaching, removing or replacing a
sampler changes the sound without changing the notes. The instrument boundary
also accommodates software instruments built later.

The work is three items, each useful on its own:

1. **Note clips in the song and the commands.** The Rust model, the schema,
   the sampler as a note instrument, host commands, inspection, playback and
   render. The agent can program parts from here; the app shows MIDI tracks
   and their clips but does not edit them.
2. **Note clips in the app.** The piano roll, clip placement, copy and paste,
   and attaching and swapping the instrument.
3. **Standard MIDI files.** One part's notes imported as a note clip and a
   note clip exported, in the CLI and the app, and a `.mid` dropped on the
   timeline (D65).

None of them builds a synthesizer, plugin hosting, MIDI keyboard recording,
MIDI effects, controller/expression editing, MPE, a chord language or a theory
engine. Those are separate backlog items. Explicit enharmonic spelling is
deferred (D61). Converting existing pattern clips into note clips is a Later
item (D62).

## Why

The person's base Ableton workflow depends on musical material being separate
from the instrument, freely timed and independently editable after a copy.
The agent needs exactly that freedom, with readable data and precise commands.

Today an event requires a pad on the receiving track; a pitched event requires
that pad's sample to have a root note. Patterns live in the project and a clip
refers to one by name, so duplicating a clip shares its pattern until Own Copy
is pressed. Notes therefore cannot be programmed on an instrumentless track as
required, and ordinary duplication does not have the required independence.
MIDI file interchange is not built.

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

**A clip owns its notes.** Notes are written inside the clip, not in a
project-level list the clip refers to as patterns are. Copying a clip copies
its notes, so a copy is independent with no further step, and nothing about
sharing can leak into it. Linked copies and a clip defined by its relation to
another stay in Later.

Each MIDI clip and note has a persistent ID that the host assigns. A clip's ID
is unique in the project and a note's is unique in its clip; the agent and the
app never have to make one up, and a command that creates a clip or notes
replies with their IDs. IDs survive saving, reopening, moving and editing;
copies receive new clip and note IDs. Display names, if present, are separate
from identity.

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

**A MIDI track holds note clips and nothing else of a clip's kind.** It has an
instrument, note clips, effects, sends and automation; it has no track-level
pads, pattern clips or audio clips. A legacy track is unchanged. Keeping the
two apart means neither the schema nor the app has to say what a pattern clip
on a MIDI track would play.

**Schema version 2** adds MIDI tracks. A song with no MIDI track still saves as
version 1, byte for byte as today, so no existing project is rewritten by
opening it. A song with one saves as version 2, which an older `daw` refuses
with a clear error rather than misreading.

The Rust model owns the data, the host owns the open revision, and the project
document saves it, currently in `song.yaml`. JSON inspection is a view of that
same state. A `.mid` file is an import/export artifact, not a second live source
of truth. This feature does not depend on the separate `.aaw` extension work.

### Instruments

An absent instrument is a valid state: notes can be created, saved, copied,
inspected and exported, and playback produces no instrument audio on that track.
The editor must show the full usable pitch range without a sample or root note.

The instrument is one value on the track, and replacing it is one edit. The
first kind is a sampler that maps notes to pads:

```yaml
  - id: keys
    type: midi
    instrument:
      sampler:
        pads:
          piano: {sample: piano_c4, mode: gate, release_ms: 300}
        map:
          - {notes: [0, 127], pad: piano, pitched: true}
    clips: [...]
  - id: drums
    type: midi
    instrument:
      sampler:
        pads:
          kick: {sample: kick_01}
          snare: {sample: snare_03, choke_group: snare}
        map:
          - {notes: 36, pad: kick}
          - {notes: 38, pad: snare}
    clips: [...]
```

A pad keeps today's fields, so modes, envelopes, gain, pan, choke groups and
stretching are reused rather than redesigned. A map entry names a note or an
inclusive range of notes and the pad it plays. A pitched entry plays its pad
repitched by the note's distance from the sample's `root_note`, which must be
known; an unpitched entry plays the pad as it is, whatever the note. Entries
may not overlap. A note no entry maps is silent, and `daw check` says which
notes in which clips fell outside the map. This is a drum rack and a
multisample in one shape, and it is what gives a drum note number a sound.

The instrument owns sample selection, root pitch, envelopes and velocity
response; the notes own musical pitch, timing and dynamics. Attaching, removing
or switching a sampler configuration preserves the notes exactly. Use two
sampler configurations to demonstrate replacement without building a synth.

In the engine, an instrument receives notes and nothing about pads: a note's
pitch, velocity, start frame and end frame, each with a voice handle. The
sampler turns that into the voices it plays today. A future synth is another
value under `instrument` (`synth: {...}`) that receives the same notes and
adopts no sample-pad fields. Device state stays out of the song's note objects.

Keep existing sampler limits explicit: note-off starts release, and a sample
without a sustain loop can end before the requested note duration. Velocity
controls the instrument's response and is not a per-note dB value.

### Relation to patterns

Note clips are where pitched and rhythmic material is headed; pattern clips
and step rows are the format of songs made before them (D62). Items 1–3 leave
pattern clips, their events, step rows and Own Copy working and sounding as
they do, on legacy tracks, and add nothing to them. A later item converts a
legacy track into a MIDI track: its pads become the sampler's pads, a pitched
pad gets a pitched range, an unpitched pad gets a note of its own, and each
pattern clip becomes a note clip that owns a copy of its pattern's events. It
must preserve the sound exactly, against generated fixtures, and is only run
when asked.

### Editing and the agent

Item 1 gives the CLI reads and edits over the existing host, beside today's
`clip` and `pattern event` groups: MIDI track creation (`daw track add
--type midi`), instrument attach, replace and remove (`daw instrument set`,
`remove`, `map`, and `daw pad` on the sampler), clip create, move, resize,
duplicate and remove (`daw clip`), and note add (in bulk), edit, move,
transpose and remove (`daw note`), with `daw note list` to read them. Inspection returns numeric
note fields and stable IDs, scoped to a clip or beat range; the agent must not
need to read the whole project for one note edit. A duplicate returns the new
clip's ID and its notes' IDs. A named batch can duplicate and vary a phrase as
one undo step.

Commands also accept a note name for a pitch, as events do today (`C4` is 60,
the convention `rules::midi` already uses), and store the number. Inspection
gives each note's derived name beside its number. Nothing stores the name; it
is there because an agent writing `pitch: 48` for middle C is an easy mistake
and `C3` beside it makes the mistake visible.

Item 2 gives the app track and clip creation, a piano roll, note creation and
deletion, pitch/start/duration/velocity edits, off-grid movement, clip
placement and copy/paste or duplication with independent content. The piano
roll reuses `PatternEditor` and `PatternLayout` where they fit; it shows the
full pitch range whatever the instrument, and labels rows with derived names.

Edits from both clients use the existing validation, revision/precondition,
origin, change-log and undo mechanisms. A stale edit fails rather than changing
an unintended note. Save/reopen, undo/redo and app selection must agree on note
identity. Do not add a second Python song model or a separate editing path that
bypasses the host.

### Standard MIDI files

Item 3 reads and writes one part (D65): the notes of one note clip, with
their pitches, starts, durations and velocities. It is what saving a chord
progression played on a synth and bringing it into a new song needs.

**Import.** `daw midi import SONG FILE [--track TRACK] [--at BEAT]` reads a
type 0 or type 1 file whose notes are all in one file track and on one
channel. A type 1 file's tempo track, which has no notes, does not count as a
part. The notes become one note clip at `--at`, 0 unless given: on `--track`,
which must be a MIDI track, or else on a new MIDI track with no instrument,
named after the file. The clip starts at the file's beat 0, so a part that
begins after a rest keeps the rest, and it lasts to the end of its last note,
rounded up to a bar. The song grows to hold it, as it does for an audio
clip. It is one command, `midi.import`, and one undo step.

A position in the file is its tick over the file's ticks per beat, an exact
fraction, so nothing is rounded on the way in. A note-on with velocity 0 is
a note-off. A note-off ends the earliest note of its pitch still sounding,
so overlapping notes of one pitch stay as overlapping notes. A note with no
note-off ends at the end of its file track, and one of no length lasts one
tick; the reply counts both. A file timed in SMPTE frames rather than ticks
per beat is refused, as is a file with notes in more than one file track or
on more than one channel, with a message naming each part by its track,
channel and name.

The file's tempo is not taken. Its positions are in beats, so the part lands
on the same bars at the song's tempo; the reply gives the file's first tempo
for information. Drum notes keep their numbers, and the sampler's map gives
them sounds.

The song holds notes and nothing else of a MIDI file. What else the file
has is left out and counted in the reply, by kind: the sustain pedal, pitch
bend, other controllers, program changes, aftertouch, system exclusive
messages, and tempo or time signature changes after the start. Nothing asks
first. A person who played with the pedal down gets the notes as long as
their fingers held them, which the count of pedal messages explains. Names,
text and other meta events are not counted.

**Export.** `daw midi export SONG CLIP FILE` writes one note clip as a type 0
file of one track: the notes that play, at 960 ticks a beat, with the song's
tempo and time signature and the track's name. A note outside its clip does
not play and is left out, and one that lasts past the clip's end is
shortened to it; the reply counts both. 960 holds triplets and positions
such as 2.025 exactly. A position that is not a whole number of ticks is
rounded to the nearest, and the reply names those notes. Every note is on
channel 1.

**The app.** A `.mid` file dropped on a MIDI track's lane makes its clip
there, at the grid line nearest the drop; dropped anywhere else on the
timeline, on a new MIDI track. A file refused shows its reason, as a
refused edit does. File › Export MIDI Clip… writes the selected note clip
where the save panel says.

Standard MIDI does not keep AAW note IDs, instruments or the song's tempo
on import; saving the project does. An import gives the notes new IDs.

### Existing projects and code

Existing render reports and legacy fingerprints must remain verifiable under
the established contract; a version 1 song's fingerprint does not change.

The relevant code is `aaw-model/src/schema.rs`, `rules.rs` and `schedule.rs`;
`aaw-host/src/command.rs`, `session.rs` and `tree.rs`;
`aaw-engine/src/program.rs`; `aaw-ffi/src/view.rs` and `edits.rs`; and the Mac
`SongModel`, `PatternEditor` and `PatternLayout`. Reuse these layers rather than
creating a parallel sequencer. Published schema, `daw describe`, architecture
and the engine/app READMEs are updated with each item.

## Done when

### Item 1: note clips in the song and the commands

1. With commands alone, create an instrumentless MIDI track and a clip, add a
   chord and a melody, save and reopen, and recover the same notes and IDs.
   The app shows the track and its clips.
2. Edit pitch, duration and velocity individually. At 120 BPM, place notes at
   1.975 and 2.025 beats to demonstrate positions 12.5 ms ahead of and behind
   beat 2. Triplets retain exact fractional positions. No operation silently
   snaps. A pitch given as a name is stored as its number.
3. Duplicate the clip, change the copy's timing, pitch and velocity, and verify
   the original's content remains identical, again after reopening and after
   undo/redo.
4. Attach a pitched sampler, render the phrase, replace it with a second
   sampler configuration, then remove the instrument; the notes remain
   identical throughout. A drum sampler plays mapped notes and `daw check`
   names unmapped ones. Chords, overlapping and repeated notes, duration and
   release, and velocity response are tested with generated audio, and
   playback and render agree through the existing engine checks.
5. The agent reads a single clip's numeric notes, and duplicates, transposes
   and varies it in a labeled batch while the app is open. The app updates,
   undo restores it, and a stale expected revision is refused.
6. Generated version 1 songs load, render and fingerprint as before and save
   unchanged. `cargo test` in `engine/`, then `uv run pytest -q`, and
   `./build.sh test` in `apps/mac/` pass.

### Item 2: note clips in the app

1. In a scratch app project (`AAW_DATA_DIR` set), create an instrumentless MIDI
   track and a clip, draw a chord and melody, and edit pitch, start, duration
   and velocity of single notes, on and off the grid.
2. Copy/paste or duplicate the clip, change the copy, and the original is
   unchanged, after reopening and undo/redo too.
3. Attach, swap and remove a sampler from the app; the notes do not change.
4. A person tries creation, note edits, an independent copy and instrument
   replacement by hand and ear. Anything not tried goes under Verify.

### Item 3: Standard MIDI files

1. Export and reimport a generated note clip; pitches, velocities, starts
   and durations match exactly where they fall on a tick. Chords, triplets,
   notes just ahead of and behind a beat, overlapping notes of one pitch,
   both note-off encodings, and type 0 and type 1 files are tested.
2. A file with a pedal, pitch bend and controllers imports its notes, and the
   reply counts what was left out by kind. A file of two parts is refused
   with a message that names them.
3. A `.mid` dropped on the timeline in the app makes a note clip, on a new
   track with no instrument where it was not dropped on a MIDI track, and
   File › Export MIDI Clip… writes the selected clip. Drum note numbers are
   kept.

On completing each item, move its backlog line to completed with the
date/PR/verification and renumber Next, following AGENTS.md. After item 3,
rewrite this file as the built reference.

## Open questions

These are implementation choices to settle at the start of the item they
belong to, without reopening the requirements above. Item 1's two, the clip's
edges and whether a note clip repeats, are settled in D63: a note sounds until
its clip's end, one that starts past it is kept and silent, and note clips do
not repeat. Item 2's, a clip shortened from its left edge, is settled in D64:
its notes stay where they are in the song, and those it passes are kept before
the clip, at a negative `at`, and do not play. Item 3's, how a file's
tracks and channels become clips and what happens to data the song cannot
hold, is settled in D65: a file is one part and makes one clip, and what is
left out is counted in the reply.
