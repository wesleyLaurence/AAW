# Audio clips in the app — implemented October 2, 2026

The Mac app's part of audio clips. What a clip is in the song, and the `daw
audio` commands, are in [audio-clips.md](audio-clips.md); the choices here are
D59 in [decisions.md](../decisions.md).

## What

A track's audio clips are on its lane with the waveform of their file. A clip is
dragged to move it, its edges are dragged to trim it, and a handle at each top
corner sets its fade. An audio file dropped on the timeline lands as an audio
clip at that beat. ⌘E splits a clip at the start position, and the detail panel
shows a clip's level, fades, tempo and stretch. Where the file has a beat map,
its beats are marked under the waveform. A shaped automation segment is drawn as
the curve it plays.

## Why

On 2026-10-02 the person made a track, dropped a WAV on it, and expected the
file on the timeline to trim and move. It became a pad, as built, and nothing on
the timeline changed. A person used to other DAWs meets the app through the
window, and there a dropped file is a region.

## What a drop makes

| Dropped on | Makes |
|---|---|
| A track's lane | An audio clip on that track, at the grid line nearest the pointer; with ⌘ (Option until D79), off the grid |
| The timeline under the tracks | A new track named after the file, with the clip at that beat |
| A track's header | A pad of that track |
| The headers' column under the tracks | A new track with a pad |

The + by a sample in the browser adds a pad. While a file is dragged over the
timeline, the clip it would make is outlined at its length; a file whose length
cannot be read is outlined a bar long. The clip plays the whole file at the
file's own tempo, and is selected when it lands. An `.m4a` or `.mp3` is decoded
into the project first, as for a pad.

## The song grows

A blank project is 32 bars. When a drop, a move, a trim, a copy or a new tempo of
its own would end an audio clip's sound past the song's end, the same undo step
lengthens the song to the end of that bar. A sound that runs past the end by
less than the song's end fade (20 ms unless set) does not: that fade takes it,
as it takes the short fade out after a clip an agent ended on the last beat.

`daw` commands leave the length alone, as before: an agent sets it. Pattern
clips still stop at the song's end.

## A clip on the lane

A clip is a block in its track's color: a title strip with its sample's name,
and under it its file's waveform from where the clip starts in the file, scaled
by the clip's gain. The waveform is the file's, not what the track plays, so it
is there while an edge is dragged outward and does not wait for the song to be
compiled again.

The song writes a fade out after the place a clip leaves (`fade_out_ms`
"follows where the clip leaves"). The app draws a clip to where its sound ends,
with the fade out inside it, as other DAWs do: the block is the clip and its
fade's tail, as far as the file goes. The edits below keep it that way.

| Input | Does |
|---|---|
| Click, Shift-click, ⌘A, Escape | Selects, as pattern clips |
| Drag the body | Moves the selected clips by grid steps and to other tracks; with ⌘ (Option until D79), off the grid. Pattern clips and audio clips move together |
| Drag an edge | Trims: the audio stays where it is and the clip shows more or less of it, no further than the file goes. The end is where the sound ends, so the clip leaves its fade out's length before that; a fade out that no longer fits is shortened to half the clip |
| Drag a corner's handle | Sets the fade in, or the fade out: the sound still ends where it did, and a longer fade out starts earlier. A fade goes no further than the other one |
| Arrow keys, ⌘D, Delete | Nudge, duplicate and remove, as pattern clips |
| ⌘E, Edit › Split | Splits the selected audio clips at the start position, or with a track selected and no clip, that track's. The later halves are selected, so Delete takes what follows the split |

The handles show on a clip that is selected or under the pointer. What a fade
takes away is shaded, under the curve it plays: equal power or linear.

A move, a trim and a fade are each sent when the drag ends, and each is one undo
step. A trim and a fade out give the engine another part of the file to prepare,
which is why they are not sent as the pointer moves.

## The beat map

`daw samples beats` keeps a file's map beside it as `NAME.beats.json`. When that
file is there, and is of the audio the song lists for the sample (its `sha256`),
each beat inside a clip is a tick at the foot of its waveform and each downbeat
a taller one. Beats are drawn once they are five points apart; closer than that
only downbeats are, and closer still nothing is.

The app measures nothing, and reads the map when the song next changes. The
ticks are the file's beats, so in a song at another tempo they leave the grid:
the detail panel says so, and what to do.

## The detail panel

Selecting an audio clip shows it where a pattern clip shows its pattern; the
tab is named Audio Clip then.

| Field | Sets |
|---|---|
| Gain | `gain_db`, from −36 to +24 dB; the waveform follows |
| Fade in, Fade out | The fades in milliseconds, as the handles do |
| Curve | `fade_curve`: equal power or linear |
| Tempo | `source_bpm`: the file's tempo, with which the clip follows the song's; empty, it plays as it is |
| Stretch | `stretch`, once the clip has a tempo: repitch, or keep pitch |

Beside them, in words: the part of the file the clip plays, the file's length,
and the beat map's tempo when there is one.

## How it is built

- **`TrackView.audio`** lists a track's audio clips for the app: key, sample,
  beat, `length_beats` to where the clip leaves and `tail_beats` of fade out
  after that, the seconds of its file it plays, seconds of file to a beat, gain,
  fades, tempo, stretch and the identity of its file.
- **`Arrangement.files`** lists the files those clips play (`files.rs`): each
  one's identity, length and beat map. A file's header is read once and kept
  while its size and date stay the same.
- **`Waveforms`** carries file peaks beside track peaks: `aaw_engine::peaks::
  file_peaks` reads a file in blocks. A track of audio clips alone has no track
  peaks worked out.
- **`Edit`** has `AudioTrim`, `AudioFade`, `AudioSplit`, `AudioSet` and
  `SampleClip`; `ClipsMove`, `ClipsDuplicate` and `ClipsRemove` take audio clips
  beside pattern clips. The song's length is set in the same batch when a clip
  needs it.
- **`audio.move`** is the host's command for a clip moved to another beat or
  track with its handle kept, and `daw audio move` sends it.
- **`PointView.shape`** is a point's shape, and `shapedProgress` in the app is
  the engine's formula for the segment after it.
- **`AudioClipLayout`** in the app is the clip's geometry: its parts under the
  pointer, and how far an edge or a fade can go.
- **`--drop FILE,X,Y`** among the app's scripted input lands a file at a point as
  a drag that ended there would, since a drag cannot be scripted.

## Limits

- Two clips that overlap on a track both play; nothing crossfades them. `daw
  audio crossfade` sets the fades of a join.
- A clip's lead (`lead_ms`) is not drawn or dragged; a clip with one starts that
  much before its block.
- The window does not zoom out when the song grows; ⌘0 fits it.
- A beat map written while the song is open shows after the next change.
- A trim of a long file at another sample rate than the song's takes the engine
  a moment, since the part is converted again.

## Tested

In Rust: the clip and file views, the beat map and when it is taken, file peaks
sent once, each edit with its limits, labels and undo, and the song's length
after each. In Swift: the clip's parts, how far edges and fades go, the fade and
automation curves, and a file's waveform at a clip's level. In the app, by
scripted input on generated audio: a WAV and an `.m4a` dropped on a blank
project, a trim, a fade, a move past the song's end, a split, and a lane with
shaped segments, each checked in a picture and in the song file. Nobody has
dragged a file from the Finder or listened to a result.
