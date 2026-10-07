# One way a sample lands — implemented October 7, 2026

Where a sample goes when it is dropped in the Mac app, or added with + in the
browser. The choice is D85 in [decisions.md](../decisions.md); the audio clip
it makes is in [audio-clips-in-app.md](audio-clips-in-app.md) and the Sampler
in [sampler-device.md](sampler-device.md).

## What

A sample dropped in the window becomes one of two things, and both are seen:
an audio clip on the timeline, or a Sampler on a MIDI track that plays it on
the keys, shown in the detail panel as it lands. Nothing a drop makes is a pad
of a pattern track any more.

| Dropped on | Makes |
|---|---|
| A track's lane | An audio clip on that track at the grid line nearest the pointer; with ⌘, off the grid |
| A MIDI track's lane | An audio clip on a new track, since a MIDI track holds note clips only |
| The timeline under the tracks | A new track named after the sample, with the clip at that beat |
| A MIDI track's header | A Sampler of the sample as the track's instrument, in place of the one it had |
| Another track's header | An audio clip on that track at the start position, where ⌘E splits and Paste lands |
| The headers' column under the tracks | A new MIDI track named after the sample, with a Sampler of it |

While the file is dragged, the clip it would make is outlined on the lane at
its length, and under the tracks in the headers' column the header the new
MIDI track would have is drawn, with the sample's name and the word Sampler,
so that it is told from the line across the timeline that a new audio track
is. A MIDI track's header lights up for its Sampler.

**+ and a double-click in the browser** put the sample where a drop on the
selected row would: into a Sampler on the selected MIDI track, onto any other
selected track as an audio clip at the start position, and with no track
selected on a new MIDI track with a Sampler of it. The hint under the samples
says which.

**Held or to the end.** A Sampler a drop makes plays its sample at every
note's pitch, as it is at middle C (D66). A sample the browser measured a
pitch of, shown as a note beside it in the list, starts Held: a note stops at
its note-off, so a bass line's notes end where the next begins. A hit, a loop,
an effect or a file from the Finder, whose pitch is not known, plays to its
end on every note, as before. Mode in the Sampler's panel switches either. The
measured pitch is not written as the root note (D66).

## Why

The person's note of October 6, 2026: a sample dragged onto the timeline made
an audio track when the outline was a line across, and a pad of a pattern
track when it was a shorter line, and the two were hard to tell apart. A pad
shows nothing on the timeline, so the sample seemed to have gone nowhere. Now
whichever line the drop lands on, something is seen: a clip on the lane, or a
Sampler in the panel on a track named after the sample, and the outline under
the tracks names what it makes.

## How it works

The arrangement view works out a drop's target from where the pointer is
(`landing`): the headers' column makes a Sampler or, on a track that is not
a MIDI track, a clip at the transport's cue; the timeline makes a clip. The
browser's + asks the model (`addSample`), which picks by the selected row.

The edits are `aaw-ffi`'s. `SamplerAdd` lists the sample and sets the
instrument of a MIDI track, or adds a MIDI track with it, as one batch: the
instrument is one pad named after the sample and a map entry
`{notes: [0, 127], pad, pitched: true}`, the pad in `gate` mode when the asset
came with a root note, which the browser gives a sample it measured as one
pitch and not a drum, an effect or a loop. `SampleClip` makes the audio clip.
The edit that made a pad, `SampleAdd`, is gone; a track that is not a MIDI
track refuses `SamplerAdd` with the reason. Pattern tracks and their pads stay
in the song and are made with `daw track add` and `daw pad add`.

## Verification

Rust tests add a sample to a MIDI track and to a new one and check the label,
the pad, its mode by whether the asset had a pitch, the map, the undo step and
the refusal of a track that is not a MIDI track. Scripted runs of the app
dropped a WAV in the headers' column under the tracks, on a pattern track's
header and on a MIDI track's header on a generated song, read what the song
held afterwards and took a picture of each. Nothing was dragged by hand or
heard, and the outline under the tracks was not seen: a scripted drop hands
the file to where a drag would end and draws no drag.

## Limits

- A drop on a MIDI track's header makes a fresh Sampler, as before. A drop on
  the Sampler in the device panel keeps the pad's mode, level and the rest.
- Whether a sample starts Held follows the browser's measured pitch, which a
  file from the Finder never has, so a bass note from the Finder plays to its
  end until its mode is switched.
- A pad is still what `daw pad add` makes, and a pattern track's pads are
  still listed, not edited, in the detail panel; see the backlog.
