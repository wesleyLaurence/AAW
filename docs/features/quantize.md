# Quantize — implemented October 31, 2026

The quantize half of the backlog's MIDI keyboard item, asked for by the person
on October 31, 2026 (D169); a keyboard plugged in through CoreMIDI stays on the
backlog. A take from the [typing keyboard](typing-keyboard.md) lands where it
was played, to a thousandth of a beat; this moves it onto the grid afterwards.

## What

- **`daw note quantize SONG NOTE|CLIP... [--grid 1/4] [--strength 100]`**
  moves each note's start onto the nearest line of a grid, all the way or
  `--strength` percent of the way, and leaves its length. The grid is in
  beats, counted from the clip's start: `1/4` is a sixteenth note and the
  default, `1/3` an eighth-note triplet, `1` a beat, `4` a bar of 4/4. A
  clip given stands for all of its notes. The reply's `label` names the
  step, `Quantize 6 notes to 1/4`, with `by 50%` when the strength is under
  100, and `moved` counts the notes that moved; a note already on a line is
  not counted.
- **`note.quantize`** is the host command: `{"op": "note.quantize", "notes":
  [PATH, ...], "grid": "1/4", "strength": 100}`, `grid` and `strength` as
  the command line's defaults when left out. An edit, one undo step,
  whoever made it.
- **Edit › Quantize, ⌘U**, in the app: the selected notes of the clip the
  piano roll shows, or every note of the clip when none is selected, as
  Ableton's ⌘U does, onto the piano roll's grid all the way. Disabled while
  the piano roll shows no clip with notes, or a text is being typed.

## How it works

- **The nearest line** is `floor(at / grid + 1/2) * grid`, in exact
  rationals, so a note exactly halfway between two lines goes later. The
  new position is `at + (line - at) * strength / 100`, written as the song
  writes beats: a whole number, an exact decimal such as `1.9875`, or a
  fraction such as `9/4`. A note before its clip, at a negative `at`, is
  quantized as any other and stays kept and not played while it is negative.
- **From the clip's start, not the song's.** A note's `at` is beats from
  its clip's start, and the piano roll draws its grid lines from there, so
  the lines Quantize snaps to are the ones in view. A clip at an off-grid
  song beat has lines off the song's grid too. A take from the typing
  keyboard starts at the start position, whole bars, so for it the two are
  the same when the start position is on the song's grid.
- **Beats, not note values.** The grid is written as `daw note move --by`
  and the app's grids are, in beats, unlike the arpeggiator's `rate`, which
  is a note value such as `1/16`. `daw describe midi` says so under
  `timing`.
- **In the app**, `Edit::NotesQuantize {notes, grid, strength}` sends one
  `note.quantize` command with the notes' paths; `SongModel.quantizable`
  is what ⌘U acts on, and `SongWindowController.quantizeSelection` the
  menu's action, validated by `canQuantize`. `⌘U` reaches the menu from the
  piano roll because its `keyDown` passes every ⌘ key on; the typing
  keyboard leaves ⌘ keys alone as well.

## Checked

- `aaw-host/tests/midi.rs`: halfway with a strength of 50, the clip
  standing for all of its notes, a triplet note at 7/3 going to 9/4 on a
  sixteenth grid and staying on a triplet grid, lengths kept, a note exactly
  halfway going later, the grid counted from a clip moved off the song's
  grid, and what is refused before anything moves: a strength outside 0 to
  100, a grid of 0, no notes, a path that is not a note.
- `aaw-ffi/tests/song.rs`: one note onto a sixteenth grid, three halfway to
  an eighth grid with their lengths kept, the labels, no notes is no edit,
  a grid of 0 refused.
- `tests/test_midi.py`: `daw note quantize` with `--strength` and `--grid`,
  the reply's label and `moved`, the song read back by Python and by `daw
  note list`.
- A scripted run of the built app, on a clip with notes at 0.05, 1.1, 1.9
  and 3.3 beats: the clip double-clicked, ⌘U with nothing selected and the
  notes read back at 0, 1, 2 and 3.25 on the 1/16 grid; one note clicked
  and ⌘U moving it alone; ⌘U then ⌘Z leaving the four where they were. The
  piano roll's picture after, with the notes on the lines. Nothing heard,
  and nothing pressed by hand.

## Limits

- Starts only: a note's end and length are left alone, so there is no
  quantizing of ends (Ableton's End), no swing and no randomness; a
  humanize with a seed is the backlog's note transformations item.
- The app quantizes all the way; part of the way is the terminal's
  `--strength`.
- A take is quantized after it is recorded, not as it is recorded.
- The grid is the clip's: a clip starting off the song's grid quantizes to
  lines off the song's grid.
