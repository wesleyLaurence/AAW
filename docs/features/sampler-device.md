# The Sampler device — proposed October 3, 2026

Status: proposed, not built. Backlog item 2, after the [browser](browser.md),
which it is dragged out of.

## What

An instrument named Sampler that the person puts on a MIDI track and then
loads with a sample, as Ableton's Simpler is used:

1. Make a MIDI track.
2. Drag Sampler from the browser's Instruments onto it. An empty Sampler
   appears in the device panel at the bottom, saying to drop a sample on it.
3. Drag a sample onto the device, from the browser or the Finder. The device
   shows its waveform, and the keys play it: as it is at middle C, higher and
   lower on the keys around it (D66).
4. Shape it in the device: where in the file it starts and ends, its root
   note, whether a note plays it to its end or only while held, its attack and
   release, its level and pan.

## Why

The person wants to play a sample as an instrument and shape it where it
plays, not only to drop it on a header and live with what it does. Today a
sampler on a MIDI track shows a list of its pads and the notes each plays, and
its sound can be changed only with `daw pad set` or the agent. A sample dropped
on a header was not pitched until D66, which is how this came up.

## Design

**The model needs nothing new.** A Sampler is the `sampler` instrument the song
already holds. Empty, it is `{sampler: {pads: {}, map: []}}`, which the model
takes and which plays nothing. Loaded, it is one pad and one pitched map entry
on every note, `{notes: [0, 127], pad, pitched: true}`, the same as a sample
dropped on a header makes now. Every control in its panel is a field the song
has, so the agent reads and changes what the person sees with the commands it
has.

**Dropping a sample on the device.** The app copies it into the project as a
drop on a header does, and makes one batch: the sample, the pad, the map entry.
On a Sampler that already has a sample, the new one takes its place and the
pad's other settings stay, as in Simpler, apart from the start and end, which
go back to the whole file. One undo step, "Load piano into the Sampler on
keys".

**Dropping a sample on a MIDI track's header** stays the shortcut it is: an
empty Sampler and a sample in one step.

**The panel.** It takes the place of the list of pads and notes when the
instrument is one pad on every note, and is wider than an effect's panel to
leave room for the waveform.

| Control | Field | Notes |
|---|---|---|
| Waveform, with a start and an end marker to drag | `start_seconds`, `end_seconds` | The file's waveform as clips draw it. The part outside the markers is dimmed |
| Root | the sample's `root_note` | Typed as C4 or 60. Empty means middle C. A Measure button sets it to the pitch `daw samples analyze` finds, for a sample of one note |
| Transpose | `transpose` | Semitones, with hundredths as cents |
| Mode: One-shot or Held | `mode`: `one_shot` or `gate` | Held stops a note at its note-off, after the release |
| Attack, Release | `attack_ms`, `release_ms` | |
| Level, Pan | `gain_db`, `pan` | |
| Reverse | `reverse` | |

The controls are drawn from the fields as an effect's panel is (D44): label,
unit, range and default from the schema, with a bar dragged or a value typed,
and Shift for finer steps. The waveform and the Root field are the two drawn
for the Sampler alone.

**Playing it from a keyboard,** the computer's or a MIDI keyboard, is not part
of this; it is the MIDI keyboard line under Later. Notes are drawn in the piano
roll, as now.

**A sampler of several pads,** such as a kit made with `daw instrument map`,
keeps the list it has now, and is not this device. A kit made in the app is
the drum kit line under Later.

## Done when

- A Sampler dragged onto a MIDI track is empty and says to drop a sample; a
  sample dropped on it, from the browser and from the Finder, plays across the
  keys from middle C, in one undo step.
- A second sample replaces the first and keeps the pad's settings.
- Each control changes its field, is heard while playing, and shows the value an
  agent's command sets.
- The Root field set by hand and by Measure changes the pitch the keys play at,
  and `daw check` has no root warning after Measure.
- Engine tests cover the batch a load makes; `./build.sh test` covers the
  panel's edits, and the window is seen in a picture empty, loaded, and with the
  markers moved. Listening to it by hand is a line under Verify.
- `apps/mac/README.md` describes the device, `daw describe sampler` says what an
  empty Sampler is, and this file is rewritten as its reference.

## Open questions

- **Root note on the sample or the pad.** The song keeps a root note per
  sample, so setting it in one Sampler changes every pad that plays the same
  file. Ableton keeps it per instrument. A `root_note` on the pad that overrides
  the sample's would match Ableton; it is a model change, and is to be decided
  when this is picked up.
- **A note range.** Simpler plays on every key. Whether the panel should set
  the lowest and highest note, so that a Sampler plays only a part of the keys,
  is left for later.
- **Glide, a sustain loop, a filter and velocity to level** are what Simpler has
  beyond this. Glide and sustain loops are already in the concept, under
  Instruments.
