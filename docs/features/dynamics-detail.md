# Dynamics in detail — proposed October 7, 2026

Status: proposed, not built. Backlog: Next, Dynamics in detail. It adds to
[perception](perception.md) and to the render's report.

## What

Three things about how a mix moves that `daw listen` does not say today: how
far its loudness ranges and where it is loudest, how hard each drum hits at its
own stem and through the mix, and how much each compressor and limiter takes
off in each section.

```json
"mix": {"loudness_range_lu": 6.4, "max_short_term_lufs": -8.9, "max_momentary_lufs": -6.2},
"tracks": {"kick": {"hits": {"count": 128, "punch_db": 11.2, "punch_in_mix_db": 4.1}}},
"effects": {"master": [{"type": "limiter", "max_gain_reduction_db": 6.8,
  "sections": {"verse": {"max_gain_reduction_db": 0.4, "fraction_over_1db_reduction": 0.0},
               "drop":  {"max_gain_reduction_db": 6.8, "fraction_over_1db_reduction": 0.47}}}]}
```

## Why

The agent sets a compressor's threshold, attack and release and a limiter's
ceiling without hearing what they do. It has `crest_db`, the energy of each
beat and, in the render's report, each compressor's and limiter's largest and
mean reduction over the whole song. That says a limiter took 7 dB somewhere.
It does not say that it works through all of the drop and rests in the verse,
whether the kick still hits after its compressor, or whether the kick that hits
at its stem is buried by the time it reaches the mix. Those are what a person
listens for when setting dynamics.

The loudness range and the loudest three seconds are what a master is held to,
and the app's analyzer shows them while a song plays; `daw listen` of a render
reports neither.

## Design

**Loudness range and maxima,** for the mix and each section:
`loudness_range_lu` by EBU Tech 3342, `max_short_term_lufs` over 3-second
windows and `max_momentary_lufs` over 400 ms. The meter in `aaw-dsp` already
computes the range and the short-term and momentary loudness for the analyzer,
and is held by a test to a render's report; the two maxima are the largest of
each as it runs. `daw listen` reads all three from that meter, run over the
saved audio, so the window and the report are one implementation and cannot
disagree. `timeline.short_term_loudness` stays as it is.

**Punch.** For a track whose hits the song schedules, a pad's triggers or a
note clip's notes, `daw listen` already reads the schedule for
`musical_context` and has each hit's start, so nothing is detected. For each
hit, the peak level of its first 30 ms against the RMS of the next 200 ms,
which is
[sound descriptors'](sound-descriptors.md) `punch` measured in the song:

- `punch_db`: the median over the track's hits, at its stem. The stem is after
  the track's inserts, so a compressor's attack moves it.
- `punch_in_mix_db`: the same windows measured on the mix. Low where the
  stem's is high means the hit is there and something covers it.
- `count`, and both by section.

Hits closer together than 230 ms are measured to the next hit. A track of
audio clips has no schedule and no `hits`. `daw compare` reports both changes:
a slower attack shows as `punch_db` rising, a duck on the bass as
`punch_in_mix_db` rising.

**Reduction by section.** The engine's render keeps each device's reduction a
section at a time as well as over the whole song, and its report gains
`sections` on a compressor's or limiter's entry, each with
`max_gain_reduction_db`, `mean_gain_reduction_db` and
`fraction_over_1db_reduction`, as the whole-song fields are named. `daw listen`
copies each chain's entries into `effects`, by track, group, return and
`master`, so one report holds the levels and what the devices did to them. A
device inside a Synth's patch is reported under its track. A song without
sections has the whole-song fields only.

**Comparing.** `daw compare` reports each device's change by section, by its
`id`, and lists devices added or removed.

## Done when

- A generated 1 kHz tone at −20 LUFS for 20 s and then −30 LUFS for 20 s
  reports a loudness range of 10 LU within 1.
- `daw listen` of a render and the app's meter fed the same render agree on
  the three figures within 0.1.
- A generated kick through a compressor with a 0.1 ms attack reports a lower
  `punch_db` than through one with a 30 ms attack, and `daw compare` reports
  the difference. The kick under a loud sustained tone reports a lower
  `punch_in_mix_db` than alone, with the same `punch_db`.
- A limiter on a song that is loud in one section reports its reduction there
  and none in the other.
- `cargo test`, `uv run pytest -q`; `daw describe` says what each field means.

## Open questions

- Whether Python reaches the Rust meter through `aaw_py` or `daw listen`'s
  measuring moves to Rust, which the app standing alone will need anyway.
- Hits found in an audio clip by detecting onsets, for a drum loop.
- Reduction over time, a value a beat, for a picture of the limiter working
  under the energy.
- Crest by band, to say that the low end is squashed and the top is not.
- The 30 and 200 ms windows are sound descriptors' and were not tried on
  stems.
