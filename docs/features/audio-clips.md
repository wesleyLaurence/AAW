# Audio clips — implemented October 1, 2026

An audio clip is part of a sample file on a track's timeline. It is what an edit
of a finished song is made of: the parts to keep, placed on beats, with the fades
that join them.

## In the song

```yaml
tracks:
- id: song
  pads: {}
  audio:
  - {sample: song, at: 4, source_start_seconds: 41.213, source_end_seconds: 79.840,
     lead_ms: 5, fade_in_ms: 2, fade_out_ms: 12}
  - {sample: song, at: 81.25, source_start_seconds: 88.345, lead_ms: 5, fade_in_ms: 4}
```

A track has `audio`, a list of audio clips, beside `clips`, its pattern clips, and
can have both. An audio clip has:

| Field | Default | Meaning |
|---|---|---|
| `sample` | required | The sample it plays |
| `at` | 0 | The beat that `source_start_seconds` of the file plays on |
| `source_start_seconds` | 0 | Where in the file it starts |
| `source_end_seconds` | none | Where in the file it ends; none is the file's end |
| `lead_ms` | 0 | How long before its beat it starts |
| `gain_db` | 0 | Its level |
| `fade_in_ms` | 0.3 | The fade over its start |
| `fade_out_ms` | 8 | The fade after where it leaves |
| `fade_curve` | `equal_power` | `equal_power` or `linear`, for both fades |
| `source_bpm` | none | The file's tempo; with it the clip follows `session.tempo` |
| `stretch` | `repitch` | `repitch` or `preserve_pitch`; see [time-stretch.md](time-stretch.md) |

A clip plays its file as it is: at full level, a mono file on both sides. A pad
plays a hit at velocity 100 of 127 and pans a mono file 3 dB down, so the same
song on a pad is 5 dB quieter.

Its start must be inside the session. Its sound may run past the session's end,
where the end fade takes it, as a pad's tail does.

## A cut before the beat

A join sounds best a few milliseconds before the beat, in the quiet before the
hit, so the incoming hit is whole. That is `lead_ms`: the clip is written with
the beat it belongs on (`at`) and the song's own time for that beat
(`source_start_seconds`), and it starts `lead_ms` early with the file as much
earlier. Nothing is offset by hand.

A clip that ends where another clip of its track begins leaves from where that
one starts. The two meet at one place before the beat, whatever each clip's own
lead is. From there the leaving clip fades out over its `fade_out_ms` and the
entering one fades in over its `fade_in_ms`, which should be shorter than its
lead so the fade is over when the beat arrives.

`equal_power` fades keep the level across two different parts of a song, which is
what a join in an edit is. `linear` fades keep it across audio that is the same.

## Commands

```sh
uv run daw track add SONG song
uv run daw audio add SONG song song --at 4 --source-start-seconds 41.213 --lead-ms 5 --fade-in-ms 2
uv run daw audio move SONG tracks.song.audio.1 --at 44 --track other   # to a beat, a track or both
uv run daw audio cut SONG song --from 82 --to 98        # take sixteen beats out and close the gap
uv run daw audio split SONG tracks.song.audio.0 --at 40
uv run daw audio trim SONG tracks.song.audio.1 --end 120
uv run daw audio crossfade SONG tracks.song.audio.1 --ms 20
uv run daw remove SONG tracks.song.audio.1              # remove one
```

A clip is named by its path, `tracks.TRACK.audio.N`, or by the `@N` reference
`daw inspect` lists while a host runs. Beats are the session's.

- **`audio add`** takes the clip's fields as `--field value`.
- **`audio move CLIP --at BEAT --track TRACK`** takes a clip to another beat,
  another track or both. Its audio goes with it and it keeps its reference.
- **`audio cut TRACK --from A --to B`** removes those beats from the track's audio
  clips. A clip across the cut becomes two; a clip inside it is removed; clips
  after it move earlier by its length; and the join is crossfaded: 12 ms out, 4 ms
  in, 5 ms before the beat, unless `--ms`, `--in-ms` and `--lead-ms` say
  otherwise. This is "take out the eight beats around 1:20". Pattern clips and
  other tracks are left where they are.
- **`audio split CLIP --at BEAT`** makes two clips of one, with no fade between
  them. They play exactly as the one did: the same frames at the same places.
- **`audio trim CLIP --start BEAT --end BEAT`** moves an edge to a beat. The audio
  stays where it is on the timeline; the clip shows more or less of it.
- **`audio crossfade CLIP`** sets the join between a clip and the one before it on
  its track.

Each is one step of the host's undo history, like any command.

## Around them

- `daw inspect` lists each track's audio clips with their references.
- `daw timeline` counts them in where a track's sound is.
- `daw joins` finds a join between two audio clips and names them `audio.N`.
- `daw describe edit` has the schema and what the fields mean.
- The engine plays a clip as one voice of its track, so the track's effects,
  sends, automation and stem are those of any track.
- The Mac app draws a track's audio clips with their file's waveform, and
  moves, trims, fades and splits them:
  [audio-clips-in-app.md](audio-clips-in-app.md).

## Limits

- A command leaves the song's length alone: a clip must start inside the song,
  and what it plays past the song's end is not heard. The Mac app lengthens the
  song for a clip placed there ([audio-clips-in-app.md](audio-clips-in-app.md)).
- A cut is of one track. A sound on another track that should move with it is
  moved by hand.
- A split of a clip that is stretched in time is two regions stretched apart, so
  it is close to, not exactly, the whole clip.
- Clips do not reverse. A clip loops with `loop_beats` and `length_beats`
  ([looping-clips.md](looping-clips.md)); a looped clip's start is not trimmed
  and it is split only at a wrap.

Tests: the engine's place a clip's file on its beat to the frame, with a lead, at
fractional beats and tempos; check both fade curves, the gain and a join; and the
host's cover each command and its refusals. In Python an edit built with
`audio add` and `audio cut` passes the join check with the beat within 0.1 ms, a
clip split four times renders bit for bit as the whole one, and the edit sped up
4% at its own pitch keeps its join. Nobody has listened to one.
