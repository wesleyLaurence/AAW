# Listening markers (implemented 2026-10-09)

Status: built, decision D105. The concept's line
([concept.md](../concept.md#working-together)): during playback a key drops a
note at the playhead, "too busy", "love this", and the agent gets feedback tied
to beats. D98 put it after the measurements of a session's sound, since it
carries the person's judgment of what no number settles.

## What

A marker is a beat and a few words. The person leaves one while the song
plays, without stopping it; the agent reads them with the bar each is in, the
section over it and the clips sounding there, so "this bar" needs no timecode.

```yaml
markers:
- {id: m1, at: 34.417, text: too busy}
- {id: m2, at: 64}
```

| Field | Takes |
|---|---|
| `id` | Unique among the markers. Left out, the next free `mN` |
| `at` | The song beat, from 0 to the song's end |
| `text` | What the person said of that place, up to 200 characters; empty unless given |

A marker plays nothing and is in no render. It is part of the song: saved in
`song.yaml`, one undo step like any edit, in the change log with who left it,
and in the song's fingerprint.

## In the app

The ruler has a strip for markers, between the sections and the bar numbers.
A marker is a flag on a line at its beat, with its text in the flag as far as
the next marker leaves room, and a faint line down the lanes under it.

| Input | Does |
|---|---|
| M, or Transport › Add Marker | Leaves a marker where the song is heard, to a thousandth of a beat; stopped, at the start position. The song plays on and the selection stays |
| ⇧M, or Transport › Add Marker and Name It | The same, and opens a field at the flag to type what it says: Return or a click elsewhere keeps it, Escape leaves the marker without text. The marker is where the playhead was when the key went down, however long the typing takes |
| Double-click in the clear of the strip | Leaves a marker there, on the grid or with ⌘ off it, and asks what it says |
| Click a marker | Selects it, in place of the clips, rows and points, and sets the start position to it. `daw status` lists it in the selection |
| Drag a marker | Moves it to the grid line under the pointer, or with ⌘ anywhere, one undo step |
| Double-click a marker, or ⌘R with one selected | Types what it says |
| Delete, with one selected | Removes it |
| Right-click or Control-click in the strip | On a marker: Rename and Delete. In the clear: Add Marker Here. Delete All Markers either way; Transport has it too |

A marker the agent left or changed lights up in the agent's color, as a clip
does. The keys work with the arrangement, the pattern editor or the piano roll
holding them, and not while a name or a number is typed.

## For the agent

| Command | Does |
|---|---|
| `daw marker list PROJECT` | The markers in time order, each with `id`, `at`, `bar` and `beat` (the bar it is in and the beat of that bar, from 1, as the time signature counts), `section` (the shortest section over it, or null), `text`, and `playing`: the clips sounding there, a track each, as `track`, `clip` (a reference `daw get` and the clip commands take) and `what` (what `daw map`'s legend says of the clip), with `muted` on a muted track |
| `daw marker add PROJECT [AT] [--text TEXT] [--id ID]` | Leaves one. Without AT, where a running host is playing, or at its start position while it is stopped; without a host AT is required |
| `daw marker move PROJECT MARKER AT`, `daw marker text PROJECT MARKER TEXT` | Moves one; changes what it says, `""` clearing it |
| `daw marker remove PROJECT MARKER... \| --all` | Removes those named, or every one |

MARKER is an ID, a handle or `markers.ID`; `daw set PROJECT markers.m1.text …`
and `daw get PROJECT markers` reach markers as they reach anything.

- **`daw map`** has a row `markers` under the sections, `!` in a cell that
  holds one and the count where it holds several, and a line for each under
  the tracks: `!  m1 at bar 9, beat 3.42: too busy`.
- **`daw inspect`** lists them as the song holds them.
- **`daw changes`** has each one the person left, as any edit: `Add marker m1
  in bar 9: too busy`, origin `user`.
- **News in the next edit's reply.** A host keeps the markers the person left
  or reworded since the agent last sent anything, and the agent's next edit
  carries them as `new_markers`: `The person left a marker since your last
  command: m3 in bar 9, beat 3.42: too busy. daw marker list reads it with
  what plays there.` Once; `daw marker list` by the agent counts as being
  told. A marker the person took back, a move, and the agent's own markers
  are not news.

How to answer one is in `daw describe project` under `markers` and in
[music.md](../music.md#edit-together): the bar a marker is in and what led
into it, since a marker dropped while the song played is a moment after what
was heard; the music changed, or a reason why not; the marker removed in the
same batch as the edit that answers it, so one undo brings both back; a marker
that asks for nothing, `love this`, left where it is and what it marks kept.

## Ranges

A marker keeps to the beats it marks when the whole song's time changes:
`range insert` and a copy with `--insert` move those at or after the place
later; `range delete` and `section remove --with-content` remove those in the
deleted beats, listed under `removed`, and move the later ones earlier;
`section move --with-content` takes those under the section along; `section
duplicate` pushes the later ones on. A copy over what is there, `range clear`
and a range of some tracks (`--track`) leave them.

## How

- `aaw-model`: `Marker` and `Project.markers`, validated as the table says,
  IDs given as note clips' are (`mN`). An empty list is not written and
  `hash::legacy_fields` drops it, so a song without markers saves and
  fingerprints as it did.
- `aaw-host`: `marker.add`, `marker.move`, `marker.text` and `marker.remove`
  are edits (`command.rs`), the list kept in time order; `markers` is a read
  (`Session::markers`, with `map::sounding` for what plays). `range.rs` moves
  them. A host fills in a `marker.add` without `at` from its clock, the beat
  being heard, and keeps the handles of the person's new and reworded markers
  for `new_markers` (`host::marker_news`); handles and not IDs, since an ID is
  given again once its marker is gone.
- `aaw-ffi`: `Arrangement.markers`, `Part::Marker` in what a change touched,
  and the edits `MarkerAdd`, `MarkerMove`, `MarkerText` and `MarkersRemove`.
  The app reads the playhead when the key goes down and sends the beat.
- The app: `TimelineLayout` has the strip and what a press takes hold of
  (`markerSpans`, `marker(atX:)`), `ArrangementView` draws and edits them,
  `SongModel` holds the selected marker.

## Limits

- A marker is a point, not a range of beats, and has no color or kind. It
  says who left it only in the change log.
- There is no "done": the agent removes a marker it answered and leaves the
  rest. A marker removed with its edit comes back with one undo.
- The marker is where the playhead was when the key went down. The host does
  not move it earlier for the time it takes to react; the guidance tells the
  agent to read what led into it.
- A marker changes the song's fingerprint, so `daw export` renders again
  after one is left, and an edit with `--expect` of the fingerprint before it
  is refused as after any change.
- A song cannot be made shorter than its last marker by setting
  `session.length_beats`: move or remove the marker, or use `daw range
  delete`, which takes it along.
- `new_markers` reaches an agent through its next edit only; an agent that is
  rendering or measuring hears of a marker when it next edits or reads them.
- Markers are not copied or pasted, and several are not selected or moved
  together.
- Whether a person finds M under their hand while listening, and whether an
  agent reads the markers and answers them well, is under Verify.
