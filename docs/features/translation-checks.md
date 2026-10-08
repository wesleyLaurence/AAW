# Translation checks — proposed October 7, 2026

Status: proposed, not built. Backlog: Next, Translation checks. It adds to
[perception](perception.md).

## What

`daw listen` measures the mix twice more, as it would arrive somewhere else:
summed to mono, and through the band a small speaker plays. It says what each
band and each stem loses, in decibels, and names the stems that nearly go.

```json
"translation": {
  "mono": {
    "loss_db": -1.1,
    "band_loss_db": {"sub": -7.9, "low": -2.4, "low_mid": -0.3, "...": 0.0},
    "tracks": {"pad": {"loss_db": -5.2}, "bass": {"loss_db": -0.1}}
  },
  "small_speaker": {
    "band_hz": [200, 8000],
    "loss_lu": -3.8,
    "tracks": {"bass": {"loss_db": -31.0}, "kick": {"loss_db": -9.5}, "lead": {"loss_db": -0.4}}
  },
  "observations": [
    "pad loses 5.2 dB in mono, most of it under 250 Hz",
    "bass loses 31 dB on a small speaker: its energy is under 200 Hz"
  ]
}
```

## Why

A mix is finished when it holds up away from the room it was made in. A person
checks by pressing a mono button and by playing the song on a phone. The agent
can do neither, and both are arithmetic on the stems it already has.

`daw listen` reports `stereo_correlation` and `side_energy_fraction` for the
whole signal, from which the loss in mono over all frequencies follows. They do
not say where: a wide pad whose low end cancels and one that is only wide up
high have the same numbers, and only the first is a fault. A bass that is a
sine at 50 Hz measures as a full low end and is silent on a phone, which no
number in the report says today.

## Design

**Mono.** The loss is the power of the mid signal, (L+R)/2, against the mean
power of the two channels: 0 dB when the channels are the same, −3 dB when they
have nothing in common, and without limit as they near opposite polarity,
written `null` when the mid is silent. `loss_db` over all frequencies is
`10·log10(1 − side_energy_fraction)`, which the report already holds; what is
added is the loss in each of the seven bands, for the mix, each section and
each stem, read from the Welch spectra of the mid and of the channels. A
utility's `mono_below_hz` on the stem that loses its low end is the usual fix,
and its effect shows as that band's loss going to 0.

**A small speaker.** The mix and each stem through a fixed filter: a highpass
at 200 Hz and a lowpass at 8 kHz, both 24 dB an octave. It stands in for a
phone's speaker and is not a model of one; the corners are printed in the
report. `loss_lu` is the filtered mix's integrated loudness against the mix's.
For each stem, `loss_db` is its filtered power against its own, and
`share_change_db` is how its share of the mix moves: a bass that carried a
fifth of the mix and carries a hundredth of what is left has gone, and a lead
that holds its level has come forward.

**Observations.** A few lines in words for the largest: a stem that loses more
than 3 dB in mono with the band it loses it in, a stem that loses more than
20 dB on the small speaker, a band of the mix that loses more than 3 dB in
mono. Measured, not judged: a sub that a phone cannot play may be meant.

**Comparing.** `daw compare` reports each loss before and after. Saturation
on a sine bass, which gives it harmonics over 200 Hz, shows as its small
speaker loss shrinking.

**Hearing it.** `daw listen RENDER --write-translation` writes `mono.wav` and
`small-speaker.wav` beside the analysis, so the person can play what the agent
measured. Nothing else is written, and the render is not changed.

## Done when

- A generated stereo file with identical channels reports 0 dB in every band;
  uncorrelated noise reports −3 dB within 0.2; a file with one channel's
  polarity flipped under 200 Hz reports a large loss in `sub` and `low` and
  under 0.5 dB above.
- The same file through a utility with `mono_below_hz: 200` reports under
  0.5 dB in every band.
- A 50 Hz sine bass reports a small speaker loss past 40 dB and is named in
  `observations`; with saturation on it the loss is under 20 dB.
- `--write-translation` writes the two files, and `daw listen` of `mono.wav`
  reports a `side_energy_fraction` of 0.
- `daw describe` says what each field means.

## Open questions

- Which systems: a laptop, earbuds and a car beside the phone, each a stated
  curve, or one small speaker and no more.
- The corners, 200 Hz and 8 kHz, were chosen as round numbers and not measured
  from a phone.
- A bass is heard on a small speaker through its harmonics. Energy over 200 Hz
  says they are there, not how loud the note seems.
- The loss in mono by third octave once
  [a finer spectrum](spectrum-detail.md) is built.
- Whether a mono loss belongs in `daw check` as a warning with a code, read
  from the last render.
