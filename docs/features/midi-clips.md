# MIDI tracks and note clips — implemented October 3, 2026

A MIDI track holds note clips that own their notes, and an instrument that plays
them or none. The person and the agent make and edit the same notes: pitches,
starts, lengths and velocities, on or off any grid, and a copy of a clip is its
own. Replacing or removing the instrument leaves the notes as they are. A
Standard MIDI file of one part comes in as a note clip, and a note clip goes
out as one.

It was built as three items on October 3, 2026, each its own pull request:
note clips in the song and the commands (#39), note clips in the app (#40),
and Standard MIDI files. Decisions D60 to D65 record the reasons. The schema
and commands are as `daw describe midi` gives them, and what the app does is
in [apps/mac/README.md](../../apps/mac/README.md).

## Why

The person's way of working in Ableton keeps musical material apart from the
instrument that plays it, freely timed, and editable after a copy. The agent
needs the same freedom, with readable data and exact commands. Before this an
event needed a pad on its track, a pitched event a sample with a root note, and
a duplicated clip shared its pattern until Own Copy was pressed.

## The song

```yaml
tracks:
  - id: keys
    type: midi
    instrument:
      sampler:
        pads:
          piano: {sample: piano_c4, mode: gate, release_ms: 300}
        map:
          - {notes: [0, 127], pad: piano, pitched: true}
    clips:
      - id: clip1
        at: 16
        length_beats: 4
        notes:
          - {id: n1, pitch: 60, duration: 1, velocity: 96}
          - {id: n2, pitch: 64, duration: 1, velocity: 80}
          - {id: n3, pitch: 67, duration: 1, velocity: 88}
          - {id: n4, pitch: 62, at: 2.025, duration: 0.5, velocity: 72}
```

**Notes.** A pitch is a MIDI number, 0 to 127, and the only pitch stored: C-sharp
and D-flat are one value (D61). Commands take a name and store its number, C4
being 60, and readings give the name beside the number. Velocity is 1 to 127,
100 unless given. Positions and lengths are exact beats: fractions such as
`7/3` and decimals such as `2.025` are kept, and nothing snaps. A note's `at`
is from its clip's start, and the clip's `at` is a song beat.

**A clip owns its notes** (D62). A copy is a deep copy with an ID of its own,
so changing it changes nothing in another clip. The host gives a clip that has
no ID the next free `clipN`, unique in the song, and a note the next free `nN`,
unique in its clip; the IDs last through edits, saving and reopening (D63).

**Edges.** A note sounds until its end or its clip's, whichever is first. One
that starts at or after its clip's end, or before its start, is kept and does
not play, so a clip trimmed from either edge and drawn out again loses nothing;
a note before its clip has a negative `at` (D64). `daw check` and
`daw note list` name the notes outside their clip. Note clips do not repeat: a
copy plays a phrase again.

**A MIDI track** has an instrument, note clips, effects, sends and automation,
and no pads, pattern clips or audio clips. A song with a MIDI track is saved as
`schema_version: 2`, which an engine from before MIDI tracks refuses; a song
without one is saved as version 1, byte for byte as before.

**The instrument** is null, which plays nothing, or a sampler of pads and a map
from notes to pads. A map entry is a note or an inclusive range and a pad,
pitched or not: a pitched entry repitches the pad from its sample's root note,
or from middle C when the sample has none (D66), to the note, and one that is not plays the pad as it is, as a drum rack does.
Entries may not overlap, and a note no entry maps is silent. A pad is any
pad's fields, so modes, envelopes, choke groups and stretching are the ones
patterns use. The engine gives an instrument notes alone, a pitch, velocity,
start and note-off each, so that a synth can later take the same notes.

**Patterns** on tracks of the older kind work and sound as they did. Note clips
are where material is headed; converting a track of patterns into a MIDI track
is a Later item.

## Commands

`daw track add SONG ID --type midi` makes a MIDI track with no instrument.
`daw clip add`, `move`, `duplicate`, `resize`, `trim` and `remove` place note
clips; `daw note add`, `set`, `move`, `transpose` and `remove` edit notes, a
clip standing for all its notes; `daw note list SONG CLIP|TRACK` reads them with
names and song beats, between `--from` and `--to` if given. `daw instrument
set`, `remove` and `map` attach, take off and map the sampler, and `daw pad`
edits its pads. A command that makes clips or notes replies with their paths,
and a labeled `daw batch` makes a phrase and its variations one undo step.
Edits from the agent and the app go through the host, with its validation,
`--expect`, origins, change log and undo.

## The app

A MIDI track is added with ⇧⌘T and a note clip by a double-click on its lane.
The piano roll shows all 128 notes whatever the instrument, and adds, selects,
moves, stretches, types and removes notes on its grid or, with Option, off it.
Note clips are moved, trimmed at either edge and duplicated, and Copy, Cut and
Paste take clips or notes as they are. A sample dropped on a MIDI track's header
becomes its instrument, a sampler that plays it at every note's pitch, as it
is at middle C (D66). A MIDI file
dropped on the timeline is a note clip, and File › Export MIDI Clip… writes one.

## Standard MIDI files

The song reads and writes one part (D65): the notes of one note clip, with their
pitches, starts, lengths and velocities. It is what saving a chord progression
played on a synth and bringing it into another song needs.

**Import.** `daw midi import SONG FILE [--track TRACK] [--at BEAT]` reads a type
0 or type 1 file whose notes are all in one file track and on one channel; a
type 1 file's tempo track has no notes and is not a part. The notes become one
note clip at `--at`, 0 unless given, on `--track`, which must be a MIDI track,
or else on a new MIDI track with no instrument named after the file. The clip
starts at the file's beat 0, so a part that begins after a rest keeps it, and
lasts to the end of its last note in whole bars. The song grows to hold it, to
the end of that bar. It is one command, `midi.import`, and one undo step. In
the app a `.mid` dropped on a MIDI track's lane makes its clip there, and one
dropped anywhere else on the timeline goes on a new MIDI track; the outline
while it is dragged is the clip's length.

A position is a tick over the file's ticks per beat, an exact fraction, so
nothing is rounded on the way in. A note-on with velocity 0 is a note-off, and
running status is read. A note-off ends the earliest note of its pitch still
sounding, so overlapping notes of one pitch stay overlapping. A note with no
note-off ends where its file track ends, and one of no length is made one tick
long; the reply's `adjusted` counts both.

The file's tempo is not taken. Its positions are in beats, so the part lands on
the same bars at the song's tempo; the reply's `file_tempo` is the first tempo
the file sets. Drum notes keep their numbers, and the sampler's map gives them
sounds. The reply's `left_out` counts by kind what the song does not hold: the
sustain pedal, pitch bend, other controllers, program changes, aftertouch,
system exclusive messages, tempo and time signature changes after the start,
and note-offs that end no note. Nothing asks first. A part played with the
pedal down comes in as long as the fingers held the notes, which the count of
pedal messages explains. Names, text and other meta events are not counted.

These are refused, with nothing changed: a file with notes in more than one
file track or on more than one channel, with each part named by its track,
channel and name; a type 2 file; a file timed in SMPTE frames; a file with no
notes; and a file that is not a MIDI file or ends in the middle of an event.

**Export.** `daw midi export SONG CLIP FILE`, and File › Export MIDI Clip…
(⇧⌘E) with one note clip selected, write the notes that play as a type 0 file
of one track at 960 ticks a beat, named after the track, with the song's tempo
and four beats to a bar, on channel 1. A note outside its clip is left out and
one that lasts past its end is shortened to it. 960 ticks hold triplets and
places such as 2.025 exactly; a place that is not a whole number of ticks is
rounded to the nearest, and the reply names those notes. At one tick, notes end
before notes start, so a note repeated right after itself reads back as two.
A clip with no notes that play is refused.

A file does not keep note IDs, instruments or the song's tempo on import;
saving the project keeps them. An import gives the notes new IDs.

## Limits

- A note of one pitch that starts inside a longer note of the same pitch and
  ends before it cannot be told apart in a MIDI file: read back, the first
  note-off ends the first note, and the two lengths change places. The export's
  reply counts such notes.
- A sample without a sustain loop can end before its note's length, and
  note-off starts the release. Velocity scales the level linearly.
- A file of several parts, the file's tempo, and the pedal, pitch bend and
  controllers are not read, and a song is not exported as one file; they are
  one line under Later. Controller editing, MIDI keyboard recording, a synth,
  enharmonic spelling and clips that loop are Later items too.

## Code

The model is in `aaw-model/src/schema.rs`, `rules.rs` and `schedule.rs`; the
commands in `aaw-host/src/command.rs` and `session.rs`; MIDI files in
`aaw-host/src/midi_file.rs`; the instrument in `aaw-engine/src/program.rs`;
the app's view and edits in `aaw-ffi/src/view.rs` and `edits.rs`, and the Mac
`NoteEditor`, `PianoRollLayout` and `ArrangementView`.
