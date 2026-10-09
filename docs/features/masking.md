# Overlap between stems — implemented 2026-10-08

Status: implemented (D100). `daw listen` says where two stems put their energy
in the same band at the same moments, and `daw compare` says how an edit
changed it. What it is for is in
[concept.md](../concept.md#library-and-perception): a perception report that
says "kick and bass overlap heavily from 60 to 120 Hz" as an observation.

```sh
daw listen projects/demo/renders/latest.json                      # the pairs, under "overlap"
daw listen projects/demo/renders/latest.json --overlap kick bass  # one pair, by section and band
daw compare BEFORE AFTER                                          # each pair's change
```

```json
"overlap": {
  "cell_beats": 0.25,
  "pairs": [
    {"a": "kick", "b": "bass", "band_hz": [45, 89],
     "fraction": 0.25, "fraction_of_a": 1.0, "fraction_of_b": 0.25,
     "contested_beats": 8.0, "level_dbfs": -11.62, "level_difference_db": -1.12,
     "keyed": false,
     "worst": [{"section": "drop", "at_beat": 16.0, "length_beats": 16.0, "fraction": 0.25}]}
  ],
  "pairs_omitted": 0
}
```

## Why

Deciding which of two sounds owns a band is most of mixing: the kick or the
bass under 100 Hz, the pad or the lead around 2 kHz. A person hears the
contest. The agent has every stem and each stem's level in seven bands over a
section, which says that the kick and the bass are both loud in `low` and not
whether they are loud at the same moments. A bass that plays between the kicks
and one that plays on them read the same there.

`daw check`'s `register-crowded` reads the notes: two tracks written in one low
octave. This reads the audio, so it sees a sample's real spectrum, a filter, a
sidechain's duck and a reverb's tail.

## What is measured

**Overlap, not masking.** Two stems overlap in a cell, a band over a short
time, when both have energy there at close levels. Overlap is what masking
needs and is not masking as an ear does it: nothing here spreads a loud band
over its neighbors as a hearing model does, tells a tone from noise or changes
with the playback level. The report says "overlap" for that reason, and it does
not say which of the two should give way.

**Cells.** Each stem's power on a grid of bands by time, on the song's own
grid. Time is a sixteenth note, or the next note up that lasts 100 ms: an
eighth above 150 BPM. `cell_beats` says which. The 26 bands are the third
octaves of [`spectrum_db`](spectrum-detail.md) from 100 Hz to 20 kHz, the top
one ending at 20 kHz, and two an octave wide below, 20–45 Hz and 45–89 Hz,
since a window that short cannot tell third octaves apart there. Edges are printed to three
figures. Channel powers are averaged, as `band_dbfs` does it.

A cell is read through two windows a cell long, centered an eighth and five
eighths of the way through it. They sit early so that a hit on the grid falls
in its own cell and 15 dB down in the one before, and a note that stops on the
grid reads 10 dB down in the one after.

**The mix a stem is heard against is the stems' sum:** the ungrouped tracks,
the groups and the returns added up, which is the mix before the master's
effects. A limiter or an equalizer on the master does not come into it.

**A contested cell.** Stems A and B contest a cell when

- the weaker is within 6 dB of the stronger,
- the weaker is within 9 dB of the sum in that cell, so the pair is what is
  heard there, and
- the cell counts: the sum there is within 40 dB of its band's loudest cell and
  within 70 dB of the loudest cell in any band, and is not the skirt of a
  louder band.

A skirt is a cell 12 dB or more under the band beside it at that moment, or
that and 4 dB more for each band further off. A struck low tone leaves energy
40 dB down for octaves above it, which is the edge of that tone and not a sound
of its own; without the guard a kick's and a bass's skirts read as a contest
from 100 to 500 Hz that nobody hears. It is a guard against that reading and
not a model of masking.

A stem **sounds** in a cell when it is within 9 dB of the sum in a cell that
counts.

## A pair

A pair is named for a range of bands: bands next to each other that are each
contested are one range, `band_hz`. Over that range a moment is contested when
any of its bands is.

| Field | Meaning |
|---|---|
| `a`, `b` | The stems, in the render's order |
| `band_hz` | The range the pair contests most of the time; `other_bands_hz` lists any other ranges |
| `fraction` | The contested moments over those where either stem sounds in the range |
| `fraction_of_a`, `fraction_of_b` | The contested moments over those where that stem sounds |
| `contested_beats` | For how long in all |
| `level_dbfs` | The level of the stems' sum over the contested cells, so a contest at −12 dBFS is told from one at −50 |
| `level_difference_db` | A minus B over the contested cells |
| `keyed` | Whether one has a compressor keyed by the other, read from the song. The stems are post-insert, so a duck is already in the audio and lowers the fraction; `keyed` says why |
| `worst` | Up to three places where the fraction is highest, each with `at_beat`, `length_beats` and `fraction`: the song's sections, named, or in a song without sections the runs of bars at least as contested as the whole |

The two fractions of one stem are what a sparse sound against a held one needs.
A kick that hits each beat under a held bass, at about its level, reads
`fraction` 0.25 and `fraction_of_a` 1: a quarter of the time the bass sounds,
and every time the kick does.

A band is named when it is contested 0.25 of the time either stem sounds in it,
or 0.5 of the time one of them does, for 4 beats or more in all.

**How much is printed.** The five pairs with the highest `fraction`, each once,
and `pairs_omitted`. `overlap` is `null` for a render of one stem and for a
file that is not a render.

**Which stems.** Tracks, groups and returns, as the render writes them: a
track's stem is its sound before its group and before the master's effects.
Left out, since one's sound is the other's: a group against its own tracks,
and a return against the tracks that feed it and their group. Two tracks of
one group are a pair. A group's pair with a stem outside it is left out of the
list where one of its tracks' pairs with that stem names the same bands, since
the track's says more.

## One pair

`daw listen RENDER --overlap A B` prints one pair in place of the report, and
takes any two stems, the pairs the list leaves out too (`related` says so):

- `ranges`: every range the pair contests, as above, each with `sections`, its
  fraction in each section;
- `bands_hz`: the 26 bands;
- `whole`: for each band, `a_dbfs` and `b_dbfs`, each stem's level over the
  render, and `fraction`, `fraction_of_a`, `fraction_of_b` and
  `level_difference_db`;
- `sections`: each with `at_beat`, `length_beats` and `fraction` in each band.

A fraction is `null` where neither stem sounds. It writes `overlap-A-B.json`
and `overlap-A-B.png` under `analysis/` in the render's folder: bands up,
beats across, each cell in the color of the stem that is louder there, and
dark where the two contest it. `--no-images` leaves the picture out.

## Comparing

`daw compare BEFORE AFTER` has `overlap.pairs`: each pair either render's list
names, over the bands it was named for, with `before`, `after` and `delta`,
`keyed_before` and `keyed_after`, and the same three numbers for each section
where the two renders line up. The largest change is first. A stem one render
lacks gives `null`. A cut at 80 Hz on the bass, a keyed compressor, or a part
moved off the other's hits shows as the fraction falling.

## Time

On this Mac, an Intel one of eight cores, overlap adds about two seconds to
`daw listen` of a generated three-minute song of twelve stems, which takes
about forty in all, and `--overlap A B` of that song takes about eight. The
design asked for under five added.

## Limits

- The thresholds, 6, 9, 40, 70 and 12 dB, and the fractions a pair is named
  at, were chosen on generated songs and read against one real render of
  fourteen stems. Nobody has listened: whether the pairs named are the ones a
  person hears is under Verify.
- It is not masking. A stem far under another in a band is covered, not
  contested, and is not in the list; each stem's `band_dbfs` shows that. A
  model with spreading between bands is a backlog line.
- A grouped track is read before its group's effects and fader, so a group's
  equalizer that clears a band does not show in its tracks' pairs. The group's
  own pair does show it.
- The two bands under 89 Hz are an octave wide, where the kick and the bass
  meet. Longer windows would split them at the cost of time.
- A layered sound, such as a kick and its click, is contested by design, and
  the list cannot know.
- `daw check` does not read a render, so it has no code for the worst pair.

`daw describe listen` says what each field means.
