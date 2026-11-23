# Clips that loop — implemented October 6, 2026; cut anywhere November 23, 2026

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
a looped clip is in Ableton. A loop starts at its clip's start.

**`loop_offset_beats`,** 0 by default and less than `loop_beats`, is how far
into its loop a clip starts, as Ableton's start marker inside a loop: it plays
from there to the loop's end, then the whole loop from each wrap. It is what a
looped clip split inside a repetition gives its right half, and what a looped
clip trimmed at its start keeps, so the loop stays where it is in the song.

```yaml
clips:
  - {id: hats, at: 0, length_beats: 64, loop_beats: 8, notes: [...]}
  - {id: hats2, at: 70, length_beats: 10, loop_beats: 8, loop_offset_beats: 6, notes: [...]}
audio:
  - {sample: break, at: 0, source_start_seconds: 0.5, source_bpm: 96,
     loop_beats: 8, length_beats: 64}
```

**Note clips.** A note that starts at or after `loop_beats` is kept and does
not play, as a note past the clip's end is (D63); `daw check` says so under
`notes-outside-clip`, "at or after the loop's end", and `daw note list` marks
it `outside`. A note held across the loop's end is cut there. Notes are edited
once and heard in every repetition. In a clip that starts into its loop, a
note sounding at `loop_offset_beats` plays from the clip's start for the rest
of its length, as the right half of any split note clip holds the rest of a
note across the cut.

**Audio clips** gain `length_beats` as well, how long a looped clip lasts on
the timeline; an audio clip that does not loop has none, and ends where its
audio does, as before. The loop is the part of the file the clip plays for
`loop_beats` of song time from its start: with `source_bpm` it is that many
beats of the file at its own tempo and follows `session.tempo` (D53), and
without it is a fixed length in seconds. Each repetition is a copy of the clip
with its own `lead_ms`, `fade_in_ms` and `fade_out_ms`, and leaves at the wrap
from where the next starts, as two clips that meet do: the wrap is a
crossfade the clip's own fades set, 8 ms out and 0.3 ms in unless the clip
says otherwise, so the jump back does not click. In a clip that starts into
its loop, the first repetition plays the file from that far into the loop,
without a lead, since it starts inside the audio.

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
  --end`** does the same for a looped audio clip. **`daw clip trim --start`**
  and **`daw audio trim --start`** on a looped clip move its start and set
  `loop_offset_beats`, the notes and the loop staying where they are.
- **`daw clip loop`** with a new length keeps a clip's offset while it is
  inside the loop and drops it otherwise; `off` moves a note clip's notes, or
  an audio clip's start in its file, to where it started in its loop, so it
  plays from there once.
- **`daw clip trim`** on a note clip and **`daw clip duplicate`** work as
  before; a copy keeps the loop.
- **`daw clip split`** (⌘E in the app), **`daw audio split`** and the **`daw
  range`** verbs cut a looped clip at any beat, each half keeping the loop
  and, for a note clip, every note; the right half starts as far into the loop
  as the cut is, so the halves play what the clip played. **`daw audio cut`**
  refuses a looped clip across the cut. After `range delete`, two halves of
  one loop that meet again become one clip when the right goes on in the loop
  from where the left leaves it. `daw clip join` writes a looped clip's notes
  out as they played.
- **`daw map`** shows a looped clip as one letter across its bars, the legend
  saying `loops every N beats`, `from N beats in` for a clip that starts into
  its loop, and how many times it plays.
- **`daw describe midi`** and **`daw describe edit`** say what `loop_beats`
  does, under `loops`.

## The app

The timeline draws a looped clip at its length with `↻` after its name and a
mark at each wrap; a note clip's notes are drawn again at each wrap, cut at
the loop's end, and an audio clip's waveform starts again from the loop's
start at each wrap, its beat-map ticks drawn across the first repetition. A
clip that starts into its loop draws its first repetition from there, and its
first wrap where that loop ends. The piano roll shades the clip from the
loop's end, with a line and a mark there, and draws the notes past it gray,
as it draws notes outside the clip; a clip that starts into its loop has a
line where it starts, flagged at the foot, and the playhead goes round the
loop. A Loop field beside the piano roll takes the loop in beats, or `off`,
through the FFI's `ClipLoop` edit. Dragging a looped clip's end sets how many
times it plays, and dragging its start moves where it starts in its loop.
⌘E splits a looped clip at any beat.

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

- A loop starts at its clip's start: a loop of the middle of a clip's notes,
  Ableton's loop start, is not there.
- A split inside a note held across the cut strikes it again at the cut, as a
  split of any note clip does; a split of an audio clip inside a repetition
  is a crossfade there, the clip's own fades, where the audio was continuous.
- A looped audio clip's start is dragged in the app no earlier than its
  file's start, as an unlooped clip's is.
- A loop of a part that is not a whole number of the file's beats drifts a
  little at each wrap; `daw check` does not measure it against the file's beat
  map. Cut loops on the file's beats, which `daw samples beats` gives, and give
  the clip `source_bpm`.
- A pattern clip with `repeats` is shown by the map and the app as it was,
  divided at each repeat; the two are not drawn alike.

## Decisions

- **A cut inside a repetition, November 23, 2026,** from the person's report
  that ⌘E on a looped MIDI clip in a song was refused at beat 94, 14 beats
  into a 16-beat loop. Asked how a cut there should work, the person chose
  Ableton's way over two cheaper ones: the right half as the partial
  repetition plus a looped clip from the next wrap, three clips from one ⌘E;
  or the right half's notes rotated to start at the cut, which strikes a note
  held across the cut again at every later wrap. `loop_offset_beats` is the
  start marker inside a loop; a loop start of its own (a loop of the middle
  of the notes) is still left out until it is missed.
- **A note sounding at the offset plays from the clip's start,** in the
  first repetition only, so a split's halves play what the clip did and
  match a split of an unlooped clip, where the right half holds the rest of
  a note across the cut. Ableton does not chase a note there; here the halves
  are meant to sound as one clip.
- **The first repetition of an audio clip that starts into its loop has no
  lead,** since it starts inside the audio; the D175 rule for a split's right
  half. Its fades stay the clip's, so the cut is the wrap's crossfade where
  the audio was continuous; zeroing them would take the crossfade away from
  every wrap.
- **Trimming a looped clip's start sets the offset** rather than moving its
  notes or being refused: the loop stays where it is in the song, as dragging
  a looped clip's left edge in Ableton does.
- **`clip loop off` moves the content to where the clip started in its
  loop,** so it plays once from there; a note sounding at the offset, which
  started before it, is kept and does not play.
