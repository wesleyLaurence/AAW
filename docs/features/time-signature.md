# The time signature (implemented 2026-10-07)

Status: built in the pull request that closed the first of the person's notes
of October 6, 2026, decision D88. Before it the model accepted `4/4` alone,
and the app, the map, the checks, the metronome and the beat map each counted
four.

## What

- **One time signature for the song**, `session.time_signature`: `N/D`, 1 to
  32 beats over 1, 2, 4, 8 or 16, `4/4` unless set. The agent sets it with
  `daw set PROJECT session.time_signature 3/4`, and `daw init --time-signature
  3/4 --bars 8` makes a song of eight such bars. A value the model does not
  read is refused with the form it wants, and `meter` or `signature` as a
  field name is answered with the field.
- **Positions stay quarter-note beats.** Every `at`, `duration` and
  `length_beats` is as it was, whatever the meter; a bar of 3/4 is 3 beats
  long, a bar of 6/8 3 and a bar of 7/8 3.5. A song in 4/4 saves byte for byte
  as before.
- **In the transport bar**, the meter is a field after the tempo, typed as
  `3/4` or `6/8`, applied by Return or leaving it and dropped by Escape, an
  edit that saves and undoes as the tempo's does. The position reads
  bar.beat.sixteenth with the beat the meter counts: in 6/8 the second number
  runs to 6, and a sixteenth within an eighth is 1 or 2. The bar count after
  it is in the meter's bars.
- **The bars, grid and rulers follow it.** Bar lines and numbers fall every
  bar of the meter; the brighter beat lines on the note it counts, the eighth
  in x/8. The grid that follows the zoom steps through a quarter of that
  note, half of it, the note and then bars. The Grid menu's `1 Bar` is the
  song's bar, 3 beats in 3/4, `7/2` in 7/8, and a size that does not fall on
  the bars draws the zoom's lines while it still snaps to its own, as a
  triplet grid did. The piano roll's and the pattern's lines and rulers count
  the same way.
- **The metronome** clicks on the note the meter counts, an eighth in 6/8,
  and accents the first of each bar's beats: three in 3/4, seven in 7/8.
- **`daw map`** has a cell a bar of the meter, `--per 2` two such bars, and
  places `bar.beat.sixteenth` by it. `daw check`'s `register-crowded` counts
  bars by it. `daw timeline` gives a place's `bar` and `beat` by it.
- **MIDI files.** An exported clip's file carries the song's time signature,
  and an imported part fills whole bars of the song's meter, the song growing
  to the end of such a bar.
- **The beat map** of an audio file takes `--meter 3/4` on `daw samples
  beats`: the numerator is how many of the map's beats make a bar, the
  downbeat is chosen among that many places, bars and phrases are counted by
  it, and the choice is kept with the map as `--bpm` and `--downbeat` are. The
  map's beat is the pulse it finds, so a 6/8 song whose pulse is found as the
  dotted quarter is counted with `--meter 2/4`; a session's `time_signature`
  is not read into it, since a song's loop and the song can differ.

## How

- `aaw_model::Meter` (`engine/crates/aaw-model/src/meter.rs`) reads `N/D`,
  gives the bar and the counted beat as exact and as floats, and writes a
  beat as a place; `Session::meter()` reads the field. The schema holds the
  field to a pattern, and `daw describe project` says `N/D such as 4/4, 3/4
  or 6/8`.
- The engine's `Program` carries `click_beats` and `beats_per_bar` for the
  metronome, read from the session when the song is compiled, so a change of
  meter is heard as a change of tempo is.
- The FFI's `Arrangement` has `time_signature`, `bar_beats` and `beat_unit`
  in place of `beats_per_bar`, and `Edit::TimeSignature { text }` sets the
  field; `midi_file_beats` takes the song's meter for a dropped file's
  outline. In Swift, `TimelineLayout.beatUnit`, `PianoRollLayout` and
  `PatternLayout` carry the meter, `Grid.list(bar:)` puts the song's bar in
  the grid list, and `MeterField` in the transport bar sends the edit.
- `agent_daw.model.meter` gives Python the bar and the counted beat;
  `beats.measure` takes `meter`, counting with `per_bar` where it counted 4.

## Limits

- One meter for the whole song; a change of meter within it is in Later with
  tempo changes.
- The beat map's meter is a count of its beats, not a measurement of them: a
  song in three has its downbeat scored among three places, but the map
  cannot tell three from four on its own.
- A pattern's `length_beats` is still 4 unless given, so a new pattern in 3/4
  is a bar and a beat long until it is set.
- A grid chosen as `1 Bar` holds its beats when the meter changes, and is
  then named by its beats until chosen again.
