# Tempo changes in the song — implemented 2026-11-26

Status: implemented; item 2 of [tempo changes and warp markers](tempo-and-warp.md#2-tempo-changes-in-the-song).

## What

A song's tempo steps along a lane on the master, `param: tempo`, and
everything that turns beats into time follows it: the schedule, the voices
and the stretched clips, delays, the Synth's LFOs, devices of the person's
own, the metronome, recording, MIDI files written, the Python tools and the
app's ruler, clock and tempo field. `daw tempo list|set|remove|clear` reads
and edits the steps.

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

## Why

Some of the person's songs change tempo: a beat that switches from 140 to
about 170, a band at several tempos. With one tempo for the session the
ruler's bars ran ahead of the song after a switch, and a tempo change had to
be hidden in a clip's `source_bpm` ([tempo and warp](tempo-and-warp.md#why)).

## Design

**In the song.** A lane on the master, `param: tempo`, holds steps: each
point `curve: hold` and no `shape`, its value 20 to 400 BPM. A ramp is
refused, with the point named. Before the first point the first value holds,
as on any lane; of two points at one beat the second holds from there. Only
the master has the lane. `session.tempo` is the lane's tempo at beat 0, which
the host keeps so after every edit, as it keeps `length_beats`; `daw set
session.tempo` alone on a song with the lane is refused, naming `daw tempo
set`. A song with the lane is saved as `schema_version: 3`, which an engine
from before refuses; one without is saved as before, byte for byte.

**One map.** `aaw_model::TempoMap` (`tempo.rs`) is built from the session and
its lane: steps, each with its beat, its tempo and the exact second it
starts on. It gives the frame of a beat, rounded half up from the exact
second, and in floating point the second of a beat, the beat of a second and
of a frame, and the tempo at a beat or a second. A map of one tempo gives
`beat::frame`'s frames by the same arithmetic and the seconds `beats * 60 /
tempo` gives, so a song without a lane schedules, renders and measures as it
did: every render comparison the tests already make holds. The schedule,
the envelopes of lanes, the program's length, the clip voices, the host's
clock and cue, recording and the range commands read it. Python reads the
same map through `aaw_py.tempo_map` and `aaw_py.map_frame` as
`model.TempoMap`; the app gets its steps in `Arrangement.tempo_steps`.

**What follows the tempo.** Everything placed in beats stays on its beat:
notes, clips' `at`, sections, markers, lane points, `length_beats`. What is in
seconds or milliseconds keeps its length: fades, `lead_ms`,
`source_start_seconds`, attack and release.

| Part | With a tempo lane |
|---|---|
| An audio clip without `source_bpm` | Plays its file at its own speed from the second its beat falls on; its length in beats is read through the map, for the song's length, the range verbs, `daw check` and the app. A looped one plays as much of its file in each repetition as its beats last there |
| An audio clip with `source_bpm` | Plays `tempo / source_bpm` as fast at the tempo where it is. One across a step plays as a part a step, each prepared at its step's tempo, the next part crossing over in the 5 ms before the step |
| A pad with `source_bpm` | Stretched to the tempo where each hit is |
| A delay's `time_beats` | Its time at each step from the step's frame, moving to the new time over 20 ms |
| A Synth LFO in beats | Its rate follows each step from the step's frame, and a locate finds its phase from the beat; a lane on its rate wins |
| The arpeggiator, MIDI effects | In beats, so on the grid wherever it is |
| An offline device of the person's own | Given `tempo`, the tempo at beat 0, and `tempo_map`, each step's beat, seconds and bpm, to an entry that takes a `tempo_map` keyword; its cache is made again when the map changes |
| A real-time WebAssembly device | Given the tempo at beat 0 in `aaw_init`, as before |
| The metronome | Clicks on each beat through the map, at each step's tempo |
| A take recorded | Placed at the beat its first frame was heard against, through the map; a note take's beats likewise |
| A MIDI file written from a clip | The tempo where the clip starts, and a tempo event at each step inside it |
| The playhead | Carried across a recompile at the same beat through both maps |

**Commands.** `daw tempo list PROJECT` gives each step's `at`, bar and beat,
seconds and `bpm`, whether the tempo changes, and the song's length in
seconds. `daw tempo set PROJECT --at PLACE --bpm X` sets the tempo from PLACE
on, a bar (`17`) or `BAR:BEAT` (`17:3`) from 1 in the song's meter: a step
added or changed, or at bar 1 the opening tempo. `daw tempo remove PROJECT
--at PLACE` takes a step out, the one before holding on; the opening one is
not removed. `daw tempo clear PROJECT` goes back to one tempo, the one at
beat 0. Each is an op through the host, one undo step: `tempo.set {at, bpm}`
and `tempo.remove {at}` in beats, and `tempo.clear`, which a batch takes; the
read is `tempo.list`. The steps are written back from the map, a step to the
tempo already playing left out, and a song left with one tempo has no lane
and that tempo as `session.tempo`, so it saves as a song that never had one.
A frozen track holds the tempo until it is unfrozen. `daw describe tempo`
describes all of it.

**Checks.** `daw check` warns of a step off a bar line
(`tempo-step-off-bar`), and of a pad or clip with `source_bpm` stretched past
about 8% at any tempo it plays at (`stretch-audible`, naming that tempo). A
delay's time must be 1 ms to 10 s at the song's slowest and fastest tempos.

**In the app.** The ruler's time strip reads each second through the map, so
`0:32` stands over bar 17 of a song at 120 that steps there and the seconds
after it are wider; bars keep their width, the timeline being drawn in
beats. Its labels are spaced at the song's slowest tempo, where a second is
narrowest. The playhead and the position follow the host's clock, which
reads its frames through the map. The transport's tempo field shows the
tempo at the playhead. The master's tempo lane is drawn as steps under the
master when A shows the lanes, labelled Tempo in BPM, its range the song's
tempos with room either side. The export sheet reads a range's length
through the map.

**Python.** `timeline.py` places clips and pads through the map, splits a
stretched clip at the steps as the engine does, and reports seconds and
beats through it, with `tempo_steps` when the tempo changes; `--tempo-for`
scales every step by one factor. `perception.py`'s energy by beat, its
sections and its plots, `overlap.py`'s cells, `spectrum.py`'s spans of bars,
`dynamics.py`'s hits, `export.py`'s range and `separation.py`'s length read
it. A report of a song whose tempo changes carries its steps in
`source.tempo_steps`; a song of one tempo's report is as it was.

## Done when

- A generated song at 120 that steps to 150 at bar 17 renders each of 96
  clicks on the frame the map gives its beat, and played from three places
  around the step it is the same frames as its render
  (`aaw-engine/tests/tempo.rs`, `tests/test_tempo.py`).
- A clip with `source_bpm: 120` across the step keeps each click within 2
  frames of its beat when repitched and within 5 ms when stretched; one
  without plays its file as it is and lasts its 48 s.
- Every render comparison of the existing tests holds, so a song without a
  lane renders as before.
- `daw tempo` sets, lists, removes and clears the steps through the host, a
  song back at one tempo saves as it did, and a frozen track refuses a change
  (`aaw-host/tests/tempo.rs`).
- Scripted runs of the app on a generated song show the time strip following
  the step, the tempo field at 150 past bar 17 and the master's lane as a step.

## Decisions

- **`session.tempo` stays, as the tempo at beat 0.** Removing it from a song
  with a lane would change every reader of it; keeping it in step with the
  lane, as `length_beats` is kept with the content, means a reader that wants
  one tempo still gets the song's opening one. Rejected: letting the two
  differ, where the field would mean nothing.
- **A song left with one tempo has no lane.** `daw tempo` writes the lane from
  the map, so a step to the tempo already playing is no step, and the last
  step removed takes the lane with it. A song edited back to one tempo is
  then the song it was, version and bytes.
- **A stretched clip across a step is a part a step, crossing over before the
  step.** Each part is prepared at one ratio as a whole clip is, with the
  stretcher's margin from the file either side, and the parts meet as two
  clips that touch do: the next part's lead of 5 ms over the last 5 ms of the
  one before. Rejected: one part stretched by a changing ratio, which neither
  stretcher's prepared path takes; that is warp markers' work (item 4).
- **A delay moves its time over 20 ms at a step.** A jump in a delay line's
  length clicks; a crossfade between the old tap and the new does not.
- **The time strip's labels are spaced at the slowest tempo.** Spaced at the
  opening tempo, a song that slows would crowd its labels after the step.
- **Seconds are read through the map in floating point and frames exactly.**
  Steps only, so every frame is a sum of exact segments; floating point is for
  what was floating point already.
- **A step off a bar line is allowed and warned of** (open question 1 of the
  design): a song that changes tempo on beat 3 is placed with `--at 19:3`.
- **The tempo field with a lane shows the tempo at the playhead**, and typing
  a new one there is refused by the host until the app sets steps (item 4).
- **A real-time device keeps its one tempo.** Its ABI's `aaw_init` takes one;
  a change of the ABI is a change of the device spec.

## Limits

- An unstretched audio clip's waveform and a track's peaks are drawn at the
  tempo where the clip starts, so across a step they drift from what plays;
  the app drawing through the steps comes with editing them there (Next).
- `daw joins`' "a beat or more" and `daw key`'s weighting of samples in beats
  use the opening tempo.
- A ramp, a meter change and editing the steps in the app are later items of
  [tempo and warp](tempo-and-warp.md).
