# Beat map — implemented October 1, 2026

`daw samples beats` measures where the beats of a whole song are: its tempo, the
time of every beat, which beats start a bar, and where the arrangement changes.
It answers the question an edit starts with, "which strong beat is nearest 0:41?".
The results are estimates from decoded audio, to check before cutting. All
computation is local and uses numpy; no model or network call is made, and nothing
here listens.

## Commands

```sh
uv run daw samples beats projects/edit/samples/HASH_song.wav
uv run daw samples beats FILE --near 0:41              # the beats around a time
uv run daw samples beats FILE --near 1:20.5 --window 5 # five seconds either side
uv run daw samples beats FILE --all                    # every beat
uv run daw samples beats FILE --click /tmp/grid.wav --near 0:41
uv run daw samples beats FILE --bpm 80                 # settle half or double time
uv run daw samples beats FILE --downbeat 0:12.4        # the beat nearest this is a one
uv run daw samples beats FILE --meter 3/4              # three of its beats to a bar
uv run daw samples beats FILE --refresh                # measure again, no corrections
```

`FILE` is a path or a library index ID. A compressed song is imported first
(`daw samples import`), and the decoded copy in the project is what is measured.
Times are seconds, or minutes and seconds as `m:ss` or `m:ss.mmm`.

The map is kept beside the audio as `NAME.beats.json` and used again while the
audio and the measuring code are unchanged, so the engine, checks and the app can
read one grid. Nothing is written beside a file in the library index; its map is
measured on each call. `--bpm`, `--downbeat` and `--meter` are corrections: they are saved
with the map (`requested`) and apply to later calls until `--refresh`.

## Report

The command prints a summary. Every beat is in the saved map, and `--all` prints
them.

- `tempo`:
  - `bpm` is the tempo of the grid, or the mean tempo of a song that leaves it.
  - `steady` says the song keeps to one grid. Its beats are then the grid's, which
    is placed more finely than any one beat can be measured.
  - `drift_ms` is how far runs of beats stray from the grid. In a steady song it
    is the difference between instruments' attacks, a few milliseconds. In a song
    that is not steady the beats are followed one by one, and `bpm_range` gives
    the slowest and fastest tempo over about four bars.
  - `ambiguous_with` lists half or double the tempo where that fits the same
    onsets. See [Half and double time](#half-and-double-time).
  - `transients_fitted`, `fit_spread_ms` and `transients_on_grid` say how many
    transients the grid was fitted to, how tightly they sit on it, and what share
    of the beats' sharp transients are within 10 ms of it.
- `first_beat_seconds` and `first_downbeat_seconds`.
- `meter` and `beats_per_bar`: the time signature given with `--meter`, `4/4`
  unless given, and how many of the map's beats make a bar, its numerator.
- `downbeat`: which beat of the bar starts it is the least certain part of a
  beat map. `candidates` lists every place with a `confidence` each, summing
  to one, and which is `chosen`. When the best two are close, ask, or play the
  person a `--click` audition. `set_by` is `measurement` or `request`.
- `phrases`: downbeats where the arrangement changes, each with its `bar`,
  `seconds` and `change` from 0 to 1 against the song's largest change. The first
  downbeat is always listed. They are hints: a change is sometimes placed a bar
  from where a person would put it, and a quiet change is missed.
- `beats` (with `--all`, and in the saved map): `seconds`, `bar`, `beat` and
  `strength` for each. Bars are counted from the first downbeat, from 1; beats
  before it are a pickup in bar 0. `strength` is the onset on the beat against the
  song's strong ones, from 0 to 1. A downbeat also has its `change`, and
  `phrase_start` where it starts a phrase.
- `near` (with `--near`): the beats within `--window` seconds of the time, each
  with its `offset_seconds` from it, and the `nearest_beat`, `nearest_downbeat`
  and `nearest_phrase_start` wherever they are.
- `click` (with `--click`): a WAV of part of the song at a lower level with a
  click on each beat and a higher, louder one on each downbeat. It covers
  `--seconds` (20 by default) around the `--near` time, or from just before the
  first downbeat. It is for a person to hear whether the grid is right.
- `map`, `cached`, `sha256`, `analyzer` and `requested` describe the saved map.

## How it is measured

1. **Onsets.** Spectral flux of the whole file in about 32 bands, a frame every
   256 samples, and the flux of the bands up to 160 Hz again, so a kick or a bass
   note counts for more than a hat.
2. **Tempo.** The autocorrelation of that envelope gives tempos between 60 and 200
   BPM, with half and double time where they are in range. For each, the beat's
   period is placed finely from the envelope's spectrum, and then as the period
   under which the beats of the whole file add up best.
3. **The grid.** The first beat is where the beats add up to the most. Near each
   grid beat the sharpest rise in level within 40 ms is taken as its transient, and
   a line is fitted through those by beat number, leaving out the ones that
   disagree. That fit sets the tempo and the first beat.
4. **Steady or not.** Windows of eight beats are compared with the grid. A song is
   steady when four fifths of it, by onset strength, stays within three frames.
   Otherwise the path through the windows' offsets that changes least is followed,
   and each beat takes its own transient where it has one that agrees with its
   neighbors.
5. **Downbeats.** Five things that tend to happen on a downbeat are scored for
   each of the four places: a low onset, a change of harmony over two beats and
   over four, and a change of sound over four beats and over sixteen.
6. **Phrases.** At each downbeat the level in eight wide bands after it is
   compared with before, over four bars, two and one.

## Half and double time

Half and double time fit the same onsets, and audio alone does not say which a
song is in. The faster tempo is taken, up to 180 BPM, when at least 70% of its
beats' sharp transients are on the grid. Its beats include the slower one's,
where the slower one's beats can land on the backbeat. At twice a song's real
tempo every other beat falls between the beats, where what sounds is seldom on
the grid, and the slower tempo is taken.

A song with even eighth notes at 80 BPM is therefore reported at 160, with 80 in
`ambiguous_with`. Its downbeats are then every half bar, which are still strong
beats to cut on. `--bpm` settles it.

## Limits

- A bar is four of the map's beats unless `--meter` says otherwise, and the meter
  is a count, not a measurement: the map cannot tell three from four on its own.
  Its beat is the pulse it finds, so a 6/8 song whose pulse is found as the
  dotted quarter is counted with `--meter 2/4`.
- One tempo, or a tempo that wanders slowly, is followed. A song that changes
  tempo, or one cut off its own grid by an earlier edit, is not.
- The grid is placed against the transients it is fitted to. Instruments differ by
  a few milliseconds in where their attacks sit, so the grid of a mastered song is
  a few milliseconds from where another instrument would put it. What matters at
  a join is that both sides are measured the same way.
- Downbeats and phrases are scored from what usually happens there. Music that
  avoids those habits gets a confident wrong answer less often than a low
  confidence, but it can.

## What was checked

On generated songs (a kick, a snare, hats, a bass line that changes each bar, and a
layer added every eight bars) at 93.7, 120 and 174 BPM, starting anywhere in the
file and with a pickup: every beat within 0.1 ms, the tempo within 0.002 BPM, the
downbeat right, and the phrases at bars 9 and 17 found. With the tempo wandering a
quarter of a second early and late: every beat within 2 ms. After AAC at 192 kb/s
and decoding on import: within 1 ms.

On five of the author's own songs rendered by this engine, whose grids are known
from their projects: the tempo within 0.002 BPM of the project's on four, and at
double it on the fifth (80 BPM with even eighths); every beat within 0.4 ms of the
grid on four and within 8 ms on one whose samples have slow attacks; the downbeat
on a bar line in four, and on the half bar in the one at double tempo. A
three-minute song takes about a second and a half.

No commercially released song was measured, and nobody listened to a click
audition. The thresholds above were chosen on that material.
