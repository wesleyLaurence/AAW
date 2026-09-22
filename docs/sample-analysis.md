# Sample analysis — implemented September 22, 2026

`daw samples analyze` measures a sample's pitch, onsets, tempo and loop/one-shot
kind from its decoded audio, not its filename. Filename hints are only echoed for
comparison. The results are estimates to check before composing, not ground truth.
All computation is local and uses only numpy/scipy; no model or network call is made.

## Commands

```sh
uv run daw samples analyze SAMPLE_ID          # or a file path
uv run daw samples analyze --all              # every indexed sample; incremental
uv run daw samples search --pitched --note-range C1-B1
uv run daw samples search --measured-type loop --measured-bpm 128
uv run daw samples import BASS_ID --project projects/my-beat/song.yaml --id sub --root-note auto
uv run daw check projects/my-beat/song.yaml   # includes root_notes and warnings
```

Results for indexed files are cached in the `analysis` table of the sample index.
A cached result is reused only while the file's size, mtime and the analyzer code
hash (`analyzer`) are unchanged. `--refresh` recomputes it. Files outside the index can
be analyzed but are not cached. Measured search filters match only samples that
have a current analysis. Run `analyze --all` after `scan` to include the whole
library. Search rows carry a `measured` object, or `null` when a sample has not
been analyzed.

## Report

- `level`: `first_sound_seconds`, `last_sound_seconds`, `trailing_silence_seconds`
  and `peak_dbfs`. Sound means samples within 50 dB of the file's peak. A late
  first sound suggests a `start_seconds` trim on the pad.
- `stereo`: channel correlation and side-energy fraction (stereo files only).
- `pitch`: monophonic YIN estimate over the sound's first 10 seconds, using frames
  within 30 dB of the loudest. `note` includes the octave. `cents` is the offset
  from that equal-tempered note (A4 = 440 Hz). `midi` is fractional. `spread_cents`
  is the interquartile range across frames. `voiced_fraction` is the share of frames
  that are periodic. `fundamental_support_db` is the strongest level near f0 or
  2×f0 relative to the spectrum's peak.
  `pitched` requires at least half the frames to be periodic and a
  confidence of at least 0.4. A periodic signal with no energy near its apparent
  fundamental (typical of chords) has its confidence reduced and is usually
  reported unpitched rather than two octaves low.
- `rhythm`: spectral-flux `onsets_seconds` (the first 64 are listed) and an
  `onset_count`. `kind` is `one_shot`, `loop` or `uncertain`, and `kind_reason`
  says why. `tempo` is reported for loops only:
  - Tempo candidates come from the file length (a whole number of 4/4 bars, from
    1 to 128) and from onset periodicity, including half and double time.
  - Each candidate is scored by how many onsets land on its sixteenth-note grid.
    Length-derived candidates assume the loop starts on a downbeat.
  - Ties favour 85–175 BPM, then an exact whole-bar length, then periodicity.
  - `ambiguous_with` lists other tempos, usually half or double time, that fit the
    onsets almost equally well. Choose among them musically.
  - `filename_bpm_hint` says whether the filename tempo agrees and whether it
    implies a whole number of bars.

## Root notes

`import --root-note auto` writes the measured note as the sample's `root_note`. If
no reliable pitch is found it fails, and you must pass the note explicitly. When a
sample is more than 10 cents off its note, the result also includes a
`suggested_pad_transpose` in semitones. Put it on the pad to play in tune.

`daw check` measures every sample that declares a `root_note` and reports one
status per sample under `root_notes`:

- `ok`: within 30 cents of the declared note.
- `detuned`: nearest to the declared note but more than 30 cents off. Includes
  `suggested_pad_transpose`.
- `octave_mismatch`: the same pitch class in a different octave.
- `note_mismatch`: a different note.
- `unverified`: no reliable pitch was measured, as with noise, chords or heavy
  modulation.

Any status other than `ok` also appears in `warnings`. Warnings do not make a
project invalid. The declared note stays authoritative, because a deliberately
different root is sometimes wanted.

## Limits

Pitch is a single-voice estimate. It does not detect keys, chords, scales or
melodies, and it summarizes a glide as its median. Tempo assumes 4/4 and a steady
tempo. It does not detect downbeats, swing, or pickups before the first downbeat.
Half and double time cannot be resolved from audio alone. Very short or quiet
material may return no pitch or no onsets. Analysis never modifies the sample,
the project or the original library.

Tests use generated audio: harmonic tones, a weak fundamental, an 808-style pitch
drop, a detuned tone, stereo input, noise, a chord, silence, and drum loops at 90,
128 and 174 BPM, including one not cut to whole bars. They also cover cache
invalidation, measured search, `--root-note auto`, and `check` warnings.
