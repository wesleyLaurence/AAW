# Notes from audio — implemented November 16, 2026

Next item 1 of the reverse-engineer track (D182, D183): the notes a part plays,
heard from its audio, as MIDI on the song's beat map. `daw transcribe` gives
them as a list, as note text, as a MIDI file and as a note clip in a song. The
bass line, a melody and the chords' voicings come from the stems `daw separate`
makes. Spotify's Basic Pitch hears the notes (Bittner, Bosch, Rubinstein,
Meseguer-Brocal and Ewert, "A Lightweight Instrument-Agnostic Model for
Polyphonic Note Transcription and Multipitch Estimation", ICASSP 2022;
Apache-2.0, code and weights). It is polyphonic, works on any instrument and is
best on one at a time. It runs on the Mac, and no audio leaves it.

## Setup

`daw separate setup` installs Basic Pitch 0.3.0 into the separation
environment, beside Beat This!. Its weights come inside its package from PyPI,
so there is nothing else to download. The package is installed without its
dependencies. They would bring TensorFlow and numpy 1.26, which would replace
the environment's numpy 2.5. What its imports need is listed in the manifest's
`also` and installed with the rest: mido, mir-eval, pretty-midi and resampy.
The model runs as its ONNX file through the environment's onnxruntime, on the
processor. An environment made before this gets the five packages the next time
setup runs, and nothing else in it changes. `daw separate models` lists Basic
Pitch under `analysis` with its license and whether it is installed. Without
it, `daw transcribe` says to run `daw separate setup`.

## How the notes are made

1. **The model hears the part.** `transcribe_runner.py` runs Basic Pitch in the
   separation environment with its own default thresholds (onset 0.5, frame
   0.3). The shortest note it keeps is 58 ms, not its default of 127.7 ms,
   which drops a staccato sixteenth at 117 BPM. It prints one line of JSON.
   Each note has its start and end in seconds and its pitch. It also has the
   model's mean note activation over the note, as the note's confidence, and
   how far its pitch moves around its middle, in steps of 33 cents.
   Basic Pitch's pitch bends call `scipy.signal.gaussian`, which newer scipy
   keeps only under `scipy.signal.windows`, so the runner puts it back.
2. **One pitch split where it wavers is one note.** The same pitch starting
   again within 20 ms of where it ended, or while it still sounds, is joined to
   it (`joined`).
3. **One line, when asked.** `--line` keeps one note at a time, for a bass
   line or a lead. Notes starting within 50 ms of each other keep the surest.
   An octave of it at least 0.75 as sure is named beside it as
   `octave_doubt`. A weaker note an octave or a semitone from a note that
   still sounds is dropped. Any other note ends the one before it.
4. **Velocity from the part's level.** A note's velocity comes from the part's
   level over its first 50 ms, against the loudest note's: 127 at the loudest,
   1 at 40 dB under, with `level_db` beside it. The model's activation says
   how sure it is, not how hard the note was played.
5. **On the beat map.** Each note is placed as `daw samples hits` places a hit:
   bar, beat, nearest sixteenth and `off_grid_ms`. It also gets `beats` from
   bar 1 and `length_beats`, which a clip, a MIDI file and note text are
   written in. A part made by `daw separate` has its source's length to the
   frame. The recipe beside the part names that source, and the part is
   placed on the source song's beat map, measured on the whole song
   (`beat_map_of`). `--beats FILE` names another file.

## Writing them

Every written form goes through one MIDI file of the notes, 960 ticks a beat,
read by the Rust `daw`:

- `--text` imports that file into a scratch song, with the beat map's meter,
  and reads it back with `daw note list --as text --grid G`. The text starts a
  bar before bar 1 for each bar of pickup; `from_bar` says which bar.
- `--midi OUT.mid` exports the clip as a type 0 file, with bar 1 at
  `bar_one_beat`.
- `--quantize GRID` runs `daw note quantize` on the clip before either. The
  listed notes keep their times.
- `--project SONG` adds the clip to the song with `daw midi import`, which
  goes through a running host as any edit does. From an audio clip
  (`tracks.T.audio.N`), bar 1 lands where that clip plays the file's first
  downbeat: its `at`, plus the time since `source_start_seconds` at its
  `source_bpm` or the song's tempo, repitched by its transpose. The clip's
  transpose, rounded, is added to the pitches. From a sample or a file, bar 1
  lands at `--at` (0). The clip starts a bar early when notes come before bar 1.
  Notes before the song's start are left out and counted. A clip that plays the
  part at another tempo than its beat map says so in `warnings`. The clip goes
  on `--track`, or on a new MIDI track with no instrument named after the part.

## Report

`path`, `model` (name, version, device, seconds), `beat_map`, `beat_map_of`
when it is another file's, `tempo_bpm`, `meter`, `first_downbeat_seconds`,
`odd_bars`, `line`, `count`, `sure` (confidence 0.5 or more), `left_out`
(joined, not_the_line, outside_range, under_min_confidence), `lowest` and
`highest`, then `clip`, `midi` and `text` when asked, and `notes`.
`--near TIME --window S` lists the notes around a time. `--min-confidence`
leaves out notes under it; by default none are left out. `--range LOW-HIGH`
keeps a range. A file too short for a beat map gets its notes in seconds,
with `no_beat_map`, and cannot be written. `daw describe beats.transcribe`
is the contract.

## What was checked

The trial, on the day it was built: Basic Pitch 0.3.0 installs on Python 3.12
on Apple silicon with its dependencies, which put TensorFlow 2.16 and numpy
1.26 in a 1.4 GB environment. It also installs without them beside the
separation environment's numpy 2.5, scipy 1.18, librosa 1.0 and onnxruntime
1.31, the versions here, and runs its ONNX model there. On a 4:54 pop record
separated with `--split stems --quality best`, each stem took 3 to 5 s:

- **Bass.** The riff came out bar after bar as the record plays it, F♯ C♯ E F♯
  E C♯ B C♯ in eighths, and moved to B where the record does. The faults were
  the octave: the model often gave F♯1 and F♯2 together, at nearly the same
  activation (0.48 and 0.46), with the ghosts mostly at 0.34 to 0.38 against
  0.45 to 0.66 for the real notes. That is why `--line` and `octave_doubt`
  exist. Onsets sat a median 15 ms after the beat map's sixteenths. Moving
  each start to the stem's attack within 45 ms did not tighten them and was
  left out.
- **Other.** The keyboard's chords came out as their upper voicings, C♯4+F♯4,
  D♯4+G♯4, E4+A4 and D♯4+G♯4+B3: the record's F♯m, G♯m and A. Chord tones sat
  as low as 0.35, so no confidence is cut by default. Onset activation,
  tried first as the confidence, was 0.1 to 0.4 even on the riff's real notes.
- **Vocals.** The voice came out in short pieces a semitone apart where it
  slid, as the model's README warns. It is the least usable of the three.

Tests run with a stand-in model (`tests/stand_in_transcriber.py`) on a
generated song whose beats are known. They cover placement, joining, one line
with its octave doubt, range and confidence, text and MIDI with a pickup bar,
a clip placed from an audio clip with and without transpose, notes before the
song's start, a part placed on its source's beat map, a file with no beat map,
a failing and a missing model, argument checks, the listing and the setup
step. Nobody has heard a transcribed part played back.

## Limits

- Notes, not sounds: a note clip plays on whatever instrument it is given.
- Pitch bends are reported and not written; a note clip holds none.
- A dense "other" stem gives its top voices more surely than its inner ones.
  A guitar or piano stem from `--split stems6` is the better source for
  voicings, and that has not been tried.
- The bars are the beat map's, odd bars included. A note clip in a song of one
  meter places the notes by beats, so after an odd bar the song's bar lines
  and the part's differ.
- Drums are the next item of the reverse-engineer track, built from `daw samples hits`.
