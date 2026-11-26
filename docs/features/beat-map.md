# Beat map — implemented October 1, 2026

Since November 16, 2026, the downbeat model leads the map where `daw separate
setup` has installed it: it hears the beats and the bars, bars can differ in
length, and `--downbeat` holds from each time given to the next. See
[downbeat-model.md](downbeat-model.md); what follows is the onset measurement,
which places the beats and measures the map without the model.

Since November 26, 2026, a song whose tempo steps is measured in segments, each
with its own tempo, and the beats and bars are counted on across each step
([Tempo segments](#tempo-segments)). It is item 1 of
[tempo and warp](tempo-and-warp.md#1-a-beat-map-through-tempo-changes).

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
uv run daw samples beats FILE --segment-at 1:56.8      # the tempo steps at the beat nearest this
uv run daw samples beats FILE --bpm 140,170            # a tempo for each segment in turn
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
measured on each call. `--bpm`, `--downbeat`, `--meter` and `--segment-at` are
corrections: they are saved with the map (`requested`) and apply to later calls
until `--refresh`.

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
  - `segments`: each stretch of one tempo, from the bar it starts on
    ([Tempo segments](#tempo-segments)). A song of one tempo has one, whose
    values are the tempo's. With several, `bpm` and `ambiguous_with` are the
    first segment's, `steady` is false, `drift_ms` is the largest segment's and
    `bpm_range` spans them all.
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
- `swing`: how late the song's off-beat notes fall against its beats, where
  the second note of each pair falls within it, on the 1/8 or the 1/16 grid
  ([key-chords-swing.md](key-chords-swing.md)). `daw samples chords` gives
  the song's key and chords on this map.
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
   Otherwise its beats are followed one by one (see [Tempo
   segments](#tempo-segments)): where the grid's beats are those within 40 ms,
   the grid is kept, the path through the windows' offsets that changes least,
   each beat taking its own transient where it is within 25 ms of the median of
   its neighbors'; where they are not, the beats followed are taken.
5. **Downbeats.** Five things that tend to happen on a downbeat are scored for
   each of the four places: a low onset, a change of harmony over two beats and
   over four, and a change of sound over four beats and over sixteen.
6. **Phrases.** At each downbeat the level in eight wide bands after it is
   compared with before, over four bars, two and one.

## Tempo segments

A song that switches tempo, a beat switch from 140 to 170 or a band coming in
faster than its riff, is cut into segments (`segments.py`), each measured as a
song of one tempo is.

1. **Where the steps are.** At each beat of a run of beats, the span of the two
   bars before it against the two bars after. Where they differ by more than 4%,
   the beat between two lines that fit each side best starts a new tempo. A
   fill, a ritardando shorter than two bars, and a band's drift of a percent or
   two are not steps. Nor is a stretch at half or double the tempo: a half-time
   switch is the same tempo counted twice as slowly, as the model hears a sparse
   verse, so a run of beats is first evened out, each beat the length of two or
   three of the song's quicker common beat cut into them.
2. **The run of beats.** With the model, its beats. Without it, the onset
   envelope is read for its tempo in windows of eight seconds a second apart, an
   octave folded into one, and a Viterbi path through those finds where it steps.
   Each stretch away from a step is measured for its tempo, and the beats are
   followed through the whole file by dynamic programming (Ellis, "Beat tracking
   by dynamic programming", 2007): the path that lands on the most onset while
   each beat's length stays near its stretch's tempo, either one near a step.
   Each beat followed is moved to the sharpest attack within 30 ms. A song that
   keeps to no grid is followed the same way, one stretch for the whole.
3. **Each segment measured.** From half a beat before its first beat to half a
   beat before the next segment's, by the measurement a song of one tempo has:
   the grid at its tempo, kept where it has the run's beats within 40 ms, or the
   run's beats. The beats are one run across the steps, counted on, so bar 69
   of the song is bar 69 of the map. Neighbours measured at one tempo are one
   segment, unless a step was asked for between them.
4. **How each is counted.** Without the model, each segment settles its own half
   or double time, as a song does. With the model, the longest segment is
   counted as a song is (doubled under 70 BPM, and so on) and each other at
   half, the same or double its heard tempo, whichever is nearest the longest:
   the model hears a 140 trap song at 70 and its switch to 170 at 85, which a
   song alone would keep as a slow groove. The bars are found in each segment
   as in a song, from where it starts.
5. **On a bar line.** A segment starts from the downbeat nearest the beat its
   tempo starts on, as a person counts a switch; `beat_seconds` is that beat.

Corrections: `--segment-at TIME`, once for each, makes the beat nearest TIME
start a segment, and a step found within two bars of it gives way to it.
`--bpm 140,170` gives a tempo for each segment in turn, which settles its half
or double time as `--bpm` does a song's; one value is the first segment's.
More values than segments is refused, with the advice to give `--segment-at`.

Each segment in `tempo.segments` has `bar`, `seconds` and `beat_seconds`,
`bpm`, `steady`, `drift_ms`, `bpm_range`, `ambiguous_with` (its half and
double, and a neighbour's tempo where it is at half or double it),
`beats_from` (onsets, model, or followed without the model) and `set_by`
(measurement or request). `hits`, `chords` and a reference continue a map past
its last beat at its last segment's tempo.

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
- A tempo that steps is followed where the step holds two bars and changes the
  beat by more than 4%; a ramp over several bars is followed as drift or cut
  into steps of about 4%. A step to exactly half or double is not found on its
  own; `--segment-at` and `--bpm` give it. A song cut off its own grid by an
  earlier edit is a run of steps at one tempo, which are not found.
- With the model, a stretch it hears at a tempo none of half, the same or
  double reaches, as a riff heard at 3:4, is a segment at the tempo it heard;
  `--bpm` gives the right one.
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

Tempo segments, November 26, 2026. Generated songs (a kick, a snare, a bass line,
hats and a layer every eight bars): a step from 120 to 150 at bar 17, and one on
beat 3 of bar 17, gave the step from bar 17 and every beat within 0.1 ms; a
band's drift, the tempo wandering 2% over eight bars and each beat off it by up
to 10 ms, at 120, 150 and 170 BPM, gave one segment and every beat within
0.1 ms, where the earlier following was off by 120 to 390 ms; the drift with a
step, two segments within 0.1 ms; a 150 song halving its beats at bar 17 one
steady 150, and with `--segment-at` and `--bpm 150,75` 75 from bar 17. With a
stand-in model: a step heard with beats ±15 ms off, the same; a 140 song heard
at 70 switching to 170, counted 140 then 170 from bar 25; a 150 song whose
middle it heard at 75, one segment.

Three of the person's songs with the installed model, against their earlier
maps:

- A song with a beat switch: 140 from bar 1, 170 from bar 69 (1:56.8),
  where its notes put the switch. Bar 68 is counted five beats, where the
  earlier map, on the model's slower beats there, had four; not heard.
- A live rock song: a free intro, its riff from bar 4 at 110, and the band at
  155 from 39.7 s, where the person's notes put the band in. The person counts
  the riff at 145: the model hears it at 3:4, which no half or double mends.
- A studio song at a steady 154: every beat where the earlier map had it, and
  a segment at 161 for its last 24 seconds, which the earlier map's range
  already reached; not heard.
