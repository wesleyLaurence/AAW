# Names with spaces — implemented November 22, 2026

Built from the person's note of November 22, 2026 (D194): "it seems that I
cant name a track something with a space in it? i had to use a - instad of a
space to make it work. But I should be able to name a track with a space in
it." Decided in D199.

## What

A track, a group or a return is named anything in the app: "Lead Vox",
"808 (sub)", "Drums – Room". The header shows the name as typed. The agent
still addresses the row by its short ID, sees both, and may use either.

## Why

A row's ID is letters, digits, `-` and `_` starting with a letter
(`aaw-model/src/schema.rs`), because sends, groups, inputs, lanes,
sidechains and every `daw` command name a row by it. Rename in the app used
to send `track.rename`, which changed the ID, so a space was refused.

## How it works

- **A `name` beside the `id`.** `tracks[]`, `groups[]` and `returns[]` take an
  optional `name`: one line of text, at most 64 characters, with something
  besides spaces and no space at either end; tabs and other control
  characters are refused. A name of digits alone, which `daw set` and YAML
  written by hand hand over as a number, is taken as its text. The song
  writes it after the `id`, quoted where YAML needs it. Two rows may share a
  name.
- **The app.** A header, the send strips, the `@` list, the device panel,
  the piano roll's title, the Move to Group ▸ menu, a compressor's sidechain
  menu, an input's label and a send's lane ("Send Big Hall") show the name,
  or the ID where there is none. `TrackView`, `GroupView` and `ReturnView`
  carry it as `name`. Rename (⌘R) sets the name and leaves the ID, so the
  song's references do not move under the agent; the text is trimmed, and
  cleared, or typed as the ID, it takes the name away. Its undo step is
  "Rename to NAME". Rows the app makes keep their free IDs (`track-1`,
  `midi-2`, `return-1`).
- **The agent.** `daw inspect` gives a row's `name` after its `id` where it
  has one, and `daw map` writes `vox "Lead Vox": …` in its line a track.
  What the app tells the agent of the selection names a row `vox ("Lead
  Vox")`; an `@` mention's reference stays `tracks.vox`.
- **By name.** A path segment under `tracks`, `groups` or `returns` that is no
  ID, handle or index is looked up by name, the longest run of the path's
  segments first, so `tracks.Vox 1.2.gain_db` reaches a track named "Vox
  1.2". So every command that takes a row (`track`, `group`, `return`, a
  send's `to`, `track.move`'s group) takes its name, and a `group` among
  `track.add`'s and `group.add`'s fields is stored as the group's ID.
  `daw map --tracks`, `daw render --track`, `daw audition` and freezing find
  a row the same way (`Project::find_track`, `find_row`). An ID comes before
  a name; a name two rows share is refused there with their IDs.
- **Setting it.** `daw set PROJECT tracks.ID.name "Lead Vox"`, `daw remove
  PROJECT tracks.ID.name`, and `daw track add PROJECT ID --name TEXT` (groups
  and returns alike). `daw track rename`, `group rename` and `return rename`
  still change the ID and what refers to it.
- **Clips** keep their names as they are; this is the rows only.

## Limits

- A value that names a row, as a `sidechain`, an `input` or a `group` set
  with `daw set`, is stored as written and must be the ID.
- A clip reference that `daw inspect` hands out, such as
  `tracks.vox.audio.0`, uses the ID; `daw transcribe` and `daw separate` take
  those as given.

## Decided

- A rename in the app never moves the ID, even of a row nothing refers to
  yet (the open question of the proposal): the agent's references hold
  through every rename, and a new row's free ID is enough to address it.
