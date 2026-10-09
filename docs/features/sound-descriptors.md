# Sound descriptors — implemented October 9, 2026

`daw samples analyze` measures what a sample sounds like in seven numbers, with
a word beside five of them: how bright it is, how much low end it has, how fast
it starts, how long it rings, how noisy it is, how loud and how hard it hits.
`daw samples search` sorts and filters by them, and `daw samples like` lists the
samples nearest a chosen one. The agent chooses sounds by these and not by their
names alone. They are measurements: nothing is heard, and a choice is still for
the person to hear. `daw describe samples` is the agent's copy of this file.

```sh
uv run daw samples analyze --all                       # once for the library
uv run daw samples search kick --sort punch --limit 5
uv run daw samples search snare --short --bright
uv run daw samples search --category hat --max-decay 60 --sort loudness
uv run daw samples like projects/my-beat/samples/45b7888b5150_kick.wav --limit 5
```

## The numbers

Each is of the sound itself, from the first sample within 50 dB of the file's
peak to the last, in the first two minutes of the file. They are in a sample's
analysis under `sound`, and in a search row under `measured`.

| Field | What it is | Words |
|---|---|---|
| `centroid_hz` | Where the middle of the power lies, from 20 Hz to 20 kHz, in the mean spectrum of 85 ms windows | dark, warm, bright |
| `low_fraction` | The share of that power under 120 Hz, 0 to 1 | thin, full, boomy |
| `attack_ms` | The time from first reaching a tenth of the sound's level to first reaching nine tenths | sharp, soft, slow |
| `decay_ms` | The time from there to the last moment within 20 dB of the level | short, medium, long |
| `noisiness` | 0 for a tone, near 1 for noise | tonal, mixed, noisy |
| `loudness_lufs` | The loudest 400 ms, as BS.1770 weighs it; a shorter sound ends in silence | — |
| `punch_db` | The peak of the first 30 ms against the RMS of the 200 ms after them, never past 60 | — |

**The level** a sound rises to and falls from is its peak, or where that is
lower the peak a tone as strong as its loudest 10 ms would have, which is 3 dB
over their RMS. Noise's highest sample is a chance and can be anywhere in it:
by the peak alone, two seconds of steady noise took a tenth of a second to
start. A sound that is cut before it falls 20 dB reads its length from the end
of its attack as its decay.

**Noisiness** holds each 40 ms of the sound against the stretch that starts
from 2 to 40 ms later. What it shares with the one most like it is 1 for a
sound that repeats, at any pitch from 25 Hz up, and near 0 for noise; the
mean of those over the sound's first ten seconds, by the energy in each, is
taken from 1. The design asked for spectral flatness. That says how much of the
spectrum a sound fills: by it a sine kick and a pure tone read alike, a hat
reads less noisy the higher it is cut, and read an octave at a time it put
kicks above hats. On the library below the medians by name are 0.02 for an
808, 0.06 for a bass, 0.43 for a kick, 0.69 for a snare, 0.79 for a hat and
0.83 for a clap.

**Loudness** is the analyzer's meter over the file's own channels
(`aaw_py.loudness`), as `daw listen` reads `max_momentary_lufs`. The difference
between two samples' is the gain that matches them. One channel is measured as
one, so the same sound on two reads 3 dB more.

**Punch** is what `daw listen` gives a hit (`dynamics.punch`), with silence
where the sound has ended. A test holds a sample's to the punch of the same
sample played alone in a song.

A loop has many hits, so its `attack_ms`, `decay_ms` and `punch_db` are null and
its other numbers are of the whole loop. Silence has none.

## The words

A word says where a number lies among samples of the same category, which
`words_among` names: a long kick rings for 150 ms and a long 808 for two and a
half seconds. Each field has two lines; a number under the first takes the first
word and one at or over the second the third. The lines are fixed numbers in
`src/agent_daw/sample_words.json`, which the Rust model reads too, so
`daw describe samples` prints them under `lines`:

- `any`, for every sample: the thirds of the 1,318 samples of the library on
  this Mac that are not loops, rounded. 300 and 1500 Hz, 0.05 and 0.5 of low
  end, 3 and 100 ms of attack, 350 and 4000 ms of decay, 0.25 and 0.6 of
  noisiness.
- A category's own, for the fields where the thirds of its samples lie far
  enough apart to tell by ear: a quarter in brightness, 0.05 of low end, half
  again and 5 ms in attack, 30% in decay, 0.1 in noisiness. The 808, bass,
  clap, hat, kick, percussion, snare and vocal samples have some, from at
  least 30 samples each. `fx` and `other` are of no one kind of sound and are
  read against `any`, as is every field a category has no line for: every hat
  is thin and every 808 dark.

The lines were set on October 9, 2026 and do not follow the library, so a word
means the same in every session and in a skill.

## Category from the audio

A file's `category` is read from its name and is `other` when the name says
nothing. `measured.category` is what a single sound measures as where that is
clear, and null otherwise:

- **hat**: decays in under 400 ms, noisiness of 0.5 or more, a centroid of
  5 kHz or more, under 1% of low end, an attack under 20 ms.
- **kick**: decays in under 400 ms, half its power or more under 120 Hz, a
  centroid under 300 Hz, an attack under 15 ms, a punch of 4 dB or more.

`--category C` finds the samples named C and those named for nothing that
measure as C, whose words are then C's. The app's browser searches through the
same command, so its category menu lists them too.

## Search

`--sort FIELD` puts the most first: `duration`, `centroid`, `low`, `attack`,
`decay`, `noisiness`, `loudness` or `punch`. `--reverse` turns the order round,
by name too. A word as a flag keeps the samples it is true of, and
`--min-FIELD X` and `--max-FIELD X` those whose number lies between.

A sort or a filter by a measurement lists measured samples only. It says on
stderr how many matches it left out, as `{"note": ...}`, and is refused when it
would leave out every match, with what to run. `--measure N` measures the
first N of them first and keeps what it measured.

## Like

`daw samples like SAMPLE` takes an ID, a path, or a file in a project, which is
measured there and then. It is compared with the measured samples of its
category: `--category C`, or the one its name says, or the one it measures as,
and every sample when it has none or with `--category any`. A loop is compared
with loops and a single sound with single sounds.

`distance` is in steps: the root of the mean square of the differences over the
numbers both samples have. A step is half an octave of `centroid_hz`, 0.15 of
`low_fraction`, a doubling of `attack_ms` plus 1 or of `decay_ms` plus 10, 0.15
of `noisiness` and 6 dB of `punch_db`, about how far a category's samples
spread in each. Loudness is left out, since a gain changes it, and so is pitch.
The result names what it compared `among`, how many it `compared`, and how many
were `not_measured`.

## Measuring the library

`daw samples analyze --all` measures every indexed sample that has no current
measurement, in up to eight processes, keeps each as it is done and says how
far it is on stderr every two seconds. A run that is stopped keeps what it had
measured, and the next one goes on from there. The 1,675 samples on this Mac, 357 of them
loops and 129 over thirty seconds long, took about three minutes (172 s and
187 s in two runs); a search over them then takes 0.2 s. A measurement is kept until the file changes or the measuring
code does (`analysis.py`, `descriptors.py`, or what a punch is).

## What it found on a real library

The library is 1,675 files from bought sample packs. Before this, 31 were
measured and 635 were `other` by their names.

- The two rules chose 91 samples as hats and 84 as kicks. Of those with a
  category in their names, 75 of 81 and 78 of 83 had the same one there; the
  rest were percussion, a snare, a clap, low impacts and bass hits. Of the 635 named for
  nothing, ten were found to be hats and one a kick: most of them are long
  sounds of no one kind.
- A rule for an 808, a long low tone, agreed with the name for 49 of 70, the
  others being sub basses and drops. A rule for a snare or a clap could not
  tell the two apart, and chose rims and key clicks. Neither is built.
- `search kick --sort punch` put five kicks of a tenth of a second first;
  `like` of the first listed four kicks within 0.4 of a step, and `like` of an
  open hat listed three open hats first, in its category and among every
  sample.

Nobody heard any of these.

## Limits

- The numbers do not say what a sound is: two samples a step apart can be a
  snare and a rim.
- A click before a slow swell has one attack, the click's if it reaches nine
  tenths of the level and the swell's if not, and a sound with an echo has a
  long decay.
- A fast sweep is not like itself a moment later, so a short kick that is
  mostly its sweep reads as noisy: among kicks, noisiness says how little of
  the kick is a settled tone. Noise held to a narrow band, or a rumble under
  100 Hz, reads as partly a tone.
- The centroid, the low end and noisiness are of the channels' sum; attack,
  decay, loudness and punch are of the file's channels, and loudness reads a
  file of more than two as their sum.
- The lines between the words came from one library of bought packs, and may
  sit elsewhere for a library of another kind.
- A sample named for nothing can only be found by `--category` once it is
  measured.

Tests use generated sounds: two sine kicks, two noise hats, a pad, steady and
highpassed noise, tones and a sawtooth, a drum loop and silence, in
`tests/test_descriptors.py`.
