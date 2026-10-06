# The piano roll for writing by hand (implemented 2026-10-05)

Status: built in the pull request that closed backlog item 1, decision D79.
[MIDI tracks and note clips](midi-clips.md) describes the piano roll as it was
first built: all 128 notes, notes added, selected, moved, stretched, typed and
removed. This file is what was added for writing parts by hand, as in Ableton.

## What

- **Notes are heard.** A note plays through the track's instrument, a Synth or
  a Sampler, and its chain when it is drawn, clicked, moved to another pitch by
  a drag or an arrow key, and when its key at the left is pressed or dragged
  across. A preview lasts the note's length, from an eighth of a beat to a
  beat, at its velocity. The headphones beside the grid turn it off. A track
  with no instrument plays nothing.
- **A velocity lane.** Under the notes, each note's velocity is a stalk at its
  start. Dragging one selects its note, as a click on the note does, and moves
  every selected note's velocity by as much, each within 1 to 127, as one
  undo step; the values show while it is dragged. The lane is 56 points and
  shows when the panel is at least 150 points tall.
- **Zoom up and down.** Option-scroll makes the rows taller or shorter around
  the pointer, from 5 to 28 points, or as short as all 128 notes in view. A
  clip opens at 10. Scroll, pinch and Command-scroll work across as before.
- **A note's start.** The left end of a note, as its right end does, moves the
  start of the selected notes to a line of the grid, or with ⌘ anywhere, and
  leaves their ends. A note narrower than three grips has its end only.
- **Option copies.** A note dragged with Option is copied where it is let go,
  and the notes stay; the copies are drawn outlined on the way and selected
  after. Option is read while the drag goes on, as in the Finder.
- **⌘ is off the grid**, across the app: the piano roll, the pattern editor,
  the timeline, automation points and files dropped on the timeline. It was
  Option until Option became copy.
- **Note values.** The piano roll's grid and the pattern's Step menu name
  their steps as note values, a beat being a quarter note: 1 Bar, 1/2, 1/4,
  1/8, 1/8T, 1/16, 1/16T, 1/32, 1/32T and 1/64. The song still writes beats:
  a grid of `1/4` beats is 1/16. A pattern step the menu does not list keeps
  its beats.

## How

- `note.preview` plays through a Sampler too. The host builds the voice with
  `aaw_engine::program::preview_voice`, as the same note in a clip would play,
  and hands it to the player, which plays up to 16 such notes in slots,
  across the tracks, the oldest giving its place. A replaced voice goes back
  to the host to be freed, as a replaced program does, so the audio thread
  still neither allocates nor frees. Previews carry on through an edit, and
  through a change of structure on a track that is still there. A pitch the
  map leaves silent is silent here, and the reply's `sounds` is false.
- Three edits through the FFI: `NotesStart` (the start of the grabbed note to
  a beat, the others as far, ends kept), `NotesCopy` (copies as far as
  `NotesMove` would move the notes, within one clip, inside MIDI notes 0 to
  127) and `NotesVelocity` (velocities moved together). Each is one undo step
  of `note.set` or `note.add` commands, as an agent's would be.
- `PianoRollLayout` holds the row height, the velocity lane's geometry, the
  start grip and the names of note values; `NoteEditor` draws and edits.
- Scripted runs take `--opt-drag` and `--cmd-drag`.

## Limits

- A preview has a fixed length; a note held while the mouse is down, as a
  MIDI keyboard's is, waits for the MIDI keyboard on the backlog.
- Only the grabbed note is heard while a chord is moved.
- The velocity lane's height is fixed, and velocity is not drawn by sweeping
  across stalks.
- Option-drag copied notes only until [selecting several things](selection.md)
  made it copy clips and pattern events too.
