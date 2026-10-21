# MIDI effects (implemented 2026-10-21)

Status: built in the pull request that closed the first of the person's notes
of October 19, 2026 still open after racks, decision D140. The backlog's line
asked for a device before the instrument that changes the notes as they
play, an arpeggiator first with rate, order, octaves and gate on the grid,
then chord, scale and velocity devices; all four are built. What was settled
in building them is under Design.

## What

A MIDI track's `midi_effects` list devices before its instrument, in order,
each `{type, id, bypass, ...}` as an effect is. They change the notes as they
play: the clips keep the notes as written, and the schedule plays what the
chain makes of them.

```yaml
tracks:
- id: keys
  type: midi
  midi_effects:
  - {type: chord, shift_1: 7, shift_2: 12}
  - {type: arpeggiator, id: arp, rate: 1/16, order: up_down, octaves: 2, gate_percent: 60}
  - {type: scale, root: D, scale: minor}
  - {type: velocity, scale_percent: 80, random: 12, seed: 4}
  instrument: {synth: {...}}
```

| Kind | Fields | What it does |
|---|---|---|
| `arpeggiator` | `rate` 1/16 (`1/1`, `1/2`, `1/4`, `1/4T`, `1/8`, `1/8D`, `1/8T`, `1/16`, `1/16D`, `1/16T`, `1/32`, `1/32T`), `order` up (`up`, `down`, `up_down`, `down_up`, `converge`, `diverge`, `played`, `random`), `octaves` 1…4, `gate_percent` 1…200, `seed` | The notes held at each step of a grid of `rate`, from beat 0, played one at a time in `order` over the octaves, each `gate_percent` of a step long and never longer than the held note it came from. The pattern starts over when a note begins. |
| `chord` | `shift_1` … `shift_6` −36…36 semitones | Each note played with a note at each shift that is not 0; two shifts alike add one note. |
| `scale` | `root` C…B, `scale` major, minor, harmonic_minor, melodic_minor, dorian, phrygian, lydian, mixolydian, locrian, major_pentatonic, minor_pentatonic, blues, whole_tone, chromatic, `transpose` −36…36 | Each note moved to the nearest note of the scale, a note halfway between two going down, then by the transpose. |
| `velocity` | `scale_percent` 0…400, `offset` −127…127, `random` 0…127, `seed`, `min` 1…127, `max` 1…127 | Each velocity scaled, moved, spread by up to `random` either way by the seed, and held between `min` and `max`. |

`daw effect add SONG tracks.T --type arpeggiator --rate 1/8` puts one on a
MIDI track, under `midi_effects` by its kind; on any other owner it is
refused with where it goes. `daw effect remove|move|bypass SONG
tracks.T.midi_effects.REF` and `daw set SONG tracks.T.midi_effects.REF.FIELD`
edit it, REF an id or an index; `--to` is refused, since a MIDI effect stays
before its instrument. `daw describe midi` says what each does and lists the
fields, and `daw effect add --help` lists them after the audio effects. In
the app the browser's MIDI Effects lists the four kinds, dropped on a MIDI
track's header or its device panel or added with + or a double-click, and
Add Effect on a MIDI track offers them after the audio effects; each is a
panel before the instrument with its bypass, the moves and ×, and a control
for each field from `describe`. A frozen track's MIDI effects are part of
what it was rendered from, so a change to them is refused until it is
unfrozen.

## Why

A chord held in a clip and heard as a run, a line heard as chords, a part
kept in a key while it is moved and a flat performance given some life are
what every DAW's MIDI devices do, and what the person asked for after using
the app. Doing it in the schedule rather than in the engine keeps playback,
a render, a freeze and `daw listen`'s `sounded` the same notes with no new
real-time code, and keeps the clips as the person wrote them, so the devices
are taken off without a trace.

## Design

- **A transform of the schedule, in exact beats.** `track_notes` collects a
  track's notes as it did and passes them through `midi_effects::apply`
  before the frames are computed, so every reader of the schedule plays what
  the chain makes: the sampler's hits, the Synth's notes, `sounded` and a
  freeze. A note played now through `note.preview`, the piano roll's
  previews and the Synth panel's keys, `daw synth audition` and `daw midi
  export` are the notes as written: the first is not in the schedule, and
  the last two are the patch's sound and the clip's file.
- **The arpeggiator is on the song's grid,** from beat 0, as the line asked:
  a chord that begins off the grid is heard from the next step. At each step
  the notes held, those that have started and not ended, are the chord; the
  pattern counts steps from the last step at which a note began and starts
  over there, as Ableton's arpeggiator does with its retrigger on a note.
  Each note made is the step's share of the gate, cut where the held note
  it came from ends, so a run stops with the chord. A note shorter than a
  step that no step falls on is not heard; hold a chord a step or longer.
  `random` picks by the seed and the step's number, so a render is the same
  every time and a note added elsewhere does not change the picks.
- **Orders as Ableton names them.** `up_down` and `down_up` do not repeat
  the ends; `converge` goes from the ends inward and `diverge` is its
  reverse; `played` keeps the order the notes began in; the octaves repeat
  the walk an octave higher each time round.
- **A chord is six shifts,** as Ableton's, rather than a list, so the
  generic panel draws it and `daw set` reaches each; 0 is off.
- **A scale snaps to the nearest degree, halfway going down,** then
  transposes, so a transpose of an octave stays in the scale; a note moved
  off the keyboard is dropped.
- **No lanes.** A MIDI effect's fields are not lane targets: a lane is read
  per frame in the engine and these run in the schedule. A lane on an
  arpeggiator's gate or a velocity's scale is a backlog line if it is
  wanted.
- **Through `daw effect`, by the kind.** The verbs that add, remove, move
  and bypass an effect take a MIDI effect too, so the agent and the app have
  one way to reach a device; a MIDI kind on an owner that is not a MIDI
  track is refused with where it goes, and an audio kind under
  `midi_effects` is refused with the same.
- **In the song as `midi_effects`,** a list of the track's beside
  `instrument`, so a track's two chains are told apart by name and a song
  without one saves as it did; the fingerprints gain the field's form and
  the golden model was re-pinned.

## Done when

- Each device makes of a list of notes what the table says, on the grid,
  starting over, cutting at the source, keeping the keyboard, the same for
  the same seed: held by `crates/aaw-model/src/midi_effects.rs`.
- Through `daw render`, a held chord through an arpeggiator is the run, the
  effects add, move, bypass and remove by their path, a chord before and
  after an arpeggiator differ, and a MIDI kind on an audio track is refused:
  held by the Python suite.
- `daw describe midi` says what each does and lists the fields, and `daw
  effect add --help` lists the kinds.
- The app draws the panel before the instrument from `describe`, lists the
  kinds in the browser and in Add Effect, and lands a kind dropped on a MIDI
  track's header or its device panel: seen in a scripted picture.

## Open questions

- Whether the arpeggiator should also run on a note played now, so a chord
  held on the Synth's keys or a MIDI keyboard arpeggiates as it does in Ableton,
  which needs it in the engine rather than the schedule.
- Whether a note shorter than a step should be heard at the next step
  anyway, as a staccato chord is, rather than lost.
- Whether lanes on `gate_percent`, `scale_percent` and `transpose` are
  wanted.
