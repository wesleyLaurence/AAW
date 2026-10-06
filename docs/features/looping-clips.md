# Clips that loop — implemented October 6, 2026

A note clip or an audio clip whose content repeats until the clip ends: a
two-bar drum phrase stretched to sixteen bars by setting its length, as an
Ableton clip with Loop on does. D63 left note clips without repeats until
audio clips could loop too, "so that one rule serves both"; this is that
rule, built as one pull request on October 6, 2026 (D83).

```sh
daw clip loop SONG tracks.drums.clips.clip1 8      # the first 8 beats repeat
daw clip resize SONG tracks.drums.clips.clip1 64   # now 8 times
daw clip loop SONG tracks.drums.clips.clip1 off
```

## Why

Most parts of a song repeat. Before this a phrase played again only as a
copy, one clip per repetition: many commands for the agent, and many clips to
change when the phrase changed, since a fix to the hi-hats in one copy was not
in the others. A looped clip is one object to change, and the arrangement map
shows it as one letter.

Pattern clips have `repeats` already, but they are the older kind of clip, and
note clips are where they are headed (D62).

## The song

**One field, `loop_beats`,** on a note clip and on an audio clip, null by
default. With it, the part of the clip from its start to `loop_beats` plays and
plays again, from the clip's start, until the clip's end. Without it, a clip
plays once, as before. The last repetition is cut off where the clip ends, as
a looped clip is in Ableton. A loop starts at its clip's start; there is no
loop start inside the clip.

```yaml
clips:
  - {id: hats, at: 0, length_beats: 64, loop_beats: 8, notes: [...]}
audio:
  - {sample: break, at: 0, source_start_seconds: 0.5, source_bpm: 96,
     loop_beats: 8, length_beats: 64}
```

**Note clips.** A note that starts at or after `loop_beats` is kept and does
not play, as a note past the clip's end is (D63); `daw check` says so under
`notes-outside-clip`, "at or after the loop's end", and `daw note list` marks
it `outside`. A note held across the loop's end is cut there. Notes are edited
once and heard in every repetition.

**Audio clips** gain `length_beats` as well, how long a looped clip lasts on
the timeline; an audio clip that does not loop has none, and ends where its
audio does, as before. The loop is the part of the file the clip plays for
`loop_beats` of song time from its start: with `source_bpm` it is that many
beats of the file at its own tempo and follows `session.tempo` (D53), and
without it is a fixed length in seconds. Each repetition is a copy of the clip
with its own `lead_ms`, `fade_in_ms` and `fade_out_ms`, and leaves at the wrap
from where the next starts, as two clips that meet do: the wrap is a
crossfade the clip's own fades set, 8 ms out and 0.3 ms in unless the clip
says otherwise, so the jump back does not click.

**Pattern clips** keep `repeats`. Converting a legacy track to a MIDI track (a
Later line) will turn `repeats` into `loop_beats`.

## Playback and render

The schedule unrolls a looped clip into its repetitions, so the engine plays
what it would play from copies: a looped note clip renders to the same samples
as the clip and its copies, and a looped audio clip to the same as copies of
its loop that meet. A locate into the middle of a loop picks up the
repetition it lands in. Python's timeline and joins see an audio loop as its
repetitions too.

## Commands

- **`daw clip loop SONG CLIP BEATS|off [--length BEATS]`** sets `loop_beats`
  on a note clip (`tracks.T.clips.ID`) or an audio clip (`tracks.T.audio.N`).
  An audio clip keeps the length it had, or takes `--length`; one that plays
  to its file's end must be given a length. `off` plays the clip once again,
  and takes an audio clip's `length_beats` away with its loop.
- **`daw clip resize SONG CLIP BEATS`** sets how long a note clip or a looped
  audio clip plays, so how many times its loop repeats. **`daw audio trim
  --end`** does the same for a looped audio clip; its start cannot be trimmed,
  since the loop starts there.
- **`daw clip trim`** on a note clip and **`daw clip duplicate`** work as
  before; a copy keeps the loop.
- **`daw audio split`** and the **`daw range`** verbs cut a looped clip only at
  a wrap, each half keeping the loop and, for a note clip, every note; inside
  a repetition they are refused, naming the clip, as a pattern clip is cut
  only between repeats. **`daw audio cut`** refuses a looped clip across the
  cut. After `range delete`, two halves of one loop that meet again become one
  clip when the left is a whole number of loops long.
- **`daw map`** shows a looped clip as one letter across its bars, the legend
  saying `loops every N beats` and how many times it plays.
- **`daw describe midi`** and **`daw describe edit`** say what `loop_beats`
  does, under `loops`.

## The app

The timeline draws a looped clip at its length with `↻` after its name and a
mark at each wrap; a note clip's notes are drawn again at each wrap, cut at
the loop's end, and an audio clip's waveform starts again from the loop's
start at each wrap, its beat-map ticks drawn across the first repetition.
The piano roll shades the clip from the loop's end, with a line and a mark
there, and draws the notes past it gray, as it draws notes outside the clip.
A Loop field beside the piano roll takes the loop in beats, or `off`, through
the FFI's `ClipLoop` edit. Dragging a looped note clip's end sets how many
times it plays; an audio clip's end does the same, and its start refuses.

## Tested

In Rust: a note clip of four beats looped to fourteen renders to the same
samples as the clip and three copies and a half, the held note cut at each
wrap and the note at the loop's end silent, and locating into the fourth
repetition plays the fourth; a looped audio clip's voices are those of copies
that meet, a render at the wrap moves by less than 0.05 a frame with the
clip's fades and jumps without them, and a loop of a file at another tempo
wraps on the song's beat; the commands, their refusals, the map's letter and
legend, `check`, `note list`, saving and reopening, the range verbs at a wrap
and inside one, and the rejoin after a delete, in the host; the FFI's edit.
In Python, `daw clip loop` on a phrase and an audio clip, `check`, `map`,
`describe` and the timeline's regions. Nothing heard.

## Limits

- A loop starts at its clip's start. Ableton's loop start inside the clip, and
  a clip cut inside a repetition by a range verb, are backlog lines.
- A loop of a part that is not a whole number of the file's beats drifts a
  little at each wrap; `daw check` does not measure it against the file's beat
  map. Cut loops on the file's beats, which `daw samples beats` gives, and give
  the clip `source_bpm`.
- A pattern clip with `repeats` is shown by the map and the app as it was,
  divided at each repeat; the two are not drawn alike.
