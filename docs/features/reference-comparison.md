# Comparing with a reference — implemented 2026-10-08

Status: implemented (D99). A song the person names as a reference is analyzed
once and kept in the workspace library, and a render is compared with it, the
whole of each and section by section. What it is for is in
[concept.md](../concept.md#library-and-perception): tracks the person points to
as "this is what good sounds like", analyzed once and compared with every mix.

```sh
daw reference add ~/Music/somebody/track.m4a --name club-ref
daw reference sections club-ref                  # the sections it found
daw compare projects/demo/renders/latest.json --reference club-ref
```

## Why

An agent does not hear its mix. `daw listen` measures it, but numbers alone do
not say whether a mix is right: −18 LUFS with 61% of its power in the low band
is a mistake in one genre and the goal in another. A reference turns the
person's taste into targets the agent can measure against and work toward,
section by section, without guessing what "good" means.

On October 4, 2026, an agent's first trial song had its tracks 27 LU apart. The
agent found this from `daw listen`, but had nothing to tell it how far apart they
should be or what the low end should weigh.

## A reference

`daw reference add FILE [--name NAME]` reads the file, a WAV, AIFF or FLAC as it
is and an `.m4a` or `.mp3` decoded as an import decodes it, at 44.1 or 48 kHz. It
measures the whole file as [`daw listen`](perception.md) measures a file, makes
its beat map as [`daw samples beats`](beat-map.md) does, and measures each
section. `--bpm`, `--downbeat` and `--meter` correct the beat map as they do
there. The name is lower-case letters, digits and `-`, the file's name unless
given; a name that is taken is refused without `--replace`.

The result is kept in `library/references/NAME/` in the workspace
(`~/Music/AAW`, or `AAW_WORKSPACE`), beside the patches and the racks:

- `reference.json`: the file's path and SHA-256, its length and format, the
  decoder and the decoded audio's hash for a compressed file, the beat map's
  summary, the whole file's measurements and each section's.
- `beats.json`: every beat of the map, which naming sections in beats reads.

No audio is kept: a decoded copy is made in a temporary folder and removed. A
reference is the person's own file, it stays where it is, and nothing of it
leaves the Mac. A file moved or changed afterwards leaves the reference usable,
since a comparison reads only the measurements; `source.available` says whether
the file is still there.

`daw reference list` names the saved references a line each, `daw reference show
NAME` prints one, and `daw reference remove NAME` forgets one and leaves the
file.

## Sections of a reference

The measured sections are the beat map's phrases: each from its start to the
next, the last to the end of the last beat. A section is named `bar-N` for the
bar it starts on and holds:

- `start_seconds`, `end_seconds`, and `at` and `length_beats` in the
  reference's own beats, where beat 0 is its first downbeat, with `bar`;
- `audio`: what `daw listen` measures of it, but for the true peak, which is
  the whole file's;
- `relative_lu`: its integrated loudness minus the whole file's;
- `label`: `high`, `mid` or `low`, by where its loudness sits between the
  reference's quietest and loudest sections, in thirds at least 1 LU wide.

Phrases are hints and can be a bar off, and a person would often put the
sections elsewhere, so they can be named by hand:

```sh
daw reference sections club-ref '[{"id": "verse", "at": 32}, {"id": "drop", "seconds": "1:02"}]'
daw reference sections club-ref --measured       # back to the phrases
```

Each section takes `at`, in the reference's beats, or `seconds`, as seconds or
`m:ss`, and runs to the next one's start; the last runs to the end of the sound.
Naming sections measures the audio again, so the file must be where it was and
unchanged, and the command says so when it is not. A file no beat map can be
made of, such as one under eight seconds, is still a reference: it has no
measured sections, is compared whole, and takes sections in seconds, and
`beat_map_problem` says why.

## Comparing

`daw compare RENDER --reference NAME` takes what `daw listen` takes: a render's
folder, report or pointer, or any audio file. A render brings its song's
sections; a plain file has none. Every delta is **mix minus reference**.

**Matching sections.** `matching.sections` says how:

- `id`: the two share section IDs, and each shared ID is a pair. Giving the
  reference's sections the song's IDs is how the pairs are chosen.
- `loudness`: no ID is shared. Each side's sections are put in order of
  integrated loudness, and the song's loudest is compared with the reference's
  loudest, its quietest with the quietest, and those between spread evenly over
  the reference's order. This is a guess at roles, and the report names each
  pair so that it can be read as one.
- `none`: a side has no sections with a loudness, and only the whole files are
  compared.

`unmatched_sections` lists what was left out on each side.

**What is compared.** `whole`, and each pair under `sections`, hold `mix`,
`reference` and `delta`:

| Field | Meaning |
|---|---|
| `integrated_lufs` | Loudness, and its difference in LU |
| `estimated_true_peak_dbtp` | The whole files only |
| `relative_lu` | Sections only: how far the section sits over or under its own file's loudness, so its delta says whether the drop lifts as far over the song as the reference's does |
| `crest_db` | Peak over average: how dynamic or squashed |
| `stereo_correlation`, `side_energy_fraction` | The stereo field |
| `band_fraction` | Each of `daw listen`'s seven bands' share of the power, and the change in share |
| `delta.band_db` | The tonal balance: each band's level, mix minus reference, with the median of the seven differences taken out |

`band_db` is the number to act on. A mix that is louder or quieter all over
reads as 0 in every band, and a mix whose sub alone is 6 dB up reads `sub: 6` and
the rest 0, which is the gain an equalizer or a fader would take. It is `null`
in a band where either side holds under −70 dB of its power, and an observation
says which side lacks the band.

`contour` holds how far apart the quietest and loudest sections are on each
side, `mix_span_lu` and `reference_span_lu`.

**Observations.** `observations` are the differences past a threshold, in
words, the largest first by how many times over its threshold each is:

```text
sub (20–60 Hz): 6.0 dB above the reference's balance over the whole mix; by section, drop +6.0 dB, verse +6.0 dB.
loudness: the mix is 5.5 LU quieter than club-ref, -27.6 against -22.1 LUFS.
contour: the mix's sections are within 1.0 LU of each other, and club-ref's within 4.2 LU.
```

The thresholds are 1 LU of loudness, 1 dB of true peak, 3 dB of a band, 2 dB of
crest, 5 points of side energy, 1.5 LU of a section's relative loudness and
2 LU of span. A line names a measurement once, over the whole mix and then by
section, up to four sections. They say which way and by how much, not what to
do: a mix can differ from its reference on purpose.

The report is printed and written to `analysis/ID/reference-NAME.json` in the
render's folder, or beside a file that is not a render. Nothing else is written
outside the workspace, and no picture is made; `--no-images` is accepted and
changes nothing.

## Time

On this Mac, an Intel one of eight cores, a three-minute `.m4a` is added in
about six seconds and a three-minute render of six sections compared in about
four, measured through `uv run daw` on a generated song; the design asked for
under ten and under five. To get there, one measurement's spectrum, each
channel's oversampled peak and the stereo figures are worked out on threads
beside its loudness, the sections are measured several at a time, and a
section's true peak is not measured. The first of these also halves a
measurement in `daw listen`; its numbers are the same to the last bit.

## Limits

- Tracks are not compared: a reference has no stems. Stem separation is a
  backlog line.
- Keys, notes and arrangement are not compared, and a pair of sections is
  compared as two wholes, not bar by bar. The two can be at any tempo and length.
- The bands are `daw listen`'s seven; third octaves and a tilt wait for
  [a finer spectrum](spectrum-detail.md).
- A beat map is always tried, and music without a steady beat can get one that
  means little, with phrases to match; name its sections in seconds.
- The thresholds, the matching by loudness and the labels were tried on
  generated songs only, not on real ones.
- It is not a judge of quality.

`daw describe reference` says what each field means.
