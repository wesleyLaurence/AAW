# Clips that loop — proposed October 4, 2026

Status: proposed, not built. Backlog: Next, Clips that loop. D63 left note clips
without repeats until audio clips could loop too, "so that one rule serves
both"; this is that rule.

## What

A note clip or an audio clip whose content repeats until the clip ends: a
two-bar drum phrase stretched to sixteen bars by setting its length, as an
Ableton clip with Loop on does.

```sh
daw clip loop PROJECT tracks.drums.clips.clip1 8      # the first 8 beats repeat
daw clip resize PROJECT tracks.drums.clips.clip1 64   # now 8 times
daw clip loop PROJECT tracks.drums.clips.clip1 off
```

## Why

Most parts of a song repeat. Today a phrase plays again only as a copy, one clip
per repetition. That is many commands for the agent, and many clips to change
when the phrase changes: a fix to the hi-hats in one copy is not in the others.
A looped clip is one object to change, and the arrangement map shows it as one
letter.

Pattern clips have `repeats` already, but they are the older kind of clip, and
note clips are where they are headed (D62).

## Design

**One field, `loop_beats`,** on a note clip and on an audio clip, absent by
default. With it, the part of the clip from its start to `loop_beats` plays and
plays again, from the clip's start, until `length_beats`. Without it, a clip
plays once, as now. The last repetition is cut off where the clip ends, as a
looped clip is in Ableton.

**Note clips.** A note that starts at or after `loop_beats` is kept and does not
play, as a note past the clip's end is kept (D63); `daw check` and
`daw note list` say so. A note held across the loop's end is cut there. Notes are
edited once and heard in every repetition.

**Audio clips.** The loop is the part of the file the clip plays for
`loop_beats` of song time from its start, so it follows the tempo when the clip
is stretched (D53) and is a fixed length in seconds when it is not. Each wrap
gets a short crossfade so the jump back does not click. A loop whose length is
not a whole number of the file's beats, per its beat map, is reported by
`daw check`.

**Pattern clips** keep `repeats`. Converting a legacy track to a MIDI track (a
Later line) turns `repeats` into `loop_beats`.

**Playback and render.** The schedule unrolls a looped clip into its
repetitions, so the engine plays what it would play from copies. A locate into
the middle of a loop picks up the repetition it lands in.

**Commands and the app.** `daw clip loop CLIP BEATS|off` sets it. `clip resize`
and `clip trim` change how many times it plays. The piano roll marks the loop's
end, and the timeline draws repetitions with a mark at each wrap; that is a
second stage, after the model, engine and commands.

## Done when

- A note clip of eight beats looped to sixty-four renders to the same samples as
  the clip and seven copies.
- A note past `loop_beats` is silent and reported. A held note is cut at the
  wrap. Locating into the fourth repetition plays the fourth.
- A looped audio clip at the song's tempo renders without a click at the wrap,
  measured from the stem, and a stretched one from a file at another tempo
  wraps on the song's beat.
- `daw describe midi` and `daw describe edit` say what `loop_beats` does.

## Open questions

- The crossfade length at an audio loop's wrap, and whether the clip's own fade
  fields should set it.
- Whether a loop should be able to start later than the clip's start (Ableton's
  loop start), or only from it. Only from it is simpler and covers the common case.
- Whether a pattern clip with `repeats` should be shown by the map and the app
  the same way as a looped clip.
