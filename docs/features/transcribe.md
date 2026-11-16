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
2. **One pitch split where it wavers is one note.** The same pitch heard twice
   at once is one note. So is the same pitch starting again within 20 ms of
   where it ended when either piece is shorter than 80 ms, shorter than any
   sixteenth up to 187 BPM (`joined`). A note struck again legato also ends
   where the next one starts, and both are kept: a bass repeating one note in
   eighths stays eighths. Joining every touching piece, as first built, made
   such a chorus one long note.
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

Every written form starts from one MIDI file of the notes in the part's own
beats, 960 ticks a beat, with bar 1 a whole number of bars in: a bar for each
bar of pickup. The Rust `daw` reads it into a scratch song whose bar is the beat
map's, `beats_per_bar` quarter beats, since the map's beats are the song's
beats there whatever the meter is called.

- `--quantize GRID` runs `daw note quantize` there, counted from the part's
  bars, and writes the file again. The listed notes keep their times. It needs
  something to write: `--text`, `--midi` or `--project`.
- `--text` reads the scratch clip back with `daw note list --as text --grid G`
  (`--grid` goes only with `--text`; 1/4 unless given). `from_bar` is the bar
  its first cell is.
- `--midi OUT.mid` is that file, bar 1 at `bar_one_beat`. A folder that is not
  there is refused before anything runs.
- `--project SONG` changes the song last, once, with `daw midi import`, which
  goes through a running host as any edit does. A step that fails before it
  leaves the song as it was, and the clip is one edit to undo.
  - From an audio clip (`tracks.T.audio.N`), only what the clip plays is
    written: from `source_start_seconds` to `source_end_seconds`, the end of
    its loop's first pass, its `length_beats` or the file's end. What lies
    outside is counted in `outside_the_clip`, and a loop says the clip holds
    one pass. Bar 1 lands where the clip plays the file's first downbeat, and
    each beat of the part is as many of the song's beats as the clip plays it
    in (`song_beats_a_beat` when that is not 1). Both are worked out from
    `at`, `source_start_seconds`, `source_bpm` or the song's tempo, and a
    repitched transpose, as the engine plays the clip (aaw-host's `Span::of`).
  - The pitches move by what the clip is heard moved by: its transpose, and
    with `repitch` a `source_bpm` played at the song's tempo, rounded to a
    semitone (a remainder of 0.1 or more is a warning). They move before
    `--range` is checked. A pitch moved past MIDI's 0–127 is left out
    (`outside_midi`).
  - From a sample or a file, bar 1 lands at `--at` (0).
  - The clip starts a bar early when notes come before bar 1. Notes before the
    song's start are left out and counted in `before_the_song`. The clip goes
    on `--track`, or on a new MIDI track with no instrument, named after the
    part.

## Report

`path`, `model` (name, version, device, seconds), `beat_map`, `beat_map_of`
when it is another file's, `tempo_bpm`, `meter`, `first_downbeat_seconds`,
`odd_bars`, `line`, `count`, `sure` (confidence 0.5 or more), `left_out`
(outside_the_clip, joined, not_the_line, outside_midi, outside_range,
under_min_confidence, each when it applies), `lowest` and
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
generated song whose beats are known. They cover placement, joining (a waver,
a doubled note, legato eighths kept), one line
with its octave doubt, range and confidence, text and MIDI with a pickup bar,
a clip placed from an audio clip, transposed by repitching and with
`preserve_pitch`, only what a clip plays, a pitch moved past MIDI, notes
before the song's start, a failing step leaving the song as it was, a map of
six beats a bar, a part placed on its source's beat map, a file with no beat map,
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
