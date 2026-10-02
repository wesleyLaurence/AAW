# Join checks — implemented October 1, 2026

`daw joins` checks a rendered edit of a song: every join between two parts of the
song, and the length of the file. It measures; whether a join sounds natural is
for a person to hear, and each join gets a short excerpt for that.

## Commands

```sh
uv run daw render projects/edit/song.yaml
uv run daw joins projects/edit/renders/latest.json
uv run daw joins projects/edit/renders/latest.json --limit 60     # the file must fit a minute
uv run daw joins RENDER --seconds 6                               # longer excerpts
uv run daw joins RENDER --no-excerpts
```

`RENDER` is a full render: its folder, its `report.json` or the `latest.json`
pointer. A track or section preview is refused, since the checks read the track's
stem and the mix on the song's own timeline.

## What a join is

A join is where a track goes from one part of a sample file to another part of
the same file: two audio clips in a row on a track, or two hits of different pads,
that play the same sample, each for a beat or more, the second starting as the
first ends, within their fades. A drum hit after another is not a join, and
neither is a loop played again from the same pad.

## Report

- `length`: the file's `seconds`, with everything in it; where its sound ends
  (60 dB under its peak); and, with `--limit`, whether it is within the limit and
  by how much it is over.
- `joins`: one entry each, in time order, and `flagged`, the numbers of those with
  anything to look at.

Each join has:

- `at_seconds` and `at_beats` on the timeline, the `parts` either side (`audio.N`
  for an audio clip, or the pad's name), and
  `source_seconds`: where the song is left and where it is entered.
- `fade`: its length, the fade out and fade in, and how far the two parts overlap.
- `grid`, from the song's beat map (`daw samples beats`, kept beside the sample):
  - the last beat of the song before the join and the first after it
    (`leaves_at`, `enters_at`, with their bar and beat in the song);
  - `interval_error_ms`: the time between them on the timeline against a whole
    number of session beats. This is whether the beat slips;
  - `before_ms` and `after_ms`: how far each sits from a session beat;
  - `source_beats_skipped`: how many beats of the song the join removes;
  - `enters_after_fade_in_ms`: how long the fade in has been over when the first
    beat arrives. Negative means the beat starts inside the fade.
  It is `null` when the sample has no beat map, and a flag says to make one.
- `measured`, from the track's stem: the sharpest rise within 40 ms of up to eight
  beats either side, as the median distance from session beats before and after,
  and their difference. It corroborates `grid` from the audio itself. Attacks
  differ by instrument, so a few milliseconds here can be a change of sound.
- `step`: the sample-to-sample change of the stem where the entering part starts
  and where the leaving part stops, against the largest within 3 ms either side.
  A fade brings a part in from nothing, so its edge adds no step; an edge without
  one jumps by whatever the audio is there.
- `level`: RMS of the stem over up to a second before the fade and after it.
- `excerpt`: a WAV of the mix around the join, four seconds unless `--seconds`
  says otherwise, in the render's `joins/` folder.
- `flags`: what to look at, in words.

## Flags

| Flag | When |
|---|---|
| The beat slips | `grid.interval_error_ms` is over 1 ms |
| Beats skipped are not whole bars | `source_beats_skipped` is not a multiple of four |
| The first beat starts inside the fade in | `enters_after_fade_in_ms` is negative |
| Transients sit differently after the join | `measured.interval_error_ms` is over 3 ms |
| A possible click | `step.ratio` is over 2 |
| The level changes | by more than 3 dB |
| No beat map | the sample has none beside it |

A join with no flags has passed these measurements. It has not been heard.

## Limits

- The grid check is as good as the beat map. A wrong downbeat in the map does not
  change `interval_error_ms`, but the bar and beat numbers are then wrong.
- The step check finds a discontinuity. It does not find a join that is smooth and
  still sounds wrong: a chord cut off, a vocal phrase split, a reverb tail lost.
- A sample whose pads are transposed by an event is placed without that
  transposition.

Tests build an edit of a generated song (keep sixteen beats, skip eight, keep the
rest) and check a join on the beat, one 7 ms late, one that skips six beats, a
splice through a note without fades, and a sample without a beat map.
