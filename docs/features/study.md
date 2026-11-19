# One analysis file — implemented November 18, 2026

Item 6 of the reverse-engineer track (D182, D192): every measure the track built,
run on a song and the parts `daw separate` made of it, kept in one JSON with
each measure's state and confidence and a markdown summary of it, which the
reverse-engineer skill's report and a recreation start from. It adds one
measure of its own, the tuning, which the skill read with a scratch script.

## Command

```sh
uv run daw samples study projects/NAME/samples/ID_song.wav
uv run daw samples study SONG.wav --out projects/NAME/reverse-engineer/NAME
uv run daw samples study SONG.wav --skip words --skip sound
```

`FILE` is a file or a library sample, as for `daw samples form`. The study is
written to `study/NAME/` in the song's project, NAME the sample's ID in the
song (or its file name without the hash), or to `--out DIR`, which a file
outside a project needs. `--skip MEASURE`, repeated, leaves a measure out;
`grid` cannot be, since every other reads it. `--language` is the words'.
A note is printed to stderr as each measure starts. Running it again replaces
what it wrote in the folder, and nothing else there.

## What runs, in order

| Measure | The same as | Writes | Confidence |
|---|---|---|---|
| `grid` | `daw samples beats`, with its corrections; each bar's seconds | | the downbeat's confidence |
| `harmony` | `daw samples chords`, with the key's Camelot number and `chords_sure_share` | | the key's odds, null when not plainly in one |
| `tuning` | new: cents from A 440, on the `other` part or the song | | `concentration` |
| `words` | `daw transcribe --words`, before the form so it names the chorus | `lyrics.txt` | the share of words heard surely |
| `form` | `daw samples form` | `form.png` | null: `named_by`, `like` and `checks` say it |
| `notes` | `daw transcribe` on bass and vocals with `--line`, on other, guitar and piano | `PART.mid` | the share of notes heard surely |
| `drums` | `daw transcribe --drums` on the drums part | `drums.mid` | the share of hits a drum explains |
| `sound` | `daw samples sound` | `sound.png` | null: checked on renders, each eq's `error_db` |
| `master` | `daw listen`'s figures for the song, and each section's loudness | `overview.png`, `spectrum.png` | null: measured to the standard |

A part the form finds 30 dB under the song (`silent_parts`) is not
transcribed. Without parts, the words, notes and drums say that `daw separate`
makes them, and the rest run on the song alone.

## The file

`study.json` holds `kind`, `schema_version` (1), `made`, `song` (path,
sha256, seconds, sample rate, channels), `parts` (the files, the split, the
model, its `residual_db` and the recipe; null with `no_parts` without them),
`measures`, `bars`, `at_a_glance` and the run's `seconds`.

Each measure has `state`: `measured`, `not measured` with `why` (a reason it
could not run, as the command would refuse), `failed` with `error` and
`where` (a fault, kept so the others run on), or `skipped`; `about`;
`same_as`, the command whose reply it holds; `seconds`; and `confidence`,
`value` from 0 to 1, or null where one figure would mislead, and `of`, what
it is. The rest is the command's reply, less what repeats the grid.

`bars` is each bar from bar 1 with what every measure says of it: its
seconds and beats, the form's section and name, its chords (`?` marks a fit
under 0.5), the drums' groove or `fill`, the lines of words that start in it,
a count of notes a part, and the bass notes' names.

`study.md` is the summary a person reads: at a glance, the measures with
their state and confidence, the sections, each section's progression a bar a
cell, the words, the notes, each drum groove as note text, the parts' sound,
the master, the bars table, and what was not measured.

## Tuning

Each frame's 20 loudest partials from 80 Hz to 2 kHz, within 30 dB of the
frame's loudest, are placed between FFT bins (8192 points) by a parabola
through their log magnitudes, and each one's distance from the nearest equal
tempered note on A 440 is taken as an angle on a circle of 100 cents. The
mean of those angles, weighted by level, is `cents` (and `a4_hz`); its length
is `concentration`, from 0 when the partials fall anywhere to 1 when all sit
the same distance from their notes, and `spread_cents` the circular spread.
The `other` part is read when there is one, since drums and a voice's slides
blur it. Tones detuned by 0, +20, −35 and +45 cents read back within 2 cents;
noise reads under 0.2 concentration. On one real song the other part read
−1.1 cents at 0.73, its bass +3.5 at 0.49 and its mix +0.3 at 0.16.

## On one record

A 2:55 rap song at 77 BPM, split with htdemucs_ft, with nobody listening:
all nine measures ran in 2½ minutes on this Mac, the words 42 s of it and
the sound 77 s. The words named the chorus and the form changed with them,
as it should. The file is 1.3 MB, most of it the notes, the hits and the
spectra; the summary 17 kB.

## Limits

- The study reads the newest split of the most parts beside the song, as the
  form does: with a `stems6` split beside a `stems` one, the six parts'.
- It changes nothing in the song: the note clips and the drum track that
  `daw transcribe --project` places, and the sections `daw section add`
  makes, are the skill's steps after it.
- What the skill reads with other commands is not in the file: the stems'
  levels section by section and their resonances and overlaps from `daw
  listen` on a render, `daw samples analyze` on each stem, the hits'
  pictures by band and a lead line looked for with `--range`.
- A correction to the beat map is made with `daw samples beats` and the study
  run again; it does not ask.
