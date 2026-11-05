# An audio clip's pitch — implemented November 4, 2026

Asked for by the person on November 4, 2026 (D173): to click an audio clip in
the app and move its pitch in semitones, with cents for finer steps, in a
mode that either moves pitch and speed together or keeps the clip's length,
as Ableton's Transpose, Detune and warp modes do.

## What

- **`transpose`** on an audio clip moves it −48 to 48 semitones. A fraction
  is part of a semitone: 0.5 is a quarter tone, 0.1 is 10 cents, −2.25 is two
  semitones and a quarter down. It is left out of the saved file at 0.
- **`stretch`** says how it moves, as it already said how a clip follows the
  song's tempo:

  | `stretch` | In the app | A transpose | With `source_bpm` |
  |---|---|---|---|
  | `repitch`, the default | Repitch | Plays faster or slower, so a clip moved up is shorter on the timeline, as a record sped up | Faster and higher, as before |
  | `preserve_pitch` | Stretch | Moves the pitch and keeps the length | Follows the tempo at its own pitch, as before |
  | `preserve_formants` | Stretch voice | As Stretch, keeping a voice's formants where they are, so a voice moved up does not sound small | As Stretch |

  Neither of the two stretching modes needs `source_bpm` for a transpose.
  `preserve_formants` is an audio clip's only; a pad has the first two.
- **In the app**, the Audio Clip panel's Pitch row has two bars side by side,
  the transpose in whole semitones and the detune in cents from −50 to 50,
  and its Mode row the three modes above. The bars are as Gain is: dragged,
  typed or double-clicked back to 0, each one undo step. They write the one
  field: +3 st and −20 ct is 2.8. A transpose of 2.6 shows as +3 st and
  −40 ct. Mode is there with or without a tempo.
- **From the terminal:**

  ```sh
  uv run daw set SONG tracks.vox.audio.0.transpose -2
  uv run daw set SONG tracks.vox.audio.0.stretch preserve_formants
  uv run daw audio add SONG vox take --at 16 --transpose 0.5 --stretch preserve_pitch
  ```

  `daw map` writes a transposed clip's move after its length, `transposed
  +3 st`, and the same audio at another pitch is other music with its own
  letter.

## How it works

- **Repitch** is a pad's transpose: the clip's audio is resampled when it is
  prepared, `2^(transpose/12)` times as fast. `AudioClip::seconds_per_beat`
  includes it, so the clip's length, its lead, its tail, a split, a trim, a
  loop's wrap and the song's length all follow from one place in the model;
  the engine, the app's view, the map, the checks and Python's timeline read
  it from there or compute it the same way.
- **Stretch** hands the move to the song's stretcher with the tempo's ratio,
  when the audio is prepared, off the audio thread, and cached by what it was
  made from, as a stretch for the tempo is ([time-stretch.md](time-stretch.md)):
  Signalsmith Stretch's transpose factor, with formant compensation for
  Stretch voice, or Rubber Band's `--pitch`, with `--formant`. The 0.2 s of
  file either side is stretched with it and cut off, so a join between two
  transposed clips is moved in context. Playback equals the render.
- A clip moved by the stretcher names the stretcher and its version in the
  render's report, as a stretched pad does, so a render made with another
  version has another identity. A song with no transpose has the identity it
  had before; the new field has a legacy fingerprint form.
- `daw check`'s `stretch-without-tempo` no longer warns about a clip in a
  stretching mode without `source_bpm` when it is transposed: the stretcher
  has a move to make.
- A split and `daw range` keep the transpose on both halves, and `clip join`
  joins two halves only when their transposes are the same.

## Checked

- The stretcher's tests move a 440 Hz tone 7 semitones, −3.5 with formants,
  and 0.3 at a 4% stretch, with each stretcher: the tone lands within 6 Hz of
  where the semitones put it, the length is the stretch's, and the same audio
  moves the same each time.
- Engine tests: a clip transposed an octave by repitching is half as long and
  an octave up; by stretching it keeps its length and goes an octave up.
- The app's test of how a transpose shows as semitones and cents, and
  scripted runs of the app on a generated song: a clip at 2.8 in Stretch
  voice drawn at its eight seconds with +3 st and −20 ct; the semitone bar
  dragged, the song read back at −20.2; the same clip at +12 in Repitch drawn
  half as long.
- Nothing has been heard by a person, and nothing was dragged by hand.

## Limits

- No mode for drums as Ableton's Beats is, which slices at transients and
  plays each slice at its pitch; a drum loop moved by Stretch may smear its
  hits. A line under Ideas in the backlog.
- A pad's transpose and a note still repitch, in either mode: a pad moved at
  its length is a line under Ideas.
- A transpose is one value for the whole clip; it is not automated.
- Past a few semitones at the clip's length a stretcher can be heard; no check
  warns about it yet.
