# The Sampler device — implemented October 3, 2026

Status: implemented in [#51](https://github.com/wesleyLaurence/AAW/pull/51).
Backlog item 1, after the [browser](browser.md), which it is dragged out of.
The [Synth](synth.md) is next and shares its wide instrument panel.

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
   release, its level and pan, and whether it plays backwards.

## Why

The person wants to play a sample as an instrument and shape it where it
plays, not only to drop it on a header and live with what it does. Before
this, a sampler on a MIDI track showed a list of its pads and the notes each
plays, and its sound could be changed only with `daw pad set` or by the agent.

## How it works

**The model has nothing new.** A Sampler is the `sampler` instrument the song
already holds. Empty, it is `{sampler: {pads: {}, map: []}}`, which plays
nothing. Loaded, it is one pad and one pitched map entry on every note,
`{notes: [0, 127], pad, pitched: true}`, the same as a sample dropped on a
header makes. Every control in its panel is a field the song has, so the
agent reads and changes what the person sees with the commands it has: `daw
pad set` for the pad, `daw set samples.S.root_note` for the root note. `daw
describe sampler` says so under `device`.

**When a sampler is the device.** The app draws the Sampler panel for a
sampler that is empty, or that is exactly one pad played on every note at its
pitch. A sampler of several pads, such as a kit made with `daw instrument
map`, keeps the list of pads and notes it had, and is not this device; a kit
made in the app is the drum kit line under Later in the backlog. A track with
no instrument takes a sample dropped on its panel as an empty Sampler would.

**Dropping a sample on the device.** The app copies it into the project as a
drop on a header does, and sends one batch: the sample listed, then
`instrument.set` with the pad and the map entry. The pad is named after the
sample, as a pad from a header is. On a Sampler that already has a sample,
the new one takes its place: the pad is renamed after the new sample so that
commands name it for what it plays, and its mode, level, pan, transpose,
attack, release and reverse carry over, as in Simpler, while the start and
the end go back to the whole file. A sample new to the song comes in with no
root note, whatever pitch the browser measured (D66); one the song already
lists keeps the root note it has. One undo step, "Load piano into the Sampler
on keys".

**Dropping a sample on a MIDI track's header** stays the shortcut it is: an
empty Sampler and a sample in one step. Since D85 a sample dropped under the
tracks in the headers' column, or added with + and no track selected, makes a
new MIDI track with a Sampler of it, and a sample the browser measured a pitch
of starts Held; see [one way a sample lands](sample-landing.md).

**The panel** is 448 points wide, in place of the 216-point list. At its left
the file's waveform, 204 by 92 points, with the part outside the start and
the end dimmed and a marker at each, with a handle at the top that points
inward. Under it the start and the end as seconds to type, and a line naming
the sample, its length and the note it plays as it is at. At its right, a
row for each control:

| Control | Field | Notes |
|---|---|---|
| Waveform markers; Start, End | `start_seconds`, `end_seconds` | Dragging a marker moves it to a thousandth of a second, no further than the file or the other marker, and sends the place when the drag ends. An end at the file's end, or an empty End, takes `end_seconds` off: the pad plays to the file's end. The host refuses a start typed past the end, and the panel shows the reason |
| Root, Measure | the sample's `root_note` | Typed as C4 or 60, kept as C4. Empty takes it off: the sample plays as it is at middle C. Measure runs `daw samples analyze` on the file off the main thread and sets the note it finds; a file with no one pitch, such as a drum, is refused with a message and nothing changes |
| Mode: One-shot or Held | `mode`: `one_shot` or `gate` | Held stops a note at its note-off, after the release |
| Transpose | `transpose` | Semitones, with hundredths as cents |
| Attack, Release | `attack_ms`, `release_ms` | |
| Level, Pan | `gain_db`, `pan` | |
| Reverse | `reverse` | |

The bars, the picker and the checkbox are the controls an effect's panel
uses (D44), drawn from `aaw_model::describe::PAD`: label, unit, range and
default from the model, a bar dragged or a value typed, Shift for finer
steps, a double-click for the default. Every pad field gives the sampler new
voices, so a bar sends its value when the drag ends and a playing song fades
through the change over 10 ms, as it does for a delay's time. The start and
the end take the file's length as their range. No pad field has an
automation lane, so no diamond is drawn.

**Playing it from a keyboard,** the computer's or a MIDI keyboard, is not part
of this; it is the MIDI keyboard line under Later. Notes are drawn in the piano
roll, as now.

**For the agent,** the arrangement `aaw-ffi` builds has `sampler` on each MIDI
track: the pad, its sample and file, the fields as `FieldView`s, the root
note, the file's identity and length, and the seconds played. Its file's
peaks are sent as an audio clip's are, so the waveform is drawn from the
same store. The edits are `SamplerLoad`, `SamplerSet` and `SamplerRoot`.

## Verification

An FFI test loads a Sampler made from the browser, reads its fields, sets
each kind of field, refuses a start past the end and a field the panel does
not set, types the root note as a number and a name and takes it off, loads
a second sample over the first and undoes it, and finds a kit of two pads is
not the device; it also waits for the file's peaks. A model test holds the
pad's described fields to validation and the defaults. Swift tests cover the
waveform's layout: seconds to points, which marker a point takes, and how far
a drag goes. Scripted runs of the app on a generated tone: the empty panel,
a `--drop` on the panel, both markers dragged, Measure clicked and the root
note set to A3 with `daw check` quiet, and a second tone dropped over the
first with the mode and attack kept. Nothing was dragged by hand or heard;
that is a line under Verify.

## Limits

- The Sampler plays on every key; there is no note range to set.
- Glide, a sustain loop, a filter and velocity to level are what Simpler has
  beyond this. Glide and sustain loops are under Later.
- The root note is the sample's, so setting it in one Sampler sets it for
  every pad that plays the same file (D70).
- A sample dropped on the panel from the browser is taken from what the
  browser remembers of the drag, as a drop on the headers is; a file from
  the Finder is read from the drag.
