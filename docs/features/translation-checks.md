# Translation checks — implemented 2026-10-09

Status: implemented (D102). `daw listen` measures the mix twice more, as it
would arrive somewhere else: summed to mono, and through the band a small
speaker plays. It says what the mix, each band and each stem loses, in
decibels, and names the stems that nearly go. It adds to
[perception](perception.md).

```sh
daw listen projects/demo/renders/latest.json                      # the losses, under "translation"
daw listen projects/demo/renders/latest.json --write-translation  # and the two files, to hear
daw listen projects/demo/renders/latest.json --section drop       # one section's, with the rest of it
daw compare BEFORE AFTER                                          # each loss before and after
```

```json
"translation": {
  "mono": {
    "loss_db": -1.1,
    "band_loss_db": {"sub": -7.9, "low": -2.4, "low_mid": -0.3, "mid": 0.0, "high_mid": 0.0, "high": -0.1, "air": -0.4},
    "sections": {"drop": {"loss_db": -1.3, "band_loss_db": {"sub": -8.2, "...": 0.0}}},
    "tracks": {"pad": {"loss_db": -5.2, "band_loss_db": {"sub": -60.0, "...": 0.0}}}
  },
  "small_speaker": {
    "band_hz": [200, 8000],
    "slope_db_per_octave": 24,
    "loss_lu": -3.8,
    "loss_db": -6.1,
    "sections": {"drop": {"loss_lu": -4.4, "loss_db": -7.0}},
    "tracks": {"bass": {"loss_db": -47.6, "share_change_db": -41.5},
               "lead": {"loss_db": -0.4, "share_change_db": 5.7}}
  },
  "observations": [
    "the mix loses 7.9 dB in mono in sub, 20 Hz to 60 Hz: pad loses 60 dB or more there",
    "pad loses 5.2 dB in mono, most of it under 250 Hz",
    "bass loses 47.6 dB on a small speaker: its energy is under 200 Hz"
  ],
  "observations_omitted": 0
}
```

## Why

A mix is finished when it holds up away from the room it was made in. A person
checks by pressing a mono button and by playing the song on a phone. The agent
can do neither, and both are arithmetic on the stems it already has.

`stereo_correlation` and `side_energy_fraction` describe the whole signal, from
which the loss in mono over all frequencies follows. They do not say where: a
wide pad whose low end cancels and one that is only wide up high have the same
numbers, and only the first is a fault. A bass that is a sine at 50 Hz measures
as a full low end and is silent on a phone, which no other number in the report
says.

## A loss

Every loss is in decibels, at or under zero. It is never written past −60 dB:
−60 stands for 60 dB or more, which is gone. `null` means there was nothing
to lose: silence, or a band more than 60 dB under the sound's loudest band, as
the top of a bass. Opposite channels are a number, −60, and not `null`, so that
"gone" and "not there" read differently.

## Mono

`mono.loss_db` is the power of the mid, (L+R)/2, against the mean power of the
two channels: 0 dB when the channels are the same, −3 dB when they have nothing
in common, which is as far as width alone goes, and further only as they near
opposite polarity. Over all frequencies it is
`10·log10(1 − side_energy_fraction)`.

`band_loss_db` is the same in each of the [seven bands](perception.md), read
from a spectrum of the channels and of their mid: Hann windows of 8192 frames,
as `band_dbfs` has them. The mid's spectrum is the mean of the channels' before
the power is taken, so one pass over the audio gives both.

It is measured for the mix, each section of the mix (`sections`) and each stem
(`tracks`). `mono` is `null` for a file of one channel.

A utility's `mono_below_hz` on the stem that loses its low end is the usual
fix. A stem with one channel's polarity turned over under 200 Hz reads −60 dB
in `sub` and −5.9 in `low`; with `mono_below_hz: 200` it reads `null` in `sub`,
where nothing is left, and −0.7 in `low`: the crossover's upper half, 24 dB an
octave, lets a little of what lies under it through. At 300 Hz it reads −0.1.

## A small speaker

The mix and each stem through a fixed filter: a Butterworth highpass at 200 Hz
and a lowpass at 8 kHz, both 24 dB an octave. It stands in for a phone's
speaker and is not a model of one; the corners are printed as `band_hz`.

| Field | Meaning |
|---|---|
| `loss_lu` | The filtered mix's integrated loudness minus the mix's |
| `loss_db` | The power the filter keeps, against the whole: of the mix, of a section, of a stem |
| `share_change_db` | A stem's `loss_db` minus the mix's: how its share of the mix moves. Under zero it falls back, over zero it comes forward |

`loss_lu` is measured on the filtered audio; `loss_db` is read from the
spectrum through the filter's response, which costs a stem no second pass. The
two differ for the mix because loudness weighs the low end less than power
does. A sine bass at 50 Hz, two octaves under the highpass, loses 48 dB; driven
24 dB into a soft saturation, which gives it harmonics over 200 Hz, it loses
15.8.

## Observations

A few lines in words, the mono ones first and the largest first within each
kind:

- a band of the mix that loses more than 3 dB in mono, with the stem that
  loses the most power there, when that stem loses more than 3 dB in the band
  itself. A group is not named there: its tracks are;
- a stem that loses more than 3 dB in mono, with where: the shortest run of
  bands that holds two thirds of what it loses, as "most of it under 250 Hz",
  and nothing when the loss is spread over more than four of the seven;
- a stem that loses more than 20 dB on the small speaker, with the side its
  energy is on.

A stem more than 40 dB under the mix is not named, nor a group where one of its
tracks is, which says more. Up to five stems are named for each of the two, and
`observations_omitted` counts the rest. Measured, not judged: a sub that a
phone cannot play may be meant, and unrelated channels lose 3 dB by their
nature, which is why the line is past 3.

## Comparing

`daw compare BEFORE AFTER` has `translation`, each loss with `before`, `after`
and `delta`:

- `mono.loss_db` and `mono.band_loss_db`, and `small_speaker.loss_lu` and
  `loss_db`, of the mix;
- `sections`, the `loss_db` in mono and the `loss_lu` on the small speaker of
  each section where the two renders line up;
- `tracks`, each stem both renders have, with `loss_db` and `band_loss_db` in
  mono and `loss_db` and `share_change_db` on the small speaker, the stem whose
  loss changed most first.

Saturation on a sine bass shows as its small speaker `loss_db` rising by 30 dB
or so, and a `mono_below_hz` as the low bands' loss going to about 0. `mono` is
`null` when either file has one channel.

## Hearing it

`daw listen RENDER --write-translation` writes two files beside `listen.json`,
with their paths under `translation_audio`, so the person can play what the
agent measured:

- `mono.wav`: the mid in both channels, which is what a mono button plays;
- `small-speaker.wav`: the mix through the filter. Where its peaks would pass
  full scale, which a highpass can do to a limited mix, the file is lowered by
  that much, and `small_speaker_gain_db` says by how much; it is 0 otherwise.

Both are written as the mix's kind of WAV. Nothing else is written, and the
render is not changed. Without the flag `translation_audio` is `{}`. The flag
goes with the whole report, not with `--overlap` or `--section`.

`daw listen RENDER --section ID` has `translation`: that section's `mono` and
`small_speaker` of the mix.

## Time

On this Mac, an Intel one of eight cores, the checks add about four seconds to
`daw listen` of a generated three-minute song of twelve stems and six
sections, which takes about forty-eight in all: a stem's losses are read beside
its other measurements, and the mix is filtered once. `--write-translation`
adds under a second.

## Limits

- The corners, 200 Hz and 8 kHz, are round numbers and not measured from a
  phone, and there is one system: no laptop, earbuds or car.
- A bass is heard on a small speaker through its harmonics. Energy over 200 Hz
  says they are there, not how loud the note seems.
- Mono's loss is read in seven bands, the widest two octaves, not in third
  octaves: `low` runs from 60 to 250 Hz, so a loss under 100 Hz and one at
  200 Hz read alike.
- The lines an observation is made at, 3 dB, 20 dB and 40 dB under the mix,
  were chosen on generated audio and read on one real render of fourteen
  stems. Nobody has listened to the two files: whether the losses named are
  the ones a person hears is under Verify.
- A stem's band that loses in mono is said in words only when the mix's band
  or the whole stem is past the line. The numbers are in `band_loss_db`.
- A stem's small speaker loss by section, and its mono loss by section, are
  not measured: the mix's are.
- A grouped track is read before its group's effects and fader, as every stem
  is.
- A reference's losses are not kept, so `daw compare --reference` does not
  compare them.
- `daw check` does not read a render, so it has no code for a mono loss.

`daw describe listen` says what each field means.
