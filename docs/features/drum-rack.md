# The Drum Rack, and patterns gone

Status: stage 1, the Drum Rack, implemented November 17, 2026 (D189), in [#187](https://github.com/wesleyLaurence/AAW/pull/187).
Stage 2, patterns converted, implemented November 18, 2026 (D190), in [#188](https://github.com/wesleyLaurence/AAW/pull/188).
Stage 3 is proposed, Next item 1.

## What

A drum kit on a MIDI track that the person sees and works with in the app, as
Ableton's Drum Rack is used, and patterns gone from everything:

1. **The Drum Rack.** An instrument in the browser's Instruments beside the
   Sampler and the Synth. Its panel is a grid of 4 × 4 pads, one a key, with a
   small overview of all 128 keys at its left that picks which 16 show; a list
   of its pads beside the grid, each with its volume, pan, mute and solo; and
   the selected pad's sample beside that, as the Sampler shows its one pad. A
   sample dropped on a pad is played by that key. The piano roll of a Drum
   Rack's clip lists only the keys that have a pad, each row named for its pad.
2. **Patterns converted.** A song that has patterns, opened by the app or by
   any `daw` command, is converted once: each track of pads becomes a MIDI
   track whose Drum Rack holds the same pads, and each pattern clip becomes a
   note clip that plays the same hits, with the sound unchanged.
3. **Patterns removed.** `patterns`, pads on a track and pattern clips are out
   of the song, the engine and the host; `daw pattern`, `daw event` and a
   track's `daw pad add` are gone; the app's pattern editor is gone; `daw
   track add` makes a MIDI track unless told otherwise; and `daw describe`
   teaches only the Drum Rack.

## Why

On November 17, 2026 an agent session in the app built a beat's drums as a
track of pads played by patterns, although step 1 of retiring patterns (D184)
had taken them out of the start guide. It wanted one kit on one track, with the
open hat choked by the closed one, and the guide showed a track a drum; the
describe topics that still described pads on a track and patterns led it
there. The person then found nothing in the app to work with that kit: the app
makes no pads (D85) and a Sampler of several pads is listed, not edited. A
Sampler of several pads, as `daw instrument map` makes, already plays a kit
on note clips, but the app has no face for it. The person asked for Ableton's
Drum Rack, from four pictures of it, and for no patterns anywhere again.

## Design

### The model

A Drum Rack is the `sampler` instrument the song already holds, with a flag:
`{sampler: {drum_rack: true, pads, map}}`. The engine plays it as any
sampler; the flag only says which face the app gives it, so that an empty
Drum Rack is not an empty Sampler. A pad of a Drum Rack is one map entry of
one note, not pitched: `{notes: 36, pad: kick}`. A sampler without the flag
that is neither empty nor one pad on every note, such as a kit `daw instrument
map` or `daw transcribe --drums` made before this, is drawn as a Drum Rack
too; the flag is written when the Drum Rack next takes a sample on a key, and
`daw pad add` on a track with no instrument writes it from the start. A map entry
of a range shows its pad on each key of the range.

Each pad gains `mute` and `solo`, both false unless given. A muted pad's
notes play nothing. When any pad of the sampler is soloed, only soloed pads
play. Both are pads' own, apart from the track's mute and solo, and `daw pad
set --mute/--solo` sets them as it sets the rest.

### The panel

From the left, as in Ableton:

- **The overview,** 4 keys wide and 32 rows tall, one cell a key from 0 at the
  bottom to 127 at the top; a cell with a pad is filled. A frame marks the 16
  keys the grid shows. A click or a drag in it moves the frame a row of 4 at a
  time. The grid opens on keys 36–51 (C2–D♯3, kick at the bottom left, as
  `daw describe midi.drums` maps them), or on the row of the lowest pad when
  none is there.
- **The grid,** 4 × 4, the lowest key at the bottom left. An empty pad shows
  its key's name, as the song names notes (C4 is middle C, so the kick on 36 is
  C2 where Ableton says C1). A pad with a sample shows the pad's name and, along
  its foot, M, ▶ and S: mute, play it at velocity 100, solo. A click selects
  it. A sample
  dropped on an empty pad, from the browser or the Finder, adds a pad named
  for the sample on that key; dropped on a pad with a sample, it takes that
  one's place and keeps the pad's settings, as the Sampler does. Delete, or
  Remove in the pad's right-click menu, takes the pad off; its notes are kept
  and play nothing, and `daw check` names them.
- **The list,** one row a pad by key, low to high: its name, which selects it,
  ▶, its volume in dB, its pan, mute and solo. The selected pad's row is lit.
- **The pad,** the Sampler panel for the selected pad: its waveform with the
  start and the end, the root note, mode, transpose, attack, release, gain,
  pan and reverse as the Sampler shows them, and its choke group, mono, source
  tempo and stretch, which no panel shows today.

Each control is a field the song has, set through the same edits the Sampler
uses with the pad named, so the agent reads and changes what the person sees.

### The piano roll

The piano roll of a note clip on a Drum Rack is folded: only the keys that
have a pad are rows, from the highest at the top, each named for its pad
beside a key that plays it. A Fold button in its header unfolds it to all 128
keys, as Ableton's Fold does, and folds it back. A note on a key with no pad
unfolds nothing: it is drawn in a row of its own named for its key, so a note
is never hidden.

### The agent's side

`daw instrument set TRACK '{"sampler": {"drum_rack": true}}'` makes an empty
one, and the browser's Drum Rack does the same; `daw instrument map` gives a
pad its key; `daw pad set` sets mute and solo. `daw describe start` builds the
drums as one MIDI track with a Drum Rack of kick, snare, closed and open hat
choked together, played by a looped note clip, in place of a track a drum, and
`daw describe sampler.drum_rack` says what the panel shows. The topics that
describe pads on a track and patterns say they are only the format of older
songs until stage 3 removes them.

### Stage 2: patterns converted (implemented November 18, 2026)

A song with a track of pads or pattern clips is converted the first time it
is read from its file: by `aaw_model::load`, which every `daw` command, the
Python package and a render read through, and by the host's session when it
opens a song or loads an edit made outside it. `aaw_model::convert` writes
the file as it was beside the song as `song.before-drum-rack.yaml`, numbered
when that name is taken, and the converted song over it; read again, nothing
changes. Text given to `aaw_model::parse` is not converted, so the model's
corpus still reads what it is given, and a render's `song.snapshot.yaml` is
read as it was written, so an old render of a song of patterns still
verifies against its report, and a pattern clip an edit adds inside a
running session stays one until the song is next read.

Each such track becomes a MIDI track of the same ID, place, mixer, chain,
sends, lanes and group, whose instrument is a sampler with `drum_rack: true`
holding its pads; their own mute and solo are cleared, since a track's pads
never had them. Keys are given in this order:

1. A pad played with notes keeps a pitched range of the notes it played, from
   its lowest to its highest, at those notes when they are free and otherwise
   moved to the nearest free range, the pad copied and moved back to match.
   When such ranges would collide or leave too few keys for the drums, as a
   pad played at C-1 and at G9 would take every key, each pad keeps instead
   each run of the notes it plays.
2. A pad whose name says a General MIDI drum takes that drum's key when it is
   free: kick C2, rim C♯2, snare D2, clap D♯2, closed hat F♯2, pedal hat
   G♯2, open hat A♯2, the toms, crash C♯3, ride D♯3, tambourine, cowbell and
   shaker.
3. The rest take the lowest free keys from C2 up, then below it, in the
   track's order: pads as they are first, then copies.

A hit its event transposes plays a copy of its pad moved as far, named for it
(`kick-2`, `hat+0.5`) on a key of its own, and a pitched hit's fraction of a
semitone the same way, so every hit sounds as it did; a pad played by
nothing gets a key so that it shows in the rack.

Each pattern clip becomes a note clip named after its pattern (`hook`,
`hook-2`), at its place, active or not as it was, holding the pattern's hits
as notes: steps and events alike, at their times with swing written in. A
time a pattern worked out, as a swung step's, is written as the plainest
fraction within a billionth of a beat that starts on the same frame in every
repetition, so a step swung by 0.6 is at 13/10. A clip that repeats loops
over the pattern's length, unless a gated hit lasts past the pattern's end,
which a loop would cut, and then its repetitions are written out and the clip
lasts to that hit's end. A hit with no length is a step long, within the
pattern; only a gated pad's lengths are heard. A clip's `velocity_scale` is
rounded into its notes' velocities, so 42 scaled by 0.9 is 38 rather than
37.8, at most 0.05 dB from what it was. Two hits of one pad at once in a
choke group are played in order of their lengths, so the one written last,
which choked the others, is the longest.

A MIDI track holds no audio clips, so a track of pads that also had audio
clips or an input gives them to a track `ID-audio` right after it with the
same mixer, chain, sends, lanes and group; a frozen one's `ID-audio` is muted,
since the frozen file holds both. The patterns go.

Tests render each case as written and converted and compare them sample for
sample, the last bits of a float apart, and the clip's velocity scale within
its rounding; the person's one song with patterns, made in the app on
November 17, renders its drums the same to the bit.

Limits that are simply true: two tracks summed through one chain sound as one
track did only through a chain that is linear, so the `ID-audio` split is not
exact through a compressor, limiter, gate, clipper or overdrive, and a
sidechain keyed by the track no longer hears its audio clips; hits of one pad
at once from two overlapping pattern clips, or of a pitched pad at two
pitches, in a choke group may choke in another order. None of the person's
songs has any of these.

### Stage 3: patterns removed

Everything in stage 3 of D184's list, once stage 2 has converted every song
that is opened: the model refuses `patterns`, track pads and pattern clips
after converting them, the engine and the host lose their code, the app its
pattern editor and its step grid, the CLI its verbs and topics.

## Done when

1. A Drum Rack dragged onto a MIDI track shows the empty grid; a sample from
   the browser dropped on C2 plays on C2; ▶ plays it; M, S, the volume and pan
   change it as heard; its sample's settings change in the pad panel; the
   overview moves the grid; the piano roll of its clip shows only its pads,
   named, and Fold shows all keys.
2. The kit the agent made on November 17, opened in the app, is a Drum Rack
   with its pads on their keys, playing as before.
3. No `daw` command, describe topic, batch op or app view makes or shows a
   pattern, and every song in `projects/` opens and renders as before.

## Open questions

- Each pad as a chain of its own effects, as in Ableton, and a pad's own
  filter, LFO and velocity to volume, as Simpler has: later lines, not this
  item.
- Dragging a pad onto another to swap their keys, and renaming a pad in the
  grid: later lines.
- A key played on the typing keyboard or a MIDI keyboard selecting its pad,
  as Ableton's does: not built.
