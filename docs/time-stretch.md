# Time stretch — implemented October 1, 2026

A pad can follow the session's tempo at its own pitch. "Speed it up slightly"
is then a change of `session.tempo`, and the song plays faster without sounding
higher.

## In the song

```yaml
session: {tempo: 124.8, stretcher: signalsmith}
tracks:
- id: song
  pads:
    part: {sample: song, source_bpm: 120, stretch: preserve_pitch}
```

- `pad.source_bpm` is the tempo of the pad's sample. A pad that has it follows
  `session.tempo`: it plays `tempo / source_bpm` times as fast.
- `pad.stretch` says how. `repitch`, the default and what the sampler always did,
  plays the sample faster or slower, and its pitch moves with it. `preserve_pitch`
  stretches it in time and keeps its pitch. `transpose` and an event's `note` still
  repitch, on top.
- `session.stretcher` is `signalsmith`, the default, or `rubberband`.
- A pad without `source_bpm` is not stretched, so a one-shot keeps its length when
  the tempo changes.

Everything placed in beats stays where it is when the tempo changes: a part that
started on beat 20 still does, and the beats of the song inside it still fall on
session beats. An edit built at the song's tempo keeps its joins at any tempo.

```sh
uv run daw timeline SONG --tempo-for 58        # the tempo at which the song lasts 58 s
uv run daw set SONG session.tempo 124.8
uv run daw check SONG                          # warns past about 8%
```

`--tempo-for` gives the tempo, the change in percent and the pads that follow the
tempo with their stretch mode. Sounds that do not follow it keep their seconds, so
one that starts after the last beat takes more beats at a faster tempo; fit the
length again (`daw timeline --fit`) and ask for the tempo once more if it matters.

## The stretchers

| | `signalsmith` | `rubberband` |
|---|---|---|
| What | Signalsmith Stretch 0.1.3 | Rubber Band's finer engine (R3) |
| Where | Linked into `daw` and the app | The installed `rubberband` program, run as a separate process |
| License | MIT | GPL; nothing of it is linked |
| To get it | Nothing | `brew install rubberband` |
| Time for a minute of stereo | about 0.7 s | 4 to 8 s |

A song that names `rubberband` on a Mac without the program fails to render with
a message that says so. Both give the same audio for the same input each time, and
a render's report names the stretcher and its version under `dependencies`, so a
render made with another version has another identity. A song that stretches
nothing has the identity it had before.

Which sounds better is for ears, on real songs. `session.stretcher` switches
between them for the whole song, so the same edit can be rendered with each.

## How it is done

Stretching happens where repitching does: when a pad's audio is prepared, off the
audio thread, and cached by what it was made from. Nothing stretches while audio
plays, and playback equals the render.

A region is stretched with 0.2 s of the file either side of it, which is then cut
off, so what a stretcher does at its own start and end is not in the region and
the audio at a join is stretched in context on both sides. At the song's own
tempo the ratio is one and the stretcher is not run: the audio is bit for bit what
`repitch` gives.

## What was measured

Before choosing, each candidate stretched a generated song and a minute of two
local renders by 4%, and the beat map of each result was held against the
original's beats divided by 1.04:

| On the two with drums | Length | Worst beat against the others | All beats, against where the ratio puts them |
|---|---|---|---|
| Signalsmith | exact | 0.01 to 0.04 ms | 0.3 to 0.9 ms early |
| Rubber Band R3 | exact | 0.1 ms | 0.3 to 1.1 ms early |
| Rubber Band R2 | exact | 0.03 to 0.4 ms | 0.5 to 0.6 ms late |
| ffmpeg `atempo` | off by 5 to 21 ms | up to 7.5 ms | up to 12 ms late |

The first three keep the beats a constant fraction of a millisecond from where the
ratio puts them, the same for every beat, so a join between two stretched parts
holds. `atempo` does not and is not offered. On the third song, whose samples have
slow attacks and whose own beats are measured only to a few milliseconds, the
first three were each 1 to 2.7 ms late with the worst beat about 2 ms from the
others.

In the tests a tone stretched 4% faster keeps its 440 Hz to half a hertz with both
stretchers, where `repitch` gives 457.6 Hz; its clicks land within 2 ms of their
beats; and the edit of [join-checks.md](join-checks.md), sped up 4%, passes its
join check with the beat within 0.1 ms by the beat map and 0.5 ms by the audio.

## Limits

- Nobody has listened to a stretched render. The choice of default rests on
  licensing, speed and those timing measurements, not on sound.
- Past about 8% a stretch can usually be heard; `daw check` warns there.
- Audio shorter than about a fifth of a second, with nothing either side of it in
  the file, is too short for Signalsmith and is refused.
- The Mac app's bundle does not carry `rubberband`, and it does not gather the
  licenses of the Rust crates the engine links, which now include this one.
