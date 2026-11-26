# The grid follows a song — implemented 2026-11-26

Status: implemented; item 3 of [tempo changes and warp markers](tempo-and-warp.md#3-the-grid-follows-a-song-daw-tempo-follow).

## What

`daw tempo follow` writes the master's tempo lane ([tempo changes](tempo-changes.md))
from an audio clip's beat map ([beat map](beat-map.md)), so that each of the
song's downbeats falls on a bar line of the session. A song played live, at
several tempos and drifting, then plays unstretched and on the click all the way
through, and a clip with `source_bpm` laid over it follows it.

```sh
uv run daw tempo follow PROJECT tracks.song.audio.0                    # a step a bar
uv run daw tempo follow PROJECT tracks.song.audio.0 --by segment        # a step a segment
uv run daw tempo follow PROJECT tracks.song.audio.0 --by beat           # a step a beat
uv run daw tempo follow PROJECT tracks.song.audio.0 --bar 5             # its first downbeat on bar 5
uv run daw tempo follow PROJECT tracks.song.audio.0 --keep-time         # a song already built
```

## Why

A live rock song under a mashup kept the riff on the grid and nothing else,
and a studio vocal that had to follow the band had its tempo change hidden in
its `source_bpm`, 154 × 145.34 / 155.45, loosening through the verse as the
band drifted ([tempo and warp](tempo-and-warp.md#why)).

## Design

**Measuring.** The command reads the clip's sample and its path through the
host (`get`), and runs `daw samples beats FILE --meter TS --all` on the file,
which measures the map, or reads it from `NAME.beats.json` beside the file. It
turns the map into pins: a file second pinned to a bar and a beat of that bar.

- `--by bar` (the default): each downbeat from bar 1, so each bar's tempo is
  its own length.
- `--by segment`: the downbeat each segment of the map starts on, and the last
  downbeat, so each segment's tempo is the length of its bars in the file.
- `--by beat`: each beat from bar 1, a bar of another length than the meter's
  (`odd_bars`) spread evenly over the session's bar.

**The op.** `tempo.follow {clip, pins, bpm, bar, keep_time}` does the rest in
the host (`aaw-host/src/follow.rs`), one undo step. A pin is `{seconds, bar}`,
with `beat` and `beats` (that bar's count) for a beat inside a bar. It refuses a
clip that is not an audio clip, one with `source_bpm`, a looped one, and fewer
than two pins inside the part of the file the clip plays.

- **The clip's place.** The clip moves so its first pin's bar starts on a bar
  line: bar `bar` when given, else the first bar its lead-in fits before at
  `bpm`, the tempo of the map's segment the first downbeat is in. That tempo
  holds from beat 0 to the first pin, so an intro played freely plays as it
  is. A clip cut into a bar keeps its first pin where it is in the bar.
- **The steps.** From the first pin on, each step's tempo is the one that takes
  the beat it starts on to where the next pin sounds, worked out from where
  that beat falls under the steps already written, so rounding to 0.001 BPM
  and a step left out are made up at the next one and never add up. A step is
  left out where it is within 0.05 BPM of the tempo playing and its pin lands
  within 1 ms without it, so a steady stretch is one step. A tempo outside 20
  to 400 is refused with the file's second, which `daw samples beats --near`
  shows.
- **`keep_time`.** The tempo before the first pin is fitted so the clip's
  first downbeat, at the second it sounds at now, is on a bar line: the bar
  whose fitted tempo is nearest the map's, or `bar`. Everything else placed in
  beats moves to the beat that sounds at its second now: note clips and audio
  clips by their `at`, each as a whole with its notes and length kept in
  beats, lane points on every track, group, return and the master, sections
  by their start and end, markers, and `length_beats`. Beats are written to a
  millionth of a beat.

**The reply** has `bar` and `at`, the clip's place, and `first_beat_seconds`,
its first pin in the file; `steps`, each with `at`, bar and beat, seconds and
`bpm`; `largest_off_ms`, how far the farthest pin is from its beat under the
steps written; `segments`, the map's segments the clip plays, each on the
session's bar with its tempo, steadiness and drift; `odd_bars` on the session's
bars; `moved`, with `keep_time`; and `stretched`, each audio clip with
`source_bpm` whose tempo changed, with the tempos it now plays at and its
speed against its file, and each pad with `source_bpm`. The command cannot
tell a clip's own tempo from a ratio written to make it follow another's;
those are set by the agent or the person. `by` and `beat_map` (the file, the
analyzer, where the beats came from) are added by the CLI.

## Done when

- A generated band at 120 that steps to 150 at bar 17, drifting 2% over eight
  bars and each beat off by up to 10 ms, after a two-beat pickup, followed
  `--by bar` and rendered: every downbeat of its stem measured by `daw samples
  beats` within 5 ms of the session's bar lines from the first to the last,
  and a steady 154 song with `source_bpm: 154` over its second segment keeping
  each of its downbeats within 10 ms of the bar (`tests/test_tempo_follow.py`).
- `--by segment` writes the step before the pickup, the first segment's and
  the second's, each pin within 1 ms; `--by beat` a step on most beats.
- `--keep-time` on a song built at 120 around the band renders its clicks on
  the same frames, within one, and the band within a millisecond, with its
  first downbeat on the bar the reply gives and a marker at its second.
- The op places pins on their beats to within a microsecond, a step where a
  bar changes tempo; refuses `source_bpm`, a track and too few pins; keeps a
  bar given and refuses one the lead-in does not fit before; follows beats
  inside a bar from a clip cut into one; and undoes as one step
  (`aaw-host/tests/tempo.rs`).

## Decisions

- **The CLI measures and the host follows.** The beat map is Python's, so the
  CLI runs it and sends the host pins; the host works out the steps and the
  clip's place against the song it holds, in one undo step, and a batch can
  give pins of its own. Rejected: the steps worked out in the CLI from the
  saved song, which a running host's unsaved edits would have made wrong.
- **A step left out keeps its pin within 1 ms, not only within 0.05 BPM.** Over
  a segment of sixteen bars, 0.05 BPM is 13 ms by its end; the rule alone put
  the second segment's first downbeat 3.5 ms off in the test. Within a bar the
  two rules are about the same.
- **The default bar is the first the lead-in fits before.** The design said
  bar 1; a song whose first downbeat comes after a pickup or an intro cannot
  start before beat 0, so bar 1 holds only a song that starts on its downbeat.
- **The tempo before the first downbeat is the map's**, the tempo of the
  segment the clip's first downbeat is in, to 0.001 BPM; with `keep_time` it
  is fitted to the clip's second instead, the nearest bar to the map's tempo
  chosen.
- **A note clip moves as a whole under `keep_time`.** Its notes keep their
  beats, as the design has it, so a clip whose notes span a step plays them at
  the new tempo; an audio clip without `source_bpm` plays its file as before.

## Limits

- The steps are only as good as the beat map. On a generated band at 145 that
  steps to 155 while drifting, `daw samples beats` without the downbeat model
  measures two steady grids rather than following the beats, up to 23 ms off,
  so its downbeats would be off the bars by as much (Later in the backlog).
- `odd_bars` are followed, a bar of three beats in 4/4 stepped to the tempo
  that makes its three beats one session bar; the meter itself does not
  change (a meter lane is item 6 of [tempo and warp](tempo-and-warp.md)).
- `keep_time` moves what is in beats and leaves the rest; a delay in beats
  or a synced LFO follows the new tempo, and a clip with `source_bpm` plays at
  the new tempo where it is, which `stretched` names.
