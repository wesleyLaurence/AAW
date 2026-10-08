# Masking between stems — proposed October 7, 2026

Status: proposed, not built. Backlog: Next, Masking between stems. What it is
for is in [concept.md](../concept.md#library-and-perception): a perception
report that says "kick and bass overlap heavily from 60 to 120 Hz" as an
observation.

## What

`daw listen` says where two stems put their energy in the same band at the same
moments: which pair, which band, how much of the time, and the bars where it is
worst. `daw compare` says how an edit changed it.

```sh
daw listen projects/demo/renders/latest.json                      # the worst pairs, under "overlap"
daw listen projects/demo/renders/latest.json --overlap kick bass  # one pair, by section and band
daw compare BEFORE AFTER                                          # each pair's change
```

```json
"overlap": {
  "pairs": [
    {"a": "kick", "b": "bass", "band_hz": [45, 110], "fraction": 0.71,
     "level_difference_db": 1.8, "keyed": false,
     "worst": [{"section": "drop", "at_beat": 64, "length_beats": 32, "fraction": 0.93}]}
  ],
  "pairs_omitted": 12
}
```

## Why

Deciding which of two sounds owns a band is most of mixing: the kick or the
bass under 100 Hz, the pad or the lead around 2 kHz. A person hears the
contest. The agent has every stem and, today, each stem's level in seven bands
over a section, which says that the kick and the bass are both loud in `low`
and not whether they are loud at the same moments. A bass that plays between
the kicks and one that plays on them read the same.

`daw check`'s `register-crowded` reads the notes: two tracks written in one low
octave. This reads the audio, so it sees a sample's real spectrum, a filter, a
sidechain's duck and a reverb's tail.

## Design

**What is measured is overlap.** Two stems overlap in a cell, a band over a
short time, when both have energy there at close levels. Overlap is what
masking needs and is not masking as an ear does it: nothing here spreads a loud
band over its neighbors, tells a tone from noise or changes with the playback
level. The report says "overlap" for that reason, and the words stay
observations.

**Cells.** Each stem's power on a grid of bands by time. Time is a sixteenth
note, and never under 100 ms. Bands are third octaves from 90 Hz to 20 kHz, and
two wider bands below, 20–45 Hz and 45–90 Hz, since a window that short cannot
tell third octaves apart there. Channel powers are averaged, as `band_dbfs`
does it.

**A contested cell.** Stems A and B contest a cell when the weaker is within
6 dB of the stronger, the weaker is within 9 dB of the mix in that cell, so the
pair is what is heard there, and the mix's cell is within 40 dB of the band's
loudest, so silence is not counted. For a pair and a band, `fraction` is the
contested cells over the cells where either stem sounds. Bands next to each
other that are contested together are reported as one range, `band_hz`.

**What a pair carries.** `fraction` over the render; `level_difference_db`, A
minus B, over the contested cells; `worst`, up to three stretches of bars or
sections with the highest fraction, as `at_beat` and `length_beats`; and
`keyed`, read from the song: whether one has a compressor keyed by the other.
The stems are post-insert, so a duck is already in the audio and lowers the
fraction; `keyed` says why.

**How much is printed.** The five pairs with the highest fraction over 0.25,
and `pairs_omitted`. `--overlap A B` prints one pair by section and band and
writes `overlap-A-B.png`: bands up, beats across, each cell colored by which
stem is louder and marked where contested.

**Comparing.** For each pair in either report, `fraction` before and after, and
its change by section when the timelines align. A cut at 80 Hz on the bass, or
a sidechain, shows as the fraction falling.

**Which stems.** Tracks, groups and returns, as the render writes them: a
track's stem is its sound before its group and before the master's effects.
A return against the tracks that feed it is left out, since its sound is
theirs. A file without stems has no `overlap`.

## Done when

- Two generated tracks with an 80 Hz tone in bars 5 to 8 report one pair, in
  the 45–90 Hz band, with those bars as the worst. An 80 Hz tone against a
  2 kHz tone reports no pair.
- A bass under a kick reports a lower fraction with a compressor keyed by the
  kick than without, and `keyed: true`. `daw compare` of the two renders
  reports the fall.
- The thresholds are tried on one real session with the person listening, and
  the pairs it names are the ones they hear; what was changed is written here.
- A three-minute song of twelve stems adds under five seconds to `daw listen`,
  measured on this Mac. `daw describe` says what each field means.

## Open questions

- The thresholds, 6, 9 and 40 dB, are a first guess and were not run on audio.
- A model of masking proper, with spreading between bands as a codec's
  psychoacoustic model has, and whether it names different pairs from overlap.
- Whether the low bands should be read from longer windows at the cost of time
  resolution, since the kick and the bass meet exactly there.
- A grouped track against its own group's stem, and two tracks of one group.
- Whether `daw check` should gain a code for the worst pair, read from the last
  render when there is one.
