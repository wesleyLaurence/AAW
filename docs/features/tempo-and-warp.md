# Tempo changes and warp markers — proposed November 26, 2026

Status: item 1, a beat map through tempo changes, built November 26, 2026
([beat map](beat-map.md#tempo-segments) is its reference); item 2, tempo
changes in the song, built November 26, 2026 ([tempo changes](tempo-changes.md)
is its reference, with its decisions); item 3, the grid following a song,
built November 26, 2026 ([tempo follow](tempo-follow.md) is its reference,
with its decisions); items 4–6 proposed, built in order, each its own branch
and pull request (D205). The tempo half of item 5, the tempo edited in the
app, is Next item 2 of the backlog, among the person's notes of November 26,
2026 (D206); warp markers, warp markers in the app and item 6 are Next items
9–11.

## What

Songs that do not keep one tempo, in the song and in what is laid on it:

1. [A beat map through tempo changes](#1-a-beat-map-through-tempo-changes):
   `daw samples beats` finds where a song's tempo steps, and gives each
   stretch of it its own tempo.
2. [Tempo changes in the song](#2-tempo-changes-in-the-song): a `tempo` lane
   on the master, steps in beats, which everything that turns beats into time
   follows.
3. [The grid follows a song](#3-the-grid-follows-a-song-daw-tempo-follow):
   `daw tempo follow` writes that lane from an audio clip's beat map, so a song
   played live, at several tempos, plays unstretched and on the click.
4. [Warp markers](#4-warp-markers): an audio clip's file seconds pinned to its
   beats, so a song that drifts or changes tempo is stretched onto any grid.
5. [Both in the app](#5-both-in-the-app): the tempo drawn and edited on the
   master's lane and in the transport, and an audio clip's markers dragged on
   its waveform.
6. [Ramps and meter changes](#6-ramps-and-meter-changes): a tempo that moves
   linearly between two points, and the time signature as a lane of its own.

Items 1–3 make the grid follow a song; item 4 makes a song follow a grid.
Together they cover a song with several tempos that drifts, laid on the
click, and a second song stretched to the first, bar by bar.

## Why

The person's edits and mashups start from finished songs, and not every song
keeps one tempo. On November 25, 2026 two of them did not:

- A clean version of a song whose beat switches from a steady 140 to about
  170 after bar 68. The session holds one tempo, so after the switch the
  ruler's bars ran ahead of the song's, two bars by bar 79, and every swap was
  placed by its seconds.
- A mashup over a rock song played live: an intro, a riff at about 145, the
  band at about 155, drifting as a band does. Only the riff sits on the grid.
  A vocal from a studio song at a steady 154 had to follow the band, so the
  agent wrote its `source_bpm` as 143.987, which is 154 × 145.34 / 155.45:
  the tempo change hidden in one clip's field, and the lock loosening through
  the verse as the band drifted.

What the person asked for: the live song lined up to a click all the way
through, at each of its tempos, without being stretched; and the studio song
imported and stretched to the live song's tempo where it plays.

Ableton does both. Its tempo is an automatable parameter on the master, and
since every position is in beats, a tempo change moves nothing but the length
of a beat. An audio clip's warp markers pin seconds in the file to beats, and
the clip is stretched between them. In a Live Set the first is an
`AutomationEnvelope` on the master's `Tempo`, its events in beats, and the
second an audio clip's `WarpMarkers`, pairs of `SecTime` and `BeatTime`. The
concept already lists "tempo and time signature changes ... warp markers for
material whose tempo drifts".

## Design

### Old songs are unchanged

The tempo lane and warp markers are optional. A song without them means what
it means today and saves byte for byte as it does: one `session.tempo` for the
whole song and one `source_bpm` an audio clip. Its fingerprints are the same
and its render reports still verify. The tempo map of a song without a lane is
one tempo, and its frames are the ones `aaw_model::beat::frame` gives today,
by the same exact arithmetic; the golden model and the render comparisons hold
it.

A song with a tempo lane or a warped clip is saved as `schema_version: 3`,
which an engine that knows only 1 and 2 refuses by name, rather than playing
everything after the first change at the wrong time. A song with neither is
written as 1 or 2, as now.

A song that worked around one tempo keeps its workaround and plays as it did.
Nothing converts it: whether a `source_bpm` is a clip's real tempo or a ratio
hidden in it is not in the song. Converting one is a deliberate edit,
[`daw tempo follow --keep-time`](#3-the-grid-follows-a-song-daw-tempo-follow).

### 1. A beat map through tempo changes

Built November 26, 2026; [beat map](beat-map.md#tempo-segments) is its
reference, and its choices are under [Decisions](#decisions). What follows is
the design as proposed.

`daw samples beats` follows one tempo, or one that wanders slowly; a song that
changes tempo is not followed ([beat map](beat-map.md)). The downbeat model
already hears the beats one by one and bars of different lengths
([the downbeat model](downbeat-model.md)), so most of what is needed is
measured; what is missing is reading the steps from it.

- **Segments.** The report's `tempo` gains `segments`: each from a bar and its
  seconds, with its `bpm`, `steady` and `drift_ms` as the whole song has them
  now, and its `bpm_range` where it is not steady. A song with one tempo has one
  segment, so a reader of `tempo.bpm` is unchanged; `tempo.bpm` stays the first
  segment's, and `steady` is false where there are several.
- **Where a step is.** A change of the beat period that holds for at least two
  bars, placed on the downbeat where it starts; a fill or a ritardando shorter
  than that is drift. A step is placed on a bar line, as the person counts it.
- **Through the step.** The beats and bars are followed across it and counted
  on, so bar 69 of the song is bar 69 of the map.
- **Half and double time** are settled for each segment, as for a song today,
  and a segment at half or double its neighbour's tempo is listed in
  `ambiguous_with`, since a half-time switch is often the same tempo. Tempo is
  counted as the person's taste counts it: trap at 140, not 70.
- **Corrections.** `--segment-at TIME`, repeated, says a step is there, and
  `--bpm` takes one value for each segment (`--bpm 140,170`). They are saved
  with the map, as `--downbeat` is, until `--refresh`.
- The saved map gains the segments; since the measuring code changes, each saved
  map is measured again once, as when the downbeat model came.

Done when: generated songs with a step (120 to 150 at bar 17), with a step
between half and double time, and with a band's drift (the tempo wandering 2%
over eight bars, each beat off it by up to 10 ms) give the step on its bar and
every beat within 5 ms of where it was made; and the person's song that
switches from 140 to 170 reports its two segments with the switch on bar 69.

### 2. Tempo changes in the song

Built November 26, 2026; [tempo changes](tempo-changes.md) is its reference,
and its choices are under its Decisions. What follows is the design as
proposed.

**In the song.** A lane on the master, `param: tempo`, as a master `gain_db`
lane is today, values within `session.tempo`'s bounds, 20 to 400 BPM:

```yaml
session:
  tempo: 140.0
master:
  automation:
  - param: tempo
    points:
    - {at: 0, value: 140.0, curve: hold}
    - {at: 272, value: 170.0, curve: hold}   # bar 69
```

In this item a tempo point is a step: `curve: hold` is the only curve, and
`shape` is refused. Before the first point the first value holds, as for any
lane. With a lane, `session.tempo` is the lane's value at beat 0, kept so by
the host as `length_beats` is, so a reader of `session.tempo` gets the song's
opening tempo; `daw set session.tempo` on a song with a lane is refused with
the lane's name and `daw tempo set`.

**What follows the tempo.** Everything placed in beats stays on its beat: note
clips and notes, an audio clip's `at`, sections, markers, lane points and
`length_beats`. Everything in seconds or milliseconds keeps its length: fades,
`lead_ms`, `source_start_seconds`, `attack_ms`. The rest:

| Part | With a tempo lane |
|---|---|
| An audio clip without `source_bpm` | Plays its file at its own speed; its length in beats is read through the map |
| An audio clip with `source_bpm`, a pad with it | Plays `tempo / source_bpm` as fast at each beat, so it follows each step; a clip across a step is prepared a part a step |
| A delay's `time_beats` | Its time at each step, the change crossfaded |
| The Synth's synced LFOs, the arpeggiator's rate | Follow the tempo where they play |
| A device of the person's own | Given the map, where it is given the tempo now |
| The metronome | Clicks at the tempo where it is |
| A take recorded | Placed by the map |
| A Standard MIDI file written | Writes the map as its tempo events |
| `length_beats` | Fitted through the map from a clip's sound in seconds |

**One map, read everywhere.** A `TempoMap` in `aaw-model` gives the frame and
the seconds of a beat, the beat of a second and the tempo at a beat, built from
the session and its lane. `beat::frame` takes it; its 36 callers and the places
that compute `session.tempo / bpm` or `60 / tempo` themselves read it instead.
With steps only, the map is a sum of exact segments and stays exact. Python's
conversions between beats and seconds, some 45 lines across `timeline.py`,
`perception.py`, `export.py`, `joins.py` and others, call it through `aaw_py`
rather than computing it. The app reads it through the FFI.

**Commands.** `daw tempo list PROJECT` gives each step's bar, beat, seconds and
tempo. `daw tempo set PROJECT --at BAR --bpm X` adds or changes a step, `daw
tempo remove PROJECT --at BAR` takes one out, and `daw tempo clear` goes back to
one tempo. Each edits the lane, as `daw automation` would, through the host.
`daw describe tempo` describes them.

**In the app, as far as reading goes.** The ruler's time strip
([time ruler](time-ruler.md)), the playhead and the clock follow the map; bars
keep their width, since the timeline is drawn in beats. The transport's tempo
field shows the tempo at the playhead. The master's `tempo` lane is drawn in the
automation lanes as steps. Editing it there is item 5.

**Checks.** `daw check` warns of a clip with `source_bpm` stretched past about
8% in any step it plays in, as it warns for the whole song now, and of a step
that is not on a bar line.

Done when: a generated song with a step at bar 17 renders each hit on the frame
the map gives it, played from any beat and rendered alike; a song without a lane
renders as before, bit for bit, over the corpus the tests already render; and
the app's ruler, clock and tempo field follow the step on a generated song.

### 3. The grid follows a song: `daw tempo follow`

Built November 26, 2026; [tempo follow](tempo-follow.md) is its reference,
and its choices are under its Decisions. What follows is the design as
proposed.

```sh
uv run daw tempo follow PROJECT tracks.song.audio.0              # a step a bar
uv run daw tempo follow PROJECT tracks.song.audio.0 --by segment  # a step a segment
uv run daw tempo follow PROJECT tracks.song.audio.0 --keep-time   # a song already built
```

From the beat map of the clip's sample (item 1), it writes the tempo lane so
that each of the song's downbeats falls on a bar line of the session, and
moves the clip so its first downbeat is on one (`--bar N`, bar 1 unless
given). The song is not stretched: the clip must have no `source_bpm` and no
warp, and the command refuses one that has them. The click then lines up with
every bar of the song through each of its tempos.

- **`--by bar`** (the default) writes a step a bar from the map's beats, which
  is what a song played live needs: each bar's tempo is the bar's own length.
  Steps within 0.05 BPM of the one before are left out, so a steady stretch is
  one step.
- **`--by segment`** writes a step a segment of the map, for a song that keeps
  each of its tempos steadily, such as a beat switch.
- **`--by beat`** writes a step a beat, for a song that drifts within the bar.
- **Before the first downbeat**, an intro played freely, the first segment's
  tempo holds; the intro plays as it is, off the grid.
- **Several songs.** The lane follows one clip. A stretched clip on the same
  timeline, with `source_bpm`, then follows that lane (item 2): a studio
  vocal at a steady 154 laid over the live song plays at the live song's tempo
  in each bar, its own pitch kept by its `stretch`. That is the person's second
  case, with no ratio written by hand.

**`--keep-time`**, for a song built at one tempo before the lane: everything
placed in beats after the first step, clips, notes' clips, lane points,
sections and markers, moves to the beat that sounds at the second it sounds at
now, so the song sounds as before and only the grid moves under it. A note
clip is moved as a whole and its notes keep their beats. The reply lists every
clip with `source_bpm` whose speed changes under the new lane, and by how much,
since the command cannot tell a clip's real tempo from a ratio written to work
around one tempo; the agent or the person sets those.

The reply gives the steps written, the clip's place and, for each segment, the
tempo and its drift.

Done when: on a generated song with an intro, a step and a band's drift, the
render's downbeats measured by `daw samples beats` are within 5 ms of the
session's bar lines from the first downbeat to the end, and a steady 154 vocal
with `source_bpm: 154` over it keeps each of its downbeats within 10 ms of the
bar; `--keep-time` on a song built at one tempo renders the same audio before
and after, within the stretcher's own differences.

### 4. Warp markers

**In the song.** An audio clip may have `warp`, a list of at least two
markers, each a file's second pinned to a beat counted from the clip's `at`:

```yaml
- sample: song
  at: 64
  stretch: preserve_pitch
  warp:
  - {beat: 0, seconds: 12.402}
  - {beat: 16, seconds: 22.718}
  - {beat: 32, seconds: 32.961}   # the band pushed here: a shorter stretch of file
  - {beat: 48, seconds: 43.390}
```

- Between two markers the clip plays at one speed, the one that takes the first
  marker's second to its beat and the second's to its; before the first and
  after the last, the speed of the nearest pair holds.
- Beats and seconds both increase strictly from one marker to the next.
- The beat a marker gives is a session beat, so the time it plays at comes from
  the tempo map: a warped clip follows the tempo lane too.
- With `warp`, `source_bpm` is refused: a clip with `source_bpm` is the case of
  two markers at one speed, and reads as it does now. `source_start_seconds` is
  the second the markers give at beat 0, kept so by the model; markers before
  beat 0 are kept, as a note before its clip's start is, so a clip trimmed from
  its start and lengthened again loses nothing.
- `stretch` applies as now: `preserve_pitch` and `preserve_formants` keep the
  pitch at each speed; `repitch` plays each stretch faster or slower with its
  pitch, as a record nudged. `transpose` and loops apply on top, a loop
  repeating the warped content.
- Split, trim, move, copy, join, the range verbs and a time selection keep a
  clip's markers, the right half of a split taking those after the cut, moved by
  the split's beat.

**The engine.** A stretched clip is prepared before it plays
([time stretch](time-stretch.md)), so each stretch between markers is prepared
at its own speed and the parts are joined on the frame. Signalsmith takes a
changing ratio as it is fed; Rubber Band is given the markers as its key-frame
map. A render's report names the markers with the stretcher, so the same song
always makes the same audio.

**Commands.**

```sh
uv run daw audio warp PROJECT tracks.b.audio.0 --to-beats            # a marker a downbeat
uv run daw audio warp PROJECT tracks.b.audio.0 --to-beats --by beat  # a marker a beat
uv run daw audio warp PROJECT tracks.b.audio.0 --add 32 --seconds 32.961
uv run daw audio warp PROJECT tracks.b.audio.0 --move 32 --seconds 32.95
uv run daw audio warp PROJECT tracks.b.audio.0 --remove 32
uv run daw audio warp PROJECT tracks.b.audio.0 --clear               # unwarped again
```

`--to-beats` reads the sample's beat map (item 1) and pins each of its
downbeats, or each beat, to the clip's bar lines, the file's bar *k* on the
clip's bar *k*, so the song keeps every bar on the grid whatever its own tempo
does. `--from-bar N` starts it at the file's bar *N*. The reply gives each
stretch's speed against the file, the largest first.

**Checks.** `daw check` warns of a stretch faster or slower than about 8%
against its file, by the marker it starts at, and of markers closer than a
sixteenth note, which a stretcher can be heard joining.

This is the other direction from item 3: the song moved onto a grid, not the
grid onto the song. It is for a song that drifts or changes tempo laid on
another song's grid or on a steady session: the live song under a studio song
at the studio song's tempo, a sample from a record not played to a click
looped on the grid, or one late hit pinned back onto its beat.

Done when: a generated song with a step and a band's drift, warped
`--to-beats` into a session at a steady tempo, renders with every beat
measured within 5 ms of the grid; a split, a trim and a loop of it render the
same beats; and a clip with `source_bpm` renders bit for bit as before.

### 5. Both in the app

The tempo is built first, ahead of warp markers, as its own item: the person
asked for tempo changes made in the app on November 26, 2026 (D206).

- **The tempo** on the master's automation lane: a step dragged up or down
  changes its BPM, with the value shown as it moves; a double-click adds a step
  at the bar under the pointer; Delete removes the selected ones, as on any
  lane. The transport's tempo field, typed into, sets the step the playhead is
  in, or the one tempo of a song without a lane. A Tempo ▸ Follow This Clip
  item in an audio clip's menu runs `daw tempo follow`.
- **Warp markers** on an audio clip's waveform in the detail panel, where its
  beat map's beats are already marked ([audio clips in the
  app](audio-clips-in-app.md)): a marker dragged along the waveform pins another
  second to its beat; a double-click on a beat mark adds one there; a marker's
  menu removes it; Warp to Beats runs `daw audio warp --to-beats`. The
  timeline's clip shows a small mark where each marker is.

Each is a command through the host, in the undo history.

Done when: scripted runs of the bundle on a generated song move a step, add one
and type a tempo, and drag a marker, each landing in the song as the matching
`daw` command would write it.

### 6. Ramps and meter changes

- **Ramps.** `curve: linear` on a tempo point: the tempo moves linearly in beats
  to the next point, as a ritardando or a build. The seconds of a beat in a ramp
  are 60 × (b₁ − b₀) / (t₁ − t₀) × ln(t₁ / t₀), which is not a rational number,
  so a ramp's frames are computed in floating point and rounded as every frame
  is; steps stay exact. `daw tempo follow` may then write a ramp where a band
  speeds up through several bars, in place of a step a bar.
- **Meter changes.** A master lane `param: time_signature` of `N/D` values, each
  held to the next, on a bar line of the meter before it; bars are counted
  through it, by the ruler, `BAR:BEAT` and the beat map's `--meter`. Until
  then, the song has one time signature ([time signature](time-signature.md)).

Done when: a generated song with a ramp renders each hit within one frame of the
formula's time, and a song in 4/4 that turns to 3/4 at bar 9 counts its bars on
in the ruler and `daw map`.

## Decisions

Item 1, November 26, 2026:

- **A step is a change of the span of two bars, not of their median.** The
  model places a beat on its 20 ms frame, so at 150 BPM a beat is 0.38 or
  0.40 s and the median of eight jumps by 5%, which read as steps all through a
  steady song. The span averages the frames out.
- **Half and double time is no step, anywhere.** The model hears a sparse
  stretch of a 154 song at 77, and the stretches where it flips read at
  in-between tempos, 100 to 113, which a fold of the spans alone did not stop:
  one steady studio song came out in thirteen segments. The model's beats are
  first evened out, a beat two or three of the song's quicker common beat long
  cut into them; without the model the rough tempos are an octave folded. A
  half-time switch is then the same tempo, as the design has it, and is given
  with `--segment-at` and `--bpm` when wanted. Rejected: confirming the model's
  steps with the onset tempogram's, since that one found three steps in the
  steady song and missed the band's step in the live one.
- **With the model, a segment is counted nearest the longest one.** Counted
  alone, as the design said, the beat switch the model heard at 85 after 70
  stayed 85, a slow groove by the person's taste, giving 140 then 85. Each
  segment but the longest takes the half, same or double nearest the longest's
  tempo. Without the model each segment settles its own, from where its
  transients sit.
- **Without the model, a song that keeps to no grid is followed by dynamic
  programming.** The earlier following of window offsets was off by 120 to 390
  ms on a band's drift with beats 10 ms either way, and joining a grid fitted to
  each side of a step landed eleven beats late in the same drift. Ellis's
  dynamic programming, with the beat's length held near each stretch's tempo
  and either one near a step, followed both within a frame, and each beat moved
  to its attack within 30 ms is within 0.1 ms. A steady song is measured as
  before, bit for bit.
- **A beat followed takes its own transient within 25 ms of its neighbours'
  median**, not 10 ms: a band moves a beat 10 ms either way, and at 10 ms the
  beats furthest out were taken from their neighbours, 13 ms off.
- **The bars are found in each segment as in a song.** One path of the model's
  downbeats through the whole song, bars of four counted beats, put a bar of
  seven before the switch in the beat switch song, since half the bars of a
  segment counted doubled have no downbeat the model heard.
- **Neighbours at one tempo after measuring are one segment**, measured again
  over both, with and without the model: stretches the model heard two ways
  can come out at one tempo.
- **A step is reported on the nearest bar line**, with the beat it starts on
  as `beat_seconds`; the beats themselves stay where they are measured.

## Open questions

1. **A step off a bar line.** Allowed and warned of, as built in item 2:
   `daw tempo set --at 19:3` places one.
2. **`daw timeline --tempo-for`**, the tempo at which a song lasts so many
   seconds: with a lane, scale every step by one factor, as "make it 2% faster"
   is said?
3. **The tempo field typed into with a lane:** it shows the tempo at the
   playhead (item 2); set the step the playhead is in (the design), or scale
   the whole lane, or ask?
4. **A MIDI file read in** keeps not taking its tempo (D65); with a lane, should
   `--tempo` take the file's tempo map as the song's?
5. **A take recorded into a song with a lane** is placed by the map; is the
   metronome's click while recording enough for a person playing along to a
   step?
