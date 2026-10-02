# Timeline — implemented October 1, 2026

`daw timeline` gives a song's timeline in both units. Positions in a song are
beats; a request names seconds. It converts between them, says where the sound
is, places a sound to end on a beat, and fits the session's length.

```sh
uv run daw timeline SONG                              # the length, where the sound ends, each track
uv run daw timeline SONG --seconds 0:41 1:43.5        # times as beats, and as bar and beat
uv run daw timeline SONG --beats 16 82.5 1/3          # beats as seconds and m:ss
uv run daw timeline SONG --end-at 64 --pad fx.swell   # where the pad starts to end on beat 64
uv run daw timeline SONG --fit --tail 1               # end the session a beat after the sound
```

- A place is given as `beats`, `seconds`, `time` (`m:ss.mmm`), and `bar` and
  `beat` counted from 1 in 4/4. Times are places on the song's timeline at its
  tempo, not places in a sample file.
- `length` is the session's; `sound_ends` is the end of the last hit's audio, and
  `tracks` gives each track's first sound and where its last one ends. Effect tails
  are not counted.
- `--end-at BEAT --pad TRACK.PAD` gives the pad's length and the beat it must start
  on to end at `BEAT`: its sample from `start_seconds` to `end_seconds`, or to the
  file's end, at the speed the pad plays it. Trim the pad's `end_seconds` to where
  its sound should stop first, since a file can end in silence.
- `--fit` sets `session.length_beats` to the next whole beat after the sound ends,
  plus `--tail` beats, through `daw set`, so a running host sees it and it can be
  undone. A session holds its clips whole, so it does not end before the last clip
  does; the result says when a clip held it longer.

A hit that an event transposes is measured without that transposition.
