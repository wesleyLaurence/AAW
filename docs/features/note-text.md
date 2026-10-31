# Note text — implemented October 31, 2026

Pitched music written as the drum rows are, a token a cell, in place of a
JSON list of notes whose every `at` the agent adds up by hand. The first of
the backlog's Ideas lines from the agent's session driving the CLI on
October 31, 2026, planned with the person that day (D171).

## What

- **`daw note write SONG CLIP TEXT [--at 0] [--grid 1/4] [--replace]`** adds
  the notes TEXT describes to a note clip, from `--at` beats after the clip's
  start, each cell `--grid` beats long (`1/4`, a sixteenth note, unless
  given, as `note quantize` and a pattern's grid). `--replace` first removes
  the clip's notes that start inside the span the text covers, so one bar
  can be rewritten, and the reply counts them as `removed`. A text that runs
  past the clip's end makes the clip long enough, and the reply says
  `resized` with the new length. One edit, one undo step; the reply lists
  the paths made, as `note add` does, and `label` is `Write 12 notes to
  clip b at 0`.
- **`daw note list ... --as text [--grid 1/4]`** prints the same notation
  back: `text` for each clip, a row a voice from the highest down, `grid`,
  and `off_grid`, the refs of notes whose start or length the grid could not
  hold, each written at its nearest cell. The agent reads what it wrote, and
  what the person played, as lines of music.
- **`daw clip add SONG TRACK --text TEXT [--grid 1/4]`** makes a note clip
  as long as the text, in whole bars unless `--length-beats` is given, and
  writes it, as one step; the reply counts the `notes`. The clip's other
  fields come along, so `--text "d1 . d1 a1 . c2" --grid 1/2 --loop-beats 3
  --length-beats 32` is a three-beat figure against the bar in one
  command.
- **`note.write`** is the host command, `{"op": "note.write", "clip": PATH,
  "text": "...", "at": 0, "grid": "1/4", "replace": false}`, in `daw batch`
  and `daw describe batch` too; `clip.add` takes `text` and `grid`, and
  `notes` takes `text: true` and `grid`.
- **`daw describe midi.text`** is the notation's reference; `midi.commands`
  names the commands, and `daw describe start` writes its bassline as one
  line of it.

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
  starts at its cell and lasts one cell unless held or given a length.
- **A hold `-`** makes the previous cell's note, or every note of its chord,
  one cell longer; inside a split, the split's share. It may follow `|`, so
  a pad holds across a bar line. A hold after a rest, or first in the text,
  is refused.
- **A length `:N`** after a pitch makes its note N cells long, `c2:32`,
  whatever the cells after it hold, so a sustain under a melody needs no
  row of dashes. `c2 - - -` and `c2:4 . . .` are the same four beats on a
  `1` grid.
- **A chord** is pitches joined by commas with no spaces, `c2,eb3,g3`: one
  token, one cell, every note at the same start; `:N` on the last pitch is
  the chord's, for the pitches without one of their own, `c2,g2:32`, and on
  an earlier pitch its own, `c2:32,e4`.
- **A split `[ ]`** divides one cell evenly among the tokens inside it:
  `[bb1 c2]` is two thirty-second notes on a sixteenth grid, `[c3 d3 e3]`
  with `--grid 1` a beat of eighth-note triplets. Splits do not nest, and
  hold no bar line.
- **Velocity** is `@N` after a pitch or a chord, `c2@70`, `c2,g2@90`, 1 to
  127; 100 unless written, as a note's default. With a length, `c2:4@70`.
- **A bar line `|`** says the next cell begins a bar. It is optional, and
  where written it is checked: the cells since the last bar line must be a
  bar of the song's time signature at this grid, `row 1, bar 2 has 15
  cells; a bar of 4 beats on a 1/4 grid has 16`, so a miscount is refused
  where it is instead of moving every bar after it. With bar lines, `--at`
  is whole bars from the clip's start, and the bar must be whole cells of
  the grid (a bar of 7/8 is 14 cells of `1/4`, and not whole cells of `1`).
  A row may stop short; what it does not write is rest. A `|` with no cell
  since the last is ignored, so a text may begin or end with one.
- **A comment** runs from a `#` where a token starts to the end of the row;
  the `#` of `a#4` is a sharp.
- **Reserved:** a token with a capital A to G and a quality or no octave,
  `Am7`, `Cmaj7`, `G`, is refused by name and kept for chord symbols from
  the backlog's note transformations item. `A7` with its octave is the
  pitch.
- **Every error names its place:** `row 2, bar 2, cell 7: `h2` is not a
  pitch: a name with its octave, c2 or F#3, or a MIDI number`.

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

## How it works

- **Parser and printer in `aaw-model`**, `note_text.rs`, beside
  `step_cells`: `parse(text, grid, bar, at)` is pure and gives `Written`
  notes in beats from the clip's start and the `span` the text covers, the
  longest row with rests counted or the last note's end where a length runs
  past it; every error carries its row, bar and cell. Pitch names go
  through `rules::midi`, the one `note add` uses. `print(notes, grid, bar)`
  is its inverse on grid-aligned notes: each note at its nearest cell,
  halfway later, its length in cells, one at least, and counted `off_grid`
  when either moved; notes of one start, length and velocity are a chord
  token; tokens by start then the highest first, each put in the first row
  whose last note has ended by its start, else a new row, so a held dyad
  takes one row and the melody over it the next; rows from the highest
  pitch down. Holds are `-` within a bar and `:N` when a note crosses a bar
  line; a row stops at its last token, trailing rests left off; a note
  before the clip is left out and counted. The notation's own examples
  print back as written.
- **Host op `NoteWrite {clip, text, at, grid, replace}`** in
  `aaw-host/command.rs`: parses with the song's time signature, removes
  under `replace` from the last index, then inserts as `NoteAdd` does and
  names the notes with `name_notes`; sets `length_beats` when the span
  passes the clip's end; one label. `ClipAdd` on a MIDI track takes `text`
  and `grid` out of its fields, sets the length to whole bars unless given,
  and puts the notes in. `Session.notes` prints the kept notes of each clip
  with `text`.
- **CLI** in `aaw-cli/main.rs`: `NoteCmd::Write`, `--as text` and `--grid`
  on `NoteCmd::List`, `--text` and `--grid` in `clip add --help`.
- **`describe`**: the `text` part under `midi` in `aaw-model/contract.rs`;
  `commands` names the three; the start recipe in `recipes.rs` writes its
  bassline with `note write` on a grid of a beat, and the recipes test runs
  it as written.
- **The app** is untouched.

## Checked

- `aaw-model/src/note_text.rs`: every token kind, a chord held across a bar
  line, `:N` on a pitch, a chord and one pitch of a chord, a hold inside
  and after a split, two rows and a row stopping short, the span, bar
  lines in 4/4, 3/4 and 7/8 with a miscount refused where it is and `--at`
  off a bar refused, each refusal with its place, the notation's examples
  printing back as written, a dyad held under a melody reading back as two
  rows melody first, and off-grid notes at their nearest cell and counted.
- `aaw-host/tests/midi.rs`: `clip.add` with a text of two rows at a bar,
  `notes` with text giving it back, `note.write` at a bar with `replace`
  taking only the notes in its span and the clip resized, the default grid
  and an off-grid note named by its ref, one undo step, and what is refused
  before anything is written.
- `tests/test_midi.py`: `daw clip add --text`, `daw note list --as text`
  on two grids with `off_grid`, `daw note write --at --replace` with
  `removed` and `resized`, a miscount refused with nothing written, and
  `--as` taking only `text`.
- `tests/test_recipes.py`: `daw describe start` runs as written with its
  bassline in one line. `daw describe midi` grew by the text part, from
  12991 bytes to 15533, so `test_discovery.py`'s budget for it is 16000
  bytes, from 13000.
- Tried on a scratch song from the terminal: written, read back, rewritten
  in place, a batch of `clip.add` with a text and `note.write`, and
  `daw map` showing the clips. Then two sketches in `projects/`, each
  rendered and exported: sixteen bars in A minor with chords, bass, melody,
  kick and hat as text and the snare as drum steps; and twenty-four bars of
  modal jazz, quartal stacks in a tresillo, 3-, 5- and 7-beat loops against
  4/4, a triplet ride and quintuplet and septuplet runs. Every part was one
  command, no `at` computed by hand; `daw map` matched each text's span and
  `daw check` caught the cut tails and a pad in the bass's octave. The bar
  check never fired: no miscount that day. Nothing heard.

## Limits

- The printer writes no split: a note between the cells of the grid is
  `off_grid` and at its nearest cell; a finer `--grid` holds it, and a
  septuplet read on a triplet grid comes back as neighbours glued into
  chords, named off the grid. A split-aware printer is an Ideas line.
- Chord symbols are refused by name; writing `Am7` as its notes is the
  backlog's note transformations item.
- A `#` where a token starts is a comment, so a MIDI number cannot follow
  one on the same row.
- The piano roll has no text field; a clip shown and taken as text in the
  app is a Later line.
