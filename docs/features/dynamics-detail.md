# Dynamics in detail — implemented 2026-10-09

Status: implemented (D103). `daw listen` says three things about how a mix
moves: how far its loudness ranges and where it is loudest, how hard each
track's hits land at its own stem, through its group and through the mix, and
how much each compressor, limiter and clipper takes off in each section. It
adds to [perception](perception.md) and to the render's report.

```sh
daw render projects/demo                                     # each device's reduction by section, in the reply
daw listen projects/demo/renders/latest.json                 # the three, in one report
daw listen projects/demo/renders/latest.json --section drop  # one section's
daw compare BEFORE AFTER                                     # each of them before and after
```

```json
"mix": {"loudness_range_lu": 6.03, "max_short_term_lufs": -6.91, "max_momentary_lufs": -6.9},
"tracks": {"kick": {"hits": {
  "count": 128, "punch_db": 13.2, "punch_in_group_db": 9.6, "punch_in_mix_db": 7.4,
  "sections": {"drop": {"count": 64, "punch_db": 13.2, "punch_in_group_db": 9.6, "punch_in_mix_db": 5.1}}}}},
"effects": {
  "master": [{"path": "master.effects.lim", "type": "limiter", "id": "lim",
    "max_gain_reduction_db": 5.71, "mean_gain_reduction_db": 1.93, "fraction_over_1db_reduction": 0.381,
    "sections": {"verse": {"max_gain_reduction_db": 0.0, "mean_gain_reduction_db": 0.0, "fraction_over_1db_reduction": 0.0},
                 "drop":  {"max_gain_reduction_db": 5.71, "mean_gain_reduction_db": 3.86, "fraction_over_1db_reduction": 0.762}}}],
  "tracks": {"bass": [{"path": "tracks.bass.effects.duck", "type": "compressor", "id": "duck", "keyed_by": "kick", "...": 0}]}
}
```

## Why

The agent sets a compressor's threshold, attack and release and a limiter's
ceiling without hearing what they do. It had `crest_db`, the energy of each
beat and, in the render's report, each device's largest and mean reduction over
the whole song. That says a limiter took 6 dB somewhere. It does not say that
it works through all of the drop and rests in the verse, whether the kick still
hits after its compressor, or whether the kick that hits at its stem is buried
by the time it reaches the mix. Those are what a person listens for when
setting dynamics.

The loudness range and the loudest three seconds are what a master is held to,
and the app's analyzer shows the range while a song plays; `daw listen` of a
render reported neither.

## The loudness range and the loudest seconds

The mix and each section have three figures, which stems do not:

| Field | Meaning |
|---|---|
| `loudness_range_lu` | The range of EBU Tech 3342: the loudness of the last 3 s is read every 100 ms, the readings under −70 LUFS and those more than 20 LU under the mean of the rest are left out, and the range runs from the 10th to the 95th percentile of what is left |
| `max_short_term_lufs` | The loudest 3 s |
| `max_momentary_lufs` | The loudest 400 ms |

They come from the analyzer's meter, not from a second implementation.
`aaw-dsp`'s `loudness` holds the K-weighting, the 100 ms blocks and the range;
the meter of the app's window feeds it what plays, and `daw listen` feeds it
the saved audio through `aaw_py.loudness`, so the window and the report cannot
disagree. A test feeds the meter a signal in uneven blocks and holds the three
figures to what the file's measurement gives.

A tone at −20 LUFS for 20 s and then at −30 for 20 s reads a range of 10.0 and
a loudest 3 s of −20.0. A figure is `null` under 3 s of audio, or 400 ms for
the momentary, and under −70 LUFS; the range takes two readings, so a section
of exactly 3 s has a loudest 3 s and no range. `integrated_lufs` is still
pyloudnorm's and `timeline.short_term_loudness` is as it was, a reading a
second; the loudest of those and `max_short_term_lufs`, read every 100 ms,
agree within 0.1 on a steady sound.

## Hits

For a track whose hits the song schedules, a pad's triggers or a note clip's
notes through a Sampler or a Synth, `hits` says how hard they land. Nothing is
detected: each hit's first frame is read from the song, as
[a finer spectrum](spectrum-detail.md) reads what a track plays. Hits that
start within 50 ms of one another are one hit: a chord, a flam, a roll.

A hit's punch is the peak of its first 30 ms against the RMS of the 200 ms
after them, in decibels, which is [sound descriptors'](sound-descriptors.md)
`punch` measured in the song. Where the next hit comes sooner, the RMS is of
what there is before it. A punch is never written past 60 dB: an attack with
nothing after it.

| Field | Meaning |
|---|---|
| `count` | The hits that made a sound: a hit whose first 30 ms peak under −80 dBFS at the stem is not one |
| `punch_db` | The median over the track's hits, at its stem. The stem is after the track's effects, so a compressor's attack moves it |
| `punch_in_group_db` | The same frames at the stem of the track's group, after the group's effects, which the track's own stem is before. Only a grouped track has it |
| `punch_in_mix_db` | The same frames in the mix. Far under `punch_db`, the hit is there and something covers it |
| `sections` | The same for each section a hit starts in; a section without one is left out |
| `pads` | On a track that plays more than one pad, the same for the hits each pad starts in, with whatever else the track plays then |

A generated kick, a sweep that dies away in a quarter of a second, reads 13.2.
Through a compressor at a ratio of 8 with an attack of 0.1 ms it reads 7.1: the
front is taken off. With an attack of 30 ms it reads 25.2: the front goes
through and the rest is taken down. Under a held tone at −10 dBFS it still
reads 13.2 at its stem and 9.7 in the mix. A held tone reads 3, a click with
nothing after it 60, and a sound that swells, or a bass ducked by the kick it
starts with, under 0.

`hits` is `null` for a group, a return and a track of audio clips, where the
song schedules nothing, and has a `count` of 0 for a track whose hits make no
sound. A pattern track's hits and a MIDI track's notes are read alike; a hit
with a note, which repitches a sample, names no pad.

## Reduction by section

The engine's render keeps each compressor's, limiter's and clipper's
reduction between the edges of the song's sections as well as over the whole
of it. Each such
device's entry in `report.json`, under a track's, group's or return's
`effects`, a Synth track's `instrument_effects` or `master_effects`, gains
`sections`, with `max_gain_reduction_db`, `mean_gain_reduction_db` and
`fraction_over_1db_reduction` for each section that holds a frame, as the
whole-song fields are named. An effect that has an `id` in the song now names
it in its entry. A song without sections has the whole-song fields only.
Sections may overlap: the timeline is cut at every section's edges, and a
section's reduction is put together from the pieces inside it.

`daw listen` copies the compressors, limiters and clippers into `effects`, so
one report holds the levels and what the devices did to them: the master's
under `master`
and the others under `tracks`, by track, group or return, in the chain's order
with a Synth patch's devices first.

| Field | Meaning |
|---|---|
| `path` | Where the device is in the song, as `daw effect` and `daw set` take it: `master.effects.lim`, `groups.drums.effects.0`, `tracks.lead.instrument.synth.effects.hold` |
| `type`, `id` | `compressor` or `limiter`, and its ID if it has one |
| `keyed_by` | The track a compressor's sidechain names |
| `max_gain_reduction_db`, `mean_gain_reduction_db`, `fraction_over_1db_reduction` | The most it took off, the mean, and the share of the time it took more than 1 dB, over the song |
| `sections` | The same three in each section |
| `bypass` | `true` for a bypassed device, which has no numbers |

A channel without a compressor or a limiter is left out, and other kinds of
effect are not listed: they are in the render's report. A limiter with a
ceiling of −6 dB on a song that is over it for six of its last eight seconds
reads 0 in the first half and 5.7 dB, 76% of the time over 1 dB, in the second.

## Comparing

`daw compare BEFORE AFTER` has all three:

- the mix and each section that lines up have the change in
  `loudness_range_lu`, `max_short_term_lufs` and `max_momentary_lufs` under
  `actual_delta`, and the two maxima under `loudness_matched_delta`: a gain
  moves them and leaves the range where it was;
- each track both renders have has `hits`: `count`, `punch_db`,
  `punch_in_group_db` and `punch_in_mix_db` with `before`, `after` and `delta`,
  under `sections` where the renders line up and under `pads` for the pads
  both have. A slower attack shows as `punch_db` rising, and a duck on the bass
  as the kick's `punch_in_mix_db` rising. It is `null` when either render
  schedules nothing on the track;
- `effects` has `devices`, each compressor, limiter and clipper both renders
  have, with each reduction before and after, whole and by section, the
  largest change
  first; and `added` and `removed`, the paths of those only one render has. A
  device is matched by its `id`, or by its place among its channel's devices
  of its type when it has none, so a device that moved in its chain is still
  the same one and has `path_before`. `bypass_before` and `bypass_after` are
  there when either is bypassed.

`daw listen RENDER --section ID` has the section's three figures under `mix`,
each track's `hits` in it and `effects`, each device with what it took off
there.

## Time

On this Mac, an Intel one of eight cores, the three add under a second to
`daw listen` of a generated three-minute song of twelve stems and six
sections: 0.2 s for the loudness of the mix and its sections and 0.4 s for
7560 hits on nine tracks. The render's own time is as it was.

## Limits

- A hit inside an audio clip is not found: the song does not say what a file
  holds, so a drum loop has no `hits`.
- `punch_in_mix_db` reads everything that plays at the hit. In a full mix it is
  close to the mix's own movement at those moments, so it says most beside the
  track's `punch_db` and beside other tracks'.
- On a kit, a pad's hits are read in the track's stem with whatever else the
  track plays then: a kick with a hat on it reads a little over the kick alone.
- The 30 and 200 ms are sound descriptors' and were chosen on generated audio.
  Nobody has listened: whether a `punch_db` that rose is a kick that hits
  harder to a person is under Verify.
- A limiter reaches ahead by its look-ahead, so a peak just past a section's
  start shows in the `max_gain_reduction_db` of the section before it, for a
  few milliseconds. The mean and the fraction do not move.
- A section render and a track render report the reduction of the whole song,
  since the engine renders the song to make them; a track render has no master
  chain.
- A render made by an earlier engine has no `sections` in its devices. Render
  the song again.
- Reduction over time, a value a beat, crest by band and the loudness range of
  a stem are not measured, and a reference keeps none of the three figures.
- The app's window shows the range; the loudest 3 s and 400 ms are in the
  meter's reading and not drawn.

`daw describe listen` says what each field means.
