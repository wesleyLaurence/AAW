# Sound descriptors — proposed October 4, 2026

Status: proposed, not built. Backlog: Next, Sound descriptors. What
it is for is in [concept.md](../concept.md#library-and-perception): the
library's samples measured for loudness and spectrum, not only pitch and tempo.

## What

Each sample measured for what it sounds like, in numbers with a word beside each:
how bright it is, how fast it starts, how long it rings, how much low end it has,
how noisy or tonal it is. Search filters and sorts on them, and finds samples
close to one already chosen.

```sh
daw samples search kick --sort punch --limit 5
daw samples search snare --short --bright
daw samples like samples/45b7888b5150_kick.wav --limit 5
```

## Why

The agent chooses sounds without hearing them. On October 4, 2026, an agent
chose a kick, a snare and a hat for a trial song by their file names. Of the
1,675 samples in the library on this Mac, none had been measured, 635 (38%) were
categorized "other" by their names, and a measured sample would have reported
only its pitch, onsets, tempo and whether it loops. Two kicks with the same name
pattern can be a short click and a long boom. A person hears the difference in a
second; the agent has nothing to tell them apart.

Search by a description with an audio-text model is a separate Later line, to be
built only if it beats names and measurements in a labeled test. This feature is
those measurements.

## Design

**The measurements,** added to `daw samples analyze` and kept in the library's
`analysis` table with its other results. Each is computed on the sound itself,
from its first sound to its last (already found by `analyze`):

| Field | What | Word |
|---|---|---|
| `centroid_hz` | Spectral centroid over the sound, weighted by energy | dark, warm, bright |
| `low_fraction` | Share of power under 120 Hz | thin, full, boomy |
| `attack_ms` | Time from 10% to 90% of the peak envelope | sharp, soft, slow |
| `decay_ms` | Time from the peak to 20 dB under it | short, medium, long |
| `flatness` | Spectral flatness: noise near 1, a tone near 0 | tonal, mixed, noisy |
| `loudness_lufs` | Integrated loudness of the sound, gated as a short file allows | — |
| `punch` | Peak level of the first 30 ms against the next 200 ms | — |

The words come from thresholds per category: a kick's "long" is not a pad's.
The thresholds are set from the library's own distribution per category, and
a new `samples` topic of `daw describe` prints them.

**Measuring the library.** `daw samples analyze --all` already exists. It gains
a progress line on stderr and can be stopped and resumed, since it skips what is
measured. On this Mac one sample took 0.6 s through the command, most of it
start-up, so a batch is the way. `search --measure N` measures the top N matches
that are not yet measured before it sorts them, so a search works on a library
that was never analyzed.

**Search.** `--sort FIELD` and `--reverse`, filters such as `--short`,
`--bright`, `--max-decay 300`, and every result carries its measurements and words.
`samples like FILE` ranks samples of the same category by distance over the
normalized fields.

**Category from the audio.** A sample whose name says nothing ("other") gets
`measured_category` from simple rules where they are clear: a short noisy sound
with no low end is a hat, a short tonal sound under 100 Hz with a falling pitch
is a kick. Where the rules are not clear it stays "other". A trained classifier
is an open question.

## Done when

- Generated test sounds (a short sine kick, a long one, white-noise hats of two
  lengths, a pad) are ordered correctly by each field and get the right words.
- `search --measure 20` on an unmeasured folder sorts by `punch` and caches what
  it measured. `samples like` returns the generated kick's nearest kin first.
- `analyze --all` over the library on this Mac completes, with its time recorded
  in this file when it is rewritten as the feature's reference.

## Open questions

- Whether the per-category thresholds should come from the person's library or
  be fixed numbers that ship with the app. The library's own spread adapts to
  the person; fixed numbers mean the same thing everywhere.
- A classifier for the category, trained on the names of the samples that have
  clear ones, and whether it beats the rules.
- Loops: whether the measurements should be per beat or over the loop.
