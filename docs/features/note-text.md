# Note text — proposed

The first of the backlog's Ideas lines from the agent's session driving the CLI
on October 31, 2026: pitched music written as the drum rows are, a token a
cell, in place of a JSON list of notes whose every `at` the agent adds up by
hand. Planned with the person on October 31, 2026.

## What

- **`daw note write SONG CLIP TEXT [--at 0] [--grid 1/4] [--replace]`** adds
  the notes TEXT describes to a note clip, from `--at` beats after the clip's
  start, each cell `--grid` beats long (`1/4`, a sixteenth note, unless
  given, as `note quantize` and a pattern's grid). `--replace` first removes
  the clip's notes that start inside the span the text covers, so one bar
  can be rewritten. A text that runs past the clip's end makes the clip long
  enough, and the reply says `resized`. One edit, one undo step; the reply
  lists the paths made, as `note add` does, and `label` is `Write 12 notes
  to clip b`.
- **`daw note list ... --as text [--grid 1/4]`** prints the same notation
  back: `text` for each clip, a row a voice from the highest down, and
  `off_grid`, the refs of notes whose start or length the grid could not
  hold, each rounded to its nearest cell in the text. The agent reads what
  it wrote, and what the person played, as lines of music.
- **`daw clip add SONG TRACK --text TEXT [--grid 1/4]`** makes a note clip
  as long as the text, in whole bars, and writes it, as one step.
- **`note.write`** is the host command, `{"op": "note.write", "clip": PATH,
  "text": "...", "at": 0, "grid": "1/4", "replace": false}`, in `daw batch`
  and `daw describe batch` too.
- **`daw describe midi.text`** is the notation's reference; `midi.commands`
  and `daw describe start` write the bassline as one line of it.

## The notation

```
e4 . g4 a4 . e4 . g4 bb4 a4 . g4 . [e4 d4] . d4@80 | e4 . g4 a4 . e4 . g4 bb4 a4 . g4 . e4 . d4
c2,g2:32
```

- **A row is a voice:** the lines of the text play together from `--at`,
  each read on its own. Most clips are one row; a held bass under a moving
  melody is two. Which row a note is in means nothing once written.
- **A token is a cell** of `--grid` beats, left to right, separated by
  spaces. Four kinds: a pitch, a rest `.`, a hold `-`, a split `[ ]`.
- **A pitch** is a name with its octave, `c2`, `bb1`, `f#3`, `C4` (middle C,
  60), in either case, or a MIDI number, `36`. The octave is always written:
  nothing carries from the note before, so each token is read alone. A note
  starts at its cell and lasts one cell unless held.
- **A hold `-`** makes the previous cell's note, or every note of its chord,
  one cell longer. It may follow `|`, so a pad holds across a bar line. A
  hold after a rest, or first in the text, is refused.
- **A length `:N`** after a pitch makes its note N cells long, `c2:32`,
  whatever the cells after it hold, so a sustain under a melody needs no
  row of dashes. `c2 - - -` and `c2:4 . . .` are the same four beats on a
  `1` grid.
- **A chord** is pitches joined by commas with no spaces, `c2,eb3,g3`: one
  token, one cell, every note at the same start; `:N` after the chord gives
  all its notes the length, `c2,g2:32`, and after one pitch only that one,
  `c2:32,e4`.
- **A split `[ ]`** divides one cell evenly among the tokens inside it:
  `[bb1 c2]` is two thirty-second notes on a sixteenth grid, `[c3 d3 e3]`
  with `--grid 1` a beat of eighth-note triplets. Splits do not nest.
- **Velocity** is `@N` after a pitch or a chord, `c2@70`, `c2,g2@90`, 1 to
  127; 100 unless written, as a note's default. With a length, `c2:4@70`.
- **A bar line `|`** says the next cell begins a bar. It is optional, and
  where written it is checked: the cells since the last bar line must be a
  bar of the song's time signature at this grid, `row 1, bar 2 has 15
  cells; a bar of 4/4 on a 1/4 grid has 16`, so a miscount is refused where
  it is instead of moving every bar after it. `--at` must then be on a bar.
  A row may stop short; what it does not write is rest.
- **A comment** runs from `#` to the end of the row, so `--as text` can lead
  a row with its song beat, `# 16`, and the text paste back.
- **Reserved:** a token with a capital letter and no octave, `Am7`, is not
  a pitch today and is refused by name, kept for chord symbols from the
  backlog's note transformations item.
- **Every error names its place:** `row 2, bar 2, cell 7: `cb9` is not a
  pitch`.

Positions are exact, as the song's are: a cell at index 5 on a `1/4` grid
is `5/4`, a split's thirds are fractions, nothing is rounded on the way in.
Swing, humanizing and off-grid timing are what `note move`, `note quantize
--strength` and the backlog's humanize are for, after the text is written.

## Why

The drum rows prove it: `"x... .... x.x. ...."` has no arithmetic, and a
wrong cell in bar 3 leaves bar 4 where it was. A JSON list has the agent add
up every `at` across the list, the one task it does worst, and an error
moves every note after it where nothing can hear it. The mini-notations the
agent already knows from TidalCycles and the trackers are grids with local
errors; a fixed grid with checked bar lines is the one form whose miscounts
the parser can catch. Reading matters as much: `note list` gives numbers,
and `--as text` lets the agent check what it wrote by eye and read what the
person played as a phrase. The model stays MIDI; the text is a view of it,
as the piano roll is the person's.

The person's piano roll is pitch by time, because a person sees shape: a
contour, a stack, a length, at a glance. A row a pitch would be that in
text, and is what the drum rows are; but an agent reads tokens in order,
not positions, and a melody spread over twelve rows of dots has to be
aligned column by column to be read at all. A row a voice, pitch inside
the token, is what score notation settled on for the same reader: a line
to follow, staves stacked only to show what sounds together. Drums on a
MIDI track, where there is no contour, are a row a pitch already, as
`pattern steps`.

## Design

- **Parser in `aaw-model`**, beside `step_cells`: `note_text::parse(text,
  grid, bar_beats, at) -> Result<Vec<NoteFields>, String>`, pure, with the
  cell index, bar and token in every error. Pitch names through
  `rules::note_name`'s inverse, the one `note add` uses. The printer,
  `note_text::print(notes, grid, bar_beats) -> (String, Vec<ref>)`, is its
  inverse on grid-aligned notes. Voices: the notes sorted by start then
  pitch, highest first, each put in the first row whose last note has ended
  by its start, else a new row, so a held dyad takes one row and the melody
  over it the next; notes of one start and length are a chord token. Rows
  print from the highest first note down. Holds are `-` within a bar and
  `:N` when a note crosses a bar line; `[ ]` when the notes in a cell split
  it evenly; rounding and `off_grid` otherwise.
- **Host op `NoteWrite {clip, text, at, grid, replace}`** in
  `aaw-host/command.rs`: parses with the song's time signature, removes
  under `--replace`, then inserts as `NoteAdd` does and names the notes with
  `name_notes`; resizes the clip when the span passes its end; one label.
- **CLI** in `aaw-cli/main.rs`: `NoteCmd::Write`, `--as text` and `--grid`
  on `NoteCmd::List`, `--text` on `clip add` composed as a labeled batch of
  `clip.add` and `note.write`.
- **Batch** `note.write` in `aaw-ffi/edits.rs`; `describe batch.note` lists
  it.
- **`describe`**: a `text` part under `midi` in `aaw-model/contract.rs` with
  the notation above; `commands` and `timing` updated; the start recipe in
  `recipes.rs` writes its bassline with `note write`, and the recipe test
  runs it as written.
- **The app** is untouched. A text field in the piano roll that shows the
  clip as text and takes it back is a Later line.

## Done when

- `cargo test` in `aaw-model`: every token kind, a chord held across a bar
  line, `:N` on a pitch, a chord and one pitch of a chord, a split,
  velocity, numbers as pitches, a comment, two rows, a row stopping short,
  the bar check in 4/4, 3/4 and 7/8, each refusal with its place, and
  `print(parse(t)) == t` for the notation's own examples: a dyad held under
  a melody reads back as two rows, melody first.
- `aaw-host/tests/midi.rs`: `note.write` at 0 and at a bar, `replace`
  clearing only the span, the clip resized and said so, IDs continuing from
  the clip's, one undo step.
- `aaw-ffi/tests/song.rs`: the batch op and its refusals.
- `tests/test_midi.py`: `daw note write`, `daw note list --as text` giving
  the text back, `off_grid` on a note at 1.975, `daw clip add --text`.
- `daw describe start` makes its bassline in one line and the recipes test
  passes as written.

## Open questions

- A rest is `.` as the drum rows have it, or `~` as TidalCycles has it. `.`,
  one convention in one tool.
- Chords by commas, `c2,eb3,g3`, or by brackets, `[c2 eb3 g3]`, with splits
  then needing another pair. Commas: a chord stays one token, one cell.
- Whether `--as text` prints the song beat of each bar at the line's start,
  `17 | c2 . . .`, for a clip read from the middle of a song. Yes, as a
  comment the parser ignores, so a line can be pasted back.
